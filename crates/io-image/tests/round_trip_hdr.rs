//! Radiance HDR round-trip integration tests.
//!
//! HDR uses RGBE encoding (8-bit mantissa + shared exponent); precision is
//! ~ 1% relative. We assert a generous tolerance for non-zero values.

mod common;

use rge_io_image::{detect_format, hdr, ImageFormat, PixelFormat};

#[test]
fn rgba32f_roundtrip_relative_tolerance() {
    let original = common::synthetic_rgba32f();
    let bytes = hdr::save_hdr(&original).expect("save");
    assert_eq!(detect_format(&bytes), Some(ImageFormat::RadianceHdr));
    let decoded = hdr::load_hdr(&bytes).expect("load");

    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
    assert_eq!(decoded.pixel_format, PixelFormat::Rgba32F);

    let original_floats: Vec<[f32; 4]> = original.iter_rgba32f().collect();
    let decoded_floats: Vec<[f32; 4]> = decoded.iter_rgba32f().collect();
    for (i, (o, d)) in original_floats
        .iter()
        .zip(decoded_floats.iter())
        .enumerate()
    {
        // RGB channels: ~1% relative tolerance.
        for c in 0..3 {
            let rel = if o[c].abs() < f32::EPSILON {
                d[c].abs()
            } else {
                ((o[c] - d[c]).abs()) / o[c].abs().max(1e-3)
            };
            assert!(
                rel < 0.05,
                "px[{i}].chan[{c}]: original={} decoded={} rel-err={}",
                o[c],
                d[c],
                rel
            );
        }
        // Alpha is synthetic (HDR has none); decoder fills with 1.0.
        assert!((d[3] - 1.0).abs() < 1e-6);
    }
}

#[test]
fn fixture_file_loads() {
    common::ensure_fixtures();
    let path = common::fixtures_dir().join("test.hdr");
    let img = rge_io_image::load_path(&path).expect("load fixture");
    assert_eq!(img.pixel_format, PixelFormat::Rgba32F);
}
