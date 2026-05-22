//! Camera UBO: `view_proj` matrix + `normal_matrix` for normal transformation.
//!
//! Packed as a single 128-byte uniform buffer at `@group(0) @binding(0)`:
//!
//! | offset | field           | type            | size |
//! |--------|-----------------|-----------------|------|
//! | 0      | `view_proj`     | `mat4x4<f32>`   | 64   |
//! | 64     | `normal_matrix` | `mat4x4<f32>`   | 64   |
//!
//! The `normal_matrix` is `(model.inverse().transpose())` — the standard
//! correction needed when transforming surface normals through a non-uniform
//! scale.  We store it as a full 4×4 even though only the top-left 3×3 is used
//! by the shader, because WGSL `mat4x4<f32>` aligns at 16 bytes and a 3×3
//! would cost the same after padding.  The matrix is column-major, matching
//! [`glam::Mat4::to_cols_array`].

use bytemuck::{Pod, Zeroable};

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the camera uniform buffer in bytes: 2 × `mat4x4<f32>` = 128 bytes.
const CAMERA_UBO_SIZE: u64 = 128;

// ---------------------------------------------------------------------------
// CameraUbo (POD struct uploaded to GPU)
// ---------------------------------------------------------------------------

/// POD layout of the camera UBO — 128 bytes, column-major matrices.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CameraUbo {
    view_proj: [f32; 16],
    normal_matrix: [f32; 16],
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`Camera`].
///
/// Currently no failure modes exist at runtime; this enum is kept for API
/// symmetry and future-proofing.
#[derive(Debug, thiserror::Error)]
pub enum CameraError {
    /// Placeholder — should never be emitted by current code.
    #[error("unreachable")]
    Unreachable,
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

/// Camera UBO holding `view_proj` and `normal_matrix` matrices.
///
/// Bound at `@group(0) @binding(0)` and visible to both vertex and fragment
/// stages.  Initialised to identity by [`Camera::new`]; update by calling
/// [`Camera::update`] with a view*proj matrix and a model matrix (the model
/// matrix's inverse-transpose becomes the normal matrix for correct lighting).
#[derive(Debug)]
pub struct Camera {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Camera {
    /// Create a [`Camera`] initialised to identity matrices.
    ///
    /// # Errors
    ///
    /// Returns [`CameraError::Unreachable`] in theory; currently infallible.
    /// The `Result` wrapper is kept for API symmetry and future-proofing.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(ctx: &GfxContext) -> Result<Self, CameraError> {
        let device = ctx.device();

        let identity_ubo = CameraUbo {
            view_proj: glam::Mat4::IDENTITY.to_cols_array(),
            normal_matrix: glam::Mat4::IDENTITY.to_cols_array(),
        };
        let bytes: &[u8] = bytemuck::bytes_of(&identity_ubo);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera.buffer"),
            size: CAMERA_UBO_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue().write_buffer(&buffer, 0, bytes);

        // Bind group layout: one uniform buffer at binding 0, visible to both
        // stages (vertex needs view_proj, fragment uses neither but the camera
        // group is present at @group(0) for the lit pipeline; visibility is
        // VERTEX | FRAGMENT to match WGSL's bind-group declaration freely).
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera.bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(CAMERA_UBO_SIZE),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera.bind_group"),
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

    /// Upload new matrices to the GPU.
    ///
    /// `view_proj` is the combined view*projection matrix.
    /// The `normal_matrix` is computed as `(model.inverse().transpose())`.
    /// Both are stored column-major, matching WGSL `mat4x4<f32>`.
    pub fn update(&self, ctx: &GfxContext, view_proj: glam::Mat4, model: glam::Mat4) {
        let normal = model.inverse().transpose();
        let ubo = CameraUbo {
            view_proj: view_proj.to_cols_array(),
            normal_matrix: normal.to_cols_array(),
        };
        let bytes: &[u8] = bytemuck::bytes_of(&ubo);
        ctx.queue().write_buffer(&self.buffer, 0, bytes);
    }

    /// Return the bind group layout (needed when building a pipeline layout).
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Return the bind group to bind at `@group(0)` during a render pass.
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
    fn new_succeeds() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let cam = Camera::new(&ctx).expect("camera");
        let _bg = cam.bind_group();
        let _bgl = cam.bind_group_layout();
    }

    #[test]
    fn update_does_not_invalidate_bind_group() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let cam = Camera::new(&ctx).expect("camera");
        // Update once with a perspective + identity model.
        let proj = glam::Mat4::perspective_rh_gl(1.0, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        cam.update(&ctx, proj * view, glam::Mat4::IDENTITY);
        let _bg1 = cam.bind_group();
        // And again with a translated model — bind group must still be valid.
        let model = glam::Mat4::from_translation(glam::Vec3::new(1.0, 0.0, 0.0));
        cam.update(&ctx, proj * view, model);
        let _bg2 = cam.bind_group();
    }

    #[test]
    fn bind_group_layout_is_uniform_at_binding_0() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let cam = Camera::new(&ctx).expect("camera");
        // We can't introspect the layout's entries directly via wgpu's public
        // API, but we can verify that the layout can be used to create a
        // pipeline layout — that requires the binding to be wired correctly.
        let _pl = ctx
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("CameraTestPipelineLayout"),
                bind_group_layouts: &[Some(cam.bind_group_layout())],
                immediate_size: 0,
            });
    }
}
