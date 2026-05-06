//! `Tessellation` — triangle-soup mesh produced by operator evaluation.
//!
//! Failure class: snapshot-recoverable
//!
//! A [`Tessellation`] is a flat-position + index-buffer pair, optionally
//! carrying per-triangle face labels ([`TopologyFaceId`]). Labels are an
//! `Option<Vec<TopologyFaceId>>` field — `None` for unlabeled output (the
//! default for primitive operators) and `Some(labels)` for labeled output
//! (typically the result of a Boolean operation that propagated input
//! labels through `csgrs`'s polygon metadata).
//!
//! Until 2026-05 cad-core had two parallel mesh types (`Tessellation` and
//! `LabeledMesh`); the unified design collapses them so both the unlabeled
//! and labeled paths compose through a single `Operator::evaluate` /
//! `OperatorGraph::evaluate` substrate. Consumers detect labeling via
//! [`Tessellation::is_labeled`] / [`Tessellation::face_labels`].
//!
//! # Invariants
//!
//! * `indices.len() % 3 == 0` (triangle list)
//! * Every `idx in indices` satisfies `(idx as usize) < positions.len()`
//! * If `face_labels = Some(labels)`, then `labels.len() == indices.len() / 3`
//!
//! All three invariants are enforced by [`Tessellation::new`] /
//! [`Tessellation::with_labels`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// TopologyFaceId
// ---------------------------------------------------------------------------

/// Per-mesh face identity.
///
/// Sequential within a single labeled [`Tessellation`]; not stable across
/// rebuilds (that's a Phase 7.2 dispatch — needs a B-Rep model first).
///
/// The sentinel value [`TopologyFaceId::DEGENERATE`] (`u64::MAX`) labels
/// triangles that could not be assigned a plane because they were
/// degenerate (zero area) or had non-finite normals. These triangles are
/// excluded from lineage inference.
///
/// Lives in `tessellation::mesh` (not `topo_lineage::types`) so the
/// `Tessellation` substrate can carry labels without a reverse import.
/// Re-exported through `topo_lineage::types` for back-compat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TopologyFaceId(pub u64);

impl TopologyFaceId {
    /// Sentinel face id for triangles whose plane could not be derived
    /// (degenerate / non-finite). The lineage pipeline silently excludes
    /// these from face counts and edge inference.
    pub const DEGENERATE: TopologyFaceId = TopologyFaceId(u64::MAX);

    /// `true` iff this face id is the degenerate sentinel.
    #[must_use]
    pub fn is_degenerate(self) -> bool {
        self.0 == u64::MAX
    }
}

impl std::fmt::Display for TopologyFaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_degenerate() {
            write!(f, "face:degenerate")
        } else {
            write!(f, "face:{}", self.0)
        }
    }
}

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
    /// `face_labels.len()` did not equal `indices.len() / 3` (one label per
    /// triangle).
    #[error("face_labels length ({got}) must equal triangle count ({expected})")]
    LabelLengthMismatch {
        /// Length actually supplied.
        got: usize,
        /// Length the caller's `indices` buffer implied.
        expected: usize,
    },
}

// ---------------------------------------------------------------------------
// Tessellation
// ---------------------------------------------------------------------------

/// A flat triangle-list mesh: parallel `positions` + `indices` buffers, plus
/// optional per-triangle face labels.
///
/// Each consecutive triple of indices is one triangle. Right-handed CCW
/// winding is the convention all operators must follow.
///
/// `face_labels` is `None` when the mesh is unlabeled (the default for
/// primitive operators) and `Some(labels)` when each triangle carries an
/// originating-face id (typically from a Boolean op that propagated input
/// labels through `csgrs` polygon metadata, or from
/// [`crate::label_by_plane`] grouping triangles by plane equation).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tessellation {
    /// Per-vertex positions in object space. `[x, y, z]` order.
    pub positions: Vec<[f32; 3]>,
    /// Triangle indices, three per triangle, into `positions`.
    pub indices: Vec<u32>,
    /// Per-triangle face labels. `None` = unlabeled (default for primitive
    /// operators). `Some(labels)` = labeled, with `labels.len() ==
    /// indices.len() / 3`. Labels propagate through Boolean operations;
    /// lineage inference is high-confidence when labels are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_labels: Option<Vec<TopologyFaceId>>,
}

