use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    /// One name matches.
    Any,
    /// Every name matches.
    All,
    /// No name matches.
    None,
}

impl MatchMode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "any" => Self::Any,
            "all" => Self::All,
            "none" => Self::None,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchOn {
    /// The name equals `comm` or the basename of `exe`.
    #[default]
    Name,
    /// The name is a substring of the command line. It finds games that run under Proton, where
    /// the executable is the Wine loader.
    Cmdline,
}

impl MatchOn {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "name" => Some(Self::Name),
            "cmdline" => Some(Self::Cmdline),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Cmdline => "cmdline",
        }
    }
}

/// One process of the list, with the text that a rule compares, in lower case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessEntry {
    comm: String,
    /// The basename of `/proc/<pid>/exe`. Empty when the daemon cannot read it.
    exe: String,
    /// `/proc/<pid>/cmdline`, with a space for each NUL byte.
    cmdline: String,
}

impl ProcessEntry {
    pub fn new(comm: &str, exe_basename: &str, cmdline: &str) -> Self {
        Self {
            comm: comm.to_lowercase(),
            exe: exe_basename.to_lowercase(),
            cmdline: cmdline.to_lowercase(),
        }
    }
}

/// The running processes, as the scan of `/proc` saw them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessList {
    pub entries: Vec<ProcessEntry>,
}

/// A list of process names with a match mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProcessRule {
    pub names: Vec<String>,
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    #[serde(default)]
    pub match_on: MatchOn,
}

impl ProcessRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.names.is_empty() || self.names.iter().any(|n| n.trim().is_empty()) {
            return Err("names needs at least one name, and no name is empty".into());
        }
        Ok(())
    }

    fn name_matches(&self, name: &str, list: &ProcessList) -> bool {
        let name = name.to_lowercase();
        list.entries.iter().any(|p| match self.match_on {
            MatchOn::Name => p.comm == name || p.exe == name,
            MatchOn::Cmdline => p.cmdline.contains(&name),
        })
    }

    pub fn holds(&self, list: &ProcessList) -> bool {
        let mut matches = self.names.iter().map(|n| self.name_matches(n, list));
        match self.match_mode {
            MatchMode::Any => matches.any(|m| m),
            MatchMode::All => matches.all(|m| m),
            MatchMode::None => !matches.any(|m| m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[(&str, &str, &str)]) -> ProcessList {
        ProcessList {
            entries: entries
                .iter()
                .map(|(comm, exe, cmdline)| ProcessEntry::new(comm, exe, cmdline))
                .collect(),
        }
    }

    fn rule(names: &[&str], match_mode: MatchMode, match_on: MatchOn) -> ProcessRule {
        ProcessRule {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
            match_mode,
            match_on,
        }
    }

    #[test]
    fn process_name_exact_case_insensitive_on_exe_basename() {
        // `comm` is cut at 15 characters, so the name of the executable carries the match.
        let list = list(&[("loader", "Game", "/opt/x/Game --fullscreen")]);
        let rule = rule(&["game"], MatchMode::Any, MatchOn::Name);
        assert!(rule.holds(&list));
        // Exact: a part of the name does not match.
        assert!(!self::rule(&["gam"], MatchMode::Any, MatchOn::Name).holds(&list));
        assert!(!self::rule(&["game2"], MatchMode::Any, MatchOn::Name).holds(&list));
        // The command line does not count for `match_on` name.
        assert!(!self::rule(&["fullscreen"], MatchMode::Any, MatchOn::Name).holds(&list));
    }

    #[test]
    fn process_name_matches_comm() {
        let list = list(&[("game", "", "")]);
        assert!(rule(&["game"], MatchMode::Any, MatchOn::Name).holds(&list));
        assert!(rule(&["GAME"], MatchMode::Any, MatchOn::Name).holds(&list));
    }

    #[test]
    fn process_cmdline_substring_matches_proton_game() {
        // A Proton game: the executable is the Wine loader.
        let list = list(&[(
            "wine64-preloader",
            "wine64-preloader",
            "Z:\\games\\Cyberpunk 2077\\bin\\x64\\Cyberpunk2077.exe -windowed",
        )]);
        let by_cmdline = rule(&["Cyberpunk2077.exe"], MatchMode::Any, MatchOn::Cmdline);
        assert!(by_cmdline.holds(&list));
        let by_name = rule(&["Cyberpunk2077.exe"], MatchMode::Any, MatchOn::Name);
        assert!(!by_name.holds(&list));
    }

    #[test]
    fn process_match_any_all_none() {
        let list = list(&[("game", "game", "")]);
        let names = ["game", "launcher"];
        let holds = |mode| rule(&names, mode, MatchOn::Name).holds(&list);
        // Only one of the two names runs.
        assert!(holds(MatchMode::Any));
        assert!(!holds(MatchMode::All));
        assert!(!holds(MatchMode::None));
        // Neither runs, then both.
        let empty = ProcessList::default();
        assert!(!rule(&names, MatchMode::Any, MatchOn::Name).holds(&empty));
        assert!(rule(&names, MatchMode::None, MatchOn::Name).holds(&empty));
        let both = self::list(&[("game", "", ""), ("launcher", "", "")]);
        assert!(rule(&names, MatchMode::All, MatchOn::Name).holds(&both));
    }

    #[test]
    fn a_rule_needs_names() {
        assert!(rule(&[], MatchMode::Any, MatchOn::Name).validate().is_err());
        assert!(
            rule(&[" "], MatchMode::Any, MatchOn::Name)
                .validate()
                .is_err()
        );
        assert!(
            rule(&["a"], MatchMode::Any, MatchOn::Name)
                .validate()
                .is_ok()
        );
    }
}
