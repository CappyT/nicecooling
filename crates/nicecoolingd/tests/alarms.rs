//! The alarm responses and the poweroff on a private bus, with a mock logind. The tick runs on a
//! simulated clock, so a countdown of 30 s takes no real time.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::alarms::AlarmsProxy;
use nicecooling_api::errors::Error;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecoolingd::alarms::logind::{CallFuture, Login1, ZbusLogin1};
use nicecoolingd::dbus::alarms::run_effects;
use support::{PrivateBus, Rig, error_name};
use tokio::time::timeout;

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const PUMP_STALL: &str = "leakshield.system-alarm";
const PUMP_STALL_BIT: u32 = 1 << 8;
const LEAK_BIT: u32 = 1 << 6;
const LEAK_WARNING_BIT: u32 = 1 << 2;

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(support::path(Kind::Leakshield, support::LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

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
    proxy
        .set_response(event, "", response, delay_s)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_responses() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let rows = proxy.get_responses().await.unwrap();
    // One row for each event and group, not for each alarm bit, and one row of the Leakshield for
    // each event of the Leakshield.
    assert_eq!(rows.len(), 8 + 7 + 7);
    let rows: Vec<_> = rows.into_iter().filter(|r| r.1.is_empty()).collect();
    assert_eq!(rows.len(), 8 + 7);
    for id in [
        "leakshield.leak-alarm-small",
        "leakshield.leak-alarm",
        "leakshield.system-alarm",
    ] {
        assert!(rows.iter().any(|r| r.0 == id), "{id}");
    }
    assert!(rows.iter().all(|r| !r.0.contains("pump-stall")));
    for (event, _, response, delay, overridden) in &rows {
        let expected = if event == "pump-stop" || event == "leakshield.leak-alarm" {
            "poweroff"
        } else {
            "notify"
        };
        assert_eq!(response, expected, "{event}");
        assert_eq!((*delay, *overridden), (30, false), "{event}");
    }
    assert!(
        rows.iter()
            .any(|r| r.0 == "leakshield-unreachable" && r.2 == "notify")
    );
    // A user response replaces the default, and a bad one is refused.
    set_response(&proxy, "pump-stop", "none", 0).await;
    let rows = proxy.get_responses().await.unwrap();
    // The default differs from the built-in default: the row says so.
    assert!(rows.contains(&("pump-stop".into(), "".into(), "none".into(), 0, true)));
    let e = proxy
        .set_response("pump-stop", "", "reboot", 0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    let e = proxy
        .set_response("no-event", "", "none", 0)
        .await
        .unwrap_err();
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

/// A logind that accepts the call after `delay`. `started` and `accepted` count the calls.
struct SlowLogin1 {
    started: Arc<AtomicUsize>,
    accepted: Arc<AtomicUsize>,
    delay: Duration,
}

impl Login1 for SlowLogin1 {
    fn power_off_with_flags(&self, _flags: u64) -> CallFuture {
        let (started, accepted, delay) = (self.started.clone(), self.accepted.clone(), self.delay);
        Box::pin(async move {
            started.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            accepted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

/// A logind on a private bus that never answers `PowerOffWithFlags`.
struct HangingLogin1(Arc<AtomicUsize>);

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl HangingLogin1 {
    async fn power_off_with_flags(&self, _flags: u64) -> zbus::fdo::Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await;
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn instance_ids_stay_unique_across_a_restart() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    set_response(&proxy, PUMP_STALL, "poweroff", 30).await;
    raise(&rig, PUMP_STALL_BIT);
    rig.step(2).await;
    let old_system = active(&proxy).await[0].0.clone();
    assert!(old_system.starts_with("alarm_"));
    // A large leak starts, and the daemon restarts. The app still shows the old notification.
    raise(&rig, PUMP_STALL_BIT | LEAK_BIT);
    rig.restart_daemon().await;
    rig.step(1).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let rows = hub.alarm_rows();
    assert!(rows.iter().all(|r| r.instance != old_system), "{rows:?}");
    // The cancel of the old notification must not touch the countdown of the large leak.
    assert!(matches!(
        hub.cancel_poweroff(&old_system),
        Err(Error::NoPoweroff(_))
    ));
    let leak = hub
        .alarm_rows()
        .into_iter()
        .find(|r| r.event == "leakshield.leak-alarm")
        .unwrap();
    assert_ne!(leak.poweroff_at_us, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_during_the_logind_call_fails_with_poweroff_started() {
    let mut rig = Rig::start_with_clock().await;
    // The restarted daemon is not on the bus: the test runs its effects with its own logind.
    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let started = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::new(AtomicUsize::new(0));
    hub.set_login1(Arc::new(SlowLogin1 {
        started: started.clone(),
        accepted: accepted.clone(),
        delay: Duration::from_secs(2),
    }));
    let rx = hub.take_alarm_receiver().unwrap();
    tokio::spawn(run_effects(rig.server.clone(), hub.clone(), rx));
    hub.set_alarm_response(PUMP_STALL, "", "poweroff", 2)
        .unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(1).await;
    let instance = hub.alarm_rows()[0].instance.clone();
    rig.step(2).await;
    assert_eq!(started.load(Ordering::SeqCst), 1, "the call is under way");
    assert_eq!(accepted.load(Ordering::SeqCst), 0);
    // The user cancels while logind handles the call: it is too late, and the answer says so.
    let cancel = hub.cancel_poweroff(&instance);
    assert!(
        matches!(cancel, Err(Error::PoweroffStarted(_))),
        "{cancel:?}"
    );
    assert_ne!(hub.alarm_rows()[0].poweroff_at_us, 0);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    // After logind accepted the call, the poweroff has started too.
    assert!(matches!(
        hub.cancel_poweroff(&instance),
        Err(Error::PoweroffStarted(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_logind_that_never_answers_fails_the_call_and_retries() {
    let mut rig = Rig::start_with_clock().await;
    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let bus = PrivateBus::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let mock = bus.connect().await;
    mock.object_server()
        .at("/org/freedesktop/login1", HangingLogin1(calls.clone()))
        .await
        .unwrap();
    mock.request_name("org.freedesktop.login1").await.unwrap();
    hub.set_login1(Arc::new(ZbusLogin1::new(bus.connect().await)));
    let rx = hub.take_alarm_receiver().unwrap();
    tokio::spawn(run_effects(rig.server.clone(), hub.clone(), rx));
    hub.set_alarm_response(PUMP_STALL, "", "poweroff", 2)
        .unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(4).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // The call times out after 10 s of real time and counts as failed.
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.step(6).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the failed call is retried"
    );
}

/// The device holds a latched bit until `reset_alarm`: a bit that flickers in `alarm_bits` keeps
/// one instance, and its countdown runs on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flickering_bit_with_a_latched_bit_keeps_one_countdown() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    // The default response of the large leak is poweroff after 30 s.
    rig.sim.set_alarm_bits(LEAK_BIT);
    rig.step(25).await;
    let rows = active(&proxy).await;
    assert_eq!(rows.len(), 1);
    let instance = rows[0].0.clone();
    assert_eq!(rig.logind.call_count(), 0);

    for cycle in 0..4 {
        // The bit goes for 1 s while the latched bit stays.
        rig.sim.edit_report1(|r| r.alarm_bits = 0);
        rig.step(1).await;
        let rows = active(&proxy).await;
        assert_eq!(rows.len(), 1, "cycle {cycle}");
        assert_eq!(rows[0].0, instance, "cycle {cycle}");
        rig.sim.set_alarm_bits(LEAK_BIT);
        rig.step(25).await;
        let rows = active(&proxy).await;
        assert_eq!(rows.len(), 1, "cycle {cycle}");
        assert_eq!(rows[0].0, instance, "cycle {cycle}");
    }
    // The Alarm rose at t = 1 s, so the call came at t = 31 s, and only once.
    assert_eq!(*rig.logind.calls.lock().unwrap(), [0x10]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_latched_bit_alone_keeps_the_alarm_and_closes_when_the_latch_clears() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    rig.sim.set_alarm_bits(LEAK_WARNING_BIT);
    rig.step(2).await;
    rig.sim.edit_report1(|r| r.alarm_bits = 0);
    rig.step(5).await;
    assert_eq!(
        event_and_bits(&proxy).await,
        [("leakshield.leak-warning".to_owned(), 0)]
    );
    let details = &proxy.active().await.unwrap()[0].7;
    assert!(!details.contains_key("bits"));
    assert_eq!(
        u32::try_from(details["latched_bits"].try_clone().unwrap()).unwrap(),
        LEAK_WARNING_BIT
    );
    rig.sim.edit_report1(|r| r.alarm_bits_latched = 0);
    rig.step(2).await;
    assert!(active(&proxy).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_alarms_closes_the_alarm_and_stops_the_countdown() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let device = leakshield(&rig).await;
    rig.sim.set_alarm_bits(LEAK_BIT);
    rig.step(12).await;
    assert_ne!(active(&proxy).await[0].6, 0, "a poweroff is pending");
    // The current bit goes, and the latch holds the Alarm.
    rig.sim.edit_report1(|r| r.alarm_bits = 0);
    rig.step(3).await;
    assert_eq!(active(&proxy).await.len(), 1);

    device.reset_alarms().await.unwrap();
    rig.step(3).await;

    assert_eq!(rig.sim.commands().last().map(|c| c.command), Some(11));
    let report1 = rig.sim.report1();
    assert_eq!((report1.alarm_bits, report1.alarm_bits_latched), (0, 0));
    assert!(active(&proxy).await.is_empty());
    rig.step(60).await;
    assert_eq!(rig.logind.call_count(), 0);
}

// The Alarm history (design D18).

const CLEAR: fn(&mut nicecooling_protocol::report1::Report1) = |r| {
    r.alarm_bits = 0;
    r.alarm_bits_latched = 0;
};

async fn history(proxy: &AlarmsProxy<'_>) -> Vec<nicecooling_api::alarms::HistoryRow> {
    proxy.get_history(0, 0).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_row_on_raise_and_clear() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    rig.sim.set_alarm_bits(LEAK_WARNING_BIT);
    rig.step(2).await;

    let rows = history(&proxy).await;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(
        (
            row.1.as_str(),
            row.2.as_str(),
            row.4.as_str(),
            row.7.as_str()
        ),
        ("leakshield.leak-warning", "warning", "notify", "notified")
    );
    assert_eq!(row.3, support::LEAKSHIELD);
    assert_ne!(row.5, 0, "raised_us");
    assert_eq!(row.6, 0, "active");
    assert!(!row.8.contains_key("closed_by_restart"));
    assert_eq!(
        u32::try_from(row.8["bits"].try_clone().unwrap()).unwrap(),
        LEAK_WARNING_BIT
    );

    rig.sim.edit_report1(CLEAR);
    rig.step(3).await;
    let rows = history(&proxy).await;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].6 >= rows[0].5, "cleared_us after raised_us");
    assert_ne!(rows[0].6, 0);
    // A `since_us` after the clear leaves the row out.
    assert!(
        proxy
            .get_history(rows[0].6 + 1, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(proxy.get_history(rows[0].6, 0).await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_survives_restart() {
    let mut rig = Rig::start_with_clock().await;
    rig.sim.set_alarm_bits(LEAK_WARNING_BIT);
    rig.step(2).await;
    rig.sim.edit_report1(CLEAR);
    rig.step(3).await;
    let before = rig.daemon.as_ref().unwrap().hub.alarm_history(0, 0);
    assert_eq!(before.len(), 1);

    rig.restart_daemon().await;

    let after = rig.daemon.as_ref().unwrap().hub.alarm_history(0, 0);
    assert_eq!(after, before);
    assert!(rig.state.join("alarm-history.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_row_closed_by_restart() {
    let mut rig = Rig::start_with_clock().await;
    rig.sim.set_alarm_bits(LEAK_WARNING_BIT);
    rig.step(2).await;
    let old = rig.daemon.as_ref().unwrap().hub.alarm_history(0, 0);
    assert_eq!(old[0].cleared_us, 0);

    // The daemon restarts while the Warning is active.
    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let closed = hub.alarm_history(0, 0);
    assert_eq!(closed.len(), 1);
    assert!(closed[0].closed_by_restart);
    assert!(closed[0].cleared_us >= closed[0].raised_us);
    assert_eq!(closed[0].outcome, "notified");

    // The Warning is still active: it rises again as a new instance.
    rig.step(2).await;
    let rows = hub.alarm_history(0, 0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cleared_us, 0);
    assert!(!rows[0].closed_by_restart);
    assert_ne!(rows[0].instance, rows[1].instance);
    assert!(rows[1].closed_by_restart);
}

/// A logind that copies the history file at the time of the call.
struct FileLogin1 {
    file: std::path::PathBuf,
    seen: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Login1 for FileLogin1 {
    fn power_off_with_flags(&self, _flags: u64) -> CallFuture {
        let text = std::fs::read_to_string(&self.file).unwrap_or_default();
        self.seen.lock().unwrap().push(text);
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poweroff_requested_on_disk_before_logind_call() {
    let mut rig = Rig::start_with_clock().await;
    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    hub.set_login1(Arc::new(FileLogin1 {
        file: rig.state.join("alarm-history.json"),
        seen: seen.clone(),
    }));
    let rx = hub.take_alarm_receiver().unwrap();
    tokio::spawn(run_effects(rig.server.clone(), hub.clone(), rx));
    hub.set_alarm_response(PUMP_STALL, "", "poweroff", 2)
        .unwrap();
    raise(&rig, PUMP_STALL_BIT);

    rig.step(5).await;
    for _ in 0..100 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one call to logind");
    assert!(
        seen[0].contains("\"outcome\": \"poweroff-requested\""),
        "the file had no poweroff-requested row at the call: {}",
        seen[0]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_outcome_cancelled() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    // The default response of the large leak is poweroff after 30 s.
    rig.sim.set_alarm_bits(LEAK_BIT);
    rig.step(5).await;
    let rows = history(&proxy).await;
    assert_eq!(rows[0].7, "poweroff-pending");

    proxy.cancel_poweroff(&rows[0].0).await.unwrap();

    assert_eq!(history(&proxy).await[0].7, "poweroff-cancelled");
    // The Alarm clears: a final outcome does not change.
    rig.sim.edit_report1(CLEAR);
    rig.step(3).await;
    let rows = history(&proxy).await;
    assert_eq!(rows[0].7, "poweroff-cancelled");
    assert_ne!(rows[0].6, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_outcome_stopped() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    rig.sim.set_alarm_bits(LEAK_BIT);
    rig.step(5).await;
    assert_eq!(history(&proxy).await[0].7, "poweroff-pending");

    rig.sim.edit_report1(CLEAR);
    rig.step(3).await;

    let rows = history(&proxy).await;
    assert_eq!(rows[0].7, "poweroff-stopped");
    assert_ne!(rows[0].6, 0);
    assert_eq!(rig.logind.call_count(), 0);
}

/// A logind that fails the first call and accepts the next.
struct FlakyLogin1(AtomicUsize);

impl Login1 for FlakyLogin1 {
    fn power_off_with_flags(&self, _flags: u64) -> CallFuture {
        let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
        Box::pin(async move {
            if first {
                Err("blocked".to_owned())
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_outcome_failed_then_retry() {
    let mut rig = Rig::start_with_clock().await;
    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    hub.set_login1(Arc::new(FlakyLogin1(AtomicUsize::new(0))));
    let rx = hub.take_alarm_receiver().unwrap();
    tokio::spawn(run_effects(rig.server.clone(), hub.clone(), rx));
    hub.set_alarm_response(PUMP_STALL, "", "poweroff", 2)
        .unwrap();
    raise(&rig, PUMP_STALL_BIT);
    rig.step(4).await;
    for _ in 0..100 {
        if hub.alarm_history(0, 0)[0].outcome == "poweroff-failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(hub.alarm_history(0, 0)[0].outcome, "poweroff-failed");

    // The retry comes 5 s after the failure, and logind accepts it.
    rig.step(8).await;
    for _ in 0..100 {
        if hub.alarm_history(0, 0)[0].outcome == "poweroff-requested" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(hub.alarm_history(0, 0)[0].outcome, "poweroff-requested");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_closed_row_carries_closed_by_restart() {
    // The state directory holds a row that an earlier daemon left open.
    let rig = Rig::start_with(|_, state| {
        std::fs::write(
            state.join("alarm-history.json"),
            r#"{"schema_version": 1, "rows": [{"instance": "alarm_1_1",
                "event": "pump-stop", "severity": "alarm", "source": "loop_1",
                "response": "poweroff", "raised_us": 1000, "cleared_us": 0,
                "outcome": "poweroff-pending", "closed_by_restart": false, "details": {}}]}"#,
        )
        .unwrap();
    })
    .await;
    let proxy = alarms(&rig).await;

    let rows = history(&proxy).await;

    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0].6, 0, "the restart closed the row");
    // The outcome of the row does not change: the restart is not a clear of the instance.
    assert_eq!(rows[0].7, "poweroff-pending");
    assert!(bool::try_from(rows[0].8["closed_by_restart"].try_clone().unwrap()).unwrap());
}

// The alarm response per Leakshield (design D18).

const LARGE: &str = "leakshield.leak-alarm";

/// The `GetResponses` row of an event for a source.
async fn response_row(proxy: &AlarmsProxy<'_>, event: &str, source: &str) -> (String, u32, bool) {
    let rows = proxy.get_responses().await.unwrap();
    let row = rows
        .into_iter()
        .find(|r| r.0 == event && r.1 == source)
        .unwrap_or_else(|| panic!("no row for {event} of {source:?}"));
    (row.2, row.3, row.4)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_response_per_leakshield() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;

    proxy
        .set_response(LARGE, support::LEAKSHIELD, "none", 0)
        .await
        .unwrap();

    // The row of the Leakshield shows the override, and the row of the event keeps its default.
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("none".to_owned(), 0, true)
    );
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("poweroff".to_owned(), 30, false)
    );
    // The state file keeps it under the key `<event>@<source>`.
    rig.daemon.as_ref().unwrap().hub.flush_now();
    let state = std::fs::read_to_string(rig.state.join("state.json")).unwrap();
    assert!(
        state.contains(&format!("\"{LARGE}@{}\"", support::LEAKSHIELD)),
        "{state}"
    );
    // An instance of this Leakshield takes the override: no poweroff.
    rig.sim.set_alarm_bits(LEAK_BIT);
    rig.step(3).await;
    let rows = active(&proxy).await;
    assert_eq!((rows[0].4.as_str(), rows[0].6), ("none", 0));
    rig.step(40).await;
    assert_eq!(rig.logind.call_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_response_removes_override() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    proxy
        .set_response(LARGE, support::LEAKSHIELD, "none", 0)
        .await
        .unwrap();
    proxy.set_response(LARGE, "", "notify", 10).await.unwrap();

    proxy
        .reset_response(LARGE, support::LEAKSHIELD)
        .await
        .unwrap();

    // The Leakshield follows the default of the event again.
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("notify".to_owned(), 10, false)
    );
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("notify".to_owned(), 10, true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_default_restores_builtin() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    proxy.set_response(LARGE, "", "none", 5).await.unwrap();
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("none".to_owned(), 5, false)
    );
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("none".to_owned(), 5, true)
    );

    proxy.reset_response(LARGE, "").await.unwrap();

    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("poweroff".to_owned(), 30, false)
    );
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("poweroff".to_owned(), 30, false)
    );
    assert_eq!(
        proxy
            .reset_response("no-event", "")
            .await
            .map_err(|e| error_name(&e)),
        Err(format!("{ERR}.NotFound"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_accepts_removed_leakshield_key() {
    // The state holds the override of a Leakshield that this start does not know.
    let rig = Rig::start_with(|_, state| {
        std::fs::write(
            state.join("state.json"),
            r#"{"schema_version": 1, "alarm_responses": {
                "leakshield.leak-alarm@leakshield_1_1": {"response": "none", "delay_s": 0}}}"#,
        )
        .unwrap();
    })
    .await;
    let proxy = alarms(&rig).await;
    // No row shows it: the Leakshield is not known.
    assert!(
        proxy
            .get_responses()
            .await
            .unwrap()
            .iter()
            .all(|r| r.1 != "leakshield_1_1")
    );

    proxy.reset_response(LARGE, "leakshield_1_1").await.unwrap();

    rig.daemon.as_ref().unwrap().hub.flush_now();
    let state = std::fs::read_to_string(rig.state.join("state.json")).unwrap();
    assert!(!state.contains("leakshield_1_1"), "{state}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_for_non_leakshield_event_invalid_argument() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    for event in ["pump-stop", "lost-input", "leakshield-unreachable"] {
        let e = proxy
            .set_response(event, "loop_1", "none", 0)
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"), "{event}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_unknown_leakshield_not_found() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    let e = proxy
        .set_response(LARGE, "leakshield_9_9", "none", 0)
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.NotFound"));
    assert!(
        proxy
            .get_responses()
            .await
            .unwrap()
            .iter()
            .all(|r| r.1 != "leakshield_9_9")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_state_without_overrides_loads() {
    let rig = Rig::start_with(|_, state| {
        std::fs::write(
            state.join("state.json"),
            r#"{"schema_version": 1, "alarm_responses": {
                "pump-stop": {"response": "none", "delay_s": 7},
                "leakshield.leak-alarm": {"response": "notify", "delay_s": 11}}}"#,
        )
        .unwrap();
    })
    .await;
    let proxy = alarms(&rig).await;

    assert_eq!(
        response_row(&proxy, "pump-stop", "").await,
        ("none".to_owned(), 7, true)
    );
    // The Leakshield follows the default of the event, and no Leakshield row is an override.
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("notify".to_owned(), 11, false)
    );
    let rows = proxy.get_responses().await.unwrap();
    assert!(rows.iter().filter(|r| !r.1.is_empty()).all(|r| !r.4));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_row_override_follows_the_builtin_default() {
    let rig = Rig::start_with_clock().await;
    let proxy = alarms(&rig).await;
    // The built-in default: no override.
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("poweroff".to_owned(), 30, false)
    );
    // The same response with another delay differs from the built-in default.
    proxy.set_response(LARGE, "", "poweroff", 5).await.unwrap();
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("poweroff".to_owned(), 5, true)
    );
    // A `notify` row with another delay differs too.
    proxy
        .set_response("lost-input", "", "notify", 5)
        .await
        .unwrap();
    assert_eq!(
        response_row(&proxy, "lost-input", "").await,
        ("notify".to_owned(), 5, true)
    );
    // Back to the built-in values by hand: the default is the built-in default again.
    proxy.set_response(LARGE, "", "poweroff", 30).await.unwrap();
    assert_eq!(
        response_row(&proxy, LARGE, "").await,
        ("poweroff".to_owned(), 30, false)
    );
    // A row of a Leakshield has the flag of its own override only.
    proxy.set_response(LARGE, "", "poweroff", 5).await.unwrap();
    assert_eq!(
        response_row(&proxy, LARGE, support::LEAKSHIELD).await,
        ("poweroff".to_owned(), 5, false)
    );
}