impl Tessellation {
    /// Build an unlabeled [`Tessellation`] after validating index-buffer
    /// invariants. `face_labels` defaults to `None`.
    ///
    /// # Errors
    ///
    /// * [`TessellationError::IncompleteTriangle`] if `indices.len() % 3 != 0`.
    /// * [`TessellationError::IndexOutOfBounds`] if any index is `>= positions.len()`.
    pub fn new(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Result<Self, TessellationError> {
        Self::validate_buffers(&positions, &indices)?;
        Ok(Self {
            positions,
            indices,
            face_labels: None,
        })
    }

    /// Build a labeled [`Tessellation`] after validating index-buffer
    /// invariants and the label-length match.
    ///
    /// # Errors
    ///
    /// * [`TessellationError::IncompleteTriangle`] if `indices.len() % 3 != 0`.
    /// * [`TessellationError::IndexOutOfBounds`] if any index is `>= positions.len()`.
    /// * [`TessellationError::LabelLengthMismatch`] if `face_labels.len() !=
    ///   indices.len() / 3`.
    pub fn with_labels(
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        face_labels: Vec<TopologyFaceId>,
    ) -> Result<Self, TessellationError> {
        Self::validate_buffers(&positions, &indices)?;
        let triangle_count = indices.len() / 3;
        if face_labels.len() != triangle_count {
            return Err(TessellationError::LabelLengthMismatch {
                got: face_labels.len(),
                expected: triangle_count,
            });
        }
        Ok(Self {
            positions,
            indices,
            face_labels: Some(face_labels),
        })
    }

    /// Validate the index-buffer invariants shared by [`Self::new`] and
    /// [`Self::with_labels`].
    fn validate_buffers(positions: &[[f32; 3]], indices: &[u32]) -> Result<(), TessellationError> {
        if indices.len() % 3 != 0 {
            return Err(TessellationError::IncompleteTriangle {
                index_count: indices.len(),
            });
        }
        let num_positions = positions.len();
        for &idx in indices {
            if (idx as usize) >= num_positions {
                return Err(TessellationError::IndexOutOfBounds {
                    index_value: idx,
                    num_positions,
                });
            }
        }
        Ok(())
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

    /// `true` iff this tessellation carries per-triangle face labels.
    #[must_use]
    pub fn is_labeled(&self) -> bool {
        self.face_labels.is_some()
    }

    /// Borrow the per-triangle face labels if present.
    #[must_use]
    pub fn face_labels(&self) -> Option<&[TopologyFaceId]> {
        self.face_labels.as_deref()
    }

    /// Number of distinct face ids (excluding the degenerate sentinel) when
    /// the tessellation is labeled; `None` when unlabeled.
    #[must_use]
    pub fn face_count(&self) -> Option<usize> {
        self.face_labels.as_ref().map(|labels| {
            let mut seen = std::collections::BTreeSet::new();
            for id in labels {
                if !id.is_degenerate() {
                    seen.insert(*id);
                }
            }
            seen.len()
        })
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

    // --- TopologyFaceId / serde ----------------------------------------------

    #[test]
    fn topology_face_id_round_trips_through_serde() {
        let id = TopologyFaceId(42);
        let s = ron::to_string(&id).expect("serialize");
        let back: TopologyFaceId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn topology_face_id_degenerate_sentinel() {
        let id = TopologyFaceId::DEGENERATE;
        assert!(id.is_degenerate());
        assert_eq!(id.0, u64::MAX);
        let normal = TopologyFaceId(7);
        assert!(!normal.is_degenerate());
    }

    // --- Tessellation::new -- unlabeled -------------------------------------

    #[test]
    fn tessellation_new_returns_unlabeled() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0, 1, 2];
        let tess = Tessellation::new(positions, indices).expect("valid");
        assert!(!tess.is_labeled());
        assert!(tess.face_labels().is_none());
        assert_eq!(tess.face_count(), None);
    }

    // --- Tessellation::with_labels -- labeled -------------------------------

    #[test]
    fn tessellation_with_labels_validates_label_count_matches_triangle_count() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0_u32, 1, 2];
        // Wrong: 0 labels for 1 triangle.
        let err =
            Tessellation::with_labels(positions.clone(), indices.clone(), vec![]).unwrap_err();
        assert!(matches!(
            err,
            TessellationError::LabelLengthMismatch {
                got: 0,
                expected: 1
            }
        ));
        // Wrong: 2 labels for 1 triangle.
        let err = Tessellation::with_labels(
            positions.clone(),
            indices.clone(),
            vec![TopologyFaceId(0), TopologyFaceId(1)],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TessellationError::LabelLengthMismatch {
                got: 2,
                expected: 1
            }
        ));
        // Correct: 1 label for 1 triangle.
        let ok =
            Tessellation::with_labels(positions, indices, vec![TopologyFaceId(0)]).expect("valid");
        assert_eq!(ok.triangle_count(), 1);
        assert!(ok.is_labeled());
    }

    #[test]
    fn tessellation_with_labels_validates_indices_multiple_of_3() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // 4 indices is not a multiple of 3.
        let err =
            Tessellation::with_labels(positions, vec![0_u32, 1, 2, 0], vec![TopologyFaceId(0)])
                .unwrap_err();
        assert!(matches!(
            err,
            TessellationError::IncompleteTriangle { index_count: 4 }
        ));
    }

    #[test]
    fn tessellation_with_labels_validates_index_bounds() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Index 5 is out of bounds (positions.len() = 3).
        let err = Tessellation::with_labels(positions, vec![0_u32, 1, 5], vec![TopologyFaceId(0)])
            .unwrap_err();
        assert!(matches!(
            err,
            TessellationError::IndexOutOfBounds {
                index_value: 5,
                num_positions: 3,
            }
        ));
    }

    // --- is_labeled / face_labels / face_count -------------------------------

    #[test]
    fn tessellation_is_labeled_returns_true_when_labeled_false_otherwise() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0_u32, 1, 2];

        let unlabeled = Tessellation::new(positions.clone(), indices.clone()).expect("unlabeled");
        assert!(!unlabeled.is_labeled());

        let labeled = Tessellation::with_labels(positions, indices, vec![TopologyFaceId(7)])
            .expect("labeled");
        assert!(labeled.is_labeled());
    }

    #[test]
    fn tessellation_face_count_returns_some_when_labeled_none_otherwise() {
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        // Unlabeled: face_count() == None.
        let unlabeled =
            Tessellation::new(positions.clone(), vec![0_u32, 1, 2, 1, 3, 2]).expect("unlabeled");
        assert_eq!(unlabeled.face_count(), None);
        // 2 triangles with distinct labels → face_count = Some(2).
        let m = Tessellation::with_labels(
            positions.clone(),
            vec![0_u32, 1, 2, 1, 3, 2],
            vec![TopologyFaceId(0), TopologyFaceId(1)],
        )
        .expect("valid");
        assert_eq!(m.face_count(), Some(2));
        // 2 triangles with the same label → face_count = Some(1).
        let m = Tessellation::with_labels(
            positions,
            vec![0_u32, 1, 2, 1, 3, 2],
            vec![TopologyFaceId(7), TopologyFaceId(7)],
        )
        .expect("valid");
        assert_eq!(m.face_count(), Some(1));
    }

    #[test]
    fn tessellation_face_count_excludes_degenerate_sentinel() {
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let m = Tessellation::with_labels(
            positions,
            vec![0_u32, 1, 2, 1, 3, 2],
            vec![TopologyFaceId(0), TopologyFaceId::DEGENERATE],
        )
        .expect("valid");
        // Only the non-degenerate label counts → face_count = Some(1).
        assert_eq!(m.face_count(), Some(1));
    }

    #[test]
    fn tessellation_face_labels_returns_slice_when_labeled() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0_u32, 1, 2];
        let labels = vec![TopologyFaceId(42)];
        let m = Tessellation::with_labels(positions, indices, labels).expect("valid");
        let slice = m.face_labels().expect("labeled");
        assert_eq!(slice.len(), 1);
        assert_eq!(slice[0], TopologyFaceId(42));
    }
}
