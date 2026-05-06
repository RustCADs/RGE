//! Shared test helpers.
//!
//! Generates vendored fixture files on first call and idempotently afterward.
//! Fixtures live at `crates/io-image/tests/fixtures/`.

#![allow(dead_code, unreachable_pub, let_underscore_drop)]

use std::path::PathBuf;
use std::sync::Once;

use rge_io_image::{exr, hdr, jpeg, png, Image};

static GEN: Once = Once::new();

/// Path to the vendored fixtures directory (relative to `CARGO_MANIFEST_DIR`).
pub fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("tests").join("fixtures")
}

/// Construct a deterministic 16×16 gradient image in `Rgba8`.
pub fn synthetic_rgba8() -> Image {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u32 {
        for x in 0..16u32 {
            pixels.push((x * 16) as u8);
            pixels.push((y * 16) as u8);
            pixels.push(((x + y) * 8) as u8);
            pixels.push(255);
        }
    }
    Image::from_rgba8(16, 16, pixels)
}

/// Construct a deterministic 8×8 gradient image in `Rgba32F`.
pub fn synthetic_rgba32f() -> Image {
    let mut samples = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8u32 {
        for x in 0..8u32 {
            samples.push(x as f32 / 7.0);
            samples.push(y as f32 / 7.0);
            samples.push((x as f32 + y as f32) / 14.0);
            samples.push(1.0);
        }
    }
    Image::from_rgba32f(8, 8, &samples)
}

/// Construct a deterministic 8×8 image in `Rgba16`.
pub fn synthetic_rgba16() -> Image {
    let mut samples = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8u32 {
        for x in 0..8u32 {
            samples.push((x as u16) * 8000);
            samples.push((y as u16) * 8000);
            samples.push(((x as u16) + (y as u16)) * 4000);
            samples.push(u16::MAX);
        }
    }
    Image::from_rgba16(8, 8, &samples)
}

/// Ensure on-disk fixtures exist; generates them on first test run.
pub fn ensure_fixtures() {
    GEN.call_once(|| {
        let dir = fixtures_dir();
        std::fs::create_dir_all(&dir).expect("create fixtures dir");

        // PNG fixture (16x16 RGBA8 gradient).
        let png_img = synthetic_rgba8();
        let png_bytes = png::save_png(&png_img).expect("save fixture PNG");
        std::fs::write(dir.join("test.png"), png_bytes).expect("write fixture PNG");

        // JPEG fixture (16x16 RGBA8 gradient at quality 95).
        let jpg_bytes = jpeg::save_jpeg(&png_img, 95).expect("save fixture JPEG");
        std::fs::write(dir.join("test.jpg"), jpg_bytes).expect("write fixture JPEG");

        // EXR fixture (8x8 RGBA32F gradient).
        let exr_img = synthetic_rgba32f();
        let exr_bytes = exr::save_exr(&exr_img).expect("save fixture EXR");
        std::fs::write(dir.join("test.exr"), exr_bytes).expect("write fixture EXR");

        // HDR fixture (8x8 RGBA32F gradient encoded as RGBE).
        let hdr_bytes = hdr::save_hdr(&exr_img).expect("save fixture HDR");
        std::fs::write(dir.join("test.hdr"), hdr_bytes).expect("write fixture HDR");
    });
}
