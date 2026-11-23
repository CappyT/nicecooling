//! A private `dbus-daemon` for one test binary. Tests never touch the system or session bus.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct PrivateBus {
    child: Child,
    dir: PathBuf,
    pub address: String,
}

impl PrivateBus {
    pub fn start(name: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nicecooling-bus-{name}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("bus directory");
        let socket = dir.join("bus");
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                socket.display()
            ),
        )
        .expect("bus config");
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--nofork")
            .arg("--print-address=1")
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be installed to run the bus tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().expect("stdout"))
            .read_line(&mut address)
            .expect("bus address");
        Self {
            child,
            dir,
            address: address.trim().to_owned(),
        }
    }

    pub async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .build()
            .await
            .expect("connection to the private bus")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
