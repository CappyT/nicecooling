//! The image container of a firmware row: an XML document with base64 segments. The bytes of a
//! segment stay encrypted. The bootloader holds the key (`docs/protocol/leakshield-firmware.md`).

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::frame::CHUNK_SIZE;

/// The first flash address of the application. Below it lies the bootloader.
pub const APPLICATION_START: u32 = 0x4000;
/// The bound on the decoded data of all segments. No source gives the flash size (design OQ2).
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

/// One run of flash. `parse` is the only constructor, so every segment starts at or above
/// `APPLICATION_START` on a chunk boundary and ends below 2^32 after its padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    adr_start: u32,
    data: Vec<u8>,
}

impl Segment {
    pub fn adr_start(&self) -> u32 {
        self.adr_start
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// The segments of a container, in address order, without overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    segments: Vec<Segment>,
}

impl Image {
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    NotXml,
    WrongRoot,
    /// The document is not `FirmwareContainer` > one `firmware` > `Segment` elements that hold one
    /// `adr_start` and one `data`, each with text only.
    BadShape,
    NoSegment,
    EmptySegment,
    BadBase64,
    BadAdrStart,
    BelowApplication {
        adr_start: u32,
    },
    UnalignedStart {
        adr_start: u32,
    },
    EndAboveU32 {
        adr_start: u32,
    },
    Overlap,
    Oversize,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotXml => write!(f, "the image is not an XML document"),
            Self::WrongRoot => write!(f, "the root element is not FirmwareContainer"),
            Self::BadShape => write!(f, "the container has not the shape of an aquasuite image"),
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
            Self::UnalignedStart { adr_start } => {
                write!(
                    f,
                    "a segment starts at {adr_start:#x}, not on a {CHUNK_SIZE}-byte boundary"
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

    let firmware = single_child(root, "firmware")?;
    let mut segments = Vec::new();
    let mut total = 0usize;
    for node in structure_children(firmware)? {
        if node.tag_name().name() != "Segment" {
            return Err(ImageError::BadShape);
        }
        let mut adr_start = None;
        let mut data = None;
        for field in structure_children(node)? {
            let slot = match field.tag_name().name() {
                "adr_start" => &mut adr_start,
                "data" => &mut data,
                _ => return Err(ImageError::BadShape),
            };
            if slot.replace(text_only(field)?).is_some() {
                return Err(ImageError::BadShape);
            }
        }
        let adr_start = adr_start
            .flatten()
            .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|t| t.parse::<u32>().ok())
            .ok_or(ImageError::BadAdrStart)?;
        let data = STANDARD
            .decode(data.flatten().ok_or(ImageError::BadBase64)?)
            .map_err(|_| ImageError::BadBase64)?;
        if data.is_empty() {
            return Err(ImageError::EmptySegment);
        }
        if adr_start < APPLICATION_START {
            return Err(ImageError::BelowApplication { adr_start });
        }
        if !adr_start.is_multiple_of(CHUNK_SIZE as u32) {
            return Err(ImageError::UnalignedStart { adr_start });
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

/// The element children of a node that holds structure. A comment or a processing instruction is
/// skipped, and text other than white space is refused.
fn structure_children<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
) -> Result<Vec<roxmltree::Node<'a, 'i>>, ImageError> {
    let mut elements = Vec::new();
    for child in node.children() {
        if child.is_element() {
            elements.push(child);
        } else if child.is_text() && !child.text().unwrap_or("").trim().is_empty() {
            return Err(ImageError::BadShape);
        }
    }
    Ok(elements)
}

fn single_child<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'i>, ImageError> {
    match structure_children(node)?.as_slice() {
        [only] if only.tag_name().name() == name => Ok(*only),
        _ => Err(ImageError::BadShape),
    }
}

/// The text of an element that holds nothing else. A comment or an element splits the text, and
/// the first part alone would be a truncated value. `None` is an empty element.
fn text_only<'a>(node: roxmltree::Node<'a, '_>) -> Result<Option<&'a str>, ImageError> {
    let mut children = node.children();
    match (children.next(), children.next()) {
        (None, _) => Ok(None),
        (Some(only), None) if only.is_text() => Ok(only.text().map(str::trim)),
        _ => Err(ImageError::BadShape),
    }
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

    /// An image of segments that `parse` accepts, for the tests of other modules.
    pub(crate) fn image_of(segments: &[(u32, Vec<u8>)]) -> Image {
        let xml: Vec<String> = segments
            .iter()
            .map(|(adr_start, data)| segment_xml(&adr_start.to_string(), data))
            .collect();
        parse(&container(&xml)).expect("the test segments are valid")
    }

    fn in_firmware(body: &str) -> Vec<u8> {
        format!("<FirmwareContainer><firmware>{body}</firmware></FirmwareContainer>").into_bytes()
    }

    #[test]
    fn refuses_comment_inside_data() {
        let (a, b) = (STANDARD.encode([1u8; 3000]), STANDARD.encode([2u8; 3000]));
        let body = format!(
            "<Segment><adr_start>16384</adr_start><data>{a}<!-- split -->{b}</data></Segment>"
        );
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_element_inside_data() {
        let (a, b) = (STANDARD.encode([1u8; 3000]), STANDARD.encode([2u8; 3000]));
        let body =
            format!("<Segment><adr_start>16384</adr_start><data>{a}<x/>{b}</data></Segment>");
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_comment_inside_adr_start() {
        let body = format!(
            "<Segment><adr_start>16<!-- c -->384</adr_start><data>{}</data></Segment>",
            STANDARD.encode([1u8; 8])
        );
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_repeated_data() {
        let body = format!(
            "<Segment><adr_start>16384</adr_start><data>{}</data><data>{}</data></Segment>",
            STANDARD.encode([1u8; 8]),
            STANDARD.encode([2u8; 8])
        );
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_repeated_adr_start() {
        let body = format!(
            "<Segment><adr_start>16384</adr_start><adr_start>32768</adr_start><data>{}</data></Segment>",
            STANDARD.encode([1u8; 8])
        );
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_unknown_element_in_a_segment() {
        let body = format!(
            "<Segment><adr_start>16384</adr_start><data>{}</data><extra/></Segment>",
            STANDARD.encode([1u8; 8])
        );
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_segment_outside_firmware() {
        let bytes = format!(
            "<FirmwareContainer>{}</FirmwareContainer>",
            segment_xml("16384", &[1; 8])
        );
        assert_eq!(parse(bytes.as_bytes()), Err(ImageError::BadShape));
        let nested = format!(
            "<FirmwareContainer><firmware><group>{}</group></firmware></FirmwareContainer>",
            segment_xml("16384", &[1; 8])
        );
        assert_eq!(parse(nested.as_bytes()), Err(ImageError::BadShape));
    }

    #[test]
    fn refuses_second_firmware() {
        let one = format!("<firmware>{}</firmware>", segment_xml("16384", &[1; 8]));
        let two = format!("<firmware>{}</firmware>", segment_xml("32768", &[2; 8]));
        let bytes = format!("<FirmwareContainer>{one}{two}</FirmwareContainer>");
        assert_eq!(parse(bytes.as_bytes()), Err(ImageError::BadShape));
        assert_eq!(
            parse(b"<FirmwareContainer></FirmwareContainer>"),
            Err(ImageError::BadShape)
        );
    }

    #[test]
    fn refuses_text_between_segments() {
        let body = format!("stray{}", segment_xml("16384", &[1; 8]));
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadShape));
    }

    #[test]
    fn comments_and_white_space_between_elements_are_skipped() {
        let body = format!(
            "\r\n  <!-- a -->\r\n  <Segment>\r\n    <adr_start> 16384 </adr_start>\r\n    <data>{}</data>\r\n  </Segment>\r\n",
            STANDARD.encode([1u8; 8])
        );
        assert_eq!(parse(&in_firmware(&body)).unwrap().segments().len(), 1);
    }

    #[test]
    fn refuses_unaligned_start() {
        for adr_start in [16385u32, 0x4200, 0x4000 + 1023] {
            let bytes = one_segment(adr_start, &[1; 8]);
            assert_eq!(parse(&bytes), Err(ImageError::UnalignedStart { adr_start }));
        }
    }

    #[test]
    fn refuses_end_exactly_at_2_pow_32() {
        let bytes = one_segment(0xFFFF_FC00, &[1; 1024]);
        assert_eq!(
            parse(&bytes),
            Err(ImageError::EndAboveU32 {
                adr_start: 0xFFFF_FC00
            })
        );
    }

    #[test]
    fn refuses_dtd_entities() {
        let bytes = b"<?xml version=\"1.0\"?><!DOCTYPE a [<!ENTITY x \"AAAA\">]>\
<FirmwareContainer><firmware><Segment><adr_start>16384</adr_start><data>&x;</data></Segment></firmware></FirmwareContainer>";
        assert_eq!(parse(bytes), Err(ImageError::NotXml));
    }

    #[test]
    fn refuses_base64_with_line_breaks() {
        let b64 = STANDARD.encode([1u8; 300]);
        let wrapped = format!("{}\r\n{}", &b64[..76], &b64[76..]);
        let body = format!("<Segment><adr_start>16384</adr_start><data>{wrapped}</data></Segment>");
        assert_eq!(parse(&in_firmware(&body)), Err(ImageError::BadBase64));
    }

    #[test]
    fn refuses_equal_start() {
        let d = [1u8; 8];
        let bytes = container(&[segment_xml("16384", &d), segment_xml("16384", &d)]);
        assert_eq!(parse(&bytes), Err(ImageError::Overlap));
    }

    #[test]
    fn oversize_counts_the_sum_over_segments() {
        let data = vec![1u8; 600 * 1024];
        let bytes = container(&[segment_xml("16384", &data), segment_xml("1048576", &data)]);
        assert_eq!(parse(&bytes), Err(ImageError::Oversize));
    }

    #[test]
    fn parses_two_segments() {
        let bytes = container(&[
            segment_xml("32768", &[3; 10]),
            segment_xml("16384", &[1; 2000]),
        ]);
        let image = parse(&bytes).unwrap();
        let segments = image.segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].adr_start(), 0x4000);
        assert_eq!(segments[0].data(), vec![1; 2000]);
        assert_eq!(segments[1].adr_start(), 0x8000);
        assert_eq!(segments[1].data(), vec![3; 10]);
    }

    #[test]
    fn bom_is_accepted() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(one_segment(0x4000, &[7; 5]));
        assert_eq!(parse(&bytes).unwrap().segments()[0].data(), vec![7; 5]);
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
