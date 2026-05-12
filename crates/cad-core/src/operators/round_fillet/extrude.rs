//! `RoundFilletOp` constructor + helpers for `ExtrudeOp` upstream (sub-β).
//!
//! Per ADR-119 D5 (substrate parallelism, not sharing) + the sub-β
//! green-light direction ("cap-perimeter only; vertical seams reject
//! via existing `UnsupportedEdgeGeometry` and become sub-β.γ"), this
//! module mirrors the shape of the chamfer side's
//! [`crate::operators::fillet::extrude`] module but stays byte-distinct
//! AND has stricter edge eligibility than chamfer's `new_for_extrude`.
//!
//! # Edge eligibility (sub-β scope)
//!
//! `ExtrudeOp`'s [`crate::topology::BRepEdgeProvider`] impl emits `3 * N`
//! edges for a profile of `N` vertices, in three classes:
//!
//! * Indices `0..N` — bottom-perimeter (`Bottom ∩ Side(i)`)
//! * Indices `N..2N` — top-perimeter (`Top ∩ Side(i)`)
//! * Indices `2N..3N` — vertical-seam (`Side(i) ∩ Side((i + 1) % N)`)
//!
//! **Sub-β supports bottom-perimeter + top-perimeter edges only** (the
//! `2 * N` cap-perimeter edges). These have **90° dihedrals by
//! construction**: Bottom/Top caps lie in the XY plane (normal `±Z`),
//! Side(i) walls extrude along Z with normals in the XY plane —
//! `n_cap · n_side = 0` regardless of profile shape. Sub-α's cylinder
//! parameterization (`axis_center = pos + r·(a + b)`, quarter-arc θ ∈
//! [0, π/2]) is geometrically correct only when `a ⊥ b`; for the
//! cap-perimeter edges this holds for any valid profile.
//!
//! **Vertical-seam edges are REJECTED** at construction time via
//! [`RoundFilletError::UnsupportedEdgeGeometry`]. The dihedral between
//! adjacent Side faces depends on the profile interior angle at the
//! shared corner — it equals 90° only for rectangular profiles; for
//! pentagons / triangles / etc. it's profile-dependent. Generalizing
//! sub-α's `axis_center` placement and arc parameterization to
//! arbitrary dihedrals is a **separate dispatch (sub-β.γ)**; folding it
//! into sub-β would change `RoundFilletOp::evaluate`'s body — which
//! triggers the user-stated halt condition "broader face-strip
//! algorithm than the Cuboid inset-vertex path can honestly support".
//!
//! # Substrate posture
//!
//! `RoundFilletOp` (struct, evaluate body, error enum, spec, trait,
//! resolver arms) stays **byte-identical to sub-α** (`c5c590a` →
//! `7087589`). This module adds ONLY the `RoundFilletUpstream` impl
//! for `ExtrudeOp` and the public `RoundFilletOp::new_for_extrude`
//! constructor (thin delegate to `from_upstream`). Chamfer's
//! `fillet::extrude::FilletOp::new_for_extrude` (D6 byte-identical) is
//! parallel substrate, not shared.

