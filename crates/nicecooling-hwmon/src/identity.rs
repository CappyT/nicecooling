use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The identity of a hwmon device: its `name` plus the resolved path of its `device` link,
/// relative to the sysfs root with a leading `/`. It never holds the `hwmonN` index.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub hwmon_name: String,
    pub device_path: String,
}

/// Lowercase `[a-z0-9_]` form of a hwmon name, as the identifier rules of `docs/api.md` need.
pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The identifier prefix of each device, in the order of `identities`.
/// The first chip of a hwmon name, in device path order, gets the plain name.
/// The nth chip of the same name gets `<name>_<n>`.
pub fn device_prefixes(identities: &[DeviceIdentity]) -> Vec<String> {
    assign_prefixes(identities, &BTreeMap::new())
}

/// Like `device_prefixes`, but a device in `known` keeps its prefix, and a device that is not in
/// it gets the first free prefix of its name. A new chip with the name of a known chip so never
/// renames the ids of the known one.
pub fn assign_prefixes(
    identities: &[DeviceIdentity],
    known: &BTreeMap<DeviceIdentity, String>,
) -> Vec<String> {
    let mut prefixes: Vec<Option<String>> =
        identities.iter().map(|i| known.get(i).cloned()).collect();
    let mut taken: BTreeSet<String> = known.values().cloned().collect();
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, identity) in identities.iter().enumerate() {
        if prefixes[i].is_none() {
            groups
                .entry(sanitize(&identity.hwmon_name))
                .or_default()
                .push(i);
        }
    }
    for (name, mut members) in groups {
        members.sort_by(|&a, &b| identities[a].device_path.cmp(&identities[b].device_path));
        for i in members {
            let prefix = std::iter::once(name.clone())
                .chain((2..).map(|n| format!("{name}_{n}")))
                .find(|p| !taken.contains(p))
                .unwrap_or_default();
            taken.insert(prefix.clone());
            prefixes[i] = Some(prefix);
        }
    }
    prefixes
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeSysfs;

    #[test]
    fn identity_stable_across_indexes() {
        let attrs = [("name", "nct6798"), ("pwm1", "10"), ("pwm1_enable", "1")];
        let a = FakeSysfs::new();
        a.add_hwmon(
            3,
            "nct6798",
            "devices/platform/nct6775.656",
            Some("nct6775"),
            &attrs,
        );
        let b = FakeSysfs::new();
        b.add_hwmon(
            9,
            "nct6798",
            "devices/platform/nct6775.656",
            Some("nct6775"),
            &attrs,
        );

        let ia = a.sysfs().hwmon_devices().unwrap();
        let ib = b.sysfs().hwmon_devices().unwrap();
        assert_ne!(ia[0].dir.file_name(), ib[0].dir.file_name());
        assert_eq!(ia[0].identity, ib[0].identity);
        assert_eq!(ia[0].identity.device_path, "/devices/platform/nct6775.656");
    }

    #[test]
    fn same_name_two_chips() {
        let fake = FakeSysfs::new();
        // The lower index belongs to the chip with the higher device path.
        fake.add_hwmon(
            0,
            "nvme",
            "devices/pci0000:00/0000:00:01.2/nvme/nvme1",
            None,
            &[("name", "nvme")],
        );
        fake.add_hwmon(
            1,
            "nvme",
            "devices/pci0000:00/0000:00:01.1/nvme/nvme0",
            None,
            &[("name", "nvme")],
        );
        let devices = fake.sysfs().hwmon_devices().unwrap();
        let identities: Vec<_> = devices.iter().map(|d| d.identity.clone()).collect();
        let prefixes = device_prefixes(&identities);
        assert_eq!(
            identities[0].device_path,
            "/devices/pci0000:00/0000:00:01.1/nvme/nvme0"
        );
        assert_eq!(prefixes, ["nvme", "nvme_2"]);
    }

    #[test]
    fn known_chip_keeps_its_prefix_when_a_same_name_chip_appears() {
        let known_chip = DeviceIdentity {
            hwmon_name: "nvme".into(),
            device_path: "/devices/pci0000:00/0000:00:01.2/nvme/nvme1".into(),
        };
        let new_chip = DeviceIdentity {
            hwmon_name: "nvme".into(),
            device_path: "/devices/pci0000:00/0000:00:01.1/nvme/nvme0".into(),
        };
        let known = BTreeMap::from([(known_chip.clone(), "nvme".to_owned())]);
        // The new chip sorts first by path, and still does not take the plain name.
        let both = [new_chip.clone(), known_chip.clone()];
        assert_eq!(assign_prefixes(&both, &known), ["nvme_2", "nvme"]);
        assert_eq!(device_prefixes(&both), ["nvme", "nvme_2"]);
        // A second new chip skips the taken `nvme_2`.
        let third = DeviceIdentity {
            hwmon_name: "nvme".into(),
            device_path: "/devices/pci0000:00/0000:00:01.3/nvme/nvme2".into(),
        };
        let known = BTreeMap::from([
            (known_chip.clone(), "nvme".to_owned()),
            (new_chip.clone(), "nvme_2".to_owned()),
        ]);
        assert_eq!(
            assign_prefixes(&[new_chip, known_chip, third], &known),
            ["nvme_2", "nvme", "nvme_3"]
        );
    }

    #[test]
    fn sanitize_gives_identifier_characters() {
        assert_eq!(sanitize("r8169_0_500:00"), "r8169_0_500_00");
        assert_eq!(sanitize("CoreTemp"), "coretemp");
    }
}
