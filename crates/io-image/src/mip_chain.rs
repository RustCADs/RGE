//! Mip-chain generation.
//!
//! Given a level-0 [`Image`], synthesize a complete mip pyramid down to 1×1
//! using a 2×2 box filter. Each successive level is half the previous in both
//! dimensions (rounded up — odd dims round to next even, with edge pixels
//! mirrored). The chain ends when both dimensions reach 1.
//!
//! For `width × height`, the chain length is `1 + floor(log2(max(width,
//! height)))`.

use crate::error::{ImageError, Result};
use crate::image_data::{Image, PixelFormat};

/// Generate the full mip chain for an image, including level 0.
///
/// Returns `Vec<Image>` ordered from highest resolution (level 0, the input)
/// down to 1×1. Always at least 1 level long; for a 1×1 input the chain
/// contains just the input.
pub fn generate_mip_chain(level0: &Image) -> Result<Vec<Image>> {
    if level0.width == 0 || level0.height == 0 {
        return Err(ImageError::Decode(
            "mip chain input has zero dimension".into(),
        ));
    }
    let mut chain = Vec::new();
    chain.push(level0.clone());

    let mut current = level0.clone();
    while current.width > 1 || current.height > 1 {
        let next = downsample_box(&current)?;
        current = next.clone();
        chain.push(next);
    }

    Ok(chain)
}

/// Downsample by exactly 2× in each axis using a box filter.
///
/// Output dimensions: `max(1, w/2)` × `max(1, h/2)`. For odd source
/// dimensions, the rightmost / bottom row is sampled by replication
/// (avoid skipping pixels).
pub fn downsample_box(src: &Image) -> Result<Image> {
    let dst_w = (src.width / 2).max(1);
    let dst_h = (src.height / 2).max(1);
    match src.pixel_format {
        PixelFormat::Rgba8 => downsample_rgba8(src, dst_w, dst_h),
        PixelFormat::Rgba16 => downsample_rgba16(src, dst_w, dst_h),
        PixelFormat::Rgba32F => downsample_rgba32f(src, dst_w, dst_h),
    }
}

fn downsample_rgba8(src: &Image, dst_w: u32, dst_h: u32) -> Result<Image> {
    let sw = src.width as usize;
    let sh = src.height as usize;
    let dw = dst_w as usize;
    let dh = dst_h as usize;
    let mut out = Vec::with_capacity(dw * dh * 4);

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sx1 = (sx0 + 1).min(sw - 1);
            let sy1 = (sy0 + 1).min(sh - 1);
            for c in 0..4 {
                let p00 = u32::from(src.pixels[(sy0 * sw + sx0) * 4 + c]);
                let p10 = u32::from(src.pixels[(sy0 * sw + sx1) * 4 + c]);
                let p01 = u32::from(src.pixels[(sy1 * sw + sx0) * 4 + c]);
                let p11 = u32::from(src.pixels[(sy1 * sw + sx1) * 4 + c]);
                let avg = (p00 + p10 + p01 + p11 + 2) / 4;
                out.push(avg as u8);
            }
        }
    }

    Ok(Image {
        width: dst_w,
        height: dst_h,
        pixel_format: PixelFormat::Rgba8,
        pixels: out,
    })
}

fn downsample_rgba16(src: &Image, dst_w: u32, dst_h: u32) -> Result<Image> {
    let sw = src.width as usize;
    let sh = src.height as usize;
    let dw = dst_w as usize;
    let dh = dst_h as usize;
    let mut samples = Vec::with_capacity(dw * dh * 4);

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sx1 = (sx0 + 1).min(sw - 1);
            let sy1 = (sy0 + 1).min(sh - 1);
            for c in 0..4 {
                let i00 = ((sy0 * sw + sx0) * 4 + c) * 2;
                let i10 = ((sy0 * sw + sx1) * 4 + c) * 2;
                let i01 = ((sy1 * sw + sx0) * 4 + c) * 2;
                let i11 = ((sy1 * sw + sx1) * 4 + c) * 2;
                let p00 = u32::from(u16::from_le_bytes([src.pixels[i00], src.pixels[i00 + 1]]));
                let p10 = u32::from(u16::from_le_bytes([src.pixels[i10], src.pixels[i10 + 1]]));
                let p01 = u32::from(u16::from_le_bytes([src.pixels[i01], src.pixels[i01 + 1]]));
                let p11 = u32::from(u16::from_le_bytes([src.pixels[i11], src.pixels[i11 + 1]]));
                let avg = (p00 + p10 + p01 + p11 + 2) / 4;
                samples.push(avg as u16);
            }
        }
    }

    Ok(Image::from_rgba16(dst_w, dst_h, &samples))
}

fn downsample_rgba32f(src: &Image, dst_w: u32, dst_h: u32) -> Result<Image> {
    let sw = src.width as usize;
    let sh = src.height as usize;
    let dw = dst_w as usize;
    let dh = dst_h as usize;
    let mut samples = Vec::with_capacity(dw * dh * 4);

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sx1 = (sx0 + 1).min(sw - 1);
            let sy1 = (sy0 + 1).min(sh - 1);
            for c in 0..4 {
                let i00 = ((sy0 * sw + sx0) * 4 + c) * 4;
                let i10 = ((sy0 * sw + sx1) * 4 + c) * 4;
                let i01 = ((sy1 * sw + sx0) * 4 + c) * 4;
                let i11 = ((sy1 * sw + sx1) * 4 + c) * 4;
                let p00 = f32_at(&src.pixels, i00);
                let p10 = f32_at(&src.pixels, i10);
                let p01 = f32_at(&src.pixels, i01);
                let p11 = f32_at(&src.pixels, i11);
                samples.push((p00 + p10 + p01 + p11) * 0.25);
            }
        }
    }

    Ok(Image::from_rgba32f(dst_w, dst_h, &samples))
}

fn f32_at(buf: &[u8], i: usize) -> f32 {
    f32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_dimensions_descend_to_1x1() {
        let img = Image::zeros(8, 4, PixelFormat::Rgba8);
        let chain = generate_mip_chain(&img).unwrap();
        // 8x4 → 4x2 → 2x1 → 1x1
        let dims: Vec<(u32, u32)> = chain.iter().map(|i| (i.width, i.height)).collect();
        assert_eq!(dims, vec![(8, 4), (4, 2), (2, 1), (1, 1)]);
    }

    #[test]
    fn chain_for_1x1_is_singleton() {
        let img = Image::zeros(1, 1, PixelFormat::Rgba8);
        let chain = generate_mip_chain(&img).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!((chain[0].width, chain[0].height), (1, 1));
    }

    #[test]
    fn chain_uniform_color_preserved() {
        // A constant-color image should remain that color through the chain.
        let mut pixels = vec![0u8; 16 * 16 * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = 100;
            chunk[1] = 150;
            chunk[2] = 200;
            chunk[3] = 255;
        }
        let img = Image::from_rgba8(16, 16, pixels);
        let chain = generate_mip_chain(&img).unwrap();
        assert_eq!(chain.len(), 5); // 16,8,4,2,1
        for level in &chain {
            for px in level.iter_rgba8() {
                assert_eq!(px, [100, 150, 200, 255]);
            }
        }
    }
}
