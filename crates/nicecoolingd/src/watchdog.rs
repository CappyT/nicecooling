use nicecooling_core::tick::{Stage, TickContext};

/// Sends the watchdog ping to systemd.
pub trait Notifier: Send {
    fn ping(&mut self);
}

pub struct SystemdNotifier;

impl Notifier for SystemdNotifier {
    fn ping(&mut self) {
        // Without `NOTIFY_SOCKET` this does nothing. A failed ping needs no handling:
        // systemd restarts the daemon when the pings stop.
        let _ = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]);
    }
}

/// Stage 11: ping the watchdog. It runs after stages 1 to 10, so a tick that stalls sends no ping.
pub struct WatchdogStage<N: Notifier> {
    notifier: N,
}

impl<N: Notifier> WatchdogStage<N> {
    pub fn new(notifier: N) -> Self {
        Self { notifier }
    }
}

impl<N: Notifier> Stage for WatchdogStage<N> {
    fn name(&self) -> &'static str {
        "watchdog"
    }

    fn run(&mut self, _ctx: &mut TickContext) {
        self.notifier.ping();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nicecooling_core::tick::Pipeline;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    #[derive(Clone, Default)]
    pub(crate) struct CountingNotifier(pub Arc<AtomicUsize>);

    impl Notifier for CountingNotifier {
        fn ping(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A stage that waits for a release, like a tick that hangs.
    struct Hang(Arc<Mutex<mpsc::Receiver<()>>>);

    impl Stage for Hang {
        fn name(&self) -> &'static str {
            "hang"
        }

        fn run(&mut self, _ctx: &mut TickContext) {
            let _ = self.0.lock().unwrap().recv();
        }
    }

    #[test]
    fn stalled_tick_sends_no_ping() {
        let pings = CountingNotifier::default();
        let (release, gate) = mpsc::channel();
        let mut pipeline = Pipeline::new(vec![Box::new(Hang(Arc::new(Mutex::new(gate))))]);
        pipeline.push(Box::new(WatchdogStage::new(pings.clone())));

        let tick = std::thread::spawn(move || {
            pipeline.run(&mut TickContext::default(), Duration::ZERO);
        });
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(pings.0.load(Ordering::SeqCst), 0);

        release.send(()).unwrap();
        tick.join().unwrap();
        assert_eq!(pings.0.load(Ordering::SeqCst), 1);
    }
}
