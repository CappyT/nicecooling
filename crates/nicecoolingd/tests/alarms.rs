//! The alarm responses and the poweroff on a private bus, with a mock logind. The tick runs on a
//! simulated clock, so a countdown of 30 s takes no real time.

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::alarms::AlarmsProxy;
use support::{Rig, error_name};
use tokio::time::timeout;

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const PUMP_STALL: &str = "leakshield.system-alarm";
const PUMP_STALL_BIT: u32 = 1 << 8;
const LEAK_WARNING_BIT: u32 = 1 << 2;

async fn alarms(rig: &Rig) -> AlarmsProxy<'static> {
    AlarmsProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn raise(rig: &Rig, bits: u32) {
    rig.sim.edit_report1(|r| r.alarm_bits = bits);
}

/// A rig on a simulated clock whose Leakshield has raised `bits`, with the first tick done.
async fn raised(bits: u32) -> Rig {
    let mut rig = Rig::start_with_clock().await;
    raise(&rig, bits);
    rig.step(1).await;
    rig
}

async fn active(
    proxy: &AlarmsProxy<'_>,
) -> Vec<(String, String, String, String, String, u64, u64)> {
    proxy
        .active()
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.0, r.1, r.2, r.3, r.4, r.5, r.6))
        .collect()
}

