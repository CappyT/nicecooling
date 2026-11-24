//! One-bit images of the Leakshield display: the bitmap of the daemon contract, its draw model
//! and the conversion of a PNG file into it.

/// A bitmap of `docs/api.md` (Leakshield display): rows from the top, each row padded to a whole
/// byte, bit 7 the leftmost pixel, 1 lit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u16,
    pub height: u16,
    pub rows: Vec<u8>,
}

/// The size of a display page and of the logo.
pub const PAGE_SIZE: (u16, u16) = (128, 64);
/// The size of an icon.
pub const ICON_SIZE: (u16, u16) = (23, 23);

impl Bitmap {
    /// A dark bitmap.
    pub fn new(width: u16, height: u16) -> Self {
        let stride = usize::from(width).div_ceil(8);
        Self {
            width,
            height,
            rows: vec![0; stride * usize::from(height)],
        }
    }

    /// The bytes of one row.
    pub fn stride(&self) -> usize {
        usize::from(self.width).div_ceil(8)
    }

    /// The byte count fits the size, and the size is not zero.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rows.len() == self.stride() * usize::from(self.height)
    }

    /// The pixel at column `x` and line `y`. A pixel outside the bitmap is dark.
    pub fn pixel(&self, x: u16, y: u16) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let byte = usize::from(y) * self.stride() + usize::from(x) / 8;
        self.rows
            .get(byte)
            .is_some_and(|bits| bits & (0x80 >> (x % 8)) != 0)
    }

    pub fn set(&mut self, x: u16, y: u16, lit: bool) {
        if x >= self.width || y >= self.height {
            return;
        }
        let byte = usize::from(y) * self.stride() + usize::from(x) / 8;
        let mask = 0x80 >> (x % 8);
        if let Some(bits) = self.rows.get_mut(byte) {
            if lit {
                *bits |= mask;
            } else {
                *bits &= !mask;
            }
        }
    }

    pub fn lit_count(&self) -> usize {
        self.runs().iter().map(|run| usize::from(run.2)).sum()
    }

    /// The draw model: one `(x, y, length)` for each horizontal run of lit pixels, from the top
    /// line down and from the left. The view paints one rectangle for each run.
    pub fn runs(&self) -> Vec<(u16, u16, u16)> {
        let mut runs = Vec::new();
        for y in 0..self.height {
            let mut start = None;
            for x in 0..self.width {
                match (self.pixel(x, y), start) {
                    (true, None) => start = Some(x),
                    (false, Some(first)) => {
                        runs.push((first, y, x - first));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(first) = start {
                runs.push((first, y, self.width - first));
            }
        }
        runs
    }
}

/// A picture in the size of its target, as brightness from 0 (dark) to 255.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    pub width: u16,
    pub height: u16,
    pub luma: Vec<u8>,
    /// The size of the file.
    pub source: (u32, u32),
}

impl Gray {
    /// The file had another size than the target: the picture was scaled.
    pub fn scaled(&self) -> bool {
        self.source != (u32::from(self.width), u32::from(self.height))
    }

    /// The one-bit image. A pixel is lit when its brightness is `threshold` percent or more.
    /// `invert` then swaps lit and dark.
    pub fn to_bitmap(&self, threshold: u8, invert: bool) -> Bitmap {
        let mut bitmap = Bitmap::new(self.width, self.height);
        let limit = u32::from(threshold.min(100)) * 255;
        for y in 0..self.height {
            for x in 0..self.width {
                let index = usize::from(y) * usize::from(self.width) + usize::from(x);
                let bright = self
                    .luma
                    .get(index)
                    .is_some_and(|luma| u32::from(*luma) * 100 >= limit);
                bitmap.set(x, y, bright != invert);
            }
        }
        bitmap
    }
}

/// Why a file gives no picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The file cannot be read. The text is the error of the system.
    Read(String),
    /// The bytes are not a PNG image that the decoder reads.
    NotPng,
}

/// The default of the threshold slider: the device software writes a pixel as lit from half
/// brightness up.
pub const DEFAULT_THRESHOLD: u8 = 50;

/// Decode a PNG file into a picture of `width` x `height`. A file of another size is not
/// refused: it is scaled to fit, with its aspect ratio, and centered on a dark ground. The user
/// sees the result before the write, so any picture is a valid start. A transparent pixel is dark.
pub fn import_png(bytes: &[u8], width: u16, height: u16) -> Result<Gray, ImportError> {
    use image::imageops::{self, FilterType};

    let decoded = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes))
        .and_then(image::DynamicImage::from_decoder)
        .map_err(|_| ImportError::NotPng)?
        .to_rgba8();
    let source = decoded.dimensions();
    if source.0 == 0 || source.1 == 0 {
        return Err(ImportError::NotPng);
    }
    let (target_w, target_h) = (u32::from(width), u32::from(height));
    let fitted = if source == (target_w, target_h) {
        decoded
    } else {
        // The largest size with the aspect ratio of the file that fits the target.
        let (w, h) = if u64::from(source.0) * u64::from(target_h)
            >= u64::from(source.1) * u64::from(target_w)
        {
            let h = (u64::from(source.1) * u64::from(target_w)).div_ceil(u64::from(source.0));
            (target_w, (h as u32).clamp(1, target_h))
        } else {
            let w = (u64::from(source.0) * u64::from(target_h)).div_ceil(u64::from(source.1));
            ((w as u32).clamp(1, target_w), target_h)
        };
        if w < source.0 {
            // A smaller picture takes the mean of the pixels that each new pixel covers.
            imageops::thumbnail(&decoded, w, h)
        } else {
            // A larger picture keeps hard edges: an icon drawn pixel by pixel stays sharp.
            imageops::resize(&decoded, w, h, FilterType::Nearest)
        }
    };
    let (w, h) = fitted.dimensions();
    let (left, top) = ((target_w - w) / 2, (target_h - h) / 2);
    let mut luma = vec![0u8; usize::from(width) * usize::from(height)];
    for (x, y, pixel) in fitted.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let bright =
            (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) * u32::from(a) / 255;
        let index = ((top + y) * target_w + left + x) as usize;
        luma[index] = ((bright + 500) / 1000).min(255) as u8;
    }
    Ok(Gray {
        width,
        height,
        luma,
        source,
    })
}

/// Read a PNG file and decode it with `import_png`.
pub fn import_png_file(
    path: &std::path::Path,
    width: u16,
    height: u16,
) -> Result<Gray, ImportError> {
    let bytes = std::fs::read(path).map_err(|error| ImportError::Read(error.to_string()))?;
    import_png(&bytes, width, height)
}
