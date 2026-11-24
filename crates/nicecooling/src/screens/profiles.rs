//! The Profiles screen (`docs/ui/screens/profiles.md`): the list of Profiles, the options of the
//! switching, the Leakshield slot of one Profile and the Rule groups with their editor.

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{
    AppState, Control, FieldValue, Link, ProfileContent, ProfileInfo, Rule, RuleGroup,
};
use crate::screens::format::{clock_minutes, plain, value_text};
use crate::screens::leakshield::fields::humanize;
use crate::screens::leakshield::refused_text;
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};
use crate::shell::leakshield_label;

pub const SLOTS: [u8; 4] = [1, 2, 3, 4];

/// The conditions that the editor offers. There is no hotkey: Wayland has no global shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    Sensor,
    Process,
    Time,
    Interval,
}

pub const RULE_KINDS: [RuleKind; 4] = [
    RuleKind::Sensor,
    RuleKind::Process,
    RuleKind::Time,
    RuleKind::Interval,
];

impl RuleKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Sensor => "sensor",
            Self::Process => "process",
            Self::Time => "time",
            Self::Interval => "interval",
        }
    }

    pub fn of(rule: &Rule) -> Self {
        match rule {
            Rule::Sensor { .. } => Self::Sensor,
            Rule::Process { .. } => Self::Process,
            Rule::Time { .. } => Self::Time,
            Rule::Interval { .. } => Self::Interval,
        }
    }

    /// A rule with the defaults of its kind.
    pub fn fresh(self, state: &AppState) -> Rule {
        match self {
            Self::Sensor => Rule::Sensor {
                sensor: state
                    .snapshot
                    .loops
                    .first()
                    .and_then(|l| l.water_temperature.clone())
                    .or_else(|| {
                        state
                            .snapshot
                            .sensors
                            .values()
                            .find(|s| s.kind == "temperature")
                            .map(|s| s.id.clone())
                    })
                    .unwrap_or_default(),
                comparison: "gt".to_owned(),
                value: 38.0,
                value_max: 0.0,
                hold_s: 60,
            },
            Self::Process => Rule::Process {
                names: Vec::new(),
                match_kind: "any".to_owned(),
                match_on: "name".to_owned(),
            },
            Self::Time => Rule::Time {
                days: vec![1, 2, 3, 4, 5, 6, 7],
                start: 22 * 60,
                end: 7 * 60,
            },
            Self::Interval => Rule::Interval {
                interval_s: 600,
                delay_s: 60,
                duration_s: 120,
            },
        }
    }
}

/// The comparisons of a Sensor rule, in the order of the select.
pub const COMPARISONS: [&str; 7] = ["lt", "le", "eq", "ge", "gt", "ne", "in_range"];
pub const PROCESS_MATCHES: [&str; 3] = ["any", "all", "none"];

/// The API keys that the editor shows for a kind of rule, in order (`docs/api.md`). `value_max`
/// shows only for `in_range`.
pub fn rule_keys(rule: &Rule) -> Vec<&'static str> {
    match rule {
        Rule::Sensor { comparison, .. } => {
            let mut keys = vec!["sensor", "comparison", "value"];
            if comparison == "in_range" {
                keys.push("value_max");
            }
            keys.push("hold_s");
            keys
        }
        Rule::Process { .. } => vec!["names", "match", "match_on"],
        Rule::Time { .. } => vec!["days", "start", "end"],
        Rule::Interval { .. } => vec!["interval_s", "delay_s", "duration_s"],
    }
}

/// The value of a text field of the rule editor. A wrong text keeps the old value.
pub fn set_rule_field(rule: &mut Rule, key: &str, text: &str) {
    let number = |text: &str| text.trim().replace(',', ".").parse::<f64>().ok();
    let seconds = |text: &str| number(text).filter(|v| *v >= 0.0).map(|v| v.round() as u32);
    let minutes = |text: &str| {
        let (hours, minutes) = text.trim().split_once(':')?;
        let total = hours.parse::<u16>().ok()? * 60 + minutes.parse::<u16>().ok()?;
        (total < 24 * 60).then_some(total)
    };
    match (rule, key) {
        (Rule::Sensor { value, .. }, "value") => {
            if let Some(n) = number(text) {
                *value = n;
            }
        }
        (Rule::Sensor { value_max, .. }, "value_max") => {
            if let Some(n) = number(text) {
                *value_max = n;
            }
        }
        (Rule::Sensor { hold_s, .. }, "hold_s") => {
            if let Some(n) = seconds(text) {
                *hold_s = n;
            }
        }
        (Rule::Process { names, .. }, "names") => {
            *names = text
                .split(',')
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .collect();
        }
        (Rule::Time { start, .. }, "start") => {
            if let Some(m) = minutes(text) {
                *start = m;
            }
        }
        (Rule::Time { end, .. }, "end") => {
            if let Some(m) = minutes(text) {
                *end = m;
            }
        }
        (Rule::Interval { interval_s, .. }, "interval_s") => {
            if let Some(n) = seconds(text).filter(|n| *n >= 1) {
                *interval_s = n;
            }
        }
        (Rule::Interval { delay_s, .. }, "delay_s") => {
            if let Some(n) = seconds(text) {
                *delay_s = n;
            }
        }
        (Rule::Interval { duration_s, .. }, "duration_s") => {
            if let Some(n) = seconds(text).filter(|n| *n >= 1) {
                *duration_s = n;
            }
        }
        _ => {}
    }
}

/// The editor of one Rule group, before the user saves it.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupEditor {
    /// The position in the list, or `None` for a new group.
    pub index: Option<usize>,
    pub group: RuleGroup,
    /// The rule that the editor changes: its position (`None`: a new rule) and the draft.
    pub rule: Option<(Option<usize>, Rule)>,
}

#[derive(Clone, Debug, Default)]
pub struct ProfilesUi {
    pub selected: Option<String>,
    /// The rename popover: the Profile and the text of its input.
    pub rename: Option<(String, String)>,
    pub menu_open: bool,
    pub delete_dialog: bool,
    /// The slot that waits for "Change slot".
    pub slot_dialog: Option<u8>,
    /// The diff table before an activation is open for this Profile.
    pub diff_dialog: Option<String>,
    /// The warning of Immediate switching is open. The flag is "Do not show this again".
    pub immediate_dialog: Option<bool>,
    pub editor: Option<GroupEditor>,
    /// The name of a Profile that "Duplicate" asked for: it opens its rename popover.
    pub opening: Option<String>,
}

