//! The System tab (`docs/ui/screens/leakshield-system.md`): the device, its clock, the standby
//! behavior, the backups, the refill history and the resets.

use std::path::Path;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{BackupEntry, FieldValue};
use crate::screens::format::{clock, date_time};
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};

/// The word that the user types before a factory reset.
pub const RESET_WORD: &str = "RESET";
/// A clock that differs from the PC clock by this much was not set.
const CLOCK_UNSET_S: i64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Restore(String),
    FactoryReset,
    RefillHistory,
    ServiceCounter,
}

#[derive(Clone, Debug, Default)]
pub struct SystemUi {
    pub ask: Option<Ask>,
    /// The text that the user typed for the factory reset.
    pub reset_word: String,
    /// The backup of the last restore, for "Try again".
    pub last_restore: Option<String>,
    /// The user closed the message of the last failed restore.
    pub closed_failure: bool,
}

impl SystemUi {
    pub fn request(&mut self, ask: Ask) {
        self.reset_word.clear();
        if let Ask::Restore(backup) = &ask {
            self.last_restore = Some(backup.clone());
        }
        self.closed_failure = false;
        self.ask = Some(ask);
    }

    pub fn cancel(&mut self) {
        self.ask = None;
        self.reset_word.clear();
    }

    /// The command of the answered question. A factory reset needs the typed word.
    pub fn confirm(&mut self, device: &str) -> Option<Command> {
        let leakshield = device.to_owned();
        let command = match self.ask.as_ref()? {
            Ask::Restore(backup) => Command::RestoreBackup {
                leakshield,
                backup: backup.clone(),
            },
            Ask::FactoryReset if self.reset_word.trim() == RESET_WORD => {
                Command::FactoryReset { leakshield }
            }
            Ask::FactoryReset => return None,
            Ask::RefillHistory => Command::ResetRefillHistory { leakshield },
            Ask::ServiceCounter => Command::ResetServiceCounter { leakshield },
        };
        self.ask = None;
        self.reset_word.clear();
        Some(command)
    }

    /// "Try again" after a failed restore.
    pub fn retry_restore(&mut self, device: &str) -> Option<Command> {
        self.closed_failure = false;
        self.last_restore
            .clone()
            .map(|backup| Command::RestoreBackup {
                leakshield: device.to_owned(),
                backup,
            })
    }
}

/// The firmware and the reports 3, 5 and 8 of a backup.
pub type BackupReports = (u16, Vec<u8>, Vec<u8>, Vec<u8>);

/// The text file of an exported backup: the firmware in decimal, then one line for each report,
/// in hexadecimal.
pub fn backup_file(reports: &BackupReports) -> String {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "firmware {}\nreport3 {}\nreport5 {}\nreport8 {}\n",
        reports.0,
        hex(&reports.1),
        hex(&reports.2),
        hex(&reports.3)
    )
}

