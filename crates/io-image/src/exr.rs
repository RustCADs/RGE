//! OpenEXR load/save via the `exr` crate.
//!
//! EXR is the canonical HDR float format. We always import to
//! [`PixelFormat::Rgba32F`] and only accept that format for save.
//!
//! Compression: ZIP (default lossless). The W18 spec asserts EXR round-trip
//! float values within `1e-5`.

use std::io::Cursor;

use exr::image::pixel_vec::PixelVec;
use exr::math::Vec2;
use exr::prelude::{
    read, Compression, Encoding, Image as ExrImage, LineOrder, ReadChannels, ReadLayers,
    SpecificChannels, WritableImage,
};

use crate::error::{ImageError, Result};
use crate::image_data::{Image, PixelFormat};

impl From<exr::error::Error> for ImageError {
    fn from(e: exr::error::Error) -> Self {
        Self::Exr(e.to_string())
    }
}

/// Load an OpenEXR image; always produces `Rgba32F`.
///
/// The first valid layer with RGBA channels is used. Missing alpha is filled
/// with 1.0 by exr crate convention.
pub fn load_exr(bytes: &[u8]) -> Result<Image> {
    let cursor = Cursor::new(bytes);

    // We use `rgba_channels` with closures that construct/fill our flat byte
    // buffer in scanline order. f32 samples are pulled directly from native.
    let pixels: ExrImage<
        exr::image::Layer<exr::image::SpecificChannels<RgbaPixelVec, exr::image::RgbaChannels>>,
    > = read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
            |resolution: Vec2<usize>, _channels: &exr::image::RgbaChannels| {
                let total = resolution.0 * resolution.1 * 4;
                RgbaPixelVec {
                    width: resolution.0,
                    height: resolution.1,
                    data: vec![0.0_f32; total],
                }
            },
            |out: &mut RgbaPixelVec, position: Vec2<usize>, (r, g, b, a): (f32, f32, f32, f32)| {
                let idx = (position.1 * out.width + position.0) * 4;
                out.data[idx] = r;
                out.data[idx + 1] = g;
                out.data[idx + 2] = b;
                out.data[idx + 3] = a;
            },
        )
        .first_valid_layer()
        .all_attributes()
        .from_buffered(cursor)?;

    let layer_data = pixels.layer_data;
    let channel = layer_data.channel_data;
    let pixmap = channel.pixels;
    let width = u32::try_from(pixmap.width)
        .map_err(|_| ImageError::Decode("EXR width too large".into()))?;
    let height = u32::try_from(pixmap.height)
        .map_err(|_| ImageError::Decode("EXR height too large".into()))?;
    Ok(Image::from_rgba32f(width, height, &pixmap.data))
}

/// Save an `Rgba32F` image as OpenEXR with default ZIP compression.
pub fn save_exr(img: &Image) -> Result<Vec<u8>> {
    if img.pixel_format != PixelFormat::Rgba32F {
        return Err(ImageError::UnsupportedPixelFormat {
            codec: "exr",
            actual: img.pixel_format,
        });
    }
    let pixel_count = (img.width as usize) * (img.height as usize);

    // Decode all f32 samples up front from the byte buffer so we can capture
    // them by reference inside the per-pixel closure.
    let samples: Vec<(f32, f32, f32, f32)> = (0..pixel_count)
        .map(|i| {
            let base = i * 16;
            let r = f32::from_le_bytes([
                img.pixels[base],
                img.pixels[base + 1],
                img.pixels[base + 2],
                img.pixels[base + 3],
            ]);
            let g = f32::from_le_bytes([
                img.pixels[base + 4],
                img.pixels[base + 5],
                img.pixels[base + 6],
                img.pixels[base + 7],
            ]);
            let b = f32::from_le_bytes([
                img.pixels[base + 8],
                img.pixels[base + 9],
                img.pixels[base + 10],
                img.pixels[base + 11],
            ]);
            let a = f32::from_le_bytes([
                img.pixels[base + 12],
                img.pixels[base + 13],
                img.pixels[base + 14],
                img.pixels[base + 15],
            ]);
            (r, g, b, a)
        })
        .collect();

    let pixel_vec = PixelVec::new(Vec2(img.width as usize, img.height as usize), samples);
    let channels = SpecificChannels::rgba(pixel_vec);
    let exr_image = ExrImage::from_encoded_channels(
        (img.width as usize, img.height as usize),
        Encoding {
            compression: Compression::ZIP1,
            line_order: LineOrder::Increasing,
            ..Encoding::default()
        },
        channels,
    );

    let mut out: Vec<u8> = Vec::new();
    exr_image.write().to_buffered(Cursor::new(&mut out))?;
    Ok(out)
}

/// Internal flat-vec image type, scanline-row-major `[r,g,b,a, r,g,b,a, ...]`.
struct RgbaPixelVec {
    width: usize,
    height: usize,
    data: Vec<f32>,
}

// Helper accessors to satisfy clippy & expose for the from_buffered closure.
impl RgbaPixelVec {
    #[allow(dead_code)]
    fn pixel_count(&self) -> usize {
        self.width * self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_rejects_non_float() {
        let img = Image::zeros(4, 4, PixelFormat::Rgba8);
        let err = save_exr(&img).unwrap_err();
        assert!(matches!(
            err,
            ImageError::UnsupportedPixelFormat { codec: "exr", .. }
        ));
    }
}