impl ProfilesUi {
    /// The Profile that the detail column shows: the selected one, or the active one.
    pub fn current<'a>(&self, state: &'a AppState) -> Option<&'a ProfileInfo> {
        let profiles = &state.snapshot.profiles;
        self.selected
            .as_ref()
            .and_then(|id| profiles.iter().find(|p| &p.id == id))
            .or(state.snapshot.active_profile.as_ref())
            .or_else(|| profiles.first())
    }

    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
        self.rename = None;
        self.menu_open = false;
        self.delete_dialog = false;
        self.slot_dialog = None;
        self.diff_dialog = None;
    }

    /// Open the rename popover of a Profile that "Duplicate" made, when the daemon lists it.
    pub fn sync(&mut self, state: &AppState) {
        let Some(name) = &self.opening else { return };
        if let Some(found) = state.snapshot.profiles.iter().find(|p| &p.name == name) {
            let (id, name) = (found.id.clone(), found.name.clone());
            self.select(&id);
            self.rename = Some((id, name));
            self.opening = None;
        }
    }

    /// "New": a Profile with the slot of the active Profile and a free name.
    pub fn new_profile(&self, input: &ScreenInput<'_>) -> Command {
        let profiles = &input.state.snapshot.profiles;
        let mut number = profiles.len() + 1;
        let name = loop {
            let name = input
                .locale
                .fmt("profiles.new_name", &[("n", &number.to_string())]);
            if !profiles.iter().any(|p| p.name == name) {
                break name;
            }
            number += 1;
        };
        Command::CreateProfile {
            name,
            copy_from: String::new(),
        }
    }

    /// "Duplicate": "Quiet copy", with the same slot.
    pub fn duplicate(&mut self, input: &ScreenInput<'_>) -> Option<Command> {
        let source = self.current(input.state)?;
        let name = input
            .locale
            .fmt("profiles.copy_name", &[("name", &source.name)]);
        let command = Command::CreateProfile {
            name: name.clone(),
            copy_from: source.id.clone(),
        };
        self.opening = Some(name);
        self.menu_open = false;
        Some(command)
    }

    pub fn open_rename(&mut self, state: &AppState) {
        if let Some(profile) = self.current(state) {
            self.rename = Some((profile.id.clone(), profile.name.clone()));
        }
        self.menu_open = false;
    }

    pub fn rename_command(&mut self) -> Option<Command> {
        let (profile, name) = self.rename.take()?;
        let name = name.trim().to_owned();
        (!name.is_empty()).then_some(Command::SetProfileName { profile, name })
    }

    pub fn activate(&self, state: &AppState) -> Option<Command> {
        Some(Command::ActivateProfile {
            id: self.current(state)?.id.clone(),
        })
    }

    /// "Activate manually". A Profile whose slot differs from `ActiveSlot` shows the diff table
    /// first, when the stored slot and the device differ.
    pub fn request_activation(&mut self, state: &AppState) -> Option<Command> {
        let profile = self.current(state)?;
        if !slot_diff(state, profile.slot).is_empty() {
            self.diff_dialog = Some(profile.id.clone());
            return None;
        }
        self.activate(state)
    }

    /// "Activate" in the diff dialog.
    pub fn confirm_activation(&mut self) -> Option<Command> {
        let id = self.diff_dialog.take()?;
        Some(Command::ActivateProfile { id })
    }

    /// "Apply to all slots": for each Leakshield, the alarm thresholds of this Profile go to every
    /// other slot that has stored settings. The slot that the device runs gets `SetFields`, the
    /// others `SetStoredFields`.
    pub fn apply_to_all_slots(&self, state: &AppState) -> Vec<Command> {
        let Some(profile) = self.current(state) else {
            return Vec::new();
        };
        let Some(content) = state.snapshot.profile_contents.get(&profile.id) else {
            return Vec::new();
        };
        content
            .leakshield
            .iter()
            .filter_map(|(device, fields)| {
                Some(slot_commands(
                    state,
                    device,
                    profile.slot,
                    &threshold_values(fields)?,
                ))
            })
            .flatten()
            .collect()
    }

    /// The slot select. The change of the active Profile asks first.
    pub fn request_slot(&mut self, state: &AppState, slot: u8) -> Option<Command> {
        let profile = self.current(state)?;
        if profile.is_default || profile.slot == slot || !SLOTS.contains(&slot) {
            return None;
        }
        let active = state.snapshot.active_profile.as_ref().map(|p| &p.id) == Some(&profile.id);
        if active {
            self.slot_dialog = Some(slot);
            return None;
        }
        Some(Command::SetProfileSlot {
            profile: profile.id.clone(),
            slot,
        })
    }

    pub fn confirm_slot(&mut self, state: &AppState) -> Option<Command> {
        let slot = self.slot_dialog.take()?;
        Some(Command::SetProfileSlot {
            profile: self.current(state)?.id.clone(),
            slot,
        })
    }

    /// The groups that point to a Profile are deleted with it.
    pub fn confirm_delete(&mut self, state: &AppState) -> Option<Command> {
        self.delete_dialog = false;
        let profile = self.current(state)?;
        if profile.is_default {
            return None;
        }
        let id = profile.id.clone();
        let uses = state.snapshot.rule_groups.iter().any(|g| g.profile == id);
        let groups = uses.then(|| {
            state
                .snapshot
                .rule_groups
                .iter()
                .filter(|g| g.profile != id)
                .cloned()
                .collect()
        });
        self.selected = None;
        self.editor = None;
        Some(Command::DeleteProfile {
            profile: id,
            groups,
        })
    }

    /// The switch. Turning it on opens the warning, unless the user hid it.
    pub fn toggle_immediate(&mut self, state: &AppState, warning_hidden: bool) -> Option<Command> {
        let enabled = !state.snapshot.immediate_switching;
        if enabled && !warning_hidden {
            self.immediate_dialog = Some(false);
            return None;
        }
        Some(Command::SetImmediateSwitching { enabled })
    }

    pub fn confirm_immediate(&mut self) -> Option<(Command, bool)> {
        let hide = self.immediate_dialog.take()?;
        Some((Command::SetImmediateSwitching { enabled: true }, hide))
    }

    /// The Rule delay in seconds and the Profile dwell in minutes.
    pub fn set_timing(
        &self,
        state: &AppState,
        rule_delay_s: Option<u32>,
        dwell_min: Option<u32>,
    ) -> Command {
        Command::SetRuleTiming {
            rule_delay_s: rule_delay_s.unwrap_or(state.snapshot.rule_delay_s),
            dwell_s: dwell_min.map_or(state.snapshot.dwell_s, |m| m * 60),
        }
    }

    // --- Rule groups ---

    pub fn add_group(&mut self, state: &AppState) {
        let target = self
            .current(state)
            .filter(|p| !p.is_default)
            .or_else(|| state.snapshot.profiles.iter().find(|p| !p.is_default))
            .or_else(|| state.snapshot.profiles.first())
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.editor = Some(GroupEditor {
            index: None,
            group: RuleGroup {
                id: String::new(),
                profile: target,
                enabled: true,
                rules: Vec::new(),
                holds: false,
                since_us: 0,
            },
            rule: None,
        });
    }

    pub fn open_group(&mut self, state: &AppState, index: usize) {
        self.editor = state
            .snapshot
            .rule_groups
            .get(index)
            .map(|group| GroupEditor {
                index: Some(index),
                group: group.clone(),
                rule: None,
            });
    }

    pub fn cancel_group(&mut self) {
        self.editor = None;
    }

    pub fn set_group_target(&mut self, profile: &str) {
        if let Some(editor) = self.editor.as_mut() {
            editor.group.profile = profile.to_owned();
        }
    }

    pub fn set_group_enabled(&mut self, enabled: bool) {
        if let Some(editor) = self.editor.as_mut() {
            editor.group.enabled = enabled;
        }
    }

    pub fn add_rule(&mut self, state: &AppState) {
        if let Some(editor) = self.editor.as_mut() {
            editor.rule = Some((None, RuleKind::Sensor.fresh(state)));
        }
    }

    pub fn edit_rule(&mut self, index: usize) {
        if let Some(editor) = self.editor.as_mut()
            && let Some(rule) = editor.group.rules.get(index)
        {
            editor.rule = Some((Some(index), rule.clone()));
        }
    }

    pub fn remove_rule(&mut self, index: usize) {
        if let Some(editor) = self.editor.as_mut()
            && index < editor.group.rules.len()
        {
            editor.group.rules.remove(index);
            editor.rule = None;
        }
    }

    /// The select "Condition": a new draft of that kind.
    pub fn set_rule_kind(&mut self, state: &AppState, kind: RuleKind) {
        if let Some((_, rule)) = self.editor.as_mut().and_then(|e| e.rule.as_mut())
            && RuleKind::of(rule) != kind
        {
            *rule = kind.fresh(state);
        }
    }

    pub fn rule_draft(&mut self) -> Option<&mut Rule> {
        self.editor.as_mut()?.rule.as_mut().map(|(_, rule)| rule)
    }

    /// "Save rule": the draft joins the group. A rule needs its parts: a Sensor rule a sensor,
    /// a Process rule a name.
    pub fn save_rule(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some((index, rule)) = editor.rule.take() else {
            return;
        };
        let complete = match &rule {
            Rule::Sensor { sensor, .. } => !sensor.is_empty(),
            Rule::Process { names, .. } => !names.is_empty(),
            Rule::Time { days, .. } => !days.is_empty(),
            Rule::Interval { .. } => true,
        };
        if !complete {
            editor.rule = Some((index, rule));
            return;
        }
        match index {
            Some(index) if index < editor.group.rules.len() => editor.group.rules[index] = rule,
            _ => editor.group.rules.push(rule),
        }
    }

    pub fn cancel_rule(&mut self) {
        if let Some(editor) = self.editor.as_mut() {
            editor.rule = None;
        }
    }

    /// "Save group": the whole list goes to the daemon.
    pub fn save_group(&mut self, state: &AppState) -> Option<Command> {
        let editor = self.editor.take()?;
        let mut groups = state.snapshot.rule_groups.clone();
        match editor.index {
            Some(index) if index < groups.len() => groups[index] = editor.group,
            _ => groups.push(editor.group),
        }
        Some(Command::SetRuleGroups { groups })
    }

    pub fn delete_group(&mut self, state: &AppState) -> Option<Command> {
        let editor = self.editor.take()?;
        let index = editor.index?;
        let mut groups = state.snapshot.rule_groups.clone();
        if index >= groups.len() {
            return None;
        }
        groups.remove(index);
        Some(Command::SetRuleGroups { groups })
    }

    /// A drag, or Alt+Up and Alt+Down: the group at `from` moves to `to`.
    pub fn move_group(&mut self, state: &AppState, from: usize, to: usize) -> Option<Command> {
        let mut groups = state.snapshot.rule_groups.clone();
        if from >= groups.len() || to >= groups.len() || from == to {
            return None;
        }
        let group = groups.remove(from);
        groups.insert(to, group);
        self.editor = None;
        Some(Command::SetRuleGroups { groups })
    }
}

