//! Loft-specific FilletOp adapter.
//!
//! See `fillet/mod.rs` for the FilletOp shape and the
//! [`super::FilletUpstream`] trait. This file implements the trait for
//! [`LoftOp`] and exposes [`FilletOp::new_for_loft`] as the public
//! constructor.
//!
//! # Geometry
//!
//! Loft has the same `Bottom + Top + N Sides` face structure and 3N
//! clean 2-endpoint edges as Extrude (sub-β):
//!
//! | Canonical index range | Edge type | Vertex pair |
//! |---|---|---|
//! | `0..N` | Bottom-perimeter (`Bottom ∩ Side(i)`) | `(i, (i+1)%N)` on bottom ring |
//! | `N..2N` | Top-perimeter (`Top ∩ Side(i)`) | `(N+i, N+(i+1)%N)` on top ring |
//! | `2N..3N` | Vertical seam (`Side(i) ∩ Side((i+1)%N)`) | `((i+1)%N, N+(i+1)%N)` |
//!
//! All 3N edges are chamferable in v0; no
//! [`super::FilletError::UnsupportedEdgeGeometry`] variant is ever
//! returned for Loft inputs (mirrors Extrude).
//!
//! # Profile pairing (load-bearing convention)
//!
//! v0 inherits LoftOp's `profile_a[i]` ↔ `profile_b[i]` pairing
//! convention exactly. **No twist matching, no rotation alignment, no
//! coordinate-aware vertex correspondence is introduced by sub-δ.**
//! If [`LoftOp`] ever extends to support twist or vertex correspondence,
//! sub-δ's chamfer geometry will need to be revisited; until then,
//! the i-to-i convention is the substrate's contract.
//!
//! # Inward chamfer offset
//!
//! Mirrors Extrude's face-normal-bisector approach with two-profile
//! awareness:
//!
//! * Bottom-perimeter edge `i`: bottom face normal `(0, 0, -1)` +
//!   side face normal at `z = 0` (perpendicular to `profile_a` edge `i`).
//! * Top-perimeter edge `i`: top face normal `(0, 0, 1)` +
//!   side face normal at `z = length` (perpendicular to `profile_b`
//!   edge `i`).
//! * Vertical seam `i`: average of side normals at `z = 0` (from
//!   `profile_a`) and at `z = length` (from `profile_b`), for both
//!   `Side(i)` and `Side((i+1)%N)` — total of four normal contributions.
//!
//! Magnitude convention: half-bisector (~0.707) matching sub-α/β.

use super::{ChamferSpec, FilletError, FilletOp, FilletUpstream};
use crate::operators::LoftOp;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl FilletUpstream for LoftOp {
    fn resolve_chamfer_spec(&self, canonical_index: usize) -> Result<ChamferSpec, &'static str> {
        let n_a = self.profile_a.len();
        let n_b = self.profile_b.len();
        // Defensive: LoftOp::evaluate enforces equal counts, but use min
        // here for the same robustness reason sub-7.2-ζ.δ's
        // BRepEdgeProvider impl does (mid-mutation through pub fields).
        let n = n_a.min(n_b);

        if canonical_index < n {
            // Bottom-perimeter edge i.
            let i = canonical_index;
            let next = (i + 1) % n;
            let vertex_a = i as u32;
            let vertex_b = next as u32;
            let inward_direction = loft_chamfer_inward_direction_bottom_perimeter(self, i);
            Ok(ChamferSpec {
                vertex_a,
                vertex_b,
                inward_direction,
            })
        } else if canonical_index < 2 * n {
            // Top-perimeter edge i.
            let local = canonical_index - n;
            let i = local;
            let next = (i + 1) % n;
            let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);
            let vertex_a = n_u32 + (i as u32);
            let vertex_b = n_u32 + (next as u32);
            let inward_direction = loft_chamfer_inward_direction_top_perimeter(self, i);
            Ok(ChamferSpec {
                vertex_a,
                vertex_b,
                inward_direction,
            })
        } else if canonical_index < 3 * n {
            // Vertical seam i: shared between Side(i) and Side((i+1)%N).
            // Mirrors Extrude's vertical-seam vertex pairing
            // (extrude.rs::extrude_edge_vertex_pair, sub-β):
            // bottom_ring[(i+1)%N] connects to top_ring[(i+1)%N].
            let local = canonical_index - 2 * n;
            let v = (local + 1) % n;
            let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);
            let vertex_a = v as u32;
            let vertex_b = n_u32 + (v as u32);
            let inward_direction = loft_chamfer_inward_direction_vertical(self, local, n);
            Ok(ChamferSpec {
                vertex_a,
                vertex_b,
                inward_direction,
            })
        } else {
            // Defensive — should be unreachable since canonical_index
            // came from upstream.brep_edge_ids(owner) which is bounded
            // to 3N. Treat as substrate bug.
            Err("loft canonical edge index out of bounds; substrate bug")
        }
    }
}

