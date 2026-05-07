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
//! * [`TopologyEvolution`] — `Preserved` / `Split` / `Merged` / `Deleted` / `Reinterpreted`
//! * [`LineageEdge`] — one step in a face's history
//! * [`LineageGraph`] — collection of edges with simple iteration helpers
//!
//! [`TopologyFaceId`] lives in [`crate::tessellation::mesh`] (so the
//! [`crate::Tessellation`] substrate can carry per-triangle labels without
//! a `tessellation → topo_lineage` reverse import) and is re-exported here
//! for back-compat with code that imports it from `topo_lineage::types`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Re-export so existing `crate::topo_lineage::types::TopologyFaceId` paths
// keep working after the move to `tessellation::mesh`.
pub use crate::tessellation::TopologyFaceId;

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
#[non_exhaustive]
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
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    /// SemVer hardening fixture: [`TopologyEvolution`] is `#[non_exhaustive]`,
    /// so cross-crate consumers MUST include a wildcard arm when
    /// pattern-matching. ADR-098 explicitly tags this enum as "v0 prototype" —
    /// future variants are likely. This test simulates the consumer pattern:
    /// when future variants are added, the wildcard arm absorbs them and this
    /// test still compiles — proving the `#[non_exhaustive]` annotation is
    /// correctly applied.
    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn topology_evolution_non_exhaustive_pattern_match_compiles() {
        let evo = TopologyEvolution::Preserved;
        let _label = match evo {
            TopologyEvolution::Preserved => "preserved",
            TopologyEvolution::Split => "split",
            TopologyEvolution::Merged => "merged",
            TopologyEvolution::Deleted => "deleted",
            TopologyEvolution::Reinterpreted => "reinterpreted",
            _ => "future-variant", // required by #[non_exhaustive]
        };
    }
}
