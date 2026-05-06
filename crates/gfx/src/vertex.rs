//! Minimal vertex format: position (vec3) + color (vec3).
//!
//! [`Vertex`] is `#[repr(C)]`, `Pod`, and `Zeroable` so it can be cast
//! directly to a byte slice by bytemuck and uploaded to a [`wgpu::Buffer`].

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// Vertex
// ---------------------------------------------------------------------------

/// A minimal vertex: 3-component position followed by 3-component linear color.
///
/// Memory layout (24 bytes total, `#[repr(C)]`):
///
/// | offset | field      | type       |
/// |--------|------------|------------|
/// | 0      | `position` | `[f32; 3]` |
/// | 12     | `color`    | `[f32; 3]` |
///
/// Both fields are at `@location(0)` and `@location(1)` in WGSL respectively.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// Position in local/object space as `(x, y, z)`.
    pub position: [f32; 3],
    /// Linear sRGB color as `(r, g, b)`.  Values in `0.0–1.0`.
    pub color: [f32; 3],
}

/// Stride of a single [`Vertex`] in bytes (24).
const VERTEX_SIZE: u64 = std::mem::size_of::<Vertex>() as u64;

impl Vertex {
    /// Construct a vertex with the given position and color arrays.
    #[must_use]
    pub const fn new(position: [f32; 3], color: [f32; 3]) -> Self {
        Self { position, color }
    }

    /// Return the [`wgpu::VertexBufferLayout`] that describes this format.
    ///
    /// - `@location(0)` → `position` (`Float32x3`)
    /// - `@location(1)` → `color`    (`Float32x3`)
    #[must_use]
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        use wgpu::{VertexAttribute, VertexStepMode};

        static ATTRS: &[VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3,   // position
            1 => Float32x3,   // color
        ];

        wgpu::VertexBufferLayout {
            array_stride: VERTEX_SIZE,
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
    fn new_stores_position_and_color() {
        let v = Vertex::new([1.0, 2.0, 3.0], [0.5, 0.0, 1.0]);
        // Use bit-exact comparison via u32 transmute to satisfy clippy::float_cmp.
        let pos_bits: [u32; 3] = v.position.map(f32::to_bits);
        let col_bits: [u32; 3] = v.color.map(f32::to_bits);
        assert_eq!(
            pos_bits,
            [1.0_f32.to_bits(), 2.0_f32.to_bits(), 3.0_f32.to_bits()]
        );
        assert_eq!(
            col_bits,
            [0.5_f32.to_bits(), 0.0_f32.to_bits(), 1.0_f32.to_bits()]
        );
    }

    #[test]
    fn size_is_24_bytes() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
    }

    #[test]
    fn layout_stride_equals_vertex_size() {
        let layout = Vertex::layout();
        assert_eq!(layout.array_stride, VERTEX_SIZE);
    }

    #[test]
    fn zeroable_is_all_zeros() {
        let v: Vertex = bytemuck::Zeroable::zeroed();
        // All bits must be zero for the zero-value float (0.0 bit-pattern = 0x00000000).
        assert!(v.position.iter().all(|&f| f.to_bits() == 0));
        assert!(v.color.iter().all(|&f| f.to_bits() == 0));
    }
}
