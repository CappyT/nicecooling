//! A fake sysfs tree in a temporary directory. Tests write to it, never to the real `/sys`.

use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::sysfs::Sysfs;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The tree is `<temporary dir>/sys`. The directory goes away on drop.
pub struct FakeSysfs {
    tmp: PathBuf,
    root: PathBuf,
}

impl FakeSysfs {
    pub fn new() -> Self {
        let tmp = std::env::temp_dir().join(format!(
            "nicecooling-fake-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let root = tmp.join("sys");
        fs::create_dir_all(root.join("class/hwmon")).unwrap();
        Self { tmp, root }
    }

    /// The tree of the reference Board, `tests/fixtures/reference-board/tree.txt`.
    pub fn reference_board() -> Self {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/reference-board/tree.txt");
        Self::from_manifest(&manifest).unwrap()
    }

    /// Builds a tree from a manifest. A line is `dir <path>`, `link <path> <target>` or
    /// `file <path> <octal mode> <value>`. A line that starts with `#` is a comment.
    pub fn from_manifest(manifest: &Path) -> io::Result<Self> {
        let fake = Self::new();
        for line in fs::read_to_string(manifest)?.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, ' ');
            let kind = parts.next().unwrap_or_default();
            let rest = parts.next().unwrap_or_default();
            match kind {
                "dir" => fs::create_dir_all(fake.root.join(rest))?,
                "link" => {
                    let (path, target) = rest.split_once(' ').ok_or_else(bad_line)?;
                    fake.link(path, target);
                }
                "file" => {
                    let mut f = rest.splitn(3, ' ');
                    let path = f.next().ok_or_else(bad_line)?;
                    let mode = f.next().ok_or_else(bad_line)?;
                    let mode = u32::from_str_radix(mode, 8).map_err(|_| bad_line())?;
                    fake.file(path, mode, f.next().unwrap_or_default());
                }
                _ => return Err(bad_line()),
            }
        }
        Ok(fake)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sysfs(&self) -> Sysfs {
        Sysfs::new(&self.root)
    }

    /// Creates a file below the root. Its parent directories appear as needed.
    pub fn file(&self, path: &str, mode: u32, value: &str) {
        let full = self.root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, format!("{value}\n")).unwrap();
        fs::set_permissions(&full, fs::Permissions::from_mode(mode)).unwrap();
    }

    pub fn link(&self, path: &str, target: &str) {
        let full = self.root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        symlink(target, full).unwrap();
    }

    /// Adds a hwmon device. Attributes named `pwm*` and `*_enable` are writable, the others are not.
    /// `device` is the path of the device directory relative to the root.
    pub fn add_hwmon(
        &self,
        index: u32,
        name: &str,
        device: &str,
        driver: Option<&str>,
        attributes: &[(&str, &str)],
    ) {
        let hwmon = format!("{device}/hwmon/hwmon{index}");
        fs::create_dir_all(self.root.join(&hwmon)).unwrap();
        fs::create_dir_all(self.root.join("class/hwmon")).unwrap();
        self.link(
            &format!("class/hwmon/hwmon{index}"),
            &format!("../../{hwmon}"),
        );
        self.link(
            &format!("{hwmon}/device"),
            &format!("../../../{}", device.rsplit('/').next().unwrap()),
        );
        self.file(&format!("{hwmon}/name"), 0o444, name);
        for (attribute, value) in attributes {
            if *attribute == "name" {
                continue;
            }
            let writable = attribute.starts_with("pwm");
            self.file(
                &format!("{hwmon}/{attribute}"),
                if writable { 0o644 } else { 0o444 },
                value,
            );
        }
        if let Some(driver) = driver {
            let drivers = format!("bus/fake/drivers/{driver}");
            fs::create_dir_all(self.root.join(&drivers)).unwrap();
            let up = "../".repeat(device.matches('/').count() + 1);
            self.link(&format!("{device}/driver"), &format!("{up}{drivers}"));
        }
    }

    /// The text of a file below the root, trimmed.
    pub fn get(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path))
            .unwrap()
            .trim()
            .to_owned()
    }

    /// Replaces the content of an existing file below the root.
    pub fn set(&self, path: &str, value: &str) {
        fs::write(self.root.join(path), format!("{value}\n")).unwrap();
    }

    /// The path of a hwmon attribute of the device with this hwmon name.
    pub fn attr_path(&self, hwmon_name: &str, attribute: &str) -> PathBuf {
        let device = self
            .sysfs()
            .hwmon_devices()
            .unwrap()
            .into_iter()
            .find(|d| d.identity.hwmon_name == hwmon_name)
            .unwrap();
        device.dir.join(attribute)
    }
}

impl Default for FakeSysfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FakeSysfs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

fn bad_line() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "bad manifest line")
}