/// Outward normal of `profile_a`'s edge `i` (in XY plane, at `z = 0`).
///
/// For a CCW-wound profile (signed area `> 0`) the outward normal is
/// `(dy, -dx)` for edge vector `(dx, dy)`. Returns the zero vector for a
/// degenerate (zero-length) edge — `Polygon2D::new` rejects these at
/// construction so this is a defensive fallback. Mirrors the
/// extrude.rs::extrude_side_outward_normal convention exactly.
fn profile_a_side_outward_normal(upstream: &LoftOp, i: usize) -> [f32; 3] {
    let pts = upstream.profile_a.points();
    let n = pts.len();
    if n == 0 {
        return [0.0, 0.0, 0.0];
    }
    let p_i = pts[i % n];
    let p_next = pts[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return [0.0, 0.0, 0.0];
    }
    [dy / mag, -dx / mag, 0.0]
}

/// Outward normal of `profile_b`'s edge `i` (in XY plane, at
/// `z = length`).
///
/// Same `(dy, -dx)` rotation as `profile_a_side_outward_normal`. The
/// two helpers are intentionally separate to make the i-to-i pairing
/// convention visible at the call site (each profile's edge is read in
/// isolation; no cross-profile correspondence is introduced).
fn profile_b_side_outward_normal(upstream: &LoftOp, i: usize) -> [f32; 3] {
    let pts = upstream.profile_b.points();
    let n = pts.len();
    if n == 0 {
        return [0.0, 0.0, 0.0];
    }
    let p_i = pts[i % n];
    let p_next = pts[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return [0.0, 0.0, 0.0];
    }
    [dy / mag, -dx / mag, 0.0]
}

/// Bottom-perimeter chamfer direction: bottom face `(0, 0, -1)` +
/// `profile_a`'s side normal at edge `i`. Inward bisector =
/// `-(bottom + side) / 2`.
fn loft_chamfer_inward_direction_bottom_perimeter(upstream: &LoftOp, i: usize) -> [f32; 3] {
    let side = profile_a_side_outward_normal(upstream, i);
    [
        -side[0] / 2.0,
        -side[1] / 2.0,
        0.5, // -((-1) + 0) / 2 = 0.5; side has no Z component
    ]
}

/// Top-perimeter chamfer direction: top face `(0, 0, 1)` +
/// `profile_b`'s side normal at edge `i`. Inward bisector =
/// `-(top + side) / 2`.
fn loft_chamfer_inward_direction_top_perimeter(upstream: &LoftOp, i: usize) -> [f32; 3] {
    let side = profile_b_side_outward_normal(upstream, i);
    [
        -side[0] / 2.0,
        -side[1] / 2.0,
        -0.5, // -((1) + 0) / 2 = -0.5; side has no Z component
    ]
}

