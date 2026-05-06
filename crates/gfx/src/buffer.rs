//! Typed GPU buffer wrappers: [`VertexBuffer`] and [`IndexBuffer`].
//!
//! Both are thin wrappers around [`wgpu::Buffer`] with the appropriate usage
//! flags. Data is uploaded immediately on construction via
//! [`wgpu::Queue::write_buffer`] so callers need not handle staging manually.

use bytemuck::cast_slice;
use wgpu::util::DeviceExt as _;

use crate::context::GfxContext;
use crate::vertex::Vertex;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`VertexBuffer`] or [`IndexBuffer`].
#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    /// The supplied slice was empty; buffers must contain at least one element.
    #[error("empty data — must contain at least one element")]
    Empty,
}

// ---------------------------------------------------------------------------
// VertexBuffer
// ---------------------------------------------------------------------------

/// A GPU vertex buffer holding [`Vertex`] data.
///
/// Usage flags: `VERTEX | COPY_DST`.
pub struct VertexBuffer {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl VertexBuffer {
    /// Allocate a vertex buffer and upload `vertices` to the GPU.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `vertices` is empty.
    pub fn new(ctx: &GfxContext, vertices: &[Vertex]) -> Result<Self, BufferError> {
        if vertices.is_empty() {
            return Err(BufferError::Empty);
        }

        let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        let bytes: &[u8] = cast_slice(vertices);

        let buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("VertexBuffer"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        Ok(Self {
            buffer,
            vertex_count,
        })
    }

    /// Borrow the underlying [`wgpu::Buffer`].
    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Number of vertices in this buffer.
    #[must_use]
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

// ---------------------------------------------------------------------------
// IndexBuffer
// ---------------------------------------------------------------------------

/// A GPU index buffer holding `u32` indices.
///
/// Usage flags: `INDEX | COPY_DST`.
/// Index format is always [`wgpu::IndexFormat::Uint32`].
pub struct IndexBuffer {
    buffer: wgpu::Buffer,
    index_count: u32,
}

impl IndexBuffer {
    /// Allocate an index buffer and upload `indices` to the GPU.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `indices` is empty.
    pub fn new(ctx: &GfxContext, indices: &[u32]) -> Result<Self, BufferError> {
        if indices.is_empty() {
            return Err(BufferError::Empty);
        }

        let index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        let bytes: &[u8] = cast_slice(indices);

        let buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("IndexBuffer"),
                contents: bytes,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            });

        Ok(Self {
            buffer,
            index_count,
        })
    }

    /// Borrow the underlying [`wgpu::Buffer`].
    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Number of indices in this buffer.
    #[must_use]
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Index format used by this buffer (always [`wgpu::IndexFormat::Uint32`]).
    #[must_use]
    #[allow(clippy::unused_self)]
    pub const fn index_format(&self) -> wgpu::IndexFormat {
        wgpu::IndexFormat::Uint32
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
                Err(_) => {
                    eprintln!("SKIP: no GPU adapter");
                    return;
                }
            }
        }};
    }

    #[test]
    fn vertex_buffer_empty_returns_error() {
        let ctx = ctx_or_skip!();
        let result = VertexBuffer::new(&ctx, &[]);
        assert!(matches!(result, Err(BufferError::Empty)));
    }

    #[test]
    fn index_buffer_empty_returns_error() {
        let ctx = ctx_or_skip!();
        let result = IndexBuffer::new(&ctx, &[]);
        assert!(matches!(result, Err(BufferError::Empty)));
    }

    #[test]
    fn index_format_is_uint32() {
        let ctx = ctx_or_skip!();
        let ib = IndexBuffer::new(&ctx, &[0u32, 1, 2]).expect("index buffer");
        assert_eq!(ib.index_format(), wgpu::IndexFormat::Uint32);
    }

    #[test]
    fn vertex_buffer_count_matches_input() {
        let ctx = ctx_or_skip!();
        let verts = [
            Vertex::new([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            Vertex::new([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Vertex::new([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ];
        let vb = VertexBuffer::new(&ctx, &verts).expect("vertex buffer");
        assert_eq!(vb.vertex_count(), 3);
    }
}
