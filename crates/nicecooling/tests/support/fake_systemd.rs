//! A fake `org.freedesktop.systemd1` that serves the state of `nicecoolingd.service`.

use zbus::OwnedObjectPath;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

const UNIT_PATH: &str = "/org/freedesktop/systemd1/unit/nicecoolingd_2eservice";

struct Manager;

#[interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn load_unit(&self, name: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        if name == "nicecoolingd.service" {
            Ok(OwnedObjectPath::try_from(UNIT_PATH).expect("path"))
        } else {
            Err(zbus::fdo::Error::UnknownObject(name.to_owned()))
        }
    }
}

struct Unit {
    active_state: String,
}

#[interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        self.active_state.clone()
    }
}

struct Service {
    result: String,
}

#[interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property)]
    fn result(&self) -> String {
        self.result.clone()
    }
}

pub struct FakeSystemd {
    conn: Connection,
}

impl FakeSystemd {
    pub async fn start(conn: Connection, active_state: &str, result: &str) -> Self {
        conn.object_server()
            .at("/org/freedesktop/systemd1", Manager)
            .await
            .expect("manager");
        conn.object_server()
            .at(
                UNIT_PATH,
                Unit {
                    active_state: active_state.to_owned(),
                },
            )
            .await
            .expect("unit");
        conn.object_server()
            .at(
                UNIT_PATH,
                Service {
                    result: result.to_owned(),
                },
            )
            .await
            .expect("service");
        conn.request_name("org.freedesktop.systemd1")
            .await
            .expect("systemd name");
        Self { conn }
    }

    pub async fn set_state(&self, active_state: &str, result: &str) {
        let server = self.conn.object_server();
        let unit = server.interface::<_, Unit>(UNIT_PATH).await.expect("unit");
        unit.get_mut().await.active_state = active_state.to_owned();
        let emitter: &SignalEmitter<'_> = unit.signal_emitter();
        unit.get()
            .await
            .active_state_changed(emitter)
            .await
            .expect("changed");
        let service = server
            .interface::<_, Service>(UNIT_PATH)
            .await
            .expect("service");
        service.get_mut().await.result = result.to_owned();
    }
}