/// The alarm thresholds that "Apply to all slots" copies.
const THRESHOLD_FIELDS: [&str; 3] = [
    "fill_level_warning_threshold",
    "fill_level_alarm_threshold",
    "leak_rate_warning_threshold",
];

/// The threshold fields of a slot, when it has all of them.
fn threshold_values(
    fields: &std::collections::BTreeMap<String, FieldValue>,
) -> Option<Vec<(String, FieldValue)>> {
    let values: Vec<(String, FieldValue)> = THRESHOLD_FIELDS
        .iter()
        .filter_map(|id| Some(((*id).to_owned(), fields.get(*id)?.clone())))
        .collect();
    (values.len() == THRESHOLD_FIELDS.len()).then_some(values)
}

/// The feedback key of the write to one slot of a Leakshield.
fn slot_key(device: &str, slot: u8, active: u8) -> String {
    if slot == active {
        format!("ls:{device}:slot-{slot}")
    } else {
        format!("stored:{device}:{slot}")
    }
}

/// The writes that copy `values` to every other slot of a Leakshield that has stored settings.
/// The slot that the device runs is written through the device (`SetFields`): the daemon refuses
/// `SetStoredFields` for it.
fn slot_commands(
    state: &AppState,
    device: &str,
    slot: u8,
    values: &[(String, FieldValue)],
) -> Vec<Command> {
    let Some(data) = state.snapshot.leakshield_data.get(device) else {
        return Vec::new();
    };
    let active = state
        .snapshot
        .leakshields
        .iter()
        .find(|d| d.id == device)
        .map_or(0, |d| d.active_slot);
    data.stored
        .keys()
        .filter(|other| **other != slot)
        .map(|other| {
            if *other == active {
                Command::SetLeakshieldFields {
                    leakshield: device.to_owned(),
                    control: format!("slot-{other}"),
                    fields: values.to_vec(),
                }
            } else {
                Command::SetStoredFields {
                    leakshield: device.to_owned(),
                    slot: *other,
                    fields: values.to_vec(),
                }
            }
        })
        .collect()
}

