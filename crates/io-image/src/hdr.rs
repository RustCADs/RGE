//! Radiance HDR (`.hdr`) load/save via the `image` crate.
//!
//! Radiance HDR (a.k.a. RGBE) is an 8-bit-mantissa-+-shared-exponent float
//! format. We expose float in/out via [`PixelFormat::Rgba32F`] (alpha is
//! synthetic — RGBE has no alpha; we set 1.0 on load and drop on save).

use std::io::Cursor;

use image::codecs::hdr::{HdrDecoder, HdrEncoder};
use image::ImageDecoder;

use crate::error::{ImageError, Result};
use crate::image_data::{Image, PixelFormat};

/// Load a Radiance HDR; produces `Rgba32F` (alpha=1.0).
pub fn load_hdr(bytes: &[u8]) -> Result<Image> {
    let cursor = Cursor::new(bytes);
    let decoder = HdrDecoder::new(cursor)?;
    let (width, height) = decoder.dimensions();
    let pixel_count = (width as usize) * (height as usize);

    // HDR decoder yields Rgb32F via image-rs ImageDecoder trait. Allocate
    // bytes for `pixel_count * 3 * 4` then convert to RGBA32F.
    let mut rgb_bytes = vec![0u8; pixel_count * 12];
    decoder.read_image(&mut rgb_bytes)?;

    // Reinterpret rgb_bytes as f32 quartets (3 per pixel) and assemble RGBA.
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    for i in 0..pixel_count {
        let base = i * 12;
        let r = f32::from_le_bytes([
            rgb_bytes[base],
            rgb_bytes[base + 1],
            rgb_bytes[base + 2],
            rgb_bytes[base + 3],
        ]);
        let g = f32::from_le_bytes([
            rgb_bytes[base + 4],
            rgb_bytes[base + 5],
            rgb_bytes[base + 6],
            rgb_bytes[base + 7],
        ]);
        let b = f32::from_le_bytes([
            rgb_bytes[base + 8],
            rgb_bytes[base + 9],
            rgb_bytes[base + 10],
            rgb_bytes[base + 11],
        ]);
        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(1.0);
    }

    Ok(Image::from_rgba32f(width, height, &rgba))
}

/// Save `Rgba32F` as Radiance HDR. Alpha is dropped.
pub fn save_hdr(img: &Image) -> Result<Vec<u8>> {
    if img.pixel_format != PixelFormat::Rgba32F {
        return Err(ImageError::UnsupportedPixelFormat {
            codec: "hdr",
            actual: img.pixel_format,
        });
    }

    let pixel_count = (img.width as usize) * (img.height as usize);
    let mut rgb: Vec<image::Rgb<f32>> = Vec::with_capacity(pixel_count);
    for chunk in img.pixels.chunks_exact(16) {
        let r = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let g = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        let b = f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
        rgb.push(image::Rgb([r, g, b]));
    }

    let mut out: Vec<u8> = Vec::new();
    let cursor = Cursor::new(&mut out);
    let encoder = HdrEncoder::new(cursor);
    encoder.encode(&rgb, img.width as usize, img.height as usize)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_rejects_non_float() {
        let img = Image::zeros(4, 4, PixelFormat::Rgba8);
        let err = save_hdr(&img).unwrap_err();
        assert!(matches!(
            err,
            ImageError::UnsupportedPixelFormat { codec: "hdr", .. }
        ));
    }
}
