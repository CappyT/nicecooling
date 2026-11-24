use nicecooling::model::{
    AlarmEntry, AppState, BackupEntry, BiosSnapshot, CalibrationTableInfo, Cause, ChannelInfo,
    Control, CurveSpec, DeviceEvent, FeedState, FieldEntry, FieldRisk, FieldValue, LastFeed,
    LeakshieldData, LeakshieldInfo, Link, LoopInfo, ProfileInfo, Response, ResponseProposal,
    SensorInfo, SensorSample, Severity,
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
        fan_sensor: (id == "nct6798_pwm6").then(|| "nct6798_fan6".to_owned()),
        bios_snapshot: BiosSnapshot {
            time_us: BIOS_SNAPSHOT_US,
            attributes: if id == "nct6798_pwm2" {
                hardware_curve_attributes("pwm2")
            } else {
                vec![
                    ("pwm6".into(), "158".into()),
                    ("pwm6_enable".into(), "5".into()),
                ]
            },
        },
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
            (format!("{LEAKSHIELD}_pressure_setpoint"), 300.0, 0),
            (format!("{LEAKSHIELD}_pressure_low"), 250.0, 0),
            (format!("{LEAKSHIELD}_pressure_high"), 350.0, 0),
        ],
        channels: vec![
            ("nct6798_pwm6".into(), 62.0, 31.4, 42.8, 4),
            ("nct6798_pwm2".into(), 40.0, 0.0, 0.0, 2),
        ],
    });
    state.last_sensors = state.sensors.clone();
    state.snapshot.leakshields[0].mode = "shield".into();
    state.snapshot.leakshields[0].feed_state = FeedState::Ok;
    state.snapshot.leakshields[0].last_feed = LastFeed {
        time_us: NOW_US,
        feed_pump: 2840.0,
        flow_lph: 115.2,
    };
    state
}

/// The time of the BIOS snapshot in the fixtures: 2023-11-14 22:13 UTC.
pub const BIOS_SNAPSHOT_US: u64 = 1_700_000_000_000_000;

/// The auto points of a Hardware curve with 5 points, as the snapshot holds them.
pub fn hardware_curve_attributes(pwm: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    for (point, (milli_degrees, raw)) in [
        (30_000, 51),
        (40_000, 102),
        (50_000, 153),
        (60_000, 204),
        (70_000, 255),
    ]
    .into_iter()
    .enumerate()
    {
        let point = point + 1;
        attributes.push((
            format!("{pwm}_auto_point{point}_temp"),
            milli_degrees.to_string(),
        ));
        attributes.push((format!("{pwm}_auto_point{point}_pwm"), raw.to_string()));
    }
    attributes.push((pwm.to_owned(), "158".to_owned()));
    attributes.push((format!("{pwm}_enable"), "5".to_owned()));
    attributes
}

fn field(id: &str, risk: FieldRisk, value: FieldValue, writable: bool) -> FieldEntry {
    FieldEntry {
        id: id.into(),
        risk,
        value,
        writable,
        reason: if writable {
            String::new()
        } else {
            "not-verified".into()
        },
    }
}

