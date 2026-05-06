//! JPEG load/save via the `image` crate.
//!
//! JPEG is fundamentally 8-bit YCbCr; we always decode to [`PixelFormat::Rgba8`]
//! (with alpha=255), and accept only `Rgba8` for save.
//!
//! Quality parameter for save is in the range `1..=100`. Image-rs' encoder
//! defines higher = better quality / larger file. The W18 spec asserts
//! quality 95 round-trip PSNR > 40 dB.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;

use crate::error::{ImageError, Result};
use crate::image_data::{Image, PixelFormat};

/// Load a JPEG from a byte buffer; always produces `Rgba8`.
pub fn load_jpeg(bytes: &[u8]) -> Result<Image> {
    let cursor = Cursor::new(bytes);
    let decoder = image::codecs::jpeg::JpegDecoder::new(cursor)?;
    let dyn_img = DynamicImage::from_decoder(decoder)?;
    let buf = dyn_img.to_rgba8();
    let width = buf.width();
    let height = buf.height();
    let raw = buf.into_raw();
    Ok(Image::from_rgba8(width, height, raw))
}

/// Save an [`Image`] as JPEG. `quality` ∈ `[1, 100]` (typical: 75 ≈ web).
///
/// JPEG has no alpha channel; the alpha plane of an `Rgba8` is silently
/// dropped (alpha pre-multiplication is the caller's responsibility if
/// they care about transparent edges).
pub fn save_jpeg(img: &Image, quality: u8) -> Result<Vec<u8>> {
    if img.pixel_format != PixelFormat::Rgba8 {
        return Err(ImageError::UnsupportedPixelFormat {
            codec: "jpeg",
            actual: img.pixel_format,
        });
    }
    let quality = quality.clamp(1, 100);

    // Strip alpha — pack RGB bytes.
    let pixel_count = (img.width as usize) * (img.height as usize);
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for chunk in img.pixels.chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }

    let mut out = Vec::new();
    let cursor = Cursor::new(&mut out);
    let mut encoder = JpegEncoder::new_with_quality(cursor, quality);
    encoder.encode(&rgb, img.width, img.height, image::ExtendedColorType::Rgb8)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_rgba8_save_rejected() {
        let img = Image::zeros(4, 4, PixelFormat::Rgba16);
        let err = save_jpeg(&img, 90).unwrap_err();
        assert!(matches!(
            err,
            ImageError::UnsupportedPixelFormat { codec: "jpeg", .. }
        ));
    }
}
