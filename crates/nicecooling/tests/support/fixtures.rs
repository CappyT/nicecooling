use nicecooling::model::{
    AlarmEntry, AppState, Cause, LeakshieldInfo, Link, LoopInfo, ProfileInfo, Response,
    SensorSample, Severity,
};

pub const LEAKSHIELD: &str = "leakshield_20417_31072";
pub const NOW_US: u64 = 1_000_000_000;

/// An online installation with one Loop, one Leakshield, two Profiles and live values.
pub fn online() -> AppState {
    let mut state = AppState {
        link: Link::Online,
        ..Default::default()
    };
    let profiles = vec![
        ProfileInfo {
            id: "profile_default".into(),
            name: "Default".into(),
        },
        ProfileInfo {
            id: "profile_2".into(),
            name: "Quiet".into(),
        },
    ];
    state.snapshot.active_profile = Some(profiles[1].clone());
    state.snapshot.profiles = profiles;
    state.snapshot.cause = Cause::Default;
    state.snapshot.loops = vec![LoopInfo {
        id: "loop_1".into(),
        name: "Main loop".into(),
        leakshield: Some(LEAKSHIELD.into()),
        flow: Some("loop_1_flow".into()),
        water_temperature: Some("nct6798_temp1".into()),
    }];
    state.snapshot.leakshields = vec![LeakshieldInfo {
        id: LEAKSHIELD.into(),
        serial: "20417-31072".into(),
        firmware: 1040,
        connected: true,
        read_only_reason: String::new(),
    }];
    state.sensors = Some(SensorSample {
        time_us: NOW_US,
        sensors: vec![
            ("nct6798_temp1".into(), 31.4, 0),
            ("loop_1_flow".into(), 115.2, 0),
            (format!("{LEAKSHIELD}_pressure"), 298.0, 0),
        ],
        channels: Vec::new(),
    });
    state
}

pub fn alarm(
    instance: &str,
    event: &str,
    source: &str,
    response: Response,
    in_s: u64,
) -> AlarmEntry {
    AlarmEntry {
        instance: instance.into(),
        event: event.into(),
        severity: if event.ends_with("warning") || event == "lost-input" {
            Severity::Warning
        } else {
            Severity::Alarm
        },
        source: source.into(),
        response,
        raised_us: NOW_US,
        poweroff_at_us: if response == Response::Poweroff {
            NOW_US + in_s * 1_000_000
        } else {
            0
        },
    }
}
