//! A small PNG file can declare a picture of gigabytes: the import refuses it before it
//! allocates the picture. The fixture has 17582 bytes and 12000 x 12000 pixels.

use std::time::Instant;

use nicecooling::bitmap::import_png;

#[test]
fn a_png_with_a_huge_size_is_refused() {
    let file = include_bytes!("fixtures/huge-12000.png");
    let started = Instant::now();
    let result = import_png(file, 128, 64);
    eprintln!(
        "file {} bytes, decoded in {:?}, ok = {}",
        file.len(),
        started.elapsed(),
        result.is_ok()
    );
    assert!(
        result.is_err(),
        "a 12000 x 12000 PNG of {} bytes was decoded",
        file.len()
    );
}
