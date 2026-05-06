//! Lit vertex format: position (vec3) + normal (vec3) + uv (vec2).
//!
//! [`VertexLit`] is `#[repr(C)]`, `Pod`, and `Zeroable` so it can be cast
//! directly to a byte slice by bytemuck and uploaded to a [`wgpu::Buffer`].
//!
//! Memory layout (32 bytes total, `#[repr(C)]`):
//!
//! | offset | field      | type       | location |
//! |--------|------------|------------|----------|
//! | 0      | `position` | `[f32; 3]` | 0        |
//! | 12     | `normal`   | `[f32; 3]` | 1        |
//! | 24     | `uv`       | `[f32; 2]` | 2        |

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// VertexLit
// ---------------------------------------------------------------------------

/// A lit vertex: 3-component position + 3-component normal + 2-component uv.
///
/// 32 bytes total stride. Used by the [`LitMeshPipeline`] for Lambert+Phong
/// shading with a base-colour texture sample.
///
/// [`LitMeshPipeline`]: crate::lit_mesh_pipeline::LitMeshPipeline
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct VertexLit {
    /// Position in local/object space as `(x, y, z)`.
    pub position: [f32; 3],
    /// Surface normal (should be unit length for correct lighting).
    pub normal: [f32; 3],
    /// Texture coordinate `(u, v)` in `[0, 1]` (with [`wgpu::AddressMode::Repeat`]
    /// applied at sample time, values outside this range wrap).
    pub uv: [f32; 2],
}

/// Stride of a single [`VertexLit`] in bytes (32).
const VERTEX_LIT_SIZE: u64 = std::mem::size_of::<VertexLit>() as u64;

impl VertexLit {
    /// Construct a lit vertex from explicit position / normal / uv arrays.
    #[must_use]
    pub const fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
        }
    }

    /// Return the [`wgpu::VertexBufferLayout`] that describes this format.
    ///
    /// - `@location(0)` → `position` (`Float32x3`)
    /// - `@location(1)` → `normal`   (`Float32x3`)
    /// - `@location(2)` → `uv`       (`Float32x2`)
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        use wgpu::{VertexAttribute, VertexStepMode};

        static ATTRS: &[VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3,   // position
            1 => Float32x3,   // normal
            2 => Float32x2,   // uv
        ];

        wgpu::VertexBufferLayout {
            array_stride: VERTEX_LIT_SIZE,
            step_mode: VertexStepMode::Vertex,
            attributes: ATTRS,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_32_bytes() {
        assert_eq!(std::mem::size_of::<VertexLit>(), 32);
    }

    #[test]
    fn layout_stride_equals_vertex_lit_size() {
        let layout = VertexLit::layout();
        assert_eq!(layout.array_stride, VERTEX_LIT_SIZE);
    }

    #[test]
    fn zeroable_and_pod_smoke() {
        // Zeroable: bit-zero is a valid value.
        let v: VertexLit = bytemuck::Zeroable::zeroed();
        assert!(v.position.iter().all(|&f| f.to_bits() == 0));
        assert!(v.normal.iter().all(|&f| f.to_bits() == 0));
        assert!(v.uv.iter().all(|&f| f.to_bits() == 0));

        // Pod: cast to a byte slice round-trips through bytemuck.
        let v2 = VertexLit::new([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.5, 0.25]);
        let bytes: &[u8] = bytemuck::bytes_of(&v2);
        assert_eq!(bytes.len(), 32);
        let back: VertexLit = *bytemuck::from_bytes(bytes);
        assert_eq!(back.position[0].to_bits(), 1.0_f32.to_bits());
        assert_eq!(back.normal[1].to_bits(), 1.0_f32.to_bits());
        assert_eq!(back.uv[0].to_bits(), 0.5_f32.to_bits());
    }
}
