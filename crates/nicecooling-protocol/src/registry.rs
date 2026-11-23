use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

use crate::report3::{REPORT3_SIZE, REPORT3_STRUCT_END, Report3};

const LEAKSHIELD_TOML: &str = include_str!("../registry/leakshield.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskClass {
    Safety,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationState {
    CodeDefined,
    PlausibleRead,
    VerifiedWrite,
    Failed,
}

impl VerificationState {
    /// A low-risk field is writable at `plausible-read` or `verified-write`, a Safety field
    /// only at `verified-write` (design D10).
    pub fn allows_write(self, risk: RiskClass) -> bool {
        matches!(
            (risk, self),
            (_, Self::VerifiedWrite) | (RiskClass::Low, Self::PlausibleRead)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    U8,
    I16,
    U16,
}

impl Scalar {
    pub fn size(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
        }
    }
}

/// A scalar (`u8`, `i16`, `u16`) or an array of scalars such as `u8[4]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct RawType {
    pub scalar: Scalar,
    pub len: usize,
}

impl TryFrom<String> for RawType {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let (name, len) = match text.split_once('[') {
            Some((name, rest)) => {
                let len = rest
                    .strip_suffix(']')
                    .and_then(|n| n.parse().ok())
                    .filter(|&n| n > 0);
                (
                    name,
                    len.ok_or_else(|| format!("bad array length in type {text:?}"))?,
                )
            }
            None => (text.as_str(), 1),
        };
        let scalar = match name {
            "u8" => Scalar::U8,
            "i16" => Scalar::I16,
            "u16" => Scalar::U16,
            _ => return Err(format!("unknown type {text:?}")),
        };
        Ok(Self { scalar, len })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub report: u8,
    pub offset: usize,
    #[serde(rename = "type")]
    pub value_type: RawType,
    pub scale: Option<f64>,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Allowed raw values.
    pub values: Option<Vec<i64>>,
    /// Mask of the bits that the field uses.
    pub bits: Option<i64>,
    pub default: Option<f64>,
    pub count: Option<usize>,
    pub stride: Option<usize>,
    pub inner_count: Option<usize>,
    pub inner_stride: Option<usize>,
    pub risk_class: RiskClass,
    pub verification_state: VerificationState,
}

impl Field {
    pub fn is_writable(&self) -> bool {
        self.verification_state.allows_write(self.risk_class)
    }

    /// Offsets of every scalar: `count` copies, `stride` bytes apart, in each copy `inner_count`
    /// copies, `inner_stride` bytes apart, and in each of those the scalars of an array type.
    pub fn element_offsets(&self) -> Vec<usize> {
        let RawType { scalar, len } = self.value_type;
        let size = scalar.size();
        let stride = self.stride.unwrap_or(size * len);
        let inner_stride = self.inner_stride.unwrap_or(size * len);
        let mut offsets = Vec::new();
        for i in 0..self.count.unwrap_or(1) {
            for j in 0..self.inner_count.unwrap_or(1) {
                for k in 0..len {
                    offsets.push(self.offset + i * stride + j * inner_stride + k * size);
                }
            }
        }
        offsets
    }

    /// Scalars in each of the `count` repeats.
    pub fn repeat_len(&self) -> usize {
        self.element_offsets().len() / self.count.unwrap_or(1)
    }

    /// Offsets of the scalars of repeat `index`, or none when `index` is not below `count`.
    pub fn repeat_offsets(&self, index: usize) -> Vec<usize> {
        let len = self.repeat_len();
        self.element_offsets()
            .into_iter()
            .skip(index * len)
            .take(len)
            .collect()
    }

    /// Raw values of repeat `index`.
    pub fn read_repeat(&self, report: &Report3, index: usize) -> Vec<i64> {
        self.repeat_offsets(index)
            .into_iter()
            .map(|o| self.read_raw(report, o))
            .collect()
    }

    /// Raw value of the scalar at `element_offset`, one of `element_offsets`.
    pub fn read_raw(&self, report: &Report3, element_offset: usize) -> i64 {
        let scalar = self.value_type.scalar;
        let b = report.bytes_at(element_offset, scalar.size());
        match scalar {
            Scalar::U8 => i64::from(b[0]),
            Scalar::U16 => i64::from(u16::from_be_bytes([b[0], b[1]])),
            Scalar::I16 => i64::from(i16::from_be_bytes([b[0], b[1]])),
        }
    }

    pub fn raw_in_range(&self, raw: i64) -> bool {
        if let Some(values) = &self.values {
            return values.contains(&raw);
        }
        if let Some(bits) = self.bits {
            return raw & !bits == 0;
        }
        let value = raw as f64 * self.scale.unwrap_or(1.0);
        // A scale such as 0.392157 is rounded, so the full-scale raw value lands a hair past the bound.
        let tolerance = |bound: f64| 1e-4 * bound.abs().max(1.0);
        self.min.is_none_or(|min| value >= min - tolerance(min))
            && self.max.is_none_or(|max| value <= max + tolerance(max))
    }

    /// True when every element of the field is in range in `report`.
    pub fn in_range_in(&self, report: &Report3) -> bool {
        self.element_offsets()
            .into_iter()
            .all(|o| self.raw_in_range(self.read_raw(report, o)))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrcSpec {
    pub algorithm: String,
    pub first: usize,
    pub last: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report3Spec {
    pub size: usize,
    pub structure_offset: usize,
    pub crc: CrcSpec,
    pub volatile_mask: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub code: u32,
    pub parameter: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub schema_version: u32,
    pub model: String,
    pub firmware: u16,
    pub structure: u16,
    pub report3: Report3Spec,
    #[serde(default)]
    pub command: Vec<Command>,
    #[serde(default)]
    pub field: Vec<Field>,
}

#[derive(Debug)]
pub enum RegistryError {
    Parse(toml::de::Error),
    Invalid(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "registry does not parse: {e}"),
            Self::Invalid(m) => write!(f, "registry is invalid: {m}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl Registry {
    /// The registry for firmware 1022, compiled into the crate.
    pub fn leakshield() -> Self {
        Self::from_toml(LEAKSHIELD_TOML).expect("the compiled-in registry loads")
    }

    pub fn from_toml(text: &str) -> Result<Self, RegistryError> {
        let registry: Self = toml::from_str(text).map_err(RegistryError::Parse)?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn field(&self, id: &str) -> Option<&Field> {
        self.field.iter().find(|f| f.id == id)
    }

    /// Raw values of repeat `index` of field `id`. `None` when the registry lacks the field or
    /// the field has no such repeat.
    pub fn read_repeat(&self, report: &Report3, id: &str, index: usize) -> Option<Vec<i64>> {
        let field = self.field(id)?;
        (index < field.count.unwrap_or(1)).then(|| field.read_repeat(report, index))
    }

    pub fn command(&self, id: &str) -> Option<&Command> {
        self.command.iter().find(|c| c.id == id)
    }

    fn validate(&self) -> Result<(), RegistryError> {
        let invalid = |m: String| Err(RegistryError::Invalid(m));
        if self.report3.size != REPORT3_SIZE {
            return invalid(format!(
                "report3 size {} is not {REPORT3_SIZE}",
                self.report3.size
            ));
        }
        let mut seen = BTreeSet::new();
        for field in &self.field {
            if !seen.insert(field.id.as_str()) {
                return invalid(format!("duplicate field id {}", field.id));
            }
            if field.report != 3 {
                return invalid(format!("field {} is not in report 3", field.id));
            }
            let size = field.value_type.scalar.size();
            if field
                .element_offsets()
                .iter()
                .any(|&o| o < 1 || o + size > REPORT3_STRUCT_END + 1)
            {
                return invalid(format!("field {} leaves the report 3 struct", field.id));
            }
            if field.min.zip(field.max).is_some_and(|(min, max)| min > max) {
                return invalid(format!("field {} has min above max", field.id));
            }
        }
        Ok(())
    }
}
