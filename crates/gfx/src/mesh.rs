//! [`Mesh`]: vertex buffer + optional index buffer in one convenient type.
//!
//! For non-indexed draws use [`Mesh::from_vertices`].
//! For indexed draws use [`Mesh::from_indexed`].

use crate::buffer::{BufferError, IndexBuffer, VertexBuffer};
use crate::context::GfxContext;
use crate::vertex::Vertex;

// ---------------------------------------------------------------------------
// Mesh
// ---------------------------------------------------------------------------

/// A renderable mesh: owns one [`VertexBuffer`] and an optional [`IndexBuffer`].
pub struct Mesh {
    vertex_buffer: VertexBuffer,
    index_buffer: Option<IndexBuffer>,
}

impl Mesh {
    /// Create a non-indexed mesh from a slice of vertices.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `vertices` is empty.
    pub fn from_vertices(ctx: &GfxContext, vertices: &[Vertex]) -> Result<Self, BufferError> {
        let vertex_buffer = VertexBuffer::new(ctx, vertices)?;
        Ok(Self {
            vertex_buffer,
            index_buffer: None,
        })
    }

    /// Create an indexed mesh from a vertex slice and an index slice.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if either slice is empty.
    pub fn from_indexed(
        ctx: &GfxContext,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Result<Self, BufferError> {
        let vertex_buffer = VertexBuffer::new(ctx, vertices)?;
        let index_buffer = IndexBuffer::new(ctx, indices)?;
        Ok(Self {
            vertex_buffer,
            index_buffer: Some(index_buffer),
        })
    }

    /// Borrow the mesh's [`VertexBuffer`].
    #[must_use]
    pub fn vertex_buffer(&self) -> &VertexBuffer {
        &self.vertex_buffer
    }

    /// Borrow the mesh's [`IndexBuffer`], if any.
    #[must_use]
    pub fn index_buffer(&self) -> Option<&IndexBuffer> {
        self.index_buffer.as_ref()
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

    fn sample_verts() -> [Vertex; 3] {
        [
            Vertex::new([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            Vertex::new([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Vertex::new([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ]
    }

    #[test]
    fn from_vertices_has_no_index_buffer() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let mesh = Mesh::from_vertices(&ctx, &sample_verts()).expect("mesh");
        assert!(mesh.index_buffer().is_none());
        assert_eq!(mesh.vertex_buffer().vertex_count(), 3);
    }

    #[test]
    fn from_indexed_has_both_buffers() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        let indices = [0u32, 1, 2];
        let mesh = Mesh::from_indexed(&ctx, &sample_verts(), &indices).expect("mesh");
        assert!(mesh.index_buffer().is_some());
        let ib = mesh.index_buffer().unwrap();
        assert_eq!(ib.index_count(), 3);
    }

    #[test]
    fn from_vertices_empty_returns_error() {
        let _gpu_lock = crate::test_lock::guard();
        let ctx = ctx_or_skip!();
        assert!(Mesh::from_vertices(&ctx, &[]).is_err());
    }
}