/// The fields, events, backups and Software sensors of a Leakshield with known firmware.
pub fn leakshield_data() -> LeakshieldData {
    use FieldRisk::{Low, Safety};
    let n = FieldValue::Number;
    let t = |text: &str| FieldValue::Text(text.into());
    let list = |values: Vec<f64>| FieldValue::List(values);
    let names =
        |names: &[&str]| FieldValue::TextList(names.iter().map(|n| (*n).to_owned()).collect());
    let fields = vec![
        field("default_mode", Safety, t("shield"), true),
        field("test_pressure", Safety, n(300.0), true),
        field("shield_pressure_low", Safety, n(250.0), false),
        field("shield_pressure_working_range", Safety, n(50.0), false),
        field("shield_pressure_max_range", Safety, n(50.0), false),
        field("shield_pressure_low_standby", Safety, n(80.0), true),
        field("monitor_pressure_low", Safety, n(100.0), false),
        field("monitor_pressure_working_range", Safety, n(20.0), false),
        field("monitor_pressure_max_range", Safety, n(20.0), false),
        field("shield_flags", Safety, n(127.0), true),
        field(
            "vacuum_timer_start_hour",
            Safety,
            list(vec![9.0, 18.0]),
            true,
        ),
        field(
            "vacuum_timer_start_minute",
            Safety,
            list(vec![0.0, 0.0]),
            true,
        ),
        field("vacuum_timer_end_hour", Safety, list(vec![9.0, 18.0]), true),
        field(
            "vacuum_timer_end_minute",
            Safety,
            list(vec![1.0, 1.0]),
            true,
        ),
        field("reservoir_model", Safety, t("custom"), true),
        field("reservoir_total_volume", Safety, n(270.0), false),
        field("reservoir_air_volume_full", Safety, n(65.0), true),
        field("reservoir_air_volume_empty", Safety, n(260.0), true),
        field("pump_model", Safety, t("d5"), true),
        field("pump_max_speed", Safety, n(4800.0), true),
        field("pump_max_pressure", Safety, n(375.0), true),
        field("pump_start_speed", Safety, n(2500.0), true),
        field("pump_start_flow", Safety, n(130.0), true),
        field("alarm_buzzer", Safety, n(1.0), true),
        field("fill_level_warning_threshold", Safety, n(60.0), true),
        field("fill_level_alarm_threshold", Safety, n(20.0), true),
        field("leak_rate_warning_threshold", Safety, n(150.0), true),
        field(
            "fill_level_warning_response_action",
            Safety,
            t("none"),
            true,
        ),
        field("fill_level_alarm_response_action", Safety, t("none"), true),
        field("leak_rate_warning_response_action", Safety, t("none"), true),
        field("leak_rate_alarm_1_response_action", Safety, t("none"), true),
        field(
            "leak_rate_alarm_2_response_action",
            Safety,
            t("signal-long"),
            true,
        ),
        field("system_alarm_response_action", Safety, t("none"), false),
        field("standby_conditions", Safety, n(11.0), true),
        field("display_page_time", Low, n(10.0), true),
        field("display_brightness_active", Low, t("low"), true),
        field("display_brightness_idle", Low, t("low"), true),
        field("display_flags", Low, n(0.0), true),
        field("clock_flags", Safety, n(1.0), true),
        field("clock_timezone", Safety, n(1.0), true),
        field(
            "page_template",
            Low,
            names(&[
                "leakshield-state",
                "values-1",
                "values-2",
                "none",
                "none",
                "none",
                "none",
                "none",
                "none",
                "none",
            ]),
            true,
        ),
        field("page_options", Low, list(vec![1.0; 10]), true),
        field(
            "page_sources",
            Low,
            {
                let mut sources = vec![255.0; 80];
                // Page 2 shows the pressure; page 3 shows the pressure and Software sensor 3
                // (sensor 10). A byte is the sensor index times 4.
                sources[8] = 0.0;
                sources[16] = 0.0;
                sources[17] = 40.0;
                list(sources)
            },
            true,
        ),
        field(
            "led_controller_mode",
            Low,
            names(&["fixed-color", "rainbow"]),
            true,
        ),
        field("led_controller_first_led", Low, list(vec![0.0, 2.0]), true),
        field("led_controller_led_count", Low, list(vec![6.0, 3.0]), true),
        field("led_controller_source", Low, list(vec![-1.0, 10.0]), true),
        field(
            "led_controller_parameters",
            Low,
            list(
                [
                    50.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                ]
                .repeat(2),
            ),
            true,
        ),
        field("led_controller_color_hue", Low, list(vec![0.0; 12]), true),
        field(
            "led_controller_color_saturation",
            Low,
            list(vec![255.0; 12]),
            true,
        ),
        field(
            "led_controller_color_value",
            Low,
            list(vec![255.0; 12]),
            true,
        ),
    ];
    let mut software: Vec<(u16, Option<String>)> = (1..=12).map(|slot| (slot, None)).collect();
    software[2].1 = Some("nct6798_temp1".into());
    LeakshieldData {
        fields,
        events: vec![
            DeviceEvent {
                time_us: 1_700_000_100_000_000,
                id: "mode-changed".into(),
                details: [
                    ("from".to_owned(), "monitor".to_owned()),
                    ("to".to_owned(), "shield".to_owned()),
                ]
                .into_iter()
                .collect(),
            },
            DeviceEvent {
                time_us: 1_700_000_050_000_000,
                id: "vacuum-pump-started".into(),
                details: [("pressure".to_owned(), "281".to_owned())]
                    .into_iter()
                    .collect(),
            },
            DeviceEvent {
                time_us: 1_700_000_000_000_000,
                id: "system-start".into(),
                details: Default::default(),
            },
        ],
        backups: vec![
            BackupEntry {
                id: "backup_2".into(),
                time_us: 1_700_000_200_000_000,
                sha256: "ab".repeat(32),
                firmware: 1040,
                reason: "manual".into(),
            },
            BackupEntry {
                id: "backup_1".into(),
                time_us: 1_700_000_000_000_000,
                sha256: "cd".repeat(32),
                firmware: 1040,
                reason: "before-write".into(),
            },
        ],
        software_sensors: software,
        proposals: vec![
            ResponseProposal {
                alarm: "leak-warning".into(),
                current: "none".into(),
                proposed: "none".into(),
                overlap: String::new(),
            },
            ResponseProposal {
                alarm: "leak-alarm-small".into(),
                current: "key-sequence".into(),
                proposed: "none".into(),
                overlap: "key-sequence-on".into(),
            },
            ResponseProposal {
                alarm: "leak-alarm".into(),
                current: "signal-long".into(),
                proposed: "none".into(),
                overlap: "double-poweroff".into(),
            },
            ResponseProposal {
                alarm: "fill-level-low-warning".into(),
                current: "none".into(),
                proposed: "none".into(),
                overlap: String::new(),
            },
            ResponseProposal {
                alarm: "fill-level-low-alarm".into(),
                current: "none".into(),
                proposed: "none".into(),
                overlap: String::new(),
            },
            ResponseProposal {
                alarm: "system-alarm".into(),
                current: "none".into(),
                proposed: "signal-long".into(),
                overlap: String::new(),
            },
        ],
        structure: 100,
        writable: true,
        clock_offset_s: -42,
        software_sensor_slots: 12,
        stored: Default::default(),
    }
}

/// `full()` with the data of a Leakshield, its live sensors and a Loop that feeds it.
pub fn with_leakshield() -> AppState {
    let mut state = full();
    state
        .snapshot
        .leakshield_data
        .insert(LEAKSHIELD.into(), leakshield_data());
    for (name, label, kind, unit) in [
        ("volume_change", "Volume change", "other", "ml/h"),
        ("pressure_change", "Pressure change", "other", "mbar/min"),
        ("feed_rpm", "Pump speed", "fan", "rpm"),
        ("flow", "Flow", "flow", "l/h"),
    ] {
        let info = sensor(
            &format!("{LEAKSHIELD}_{name}"),
            label,
            kind,
            unit,
            "leakshield",
            "",
        );
        state.snapshot.sensors.insert(info.id.clone(), info);
    }
    let live = [
        ("volume_change", 2.0),
        ("pressure_change", -0.4),
        ("feed_rpm", 2840.0),
        ("flow", 115.0),
    ];
    for sample in [&mut state.sensors, &mut state.last_sensors]
        .into_iter()
        .flatten()
    {
        for (name, value) in live {
            sample
                .sensors
                .push((format!("{LEAKSHIELD}_{name}"), value, 0));
        }
    }
    state
}
