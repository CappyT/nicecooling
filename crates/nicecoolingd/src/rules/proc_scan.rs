//! The process list for the process rules. The daemon scans `/proc` once per second. The root
//! of the tree is a parameter: tests point it at a fake tree.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::rules::process::{ProcessEntry, ProcessList};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::leakshield::blocking;
use crate::value_table::ValueTable;

pub const SCAN_PERIOD: Duration = Duration::from_secs(1);

/// Where the process list comes from.
pub trait ProcessSource: Send + Sync + 'static {
    fn scan(&self) -> ProcessList;
}

/// A `/proc` tree: `<root>/<pid>/{comm, exe, cmdline}`.
pub struct ProcFs {
    root: PathBuf,
}

impl ProcFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl ProcessSource for ProcFs {
    /// A process that exits during the scan, or that the daemon cannot read, has no entry or a
    /// partial one. A kernel thread has an empty `cmdline` and no `exe`.
    fn scan(&self) -> ProcessList {
        let Ok(dir) = fs::read_dir(&self.root) else {
            return ProcessList::default();
        };
        let entries = dir
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .filter_map(|e| {
                let path = e.path();
                let comm = fs::read_to_string(path.join("comm")).ok()?;
                let exe = fs::read_link(path.join("exe"))
                    .ok()
                    .and_then(|target| {
                        target
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                    })
                    .unwrap_or_default();
                let cmdline = fs::read(path.join("cmdline"))
                    .map(|bytes| {
                        String::from_utf8_lossy(&bytes)
                            .replace('\0', " ")
                            .trim_end()
                            .to_owned()
                    })
                    .unwrap_or_default();
                Some(ProcessEntry::new(comm.trim_end(), &exe, &cmdline))
            })
            .collect();
        ProcessList { entries }
    }
}

/// Scans every `period` and publishes the list to the value table, where the tick reads it.
pub fn spawn(
    source: Arc<dyn ProcessSource>,
    table: ValueTable,
    period: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            table.publish_processes(blocking(|| source.scan()));
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use nicecooling_core::rules::process::{MatchMode, MatchOn, ProcessRule};
    use nicecooling_core::tick::ValueSource;

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-proc-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `/proc/<pid>/` with a name, an executable and a command line.
    fn add_process(root: &std::path::Path, pid: u32, comm: &str, exe: &str, cmdline: &[&str]) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        fs::write(dir.join("cmdline"), cmdline.join("\0") + "\0").unwrap();
        // The target need not exist: `readlink` does not follow it.
        std::os::unix::fs::symlink(exe, dir.join("exe")).unwrap();
    }

    fn rule(name: &str) -> ProcessRule {
        ProcessRule {
            names: vec![name.to_owned()],
            match_mode: MatchMode::Any,
            match_on: MatchOn::Name,
        }
    }

    #[test]
    fn scan_reads_comm_exe_and_cmdline() {
        let root = temp("scan");
        add_process(
            &root,
            100,
            "Game",
            "/opt/x/Game",
            &["/opt/x/Game", "--window"],
        );
        add_process(
            &root,
            200,
            "wine64-preload",
            "/usr/bin/wine64-preloader",
            &["Z:\\Cyberpunk2077.exe"],
        );
        // Entries that are not processes.
        fs::create_dir_all(root.join("self")).unwrap();
        fs::write(root.join("uptime"), "1 1").unwrap();
        let list = ProcFs::new(&root).scan();
        assert_eq!(list.entries.len(), 2);
        assert!(rule("game").holds(&list));
        assert!(rule("wine64-preloader").holds(&list));
        let by_cmdline = ProcessRule {
            match_on: MatchOn::Cmdline,
            ..rule("cyberpunk2077.exe")
        };
        assert!(by_cmdline.holds(&list));
        assert!(!rule("cyberpunk2077.exe").holds(&list));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn process_start_true_within_2s() {
        let root = temp("start");
        let table = ValueTable::default();
        let task = spawn(Arc::new(ProcFs::new(&root)), table.clone(), SCAN_PERIOD);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!rule("game").holds(&table.processes()));

        // The process starts. The rule holds within 2 s, as the scan runs once per second.
        add_process(&root, 321, "game", "/opt/x/game", &["/opt/x/game"]);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(rule("game").holds(&table.processes()));

        // And it stops again.
        fs::remove_dir_all(root.join("321")).unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!rule("game").holds(&table.processes()));
        task.abort();
        fs::remove_dir_all(root).unwrap();
    }

    struct Counting(Arc<AtomicUsize>);

    impl ProcessSource for Counting {
        fn scan(&self) -> ProcessList {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProcessList::default()
        }
    }

    #[tokio::test(start_paused = true)]
    async fn process_list_scanned_every_second() {
        let scans = Arc::new(AtomicUsize::new(0));
        let task = spawn(
            Arc::new(Counting(scans.clone())),
            ValueTable::default(),
            SCAN_PERIOD,
        );
        // One scan at the start, then one for each second.
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(scans.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(scans.load(Ordering::SeqCst), 11);
        task.abort();
    }
}
