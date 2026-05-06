//! Type definitions for the topology-lineage prototype.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage`]; see that module's `//!` docs for
//! the design rationale + v0 simplifications vs PLAN §1.5.4.3.
//!
//! # Contents
//!
//! * [`LineageError`] — error enum for all topo-lineage operations
//! * [`TopologyFaceId`] — per-mesh face identity (with [`TopologyFaceId::DEGENERATE`] sentinel)
//! * [`TopologyEvolution`] — `Preserved` / `Split` / `Merged` / `Deleted` / `Reinterpreted`
//! * [`LineageEdge`] — one step in a face's history
//! * [`LineageGraph`] — collection of edges with simple iteration helpers
//! * [`LabeledMesh`] — mesh + per-triangle face labels

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// LineageError
// ---------------------------------------------------------------------------

/// Errors produced by topology-lineage operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LineageError {
    /// `face_labels.len()` did not equal `indices.len() / 3` (one label per
    /// triangle).
    #[error("face_labels length ({got}) must equal triangle count ({expected})")]
    LabelLengthMismatch {
        /// Length actually supplied.
        got: usize,
        /// Length the caller's `indices` buffer implied.
        expected: usize,
    },
    /// The supplied positions / indices buffers were malformed in a way
    /// orthogonal to the label-length check.
    #[error("input mesh is invalid: {0}")]
    InvalidInput(String),
    /// A triangle's plane normal was not finite (NaN / ∞ on any component).
    #[error("plane normal is non-finite at triangle {triangle_idx}")]
    NonFiniteNormal {
        /// Triangle index (0-based) that produced the non-finite normal.
        triangle_idx: usize,
    },
    /// A triangle had effectively zero area (cross-product magnitude below
    /// the degeneracy threshold).
    #[error("triangle {triangle_idx} is degenerate (zero area)")]
    DegenerateTriangle {
        /// Triangle index (0-based).
        triangle_idx: usize,
    },
}

// ---------------------------------------------------------------------------
// TopologyFaceId
// ---------------------------------------------------------------------------

/// Per-mesh face identity.
///
/// Sequential within a single [`LabeledMesh`]; not stable across rebuilds
/// (that's a Phase 7.2 dispatch — needs a B-Rep model first).
///
/// The sentinel value [`TopologyFaceId::DEGENERATE`] (`u64::MAX`) labels
/// triangles that could not be assigned a plane because they were
/// degenerate (zero area) or had non-finite normals. These triangles are
/// excluded from lineage inference.
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
// TopologyEvolution
// ---------------------------------------------------------------------------

/// How an input face evolves through an operator.
///
/// Per PLAN §1.5.4.3. The `Split` / `Merged` inner data
/// (`Vec<PersistentFaceId>`) is deferred to a follow-up dispatch — the v0
/// prototype represents these relationships via multiple [`LineageEdge`]
/// entries with the same `from` (Split) or same `to` (Merged) instead of
/// nesting the IDs into the enum payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TopologyEvolution {
    /// Identity unchanged — input face boundary preserved bit-identical.
    Preserved,
    /// One input face appears as multiple disjoint output regions on the
    /// same plane. (v0 detector: input triangle count > output triangle
    /// count on the matching plane.)
    Split,
    /// Multiple input faces collapse to one output face on the same plane.
    /// Recorded via multiple [`LineageEdge`]s sharing a single `to`.
    Merged,
    /// Input face has no output coverage.
    Deleted,
    /// Output face has no matching input plane (newly-introduced face from
    /// e.g. Boolean intersection planes).
    Reinterpreted,
}

// ---------------------------------------------------------------------------
// LineageEdge
// ---------------------------------------------------------------------------

/// One step in a face's history through an operator.
///
/// `from = None` for [`TopologyEvolution::Reinterpreted`] (newly-introduced
/// face). `to = None` for [`TopologyEvolution::Deleted`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineageEdge {
    /// Input face id. `None` for `Reinterpreted`.
    pub from: Option<TopologyFaceId>,
    /// Output face id. `None` for `Deleted`.
    pub to: Option<TopologyFaceId>,
    /// Evolution kind — see [`TopologyEvolution`].
    pub evolution: TopologyEvolution,
    /// Heuristic confidence in `[0.0, 1.0]`. v0 uses `1.0` for exact-plane
    /// matches, `0.5` for fuzzy / heuristic matches, `0.0` for purely
    /// inferred edges.
    pub confidence: f32,
}

// ---------------------------------------------------------------------------
// LineageGraph
// ---------------------------------------------------------------------------

/// Lineage graph: collection of [`LineageEdge`]s relating input face ids to
/// output face ids.
///
/// Stored as a `Vec` for v0; a future version may promote to a richer
/// structure (e.g. `kernel/graph-foundation::Graph`) once usage stabilizes
/// and the access patterns are clearer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LineageGraph {
    /// All recorded edges. Iteration order matches push order.
    pub edges: Vec<LineageEdge>,
}

