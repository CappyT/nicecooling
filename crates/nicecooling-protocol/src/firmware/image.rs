//! The image container of a firmware row: an XML document with base64 segments. The bytes of a
//! segment stay encrypted. The bootloader holds the key (`docs/re/firmware.md`).

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::frame::CHUNK_SIZE;

/// The first flash address of the application. Below it lies the bootloader.
pub const APPLICATION_START: u32 = 0x4000;
/// The bound on the decoded data of all segments. No source gives the flash size (design OQ2).
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub adr_start: u32,
    pub data: Vec<u8>,
}

/// The segments of a container, in address order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    NotXml,
    WrongRoot,
    NoSegment,
    EmptySegment,
    BadBase64,
    BadAdrStart,
    BelowApplication { adr_start: u32 },
    EndAboveU32 { adr_start: u32 },
    Overlap,
    Oversize,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotXml => write!(f, "the image is not an XML document"),
            Self::WrongRoot => write!(f, "the root element is not FirmwareContainer"),
            Self::NoSegment => write!(f, "the image has no segment"),
            Self::EmptySegment => write!(f, "a segment has no data"),
            Self::BadBase64 => write!(f, "a segment has no valid base64 data"),
            Self::BadAdrStart => write!(f, "a segment has no valid start address"),
            Self::BelowApplication { adr_start } => {
                write!(
                    f,
                    "a segment starts at {adr_start:#x}, in the bootloader area"
                )
            }
            Self::EndAboveU32 { adr_start } => {
                write!(f, "the segment at {adr_start:#x} ends above 32 bits")
            }
            Self::Overlap => write!(f, "two segments overlap"),
            Self::Oversize => write!(f, "the segments hold more than {MAX_IMAGE_BYTES} bytes"),
        }
    }
}

impl std::error::Error for ImageError {}

/// Parses the `data` of a firmware row. The documents of aquasuite start with a byte order mark.
pub fn parse(bytes: &[u8]) -> Result<Image, ImageError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ImageError::NotXml)?;
    let document = roxmltree::Document::parse(text).map_err(|_| ImageError::NotXml)?;
    let root = document.root_element();
    if root.tag_name().name() != "FirmwareContainer" {
        return Err(ImageError::WrongRoot);
    }

    let mut segments = Vec::new();
    let mut total = 0usize;
    for node in root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Segment")
    {
        let field = |name: &str| {
            node.children()
                .find(|n| n.is_element() && n.tag_name().name() == name)
                .and_then(|n| n.text())
                .map(str::trim)
        };
        let adr_start = field("adr_start")
            .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|t| t.parse::<u32>().ok())
            .ok_or(ImageError::BadAdrStart)?;
        let data = STANDARD
            .decode(field("data").ok_or(ImageError::BadBase64)?)
            .map_err(|_| ImageError::BadBase64)?;
        if data.is_empty() {
            return Err(ImageError::EmptySegment);
        }
        if adr_start < APPLICATION_START {
            return Err(ImageError::BelowApplication { adr_start });
        }
        if chunk_end(adr_start, data.len()) > u64::from(u32::MAX) {
            return Err(ImageError::EndAboveU32 { adr_start });
        }
        total += data.len();
        if total > MAX_IMAGE_BYTES {
            return Err(ImageError::Oversize);
        }
        segments.push(Segment { adr_start, data });
    }
    if segments.is_empty() {
        return Err(ImageError::NoSegment);
    }

    segments.sort_by_key(|s| s.adr_start);
    for pair in segments.windows(2) {
        if chunk_end(pair[0].adr_start, pair[0].data.len()) > u64::from(pair[1].adr_start) {
            return Err(ImageError::Overlap);
        }
    }
    Ok(Image { segments })
}

