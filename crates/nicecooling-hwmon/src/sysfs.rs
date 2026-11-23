use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::identity::DeviceIdentity;

/// A sysfs tree. The root is a parameter: `/sys` in production, a fake tree in tests.
#[derive(Clone, Debug)]
pub struct Sysfs {
    root: PathBuf,
}

/// One entry of `class/hwmon`.
#[derive(Clone, Debug)]
pub struct HwmonDevice {
    pub identity: DeviceIdentity,
    /// The name of the `device/driver` link, when the device has one.
    pub driver: Option<String>,
    /// The `class/hwmon/hwmonN` directory. The index changes between boots.
    pub dir: PathBuf,
}

/// A file in a hwmon directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    /// The owner write bit is set.
    pub writable: bool,
}

impl Sysfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every hwmon device, in device path order (then in index order).
    pub fn hwmon_devices(&self) -> io::Result<Vec<HwmonDevice>> {
        let root = fs::canonicalize(&self.root)?;
        let class = self.root.join("class/hwmon");
        let mut devices = Vec::new();
        for entry in fs::read_dir(&class)? {
            let dir = entry?.path();
            let Ok(name) = read_attr(&dir, "name") else {
                continue;
            };
            let device_link = dir.join("device");
            let device = fs::canonicalize(&device_link)
                .ok()
                // A hwmon device without a `device` link is identified by its parent directory.
                .or_else(|| {
                    fs::canonicalize(&dir)
                        .ok()
                        .and_then(|d| d.parent().map(Path::to_path_buf))
                });
            let Some(device) = device else { continue };
            let Ok(relative) = device.strip_prefix(&root) else {
                continue;
            };
            let driver = fs::read_link(device.join("driver"))
                .ok()
                .and_then(|l| l.file_name().map(|n| n.to_string_lossy().into_owned()));
            devices.push(HwmonDevice {
                identity: DeviceIdentity {
                    hwmon_name: name,
                    device_path: format!("/{}", relative.display()),
                },
                driver,
                dir,
            });
        }
        devices.sort_by(|a, b| {
            (&a.identity.device_path, &a.dir).cmp(&(&b.identity.device_path, &b.dir))
        });
        Ok(devices)
    }

    /// The hwmon device with this identity, at its current index.
    pub fn find(&self, identity: &DeviceIdentity) -> io::Result<Option<HwmonDevice>> {
        Ok(self
            .hwmon_devices()?
            .into_iter()
            .find(|d| &d.identity == identity))
    }

    /// A DMI value of `class/dmi/id`, for example `board_vendor`. Empty when the file is missing.
    pub fn dmi(&self, key: &str) -> String {
        read_attr(&self.root.join("class/dmi/id"), key).unwrap_or_default()
    }
}

impl HwmonDevice {
    pub fn read(&self, attribute: &str) -> io::Result<String> {
        read_attr(&self.dir, attribute)
    }

    pub fn write(&self, attribute: &str, value: &str) -> io::Result<()> {
        write_attr(&self.dir, attribute, value)
    }

    pub fn attributes(&self) -> io::Result<Vec<Attribute>> {
        list_attrs(&self.dir)
    }
}

/// The text of an attribute without its trailing whitespace.
pub fn read_attr(dir: &Path, attribute: &str) -> io::Result<String> {
    Ok(fs::read_to_string(dir.join(attribute))?.trim().to_owned())
}

/// Writes an existing attribute. It never creates a file.
pub fn write_attr(dir: &Path, attribute: &str, value: &str) -> io::Result<()> {
    use io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(dir.join(attribute))?;
    file.write_all(value.as_bytes())
}

/// The regular files of a directory, sorted by name.
pub fn list_attrs(dir: &Path) -> io::Result<Vec<Attribute>> {
    let mut attributes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(meta) = fs::metadata(entry.path()) else {
            continue;
        };
        if meta.is_file() {
            attributes.push(Attribute {
                name: entry.file_name().to_string_lossy().into_owned(),
                writable: meta.permissions().mode() & 0o200 != 0,
            });
        }
    }
    attributes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(attributes)
}

/// The writable `pwm<N>` and `pwm<N>_*` attributes of a Channel in restore order:
/// all others first, then `pwm<N>`, then `pwm<N>_enable`.
pub fn channel_attributes(dir: &Path, number: u32) -> io::Result<Vec<String>> {
    let base = format!("pwm{number}");
    let prefix = format!("{base}_");
    let enable = format!("{base}_enable");
    let mut names: Vec<String> = list_attrs(dir)?
        .into_iter()
        .filter(|a| a.writable && (a.name == base || a.name.starts_with(&prefix)))
        .map(|a| a.name)
        .collect();
    let rank = |n: &String| match n {
        n if *n == enable => 2,
        n if *n == base => 1,
        _ => 0,
    };
    names.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    Ok(names)
}
