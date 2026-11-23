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
    assert_eq!(pwm1.2 & 2, 0, "pwm1 is on a Curve");
    let pwm2 = last.2.iter().find(|c| c.0 == "nct6798_pwm2").unwrap();
    assert_eq!(pwm2.2 & 2, 2, "pwm2 is on its Hardware curve");
    assert!(received.windows(2).all(|w| w[0].0 <= w[1].0));
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