/// The end of a segment after its last chunk is padded to a whole chunk.
fn chunk_end(adr_start: u32, len: usize) -> u64 {
    u64::from(adr_start) + (len.div_ceil(CHUNK_SIZE) * CHUNK_SIZE) as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn segment_xml(adr_start: &str, data: &[u8]) -> String {
        format!(
            "<Segment><adr_start>{adr_start}</adr_start><data>{}</data></Segment>",
            STANDARD.encode(data)
        )
    }

    pub(crate) fn container(segments: &[String]) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<FirmwareContainer><firmware>{}</firmware></FirmwareContainer>",
            segments.concat()
        )
        .into_bytes()
    }

    /// A container with one segment, built in code.
    pub(crate) fn one_segment(adr_start: u32, data: &[u8]) -> Vec<u8> {
        container(&[segment_xml(&adr_start.to_string(), data)])
    }

    #[test]
    fn parses_two_segments() {
        let bytes = container(&[
            segment_xml("32768", &[3; 10]),
            segment_xml("16384", &[1; 2000]),
        ]);
        let image = parse(&bytes).unwrap();
        assert_eq!(image.segments.len(), 2);
        assert_eq!(image.segments[0].adr_start, 0x4000);
        assert_eq!(image.segments[0].data, vec![1; 2000]);
        assert_eq!(image.segments[1].adr_start, 0x8000);
        assert_eq!(image.segments[1].data, vec![3; 10]);
    }

    #[test]
    fn bom_is_accepted() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(one_segment(0x4000, &[7; 5]));
        assert_eq!(parse(&bytes).unwrap().segments[0].data, vec![7; 5]);
    }

    #[test]
    fn refuses_not_xml() {
        assert_eq!(parse(b"not xml at all"), Err(ImageError::NotXml));
        assert_eq!(parse(&[0xFF, 0xFE, 0x00]), Err(ImageError::NotXml));
    }

    #[test]
    fn refuses_wrong_root() {
        let bytes =
            b"<Other><Segment><adr_start>16384</adr_start><data>AA==</data></Segment></Other>";
        assert_eq!(parse(bytes), Err(ImageError::WrongRoot));
    }

    #[test]
    fn refuses_no_segment() {
        assert_eq!(parse(&container(&[])), Err(ImageError::NoSegment));
    }

    #[test]
    fn refuses_bad_base64() {
        let bytes =
            container(&["<Segment><adr_start>16384</adr_start><data>!!!</data></Segment>".into()]);
        assert_eq!(parse(&bytes), Err(ImageError::BadBase64));
    }

    #[test]
    fn refuses_bad_adr_start() {
        for text in ["", "abc", "-1", "0x4000", "4294967296", "16384.0"] {
            let bytes = container(&[segment_xml(text, &[1; 4])]);
            assert_eq!(parse(&bytes), Err(ImageError::BadAdrStart), "{text:?}");
        }
    }

    #[test]
    fn refuses_segment_below_0x4000() {
        let bytes = one_segment(0x3C00, &[1; 4]);
        assert_eq!(
            parse(&bytes),
            Err(ImageError::BelowApplication { adr_start: 0x3C00 })
        );
    }

    #[test]
    fn refuses_end_above_u32() {
        // The data ends below 2^32, but the padded last chunk ends at 2^32.
        let bytes = one_segment(0xFFFF_F000, &[1; 4000]);
        assert_eq!(
            parse(&bytes),
            Err(ImageError::EndAboveU32 {
                adr_start: 0xFFFF_F000
            })
        );
    }

    #[test]
    fn refuses_overlap() {
        // The first segment pads to 0x4800, so a start at 0x4400 lies inside its last chunk.
        let bytes = container(&[
            segment_xml("16384", &[1; 1025]),
            segment_xml("17408", &[2; 8]),
        ]);
        assert_eq!(parse(&bytes), Err(ImageError::Overlap));
    }

    #[test]
    fn refuses_oversize() {
        assert!(parse(&one_segment(0x4000, &vec![1; MAX_IMAGE_BYTES])).is_ok());
        assert_eq!(
            parse(&one_segment(0x4000, &vec![1; MAX_IMAGE_BYTES + 1])),
            Err(ImageError::Oversize)
        );
    }
}
