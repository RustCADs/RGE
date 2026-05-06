//! Single-frame recorder and CPU readback buffer.
//!
//! [`FrameRecorder`] builds a [`wgpu::CommandEncoder`], records render passes,
//! then submits to the GPU queue. [`ReadbackBuffer`] copies the texture to a
//! CPU-visible staging buffer and maps it synchronously.

use crate::context::GfxContext;
use crate::pipeline::TrianglePipeline;
use crate::target::HeadlessTarget;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from frame recording or texture readback.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// Buffer mapping / readback failed.
    #[error("readback failed: {0}")]
    Readback(String),

    /// Clear colour components were not in the valid 0.0–1.0 range.
    #[error("invalid clear color: components must be in 0.0–1.0")]
    InvalidClearColor,
}

// ---------------------------------------------------------------------------
// FrameRecorder
// ---------------------------------------------------------------------------

/// Records GPU commands for one frame and submits them.
///
/// Call [`render_triangle`](FrameRecorder::render_triangle) to add a render
/// pass, then [`submit`](FrameRecorder::submit) to flush to the queue.
///
/// The recorder is consumed by `submit` to prevent double-submission.
pub struct FrameRecorder<'ctx> {
    ctx: &'ctx GfxContext,
    encoder: wgpu::CommandEncoder,
}

impl<'ctx> FrameRecorder<'ctx> {
    /// Create a new frame recorder backed by `ctx`.
    #[must_use]
    pub fn new(ctx: &'ctx GfxContext) -> Self {
        let encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("FrameRecorder"),
            });
        Self { ctx, encoder }
    }

    /// Record a render pass that clears `target` to `clear` then draws the
    /// triangle via `pipeline`.
    pub fn render_triangle(
        &mut self,
        target: &HeadlessTarget,
        pipeline: &TrianglePipeline,
        clear: wgpu::Color,
    ) {
        // wgpu 29: RenderPassColorAttachment gained a `depth_slice` field (None
        // for non-3D textures). RenderPassDescriptor gained `multiview_mask`.
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("TrianglePass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(pipeline.pipeline());
        pass.draw(0..3, 0..1);
    }

    /// Submit all recorded commands to the GPU queue.
    ///
    /// Consumes `self` to prevent double-submission.
    pub fn submit(self) {
        self.ctx
            .queue()
            .submit(std::iter::once(self.encoder.finish()));
    }
}

// ---------------------------------------------------------------------------
// ReadbackBuffer
// ---------------------------------------------------------------------------

/// CPU-side RGBA8 pixel buffer read back from a [`HeadlessTarget`].
///
/// Pixels are tightly packed: `width * height * 4` bytes, row-major, no padding.
/// The GPU staging buffer's row alignment padding is stripped during the copy.
pub struct ReadbackBuffer {
    /// Tightly-packed RGBA8 pixels: `width * height * 4` bytes.
    pub pixels: Vec<u8>,
    /// Width of the source texture in texels.
    pub width: u32,
    /// Height of the source texture in texels.
    pub height: u32,
}

impl ReadbackBuffer {
    /// Read the texture back to CPU memory synchronously.
    ///
    /// Allocates a staging [`wgpu::Buffer`], copies the texture into it, then
    /// maps and reads the data, stripping the row-alignment padding that wgpu
    /// requires internally (`COPY_BYTES_PER_ROW_ALIGNMENT` = 256 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Readback`] if the buffer map fails.
    pub fn from_target(ctx: &GfxContext, target: &HeadlessTarget) -> Result<Self, FrameError> {
        let (width, height) = target.dimensions();
        let bytes_per_pixel: u32 = 4; // Rgba8Unorm

        // Compute the padded row pitch required by wgpu.
        let unpadded_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row = (unpadded_row + align - 1) & !(align - 1);

        let staging_size: u64 = u64::from(padded_row) * u64::from(height);

        let staging_buf = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("ReadbackStaging"),
            size: staging_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Copy texture → staging buffer.
        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ReadbackEncoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        ctx.queue().submit(std::iter::once(encoder.finish()));

        // Map the buffer and read the data.
        let slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });

        // Poll the device until the map completes.
        // wgpu 29: Maintain removed; use PollType::wait_indefinitely().
        let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());

        rx.recv()
            .map_err(|e| FrameError::Readback(e.to_string()))?
            .map_err(|e| FrameError::Readback(e.to_string()))?;

        // Strip row padding and collect into a tight Vec<u8>.
        let mapped = slice.get_mapped_range();
        let capacity =
            usize::try_from(u64::from(unpadded_row) * u64::from(height)).unwrap_or(usize::MAX);
        let mut pixels = Vec::with_capacity(capacity);
        for row in 0..height {
            let start = (row * padded_row) as usize;
            let end = start + unpadded_row as usize;
            pixels.extend_from_slice(&mapped[start..end]);
        }
        drop(mapped);
        staging_buf.unmap();

        Ok(Self {
            pixels,
            width,
            height,
        })
    }

    /// Sample a pixel at `(x, y)` as `(r, g, b, a)` bytes.
    ///
    /// Returns `None` when `x ≥ width` or `y ≥ height`.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        Some((
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_out_of_bounds_returns_none() {
        let buf = ReadbackBuffer {
            pixels: vec![255u8; 4],
            width: 1,
            height: 1,
        };
        assert!(buf.pixel(1, 0).is_none());
        assert!(buf.pixel(0, 1).is_none());
        assert!(buf.pixel(100, 100).is_none());
    }

    #[test]
    fn pixel_in_bounds_returns_components() {
        // 2x1 image: pixel (0,0) = red, pixel (1,0) = blue
        let pixels = vec![255, 0, 0, 255, 0, 0, 255, 255];
        let buf = ReadbackBuffer {
            pixels,
            width: 2,
            height: 1,
        };
        assert_eq!(buf.pixel(0, 0), Some((255, 0, 0, 255)));
        assert_eq!(buf.pixel(1, 0), Some((0, 0, 255, 255)));
        assert!(buf.pixel(2, 0).is_none());
    }
}