async fn set_response(proxy: &AlarmsProxy<'_>, event: &str, response: &str, delay_s: u32) {
    proxy.set_response(event, response, delay_s).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_responses() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let rows = proxy.get_responses().await.unwrap();
    // One row for each event and group, not for each alarm bit.
    assert_eq!(rows.len(), 7 + 7);
    for id in [
        "leakshield.leak-alarm-small",
        "leakshield.leak-alarm",
        "leakshield.system-alarm",
    ] {
        assert!(rows.iter().any(|r| r.0 == id), "{id}");
    }
    assert!(rows.iter().all(|r| !r.0.contains("pump-stall")));
    for (event, response, delay) in &rows {
        let expected = if event == "pump-stop" || event == "leakshield.leak-alarm" {
            "poweroff"
        } else {
            "notify"
        };
        assert_eq!(response, expected, "{event}");
        assert_eq!(*delay, 30, "{event}");
    }
    assert!(
        rows.iter()
            .any(|r| r.0 == "leakshield-unreachable" && r.1 == "notify")
    );
    // A user response replaces the default, and a bad one is refused.
    set_response(&proxy, "pump-stop", "none", 0).await;
    let rows = proxy.get_responses().await.unwrap();
    assert!(rows.contains(&("pump-stop".into(), "none".into(), 0)));
    let e = proxy
        .set_response("pump-stop", "reboot", 0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    let e = proxy.set_response("no-event", "none", 0).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NotFound"));
}

/// The `bits` detail of an active row, and its event.
async fn event_and_bits(proxy: &AlarmsProxy<'_>) -> Vec<(String, u32)> {
    proxy
        .active()
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let bits = row
                .7
                .get("bits")
                .map(|v| u32::try_from(v.try_clone().unwrap()).unwrap())
                .unwrap_or(0);
            (row.1, bits)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_bit_raises_its_group() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let expected = [
        "fill-level-low-warning",
        "fill-level-low-alarm",
        "leak-warning",
        "pump-idle-time-warning",
        "leak-alarm-small",
        "leak-alarm",
        "leak-alarm",
        "system-alarm",
        "system-alarm",
        "system-alarm",
        "system-alarm",
    ];
    for (bit, name) in expected.into_iter().enumerate() {
        raise(&rig, 1 << bit);
        rig.step(2).await;
        assert_eq!(
            event_and_bits(&proxy).await,
            [(format!("leakshield.{name}"), 1 << bit)],
            "bit {bit}"
        );
        raise(&rig, 0);
        rig.step(2).await;
        assert!(active(&proxy).await.is_empty(), "bit {bit}");
    }
    // No bit powers off, except the large leak.
    rig.step(40).await;
    assert_eq!(rig.logind.call_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bits_5_and_6_raise_one_large_leak() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let mut raised_signals = proxy.receive_alarm_raised().await.unwrap();
    raise(&rig, 1 << 5);
    rig.step(2).await;
    assert_eq!(
        event_and_bits(&proxy).await,
        [("leakshield.leak-alarm".to_owned(), 1 << 5)]
    );
    let instance = active(&proxy).await[0].0.clone();
    // The second bit joins the instance. It does not raise another one.
    raise(&rig, (1 << 5) | (1 << 6));
    rig.step(2).await;
    assert_eq!(
        event_and_bits(&proxy).await,
        [("leakshield.leak-alarm".to_owned(), (1 << 5) | (1 << 6))]
    );
    assert_eq!(active(&proxy).await[0].0, instance);
    let signal = timeout(Duration::from_secs(5), raised_signals.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().event, "leakshield.leak-alarm");
    assert!(
        timeout(Duration::from_millis(300), raised_signals.next())
            .await
            .is_err(),
        "one AlarmRaised signal for the group"
    );
    // One bit goes: the instance stays. Both go: it clears.
    raise(&rig, 1 << 6);
    rig.step(2).await;
    assert_eq!(active(&proxy).await[0].0, instance);
    raise(&rig, 0);
    rig.step(2).await;
    assert!(active(&proxy).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_leak_powers_off_after_30_s_by_default() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let mut raised_signals = proxy.receive_alarm_raised().await.unwrap();
    // The user changed nothing. The first tick with the bit set is at t = 1 s.
    raise(&rig, 1 << 6);
    rig.step(1).await;
    let signal = timeout(Duration::from_secs(5), raised_signals.next())
        .await
        .unwrap()
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!(
        (args.event.as_str(), args.response.as_str()),
        ("leakshield.leak-alarm", "poweroff")
    );
    assert!(args.poweroff_at_us > 0);
    rig.step(29).await;
    assert_eq!(rig.logind.call_count(), 0);
    rig.step(2).await;
    assert_eq!(*rig.logind.calls.lock().unwrap(), [0x10]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn small_leak_and_system_alarm_only_notify_by_default() {
    let mut rig = Rig::start_with_clock().await;
    raise(&rig, (1 << 4) | (1 << 7));
    rig.step(40).await;
    assert_eq!(rig.logind.call_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notify_only_does_not_power_off() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let mut raised_signals = proxy.receive_alarm_raised().await.unwrap();
    raise(&rig, PUMP_STALL_BIT | LEAK_WARNING_BIT);
    rig.step(2).await;

    let mut seen = Vec::new();
    for _ in 0..2 {
        let signal = timeout(Duration::from_secs(5), raised_signals.next())
            .await
            .expect("an AlarmRaised signal")
            .unwrap();
        let args = signal.args().unwrap();
        assert_eq!(args.response, "notify");
        assert_eq!(args.poweroff_at_us, 0);
        seen.push((args.event.to_string(), args.severity.to_string()));
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            ("leakshield.leak-warning".into(), "warning".into()),
            (PUMP_STALL.into(), "alarm".into())
        ]
    );
    rig.step(35).await;
    assert_eq!(rig.logind.call_count(), 0);
    assert_eq!(active(&proxy).await.len(), 2);
    // The bits go: both events clear.
    raise(&rig, 0);
    rig.step(2).await;
    assert!(active(&proxy).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expiry_calls_poweroff_with_flags_0x10() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 5).await;
    let mut raised_signals = proxy.receive_alarm_raised().await.unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(1).await;
    let signal = timeout(Duration::from_secs(5), raised_signals.next())
        .await
        .unwrap()
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!(args.response, "poweroff");
    assert!(args.poweroff_at_us > 0);
    rig.step(4).await;
    assert_eq!(rig.logind.call_count(), 0);
    rig.step(2).await;
    assert_eq!(*rig.logind.calls.lock().unwrap(), [0x10]);
    // logind accepted the call: no second call while the Alarm stays.
    rig.step(10).await;
    assert_eq!(rig.logind.call_count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_before_expiry() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 10).await;
    let mut cancelled = proxy.receive_poweroff_cancelled().await.unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(3).await;
    let rows = active(&proxy).await;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].6 > 0);
    proxy.cancel_poweroff(&rows[0].0).await.unwrap();
    let signal = timeout(Duration::from_secs(5), cancelled.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().instance, rows[0].0);
    // The Alarm stays active, with no poweroff pending.
    let rows = active(&proxy).await;
    assert_eq!((rows.len(), rows[0].6), (1, 0));
    rig.step(20).await;
    assert_eq!(rig.logind.call_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_from_second_caller() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 10).await;
    raise(&rig, PUMP_STALL_BIT);
    rig.step(3).await;
    let instance = active(&proxy).await[0].0.clone();
    // The app of another session is another connection.
    let other = rig.bus.connect().await;
    let other = AlarmsProxy::new(&other).await.unwrap();
    other.cancel_poweroff(&instance).await.unwrap();
    rig.step(15).await;
    assert_eq!(rig.logind.call_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_without_pending_returns_no_poweroff() {
    let mut rig = raised(PUMP_STALL_BIT).await;
    let proxy = alarms(&rig).await;
    // The default response of the event is notify: nothing is pending.
    let instance = active(&proxy).await[0].0.clone();
    let e = proxy.cancel_poweroff(&instance).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NoPoweroff"));
    let e = proxy.cancel_poweroff("alarm_99").await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NoPoweroff"));
    rig.step(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocked_call_emits_poweroff_failed_and_retries_every_5s() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 2).await;
    rig.logind.fail.store(true, Ordering::SeqCst);
    let mut failed = proxy.receive_poweroff_failed().await.unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(4).await;
    assert_eq!(rig.logind.call_count(), 1);
    let signal = timeout(Duration::from_secs(5), failed.next())
        .await
        .expect("a PoweroffFailed signal")
        .unwrap();
    assert!(signal.args().unwrap().error.contains("inhibited"));
    // The Alarm rose at t = 1 s and the call failed at t = 3 s. The next one is at t = 8 s.
    rig.step(3).await;
    assert_eq!(rig.logind.call_count(), 1);
    rig.step(1).await;
    assert_eq!(rig.logind.call_count(), 2);
    timeout(Duration::from_secs(5), failed.next())
        .await
        .unwrap()
        .unwrap();
    rig.step(4).await;
    assert_eq!(rig.logind.call_count(), 2);
    // The inhibitor goes: the third call at t = 13 s goes through, and the retries stop.
    rig.logind.fail.store(false, Ordering::SeqCst);
    rig.step(1).await;
    assert_eq!(rig.logind.call_count(), 3);
    rig.step(20).await;
    assert_eq!(rig.logind.call_count(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_restarts_countdown() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 10).await;
    raise(&rig, PUMP_STALL_BIT);
    // The Alarm rises at t = 1 s: the first countdown would end at t = 11 s.
    rig.step(7).await;
    assert_eq!(rig.logind.call_count(), 0);

    rig.restart_daemon().await;
    let mut effects = rig
        .daemon
        .as_ref()
        .unwrap()
        .hub
        .take_alarm_receiver()
        .expect("the new daemon is not on the bus");
    let mut calls = 0;
    // The new daemon sees the Alarm again at t = 8 s. Its countdown ends at t = 18 s.
    for second in 8..=18 {
        rig.step(1).await;
        while let Ok(effect) = effects.try_recv() {
            if matches!(
                effect,
                nicecoolingd::alarms::poweroff::Effect::CallPoweroff(_)
            ) {
                calls += 1;
                assert_eq!(second, 18, "the call comes at the end of the full delay");
            }
        }
    }
    assert_eq!(calls, 1);
}

const UNREACHABLE: &str = "leakshield-unreachable";

/// The Loop of the rig sends its Feed message to the Leakshield.
fn loop_with_leakshield(_: &nicecooling_hwmon::fake::FakeSysfs, state: &std::path::Path) {
    let text = support::STATE.replace(
        "\"leakshield\": null",
        &format!("\"leakshield\": \"{}\"", support::LEAKSHIELD),
    );
    std::fs::write(state.join("state.json"), text).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unreachable_rises_when_the_device_disconnects_and_closes_when_it_returns() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let mut raised_signals = proxy.receive_alarm_raised().await.unwrap();
    rig.step(3).await;
    assert!(active(&proxy).await.is_empty());
    rig.sim.disconnect();
    rig.step(8).await;
    assert!(active(&proxy).await.is_empty());
    rig.step(3).await;
    let rows = active(&proxy).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].1.as_str(),
            rows[0].2.as_str(),
            rows[0].3.as_str(),
            rows[0].4.as_str()
        ),
        (UNREACHABLE, "alarm", support::LEAKSHIELD, "notify")
    );
    let signal = timeout(Duration::from_secs(5), raised_signals.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().event, UNREACHABLE);
    rig.step(30).await;
    assert_eq!(rig.logind.call_count(), 0);
    rig.sim.reconnect();
    rig.step(8).await;
    assert_eq!(active(&proxy).await.len(), 1);
    rig.step(8).await;
    assert!(active(&proxy).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unreachable_rises_when_the_feed_message_fails() {
    let mut rig = Rig::start_with_clock_and(loop_with_leakshield).await;
    let proxy = alarms(&rig).await;
    rig.step(3).await;
    assert!(active(&proxy).await.is_empty());
    rig.sim.fail_bulk(true);
    rig.step(8).await;
    assert!(active(&proxy).await.is_empty());
    rig.step(4).await;
    let rows = active(&proxy).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, UNREACHABLE);
    rig.sim.fail_bulk(false);
    rig.step(14).await;
    assert!(active(&proxy).await.is_empty());
}
