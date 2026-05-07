//! PNG load/save via the `image` crate.
//!
//! Bit-depth is preserved: 8-bit PNGs decode to [`PixelFormat::Rgba8`],
//! 16-bit PNGs decode to [`PixelFormat::Rgba16`]. Save mirrors the input
//! format. PNG itself is lossless, so any `Image` round-tripped through
//! `save_png` → `load_png` is pixel-exact.
//!
//! Note: 32-bit float PNG is *not* a thing in the spec; if a caller passes
//! `Rgba32F` to [`save_png`] we error rather than silently quantize. Use EXR
//! or HDR for float storage.

use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder};

use crate::error::{ImageError, Result};
use crate::image_data::{Image, PixelFormat};

/// Load a PNG from a byte buffer; bit-depth preserved.
pub fn load_png(bytes: &[u8]) -> Result<Image> {
    let cursor = Cursor::new(bytes);
    let decoder = image::codecs::png::PngDecoder::new(cursor)?;
    let dyn_img = DynamicImage::from_decoder(decoder)?;
    convert_dynimage_to_png_image(dyn_img)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "match arms call methods like `to_rgba8()` / `to_rgba16()` that take `&self` — clippy's heuristic doesn't see the consume-via-pattern shape; passing `&DynamicImage` would force `(&dyn_img).to_rgba8()` everywhere without functional benefit"
)]
fn convert_dynimage_to_png_image(dyn_img: DynamicImage) -> Result<Image> {
    let width = dyn_img.width();
    let height = dyn_img.height();
    let pixel_count = (width as usize) * (height as usize);

    // Inspect color depth by branching on the variant directly.
    match dyn_img {
        // Anything 8-bit or fewer bits-per-channel: promote to Rgba8.
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_) => {
            let buf = dyn_img.to_rgba8();
            let raw = buf.into_raw();
            debug_assert_eq!(raw.len(), pixel_count * 4);
            Ok(Image::from_rgba8(width, height, raw))
        }
        // Anything 16-bit per channel: stay 16-bit.
        DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => {
            let buf = dyn_img.to_rgba16();
            let raw_u16 = buf.into_raw();
            debug_assert_eq!(raw_u16.len(), pixel_count * 4);
            Ok(Image::from_rgba16(width, height, &raw_u16))
        }
        // PNG never produces float; if `image` returns one we treat it as
        // an unexpected upgrade and pass through.
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            let buf = dyn_img.to_rgba32f();
            let raw_f32 = buf.into_raw();
            debug_assert_eq!(raw_f32.len(), pixel_count * 4);
            Ok(Image::from_rgba32f(width, height, &raw_f32))
        }
        _ => Err(ImageError::Decode("unsupported PNG color type".into())),
    }
}

/// Save an [`Image`] as PNG. Lossless. `Rgba32F` is rejected (use EXR/HDR).
pub fn save_png(img: &Image) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let cursor = Cursor::new(&mut out);
    let encoder = PngEncoder::new(cursor);

    match img.pixel_format {
        PixelFormat::Rgba8 => {
            encoder.write_image(
                &img.pixels,
                img.width,
                img.height,
                image::ExtendedColorType::Rgba8,
            )?;
        }
        PixelFormat::Rgba16 => {
            // PngEncoder requires the raw byte view; image-rs interprets the
            // stride based on the color type. We need big-endian per spec but
            // the encoder handles endianness if we pass through ImageEncoder.
            // Use a typed buffer route to avoid endianness ambiguity.
            let pixel_count = (img.width as usize) * (img.height as usize);
            let mut samples = Vec::with_capacity(pixel_count * 4);
            for chunk in img.pixels.chunks_exact(2) {
                samples.push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }
            // Re-encode as native u16 for image-rs:
            let img16 =
                image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(img.width, img.height, samples)
                    .ok_or_else(|| ImageError::Encode("PNG 16-bit buffer size mismatch".into()))?;
            // Write through DynamicImage to leverage image-rs PNG writer.
            let dynimg = DynamicImage::ImageRgba16(img16);
            dynimg.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)?;
        }
        PixelFormat::Rgba32F => {
            return Err(ImageError::UnsupportedPixelFormat {
                codec: "png",
                actual: PixelFormat::Rgba32F,
            });
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba32f_save_rejected() {
        let img = Image::zeros(4, 4, PixelFormat::Rgba32F);
        let err = save_png(&img).unwrap_err();
        match err {
            ImageError::UnsupportedPixelFormat { codec, actual } => {
                assert_eq!(codec, "png");
                assert_eq!(actual, PixelFormat::Rgba32F);
            }
            other => panic!("expected UnsupportedPixelFormat, got {other:?}"),
        }
    }
}