use super::{RoundFilletError, RoundFilletOp, RoundFilletSpec, RoundFilletUpstream};
use crate::operators::ExtrudeOp;
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl RoundFilletUpstream for ExtrudeOp {
    fn resolve_round_spec(&self, canonical_index: usize) -> Result<RoundFilletSpec, &'static str> {
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        let n_usize = n as usize;

        // Three edge-class dispatch over BRepEdgeProvider's canonical
        // emission order:
        //   [0..N)   bottom-perimeter — Bottom ∩ Side(i)
        //   [N..2N)  top-perimeter    — Top ∩ Side(i)
        //   [2N..3N) vertical-seam    — Side(i) ∩ Side((i+1)%N)  — REJECTED
        if canonical_index < n_usize {
            // Bottom-perimeter edge i.
            let i = canonical_index;
            let (vertex_a, vertex_b) = extrude_bottom_perimeter_vertex_pair(i, n);
            let side_normal = extrude_side_outward_normal(i, self);
            // face_a = Bottom (TopologyFaceId(0), normal -Z).
            // face_b = Side(i) (TopologyFaceId(2 + i), normal in XY).
            //
            // face_a_inward: in Bottom's plane (z=0), perpendicular to
            // edge (which runs along the profile edge in XY),
            // pointing INTO Bottom's interior (= away from Side(i)).
            // Cap × side dihedrals are perpendicular so this is just
            // `-side_normal` projected to XY (which equals
            // `-side_normal` itself since side_normal has no Z).
            //
            // face_b_inward: in Side(i)'s plane, perpendicular to edge,
            // pointing INTO Side(i)'s interior (= toward Top from
            // Bottom). Side's plane is z-extruded so the
            // perpendicular-to-edge direction in-plane is ±Z. From
            // Bottom edge, "into Side" means going UP toward Top:
            // face_b_inward = (0, 0, 1).
            let face_a_inward = [-side_normal[0], -side_normal[1], 0.0];
            let face_b_inward = [0.0, 0.0, 1.0];
            Ok(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(0),
                face_b_id: TopologyFaceId(2 + i as u64),
                face_a_inward,
                face_b_inward,
            })
        } else if canonical_index < 2 * n_usize {
            // Top-perimeter edge i (local index = canonical_index - N).
            let local = canonical_index - n_usize;
            let (vertex_a, vertex_b) = extrude_top_perimeter_vertex_pair(local, n);
            let side_normal = extrude_side_outward_normal(local, self);
            // face_a = Top (TopologyFaceId(1), normal +Z).
            // face_b = Side(local).
            //
            // face_a_inward: in Top's plane, away from Side =
            // -side_normal (no Z component).
            // face_b_inward: in Side(i)'s plane, perpendicular to
            // edge, pointing INTO Side(i) interior (= DOWN toward
            // Bottom from Top): (0, 0, -1).
            let face_a_inward = [-side_normal[0], -side_normal[1], 0.0];
            let face_b_inward = [0.0, 0.0, -1.0];
            Ok(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(1),
                face_b_id: TopologyFaceId(2 + local as u64),
                face_a_inward,
                face_b_inward,
            })
        } else if canonical_index < 3 * n_usize {
            // Vertical-seam edge — REJECTED in sub-β per ADR-119 +
            // green-light scope. Returns the static string that
            // `from_upstream` wraps into
            // `RoundFilletError::UnsupportedEdgeGeometry`.
            //
            // The dihedral between Side(i) and Side((i+1)%N) depends
            // on the profile's interior angle at the shared corner;
            // it equals 90° only for rectangular profiles. Sub-α's
            // cylinder math (axis_center = pos + r·(a+b), θ ∈
            // [0, π/2]) is geometrically correct only for
            // perpendicular dihedrals. Sub-β.γ will generalize the
            // arc parameterization; folding it into sub-β would
            // change `RoundFilletOp::evaluate` — outside scope.
            Err("vertical-seam edges require general-dihedral cylinder math; sub-β supports cap-perimeter edges only")
        } else {
            // Defensive: from_upstream's caller-side filter already
            // restricts canonical_index to the upstream's
            // brep_edge_ids length (exactly 3N for any ExtrudeOp).
            // Unreachable in production paths.
            Err("extrude canonical edge index out of range (must be < 3N)")
        }
    }
}

impl RoundFilletOp {
    /// Construct a [`RoundFilletOp`] validated against the upstream
    /// `ExtrudeOp`, with edge selection restricted to cap-perimeter
    /// edges (bottom-perimeter + top-perimeter; `2 * N` edges total).
    ///
    /// Mirrors [`RoundFilletOp::new`] (Cuboid) but resolves edges
    /// against `upstream.brep_edge_ids(owner)` (the
    /// [`crate::topology::BRepEdgeProvider`] impl on
    /// [`crate::operators::ExtrudeOp`], emitting `3 * N` edges in the
    /// canonical order `[Bottom-perimeter | Top-perimeter |
    /// Vertical-seams]`). Vertical-seam edges are intentionally
    /// rejected via [`RoundFilletError::UnsupportedEdgeGeometry`] per
    /// sub-β scope (ADR-119 D7 — sub-β.γ generalizes to arbitrary
    /// dihedrals).
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if any edge ID
    ///   corresponds to a vertical-seam edge (canonical index `>=
    ///   2N`). Sub-β.γ lifts this restriction.
    pub fn new_for_extrude(
        upstream: &ExtrudeOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Extrude helpers — derived from extrude.rs::evaluate's 2N-vertex layout.
//
// Per ADR-119 D5 these are duplicated from `fillet::extrude` (chamfer)
// rather than shared. The byte-identical formulas for vertex-pair
// mapping and side outward-normal are intentional; future-evolution
// divergence (a hypothetical Extrude winding change affecting one
// operator but not the other) MUST be expressible without rippling.
// ---------------------------------------------------------------------------

/// Map a bottom-perimeter local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair in the upstream Extrude's vertex array.
///
/// `ExtrudeOp` bottom ring occupies `positions[0..N]` in
/// (CCW-corrected) profile order; bottom-perimeter edge `i` connects
/// `bottom_ring[i]` and `bottom_ring[(i + 1) % N]`.
fn extrude_bottom_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (i_u32, next)
}

/// Map a top-perimeter local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair. Top ring occupies `positions[N..2N]`.
fn extrude_top_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (profile_count + i_u32, profile_count + next)
}

