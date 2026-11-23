mod support;

use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AlarmDetails, Link, Response};
use nicecooling::screens::overview::{OverviewModel, OverviewState, RowTone, TagTone};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling::shell::Page;
use nicecooling::widgets::loop_diagram::{BubbleState, Tag};
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, full};
use support::labels;

fn model(state: &nicecooling::model::AppState, locale: nicecooling::i18n::Locale) -> OverviewModel {
    let feedback = Feedback::default();
    OverviewModel::build(&ScreenInput {
        state,
        now_us: NOW_US + 1_000_000,
        locale,
        down_since_us: Some(NOW_US),
        feedback: &feedback,
    })
}

#[test]
fn overview_states() {
    // Loading: the daemon did not answer yet.
    let mut state = full();
    state.link = Link::Connecting;
    assert_eq!(model(&state, EN).state, OverviewState::Loading);

    // Empty: no Loop. Key temperatures still show.
    let mut state = full();
    state.snapshot.loops.clear();
    let empty = model(&state, EN);
    assert_eq!(empty.state, OverviewState::Empty);
    let card = empty.empty.expect("the empty card");
    assert_eq!(card.title, "No Loop yet");
    assert_eq!(
        card.text,
        "A Loop groups a pump, a flow sensor, a water temperature sensor and a Leakshield."
    );
    assert_eq!(card.button, "Set up a Loop");
    assert_eq!(empty.temperatures[0].value, "46.2 °C");

    // Normal: the diagram holds every part, and the captions follow the table.
    let state = full();
    let normal = model(&state, EN);
    assert_eq!(normal.state, OverviewState::Ready);
    let card = &normal.loops[0];
    assert_eq!(card.caption, "Leakshield, 1 Pump channel");
    assert_eq!(card.mode_tag.as_ref().unwrap().text, "SHIELD");
    let diagram = &card.diagram;
    let pt = diagram.bubble(Tag::Pt).unwrap();
    assert_eq!((pt.value.as_str(), pt.unit.as_str()), ("298", "mbar"));
    assert_eq!(pt.state, BubbleState::Normal);
    assert_eq!(pt.opens, Page::Leakshield(LEAKSHIELD.into()));
    let st = diagram.bubble(Tag::St).unwrap();
    assert_eq!(
        (st.value.as_str(), st.caption.as_str()),
        ("2840", "Duty 62 %")
    );
    assert_eq!(st.opens, Page::Channels);
    let ft = diagram.bubble(Tag::Ft).unwrap();
    assert_eq!(
        (ft.value.as_str(), ft.caption.as_str()),
        ("115", "calibrated")
    );
    assert!(!ft.info);
    let tt = diagram.bubble(Tag::Tt).unwrap();
    assert_eq!((tt.value.as_str(), tt.caption.as_str()), ("31.4", "water"));
    assert_eq!(diagram.bubble(Tag::Lt).unwrap().value, "68");
    assert!(diagram.no_leakshield.is_none());
    assert_eq!(
        diagram.accessible,
        "Main loop. Pressure 298 mbar, target 300. Fill level 68 %. Pump 2840 rpm. Flow 115 L/h. Water 31.4 °C."
    );
    assert!(normal.pressure.is_none());
    assert!(normal.alarms.is_empty());
    assert_eq!(normal.no_alarms, "No active Alarms");
    assert_eq!(normal.profile.name, "Quiet");
    assert_eq!(normal.profile.tag, None);

    // Flow above the Calibration table: a note on the value, not a Warning.
    let mut state = full();
    state.sensors.as_mut().unwrap().sensors[4].2 = 2;
    let above = model(&state, EN);
    let ft = above.loops[0].diagram.bubble(Tag::Ft).unwrap();
    assert!(ft.info);
    assert_eq!(ft.caption, "above table range");
    assert_eq!(ft.state, BubbleState::Normal);
    assert!(above.alarms.is_empty(), "no entry in Active Alarms");

    // A sensor without value: only that bubble is dashed.
    let mut state = full();
    state.sensors.as_mut().unwrap().sensors[0].2 = 1;
    let blank = model(&state, EN);
    let diagram = &blank.loops[0].diagram;
    let tt = diagram.bubble(Tag::Tt).unwrap();
    assert_eq!((tt.value.as_str(), tt.state), ("--", BubbleState::Offline));
    assert_eq!(diagram.bubble(Tag::Pt).unwrap().state, BubbleState::Normal);

    // Warning on the pump channel: amber bubble and one row, no banner text here.
    let mut state = full();
    state.snapshot.alarms = vec![alarm(
        "w1",
        "lost-input",
        "nct6798_pwm6",
        Response::Notify,
        0,
    )];
    let warning = model(&state, EN);
    assert_eq!(
        warning.loops[0].diagram.bubble(Tag::St).unwrap().state,
        BubbleState::Warning
    );
    assert_eq!(warning.alarms[0].tone, RowTone::Warning);
    assert_eq!(warning.alarms[0].title, "Input lost");
    assert_eq!(warning.alarms[0].opens, Some(Page::Channels));
    assert!(warning.alarms[0].detail.starts_with("Waterpump, since "));
    assert!(
        warning.alarms[0]
            .detail
            .ends_with("Alarm response: notify.")
    );

    // Alarm with poweroff: red bubble, the pressure card, the response with its delay.
    let mut state = full();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails {
        value: Some(212.0),
        limit: Some(250.0),
        unit: "mbar".into(),
        sensor: Some(format!("{LEAKSHIELD}_pressure")),
        ..Default::default()
    };
    state.snapshot.alarms = vec![leak];
    let alarmed = model(&state, EN);
    assert_eq!(
        alarmed.loops[0].diagram.bubble(Tag::Pt).unwrap().state,
        BubbleState::Alarm
    );
    assert_eq!(alarmed.alarms[0].tone, RowTone::Alarm);
    assert_eq!(alarmed.alarms[0].title, "Large leak Alarm");
    assert!(alarmed.alarms[0].detail.starts_with("Leakshield, since "));
    assert!(
        alarmed.alarms[0]
            .detail
            .ends_with("Alarm response: poweroff after 30 s."),
        "{}",
        alarmed.alarms[0].detail
    );
    let pressure = alarmed.pressure.expect("the pressure card shows");
    assert_eq!(pressure.title, "Pressure");
    assert_eq!(pressure.subtitle, "Leakshield, last 10 minutes");
    assert_eq!(pressure.sensor, format!("{LEAKSHIELD}_pressure"));
    assert_eq!(pressure.limit.as_ref().unwrap().1, "Limit 250 mbar");
    assert!(
        pressure
            .raised
            .as_ref()
            .unwrap()
            .1
            .starts_with("Alarm raised ")
    );

    // Leakshield unreachable: PT dashed, the fill level is absent, the others stay live.
    let mut state = full();
    state.snapshot.alarms = vec![alarm(
        "u1",
        "leakshield-unreachable",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    let unreachable = model(&state, EN);
    let diagram = &unreachable.loops[0].diagram;
    let pt = diagram.bubble(Tag::Pt).unwrap();
    assert_eq!((pt.value.as_str(), pt.state), ("--", BubbleState::Offline));
    assert!(diagram.bubble(Tag::Lt).is_none());
    assert_eq!(diagram.bubble(Tag::St).unwrap().state, BubbleState::Normal);
    assert_eq!(unreachable.alarms[0].title, "Leakshield unreachable");

    // An Action runs, and Mode RELEASE.
    let mut state = full();
    state.snapshot.leakshields[0].running_action = "test".into();
    assert_eq!(
        model(&state, EN).loops[0].mode_tag.as_ref().unwrap().text,
        "Test running"
    );
    let mut state = full();
    state.snapshot.leakshields[0].mode = "release".into();
    let release = model(&state, EN);
    let tag = release.loops[0].mode_tag.as_ref().unwrap();
    assert_eq!((tag.text.as_str(), tag.tone), ("RELEASE", TagTone::Warning));
    assert_eq!(
        release.loops[0].diagram.bubble(Tag::Pt).unwrap().caption,
        "no protection"
    );

    // The three limit sensors give the caption of PT.
    let mut state = full();
    assert_eq!(
        model(&state, EN).loops[0]
            .diagram
            .bubble(Tag::Pt)
            .unwrap()
            .caption,
        "target 300"
    );
    state.sensors.as_mut().unwrap().sensors[5].1 = 240.0;
    assert_eq!(
        model(&state, EN).loops[0]
            .diagram
            .bubble(Tag::Pt)
            .unwrap()
            .caption,
        "below minimum 250"
    );

    // A Loop without a Leakshield: no reservoir, no PT bubble, and the text.
    let mut state = full();
    state.snapshot.loops[0].leakshield = None;
    let bare = model(&state, EN);
    let diagram = &bare.loops[0].diagram;
    assert!(diagram.bubble(Tag::Pt).is_none() && diagram.bubble(Tag::Lt).is_none());
    assert_eq!(
        diagram.no_leakshield,
        Some(("No Leakshield".into(), "Pressure is not measured.".into()))
    );
    assert_eq!(bare.loops[0].caption, "No Leakshield, 1 Pump channel");
    assert!(bare.loops[0].mode_tag.is_none());

    // Two Pump channels: one symbol, the lowest speed, "2 pumps".
    let mut state = full();
    state.snapshot.loops[0].pump_channels = vec!["nct6798_pwm6".into(), "nct6798_pwm2".into()];
    state
        .snapshot
        .channels
        .get_mut("nct6798_pwm2")
        .unwrap()
        .fan_sensor = Some("nct6798_fan2".into());
    state.snapshot.sensors.insert(
        "nct6798_fan2".into(),
        support::fixtures::sensor("nct6798_fan2", "Fan 2", "fan", "rpm", "hwmon", "nct6798"),
    );
    state
        .sensors
        .as_mut()
        .unwrap()
        .sensors
        .push(("nct6798_fan2".into(), 1200.0, 0));
    let two = model(&state, EN);
    let st = two.loops[0].diagram.bubble(Tag::St).unwrap();
    assert_eq!(
        (st.value.as_str(), st.caption.as_str()),
        ("1200", "2 pumps")
    );

    // Daemon offline: dashed bubbles with "--", the last value as caption, the time on the card.
    let mut state = full();
    state.link = Link::Offline;
    state.sensors = None;
    let offline = model(&state, EN);
    let card = &offline.loops[0];
    assert_eq!(
        card.last_values
            .as_deref()
            .map(|t| t.starts_with("Last value ")),
        Some(true)
    );
    assert!(card.not_live.is_none());
    for bubble in &card.diagram.bubbles {
        assert_eq!(
            (bubble.value.as_str(), bubble.state),
            ("--", BubbleState::Offline)
        );
    }
    assert_eq!(card.diagram.bubble(Tag::Tt).unwrap().caption, "31.4 °C");
    assert!(offline.temperatures.iter().all(|t| t.value == "--"));

    // Daemon down: "Not live", "Last values", a row in Active Alarms and no Profile.
    state.link = Link::Down;
    let down = model(&state, EN);
    assert_eq!(down.loops[0].not_live.as_deref(), Some("Not live"));
    assert!(
        down.loops[0]
            .last_values
            .as_ref()
            .unwrap()
            .starts_with("Last values ")
    );
    assert_eq!(down.alarms[0].title, "Daemon down");
    assert_eq!(down.alarms[0].tone, RowTone::Alarm);
    assert!(
        down.alarms[0]
            .detail
            .ends_with("The daemon runs the alarm responses: none runs now.")
    );
    assert_eq!(down.profile.name, "None");
}

#[test]
fn profile_card_follows_the_cause() {
    let mut state = full();
    state.snapshot.cause = nicecooling::model::Cause::Manual;
    let manual = model(&state, EN);
    assert_eq!(manual.profile.tag.as_deref(), Some("Manual"));
    assert_eq!(
        manual.profile.detail.as_deref(),
        Some("Rule groups wait until you revoke it.")
    );
    state.snapshot.cause = nicecooling::model::Cause::Rule;
    assert_eq!(model(&state, EN).profile.tag.as_deref(), Some("By group"));
}

#[test]
fn overview_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/overview.md.
    let table = [
        ("overview.empty.title", "No Loop yet", "Nessun Loop"),
        (
            "overview.empty.button",
            "Set up a Loop",
            "Configura un Loop",
        ),
        (
            "overview.no_alarms",
            "No active Alarms",
            "Nessun Allarme attivo",
        ),
        ("overview.alarms_title", "Active Alarms", "Allarmi attivi"),
        ("overview.profile_title", "Active Profile", "Profilo attivo"),
        ("overview.not_live", "Not live", "Non in tempo reale"),
        ("overview.pressure.title", "Pressure", "Pressione"),
        (
            "overview.pressure.subtitle",
            "Leakshield, last 10 minutes",
            "Leakshield, ultimi 10 minuti",
        ),
        ("diagram.caption.calibrated", "calibrated", "calibrato"),
        (
            "diagram.caption.above_table",
            "above table range",
            "oltre la tabella",
        ),
        ("diagram.caption.water", "water", "acqua"),
        (
            "diagram.caption.no_protection",
            "no protection",
            "nessuna protezione",
        ),
        (
            "diagram.no_leakshield.text",
            "Pressure is not measured.",
            "La pressione non è misurata.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    // The model uses the words of the table in both languages.
    let state = full();
    let italian = model(&state, IT);
    assert_eq!(italian.loops[0].caption, "Leakshield, 1 Canale pompa");
    assert_eq!(
        italian.loops[0].diagram.bubble(Tag::Tt).unwrap().caption,
        "acqua"
    );
    assert_eq!(
        italian.loops[0].diagram.bubble(Tag::Tt).unwrap().value,
        "31,4"
    );
}

#[test]
fn pressure_card_target_and_minimum_from_sensors() {
    let mut state = full();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails {
        limit: Some(250.0),
        unit: "mbar".into(),
        ..Default::default()
    };
    state.snapshot.alarms = vec![leak];
    let card = model(&state, EN).pressure.expect("the pressure card shows");
    assert_eq!(card.target.as_deref(), Some("Target 300 mbar"));
    assert_eq!(card.minimum.as_deref(), Some("Minimum 250 mbar"));
    let italian = model(&state, IT).pressure.expect("the pressure card shows");
    assert_eq!(italian.target.as_deref(), Some("Obiettivo 300 mbar"));
    assert_eq!(italian.minimum.as_deref(), Some("Minimo 250 mbar"));

    // The values follow the sensors, not a stored copy.
    let sample = state.sensors.as_mut().unwrap();
    for (id, value, _) in &mut sample.sensors {
        if id.ends_with("_pressure_setpoint") {
            *value = 320.0;
        } else if id.ends_with("_pressure_low") {
            *value = 270.0;
        }
    }
    let card = model(&state, EN).pressure.unwrap();
    assert_eq!(card.target.as_deref(), Some("Target 320 mbar"));
    assert_eq!(card.minimum.as_deref(), Some("Minimum 270 mbar"));

    // A Leakshield without the sensors shows no target and no minimum.
    let sample = state.sensors.as_mut().unwrap();
    sample
        .sensors
        .retain(|(id, _, _)| !id.contains("_pressure_"));
    state.last_sensors = None;
    let card = model(&state, EN).pressure.unwrap();
    assert!(card.target.is_none() && card.minimum.is_none());
}