impl LineageGraph {
    /// Construct an empty lineage graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a single [`LineageEdge`].
    pub fn push(&mut self, edge: LineageEdge) {
        self.edges.push(edge);
    }

    /// Number of recorded edges.
    #[must_use]
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// True iff [`Self::len`] is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Iterate edges where `from == Some(face_id)`.
    pub fn edges_from(&self, face_id: TopologyFaceId) -> impl Iterator<Item = &LineageEdge> + '_ {
        self.edges.iter().filter(move |e| e.from == Some(face_id))
    }

    /// Iterate edges where `to == Some(face_id)`.
    pub fn edges_to(&self, face_id: TopologyFaceId) -> impl Iterator<Item = &LineageEdge> + '_ {
        self.edges.iter().filter(move |e| e.to == Some(face_id))
    }

    /// Iterate edges with the given evolution kind.
    pub fn edges_by_evolution(
        &self,
        ev: TopologyEvolution,
    ) -> impl Iterator<Item = &LineageEdge> + '_ {
        self.edges.iter().filter(move |e| e.evolution == ev)
    }
}

// ---------------------------------------------------------------------------
// LabeledMesh
// ---------------------------------------------------------------------------

/// Mesh with per-triangle face labels.
///
/// Invariant (enforced by [`Self::new`]): `face_labels.len() ==
/// indices.len() / 3` (one label per triangle).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabeledMesh {
    /// Per-vertex positions in object space, `[x, y, z]`.
    pub positions: Vec<[f32; 3]>,
    /// Triangle indices into `positions`, three per triangle.
    pub indices: Vec<u32>,
    /// One [`TopologyFaceId`] per triangle.
    pub face_labels: Vec<TopologyFaceId>,
}

impl LabeledMesh {
    /// Construct a [`LabeledMesh`] after validating index-buffer invariants
    /// and the label-length match.
    ///
    /// # Errors
    ///
    /// * [`LineageError::InvalidInput`] if `indices.len() % 3 != 0`.
    /// * [`LineageError::LabelLengthMismatch`] if `face_labels.len() * 3 !=
    ///   indices.len()`.
    /// * [`LineageError::InvalidInput`] if any index is `>= positions.len()`.
    pub fn new(
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        face_labels: Vec<TopologyFaceId>,
    ) -> Result<Self, LineageError> {
        if indices.len() % 3 != 0 {
            return Err(LineageError::InvalidInput(format!(
                "indices.len() ({}) must be a multiple of 3",
                indices.len()
            )));
        }
        if face_labels.len() * 3 != indices.len() {
            return Err(LineageError::LabelLengthMismatch {
                got: face_labels.len(),
                expected: indices.len() / 3,
            });
        }
        let positions_len = positions.len();
        for (i, &idx) in indices.iter().enumerate() {
            if (idx as usize) >= positions_len {
                return Err(LineageError::InvalidInput(format!(
                    "index {idx} at indices[{i}] out of bounds (positions.len() = {positions_len})"
                )));
            }
        }
        Ok(Self {
            positions,
            indices,
            face_labels,
        })
    }

    /// Number of triangles (`indices.len() / 3`).
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of distinct face ids in `face_labels`, excluding the
    /// degenerate sentinel ([`TopologyFaceId::DEGENERATE`]).
    #[must_use]
    pub fn face_count(&self) -> usize {
        let mut seen = BTreeSet::new();
        for id in &self.face_labels {
            if !id.is_degenerate() {
                seen.insert(*id);
            }
        }
        seen.len()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- TopologyFaceId / serde -------------------------------------------

    #[test]
    fn topology_face_id_round_trips_through_serde() {
        // Use RON (already a workspace dep — cad-core has it for snapshot
        // round-trips elsewhere) so this test doesn't pull in serde_json.
        let id = TopologyFaceId(42);
        let s = ron::to_string(&id).expect("serialize");
        let back: TopologyFaceId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, back);
    }

    // --- LineageGraph -----------------------------------------------------

    #[test]
    fn lineage_graph_default_is_empty() {
        let g = LineageGraph::default();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
        let g2 = LineageGraph::new();
        assert!(g2.is_empty());
    }

    #[test]
    fn lineage_graph_push_appends() {
        let mut g = LineageGraph::new();
        g.push(LineageEdge {
            from: Some(TopologyFaceId(1)),
            to: Some(TopologyFaceId(10)),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: Some(TopologyFaceId(2)),
            to: None,
            evolution: TopologyEvolution::Deleted,
            confidence: 1.0,
        });
        assert_eq!(g.len(), 2);
        assert!(!g.is_empty());
    }

