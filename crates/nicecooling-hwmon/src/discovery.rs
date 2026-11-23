use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use crate::board::BoardFile;
use crate::identity::{DeviceIdentity, device_prefixes};
use crate::sysfs::{HwmonDevice, Sysfs, list_attrs, read_attr};

/// Drivers whose `pwmN` outputs are never Channels. Their sensors stay Inputs.
pub const NON_CHANNEL_DRIVERS: [&str; 6] = [
    "amdgpu",
    "radeon",
    "nouveau",
    "i915",
    "xe",
    "aquacomputer_d5next",
];

/// The kernel hwmon device of the Leakshield. The daemon decodes report 1 itself.
pub const LEAKSHIELD_HWMON_NAME: &str = "leakshield";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Fan,
    Voltage,
    Current,
    Power,
    Humidity,
}

impl SensorKind {
    /// The factor from the sysfs integer to the unit of `docs/api.md`:
    /// degrees Celsius, rpm, V, A, W and percent.
    pub fn scale(self) -> f64 {
        match self {
            Self::Fan => 1.0,
            Self::Power => 1e-6,
            Self::Temperature | Self::Voltage | Self::Current | Self::Humidity => 1e-3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    /// `<hwmon name>_pwm<N>`
    pub id: String,
    pub name: String,
    pub device: DeviceIdentity,
    pub number: u32,
    /// The hwmon directory at discovery time.
    pub dir: PathBuf,
    /// The Board file proposes it as Pump channel.
    pub pump: bool,
}

#[derive(Clone, Debug)]
pub struct Sensor {
    /// `<hwmon name>_<attribute prefix>`
    pub id: String,
    pub name: String,
    pub device: DeviceIdentity,
    /// The attribute prefix, for example `temp1`. The value file is `<attribute>_input`.
    pub attribute: String,
    pub kind: SensorKind,
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub channels: Vec<Channel>,
    pub sensors: Vec<Sensor>,
}

/// Splits `temp1_input` into its kind and `temp1`.
fn parse_input(attribute: &str) -> Option<(SensorKind, &str)> {
    let prefix = attribute.strip_suffix("_input")?;
    let (kind, digits) = [
        ("temp", SensorKind::Temperature),
        ("fan", SensorKind::Fan),
        ("in", SensorKind::Voltage),
        ("curr", SensorKind::Current),
        ("power", SensorKind::Power),
        ("humidity", SensorKind::Humidity),
    ]
    .into_iter()
    .find_map(|(stem, kind)| prefix.strip_prefix(stem).map(|d| (kind, d)))?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some((kind, prefix))
}

fn parse_pwm(attribute: &str) -> Option<u32> {
    let digits = attribute.strip_prefix("pwm")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Lists the Channels and sensors of every hwmon device.
/// A name comes from `user_names` (id to name), then from the Board file, then from the kernel.
pub fn discover(
    sysfs: &Sysfs,
    board: Option<&BoardFile>,
    user_names: &BTreeMap<String, String>,
) -> io::Result<Discovery> {
    let devices: Vec<HwmonDevice> = sysfs
        .hwmon_devices()?
        .into_iter()
        .filter(|d| d.identity.hwmon_name != LEAKSHIELD_HWMON_NAME)
        .collect();
    let identities: Vec<_> = devices.iter().map(|d| d.identity.clone()).collect();
    let prefixes = device_prefixes(&identities);
    let mut found = Discovery::default();

    for (device, prefix) in devices.iter().zip(&prefixes) {
        let attributes = list_attrs(&device.dir)?;
        let has = |name: &str| attributes.iter().any(|a| a.name == name);
        let hwmon_name = device.identity.hwmon_name.as_str();
        let channel_driver = !device
            .driver
            .as_deref()
            .is_some_and(|d| NON_CHANNEL_DRIVERS.contains(&d));

        for attribute in &attributes {
            if channel_driver
                && let Some(number) = parse_pwm(&attribute.name)
                && has(&format!("pwm{number}_enable"))
            {
                let id = format!("{prefix}_pwm{number}");
                let board_channel = board.and_then(|b| {
                    b.channels
                        .iter()
                        .find(|c| c.hwmon_name == hwmon_name && c.pwm == number)
                });
                let name = user_names
                    .get(&id)
                    .cloned()
                    .or_else(|| board_channel.map(|c| c.name.clone()))
                    .unwrap_or_else(|| format!("pwm{number}"));
                found.channels.push(Channel {
                    id,
                    name,
                    device: device.identity.clone(),
                    number,
                    dir: device.dir.clone(),
                    pump: board_channel.is_some_and(|c| c.pump),
                });
            }
            if let Some((kind, sensor)) = parse_input(&attribute.name) {
                let id = format!("{prefix}_{sensor}");
                let name = user_names
                    .get(&id)
                    .cloned()
                    .or_else(|| {
                        board
                            .and_then(|b| {
                                b.sensors
                                    .iter()
                                    .find(|s| s.hwmon_name == hwmon_name && s.attribute == sensor)
                            })
                            .map(|s| s.name.clone())
                    })
                    .or_else(|| read_attr(&device.dir, &format!("{sensor}_label")).ok())
                    .unwrap_or_else(|| sensor.to_owned());
                found.sensors.push(Sensor {
                    id,
                    name,
                    device: device.identity.clone(),
                    attribute: sensor.to_owned(),
                    kind,
                    dir: device.dir.clone(),
                });
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::select_board;
    use crate::fake::FakeSysfs;
    use std::path::Path;

    fn boards_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/boards")
    }

    fn reference() -> (FakeSysfs, BoardFile) {
        let fake = FakeSysfs::reference_board();
        let board = select_board(
            &boards_dir(),
            &fake.sysfs().dmi("board_vendor"),
            &fake.sysfs().dmi("board_name"),
        )
        .unwrap()
        .unwrap();
        (fake, board)
    }

    fn channel<'a>(d: &'a Discovery, id: &str) -> &'a Channel {
        d.channels.iter().find(|c| c.id == id).unwrap()
    }

    fn sensor<'a>(d: &'a Discovery, id: &str) -> &'a Sensor {
        d.sensors.iter().find(|s| s.id == id).unwrap()
    }

    #[test]
    fn reference_board_lists_7_channels_with_board_file_names() {
        let (fake, board) = reference();
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        let names: Vec<_> = d
            .channels
            .iter()
            .map(|c| (c.number, c.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (1, "Chassis Fan 1"),
                (2, "CPU Fan"),
                (3, "Chassis Fan 2"),
                (4, "Chassis Fan 3"),
                (5, "High Amp Fan"),
                (6, "Waterpump"),
                (7, "AIO Pump"),
            ]
        );
        assert!(
            d.channels
                .iter()
                .all(|c| c.id == format!("nct6798_pwm{}", c.number))
        );
        assert_eq!(d.channels.iter().filter(|c| c.pump).count(), 1);
        assert!(channel(&d, "nct6798_pwm6").pump);
        // The Board file names a sensor that the kernel labels differently.
        assert_eq!(sensor(&d, "asusec_fan2").name, "Water flow");
        assert_eq!(sensor(&d, "asusec_in0").name, "CPU Core");
    }

    #[test]
    fn user_name_replaces_board_file_name() {
        let (fake, board) = reference();
        let names = BTreeMap::from([("nct6798_pwm6".to_owned(), "Loop pump".to_owned())]);
        let d = discover(&fake.sysfs(), Some(&board), &names).unwrap();
        assert_eq!(channel(&d, "nct6798_pwm6").name, "Loop pump");
        assert_eq!(channel(&d, "nct6798_pwm5").name, "High Amp Fan");
    }

    #[test]
    fn board_selected_by_dmi() {
        let fake = FakeSysfs::reference_board();
        let sysfs = fake.sysfs();
        let dir = boards_dir();
        let found = select_board(&dir, &sysfs.dmi("board_vendor"), &sysfs.dmi("board_name"))
            .unwrap()
            .unwrap();
        assert_eq!(found.id, "asus_rog_crosshair_viii_dark_hero");
        assert!(
            select_board(&dir, "ASUSTeK COMPUTER INC.", "OTHER")
                .unwrap()
                .is_none()
        );
        assert!(
            select_board(&dir, "Other", "ROG CROSSHAIR VIII DARK HERO")
                .unwrap()
                .is_none()
        );
        assert!(
            select_board(Path::new("/nonexistent-boards"), "a", "b")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn unknown_board_uses_kernel_names() {
        let fake = FakeSysfs::reference_board();
        let d = discover(&fake.sysfs(), None, &BTreeMap::new()).unwrap();
        assert_eq!(d.channels.len(), 7);
        assert_eq!(channel(&d, "nct6798_pwm6").name, "pwm6");
        assert!(d.channels.iter().all(|c| !c.pump));
        assert_eq!(sensor(&d, "asusec_fan2").name, "Water_Flow");
        assert_eq!(sensor(&d, "asusec_temp1").name, "Chipset");
        assert_eq!(sensor(&d, "nct6798_fan6").name, "fan6");
    }

    #[test]
    fn amdgpu_pwm_is_not_a_channel() {
        let (fake, board) = reference();
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        assert!(d.channels.iter().all(|c| c.device.hwmon_name != "amdgpu"));
        assert!(d.channels.iter().all(|c| !c.id.starts_with("amdgpu")));
    }

    #[test]
    fn amdgpu_sensors_are_inputs() {
        let (fake, board) = reference();
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        assert_eq!(sensor(&d, "amdgpu_temp1").name, "edge");
        assert_eq!(sensor(&d, "amdgpu_temp1").kind, SensorKind::Temperature);
        assert_eq!(sensor(&d, "amdgpu_fan1").kind, SensorKind::Fan);
    }

    #[test]
    fn leakshield_hwmon_not_listed() {
        let (fake, board) = reference();
        assert!(
            fake.sysfs()
                .hwmon_devices()
                .unwrap()
                .iter()
                .any(|d| d.identity.hwmon_name == "leakshield")
        );
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        assert!(
            d.sensors
                .iter()
                .all(|s| s.device.hwmon_name != "leakshield")
        );
        assert!(
            d.channels
                .iter()
                .all(|c| c.device.hwmon_name != "leakshield")
        );
    }

    fn add_octo(fake: &FakeSysfs) {
        fake.add_hwmon(
            20,
            "octo",
            "devices/pci0000:00/0000:00:08.1/usb2/2-1/2-1:1.0/0003:0C70:F011.0020",
            Some("aquacomputer_d5next"),
            &[
                ("temp1_input", "31000"),
                ("fan1_input", "1200"),
                ("pwm1", "128"),
                ("pwm1_enable", "1"),
            ],
        );
    }

    #[test]
    fn other_aquacomputer_sensors_listed_without_write() {
        let (fake, board) = reference();
        add_octo(&fake);
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        assert_eq!(sensor(&d, "octo_temp1").kind, SensorKind::Temperature);
        assert_eq!(sensor(&d, "octo_fan1").kind, SensorKind::Fan);
    }

    #[test]
    fn aquacomputer_pwm_is_not_a_channel() {
        let (fake, board) = reference();
        add_octo(&fake);
        let d = discover(&fake.sysfs(), Some(&board), &BTreeMap::new()).unwrap();
        assert!(d.channels.iter().all(|c| c.device.hwmon_name != "octo"));
        assert_eq!(d.channels.len(), 7);
    }

    #[test]
    fn channel_attributes_follow_restore_order() {
        let (fake, _) = reference();
        let dir = fake
            .sysfs()
            .hwmon_devices()
            .unwrap()
            .into_iter()
            .find(|d| d.identity.hwmon_name == "nct6798")
            .unwrap()
            .dir;
        let names = crate::sysfs::channel_attributes(&dir, 6).unwrap();
        assert_eq!(names.len(), 22);
        assert_eq!(&names[20..], ["pwm6", "pwm6_enable"]);
        assert_eq!(crate::sysfs::channel_attributes(&dir, 2).unwrap().len(), 28);
    }
}
