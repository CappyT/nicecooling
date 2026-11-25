//! The process list for the process rules. The daemon scans `/proc` once per second with the
//! scan of `nicecooling-core`. The root of the tree is a parameter: tests point it at a fake tree.

use std::sync::Arc;
use std::time::Duration;

pub use nicecooling_core::rules::proc_scan::{ProcFs, ProcessSource};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::leakshield::blocking;
use crate::value_table::ValueTable;

pub const SCAN_PERIOD: Duration = Duration::from_secs(1);

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
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use nicecooling_core::rules::process::{MatchMode, MatchOn, ProcessList, ProcessRule};
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

    /// The kernel cuts `comm` at 15 bytes, so a UTF-8 name can end in half a character.
    #[test]
    fn a_process_with_a_non_utf8_comm_is_still_matched() {
        let root = temp("non-utf8-comm");
        let dir = root.join("4242");
        fs::create_dir_all(&dir).unwrap();
        let mut comm = b"Weltraumspiel_".to_vec();
        comm.extend([0xC3, b'\n']);
        fs::write(dir.join("comm"), comm).unwrap();
        fs::write(
            dir.join("cmdline"),
            b"Z:\\Games\\Cyberpunk2077.exe\0--fullscreen\0",
        )
        .unwrap();
        std::os::unix::fs::symlink("/opt/games/game.bin", dir.join("exe")).unwrap();

        let list = ProcFs::new(&root).scan();

        assert_eq!(list.entries.len(), 1);
        let by_cmdline = ProcessRule {
            match_on: MatchOn::Cmdline,
            ..rule("cyberpunk2077.exe")
        };
        let none = ProcessRule {
            match_mode: MatchMode::None,
            ..rule("game.bin")
        };
        assert!(by_cmdline.holds(&list));
        assert!(rule("game.bin").holds(&list));
        assert!(!none.holds(&list));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_non_utf8_cmdline_still_matches_its_readable_part() {
        let root = temp("non-utf8-cmdline");
        let dir = root.join("77");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), "loader\n").unwrap();
        fs::write(dir.join("cmdline"), b"\xFF\xFEgame.exe\0\xC3").unwrap();

        let list = ProcFs::new(&root).scan();

        let by_cmdline = ProcessRule {
            match_on: MatchOn::Cmdline,
            ..rule("game.exe")
        };
        assert!(by_cmdline.holds(&list));
        fs::remove_dir_all(root).unwrap();
    }

    /// The arguments of `cmdline` are separated by NUL. A rule sees one space between them, and
    /// the trailing NUL leaves no space at the end.
    #[test]
    fn cmdline_nul_separators_become_single_spaces() {
        let root = temp("cmdline-nul");
        let dir = root.join("5");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), "wine\n").unwrap();
        fs::write(dir.join("cmdline"), b"wine\0game.exe\0-windowed\0").unwrap();

        let list = ProcFs::new(&root).scan();

        let on = |name: &str| ProcessRule {
            match_on: MatchOn::Cmdline,
            ..rule(name)
        };
        assert!(on("wine game.exe -windowed").holds(&list));
        assert!(!on("game.exe -windowed ").holds(&list));
        fs::remove_dir_all(root).unwrap();
    }

    /// A process that exits during the scan leaves a directory without files, or without `comm`.
    #[test]
    fn a_process_that_vanishes_during_the_scan_is_skipped() {
        let root = temp("vanished");
        add_process(&root, 100, "game", "/opt/x/game", &["/opt/x/game"]);
        // Its files are gone, but the directory is still listed.
        fs::create_dir_all(root.join("200")).unwrap();
        // `exe` and `cmdline` are gone, `comm` is still there.
        fs::create_dir_all(root.join("300")).unwrap();
        fs::write(root.join("300/comm"), "half\n").unwrap();
        // A pid that is a file, not a directory.
        fs::write(root.join("400"), "x").unwrap();

        let list = ProcFs::new(&root).scan();

        assert_eq!(list.entries.len(), 2);
        assert!(rule("game").holds(&list));
        assert!(rule("half").holds(&list));
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