    #[test]
    fn lineage_graph_edges_from_filters_correctly() {
        let mut g = LineageGraph::new();
        let f1 = TopologyFaceId(1);
        let f2 = TopologyFaceId(2);
        g.push(LineageEdge {
            from: Some(f1),
            to: Some(TopologyFaceId(10)),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: Some(f2),
            to: Some(TopologyFaceId(11)),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: Some(f1),
            to: Some(TopologyFaceId(12)),
            evolution: TopologyEvolution::Split,
            confidence: 1.0,
        });
        assert_eq!(g.edges_from(f1).count(), 2);
        assert_eq!(g.edges_from(f2).count(), 1);
        assert_eq!(g.edges_from(TopologyFaceId(99)).count(), 0);
    }

    #[test]
    fn lineage_graph_edges_to_filters_correctly() {
        let mut g = LineageGraph::new();
        let t1 = TopologyFaceId(10);
        let t2 = TopologyFaceId(11);
        g.push(LineageEdge {
            from: Some(TopologyFaceId(1)),
            to: Some(t1),
            evolution: TopologyEvolution::Merged,
            confidence: 0.5,
        });
        g.push(LineageEdge {
            from: Some(TopologyFaceId(2)),
            to: Some(t1),
            evolution: TopologyEvolution::Merged,
            confidence: 0.5,
        });
        g.push(LineageEdge {
            from: Some(TopologyFaceId(3)),
            to: Some(t2),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        assert_eq!(g.edges_to(t1).count(), 2);
        assert_eq!(g.edges_to(t2).count(), 1);
        assert_eq!(g.edges_to(TopologyFaceId(99)).count(), 0);
    }

    #[test]
    fn lineage_graph_edges_by_evolution_filters_correctly() {
        let mut g = LineageGraph::new();
        g.push(LineageEdge {
            from: Some(TopologyFaceId(1)),
            to: Some(TopologyFaceId(10)),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: Some(TopologyFaceId(2)),
            to: None,
            evolution: TopologyEvolution::Deleted,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: None,
            to: Some(TopologyFaceId(20)),
            evolution: TopologyEvolution::Reinterpreted,
            confidence: 1.0,
        });
        g.push(LineageEdge {
            from: Some(TopologyFaceId(3)),
            to: Some(TopologyFaceId(11)),
            evolution: TopologyEvolution::Preserved,
            confidence: 1.0,
        });
        assert_eq!(
            g.edges_by_evolution(TopologyEvolution::Preserved).count(),
            2
        );
        assert_eq!(g.edges_by_evolution(TopologyEvolution::Deleted).count(), 1);
        assert_eq!(
            g.edges_by_evolution(TopologyEvolution::Reinterpreted)
                .count(),
            1
        );
        assert_eq!(g.edges_by_evolution(TopologyEvolution::Split).count(), 0);
    }

    // --- LabeledMesh ------------------------------------------------------

    #[test]
    fn labeled_mesh_new_validates_label_length() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0_u32, 1, 2];
        // Wrong: 0 labels for 1 triangle.
        let err = LabeledMesh::new(positions.clone(), indices.clone(), vec![]).unwrap_err();
        assert!(matches!(
            err,
            LineageError::LabelLengthMismatch {
                got: 0,
                expected: 1
            }
        ));
        // Wrong: 2 labels for 1 triangle.
        let err = LabeledMesh::new(
            positions.clone(),
            indices.clone(),
            vec![TopologyFaceId(0), TopologyFaceId(1)],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LineageError::LabelLengthMismatch {
                got: 2,
                expected: 1
            }
        ));
        // Correct: 1 label for 1 triangle.
        let ok = LabeledMesh::new(positions, indices, vec![TopologyFaceId(0)]).expect("valid");
        assert_eq!(ok.triangle_count(), 1);
    }

    #[test]
    fn labeled_mesh_new_validates_indices_multiple_of_3() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // 4 indices is not a multiple of 3.
        let err =
            LabeledMesh::new(positions, vec![0_u32, 1, 2, 0], vec![TopologyFaceId(0)]).unwrap_err();
        match err {
            LineageError::InvalidInput(msg) => assert!(msg.contains("multiple of 3"), "{msg}"),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn labeled_mesh_new_validates_index_bounds() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Index 5 is out of bounds (positions.len() = 3).
        let err =
            LabeledMesh::new(positions, vec![0_u32, 1, 5], vec![TopologyFaceId(0)]).unwrap_err();
        match err {
            LineageError::InvalidInput(msg) => {
                assert!(msg.contains("out of bounds"), "{msg}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn labeled_mesh_face_count_counts_distinct_labels() {
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        // 2 triangles with distinct labels.
        let m = LabeledMesh::new(
            positions.clone(),
            vec![0_u32, 1, 2, 1, 3, 2],
            vec![TopologyFaceId(0), TopologyFaceId(1)],
        )
        .expect("valid");
        assert_eq!(m.face_count(), 2);
        // 2 triangles with the same label.
        let m = LabeledMesh::new(
            positions,
            vec![0_u32, 1, 2, 1, 3, 2],
            vec![TopologyFaceId(7), TopologyFaceId(7)],
        )
        .expect("valid");
        assert_eq!(m.face_count(), 1);
    }
}
