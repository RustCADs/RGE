//! `OpenEXR` round-trip integration tests.
//!
//! Spec exit: float values within 1e-5 tolerance.

mod common;

use rge_io_image::{detect_format, exr, ImageFormat, PixelFormat};

const TOL: f32 = 1e-5;

#[test]
fn rgba32f_roundtrip_within_tolerance() {
    let original = common::synthetic_rgba32f();
    let bytes = exr::save_exr(&original).expect("save");
    assert_eq!(detect_format(&bytes), Some(ImageFormat::OpenExr));
    let decoded = exr::load_exr(&bytes).expect("load");

    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
    assert_eq!(decoded.pixel_format, PixelFormat::Rgba32F);

    let original_floats: Vec<[f32; 4]> = original.iter_rgba32f().collect();
    let decoded_floats: Vec<[f32; 4]> = decoded.iter_rgba32f().collect();
    assert_eq!(original_floats.len(), decoded_floats.len());

    for (i, (o, d)) in original_floats
        .iter()
        .zip(decoded_floats.iter())
        .enumerate()
    {
        for c in 0..4 {
            assert!(
                (o[c] - d[c]).abs() < TOL,
                "px[{i}].chan[{c}]: original={} decoded={} delta={}",
                o[c],
                d[c],
                (o[c] - d[c]).abs()
            );
        }
    }
}

#[test]
fn fixture_file_loads() {
    common::ensure_fixtures();
    let path = common::fixtures_dir().join("test.exr");
    let img = rge_io_image::load_path(&path).expect("load fixture");
    assert_eq!(img.pixel_format, PixelFormat::Rgba32F);
    assert_eq!(img.width, 8);
    assert_eq!(img.height, 8);
}
