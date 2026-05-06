//! `Tessellation` — triangle-soup mesh produced by operator evaluation.
//!
//! Failure class: snapshot-recoverable
//!
//! A [`Tessellation`] is a flat-position + index-buffer pair. It carries no
//! topology (no faces / edges / vertices in the B-Rep sense) — that's a
//! later-phase concern (Phase 7.4 topology lineage). For Phase 7.1 D-prime
//! we just need positions plus triangle indices to validate end-to-end
//! evaluation through the operator graph.
//!
//! # Invariants
//!
//! * `indices.len() % 3 == 0` (triangle list)
//! * Every `idx in indices` satisfies `(idx as usize) < positions.len()`
//!
//! Both invariants are enforced by [`Tessellation::new`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors produced when constructing a [`Tessellation`] from raw buffers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TessellationError {
    /// An index value referenced a vertex slot that does not exist.
    #[error("index {index_value} out of bounds (positions has {num_positions} entries)")]
    IndexOutOfBounds {
        /// The offending index value.
        index_value: u32,
        /// Length of the positions buffer at construction time.
        num_positions: usize,
    },
    /// The index buffer length was not a multiple of three.
    #[error("incomplete triangle: {index_count} indices is not a multiple of 3")]
    IncompleteTriangle {
        /// The full index count that triggered the rejection.
        index_count: usize,
    },
}

// ---------------------------------------------------------------------------
// Tessellation
// ---------------------------------------------------------------------------

/// A flat triangle-list mesh: parallel `positions` + `indices` buffers.
///
/// Each consecutive triple of indices is one triangle. Right-handed CCW
/// winding is the convention all operators must follow.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tessellation {
    /// Per-vertex positions in object space. `[x, y, z]` order.
    pub positions: Vec<[f32; 3]>,
    /// Triangle indices, three per triangle, into `positions`.
    pub indices: Vec<u32>,
}

impl Tessellation {
    /// Build a [`Tessellation`] after validating index-buffer invariants.
    ///
    /// # Errors
    ///
    /// * [`TessellationError::IncompleteTriangle`] if `indices.len() % 3 != 0`.
    /// * [`TessellationError::IndexOutOfBounds`] if any index is `>= positions.len()`.
    pub fn new(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Result<Self, TessellationError> {
        if indices.len() % 3 != 0 {
            return Err(TessellationError::IncompleteTriangle {
                index_count: indices.len(),
            });
        }
        let num_positions = positions.len();
        for &idx in &indices {
            if (idx as usize) >= num_positions {
                return Err(TessellationError::IndexOutOfBounds {
                    index_value: idx,
                    num_positions,
                });
            }
        }
        Ok(Self { positions, indices })
    }

    /// Number of vertices (positions) in the tessellation.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles (`indices.len() / 3`).
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_bounds_index_rejected() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0, 1, 5]; // 5 is out of bounds.
        let err = Tessellation::new(positions, indices).unwrap_err();
        assert!(matches!(
            err,
            TessellationError::IndexOutOfBounds {
                index_value: 5,
                num_positions: 3,
            }
        ));
    }

    #[test]
    fn non_multiple_of_three_indices_rejected() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0, 1, 2, 0]; // length 4, not a multiple of 3.
        let err = Tessellation::new(positions, indices).unwrap_err();
        assert_eq!(
            err,
            TessellationError::IncompleteTriangle { index_count: 4 }
        );
    }

    #[test]
    fn valid_box_constructs() {
        let positions = vec![
            [-0.5_f32, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ];
        let indices = vec![
            0, 1, 2, 0, 2, 3, // back face (-z)
            5, 4, 7, 5, 7, 6, // front face (+z)
            4, 0, 3, 4, 3, 7, // left face (-x)
            1, 5, 6, 1, 6, 2, // right face (+x)
            3, 2, 6, 3, 6, 7, // top face (+y)
            4, 5, 1, 4, 1, 0, // bottom face (-y)
        ];
        let mesh = Tessellation::new(positions, indices).expect("valid box");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
    }
}
