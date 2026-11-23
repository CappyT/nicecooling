//! A mock of `org.freedesktop.login1` for the private bus. No test reaches the real logind: the
//! daemon of a test is served on the private bus, so its poweroff calls come here or fail.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::{fdo, interface};

#[derive(Default)]
pub struct Login1Mock {
    /// The flags of each `PowerOffWithFlags` call.
    pub calls: Arc<Mutex<Vec<u64>>>,
    /// The calls fail, as logind does for a block inhibitor.
    pub fail: Arc<AtomicBool>,
}

#[interface(name = "org.freedesktop.login1.Manager")]
impl Login1Mock {
    fn power_off_with_flags(&self, flags: u64) -> fdo::Result<()> {
        self.calls.lock().unwrap().push(flags);
        if self.fail.load(Ordering::SeqCst) {
            return Err(fdo::Error::Failed("Operation inhibited (mock)".into()));
        }
        Ok(())
    }
}

/// The handles of a running mock.
pub struct Login1Handles {
    pub calls: Arc<Mutex<Vec<u64>>>,
    pub fail: Arc<AtomicBool>,
    _connection: zbus::Connection,
}

impl Login1Handles {
    pub async fn start(connection: zbus::Connection) -> Self {
        let mock = Login1Mock::default();
        let (calls, fail) = (mock.calls.clone(), mock.fail.clone());
        connection
            .object_server()
            .at("/org/freedesktop/login1", mock)
            .await
            .unwrap();
        connection
            .request_name("org.freedesktop.login1")
            .await
            .unwrap();
        Self {
            calls,
            fail,
            _connection: connection,
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}
