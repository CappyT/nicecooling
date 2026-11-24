//! One-bit images of the display. The device stores them in two layouts (report 5 and report 8),
//! and the D-Bus API uses a third: rows from the top, each row padded to a whole byte, bit 7 the
//! leftmost pixel, 1 lit. `docs/protocol/leakshield.md` (section Display) describes the layouts.

use std::fmt;

/// Page layout of report 5: 8 rows of `width` column bytes. Bit `n` of byte `row * width + x` is
/// the pixel at `y = row * 8 + n`.
fn page_layout_bit(bytes: &[u8], width: usize, x: usize, y: usize) -> bool {
    bytes[(y / 8) * width + x] >> (y % 8) & 1 == 1
}

/// Column layout of report 8: `ceil(height / 8)` bytes per column, column after column. Bit `n`
/// of byte `x * ceil(height / 8) + row` is the pixel at `y = row * 8 + n`.
fn column_layout_bit(bytes: &[u8], height: usize, x: usize, y: usize) -> bool {
    bytes[x * height.div_ceil(8) + y / 8] >> (y % 8) & 1 == 1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitmapError {
    /// The byte count does not fit the size.
    Length { expected: usize, actual: usize },
    /// The image has another size than the place that takes it.
    Size {
        expected: (u16, u16),
        actual: (u16, u16),
    },
}

impl fmt::Display for BitmapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length { expected, actual } => {
                write!(f, "the image holds {actual} bytes, {expected} expected")
            }
            Self::Size { expected, actual } => write!(
                f,
                "the image is {} x {} pixels, {} x {} expected",
                actual.0, actual.1, expected.0, expected.1
            ),
        }
    }
}

impl std::error::Error for BitmapError {}

/// A black and white image. `true` is a lit pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: u16,
    height: u16,
    /// `width * height` pixels, row after row.
    pixels: Vec<bool>,
}

impl Bitmap {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            pixels: vec![false; usize::from(width) * usize::from(height)],
        }
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn get(&self, x: u16, y: u16) -> bool {
        self.pixels[usize::from(y) * usize::from(self.width) + usize::from(x)]
    }

    pub fn set(&mut self, x: u16, y: u16, lit: bool) {
        self.pixels[usize::from(y) * usize::from(self.width) + usize::from(x)] = lit;
    }

    fn from_fn(width: u16, height: u16, lit: impl Fn(usize, usize) -> bool) -> Self {
        let mut bitmap = Self::new(width, height);
        for y in 0..height {
            for x in 0..width {
                bitmap.set(x, y, lit(usize::from(x), usize::from(y)));
            }
        }
        bitmap
    }

    /// The report 5 layout (`width` x `height`, `height` a multiple of 8).
    pub fn from_page_layout(width: u16, height: u16, bytes: &[u8]) -> Result<Self, BitmapError> {
        let expected = usize::from(width) * usize::from(height).div_ceil(8);
        if bytes.len() != expected {
            return Err(BitmapError::Length {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self::from_fn(width, height, |x, y| {
            page_layout_bit(bytes, usize::from(width), x, y)
        }))
    }

    /// The report 8 layout, used by the logo and the icons.
    pub fn from_column_layout(width: u16, height: u16, bytes: &[u8]) -> Result<Self, BitmapError> {
        let expected = usize::from(width) * usize::from(height).div_ceil(8);
        if bytes.len() != expected {
            return Err(BitmapError::Length {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self::from_fn(width, height, |x, y| {
            column_layout_bit(bytes, usize::from(height), x, y)
        }))
    }

    /// The report 8 layout. The bits above the last row stay 0.
    pub fn to_column_layout(&self) -> Vec<u8> {
        let rows = usize::from(self.height).div_ceil(8);
        let mut bytes = vec![0; usize::from(self.width) * rows];
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    bytes[usize::from(x) * rows + usize::from(y) / 8] |= 1 << (y % 8);
                }
            }
        }
        bytes
    }

    /// The layout of the D-Bus API: rows from the top, each row padded to a whole byte, bit 7 the
    /// leftmost pixel, 1 lit.
    pub fn to_rows(&self) -> Vec<u8> {
        let stride = usize::from(self.width).div_ceil(8);
        let mut bytes = vec![0; stride * usize::from(self.height)];
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    bytes[usize::from(y) * stride + usize::from(x) / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        bytes
    }

    pub fn from_rows(width: u16, height: u16, bytes: &[u8]) -> Result<Self, BitmapError> {
        let stride = usize::from(width).div_ceil(8);
        let expected = stride * usize::from(height);
        if bytes.len() != expected {
            return Err(BitmapError::Length {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self::from_fn(width, height, |x, y| {
            bytes[y * stride + x / 8] & (0x80 >> (x % 8)) != 0
        }))
    }

    /// `Ok` when the image has exactly `size`.
    pub fn expect_size(&self, size: (u16, u16)) -> Result<(), BitmapError> {
        if self.size() == size {
            Ok(())
        } else {
            Err(BitmapError::Size {
                expected: size,
                actual: self.size(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pixel set in a 3 x 10 image is where each layout says it is.
    #[test]
    fn layouts_place_a_pixel() {
        let mut bitmap = Bitmap::new(3, 10);
        bitmap.set(2, 9, true);

        // Column layout: 2 bytes per column, pixel (2, 9) is bit 1 of the second byte of column 2.
        let columns = bitmap.to_column_layout();
        assert_eq!(columns.len(), 6);
        assert_eq!(columns, [0, 0, 0, 0, 0, 0b10]);
        assert_eq!(Bitmap::from_column_layout(3, 10, &columns).unwrap(), bitmap);

        // Rows: 1 byte per row, pixel x = 2 is bit 5.
        let rows = bitmap.to_rows();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[9], 0b0010_0000);
        assert!(rows[..9].iter().all(|&b| b == 0));
        assert_eq!(Bitmap::from_rows(3, 10, &rows).unwrap(), bitmap);
    }

    #[test]
    fn page_layout_has_rows_of_column_bytes() {
        // 128 x 64: pixel (5, 9) is bit 1 of byte 128 + 5.
        let mut bytes = vec![0; 1024];
        bytes[128 + 5] = 0b10;
        let bitmap = Bitmap::from_page_layout(128, 64, &bytes).unwrap();
        assert!(bitmap.get(5, 9));
        assert_eq!(bitmap.pixels.iter().filter(|&&p| p).count(), 1);
    }

    #[test]
    fn a_wrong_byte_count_is_refused() {
        assert_eq!(
            Bitmap::from_page_layout(128, 64, &[0; 1023]),
            Err(BitmapError::Length {
                expected: 1024,
                actual: 1023
            })
        );
        assert!(Bitmap::from_rows(23, 23, &[0; 68]).is_err());
        assert!(Bitmap::new(23, 23).expect_size((128, 64)).is_err());
    }
}
