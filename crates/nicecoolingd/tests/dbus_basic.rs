//! The D-Bus API on a private bus, with the simulated hardware layer of `support`.

mod support;

use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::authz_map::Action;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::derived_sensor::DerivedSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::loops::LoopProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::tick::percent_to_raw;
use nicecoolingd::lease::Lease;
use nicecoolingd::snapshot::{self, BiosSnapshot};
use nicecoolingd::state::StateStore;
use support::{Rig, error_name, none_path, path};
use tokio::time::timeout;
use zbus::fdo::ObjectManagerProxy;

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn channel(rig: &Rig, id: &str) -> ChannelProxy<'static> {
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn loop_proxy(rig: &Rig, id: &str) -> LoopProxy<'static> {
    LoopProxy::builder(&rig.client)
        .path(path(Kind::Loop, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reads_a_duty_property() {
    let mut rig = Rig::start().await;
    rig.ticks(3).await;
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    // Tctl is 61.875 C: the Curve gives 63.75%.
    let raw = percent_to_raw(63.75);
    assert_eq!(pwm1.raw().await.unwrap(), raw);
    let duty = pwm1.duty().await.unwrap();
    assert!(
        (duty - f64::from(raw) * 100.0 / 255.0).abs() < 1e-9,
        "{duty}"
    );
    assert_eq!(pwm1.control().await.unwrap(), "curve");
    assert_eq!(pwm1.id().await.unwrap(), "nct6798_pwm1");
    // The other Channel stays on its Hardware curve, and the Loop makes it a pump.
    let pwm2 = channel(&rig, "nct6798_pwm2").await;
    assert_eq!(pwm2.control().await.unwrap(), "hardware");
    assert!(pwm2.is_pump().await.unwrap());
    assert!(!pwm1.is_pump().await.unwrap());
    assert_eq!(pwm2.r#override().await.unwrap(), "none");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_curve_writes_a_curve() {
    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let input = path(Kind::Sensor, "nct6798_temp1");
    pwm1.set_curve(&input, &[(20.0, 10.0), (40.0, 50.0)], 0.0, 100.0)
        .await
        .unwrap();
    let (curve_input, points, smoothing, step) = pwm1.curve().await.unwrap();
    assert_eq!(curve_input, input);
    assert_eq!(points, [(20.0, 10.0), (40.0, 50.0)]);
    assert_eq!((smoothing, step), (0.0, 100.0));
    assert_eq!(pwm1.control().await.unwrap(), "curve");
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);

    // The new Curve takes effect: 30 C gives 30%.
    rig.ticks(3).await;
    assert_eq!(pwm1.raw().await.unwrap(), percent_to_raw(30.0));

    // The state files hold the change after the flush.
    rig.daemon.as_ref().unwrap().hub.flush_now();
    let profile =
        std::fs::read_to_string(rig.state.join("profiles/profile_default/profile.json")).unwrap();
    assert!(profile.contains("nct6798_temp1"), "{profile}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn five_sensors_updated_signals_in_five_ticks() {
    let mut rig = Rig::start().await;
    let manager = manager(&rig).await;
    let mut signals = manager.receive_sensors_updated().await.unwrap();
    rig.ticks(5).await;
    let mut received = Vec::new();
    for _ in 0..5 {
        let signal = timeout(Duration::from_secs(5), signals.next())
            .await
            .expect("a signal for each tick")
            .unwrap();
        let args = signal.args().unwrap();
        received.push((args.time_us, args.sensors.clone(), args.channels.clone()));
    }
    assert!(
        timeout(Duration::from_millis(300), signals.next())
            .await
            .is_err(),
        "one signal for each tick"
    );
    let last = received.last().unwrap();
    let sensor = |id: &str| {
        last.1
            .iter()
            .find(|s| s.0 == id)
            .unwrap_or_else(|| panic!("{id}"))
            .clone()
    };
    assert_eq!(
        sensor("k10temp_temp1"),
        ("k10temp_temp1".to_owned(), 61.875, 0)
    );
    assert_eq!(sensor("nct6798_fan1").1, 1200.0);
    assert_eq!(sensor("loop_1_flow").2, 0);
    assert!(sensor("loop_1_flow").1 > 0.0);
    let pwm1 = last.2.iter().find(|c| c.0 == "nct6798_pwm1").unwrap();
    assert_eq!(pwm1.4 & 2, 0, "pwm1 is on a Curve");
    let pwm2 = last.2.iter().find(|c| c.0 == "nct6798_pwm2").unwrap();
    assert_eq!(pwm2.4 & 2, 2, "pwm2 is on its Hardware curve");
    assert!(received.windows(2).all(|w| w[0].0 <= w[1].0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensors_updated_carries_readout() {
    let mut rig = Rig::start().await;
    let manager = manager(&rig).await;
    let mut signals = manager.receive_sensors_updated().await.unwrap();
    rig.ticks(3).await;
    let mut last = None;
    for _ in 0..3 {
        let signal = timeout(Duration::from_secs(5), signals.next())
            .await
            .expect("a signal for each tick")
            .unwrap();
        last = Some(signal.args().unwrap().channels.clone());
    }
    let channels = last.unwrap();
    // The Curve of pwm1 reads Tctl 61.875 C and gives 63.75%.
    let pwm1 = channels.iter().find(|c| c.0 == "nct6798_pwm1").unwrap();
    assert_eq!(pwm1.4 & 4, 4, "the readout is valid");
    assert_eq!((pwm1.2, pwm1.3), (61.875, 63.75));
    let pwm2 = channels.iter().find(|c| c.0 == "nct6798_pwm2").unwrap();
    assert_eq!(pwm2.4 & 4, 0, "a Hardware curve has no readout");
    assert_eq!((pwm2.2, pwm2.3), (0.0, 0.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bios_snapshot_property_in_restore_order() {
    let mut rig = Rig::start().await;
    rig.ticks(1).await;
    let stored = snapshot::read_snapshot(&StateStore::new(&rig.state)).expect("a snapshot");
    let entry = stored.chips[0]
        .channels
        .iter()
        .find(|c| c.channel == 1)
        .unwrap();
    let (taken_us, attributes) = channel(&rig, "nct6798_pwm1")
        .await
        .bios_snapshot()
        .await
        .unwrap();
    assert_eq!(taken_us, stored.taken_us);
    assert_eq!(attributes, entry.attributes);
    let names: Vec<_> = attributes.iter().map(|a| a.0.as_str()).collect();
    assert_eq!(names[names.len() - 2..], ["pwm1", "pwm1_enable"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bios_snapshot_property_no_entry() {
    let mut rig = rig_with_snapshot(|snapshot| {
        for chip in &mut snapshot.chips {
            chip.channels.retain(|c| c.channel != 2);
        }
    })
    .await;
    rig.ticks(1).await;
    assert_eq!(
        channel(&rig, "nct6798_pwm2")
            .await
            .bios_snapshot()
            .await
            .unwrap(),
        (1, Vec::new())
    );
    assert!(
        !channel(&rig, "nct6798_pwm1")
            .await
            .bios_snapshot()
            .await
            .unwrap()
            .1
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bios_snapshot_property_no_snapshot() {
    // A lease without a snapshot file: the daemon keeps no snapshot.
    let mut rig = Rig::start_with(|_, state| {
        Lease::new(state).create("test-boot", 1).unwrap();
    })
    .await;
    rig.ticks(1).await;
    assert_eq!(
        channel(&rig, "nct6798_pwm1")
            .await
            .bios_snapshot()
            .await
            .unwrap(),
        (0, Vec::new())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fan_sensor_pairs_pwm_and_fan() {
    let mut rig = Rig::start().await;
    rig.ticks(1).await;
    assert_eq!(
        channel(&rig, "nct6798_pwm1")
            .await
            .fan_sensor()
            .await
            .unwrap(),
        path(Kind::Sensor, "nct6798_fan1")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fan_sensor_slash_without_fan() {
    // The test tree has `fan1_input` and no `fan2_input`.
    let mut rig = Rig::start().await;
    rig.ticks(1).await;
    assert_eq!(
        channel(&rig, "nct6798_pwm2")
            .await
            .fan_sensor()
            .await
            .unwrap(),
        none_path()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interfaces_added_and_removed_for_new_objects() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let mut added = objects.receive_interfaces_added().await.unwrap();
    let mut removed = objects.receive_interfaces_removed().await.unwrap();

    let sources = [
        path(Kind::Sensor, "k10temp_temp1"),
        path(Kind::Sensor, "nct6798_temp1"),
    ];
    let sensor = manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", &sources)
        .await
        .unwrap();
    assert_eq!(sensor, path(Kind::Sensor, "derived_1"));
    let signal = timeout(Duration::from_secs(5), added.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().object_path, sensor.as_ref());

    let derived = DerivedSensorProxy::builder(&rig.client)
        .path(sensor.clone())
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(derived.r#type().await.unwrap(), "maximum");
    assert_eq!(derived.sources().await.unwrap(), sources);
    let proxy = SensorProxy::builder(&rig.client)
        .path(sensor.clone())
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(proxy.origin().await.unwrap(), "derived");
    assert_eq!(proxy.name().await.unwrap(), "Hottest");

    derived.delete().await.unwrap();
    let signal = timeout(Duration::from_secs(5), removed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().object_path, sensor.as_ref());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn errors_follow_api_md() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let input = path(Kind::Sensor, "nct6798_temp1");

    // InvalidArgument: a Curve with one point, points out of order, a Duty out of range.
    for points in [vec![(20.0, 10.0)], vec![(40.0, 10.0), (20.0, 50.0)]] {
        let e = pwm1.set_curve(&input, &points, 5.0, 5.0).await.unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    }
    let e = pwm1.set_fixed_duty(120.0).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    let e = pwm1
        .set_curve(&none_path(), &[(1.0, 1.0), (2.0, 2.0)], 5.0, 5.0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));

    // NotFound: an Input sensor that does not exist.
    let missing = path(Kind::Sensor, "nct6798_temp9");
    let e = pwm1
        .set_curve(&missing, &[(20.0, 10.0), (40.0, 50.0)], 5.0, 5.0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NotFound"));

    // InUse: a Derived sensor that a Curve reads.
    let derived = manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", std::slice::from_ref(&input))
        .await
        .unwrap();
    pwm1.set_curve(&derived, &[(20.0, 10.0), (40.0, 50.0)], 0.0, 100.0)
        .await
        .unwrap();
    let derived = DerivedSensorProxy::builder(&rig.client)
        .path(derived)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let e = derived.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    pwm1.set_fixed_duty(50.0).await.unwrap();
    derived.delete().await.unwrap();

    // ReadOnly: a Calibration table preset.
    let e = manager(&rig)
        .await
        .set_calibration_table("alphacool_17558", "x", &[(1.0, 1.0), (2.0, 2.0)])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.ReadOnly"));
    // InUse: the Loop uses the table of a user copy.
    let table = manager(&rig)
        .await
        .set_calibration_table("", "Mine", &[(100.0, 10.0), (200.0, 20.0)])
        .await
        .unwrap();
    assert_eq!(table, "table_1");
    let loop_1 = loop_proxy(&rig, "loop_1").await;
    loop_1
        .set_flow_sensor(&path(Kind::Sensor, "nct6798_fan1"), &table)
        .await
        .unwrap();
    let e = manager(&rig)
        .await
        .delete_calibration_table(&table)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authorization_classes_and_denial() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    pwm1.set_fixed_duty(50.0).await.unwrap();
    // A read method asks nothing.
    manager(&rig).await.get_calibration_tables().await.unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);

    rig.authorizer.deny(Action::Write);
    let e = pwm1.set_fixed_duty(60.0).await.unwrap_err();
    assert_eq!(error_name(&e), "org.freedesktop.DBus.Error.AccessDenied");
    assert_eq!(
        pwm1.fixed_duty().await.unwrap(),
        50.0,
        "a denied call changes nothing"
    );

    // The interactive flag reaches the authorizer only when the call carries it.
    rig.authorizer.allow(Action::Write);
    rig.authorizer.clear();
    let proxy = zbus::Proxy::new(
        &rig.client,
        nicecooling_api::BUS_NAME,
        path(Kind::Channel, "nct6798_pwm1"),
        "io.github.cappyt.NiceCooling1.Channel",
    )
    .await
    .unwrap();
    proxy
        .call_with_flags::<_, _, ()>(
            "SetFixedDuty",
            zbus::proxy::MethodFlags::ALLOW_INTERACTIVE_AUTH,
            &(70.0f64,),
        )
        .await
        .unwrap()
        .unwrap();
    pwm1.set_fixed_duty(80.0).await.unwrap();
    assert_eq!(
        rig.authorizer.calls(),
        [(Action::Write, true), (Action::Write, false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loops_and_their_flow_sensor() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let loop_1 = loop_proxy(&rig, "loop_1").await;
    assert_eq!(loop_1.name().await.unwrap(), "Main loop");
    assert_eq!(
        loop_1.flow().await.unwrap(),
        path(Kind::Sensor, "loop_1_flow")
    );
    assert_eq!(loop_1.pump_floor().await.unwrap(), 40.0);
    assert_eq!(loop_1.leakshield().await.unwrap(), none_path());

    let manager = manager(&rig).await;
    let new_loop = manager.create_loop("Second").await.unwrap();
    assert_eq!(new_loop, path(Kind::Loop, "loop_2"));
    let loop_2 = loop_proxy(&rig, "loop_2").await;
    assert_eq!(loop_2.flow().await.unwrap(), none_path());

    // A Channel drives one Loop only.
    let e = loop_2
        .set_pump_channels(&[path(Kind::Channel, "nct6798_pwm2")])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    loop_2
        .set_pump_channels(&[path(Kind::Channel, "nct6798_pwm1")])
        .await
        .unwrap();
    loop_2
        .set_flow_sensor(&path(Kind::Sensor, "nct6798_fan1"), "alphacool_17558")
        .await
        .unwrap();
    assert_eq!(
        loop_2.flow().await.unwrap(),
        path(Kind::Sensor, "loop_2_flow")
    );
    let flow = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "loop_2_flow"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(flow.origin().await.unwrap(), "calibrated");

    let e = loop_2.set_pump_floor(101.0).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    let e = loop_2
        .set_leakshield(&path(Kind::Leakshield, "leakshield_1_1"))
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NotFound"));
    loop_2
        .set_leakshield(&path(Kind::Leakshield, support::LEAKSHIELD))
        .await
        .unwrap();
    loop_2.set_pump_floor(55.0).await.unwrap();
    assert_eq!(loop_2.pump_floor().await.unwrap(), 55.0);

    loop_2.delete().await.unwrap();
    let ids = manager.get_calibration_tables().await.unwrap();
    assert!(ids.iter().any(|t| t.0 == "alphacool_17558" && t.2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn release_to_hardware_restores_the_snapshot() {
    let mut rig = Rig::start().await;
    rig.ticks(3).await;
    let enable = rig.fake.attr_path("nct6798", "pwm1_enable");
    assert_eq!(std::fs::read_to_string(&enable).unwrap().trim(), "1");
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    pwm1.release_to_hardware().await.unwrap();
    assert_eq!(pwm1.control().await.unwrap(), "hardware");
    assert_eq!(std::fs::read_to_string(&enable).unwrap().trim(), "5");
    let pwm = rig.fake.attr_path("nct6798", "pwm1");
    assert_eq!(std::fs::read_to_string(pwm).unwrap().trim(), "96");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_can_be_set_and_restored() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    assert_eq!(pwm1.name().await.unwrap(), "pwm1");
    pwm1.set_name("Front fan").await.unwrap();
    assert_eq!(pwm1.name().await.unwrap(), "Front fan");
    assert_eq!(pwm1.default_name().await.unwrap(), "pwm1");
    pwm1.set_name("").await.unwrap();
    assert_eq!(pwm1.name().await.unwrap(), "pwm1");
    let sensor = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "k10temp_temp1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    sensor.set_name("CPU").await.unwrap();
    assert_eq!(sensor.name().await.unwrap(), "CPU");
    assert_eq!(sensor.unit().await.unwrap(), "degC");
    assert_eq!(sensor.driver().await.unwrap(), "k10temp");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_curve_refuses_limits_that_panic_or_freeze() {
    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let input = path(Kind::Sensor, "nct6798_temp1");
    let points = [(20.0, 10.0), (40.0, 50.0)];
    for (smoothing, step) in [
        (601.0, 5.0),
        (-1.0, 5.0),
        (5.0, 0.0),
        (5.0, -1.0),
        (f64::NAN, 5.0),
    ] {
        let e = pwm1
            .set_curve(&input, &points, smoothing, step)
            .await
            .unwrap_err();
        assert_eq!(
            error_name(&e),
            format!("{ERR}.InvalidArgument"),
            "{smoothing} {step}"
        );
    }
    pwm1.set_curve(&input, &points, 600.0, 0.5).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_chip_sensor_appears_on_the_bus() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let mut added = objects.receive_interfaces_added().await.unwrap();

    rig.fake.add_hwmon(
        20,
        "asusec_late",
        "devices/platform/asus_ec_sensors_late",
        None,
        &[("name", "asusec_late"), ("temp1_input", "50000")],
    );
    let late = path(Kind::Sensor, "asusec_late_temp1");
    let signal = timeout(Duration::from_secs(5), async {
        loop {
            let signal = added.next().await.unwrap();
            if signal.args().unwrap().object_path == late.as_ref() {
                break signal;
            }
        }
    })
    .await
    .expect("InterfacesAdded for the late sensor");
    drop(signal);
    let sensor = SensorProxy::builder(&rig.client)
        .path(late)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(sensor.id().await.unwrap(), "asusec_late_temp1");
}

/// Runs `n` ticks on a separate task. `true` when a tick panicked.
async fn ticks_panic(rig: &mut Rig, n: u64) -> bool {
    let mut daemon = rig
        .daemon
        .take()
        .expect("the daemon ticks in the background");
    match tokio::spawn(async move {
        daemon.run(Some(n)).await;
        daemon
    })
    .await
    {
        Ok(daemon) => {
            rig.daemon = Some(daemon);
            false
        }
        Err(e) => e.is_panic(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_finite_curve_points_are_refused_and_the_fan_keeps_running() {
    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    let input = path(Kind::Sensor, "k10temp_temp1");
    for id in ["nct6798_pwm1", "nct6798_pwm2"] {
        let channel = channel(&rig, id).await;
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for points in [
                [(bad, 20.0), (80.0, 100.0)],
                [(bad, 50.0), (bad, 60.0)],
                [(20.0, bad), (80.0, 100.0)],
            ] {
                let e = channel
                    .set_curve(&input, &points, 0.0, 100.0)
                    .await
                    .unwrap_err();
                assert_eq!(
                    error_name(&e),
                    format!("{ERR}.InvalidArgument"),
                    "{points:?}"
                );
            }
        }
        assert!(!ticks_panic(&mut rig, 3).await, "the tick panicked");
        assert!(channel.raw().await.unwrap() > 0, "{id} stopped");
    }
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    pwm1.set_fixed_duty(100.0).await.unwrap();
    // The step limit is 5 points for each tick, from the 63.75% of the Curve.
    rig.ticks(10).await;
    assert_eq!(pwm1.raw().await.unwrap(), 255);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_finite_calibration_points_are_refused() {
    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    let manager = manager(&rig).await;
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for points in [
            [(100.0, bad), (200.0, bad)],
            [(bad, 10.0), (200.0, 20.0)],
            [(100.0, 10.0), (bad, 20.0)],
        ] {
            let e = manager
                .set_calibration_table("", "bad", &points)
                .await
                .unwrap_err();
            assert_eq!(
                error_name(&e),
                format!("{ERR}.InvalidArgument"),
                "{points:?}"
            );
        }
    }
    assert!(!ticks_panic(&mut rig, 2).await, "the tick panicked");
    assert!(
        manager
            .get_calibration_tables()
            .await
            .unwrap()
            .iter()
            .all(|t| t.2),
        "a user table was stored"
    );
}

const UNKNOWN_OBJECT: &str = "org.freedesktop.DBus.Error.UnknownObject";

/// The Sensor object of a calibrated flow, or an error name when it is not on the bus.
async fn sensor_name(rig: &Rig, id: &str) -> Result<String, String> {
    SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
        .name()
        .await
        .map_err(|e| error_name(&e))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flow_sensor_read_by_a_curve_cannot_change_or_go() {
    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let flow = path(Kind::Sensor, "loop_1_flow");
    pwm1.set_curve(&flow, &[(0.0, 30.0), (300.0, 100.0)], 0.0, 100.0)
        .await
        .unwrap();
    let loop_1 = loop_proxy(&rig, "loop_1").await;
    let in_use = Err(format!("{ERR}.InUse"));

    let delete = loop_1.delete().await.map_err(|e| error_name(&e));
    assert_eq!(delete, in_use);
    let remove = loop_1
        .set_flow_sensor(&none_path(), "")
        .await
        .map_err(|e| error_name(&e));
    assert_eq!(remove, in_use);
    let change = loop_1
        .set_flow_sensor(&path(Kind::Sensor, "nct6798_temp1"), "alphacool_17558")
        .await
        .map_err(|e| error_name(&e));
    assert_eq!(change, in_use);
    // Nothing changed, and the Curve still reads the flow.
    assert_eq!(loop_1.flow().await.unwrap(), flow);
    assert_eq!(
        loop_1.flow_sensor().await.unwrap(),
        path(Kind::Sensor, "nct6798_fan1")
    );
    assert_eq!(pwm1.curve().await.unwrap().0, flow);
    assert!(sensor_name(&rig, "loop_1_flow").await.is_ok());
    rig.ticks(3).await;
    assert_eq!(pwm1.r#override().await.unwrap(), "none");

    // The same Sensor and table are not a change.
    loop_1
        .set_flow_sensor(&path(Kind::Sensor, "nct6798_fan1"), "alphacool_17558")
        .await
        .unwrap();

    // Without the Curve, the Loop gives up its flow sensor and the object leaves the bus.
    pwm1.set_fixed_duty(50.0).await.unwrap();
    loop_1.set_flow_sensor(&none_path(), "").await.unwrap();
    assert_eq!(loop_1.flow().await.unwrap(), none_path());
    assert_eq!(loop_1.flow_sensor().await.unwrap(), none_path());
    assert_eq!(
        sensor_name(&rig, "loop_1_flow").await,
        Err(UNKNOWN_OBJECT.to_owned())
    );
    // A new flow sensor registers the object again.
    loop_1
        .set_flow_sensor(&path(Kind::Sensor, "nct6798_fan1"), "alphacool_17558")
        .await
        .unwrap();
    assert!(sensor_name(&rig, "loop_1_flow").await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loop_delete_removes_the_flow_sensor_object() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    assert!(sensor_name(&rig, "loop_1_flow").await.is_ok());
    loop_proxy(&rig, "loop_1").await.delete().await.unwrap();
    let mut gone = false;
    for _ in 0..40 {
        if sensor_name(&rig, "loop_1_flow").await == Err(UNKNOWN_OBJECT.to_owned()) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the flow Sensor object stayed on the bus");
}

/// An object whose model is gone answers an error. The handler never panics.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vanished_loop_or_channel_object_answers_an_error() {
    let rig = Rig::start().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let server = rig.server.object_server();
    let ghost_loop = path(Kind::Loop, "loop_99");
    let ghost_channel = path(Kind::Channel, "ghost_pwm9");
    server
        .at(
            ghost_loop.clone(),
            nicecoolingd::dbus::loops::Loop::new(hub.clone(), "loop_99".into()),
        )
        .await
        .unwrap();
    server
        .at(
            ghost_channel.clone(),
            nicecoolingd::dbus::channel::Channel::new(hub, "ghost_pwm9".into()),
        )
        .await
        .unwrap();
    let quick = zbus::connection::Builder::address(rig.bus.address.as_str())
        .method_timeout(Duration::from_secs(3))
        .build()
        .await
        .unwrap();
    let vanished = |e: &zbus::Error| {
        let name = error_name(e);
        name == UNKNOWN_OBJECT || name == format!("{ERR}.NotFound")
    };
    let l = LoopProxy::builder(&quick)
        .path(ghost_loop)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let c = ChannelProxy::builder(&quick)
        .path(ghost_channel)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert!(vanished(&l.name().await.unwrap_err()));
    assert!(vanished(&l.pump_floor().await.unwrap_err()));
    assert!(vanished(&l.set_name("x").await.unwrap_err()));
    assert!(vanished(&l.set_pump_channels(&[]).await.unwrap_err()));
    assert!(vanished(&l.delete().await.unwrap_err()));
    assert!(vanished(&c.name().await.unwrap_err()));
    assert!(vanished(&c.curve().await.unwrap_err()));
    assert!(vanished(&c.set_name("x").await.unwrap_err()));
    // The API still answers.
    assert!(manager(&rig).await.version().await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_loop_delete_answers_every_call() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let quick = zbus::connection::Builder::address(rig.bus.address.as_str())
        .method_timeout(Duration::from_secs(3))
        .build()
        .await
        .unwrap();
    for _ in 0..5 {
        let created = ManagerProxy::builder(&quick)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap()
            .create_loop("x")
            .await
            .unwrap();
        let on = || async {
            LoopProxy::builder(&quick)
                .path(created.clone())
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .unwrap()
        };
        let (a, b) = (on().await, on().await);
        let (da, db, name, floor) = tokio::join!(a.delete(), b.delete(), a.name(), b.pump_floor());
        for e in [da.err(), db.err(), name.err(), floor.err()]
            .into_iter()
            .flatten()
        {
            let name = error_name(&e);
            assert!(
                name.starts_with("org.freedesktop.DBus.Error.Unknown") || name.starts_with(ERR),
                "no D-Bus reply: {name}"
            );
        }
    }
    assert!(manager(&rig).await.version().await.is_ok());
}

/// Starts the daemon on a state directory that holds a lease and a BIOS snapshot, as after a
/// crash. `edit` changes the snapshot that the daemon finds.
async fn rig_with_snapshot(edit: impl FnOnce(&mut BiosSnapshot)) -> Rig {
    Rig::start_with(|fake, state| {
        let discovery =
            nicecooling_hwmon::discovery::discover(&fake.sysfs(), None, &Default::default())
                .unwrap();
        let mut snapshot = snapshot::take(&discovery.channels, Default::default(), 1).unwrap();
        edit(&mut snapshot);
        StateStore::new(state)
            .save(snapshot::SNAPSHOT_FILE, &snapshot)
            .unwrap();
        Lease::new(state).create("test-boot", 1).unwrap();
    })
    .await
}

fn attribute(rig: &Rig, name: &str) -> String {
    std::fs::read_to_string(rig.fake.attr_path("nct6798", name))
        .unwrap()
        .trim()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_restore_keeps_the_control_and_answers_failed() {
    // The restore of pwm1 writes an attribute that the chip does not have.
    let mut rig = rig_with_snapshot(|snapshot| {
        for chip in &mut snapshot.chips {
            for channel in chip.channels.iter_mut().filter(|c| c.channel == 1) {
                channel
                    .attributes
                    .insert(0, ("pwm1_missing".into(), "1".into()));
            }
        }
    })
    .await;
    rig.ticks(3).await;
    assert_eq!(attribute(&rig, "pwm1_enable"), "1");
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let e = pwm1.release_to_hardware().await.unwrap_err();
    assert_eq!(error_name(&e), "org.freedesktop.DBus.Error.Failed");
    assert_eq!(pwm1.control().await.unwrap(), "curve");
    assert_eq!(
        pwm1.curve().await.unwrap().0,
        path(Kind::Sensor, "k10temp_temp1")
    );
    // The failed restore wrote part of the snapshot. The Channel is under the Curve again once
    // the reconcile stage sees the mode change, after its 2 s of driver cache grace.
    let mut manual = false;
    for _ in 0..60 {
        if attribute(&rig, "pwm1_enable") == "1" && attribute(&rig, "pwm1") != "96" {
            manual = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(manual, "the Channel did not return to the Curve");
    assert_eq!(pwm1.control().await.unwrap(), "curve");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_channel_without_a_snapshot_entry_stays_on_its_hardware_curve() {
    let mut rig = rig_with_snapshot(|snapshot| {
        for chip in &mut snapshot.chips {
            chip.channels.retain(|c| c.channel != 1);
        }
    })
    .await;
    rig.ticks(4).await;
    // The Default Profile holds a Curve for pwm1. The daemon does not run it.
    assert_eq!(attribute(&rig, "pwm1_enable"), "5");
    assert_eq!(attribute(&rig, "pwm1"), "96");
    let context = rig.daemon.as_mut().unwrap().runner.context();
    assert!(
        context
            .warnings
            .is_active("no-bios-snapshot", "nct6798_pwm1")
    );
    assert!(
        !context
            .warnings
            .is_active("no-bios-snapshot", "nct6798_pwm2")
    );

    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    assert_eq!(pwm1.control().await.unwrap(), "hardware");
    let input = path(Kind::Sensor, "k10temp_temp1");
    let e = pwm1
        .set_curve(&input, &[(20.0, 10.0), (40.0, 50.0)], 0.0, 100.0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.ReadOnly"));
    let e = pwm1.set_fixed_duty(50.0).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.ReadOnly"));
    // It is on its Hardware curve already: the release has nothing to restore.
    pwm1.release_to_hardware().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(pwm1.control().await.unwrap(), "hardware");
    assert_eq!(attribute(&rig, "pwm1_enable"), "5");
    assert_eq!(attribute(&rig, "pwm1"), "96");
    // The other Channels are not affected.
    channel(&rig, "nct6798_pwm2")
        .await
        .set_fixed_duty(60.0)
        .await
        .unwrap();
}

fn invalid() -> Result<(), String> {
    Err(format!("{ERR}.InvalidArgument"))
}

fn ok_or_name<T>(r: zbus::Result<T>) -> Result<(), String> {
    r.map(drop).map_err(|e| error_name(&e))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn point_and_name_limits_are_refused_at_the_boundary() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    let manager = manager(&rig).await;
    let input = path(Kind::Sensor, "k10temp_temp1");

    // A Curve has 2 to 8 points.
    let curve = |n: usize| -> Vec<(f64, f64)> { (0..n).map(|i| (i as f64, 50.0)).collect() };
    assert_eq!(
        ok_or_name(pwm1.set_curve(&input, &curve(9), 0.0, 5.0).await),
        invalid()
    );
    assert_eq!(
        ok_or_name(pwm1.set_curve(&input, &curve(1), 0.0, 5.0).await),
        invalid()
    );
    assert_eq!(
        ok_or_name(pwm1.set_curve(&input, &curve(8), 0.0, 5.0).await),
        Ok(())
    );
    assert_eq!(
        ok_or_name(pwm1.set_curve(&input, &curve(2), 0.0, 5.0).await),
        Ok(())
    );

    // A Calibration table has 2 to 64 points.
    let table = |n: usize| -> Vec<(f64, f64)> { (1..=n).map(|i| (i as f64, i as f64)).collect() };
    assert_eq!(
        ok_or_name(manager.set_calibration_table("", "t", &table(65)).await),
        invalid()
    );
    assert_eq!(
        ok_or_name(manager.set_calibration_table("", "t", &table(1)).await),
        invalid()
    );
    assert_eq!(
        ok_or_name(manager.set_calibration_table("", "t", &table(64)).await),
        Ok(())
    );

    // A name has up to 64 characters, not bytes.
    let (long, edge, wide) = ("x".repeat(65), "x".repeat(64), "\u{e9}".repeat(64));
    let sensor = SensorProxy::builder(&rig.client)
        .path(input.clone())
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let l = loop_proxy(&rig, "loop_1").await;
    assert_eq!(ok_or_name(pwm1.set_name(&long).await), invalid());
    assert_eq!(ok_or_name(sensor.set_name(&long).await), invalid());
    assert_eq!(ok_or_name(l.set_name(&long).await), invalid());
    assert_eq!(ok_or_name(manager.create_loop(&long).await), invalid());
    assert_eq!(
        ok_or_name(
            manager
                .create_derived_sensor(&long, "maximum", std::slice::from_ref(&input))
                .await
        ),
        invalid()
    );
    assert_eq!(
        ok_or_name(manager.set_calibration_table("", &long, &table(2)).await),
        invalid()
    );
    assert_eq!(ok_or_name(pwm1.set_name(&edge).await), Ok(()));
    assert_eq!(ok_or_name(pwm1.set_name(&wide).await), Ok(()));
    assert_eq!(pwm1.name().await.unwrap(), wide);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn object_count_limits_are_refused_at_the_boundary() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let manager = manager(&rig).await;
    let input = path(Kind::Sensor, "k10temp_temp1");

    // 16 Loops: loop_1 exists.
    for _ in 1..16 {
        manager.create_loop("l").await.unwrap();
    }
    assert_eq!(ok_or_name(manager.create_loop("l").await), invalid());
    loop_proxy(&rig, "loop_16").await.delete().await.unwrap();
    manager.create_loop("l").await.unwrap();

    // 64 Derived sensors.
    for _ in 0..64 {
        manager
            .create_derived_sensor("d", "maximum", std::slice::from_ref(&input))
            .await
            .unwrap();
    }
    assert_eq!(
        ok_or_name(
            manager
                .create_derived_sensor("d", "maximum", std::slice::from_ref(&input))
                .await
        ),
        invalid()
    );

    // 32 user Calibration tables. The presets do not count.
    for _ in 0..32 {
        manager
            .set_calibration_table("", "t", &[(1.0, 1.0), (2.0, 2.0)])
            .await
            .unwrap();
    }
    assert_eq!(
        ok_or_name(
            manager
                .set_calibration_table("", "t", &[(1.0, 1.0), (2.0, 2.0)])
                .await
        ),
        invalid()
    );
    // A write to an existing table is not a new one.
    manager
        .set_calibration_table("table_1", "again", &[(1.0, 1.0), (3.0, 3.0)])
        .await
        .unwrap();
}
