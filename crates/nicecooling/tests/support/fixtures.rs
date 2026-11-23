use nicecooling::model::{
    AlarmEntry, AppState, CalibrationTableInfo, Cause, ChannelInfo, Control, CurveSpec,
    LeakshieldInfo, Link, LoopInfo, ProfileInfo, Response, SensorInfo, SensorSample, Severity,
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
            is_default: true,
            slot: 1,
            ..Default::default()
        },
        ProfileInfo {
            id: "profile_2".into(),
            name: "Quiet".into(),
            slot: 2,
            ..Default::default()
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
        ..Default::default()
    }];
    state.snapshot.leakshields = vec![LeakshieldInfo {
        id: LEAKSHIELD.into(),
        serial: "20417-31072".into(),
        firmware: 1040,
        connected: true,
        read_only_reason: String::new(),
        ..Default::default()
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
        details: Default::default(),
    }
}

pub fn sensor(
    id: &str,
    name: &str,
    kind: &str,
    unit: &str,
    origin: &str,
    driver: &str,
) -> SensorInfo {
    SensorInfo {
        id: id.into(),
        name: name.into(),
        default_name: name.into(),
        kind: kind.into(),
        unit: unit.into(),
        origin: origin.into(),
        driver: driver.into(),
        derived: None,
    }
}

/// `online()` with the sensors, Channels, Calibration table and Loop parts of a Board that
/// has a Leakshield, a pump, a flow sensor and a water temperature sensor.
pub fn full() -> AppState {
    let mut state = online();
    let sensors = [
        sensor(
            "nct6798_temp1",
            "Water temperature",
            "temperature",
            "degC",
            "hwmon",
            "nct6798",
        ),
        sensor(
            "k10temp_tctl",
            "CPU temperature",
            "temperature",
            "degC",
            "hwmon",
            "k10temp",
        ),
        sensor(
            "nct6798_fan6",
            "Waterpump speed",
            "fan",
            "rpm",
            "hwmon",
            "nct6798",
        ),
        sensor(
            "asusec_fan1",
            "Water flow",
            "flow",
            "rpm",
            "hwmon",
            "asusec",
        ),
        sensor("loop_1_flow", "Flow", "flow", "l/h", "calibrated", ""),
        sensor(
            &format!("{LEAKSHIELD}_pressure"),
            "Pressure",
            "pressure",
            "mbar",
            "leakshield",
            "",
        ),
        sensor(
            &format!("{LEAKSHIELD}_fill_level"),
            "Fill level",
            "level",
            "%",
            "leakshield",
            "",
        ),
    ];
    state.snapshot.sensors = sensors.into_iter().map(|s| (s.id.clone(), s)).collect();
    let channel = |id: &str, name: &str, control: Control, is_pump: bool| ChannelInfo {
        id: id.into(),
        name: name.into(),
        default_name: name.into(),
        control,
        is_pump,
        curve: CurveSpec {
            input: (control == Control::Curve).then(|| "nct6798_temp1".to_owned()),
            points: if control == Control::Curve {
                vec![(30.0, 40.0), (45.0, 70.0), (60.0, 100.0)]
            } else {
                Vec::new()
            },
            smoothing_s: 5.0,
            step_limit: 5.0,
        },
        fixed_duty: 50.0,
        ..Default::default()
    };
    state.snapshot.channels = [
        channel("nct6798_pwm6", "Waterpump", Control::Curve, true),
        channel("nct6798_pwm2", "Chassis Fan 1", Control::Hardware, false),
    ]
    .into_iter()
    .map(|c| (c.id.clone(), c))
    .collect();
    state.snapshot.calibration_tables = vec![CalibrationTableInfo {
        id: "alphacool_17558".into(),
        name: "Alphacool 17558 flow sensor".into(),
        preset: true,
        points: vec![(327.0, 40.0), (369.0, 50.0), (404.0, 60.0), (700.0, 120.0)],
    }];
    let main = &mut state.snapshot.loops[0];
    main.pump_channels = vec!["nct6798_pwm6".into()];
    main.flow_sensor = Some("asusec_fan1".into());
    main.calibration_table = "alphacool_17558".into();
    main.pump_floor = 40.0;
    state.sensors = Some(SensorSample {
        time_us: NOW_US,
        sensors: vec![
            ("nct6798_temp1".into(), 31.4, 0),
            ("k10temp_tctl".into(), 46.2, 0),
            ("nct6798_fan6".into(), 2840.0, 0),
            ("asusec_fan1".into(), 671.0, 0),
            ("loop_1_flow".into(), 115.2, 0),
            (format!("{LEAKSHIELD}_pressure"), 298.0, 0),
            (format!("{LEAKSHIELD}_fill_level"), 68.0, 0),
        ],
        channels: vec![
            ("nct6798_pwm6".into(), 62.0, 0),
            ("nct6798_pwm2".into(), 40.0, 2),
        ],
    });
    state.last_sensors = state.sensors.clone();
    state.snapshot.leakshields[0].mode = "shield".into();
    state
}
