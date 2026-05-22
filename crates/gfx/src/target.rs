//! Headless render target — a GPU texture used as a `RENDER_ATTACHMENT` with
//! `COPY_SRC` so its pixels can be read back to CPU memory.

use crate::context::GfxContext;

/// Maximum allowed texture edge length (sanity cap).
const MAX_SIDE: u32 = 8192;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`HeadlessTarget`].
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    /// Width or height was zero, or exceeded [`MAX_SIDE`].
    #[error("invalid size: {0}x{1} (must be 1–{MAX_SIDE})")]
    InvalidSize(u32, u32),
}

// ---------------------------------------------------------------------------
// HeadlessTarget
// ---------------------------------------------------------------------------

/// A GPU texture suitable for use as a render target with CPU readback.
///
/// Format is always [`wgpu::TextureFormat::Rgba8Unorm`].
/// Usages: `RENDER_ATTACHMENT | COPY_SRC`.
#[derive(Debug)]
pub struct HeadlessTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

impl HeadlessTarget {
    /// Allocate a `width × height` `RGBA8Unorm` texture with usage
    /// `RENDER_ATTACHMENT | COPY_SRC`.
    ///
    /// # Errors
    ///
    /// Returns [`TargetError::InvalidSize`] when either dimension is zero or
    /// exceeds 8192.
    pub fn new(ctx: &GfxContext, width: u32, height: u32) -> Result<Self, TargetError> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(TargetError::InvalidSize(width, height));
        }

        let format = wgpu::TextureFormat::Rgba8Unorm;

        let texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("HeadlessTarget"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Self {
            texture,
            view,
            width,
            height,
            format,
        })
    }

    /// Borrow the [`wgpu::TextureView`] for use in render passes.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Borrow the underlying [`wgpu::Texture`] (needed for `copy_texture_to_buffer`).
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Return the `(width, height)` of the texture in texels.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Return the texture format (`Rgba8Unorm`).
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a real [`GfxContext`] or skips the test if no GPU is available.
    macro_rules! ctx_or_skip {
        () => {{
            match crate::context::GfxContext::new_headless() {
                Ok(c) => c,
                Err(crate::context::GfxContextError::NoAdapter) => {
                    eprintln!("SKIP: no GPU adapter — skipping test");
                    return;
                }
                Err(e) => panic!("unexpected GfxContext error: {e}"),
            }
        }};
    }

    #[test]
    fn zero_size_returns_invalid_size_error() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let err = HeadlessTarget::new(&ctx, 0, 0).unwrap_err();
        assert!(matches!(err, TargetError::InvalidSize(0, 0)));
    }

    #[test]
    fn zero_width_returns_invalid_size_error() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let err = HeadlessTarget::new(&ctx, 0, 64).unwrap_err();
        assert!(matches!(err, TargetError::InvalidSize(0, 64)));
    }

    #[test]
    fn zero_height_returns_invalid_size_error() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let err = HeadlessTarget::new(&ctx, 64, 0).unwrap_err();
        assert!(matches!(err, TargetError::InvalidSize(64, 0)));
    }

    #[test]
    fn oversized_returns_invalid_size_error() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let err = HeadlessTarget::new(&ctx, 9000, 9000).unwrap_err();
        assert!(matches!(err, TargetError::InvalidSize(9000, 9000)));
    }

    #[test]
    fn dimensions_round_trip() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let target = HeadlessTarget::new(&ctx, 128, 64).expect("target");
        assert_eq!(target.dimensions(), (128, 64));
    }

    #[test]
    fn format_is_rgba8_unorm() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let target = HeadlessTarget::new(&ctx, 64, 64).expect("target");
        assert_eq!(target.format(), wgpu::TextureFormat::Rgba8Unorm);
    }
}
