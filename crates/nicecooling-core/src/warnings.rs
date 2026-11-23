use std::collections::BTreeSet;

/// The active Warnings, each identified by an event id and a source id (`docs/api.md`).
#[derive(Clone, Debug, Default)]
pub struct Warnings {
    active: BTreeSet<(String, String)>,
}

impl Warnings {
    /// Returns true when the Warning was not active before.
    pub fn raise(&mut self, event: &str, source: &str) -> bool {
        self.active.insert((event.to_owned(), source.to_owned()))
    }

    /// Returns true when the Warning was active.
    pub fn clear(&mut self, event: &str, source: &str) -> bool {
        self.active.remove(&(event.to_owned(), source.to_owned()))
    }

    pub fn is_active(&self, event: &str, source: &str) -> bool {
        self.active.contains(&(event.to_owned(), source.to_owned()))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.active.iter().map(|(e, s)| (e.as_str(), s.as_str()))
    }
}