/// One row of the diff table before an activation.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffRow {
    pub leakshield: String,
    pub field: String,
    /// What the device holds now.
    pub device: FieldValue,
    /// What the stored slot holds.
    pub stored: FieldValue,
}

/// The fields that differ between the stored settings of `slot` and the device, for every
/// connected Leakshield whose `ActiveSlot` is another slot.
pub fn slot_diff(state: &AppState, slot: u8) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for device in &state.snapshot.leakshields {
        if !device.connected || device.active_slot == 0 || device.active_slot == slot {
            continue;
        }
        let Some(data) = state.snapshot.leakshield_data.get(&device.id) else {
            continue;
        };
        let Some(stored) = data.stored.get(&slot) else {
            continue;
        };
        for (field, stored) in stored {
            if let Some(entry) = data.field(field)
                && entry.value != *stored
            {
                rows.push(DiffRow {
                    leakshield: device.id.clone(),
                    field: field.clone(),
                    device: entry.value.clone(),
                    stored: stored.clone(),
                });
            }
        }
    }
    rows
}

/// The other slots with stored settings whose values for `values` differ.
fn differing_slots(
    state: &AppState,
    device: &str,
    slot: u8,
    values: &[(String, FieldValue)],
) -> Vec<u8> {
    let Some(data) = state.snapshot.leakshield_data.get(device) else {
        return Vec::new();
    };
    data.stored
        .iter()
        .filter(|(other, _)| **other != slot)
        .filter(|(_, stored)| {
            values
                .iter()
                .any(|(id, value)| stored.get(id).is_some_and(|stored| stored != value))
        })
        .map(|(other, _)| *other)
        .collect()
}

/// A field value as text, for the diff table.
fn field_text(locale: Locale, value: &FieldValue) -> String {
    match value {
        FieldValue::Number(n) => plain(locale, *n),
        FieldValue::Flag(on) => locale.tr(if *on {
            "profiles.diff.on"
        } else {
            "profiles.diff.off"
        }),
        FieldValue::Text(text) => text.clone(),
        FieldValue::List(list) => list
            .iter()
            .map(|n| plain(locale, *n))
            .collect::<Vec<_>>()
            .join(", "),
        FieldValue::TextList(names) => names.join(", "),
    }
}

