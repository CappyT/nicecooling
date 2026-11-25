//! The scan of `/proc` for the process list. The Profile rules of the daemon and the process node
//! of a Graph in the app use it. The root of the tree is a parameter: tests point it at a fake
//! tree.

use std::fs;
use std::path::PathBuf;

use super::process::{ProcessEntry, ProcessList};

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
    /// A process that exits during the scan, or that the scan cannot read, has no entry or a
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
                // The kernel cuts `comm` at 15 bytes, so it can end inside a UTF-8 character.
                let comm = fs::read(path.join("comm")).ok()?;
                let comm = String::from_utf8_lossy(&comm);
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::rules::process::{MatchMode, MatchOn, ProcessRule};

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nicecooling-core-proc-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn add_process(root: &Path, pid: u32, comm: &str, exe: Option<&str>) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        fs::write(dir.join("cmdline"), "").unwrap();
        if let Some(exe) = exe {
            // The target need not exist: `readlink` does not follow it.
            std::os::unix::fs::symlink(exe, dir.join("exe")).unwrap();
        }
    }

    fn rule(name: &str) -> ProcessRule {
        ProcessRule {
            names: vec![name.to_owned()],
            match_mode: MatchMode::Any,
            match_on: MatchOn::Name,
        }
    }

    #[test]
    fn scan_reads_comm_of_all_processes() {
        let root = temp("all");
        add_process(&root, 1, "init", Some("/usr/lib/systemd/systemd"));
        add_process(&root, 20, "blender", Some("/opt/blender/blender"));
        add_process(&root, 300, "kworker", None);
        // Entries of `/proc` that are not processes.
        fs::create_dir_all(root.join("self")).unwrap();
        fs::write(root.join("uptime"), "1 1").unwrap();

        let list = ProcFs::new(&root).scan();

        assert_eq!(list.entries.len(), 3);
        for name in ["init", "blender", "kworker"] {
            assert!(rule(name).holds(&list), "{name}");
        }
        assert!(!rule("self").holds(&list));
        assert_eq!(
            ProcFs::new(root.join("missing")).scan(),
            ProcessList::default()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_skips_unreadable_exe() {
        let root = temp("exe");
        // A process of another user: `comm` is readable and `exe` is not.
        add_process(&root, 7, "render-wrap", None);

        let list = ProcFs::new(&root).scan();

        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.entries[0], ProcessEntry::new("render-wrap", "", ""));
        assert!(rule("render-wrap").holds(&list));
        assert!(!rule("render-tool").holds(&list));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn name_matches_comm_or_exe_basename_ignoring_case() {
        let list = ProcessList {
            entries: vec![
                ProcessEntry::new("Blender", "", ""),
                ProcessEntry::new("loader", "Render-Tool", ""),
            ],
        };
        let rule = rule("x");
        assert!(rule.name_matches("blender", &list));
        assert!(rule.name_matches("BLENDER", &list));
        assert!(rule.name_matches("render-tool", &list));
        assert!(!rule.name_matches("render", &list));
        assert!(!rule.name_matches("tool", &list));
        // The command line does not count for the match on the name.
        let by_cmdline = ProcessList {
            entries: vec![ProcessEntry::new("a", "b", "game --window")],
        };
        assert!(!rule.name_matches("game", &by_cmdline));
    }
}
