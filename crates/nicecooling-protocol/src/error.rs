use std::fmt;

/// A report or message that the codecs refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportError {
    WrongSize { expected: usize, actual: usize },
    WrongId { expected: u8, actual: u8 },
    BadCrc { stored: u16, computed: u16 },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongSize { expected, actual } => {
                write!(f, "wrong size: expected {expected} bytes, got {actual}")
            }
            Self::WrongId { expected, actual } => {
                write!(f, "wrong report id: expected {expected}, got {actual}")
            }
            Self::BadCrc { stored, computed } => {
                write!(f, "bad CRC: stored {stored:#06x}, computed {computed:#06x}")
            }
        }
    }
}

impl std::error::Error for ReportError {}

pub(crate) fn check_size(bytes: &[u8], expected: usize) -> Result<(), ReportError> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(ReportError::WrongSize {
            expected,
            actual: bytes.len(),
        })
    }
}

pub(crate) fn check_id(bytes: &[u8], expected: u8) -> Result<(), ReportError> {
    if bytes[0] == expected {
        Ok(())
    } else {
        Err(ReportError::WrongId {
            expected,
            actual: bytes[0],
        })
    }
}
