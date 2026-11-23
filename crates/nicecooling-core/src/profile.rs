//! The Leakshield slot of a Profile (`docs/api.md`, section Leakshield slots).

use std::fmt;

/// The number of Leakshield slots. aquasuite keeps four, one for each device profile number.
pub const SLOT_COUNT: u8 = 4;

/// A Leakshield slot, 1 to 4. The number is the device profile number: report 1 `active_profile`
/// and report 3 `device_profile`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(u8);

impl Slot {
    /// The slot of the Default Profile.
    pub const DEFAULT: Slot = Slot(1);

    /// `None` for a number outside 1 to 4.
    pub fn new(number: u32) -> Option<Self> {
        u8::try_from(number)
            .ok()
            .filter(|n| (1..=SLOT_COUNT).contains(n))
            .map(Self)
    }

    pub fn get(self) -> u8 {
        self.0
    }

    pub fn all() -> impl Iterator<Item = Slot> {
        (1..=SLOT_COUNT).map(Slot)
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_1_to_4() {
        assert_eq!(Slot::new(0), None);
        assert_eq!(Slot::new(5), None);
        assert_eq!(Slot::new(256 + 1), None);
        let all: Vec<u8> = Slot::all().map(Slot::get).collect();
        assert_eq!(all, [1, 2, 3, 4]);
        assert_eq!(Slot::new(3).map(Slot::get), Some(3));
        assert_eq!(Slot::DEFAULT.get(), 1);
    }
}
