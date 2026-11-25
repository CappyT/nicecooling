mod support;

use std::time::Duration;

use futures_util::StreamExt;
use support::bus::PrivateBus;
use support::fake_logind::{FakeLogind, MANAGER_PATH, PowerCall, session_path};
use zbus::{Connection, OwnedObjectPath, Proxy};

const LOGIND: &str = "org.freedesktop.login1";

async fn manager(conn: &Connection) -> Proxy<'static> {
    Proxy::new(conn, LOGIND, MANAGER_PATH, "org.freedesktop.login1.Manager")
        .await
        .expect("manager proxy")
}

fn call(method: &str, interactive: bool) -> PowerCall {
    PowerCall {
        method: method.to_owned(),
        interactive,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_logind_records_calls_and_interactive_flag() {
    let bus = PrivateBus::start("fake-logind-calls");
    let fake = FakeLogind::start(bus.connect().await, "3", true).await;
    let client = bus.connect().await;
    let manager = manager(&client).await;

    manager.call_method("PowerOff", &(false,)).await.unwrap();
    manager.call_method("Reboot", &(false,)).await.unwrap();
    manager.call_method("Suspend", &(true,)).await.unwrap();
    assert_eq!(
        fake.calls(),
        [
            call("PowerOff", false),
            call("Reboot", false),
            call("Suspend", true)
        ]
    );

    let path: OwnedObjectPath = manager.call("GetSession", &("3",)).await.unwrap();
    assert_eq!(path.as_str(), session_path("3"));
    assert!(manager.call_method("GetSession", &("9",)).await.is_err());
    assert_eq!(fake.sessions_asked(), ["3", "9"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_logind_can_refuse() {
    let bus = PrivateBus::start("fake-logind-refuse");
    let fake = FakeLogind::start(bus.connect().await, "3", true).await;
    let client = bus.connect().await;
    let manager = manager(&client).await;

    fake.refuse_power(true);
    let error = manager.call_method("PowerOff", &(false,)).await;
    assert!(error.is_err(), "the refused call is an error");
    // The refused call is recorded too.
    assert_eq!(fake.calls(), [call("PowerOff", false)]);

    fake.refuse_power(false);
    manager.call_method("PowerOff", &(false,)).await.unwrap();
    assert_eq!(fake.calls().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_logind_active_property_signals() {
    let bus = PrivateBus::start("fake-logind-active");
    let fake = FakeLogind::start(bus.connect().await, "3", true).await;
    let client = bus.connect().await;
    let session = Proxy::new(
        &client,
        LOGIND,
        session_path("3"),
        "org.freedesktop.login1.Session",
    )
    .await
    .expect("session proxy");

    let mut changes = session.receive_property_changed::<bool>("Active").await;
    assert!(session.get_property::<bool>("Active").await.unwrap());

    fake.set_active(false).await;
    let wait = async {
        loop {
            let change = changes.next().await.expect("the stream goes on");
            if !change.get().await.expect("the new value") {
                break;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("PropertiesChanged arrived");
    assert!(!session.get_property::<bool>("Active").await.unwrap());
}
