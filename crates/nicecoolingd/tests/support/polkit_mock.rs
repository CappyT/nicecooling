//! A mock of `org.freedesktop.PolicyKit1` for the private bus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::{OwnedVariant, interface};

#[derive(Default)]
pub struct PolkitMock {
    /// The action id and the flags of each `CheckAuthorization`.
    pub calls: Arc<Mutex<Vec<(String, u32)>>>,
    pub deny: Arc<AtomicBool>,
}

#[interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl PolkitMock {
    fn check_authorization(
        &self,
        subject: (String, HashMap<String, OwnedVariant>),
        action_id: &str,
        _details: HashMap<String, String>,
        flags: u32,
        _cancellation_id: &str,
    ) -> (bool, bool, HashMap<String, String>) {
        assert_eq!(subject.0, "system-bus-name");
        self.calls
            .lock()
            .unwrap()
            .push((action_id.to_owned(), flags));
        (!self.deny.load(Ordering::SeqCst), false, HashMap::new())
    }
}

/// The handles of a running mock.
pub struct PolkitHandles {
    pub calls: Arc<Mutex<Vec<(String, u32)>>>,
    pub deny: Arc<AtomicBool>,
    _connection: zbus::Connection,
}

impl PolkitHandles {
    pub async fn start(connection: zbus::Connection) -> Self {
        let mock = PolkitMock::default();
        let (calls, deny) = (mock.calls.clone(), mock.deny.clone());
        connection
            .object_server()
            .at("/org/freedesktop/PolicyKit1/Authority", mock)
            .await
            .unwrap();
        connection
            .request_name("org.freedesktop.PolicyKit1")
            .await
            .unwrap();
        Self {
            calls,
            deny,
            _connection: connection,
        }
    }
}
