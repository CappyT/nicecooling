//! Atomic file writes: temporary file, `fsync`, rename, `fsync` of the directory.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The file operations of an atomic write, so that tests can record or break them.
pub trait FsOps {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn fsync_file(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn fsync_dir(&self, dir: &Path) -> io::Result<()>;
    fn create_dir(&self, dir: &Path) -> io::Result<()>;
}

pub struct RealFs;

impl FsOps for RealFs {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(bytes)
    }

    fn fsync_file(&self, path: &Path) -> io::Result<()> {
        OpenOptions::new().read(true).open(path)?.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn fsync_dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    fn create_dir(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir(dir)
    }
}

pub fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Creates `dir` and its missing ancestors, and syncs the parent of each directory it created, so
/// that the new entries survive a crash. The caller syncs `dir` itself after it writes into it.
pub fn create_dir_all_with(fs: &dyn FsOps, dir: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut current = dir;
    while !current.as_os_str().is_empty() && !current.exists() {
        missing.push(current);
        current = current.parent().unwrap_or(Path::new(""));
    }
    for created in missing.into_iter().rev() {
        match fs.create_dir(created) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        let parent = created.parent().filter(|p| !p.as_os_str().is_empty());
        fs.fsync_dir(parent.unwrap_or(Path::new(".")))?;
    }
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(&RealFs, path, bytes)
}

pub fn write_atomic_with(fs: &dyn FsOps, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    fs.write(&tmp, bytes)?;
    fs.fsync_file(&tmp)?;
    fs.rename(&tmp, path)?;
    fs.fsync_dir(path.parent().unwrap_or(Path::new(".")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-atomic-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Records each operation and can fail at one of them.
    struct Spy {
        log: RefCell<Vec<&'static str>>,
        fail_at: Option<&'static str>,
        synced: RefCell<Vec<PathBuf>>,
    }

    impl Spy {
        fn step(&self, op: &'static str) -> io::Result<()> {
            self.log.borrow_mut().push(op);
            if self.fail_at == Some(op) {
                return Err(io::Error::other("crash"));
            }
            Ok(())
        }
    }

    impl FsOps for Spy {
        fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            self.step("write")?;
            RealFs.write(path, bytes)
        }
        fn fsync_file(&self, path: &Path) -> io::Result<()> {
            self.step("fsync_file")?;
            RealFs.fsync_file(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.step("rename")?;
            RealFs.rename(from, to)
        }
        fn fsync_dir(&self, dir: &Path) -> io::Result<()> {
            self.step("fsync_dir")?;
            self.synced.borrow_mut().push(dir.to_owned());
            RealFs.fsync_dir(dir)
        }
        fn create_dir(&self, dir: &Path) -> io::Result<()> {
            self.step("create_dir")?;
            RealFs.create_dir(dir)
        }
    }

    #[test]
    fn crash_between_tmp_and_rename() {
        let dir = dir("crash");
        let file = dir.join("state.json");
        write_atomic(&file, b"old").unwrap();
        let spy = Spy {
            log: RefCell::default(),
            fail_at: Some("rename"),
            synced: RefCell::default(),
        };
        assert!(write_atomic_with(&spy, &file, b"new").is_err());
        assert_eq!(fs::read(&file).unwrap(), b"old");
        assert_eq!(fs::read(tmp_path(&file)).unwrap(), b"new");
        // The next write replaces the leftover temporary file.
        write_atomic(&file, b"newer").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"newer");
        assert!(!tmp_path(&file).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_then_directory_fsync() {
        let dir = dir("order");
        let spy = Spy {
            log: RefCell::default(),
            fail_at: None,
            synced: RefCell::default(),
        };
        write_atomic_with(&spy, &dir.join("state.json"), b"x").unwrap();
        assert_eq!(
            *spy.log.borrow(),
            ["write", "fsync_file", "rename", "fsync_dir"]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn created_directories_are_synced_in_their_parents() {
        let dir = dir("mkdirs");
        let spy = Spy {
            log: RefCell::default(),
            fail_at: None,
            synced: RefCell::default(),
        };
        let leaf = dir.join("profiles/p1");
        create_dir_all_with(&spy, &leaf).unwrap();
        assert!(leaf.is_dir());
        // `profiles` is an entry of `dir`, and `p1` an entry of `profiles`.
        assert_eq!(*spy.synced.borrow(), [dir.clone(), dir.join("profiles")]);

        // Nothing is created, so nothing is synced.
        spy.synced.borrow_mut().clear();
        create_dir_all_with(&spy, &leaf).unwrap();
        assert!(spy.synced.borrow().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