// --- The model ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilesState {
    Loading,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub caption: String,
    /// The tag "Active".
    pub active: bool,
    /// The Warning icon: the Profile has Mode RELEASE.
    pub release: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionsCard {
    pub title: String,
    pub rule_delay_s: u32,
    pub dwell_min: u32,
    pub immediate: bool,
    /// "Rule delay" and "Profile dwell" do not apply with Immediate switching.
    pub timing_enabled: bool,
    pub timing_caption: Option<String>,
    pub help: Vec<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    /// A button in the row: its label and the commands that it sends.
    pub button: Option<(String, Vec<Command>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveRow {
    pub channel: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotSelect {
    pub value: u8,
    pub options: Vec<u8>,
    pub enabled: bool,
    /// "Shared with Profile 3".
    pub shared: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestoreOffer {
    pub text: String,
    pub button: String,
    pub command: Option<Command>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub title: String,
    pub text: String,
    pub checkbox: Option<String>,
    pub cancel: String,
    pub confirm: String,
}

/// The diff table before an activation.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffDialog {
    pub title: String,
    pub text: String,
    /// "Field", "Leakshield now", "Slot 3".
    pub columns: [String; 3],
    /// The field, what the device holds, what the slot holds.
    pub rows: Vec<[String; 3]>,
    pub cancel: String,
    pub confirm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileDetail {
    pub id: String,
    pub name: String,
    /// "Active by group", "Active manually" or none.
    pub tag: Option<String>,
    pub caption: String,
    /// "Activate manually", or "Revoke manual activation" while this Profile is held by hand.
    pub activate_label: String,
    pub revoke: bool,
    pub activating: Option<FeedbackLine>,
    pub activate_enabled: bool,
    pub duplicate_label: String,
    pub rename_label: String,
    pub delete_label: String,
    pub delete_enabled: bool,
    pub delete_tooltip: Option<String>,
    pub curves: Vec<CurveRow>,
    pub slot: SlotSelect,
    /// "Mode", "Display", "Lighting" and "Alarm thresholds" of the slot, as (label, value).
    pub leakshield_rows: Vec<(String, String)>,
    pub notices: Vec<Notice>,
    /// One offer for each Leakshield that holds a `FailedWriteBackup`.
    pub restore: Vec<RestoreOffer>,
    /// "Group 2: time window 22:00 to 07:00." while a group holds the active Profile.
    pub rule_group: Option<String>,
    pub diff_dialog: Option<DiffDialog>,
    pub slot_dialog: Option<Dialog>,
    pub delete_dialog: Option<Dialog>,
    pub name_write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupRow {
    pub index: usize,
    pub id: String,
    pub number: usize,
    /// One sentence for each rule.
    pub rules: Vec<String>,
    pub target: String,
    pub enabled: bool,
    pub tag: Option<String>,
    pub warning: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleEditorModel {
    pub index: Option<usize>,
    pub kind: RuleKind,
    pub keys: Vec<&'static str>,
    pub rule: Rule,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorModel {
    pub index: Option<usize>,
    pub targets: Vec<(String, String, bool)>,
    pub enabled: bool,
    pub rules: Vec<String>,
    pub rule_editor: Option<RuleEditorModel>,
    /// The kinds of the select "Condition".
    pub kinds: Vec<RuleKind>,
    pub warning: Option<String>,
    pub can_delete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupsCard {
    pub caption: String,
    pub groups: Vec<GroupRow>,
    pub empty: Option<String>,
    pub manual: Option<String>,
    pub editor: Option<EditorModel>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfilesModel {
    pub state: ProfilesState,
    pub rows: Vec<ProfileRow>,
    pub options: OptionsCard,
    pub detail: Option<ProfileDetail>,
    pub groups: GroupsCard,
    pub immediate_dialog: Option<Dialog>,
    pub new_write_line: Option<FeedbackLine>,
    pub writes_enabled: bool,
    pub writes_tooltip: Option<String>,
}

impl ProfilesModel {
    pub fn build(input: &ScreenInput<'_>, ui: &ProfilesUi) -> Self {
        let locale = input.locale;
        let state = input.state;
        let snapshot = &state.snapshot;
        let writes = input.writes();
        let current = ui.current(state);
        let ui_state = if state.link == Link::Connecting {
            ProfilesState::Loading
        } else {
            ProfilesState::Ready
        };
        let rows = snapshot
            .profiles
            .iter()
            .map(|profile| ProfileRow {
                id: profile.id.clone(),
                name: profile.name.clone(),
                caption: row_caption(input, profile),
                active: snapshot.active_profile.as_ref().map(|p| &p.id) == Some(&profile.id),
                release: profile.contains_release,
                selected: current.is_some_and(|c| c.id == profile.id),
            })
            .collect();
        let immediate_dialog = ui.immediate_dialog.map(|_| Dialog {
            title: locale.tr("profiles.immediate.title"),
            text: locale.tr("profiles.immediate.text"),
            checkbox: Some(locale.tr("profiles.immediate.hide")),
            cancel: locale.tr("profiles.cancel"),
            confirm: locale.tr("profiles.immediate.confirm"),
        });
        Self {
            state: ui_state,
            rows,
            options: options_card(input),
            detail: current.map(|profile| detail_of(input, ui, profile)),
            groups: groups_card(input, ui),
            immediate_dialog,
            new_write_line: write_line(input, "profile:new".to_owned()),
            writes_enabled: writes.enabled,
            writes_tooltip: writes.tooltip,
        }
    }
}

fn write_line(input: &ScreenInput<'_>, key: String) -> Option<FeedbackLine> {
    let locale = input.locale;
    input
        .feedback
        .get(&key)
        .map(|outcome| FeedbackLine::new(outcome, locale, &locale.tr("feedback.refused"), None))
}

/// "Active by group 2", "Group 1", "No group. Slot 3. Mode RELEASE".
fn row_caption(input: &ScreenInput<'_>, profile: &ProfileInfo) -> String {
    let locale = input.locale;
    let state = input.state;
    if profile.is_default {
        return locale.tr("profiles.caption.default");
    }
    let groups: Vec<usize> = state
        .snapshot
        .rule_groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.profile == profile.id)
        .map(|(index, _)| index + 1)
        .collect();
    let mut parts = Vec::new();
    let active = state.snapshot.active_profile.as_ref().map(|p| &p.id) == Some(&profile.id);
    if active && state.snapshot.cause == crate::model::Cause::Rule {
        let number = active_group(state).map_or(String::new(), |n| n.to_string());
        parts.push(locale.fmt("profiles.caption.active_by_group", &[("n", &number)]));
    } else if groups.is_empty() {
        parts.push(locale.tr("profiles.caption.no_group"));
        parts.push(locale.fmt(
            "profiles.caption.slot",
            &[("slot", &profile.slot.to_string())],
        ));
    } else {
        let numbers: Vec<String> = groups.iter().map(|n| n.to_string()).collect();
        parts.push(locale.fmt("profiles.caption.group", &[("n", &numbers.join(", "))]));
    }
    if active && state.snapshot.dwell_until_us > input.now_us {
        let seconds = (state.snapshot.dwell_until_us - input.now_us).div_ceil(1_000_000);
        parts.push(locale.fmt(
            "profiles.caption.dwell",
            &[("time", &duration_text(locale, seconds as u32))],
        ));
    }
    if profile.contains_release {
        parts.push(locale.tr("profiles.caption.release"));
    }
    parts.join(". ")
}

/// "Group 2: time window 22:00 to 07:00." for the group that holds the active Profile.
pub fn active_group_text(locale: Locale, state: &AppState) -> Option<String> {
    if state.snapshot.cause != crate::model::Cause::Rule {
        return None;
    }
    let number = active_group(state)?;
    let group = state.snapshot.rule_groups.get(number - 1)?;
    let rules: Vec<String> = group
        .rules
        .iter()
        .map(|rule| {
            let text = rule_sentence(locale, state, rule);
            let mut chars = text.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_lowercase().chain(chars).collect()
            })
        })
        .collect();
    Some(locale.fmt(
        "profiles.group.active",
        &[("n", &number.to_string()), ("rules", &rules.join("; "))],
    ))
}

/// The number of the group that activated the active Profile: the first one that holds.
fn active_group(state: &AppState) -> Option<usize> {
    let active = state.snapshot.active_profile.as_ref()?;
    state
        .snapshot
        .rule_groups
        .iter()
        .position(|g| g.holds && g.enabled && g.profile == active.id)
        .map(|index| index + 1)
}

fn options_card(input: &ScreenInput<'_>) -> OptionsCard {
    let locale = input.locale;
    let snapshot = &input.state.snapshot;
    let immediate = snapshot.immediate_switching;
    OptionsCard {
        title: locale.tr("profiles.options.title"),
        rule_delay_s: snapshot.rule_delay_s,
        dwell_min: snapshot.dwell_s / 60,
        immediate,
        timing_enabled: !immediate,
        timing_caption: immediate.then(|| locale.tr("profiles.options.not_used")),
        help: vec![
            locale.tr("profiles.options.help_delay"),
            locale.tr("profiles.options.help_dwell"),
        ],
        write_line: write_line(input, "profile-timing".to_owned())
            .or_else(|| write_line(input, "profile-immediate".to_owned())),
    }
}

fn channel_name(state: &AppState, id: &str) -> String {
    state
        .snapshot
        .channels
        .get(id)
        .map_or_else(|| id.to_owned(), |c| c.name.clone())
}

fn curve_rows(input: &ScreenInput<'_>, profile: &ProfileInfo) -> Vec<CurveRow> {
    let locale = input.locale;
    let state = input.state;
    let active = state.snapshot.active_profile.as_ref().map(|p| &p.id) == Some(&profile.id);
    if let Some(content) = state.snapshot.profile_contents.get(&profile.id) {
        return content
            .channels
            .iter()
            .filter(|(_, c)| c.control != Control::Hardware)
            .map(|(id, c)| CurveRow {
                channel: channel_name(state, id),
                summary: match c.control {
                    Control::Fixed => locale.fmt(
                        "profiles.curves.fixed",
                        &[("duty", &value_text(locale, c.fixed_duty, "%"))],
                    ),
                    _ => locale.plural("profiles.curves.curve", c.points.len()),
                },
            })
            .collect();
    }
    // The daemon gave no content: the active Profile is what the Channels hold now.
    if !active {
        return Vec::new();
    }
    state
        .snapshot
        .channels
        .values()
        .filter(|c| c.control != Control::Hardware)
        .map(|c| CurveRow {
            channel: c.name.clone(),
            summary: match c.control {
                Control::Fixed => locale.fmt(
                    "profiles.curves.fixed",
                    &[("duty", &value_text(locale, c.fixed_duty, "%"))],
                ),
                _ => locale.plural("profiles.curves.curve", c.curve.points.len()),
            },
        })
        .collect()
}

fn field_number(fields: &std::collections::BTreeMap<String, FieldValue>, id: &str) -> Option<f64> {
    match fields.get(id)? {
        FieldValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// The fields of the slot of the first Leakshield in a Profile content.
fn slot_fields(
    content: &ProfileContent,
) -> Option<(&String, &std::collections::BTreeMap<String, FieldValue>)> {
    content.leakshield.iter().next()
}

/// The values of the alarm thresholds, as text and as numbers for the comparison.
fn thresholds(
    locale: Locale,
    fields: &std::collections::BTreeMap<String, FieldValue>,
) -> Option<(String, [f64; 3])> {
    let warning = field_number(fields, "fill_level_warning_threshold")?;
    let alarm = field_number(fields, "fill_level_alarm_threshold")?;
    let leak = field_number(fields, "leak_rate_warning_threshold")?;
    Some((
        locale.fmt(
            "profiles.leakshield.thresholds_value",
            &[
                ("warning", &plain(locale, warning)),
                ("alarm", &plain(locale, alarm)),
                ("leak", &plain(locale, leak)),
            ],
        ),
        [warning, alarm, leak],
    ))
}

fn leakshield_rows(
    locale: Locale,
    fields: &std::collections::BTreeMap<String, FieldValue>,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mode = match fields.get("default_mode") {
        Some(FieldValue::Text(text)) => Some(text.to_uppercase()),
        _ => None,
    };
    if let Some(mode) = mode {
        rows.push((locale.tr("profiles.leakshield.mode"), mode));
    }
    let brightness = |id: &str| match fields.get(id) {
        Some(FieldValue::Text(level)) => Some(match level.as_str() {
            "maximum" | "mid" | "low" | "off" => {
                locale.tr(&format!("leakshield.display.brightness.{level}"))
            }
            other => other.to_owned(),
        }),
        _ => None,
    };
    if let (Some(active), Some(idle)) = (
        brightness("display_brightness_active"),
        brightness("display_brightness_idle"),
    ) {
        rows.push((
            locale.tr("profiles.leakshield.display"),
            locale.fmt(
                "profiles.leakshield.display_value",
                &[("active", &active), ("idle", &idle)],
            ),
        ));
    }
    if let Some(brightness) = field_number(fields, "led_strip_brightness") {
        rows.push((
            locale.tr("profiles.leakshield.lighting"),
            locale.fmt(
                "profiles.leakshield.lighting_value",
                &[("value", &plain(locale, brightness))],
            ),
        ));
    }
    if let Some((text, _)) = thresholds(locale, fields) {
        rows.push((locale.tr("profiles.leakshield.thresholds"), text));
    }
    rows
}

fn detail_of(input: &ScreenInput<'_>, ui: &ProfilesUi, profile: &ProfileInfo) -> ProfileDetail {
    let locale = input.locale;
    let state = input.state;
    let snapshot = &state.snapshot;
    let writes = input.writes();
    let active = snapshot.active_profile.as_ref().map(|p| &p.id) == Some(&profile.id);
    let manual = active && snapshot.cause == crate::model::Cause::Manual;
    let tag = if !active {
        None
    } else if manual {
        Some(locale.tr("profiles.tag.manual"))
    } else if snapshot.cause == crate::model::Cause::Rule {
        Some(locale.tr("profiles.tag.group"))
    } else {
        None
    };

    // Other Profiles on the same slot.
    let sharing: Vec<&ProfileInfo> = snapshot
        .profiles
        .iter()
        .filter(|p| p.id != profile.id && p.slot == profile.slot)
        .collect();
    let shared_names = sharing
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let slot = SlotSelect {
        value: profile.slot,
        options: SLOTS.to_vec(),
        enabled: writes.enabled && !profile.is_default,
        shared: (!sharing.is_empty())
            .then(|| locale.fmt("profiles.slot.shared", &[("names", &shared_names)])),
        write_line: write_line(input, format!("profile-slot:{}", profile.id)),
    };

    let content = snapshot.profile_contents.get(&profile.id);
    let fields = content.and_then(slot_fields).map(|(_, fields)| fields);
    let leakshield_rows = fields.map_or_else(Vec::new, |f| leakshield_rows(locale, f));

    let mut notices = Vec::new();
    if profile.contains_release {
        notices.push(Notice {
            kind: NoticeKind::Warning,
            text: locale.tr("profiles.notice.release"),
            button: None,
        });
    }
    if !sharing.is_empty() {
        notices.push(Notice {
            kind: NoticeKind::Info,
            text: locale.fmt(
                "profiles.notice.shared_slot",
                &[
                    ("slot", &profile.slot.to_string()),
                    ("names", &shared_names),
                ],
            ),
            button: None,
        });
    }
    // Alarm thresholds of this slot against the stored settings of the other slots, for each
    // Leakshield. With more than one Leakshield, each row names its device.
    if let Some(content) = content {
        for (device, fields) in &content.leakshield {
            let Some(values) = threshold_values(fields) else {
                continue;
            };
            let prefix = snapshot
                .leakshields
                .iter()
                .find(|d| &d.id == device)
                .filter(|_| snapshot.leakshields.len() > 1)
                .map(|d| format!("{}: ", leakshield_label(locale, snapshot, d)))
                .unwrap_or_default();
            let others = differing_slots(state, device, profile.slot, &values);
            if !others.is_empty() {
                notices.push(Notice {
                    kind: NoticeKind::Warning,
                    text: format!(
                        "{prefix}{}",
                        locale.fmt(
                            "profiles.notice.thresholds_differ",
                            &[("count", &others.len().to_string())],
                        )
                    ),
                    button: Some((
                        locale.tr("profiles.notice.apply_all"),
                        slot_commands(state, device, profile.slot, &values),
                    )),
                });
            }
            // One row for each slot that the last "Apply to all slots" could not change.
            let active = snapshot
                .leakshields
                .iter()
                .find(|d| &d.id == device)
                .map_or(0, |d| d.active_slot);
            for slot in SLOTS {
                if let Some(outcome) = input.feedback.get(&slot_key(device, slot, active))
                    && outcome.is_failure()
                {
                    let reason = match outcome {
                        WriteOutcome::Refused { error } => refused_text(locale, error),
                        _ => locale.tr("feedback.not_authorized"),
                    };
                    notices.push(Notice {
                        kind: NoticeKind::Warning,
                        text: format!(
                            "{prefix}{}",
                            locale.fmt(
                                "profiles.notice.slot_failed",
                                &[("slot", &slot.to_string()), ("reason", &reason)],
                            )
                        ),
                        button: None,
                    });
                }
            }
        }
    }
    if let Some(fields) = &state.external_change
        && !fields.is_empty()
    {
        let name = if fields.iter().all(|f| f.starts_with("display_")) {
            locale.tr("profiles.notice.display_brightness")
        } else {
            fields[0].replace('_', " ")
        };
        notices.push(Notice {
            kind: NoticeKind::Info,
            text: locale.fmt("profiles.notice.external_change", &[("field", &name)]),
            button: None,
        });
    }

    // The daemon keeps the backup of a failed write: an app that starts later still finds it.
    let restore: Vec<RestoreOffer> = snapshot
        .leakshields
        .iter()
        .filter(|device| active && !device.failed_write_backup.is_empty())
        .map(|device| {
            let prefix = if snapshot.leakshields.len() > 1 {
                format!("{}: ", leakshield_label(locale, snapshot, device))
            } else {
                String::new()
            };
            RestoreOffer {
                text: format!("{prefix}{}", locale.tr("profiles.restore.text")),
                button: locale.tr("profiles.restore.button"),
                command: Some(Command::RestoreBackup {
                    leakshield: device.id.clone(),
                    backup: device.failed_write_backup.clone(),
                }),
                write_line: write_line(input, format!("restore:{}", device.id)),
            }
        })
        .collect();

    let diff_dialog = (ui.diff_dialog.as_deref() == Some(profile.id.as_str())).then(|| {
        let rows = slot_diff(state, profile.slot)
            .into_iter()
            .map(|row| {
                [
                    humanize(&row.field),
                    field_text(locale, &row.device),
                    field_text(locale, &row.stored),
                ]
            })
            .collect();
        let slot = profile.slot.to_string();
        DiffDialog {
            title: locale.fmt("profiles.diff.title", &[("name", &profile.name)]),
            text: locale.fmt("profiles.diff.text", &[("slot", &slot)]),
            columns: [
                locale.tr("profiles.diff.field"),
                locale.tr("profiles.diff.device"),
                locale.fmt("profiles.slot.value", &[("slot", &slot)]),
            ],
            rows,
            cancel: locale.tr("profiles.cancel"),
            confirm: locale.tr("profiles.diff.confirm"),
        }
    });

    let dialog = |title: String, text: String, cancel: &str, confirm: &str| Dialog {
        title,
        text,
        checkbox: None,
        cancel: locale.tr(cancel),
        confirm: locale.tr(confirm),
    };
    let slot_dialog = ui.slot_dialog.map(|new| {
        dialog(
            locale.fmt("profiles.slot_dialog.title", &[("slot", &new.to_string())]),
            locale.fmt("profiles.slot_dialog.text", &[("slot", &new.to_string())]),
            "profiles.cancel",
            "profiles.slot_dialog.confirm",
        )
    });
    let used_by: Vec<usize> = snapshot
        .rule_groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.profile == profile.id)
        .map(|(i, _)| i + 1)
        .collect();
    let delete_dialog = ui.delete_dialog.then(|| {
        let text = if used_by.is_empty() {
            locale.fmt("profiles.delete.text", &[("name", &profile.name)])
        } else {
            locale.fmt(
                "profiles.delete.text_groups",
                &[
                    ("name", &profile.name),
                    (
                        "groups",
                        &used_by
                            .iter()
                            .map(|n| n.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ],
            )
        };
        dialog(
            locale.fmt("profiles.delete.title", &[("name", &profile.name)]),
            text,
            "profiles.cancel",
            "profiles.delete.confirm",
        )
    });

    ProfileDetail {
        id: profile.id.clone(),
        name: profile.name.clone(),
        tag,
        caption: row_caption(input, profile),
        activate_label: locale.tr(if manual {
            "profiles.revoke"
        } else {
            "profiles.activate"
        }),
        revoke: manual,
        activating: write_line(input, "profile-activation".to_owned()),
        activate_enabled: writes.enabled
            && (manual || !active || snapshot.cause != crate::model::Cause::Manual),
        duplicate_label: locale.tr("profiles.duplicate"),
        rename_label: locale.tr("profiles.rename"),
        delete_label: locale.tr("profiles.delete_button"),
        delete_enabled: writes.enabled && !profile.is_default,
        delete_tooltip: profile
            .is_default
            .then(|| locale.tr("profiles.delete.default_tooltip")),
        curves: curve_rows(input, profile),
        slot,
        leakshield_rows,
        notices,
        restore,
        rule_group: active.then(|| active_group_text(locale, state)).flatten(),
        diff_dialog,
        slot_dialog,
        delete_dialog,
        name_write_line: write_line(input, format!("profile-name:{}", profile.id)),
    }
}

// --- Sentences ---

/// "30 s", "10 min", "1 min 30 s".
pub fn duration_text(locale: Locale, seconds: u32) -> String {
    let (minutes, rest) = (seconds / 60, seconds % 60);
    match (minutes, rest) {
        (0, s) => locale.fmt("profiles.duration.s", &[("n", &s.to_string())]),
        (m, 0) => locale.fmt("profiles.duration.min", &[("n", &m.to_string())]),
        (m, s) => format!(
            "{} {}",
            locale.fmt("profiles.duration.min", &[("n", &m.to_string())]),
            locale.fmt("profiles.duration.s", &[("n", &s.to_string())])
        ),
    }
}

/// "22:00".
pub fn minutes_text(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// One line for a rule, as the Rule group list and the editor show it.
pub fn rule_sentence(locale: Locale, state: &AppState, rule: &Rule) -> String {
    match rule {
        Rule::Sensor {
            sensor,
            comparison,
            value,
            value_max,
            hold_s,
        } => {
            let info = state.snapshot.sensors.get(sensor);
            let name = info.map_or_else(|| sensor.clone(), |s| s.name.clone());
            let unit = info.map_or("", |s| s.unit.as_str());
            let text = if comparison == "in_range" {
                locale.fmt(
                    "profiles.rule.sensor_between",
                    &[
                        ("sensor", &name),
                        ("low", &plain(locale, *value)),
                        ("high", &value_text(locale, *value_max, unit)),
                    ],
                )
            } else {
                locale.fmt(
                    "profiles.rule.sensor",
                    &[
                        ("sensor", &name),
                        (
                            "comparison",
                            &locale.tr(&format!("profiles.cmp.{comparison}")),
                        ),
                        ("value", &value_text(locale, *value, unit)),
                    ],
                )
            };
            if *hold_s > 0 {
                format!(
                    "{text} {}",
                    locale.fmt(
                        "profiles.rule.for",
                        &[("time", &duration_text(locale, *hold_s))]
                    )
                )
            } else {
                text
            }
        }
        Rule::Process {
            names, match_kind, ..
        } => locale.fmt(
            &format!("profiles.rule.process_{match_kind}"),
            &[("names", &names.join(", "))],
        ),
        Rule::Time { days, start, end } => {
            let window = locale.fmt(
                "profiles.rule.time",
                &[("from", &minutes_text(*start)), ("to", &minutes_text(*end))],
            );
            if days.len() == 7 || days.is_empty() {
                window
            } else {
                let names: Vec<String> = days
                    .iter()
                    .map(|d| locale.tr(&format!("profiles.day.d{d}")))
                    .collect();
                format!("{window}, {}", names.join(" "))
            }
        }
        Rule::Interval {
            interval_s,
            delay_s,
            duration_s,
        } => locale.fmt(
            "profiles.rule.interval",
            &[
                ("every", &duration_text(locale, *interval_s)),
                ("delay", &duration_text(locale, *delay_s)),
                ("lasts", &duration_text(locale, *duration_s)),
            ],
        ),
    }
}

/// An Interval rule with a window shorter than the Rule delay never activates its Profile.
fn short_interval(state: &AppState, rules: &[Rule]) -> bool {
    !state.snapshot.immediate_switching
        && rules.iter().any(|rule| match rule {
            Rule::Interval { duration_s, .. } => *duration_s < state.snapshot.rule_delay_s,
            _ => false,
        })
}

fn groups_card(input: &ScreenInput<'_>, ui: &ProfilesUi) -> GroupsCard {
    let locale = input.locale;
    let state = input.state;
    let snapshot = &state.snapshot;
    let profile_name = |id: &str| {
        snapshot
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map_or_else(|| id.to_owned(), |p| p.name.clone())
    };
    let manual = snapshot.cause == crate::model::Cause::Manual;
    let groups: Vec<GroupRow> = snapshot
        .rule_groups
        .iter()
        .enumerate()
        .map(|(index, group)| {
            let tag = if group.holds && group.enabled {
                let waiting = !snapshot.immediate_switching
                    && snapshot.active_profile.as_ref().map(|p| &p.id) != Some(&group.profile)
                    && group.since_us != 0;
                let remaining = u64::from(snapshot.rule_delay_s)
                    .saturating_sub(input.now_us.saturating_sub(group.since_us) / 1_000_000);
                if waiting && remaining > 0 {
                    Some(locale.fmt(
                        "profiles.group.holds_activates",
                        &[("seconds", &remaining.to_string())],
                    ))
                } else {
                    Some(locale.fmt(
                        "profiles.group.holds_since",
                        &[("time", &clock_minutes(group.since_us))],
                    ))
                }
            } else {
                None
            };
            GroupRow {
                index,
                id: group.id.clone(),
                number: index + 1,
                rules: group
                    .rules
                    .iter()
                    .map(|rule| rule_sentence(locale, state, rule))
                    .collect(),
                target: profile_name(&group.profile),
                enabled: group.enabled,
                tag,
                warning: group
                    .rules
                    .is_empty()
                    .then(|| locale.tr("profiles.group.no_rule")),
                selected: ui.editor.as_ref().and_then(|e| e.index) == Some(index),
            }
        })
        .collect();

    let editor = ui.editor.as_ref().map(|editor| EditorModel {
        index: editor.index,
        targets: snapshot
            .profiles
            .iter()
            .map(|p| (p.id.clone(), p.name.clone(), p.id == editor.group.profile))
            .collect(),
        enabled: editor.group.enabled,
        rules: editor
            .group
            .rules
            .iter()
            .map(|rule| rule_sentence(locale, state, rule))
            .collect(),
        rule_editor: editor.rule.as_ref().map(|(index, rule)| RuleEditorModel {
            index: *index,
            kind: RuleKind::of(rule),
            keys: rule_keys(rule),
            rule: rule.clone(),
            complete: match rule {
                Rule::Sensor { sensor, .. } => !sensor.is_empty(),
                Rule::Process { names, .. } => !names.is_empty(),
                Rule::Time { days, .. } => !days.is_empty(),
                Rule::Interval { .. } => true,
            },
        }),
        kinds: RULE_KINDS.to_vec(),
        warning: short_interval(state, &editor.group.rules)
            .then(|| locale.tr("profiles.group.short_interval")),
        can_delete: editor.index.is_some(),
    });

    GroupsCard {
        caption: locale.tr("profiles.groups.caption"),
        empty: groups
            .is_empty()
            .then(|| locale.tr("profiles.groups.empty")),
        manual: manual.then(|| locale.tr("profiles.groups.manual")),
        groups,
        editor,
        write_line: write_line(input, "rule-groups".to_owned()),
    }
}