/// Vertical-seam chamfer direction: average of four side-normal
/// contributions across both profiles' adjacent edges. Inward bisector
/// = `-(sum_of_four_normals / 4)`.
///
/// `Side(local) ∩ Side((local + 1) % N)` is the boundary between two
/// consecutive side faces; both side faces span between the bottom and
/// top rings, so the inward direction averages the two profiles' two
/// adjacent side normals — four normal contributions in total.
fn loft_chamfer_inward_direction_vertical(upstream: &LoftOp, local: usize, n: usize) -> [f32; 3] {
    let normal_a_i = profile_a_side_outward_normal(upstream, local);
    let normal_a_next = profile_a_side_outward_normal(upstream, (local + 1) % n);
    let normal_b_i = profile_b_side_outward_normal(upstream, local);
    let normal_b_next = profile_b_side_outward_normal(upstream, (local + 1) % n);
    [
        -(normal_a_i[0] + normal_a_next[0] + normal_b_i[0] + normal_b_next[0]) / 4.0,
        -(normal_a_i[1] + normal_a_next[1] + normal_b_i[1] + normal_b_next[1]) / 4.0,
        0.0, // all four side normals lie in XY plane
    ]
}

impl FilletOp {
    /// Sub-δ public API — Loft constructor.
    ///
    /// Validates each [`BRepEdgeId`] against the upstream's
    /// [`crate::topology::BRepEdgeProvider`]. All `3 * N` Loft edges
    /// have clean 2-endpoint geometry, so
    /// [`FilletError::UnsupportedEdgeGeometry`] is never returned for
    /// Loft inputs — only [`FilletError::EdgeNotInUpstream`],
    /// [`FilletError::InvalidRadius`], or
    /// [`FilletError::EmptyEdgeSelection`] paths apply.
    ///
    /// # Errors
    ///
    /// See [`FilletError`].
    pub fn new_for_loft(
        upstream: &LoftOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Sub-δ unit tests — Loft constructor + helper-table correctness.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{Operator, Polygon2D};
    use crate::topology::BRepEdgeProvider;

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("ccw unit square")
    }

    fn small_pentagon() -> Polygon2D {
        Polygon2D::new(vec![
            [1.0, 0.0],
            [0.309, 0.951],
            [-0.809, 0.588],
            [-0.809, -0.588],
            [0.309, -0.951],
        ])
        .expect("ccw regular pentagon")
    }

