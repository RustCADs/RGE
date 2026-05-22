//! Transform uniform buffer — a single `mat4x4<f32>` bound at `@group(0) @binding(0)`.
//!
//! WGSL uses **column-major** `mat4x4<f32>`.  [`glam::Mat4`] also stores its
//! data column-major, so `mat.to_cols_array()` produces the correct byte layout
//! for upload with no transposition needed.
//!
//! # Example
//!
//! ```ignore
//! let transform = Transform::new(&ctx).unwrap();
//! transform.update(&ctx, glam::Mat4::IDENTITY);
//! // in a render pass:
//! pass.set_bind_group(0, transform.bind_group(), &[]);
//! ```

use bytemuck::cast_slice;

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the uniform buffer in bytes: 4×4 f32 = 64 bytes.
const MATRIX_BYTE_SIZE: u64 = 64;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`Transform`].
///
/// Currently no failure modes exist at runtime; this enum is kept for API
/// symmetry and future-proofing.
#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    /// Placeholder — should never be emitted by current code.
    #[error("unreachable")]
    Unreachable,
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

/// A `mat4x4<f32>` uniform buffer with its bind group and bind group layout.
///
/// The matrix is column-major (matching WGSL `mat4x4<f32>` and [`glam::Mat4`]
/// internal layout).  Pass `mat.to_cols_array()` to [`update`](Self::update)
/// for correct semantics.
pub struct Transform {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Transform {
    /// Create a [`Transform`] initialised to the identity matrix.
    ///
    /// # Errors
    ///
    /// Returns [`TransformError::Unreachable`] in theory; currently infallible.
    /// The `Result` wrapper is kept for API symmetry and future-proofing.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(ctx: &GfxContext) -> Result<Self, TransformError> {
        let device = ctx.device();

        // Allocate the 64-byte uniform buffer, pre-filled with the identity matrix.
        let identity = glam::Mat4::IDENTITY.to_cols_array();
        let bytes: &[u8] = cast_slice(&identity);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transform.buffer"),
            size: MATRIX_BYTE_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Upload the identity matrix via the queue (avoids mapped_at_creation).
        ctx.queue().write_buffer(&buffer, 0, bytes);

        // Bind group layout: one uniform buffer at binding 0, visible to vertex.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Transform.bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(MATRIX_BYTE_SIZE),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Transform.bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            buffer,
            bind_group,
            bind_group_layout,
        })
    }

    /// Upload a new matrix to the GPU.
    ///
    /// The matrix is written in column-major order (i.e. `mat.to_cols_array()`),
    /// which matches the WGSL `mat4x4<f32>` layout.  The bind group remains valid
    /// after this call.
    pub fn update(&self, ctx: &GfxContext, matrix: glam::Mat4) {
        let cols = matrix.to_cols_array();
        let bytes: &[u8] = cast_slice(&cols);
        ctx.queue().write_buffer(&self.buffer, 0, bytes);
    }

    /// Return the bind group layout (needed to create a [`MeshPipeline`]).
    ///
    /// [`MeshPipeline`]: crate::mesh_pipeline::MeshPipeline
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Return the bind group to bind at slot 0 during a render pass.
    #[must_use]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! ctx_or_skip {
        () => {{
            match crate::context::GfxContext::new_headless() {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("SKIP: no GPU adapter");
                    return;
                }
            }
        }};
    }

    #[test]
    fn new_succeeds_and_returns_valid_bind_group() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let transform = Transform::new(&ctx).expect("transform");
        // Just checking that bind_group() returns a reference without panicking.
        let _bg = transform.bind_group();
    }

    #[test]
    fn update_does_not_invalidate_bind_group() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let transform = Transform::new(&ctx).expect("transform");
        transform.update(&ctx, glam::Mat4::IDENTITY);
        let _bg = transform.bind_group();
        // A second update also works.
        transform.update(&ctx, glam::Mat4::from_scale(glam::Vec3::splat(0.5)));
        let _bg2 = transform.bind_group();
    }
}
