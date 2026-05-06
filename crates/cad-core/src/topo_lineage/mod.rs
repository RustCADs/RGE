//! `cad_core::topo_lineage` — Phase 7.4 topology lineage prototype.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! # Design
//!
//! Per [PLAN.md §1.5.4.3](../../../plans/PLAN.md) and ADR-098. The topology
//! lineage graph tracks how individual faces evolve through CAD operators —
//! preserved, split, merged, deleted, or reinterpreted (newly introduced).
//! Without lineage the persistent-ID story is "remap and hope"; with lineage,
//! identity becomes traceable across rebuilds, replication, and history-walk
//! UIs.
//!
//! This module is the **v0 prototype**. It surfaces design unknowns early
//! using the simplest possible substrate: per-face plane-equation hashing.
//! The eventual hybrid design (per ADR-112 §"Phase 7.2 / 7.4 hook") combines
//! csgrs's per-polygon metadata-passthrough with this plane-based fallback;
//! the prototype here is the plane-only path, intended to surface the API
//! shape and its sharp edges.
//!
//! # v0 simplifications vs PLAN §1.5.4.3 spec (DOCUMENTED for follow-up)
//!
//! The PLAN spec includes these fields/structures that the v0 prototype
//! deliberately defers:
//!
//! * `OperatorId` field on `LineageEdge` — depends on a stable
//!   operator-instance identity beyond `NodeId`. Defer.
//! * `SemanticScore` field on `LineageEdge` — depends on a richer semantic
//!   model. Defer.
//! * `Split(Vec<PersistentFaceId>)` / `Merged(Vec<PersistentFaceId>)` inner
//!   data on `TopologyEvolution` — for v0 we represent these via multiple
//!   `LineageEdge` entries with a shared `from` (Split) or shared `to`
//!   (Merged); the discriminant-only enum keeps the API surface small.
//! * `PersistentFaceId` (content-hash + lineage-path identifier) — the v0
//!   uses sequential `TopologyFaceId` per-mesh; not stable across rebuilds.
//!   That's a Phase 7.2 dispatch (needs a B-Rep model first).
//! * Per-edge / per-vertex lineage — face-only for v0.
//! * csgrs metadata-passthrough — D-Boolean confirmed it works for
//!   Union/Intersection (clones polygon metadata through plane splits +
//!   `clip_polygons`); Difference retags rhs as lhs's metadata. The v0
//!   prototype does **not** consume that metadata; it uses pure
//!   plane-equation matching as its lineage substrate. Hybridization is a
//!   future optimization documented in ADR-112.
//!
//! # Heuristic: plane-equation matching
//!
//! The lineage inference assumes a face is identifiable by the plane its
//! triangles lie on. For each input face (one plane = one `face_id`), the
//! inference looks for a matching plane in the output:
//!
//! * Exact plane match + same triangle count → `Preserved` (confidence 1.0)
//! * Exact plane match + fewer output triangles → `Split` (confidence 1.0)
//! * Exact plane match + more output triangles → `Merged` (confidence 0.5)
//! * No matching plane in output → `Deleted` (confidence 1.0)
//!
//! Output planes with no input match → `Reinterpreted` (confidence 1.0,
//! `from = None`).
//!
//! "Exact plane match" is plane-equation equality at ~1e-4 quantization. The
//! private `QuantizedPlane` type sign-canonicalizes opposite-winding (front
//! vs back) duplicates of the same plane so they hash identically — the
//! lineage logic does not care which side of a plane a triangle lies on.
//!
//! # Boundary-precision Split detection
//!
//! The v0 uses the simple triangle-count comparison heuristic noted above.
//! True Split detection (i.e. "two disjoint regions on the same plane")
//! requires connected-component analysis on triangle adjacency, which is
//! out of scope for this prototype.
//!
//! # Degenerate triangle handling in labeling
//!
//! [`label_by_plane`] and [`infer_lineage`] **skip** triangles that are
//! degenerate (zero-area) or have non-finite normals rather than erroring.
//! Real-world CSG output (csgrs's BSP-tree triangulation in particular)
//! routinely contains slivers and zero-area artifacts, especially around
//! intersection planes; the v0 prototype must not crash on these. Skipped
//! triangles still receive a face label (assigned to a special "degenerate"
//! face id) so `face_labels.len() == triangle_count` is preserved. Distinct
//! degenerate triangles share a single sentinel face id rather than each
//! producing a fresh one.
//!
//! [`LineageError::DegenerateTriangle`] / [`LineageError::NonFiniteNormal`]
//! remain reachable through the private `QuantizedPlane::from_triangle` and
//! are exercised by the unit tests; they exist as a future strict-mode hook.
//!
//! # Module layout
//!
//! * `types` — [`LineageError`], [`TopologyEvolution`], [`LineageEdge`],
//!   [`LineageGraph`]. ([`TopologyFaceId`] re-exported from
//!   [`crate::tessellation`] for back-compat.)
//! * `plane` — `QuantizedPlane` (private; only used by `infer`).
//! * `infer` — [`label_by_plane`] + [`infer_lineage`] (the unified
//!   labeled-or-unlabeled path).
//!
//! All three are private sub-modules; the public API is re-exported here
//! and at the crate root. Per the 2026-05-08 unified mesh refactor, the
//! `LabeledMesh` type has been collapsed into [`crate::Tessellation`]'s
//! optional `face_labels` field, and the `infer_lineage` /
//! `infer_lineage_labeled` duplication has collapsed to a single
//! [`infer_lineage`] that dispatches on `output.is_labeled()`.

#![allow(clippy::module_name_repetitions)]

mod infer;
mod plane;
mod types;

pub use infer::{infer_lineage, label_by_plane};
pub use types::{LineageEdge, LineageError, LineageGraph, TopologyEvolution, TopologyFaceId};