    #[test]
    fn new_for_loft_rejects_zero_radius() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let edge = loft.brep_edge_ids(owner())[0];
        let err = FilletOp::new_for_loft(&loft, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_loft_rejects_negative_radius() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let edge = loft.brep_edge_ids(owner())[0];
        let err = FilletOp::new_for_loft(&loft, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_for_loft_rejects_non_finite_radius() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let edge = loft.brep_edge_ids(owner())[0];
        let err_nan = FilletOp::new_for_loft(&loft, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, FilletError::InvalidRadius { .. }));
        let err_inf =
            FilletOp::new_for_loft(&loft, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, FilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_loft_rejects_empty_edge_list() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let err = FilletOp::new_for_loft(&loft, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, FilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_loft_rejects_unknown_edge_id() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = FilletOp::new_for_loft(&loft, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, FilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    #[test]
    fn new_for_loft_accepts_single_bottom_perimeter_edge() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let edges = loft.brep_edge_ids(owner());
        // Bottom-perimeter edges occupy indices 0..N=4. Edge[0] is
        // Bottom ∩ Side(0).
        let op = FilletOp::new_for_loft(&loft, owner(), vec![edges[0]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[0]]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_for_loft_accepts_all_3n_edges() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        let all_edges = loft.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 12); // 3 * 4
        let op = FilletOp::new_for_loft(&loft, owner(), all_edges.clone(), 0.05).expect("12 edges");
        assert_eq!(op.edges().len(), 12);
        assert_eq!(op.edges(), &all_edges[..]);
    }

    /// Confirm the canonical edge → vertex-pair mapping for a
    /// 4-vertex profile against `loft.rs::evaluate`'s vertex layout
    /// (bottom_ring `[0..N]` from profile_a, top_ring `[N..2N]` from
    /// profile_b). For N=4:
    ///
    /// * Bottom-perimeter `0` connects `bottom_ring[0]=0` and
    ///   `bottom_ring[1]=1` → `(0, 1)`.
    /// * Top-perimeter `0` (canonical index 4) connects
    ///   `top_ring[0]=4` and `top_ring[1]=5` → `(4, 5)`.
    /// * Vertical-seam `0` (canonical index 8) connects
    ///   `bottom_ring[1]=1` and `top_ring[1]=5` → `(1, 5)`.
    #[test]
    fn loft_edge_vertex_pair_table_correctness() {
        let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");

        // Bottom perimeter (canonical_index 0..N).
        let spec_0 = loft.resolve_chamfer_spec(0).expect("0");
        assert_eq!((spec_0.vertex_a, spec_0.vertex_b), (0, 1));
        let spec_1 = loft.resolve_chamfer_spec(1).expect("1");
        assert_eq!((spec_1.vertex_a, spec_1.vertex_b), (1, 2));
        let spec_2 = loft.resolve_chamfer_spec(2).expect("2");
        assert_eq!((spec_2.vertex_a, spec_2.vertex_b), (2, 3));
        let spec_3 = loft.resolve_chamfer_spec(3).expect("3");
        assert_eq!((spec_3.vertex_a, spec_3.vertex_b), (3, 0));

        // Top perimeter (canonical_index N..2N).
        let spec_4 = loft.resolve_chamfer_spec(4).expect("4");
        assert_eq!((spec_4.vertex_a, spec_4.vertex_b), (4, 5));
        let spec_5 = loft.resolve_chamfer_spec(5).expect("5");
        assert_eq!((spec_5.vertex_a, spec_5.vertex_b), (5, 6));
        let spec_6 = loft.resolve_chamfer_spec(6).expect("6");
        assert_eq!((spec_6.vertex_a, spec_6.vertex_b), (6, 7));
        let spec_7 = loft.resolve_chamfer_spec(7).expect("7");
        assert_eq!((spec_7.vertex_a, spec_7.vertex_b), (7, 4));

        // Vertical seams (canonical_index 2N..3N). Side(i) ∩ Side((i+1)%N)
        // shares profile-vertex (i+1)%N, so the seam runs from
        // bottom_ring[(i+1)%N] to top_ring[(i+1)%N].
        let spec_8 = loft.resolve_chamfer_spec(8).expect("8");
        assert_eq!((spec_8.vertex_a, spec_8.vertex_b), (1, 5));
        let spec_9 = loft.resolve_chamfer_spec(9).expect("9");
        assert_eq!((spec_9.vertex_a, spec_9.vertex_b), (2, 6));
        let spec_10 = loft.resolve_chamfer_spec(10).expect("10");
        assert_eq!((spec_10.vertex_a, spec_10.vertex_b), (3, 7));
        let spec_11 = loft.resolve_chamfer_spec(11).expect("11");
        assert_eq!((spec_11.vertex_a, spec_11.vertex_b), (0, 4));
    }

    #[test]
    fn evaluate_one_loft_edge_adds_2_vertices_and_2_triangles() {
        let loft = LoftOp::new(small_pentagon(), small_pentagon(), 1.5).expect("loft");
        let edge = loft.brep_edge_ids(owner())[0];
        let op = FilletOp::new_for_loft(&loft, owner(), vec![edge], 0.05).expect("ok");
        let upstream = loft.evaluate(&[]).expect("loft tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // Pentagon loft: 2N=10 verts, 4N-4=16 triangles (48 indices).
        // After 1 fillet: +2 verts + 6 indices (2 triangles).
        assert_eq!(out.positions.len(), upstream.positions.len() + 2);
        assert_eq!(out.indices.len(), upstream.indices.len() + 6);
    }

    #[test]
    fn evaluate_three_loft_edges_linear_growth() {
        let loft = LoftOp::new(small_pentagon(), small_pentagon(), 1.0).expect("loft");
        let all_edges = loft.brep_edge_ids(owner());
        // Pick 3 non-adjacent canonical edges: one bottom, one top,
        // one vertical-seam (indices 0, 5, 11).
        let op = FilletOp::new_for_loft(
            &loft,
            owner(),
            vec![all_edges[0], all_edges[5], all_edges[11]],
            0.05,
        )
        .expect("ok");
        let upstream = loft.evaluate(&[]).expect("loft tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // 3 fillets: +6 verts + 18 indices.
        assert_eq!(out.positions.len(), upstream.positions.len() + 6);
        assert_eq!(out.indices.len(), upstream.indices.len() + 18);
    }
}
