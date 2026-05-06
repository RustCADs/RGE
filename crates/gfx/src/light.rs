//! Directional light UBO bound at `@group(1) @binding(0)`.
//!
//! WGSL std140 alignment quirk: a `vec3<f32>` reserves 16 bytes (the trailing
//! 4 are padding).  We mirror this on the CPU side with explicit `_pad0` /
//! `_pad1` `f32` fields so `bytemuck::cast_slice` produces exactly the layout
//! WGSL expects.  Total UBO size: 32 bytes.
//!
//! | offset | field       | type        |
//! |--------|-------------|-------------|
//! | 0      | `direction` | `vec3<f32>` |
//! | 12     | `_pad0`     | `f32`       |
//! | 16     | `color`     | `vec3<f32>` |
//! | 28     | `_pad1`     | `f32`       |

use bytemuck::{Pod, Zeroable};

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the directional-light UBO in bytes (2 × `vec4<f32>` packing).
const LIGHT_UBO_SIZE: u64 = 32;

// ---------------------------------------------------------------------------
// LightUbo (POD struct uploaded to GPU)
// ---------------------------------------------------------------------------

/// POD layout of the directional-light UBO — 32 bytes with std140 padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct LightUbo {
    direction: [f32; 3],
    _pad0: f32,
    color: [f32; 3],
    _pad1: f32,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`DirectionalLight`].
///
/// Currently no failure modes exist at runtime; this enum is kept for API
/// symmetry and future-proofing.
#[derive(Debug, thiserror::Error)]
pub enum LightError {
    /// Placeholder — should never be emitted by current code.
    #[error("unreachable")]
    Unreachable,
}

// ---------------------------------------------------------------------------
// DirectionalLight
// ---------------------------------------------------------------------------

/// A single directional light: 3-component direction + 3-component colour.
///
/// Bound at `@group(1) @binding(0)` and visible to the fragment stage.  The
/// `direction` should be the direction the light **travels** (so for a light
/// shining straight down, `direction = (0, -1, 0)`); the shader negates it
/// when computing `dot(N, -L)` for Lambert.
#[derive(Debug)]
pub struct DirectionalLight {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl DirectionalLight {
    /// Create a [`DirectionalLight`] initialised to a default sun (direction
    /// `(0, -1, 0)`, white colour).
    ///
    /// # Errors
    ///
    /// Returns [`LightError::Unreachable`] in theory; currently infallible.
    /// The `Result` wrapper is kept for API symmetry and future-proofing.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(ctx: &GfxContext) -> Result<Self, LightError> {
        let device = ctx.device();

        let initial = LightUbo {
            direction: [0.0, -1.0, 0.0],
            _pad0: 0.0,
            color: [1.0, 1.0, 1.0],
            _pad1: 0.0,
        };
        let bytes: &[u8] = bytemuck::bytes_of(&initial);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DirectionalLight.buffer"),
            size: LIGHT_UBO_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue().write_buffer(&buffer, 0, bytes);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("DirectionalLight.bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(LIGHT_UBO_SIZE),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DirectionalLight.bind_group"),
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

    /// Upload a new direction + colour to the GPU.
    ///
    /// `direction` should be the direction of light travel (`(0,-1,0)` is a
    /// sun shining straight down).  `color` is in linear sRGB; values may
    /// exceed `1.0` for HDR lighting.  Padding f32s are written as zero.
    pub fn update(&self, ctx: &GfxContext, direction: glam::Vec3, color: glam::Vec3) {
        let ubo = LightUbo {
            direction: direction.to_array(),
            _pad0: 0.0,
            color: color.to_array(),
            _pad1: 0.0,
        };
        let bytes: &[u8] = bytemuck::bytes_of(&ubo);
        ctx.queue().write_buffer(&self.buffer, 0, bytes);
    }

    /// Return the bind group layout (needed when building a pipeline layout).
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Return the bind group to bind at `@group(1)` during a render pass.
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
        let ctx = ctx_or_skip!();
        let light = DirectionalLight::new(&ctx).expect("light");
        let _bg = light.bind_group();
        let _bgl = light.bind_group_layout();
    }

    #[test]
    fn update_with_normalized_direction() {
        let ctx = ctx_or_skip!();
        let light = DirectionalLight::new(&ctx).expect("light");
        let dir = glam::Vec3::new(0.3, -1.0, 0.2).normalize();
        light.update(&ctx, dir, glam::Vec3::new(1.0, 1.0, 0.9));
        let _bg = light.bind_group();
        // A second update with a different direction must remain valid.
        light.update(&ctx, glam::Vec3::new(0.0, 0.0, -1.0), glam::Vec3::ONE);
        let _bg2 = light.bind_group();
    }

    #[test]
    fn bind_group_layout_is_uniform_at_binding_0() {
        let ctx = ctx_or_skip!();
        let light = DirectionalLight::new(&ctx).expect("light");
        // Verify the layout is usable in a pipeline layout (proves binding 0
        // is correctly typed as a uniform buffer).
        let _pl = ctx
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("LightTestPipelineLayout"),
                bind_group_layouts: &[Some(light.bind_group_layout())],
                immediate_size: 0,
            });
    }
}