/// Outward normal of Extrude's side face `i`, in the XY plane.
///
/// `Side(i)` corresponds to the profile edge from `profile[i]` to
/// `profile[(i + 1) % N]`. For a CCW-wound profile (signed_area > 0),
/// the outward normal is obtained by rotating the edge vector
/// `(dx, dy)` by `-90°`, i.e. `(dy, -dx)`. Returns the zero vector
/// for a degenerate (zero-length) edge — `Polygon2D::new` rejects
/// these at construction so this is a defensive fallback.
///
/// **CCW-profile convention**: matches `extrude.rs`'s side-wall
/// outward-normal direction for canonical (CCW or CCW-corrected)
/// profile order. CW profiles surface the CW caveat documented in
/// `extrude.rs` — sub-β coverage uses CCW profiles only.
fn extrude_side_outward_normal(i: usize, upstream: &ExtrudeOp) -> [f32; 3] {
    let n = upstream.profile.len();
    let p_i = upstream.profile.points()[i];
    let p_next = upstream.profile.points()[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return [0.0, 0.0, 0.0];
    }
    [dy / mag, -dx / mag, 0.0]
}

// ---------------------------------------------------------------------------
// Sub-β unit tests — Extrude constructor + cap-perimeter acceptance
// + vertical-seam rejection + profile-size scaling.
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

    // -- Construction reject paths (mirrors chamfer + sub-α discipline) -----

    #[test]
    fn new_for_extrude_rejects_zero_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_extrude_rejects_negative_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_for_extrude_rejects_non_finite_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err_nan =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, RoundFilletError::InvalidRadius { .. }));
        let err_inf = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::INFINITY)
            .unwrap_err();
        assert!(matches!(err_inf, RoundFilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_extrude_rejects_empty_edge_list() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, RoundFilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_extrude_rejects_unknown_edge_id() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, RoundFilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    // -- Sub-β load-bearing vertical-seam rejection --------------------------

    /// Sub-β scope discipline: vertical-seam edges (canonical indices
    /// `2N..3N`) reject at construction with
    /// [`RoundFilletError::UnsupportedEdgeGeometry`]. For a square
    /// profile (N=4) the vertical-seam edges are indices 8, 9, 10, 11.
    /// We verify EACH of the 4 vertical-seam edges rejects — covering
    /// the entire third-class of edges — and that the reason string
    /// references the sub-β.γ deferral.
    #[test]
    fn new_for_extrude_rejects_vertical_seam_edges_with_unsupported_edge_geometry() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 12, "square N=4 → 3*4=12 edges");

        // For a SQUARE the vertical seams have 90° dihedrals so
        // they're geometrically supportable, but sub-β rejects ALL
        // vertical seams uniformly to keep the substrate honest: the
        // class-level filter is "vertical seams are out of sub-β
        // scope" not "non-perpendicular vertical seams are out". This
        // matches the user direction ("vertical seams should reject
        // with the existing UnsupportedEdgeGeometry path and become
        // sub-β.γ"). Sub-β.γ unconditionally lifts the class
        // restriction (and adds the general-dihedral math).
        for vs_idx in 8..12 {
            let edge = all_edges[vs_idx];
            let err =
                RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).unwrap_err();
            match err {
                RoundFilletError::UnsupportedEdgeGeometry { edge: e, reason } => {
                    assert_eq!(e, edge, "error carries the offending edge id");
                    assert!(
                        reason.contains("vertical-seam") || reason.contains("cap-perimeter"),
                        "reason should reference vertical-seam / cap-perimeter scope, got: {reason}"
                    );
                }
                other => panic!(
                    "vertical-seam edge {vs_idx} (canonical_index = {vs_idx}) should reject with \
                     UnsupportedEdgeGeometry; got {other:?}"
                ),
            }
        }
    }

    /// Mixed selection (1 cap-perimeter + 1 vertical-seam) rejects via
    /// the vertical-seam: the first vertical-seam in the selection
    /// triggers the failure. Pins the order-independence (cap-
    /// perimeter validation per-edge happens first, but the
    /// `from_upstream` loop short-circuits on the first failure).
    #[test]
    fn new_for_extrude_rejects_mixed_selection_with_vertical_seam() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        // [0] is bottom-perimeter (supported); [8] is vertical-seam
        // (unsupported).
        let err = RoundFilletOp::new_for_extrude(
            &extrude,
            owner(),
            vec![all_edges[0], all_edges[8]],
            0.1,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RoundFilletError::UnsupportedEdgeGeometry { .. }
        ));
    }

    // -- Cap-perimeter acceptance --------------------------------------------

    #[test]
    fn new_for_extrude_accepts_single_bottom_perimeter_edge() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // Bottom-perimeter edges occupy indices 0..N=4. Edge[0] is
        // Bottom ∩ Side(0).
        let op =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edges[0]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[0]]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_for_extrude_accepts_single_top_perimeter_edge() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // Top-perimeter edges occupy indices N..2N = 4..8. Edge[4] is
        // Top ∩ Side(0).
        let op =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edges[4]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[4]]);
    }

    #[test]
    fn new_for_extrude_accepts_all_cap_perimeter_edges() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // All 8 cap-perimeter edges (indices 0..8).
        let cap_edges: Vec<_> = edges[..8].to_vec();
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), cap_edges.clone(), 0.05)
            .expect("8 cap-perimeter");
        assert_eq!(op.edges().len(), 8);
        assert_eq!(op.edges(), &cap_edges[..]);
    }

    // -- Evaluation geometry --------------------------------------------------

    /// Bottom-perimeter cap × side fillet on a unit-square extrude:
    /// upstream = 8 verts / 12 tris / 36 indices; per-edge addition =
    /// 4 inset + 2*(N+1)=18 cylinder = 22 verts, 2*N=16 cylinder tris;
    /// upstream-triangle indices substituted (not added). Total:
    /// 8 + 22 = 30 verts; 12 + 16 = 28 tris; 36 + 48 = 84 indices.
    #[test]
    fn evaluate_one_bottom_perimeter_edge_produces_expected_counts() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 30, "8 upstream + 22 per-edge");
        assert_eq!(out.triangle_count(), 28, "12 upstream + 16 per-edge");
        assert_eq!(out.indices.len(), 84);
    }

    /// Top-perimeter cap × side fillet: same per-edge math as bottom-
    /// perimeter (mirror across z=length/2 plane).
    #[test]
    fn evaluate_one_top_perimeter_edge_produces_expected_counts() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        // Edge[4] is the first top-perimeter edge.
        let edge = extrude.brep_edge_ids(owner())[4];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 30);
        assert_eq!(out.triangle_count(), 28);
        assert_eq!(out.indices.len(), 84);
    }

    /// Output preserves labeled-ness from the upstream + cylinder
    /// triangles get `TopologyFaceId::DEGENERATE` (sub-α + ADR-119 D3).
    /// The Bottom cap fan triangles' labels stay `TopologyFaceId(0)`
    /// after vertex-substitution; the Side(0) wall triangles' labels
    /// stay `TopologyFaceId(2)`; the new 16 cylinder triangles all
    /// emit `DEGENERATE`.
    #[test]
    fn evaluate_preserves_labels_with_degenerate_caps_on_extrude() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert!(out.is_labeled(), "ExtrudeOp upstream is labeled");
        let labels = out.face_labels.as_ref().expect("labeled");
        assert_eq!(labels.len(), 28, "12 upstream + 16 cylinder");

        // First 12 entries are upstream-face labels (vertex indices
        // changed inside the triangles, but the face IDs themselves
        // are unchanged). The trailing 16 are cylinder DEGENERATE.
        for (i, label) in labels.iter().enumerate().skip(12) {
            assert_eq!(
                *label,
                TopologyFaceId::DEGENERATE,
                "cylinder triangle {i} must be DEGENERATE"
            );
        }
    }

    // -- Profile-size scaling -------------------------------------------------

    /// User guardrail: "Tests should prove ... profile-size scaling."
    /// Same one-cap-perimeter-edge fillet on three profile sizes
    /// (triangle / square / pentagon) — per-edge geometry contribution
    /// is constant (22 verts / 16 tris) regardless of profile shape;
    /// upstream baseline grows linearly with N. Proves that the
    /// RoundFilletUpstream impl is profile-size-agnostic for cap-
    /// perimeter edges.
    #[test]
    fn evaluate_pentagon_profile_scales_with_profile_size() {
        let triangle = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle");

        // (profile, expected upstream vert count = 2N, expected
        // upstream tri count = 4N - 4)
        let cases: Vec<(Polygon2D, usize, usize)> = vec![
            (triangle, 6, 8),           // N=3
            (unit_square(), 8, 12),     // N=4
            (small_pentagon(), 10, 16), // N=5
        ];

        for (profile, upstream_verts, upstream_tris) in cases {
            let extrude = ExtrudeOp::new(profile.clone(), 1.0).expect("ext");
            let upstream = extrude.evaluate(&[]).expect("ext tess");
            assert_eq!(upstream.vertex_count(), upstream_verts);
            assert_eq!(upstream.triangle_count(), upstream_tris);

            let edge = extrude.brep_edge_ids(owner())[0];
            let op =
                RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.05).expect("ok");
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            // +22 verts and +16 tris regardless of profile size.
            assert_eq!(
                out.vertex_count(),
                upstream_verts + 22,
                "profile N={} should add 22 verts per cap-perimeter edge",
                profile.len()
            );
            assert_eq!(
                out.triangle_count(),
                upstream_tris + 16,
                "profile N={} should add 16 tris per cap-perimeter edge",
                profile.len()
            );
        }
    }

    /// Profile-size scaling at the resolver level: number of cap-
    /// perimeter edges = `2 * N`. Verify the supported-edge band
    /// boundary at N=3, 4, 5 by accepting all `2N` cap-perimeter edges
    /// for each profile.
    #[test]
    fn new_for_extrude_accepts_all_cap_perimeter_edges_across_profile_sizes() {
        let triangle = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle");

        for (profile, n) in [
            (triangle, 3usize),
            (unit_square(), 4),
            (small_pentagon(), 5),
        ] {
            let extrude = ExtrudeOp::new(profile, 1.0).expect("ext");
            let all_edges = extrude.brep_edge_ids(owner());
            assert_eq!(all_edges.len(), 3 * n);
            // First 2N edges are cap-perimeter (bottom + top).
            let cap_edges: Vec<_> = all_edges[..2 * n].to_vec();
            let op = RoundFilletOp::new_for_extrude(&extrude, owner(), cap_edges, 0.05)
                .expect("all cap-perimeter");
            assert_eq!(op.edges().len(), 2 * n);
        }
    }

    // -- Helper-table correctness --------------------------------------------

    /// `extrude_bottom_perimeter_vertex_pair` + `_top_perimeter_*` match
    /// the canonical vertex layout of `extrude.rs::evaluate`. Verifies
    /// the duplicate-but-parallel substrate per ADR-119 D5 stays in
    /// sync with the upstream tessellation positionally.
    #[test]
    fn extrude_vertex_pair_helpers_match_extrude_evaluate_layout() {
        // Square (N=4).
        // Bottom: edges 0..4 connect (0,1), (1,2), (2,3), (3,0).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(0, 4), (0, 1));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(1, 4), (1, 2));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(2, 4), (2, 3));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(3, 4), (3, 0));
        // Top: edges 0..4 (local) connect (4,5), (5,6), (6,7), (7,4).
        assert_eq!(extrude_top_perimeter_vertex_pair(0, 4), (4, 5));
        assert_eq!(extrude_top_perimeter_vertex_pair(1, 4), (5, 6));
        assert_eq!(extrude_top_perimeter_vertex_pair(2, 4), (6, 7));
        assert_eq!(extrude_top_perimeter_vertex_pair(3, 4), (7, 4));

        // Triangle (N=3).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(0, 3), (0, 1));
        assert_eq!(extrude_top_perimeter_vertex_pair(2, 3), (5, 3));

        // Pentagon (N=5).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(4, 5), (4, 0));
        assert_eq!(extrude_top_perimeter_vertex_pair(0, 5), (5, 6));
    }

    /// `extrude_side_outward_normal` for the unit square: Side(0)
    /// covers profile edge (0,0)→(1,0), runs along +X; outward normal
    /// is -Y = (0, -1, 0). Side(1) covers (1,0)→(1,1), outward = +X =
    /// (1, 0, 0). Pins the CCW-convention math.
    #[test]
    fn extrude_side_outward_normal_unit_square_directions() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let n0 = extrude_side_outward_normal(0, &extrude);
        let n1 = extrude_side_outward_normal(1, &extrude);
        let n2 = extrude_side_outward_normal(2, &extrude);
        let n3 = extrude_side_outward_normal(3, &extrude);

        let close = |a: [f32; 3], b: [f32; 3]| -> bool {
            (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 && (a[2] - b[2]).abs() < 1e-6
        };
        assert!(
            close(n0, [0.0, -1.0, 0.0]),
            "Side(0) outward = -Y, got {n0:?}"
        );
        assert!(
            close(n1, [1.0, 0.0, 0.0]),
            "Side(1) outward = +X, got {n1:?}"
        );
        assert!(
            close(n2, [0.0, 1.0, 0.0]),
            "Side(2) outward = +Y, got {n2:?}"
        );
        assert!(
            close(n3, [-1.0, 0.0, 0.0]),
            "Side(3) outward = -X, got {n3:?}"
        );
    }

    /// Resolver returns spec with the right face IDs for cap-perimeter
    /// edges. Bottom-perimeter edge i → face_a_id = TopologyFaceId(0)
    /// (Bottom), face_b_id = TopologyFaceId(2 + i) (Side(i)).
    /// Top-perimeter edge (local i) → face_a_id = TopologyFaceId(1)
    /// (Top), face_b_id = TopologyFaceId(2 + i).
    #[test]
    fn resolve_round_spec_face_ids_match_canonical_emission_order() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");

        // Bottom-perimeter edge 0 → Bottom ∩ Side(0).
        let spec = extrude.resolve_round_spec(0).expect("bottom-perimeter 0");
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Bottom-perimeter edge 2 → Bottom ∩ Side(2).
        let spec = extrude.resolve_round_spec(2).expect("bottom-perimeter 2");
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(4));

        // Top-perimeter edge (local 0, canonical 4) → Top ∩ Side(0).
        let spec = extrude.resolve_round_spec(4).expect("top-perimeter 0");
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Top-perimeter edge (local 3, canonical 7) → Top ∩ Side(3).
        let spec = extrude.resolve_round_spec(7).expect("top-perimeter 3");
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(5));

        // Vertical-seam canonical 8 returns Err (rejected).
        let err = extrude.resolve_round_spec(8).unwrap_err();
        assert!(err.contains("vertical-seam") || err.contains("cap-perimeter"));
    }

    /// Resolver returns unit-length, perpendicular inward vectors for
    /// cap-perimeter edges across multiple profile sizes — confirms
    /// the 90° dihedral invariant the sub-α `RoundFilletOp::evaluate`
    /// body assumes geometrically.
    #[test]
    fn resolve_round_spec_inward_vectors_unit_and_perpendicular_for_cap_perimeter() {
        for profile in [unit_square(), small_pentagon()] {
            let extrude = ExtrudeOp::new(profile.clone(), 1.0).expect("ext");
            let n = profile.len();
            // Cap-perimeter canonical indices: 0..2N.
            for idx in 0..2 * n {
                let spec = extrude
                    .resolve_round_spec(idx)
                    .expect("cap-perimeter always resolves");
                let len_a = (spec.face_a_inward[0] * spec.face_a_inward[0]
                    + spec.face_a_inward[1] * spec.face_a_inward[1]
                    + spec.face_a_inward[2] * spec.face_a_inward[2])
                    .sqrt();
                let len_b = (spec.face_b_inward[0] * spec.face_b_inward[0]
                    + spec.face_b_inward[1] * spec.face_b_inward[1]
                    + spec.face_b_inward[2] * spec.face_b_inward[2])
                    .sqrt();
                assert!(
                    (len_a - 1.0).abs() < 1e-6,
                    "face_a_inward at idx {idx} (N={n}) not unit (len={len_a})"
                );
                assert!(
                    (len_b - 1.0).abs() < 1e-6,
                    "face_b_inward at idx {idx} (N={n}) not unit (len={len_b})"
                );

                let dot = spec.face_a_inward[0] * spec.face_b_inward[0]
                    + spec.face_a_inward[1] * spec.face_b_inward[1]
                    + spec.face_a_inward[2] * spec.face_b_inward[2];
                assert!(
                    dot.abs() < 1e-6,
                    "inward vectors at cap-perimeter idx {idx} (N={n}) not perpendicular (dot={dot})"
                );
            }
        }
    }
}
