//! PNG round-trip integration tests.
//!
//! Spec exit: PNG round-trip is **pixel-exact** (lossless format).

mod common;

use rge_io_image::{detect_format, png, ImageFormat, PixelFormat};

#[test]
fn rgba8_roundtrip_pixel_exact() {
    let original = common::synthetic_rgba8();
    let bytes = png::save_png(&original).expect("save");
    assert_eq!(detect_format(&bytes), Some(ImageFormat::Png));
    let decoded = png::load_png(&bytes).expect("load");

    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
    assert_eq!(decoded.pixel_format, PixelFormat::Rgba8);
    assert_eq!(decoded.pixels, original.pixels);
}

#[test]
fn rgba16_roundtrip_pixel_exact() {
    let original = common::synthetic_rgba16();
    let bytes = png::save_png(&original).expect("save");
    assert_eq!(detect_format(&bytes), Some(ImageFormat::Png));
    let decoded = png::load_png(&bytes).expect("load");

    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
    assert_eq!(decoded.pixel_format, PixelFormat::Rgba16);
    assert_eq!(decoded.pixels, original.pixels);
}

#[test]
fn fixture_file_loads() {
    common::ensure_fixtures();
    let path = common::fixtures_dir().join("test.png");
    let img = rge_io_image::load_path(&path).expect("load fixture");
    assert_eq!(img.width, 16);
    assert_eq!(img.height, 16);
    assert_eq!(img.pixel_format, PixelFormat::Rgba8);
}

#[test]
fn detect_via_magic_not_extension() {
    common::ensure_fixtures();
    // Write the PNG bytes to a file with `.bogus` extension; detection must
    // still work because we read magic bytes, not the extension.
    let path = common::fixtures_dir().join("test.png");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(detect_format(&bytes), Some(ImageFormat::Png));
}
