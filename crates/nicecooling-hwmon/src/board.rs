use std::fmt;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::sysfs::Sysfs;

/// A Board file: the names of the Channels and sensors of one Board model (`docs/storage.md`).
#[derive(Clone, Debug, Deserialize)]
pub struct BoardFile {
    pub schema_version: u32,
    pub id: String,
    #[serde(rename = "match")]
    pub matches: BoardMatch,
    #[serde(default, rename = "channel")]
    pub channels: Vec<BoardChannel>,
    #[serde(default, rename = "sensor")]
    pub sensors: Vec<BoardSensor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BoardMatch {
    pub board_vendor: String,
    pub board_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardChannel {
    pub hwmon_name: String,
    pub pwm: u32,
    pub name: String,
    /// Proposed as Pump channel in the first-run screen.
    #[serde(default)]
    pub pump: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardSensor {
    pub hwmon_name: String,
    pub attribute: String,
    pub name: String,
}

#[derive(Debug)]
pub enum BoardError {
    Io(std::io::Error),
    Parse { file: String, message: String },
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read the Board files: {e}"),
            Self::Parse { file, message } => write!(f, "invalid Board file {file}: {message}"),
        }
    }
}

impl std::error::Error for BoardError {}

impl BoardFile {
    pub fn parse(file: &str, text: &str) -> Result<Self, BoardError> {
        let board: Self = toml::from_str(text).map_err(|e| BoardError::Parse {
            file: file.to_owned(),
            message: e.to_string(),
        })?;
        if board.schema_version != 1 {
            return Err(BoardError::Parse {
                file: file.to_owned(),
                message: format!("unsupported schema_version {}", board.schema_version),
            });
        }
        Ok(board)
    }
}

/// The Board file whose `[match]` equals the DMI values, from the files of `dir`.
/// A missing directory means no Board file.
pub fn select_board(dir: &Path, vendor: &str, name: &str) -> Result<Option<BoardFile>, BoardError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(BoardError::Io(e)),
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(BoardError::Io)?;
        let board = BoardFile::parse(&path.display().to_string(), &text)?;
        if board.matches.board_vendor == vendor && board.matches.board_name == name {
            return Ok(Some(board));
        }
    }
    Ok(None)
}

/// The Board file for the DMI values of this sysfs tree.
pub fn select_board_for(sysfs: &Sysfs, dir: &Path) -> Result<Option<BoardFile>, BoardError> {
    select_board(dir, &sysfs.dmi("board_vendor"), &sysfs.dmi("board_name"))
}