/// The firmware and the reports of a backup file, or `None` for another file.
pub fn parse_backup_file(text: &str) -> Option<BackupReports> {
    let line = |name: &str| -> Option<Vec<u8>> {
        let hex = text
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(' '))?
            .trim();
        if hex.len() % 2 != 0 {
            return None;
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
            .collect()
    };
    let firmware = text
        .lines()
        .find_map(|line| line.strip_prefix("firmware ")?.trim().parse().ok())?;
    Some((
        firmware,
        line("report3")?,
        line("report5")?,
        line("report8")?,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: String,
    pub enabled: bool,
    /// The button asks for the administrator password.
    pub padlock: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCard {
    pub title: String,
    pub serial_label: String,
    pub serial: String,
    pub firmware_label: String,
    pub firmware: String,
    pub firmware_known: bool,
    pub firmware_tag: String,
    pub unknown_text: Option<String>,
    pub update_hint: String,
    pub maintenance_label: String,
    /// `None` while the device gives no value.
    pub maintenance: Option<f64>,
    pub maintenance_text: String,
    pub maintenance_warning: Option<String>,
    pub maintenance_caption: String,
    pub reset_counter: Button,
    pub counter_line: Option<FeedbackLine>,
    pub counter_unsupported: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClockCard {
    pub title: String,
    pub time_label: String,
    pub time: String,
    pub zone_label: String,
    /// The offset in hours, shown as the select value.
    pub zone: Option<f64>,
    pub zone_text: String,
    pub zone_options: Vec<(f64, String)>,
    pub format_label: String,
    pub hour12: bool,
    pub format_options: Vec<(bool, String)>,
    pub zone_writable: bool,
    pub format_writable: bool,
    pub sync: Button,
    pub warning: Option<String>,
    pub sync_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StandbyRow {
    pub label: String,
    /// The bit of `standby_conditions`.
    pub bit: u32,
    pub on: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StandbyCard {
    pub title: String,
    pub text: String,
    pub rows: Vec<StandbyRow>,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackupRow {
    pub id: String,
    pub time: String,
    pub reason: String,
    pub short_hash: String,
    pub export: Button,
    pub restore: Button,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackupsCard {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<BackupRow>,
    pub empty: Option<String>,
    pub back_up: Button,
    pub import: Button,
    pub caption: String,
    pub write_line: Option<FeedbackLine>,
    /// "Restoring": other writes are off until it ends.
    pub restoring: bool,
    pub restore_failed: Option<RestoreFailed>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestoreFailed {
    pub text: String,
    pub try_again: String,
    pub close: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefillCard {
    pub title: String,
    pub columns: Vec<String>,
    /// The device gives no refill rows through the daemon yet.
    pub empty: String,
    pub reset: Button,
    pub line: Option<FeedbackLine>,
    pub unsupported: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResetCard {
    pub title: String,
    pub factory: Button,
    pub write_line: Option<FeedbackLine>,
    pub denied: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AskDialog {
    pub title: String,
    pub text: String,
    /// The word to type, for the factory reset.
    pub word: Option<String>,
    pub cancel: String,
    pub confirm: String,
    pub confirm_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemModel {
    pub state: TabState,
    pub device: DeviceCard,
    pub clock: ClockCard,
    pub standby: StandbyCard,
    pub backups: BackupsCard,
    pub refill: RefillCard,
    pub reset: ResetCard,
    pub dialog: Option<AskDialog>,
}

fn refused_with(input: &ScreenInput<'_>, id: &str, control: &str, name: &str) -> bool {
    matches!(
        input.feedback.get(&format!("ls:{id}:{control}")),
        Some(WriteOutcome::Refused { error }) if error == name
    )
}

fn denied(input: &ScreenInput<'_>, id: &str, key: &str) -> bool {
    matches!(
        input.feedback.get(&format!("{key}:{id}")),
        Some(WriteOutcome::NotAuthorized)
    ) || matches!(
        input.feedback.get(&format!("ls:{id}:{key}")),
        Some(WriteOutcome::NotAuthorized)
    )
}

pub fn zone_text(hours: f64) -> String {
    let sign = if hours < 0.0 { '-' } else { '+' };
    let minutes = (hours.abs() * 60.0).round() as u32;
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

pub fn reason_label(locale: Locale, backup: &BackupEntry) -> String {
    locale.tr(match backup.reason.as_str() {
        "before-write" => "leakshield.system.reason.before_write",
        "import" => "leakshield.system.reason.import",
        "first" => "leakshield.system.reason.first",
        _ => "leakshield.system.reason.manual",
    })
}

impl SystemModel {
    pub fn build(input: &ScreenInput<'_>, ui: &SystemUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let ready = state == TabState::Ready;
        let unknown = device.unknown_firmware();
        let info = device.info;
        let button = |key: &str, padlock: bool, enabled: bool| Button {
            label: locale.tr(key),
            enabled: enabled && ready,
            padlock,
        };
        let restoring = matches!(
            input.feedback.get(&format!("restore:{id}")),
            Some(WriteOutcome::Pending)
        );
        let writes = ready && !restoring;

        let maintenance = device.sensor(input, "maintenance").filter(|_| ready);
        let counter_unsupported = refused_with(input, id, "service-counter", "NotSupported")
            .then(|| locale.tr("leakshield.refused.not_supported"));
        let device_card = DeviceCard {
            title: locale.tr("leakshield.system.device"),
            serial_label: locale.tr("leakshield.system.serial"),
            serial: info.map_or_else(String::new, |i| i.serial.clone()),
            firmware_label: locale.tr("leakshield.system.firmware"),
            firmware: info.map_or_else(String::new, |i| i.firmware.to_string()),
            firmware_known: !unknown,
            firmware_tag: locale.tr(if unknown {
                "leakshield.system.unknown"
            } else {
                "leakshield.system.known"
            }),
            unknown_text: unknown.then(|| locale.tr("leakshield.system.unknown_text")),
            update_hint: locale.tr("leakshield.system.update_hint"),
            maintenance_label: locale.tr("leakshield.system.maintenance"),
            maintenance,
            maintenance_text: maintenance.map_or_else(
                || "--".to_owned(),
                |value| {
                    locale.fmt(
                        "leakshield.system.percent",
                        &[("value", &locale.decimal(value, 0))],
                    )
                },
            ),
            maintenance_warning: maintenance
                .filter(|value| *value <= 0.0)
                .map(|_| locale.tr("leakshield.system.maintenance_zero")),
            maintenance_caption: locale.tr("leakshield.system.maintenance_caption"),
            reset_counter: button("leakshield.system.reset_counter", false, writes),
            counter_line: write_line(input, id, "service-counter")
                .filter(|_| counter_unsupported.is_none()),
            counter_unsupported,
        };

        let offset_s = device.data.map_or(0, |data| data.clock_offset_s);
        let zone = device.number("clock_timezone");
        let flags = device.number("clock_flags").unwrap_or(0.0) as u32;
        let device_time = (input.now_us as i64 + offset_s * 1_000_000).max(0) as u64;
        let clock_card = ClockCard {
            title: locale.tr("leakshield.system.clock"),
            time_label: locale.tr("leakshield.system.device_time"),
            time: if device.data.is_some() {
                clock(device_time)
            } else {
                "--".to_owned()
            },
            zone_label: locale.tr("leakshield.system.zone"),
            zone,
            zone_text: zone.map_or_else(|| "--".to_owned(), zone_text),
            zone_options: (-12..=14)
                .map(|hours| (f64::from(hours), zone_text(f64::from(hours))))
                .collect(),
            format_label: locale.tr("leakshield.system.time_format"),
            hour12: flags & 2 != 0,
            format_options: vec![
                (false, locale.tr("leakshield.system.hour24")),
                (true, locale.tr("leakshield.system.hour12")),
            ],
            zone_writable: device.access("clock_timezone").writable && writes,
            format_writable: device.access("clock_flags").writable && writes,
            sync: button("leakshield.system.sync", false, writes),
            warning: (device.data.is_some() && offset_s.abs() >= CLOCK_UNSET_S)
                .then(|| locale.tr("leakshield.system.clock_unset")),
            sync_line: write_line(input, id, "clock"),
        };

        let standby = device.number("standby_conditions").unwrap_or(0.0) as u32;
        let standby_access = device.access("standby_conditions");
        let standby_card = StandbyCard {
            title: locale.tr("leakshield.system.standby"),
            text: locale.tr("leakshield.system.standby_text"),
            rows: [(1, "suspended"), (0, "no_usb"), (3, "alarm")]
                .iter()
                .map(|(bit, name)| StandbyRow {
                    label: locale.tr(&format!("leakshield.system.standby_row.{name}")),
                    bit: 1 << bit,
                    on: standby & (1 << bit) != 0,
                    writable: standby_access.writable && writes,
                })
                .collect(),
            write_line: write_line(input, id, "standby"),
        };

        let backups: &[BackupEntry] = device.data.map_or(&[], |data| &data.backups);
        let restore_pending = restoring;
        let restore_failed = matches!(
            input.feedback.get(&format!("restore:{id}")),
            Some(outcome) if outcome.is_failure() && !matches!(outcome, WriteOutcome::NotAuthorized)
        ) && !ui.closed_failure;
        let backups_card = BackupsCard {
            title: locale.tr("leakshield.system.backups"),
            columns: ["time", "reason", "hash"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.system.column.{name}")))
                .collect(),
            rows: backups
                .iter()
                .map(|backup| BackupRow {
                    id: backup.id.clone(),
                    time: date_time(backup.time_us),
                    reason: reason_label(locale, backup),
                    short_hash: backup.sha256.chars().take(8).collect(),
                    // A backup file is a copy on the disk: it works while the device is away.
                    export: Button {
                        label: locale.tr("leakshield.system.export"),
                        enabled: input.writes().enabled,
                        padlock: false,
                    },
                    restore: button("leakshield.system.restore", true, writes),
                })
                .collect(),
            empty: backups
                .is_empty()
                .then(|| locale.tr("leakshield.system.no_backups")),
            back_up: button("leakshield.system.back_up", false, writes),
            import: button("leakshield.system.import", false, writes),
            caption: locale.tr("leakshield.system.backups_caption"),
            write_line: write_line(input, id, "backups"),
            restoring: restore_pending,
            restore_failed: restore_failed.then(|| RestoreFailed {
                text: locale.tr("leakshield.system.restore_failed"),
                try_again: locale.tr("leakshield.system.try_again"),
                close: locale.tr("leakshield.close"),
            }),
        };

        let refill_unsupported = refused_with(input, id, "refill-history", "NotSupported")
            .then(|| locale.tr("leakshield.refused.not_supported"));
        let refill = RefillCard {
            title: locale.tr("leakshield.system.refill"),
            columns: ["time", "duration", "volume"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.system.column.{name}")))
                .collect(),
            empty: locale.tr("leakshield.system.refill_empty"),
            reset: button("leakshield.system.reset_history", false, writes),
            line: write_line(input, id, "refill-history").filter(|_| refill_unsupported.is_none()),
            unsupported: refill_unsupported,
        };

        let reset = ResetCard {
            title: locale.tr("leakshield.system.reset"),
            factory: button("leakshield.system.factory", true, writes),
            write_line: write_line(input, id, "factory-reset"),
            denied: (denied(input, id, "factory-reset") || denied(input, id, "restore"))
                .then(|| locale.tr("leakshield.system.denied")),
        };

        let dialog = ui.ask.as_ref().map(|ask| match ask {
            Ask::Restore(backup) => {
                let time = backups
                    .iter()
                    .find(|b| &b.id == backup)
                    .map_or_else(String::new, |b| date_time(b.time_us));
                AskDialog {
                    title: locale.tr("leakshield.system.restore"),
                    text: locale.fmt("leakshield.system.restore_text", &[("time", &time)]),
                    word: None,
                    cancel: locale.tr("leakshield.cancel"),
                    confirm: locale.tr("leakshield.system.restore"),
                    confirm_enabled: true,
                }
            }
            Ask::FactoryReset => AskDialog {
                title: locale.tr("leakshield.system.factory"),
                text: locale.tr("leakshield.system.factory_text"),
                word: Some(RESET_WORD.to_owned()),
                cancel: locale.tr("leakshield.cancel"),
                confirm: locale.tr("leakshield.system.factory_confirm"),
                confirm_enabled: ui.reset_word.trim() == RESET_WORD,
            },
            Ask::RefillHistory => AskDialog {
                title: locale.tr("leakshield.system.reset_history"),
                text: locale.tr("leakshield.system.reset_history_text"),
                word: None,
                cancel: locale.tr("leakshield.cancel"),
                confirm: locale.tr("leakshield.system.reset_history_confirm"),
                confirm_enabled: true,
            },
            Ask::ServiceCounter => AskDialog {
                title: locale.tr("leakshield.system.reset_counter"),
                text: locale.tr("leakshield.system.reset_counter_text"),
                word: None,
                cancel: locale.tr("leakshield.cancel"),
                confirm: locale.tr("leakshield.system.reset_counter_confirm"),
                confirm_enabled: true,
            },
        });

        Self {
            state,
            device: device_card,
            clock: clock_card,
            standby: standby_card,
            backups: backups_card,
            refill,
            reset,
            dialog,
        }
    }
}

impl SystemUi {
    pub fn close_failure(&mut self) {
        self.closed_failure = true;
    }
}

/// The commands that the System tab sends without a question.
pub fn direct_fields(device: &str, control: &str, field: &str, value: f64) -> Command {
    fields_command(
        device,
        control,
        vec![(field.to_owned(), FieldValue::Number(value))],
    )
}

/// Save an export in a file, for a test. The window uses the command.
pub fn write_backup_file(path: &Path, reports: &BackupReports) -> std::io::Result<()> {
    std::fs::write(path, backup_file(reports))
}
