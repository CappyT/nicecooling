//! A mock of `org.freedesktop.login1` for the private bus. No test reaches the real logind: the
//! daemon of a test is served on the private bus, so its poweroff calls come here or fail.

use std::io::Read as _;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::{fdo, interface};

#[derive(Default)]
pub struct Login1Mock {
    /// The flags of each `PowerOffWithFlags` call.
    pub calls: Arc<Mutex<Vec<u64>>>,
    /// The calls fail, as logind does for a block inhibitor.
    pub fail: Arc<AtomicBool>,
    /// The ends of the locks that `Inhibit` handed out. A lock is open until its other end closes.
    pub locks: Arc<Mutex<Vec<UnixStream>>>,
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

    /// Hands out the other end of a socket pair, as logind hands out the lock of an inhibitor.
    fn inhibit(
        &self,
        _what: &str,
        _who: &str,
        _why: &str,
        _mode: &str,
    ) -> fdo::Result<zbus::OwnedFd> {
        let (mine, lock) = UnixStream::pair().map_err(|e| fdo::Error::Failed(e.to_string()))?;
        mine.set_nonblocking(true)
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        self.locks.lock().unwrap().push(mine);
        Ok(zbus::OwnedFd::from(OwnedFd::from(lock)))
    }
}

/// The handles of a running mock.
pub struct Login1Handles {
    pub calls: Arc<Mutex<Vec<u64>>>,
    pub fail: Arc<AtomicBool>,
    pub locks: Arc<Mutex<Vec<UnixStream>>>,
    connection: zbus::Connection,
}

impl Login1Handles {
    pub async fn start(connection: zbus::Connection) -> Self {
        let mock = Login1Mock::default();
        let (calls, fail, locks) = (mock.calls.clone(), mock.fail.clone(), mock.locks.clone());
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
            locks,
            connection,
        }
    }

    /// The sleep signal of logind.
    pub async fn prepare_for_sleep(&self, start: bool) {
        self.connection
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                &(start,),
            )
            .await
            .unwrap();
    }

    /// The locks that are open now: the holder of the other end of a lock has not closed it.
    pub fn open_locks(&self) -> usize {
        self.locks
            .lock()
            .unwrap()
            .iter()
            .filter(|lock| {
                let mut peek = [0u8; 1];
                // An open socket has no data and no end yet. A closed one reads 0 bytes.
                matches!((&**lock).read(&mut peek), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
            })
            .count()
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}
