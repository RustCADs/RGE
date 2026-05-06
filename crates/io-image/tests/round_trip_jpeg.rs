//! JPEG round-trip integration tests.
//!
//! Spec exit: JPEG round-trip at quality 95 yields PSNR > 40 dB.

mod common;

use rge_io_image::{detect_format, jpeg, ImageFormat, PixelFormat};

/// Compute the per-channel PSNR (peak signal-to-noise ratio) over RGB only
/// (alpha is dropped during JPEG save). Uses 255.0 as the peak; returns
/// `f64::INFINITY` when MSE is zero.
fn psnr_rgb(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len() % 4, 0);
    let pixel_count = a.len() / 4;
    let mut mse_sum = 0.0_f64;
    for i in 0..pixel_count {
        for c in 0..3 {
            let av = f64::from(a[i * 4 + c]);
            let bv = f64::from(b[i * 4 + c]);
            let diff = av - bv;
            mse_sum += diff * diff;
        }
    }
    let mse = mse_sum / (pixel_count as f64 * 3.0);
    if mse == 0.0 {
        return f64::INFINITY;
    }
    let peak = 255.0_f64;
    10.0 * (peak * peak / mse).log10()
}

#[test]
fn quality_95_psnr_above_40db() {
    let original = common::synthetic_rgba8();
    let bytes = jpeg::save_jpeg(&original, 95).expect("save");
    assert_eq!(detect_format(&bytes), Some(ImageFormat::Jpeg));
    let decoded = jpeg::load_jpeg(&bytes).expect("load");

    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
    assert_eq!(decoded.pixel_format, PixelFormat::Rgba8);

    let psnr = psnr_rgb(&original.pixels, &decoded.pixels);
    assert!(
        psnr > 40.0,
        "PSNR {psnr} dB at quality 95 should exceed 40 dB"
    );
}

#[test]
fn fixture_file_loads() {
    common::ensure_fixtures();
    let path = common::fixtures_dir().join("test.jpg");
    let img = rge_io_image::load_path(&path).expect("load fixture");
    assert_eq!(img.pixel_format, PixelFormat::Rgba8);
    assert_eq!(img.width, 16);
    assert_eq!(img.height, 16);
}

#[test]
fn rejects_non_rgba8() {
    let img = common::synthetic_rgba32f();
    let err = jpeg::save_jpeg(&img, 90).unwrap_err();
    assert!(matches!(
        err,
        rge_io_image::ImageError::UnsupportedPixelFormat { codec: "jpeg", .. }
    ));
}
