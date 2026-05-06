//! `ExtrudeOp` — sweep a 2D convex polygon along +Z to produce a closed solid
//! (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! # Geometry
//!
//! [`ExtrudeOp`] consumes a [`Polygon2D`] in the XY plane and a positive,
//! finite `length`, producing a closed prism of `2 * n` vertices and
//! `4 * n - 4` triangles, where `n` is the profile point count.
//!
//! * The bottom ring sits at `z = 0`; the top ring at `z = length`.
//! * End caps are fan-triangulated from vertex 0 of each ring.
//! * Side walls are quad strips between the two rings, each split into two
//!   triangles.
//!
//! # Conventions
//!
//! * **Right-handed CCW winding** when viewed from outside the solid.
//! * **Outward normals** — the bottom face normal points in `-Z`, the top in
//!   `+Z`, and side-wall normals point away from the polygon interior.
//! * **Profile winding is winding-agnostic from the caller's perspective**:
//!   the algorithm reads the signed area and reverses iteration order
//!   internally if the caller supplied a CW polygon, so the produced solid
//!   always has correct outward normals.
//!
//! # Restrictions (Phase 7 D-Extrude)
//!
//! * Profile must be **strictly convex** (validated at `evaluate` time via
//!   [`Polygon2D::convexity`]). Concave profiles are rejected with
//!   [`OpError::InvalidParameter`]. A future dispatch with an earcut /
//!   ear-clipping triangulator will lift this restriction.
//! * Extrusion direction is fixed to `+Z`. Arbitrary-axis extrusion is
//!   achieved by chaining a downstream [`crate::TransformOp`].
//! * No taper / draft angle.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;

// ---------------------------------------------------------------------------
// Polygon2DError
// ---------------------------------------------------------------------------

/// Errors produced by [`Polygon2D::new`] for malformed input.
///
/// These are construction-time errors. Convexity / extrusion-domain errors
/// surface from [`ExtrudeOp::evaluate`] as [`OpError::InvalidParameter`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Polygon2DError {
    /// Fewer than the minimum 3 distinct points were supplied.
    #[error("polygon needs >= 3 points (got {got})")]
    TooFewPoints {
        /// The deficient point count.
        got: usize,
    },
    /// A coordinate was NaN or infinite.
    #[error("polygon contains non-finite coordinate at index {index}")]
    NonFiniteCoordinate {
        /// Position of the offending point in the input slice.
        index: usize,
    },
    /// Two adjacent points coincide (zero-length edge).
    #[error("polygon has zero-area / coincident points at index {index}")]
    DegenerateEdge {
        /// Position of the second point of the offending edge.
        index: usize,
    },
}

// ---------------------------------------------------------------------------
// Polygon2D
// ---------------------------------------------------------------------------

/// Closed 2D polygon profile in the XY plane.
///
/// The closing edge from `points.last()` back to `points.first()` is implicit
/// — callers must NOT repeat the first point at the end.
///
/// Construction enforces:
///
/// * `points.len() >= 3`
/// * every coordinate is finite
/// * no two adjacent points coincide (no zero-length edges, including the
///   implicit closing edge)
///
/// Convexity is *not* enforced at construction time so a polygon can be
/// built up incrementally before being attached to an [`ExtrudeOp`]. The
/// extrude operator validates convexity at `evaluate` time via
/// [`Polygon2D::convexity`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polygon2D {
    points: Vec<[f32; 2]>,
}

impl Polygon2D {
    /// Build a [`Polygon2D`] after validating point count, finiteness, and
    /// adjacent-point distinctness.
    ///
    /// # Errors
    ///
    /// * [`Polygon2DError::TooFewPoints`] if `points.len() < 3`.
    /// * [`Polygon2DError::NonFiniteCoordinate`] if any coordinate is NaN /
    ///   infinite.
    /// * [`Polygon2DError::DegenerateEdge`] if two adjacent points coincide
    ///   (including the implicit closing edge between last and first).
    pub fn new(points: Vec<[f32; 2]>) -> Result<Self, Polygon2DError> {
        if points.len() < 3 {
            return Err(Polygon2DError::TooFewPoints { got: points.len() });
        }
        for (i, [x, y]) in points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(Polygon2DError::NonFiniteCoordinate { index: i });
            }
        }
        for i in 0..points.len() {
            let next = (i + 1) % points.len();
            // Bit-identical compare via to_bits — array `==` would also work
            // but trips clippy::float_cmp. We genuinely want exact equality
            // here (caller passed the same coordinate twice ⇒ zero-length
            // edge); float-tolerance comparisons are not appropriate.
            if points[i][0].to_bits() == points[next][0].to_bits()
                && points[i][1].to_bits() == points[next][1].to_bits()
            {
                return Err(Polygon2DError::DegenerateEdge { index: next });
            }
        }
        Ok(Self { points })
    }

    /// Borrow the underlying point slice.
    ///
    /// Conventional winding is counter-clockwise, but this is *not* enforced
    /// — [`ExtrudeOp::evaluate`] reads the signed area and corrects the
    /// iteration order internally if the caller supplied a CW polygon.
    #[must_use]
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    /// Number of points in the polygon.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Always `false` — [`Polygon2D::new`] guarantees `points.len() >= 3`.
    /// Provided for clippy-len-zero clarity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Classify polygon convexity by inspecting the signs of the cross
    /// products of consecutive edge pairs.
    ///
    /// * `Some(true)`  — strictly convex (all cross-products non-zero and
    ///   share the same sign).
    /// * `Some(false)` — concave (cross-products have mixed signs).
    /// * `None`        — degenerate (all cross-products zero, i.e. all points
    ///   are collinear).
    pub(crate) fn convexity(&self) -> Option<bool> {
        let n = self.points.len();
        let mut sign: i8 = 0; // 0 = unset, +1 = positive, -1 = negative
        for i in 0..n {
            let p0 = self.points[i];
            let p1 = self.points[(i + 1) % n];
            let p2 = self.points[(i + 2) % n];
            let dx1 = p1[0] - p0[0];
            let dy1 = p1[1] - p0[1];
            let dx2 = p2[0] - p1[0];
            let dy2 = p2[1] - p1[1];
            let cross = dx1 * dy2 - dy1 * dx2;
            if cross > 0.0 {
                if sign == -1 {
                    return Some(false);
                }
                sign = 1;
            } else if cross < 0.0 {
                if sign == 1 {
                    return Some(false);
                }
                sign = -1;
            }
            // cross == 0.0 → collinear edge pair; keep scanning.
        }
        if sign == 0 {
            None // every edge pair was collinear
        } else {
            Some(true)
        }
    }

    /// Signed 2D area via the shoelace formula. `> 0` for CCW winding,
    /// `< 0` for CW, `== 0` for degenerate (zero-area / collinear) polygons.
    pub(crate) fn signed_area(&self) -> f32 {
        let n = self.points.len();
        let mut sum = 0.0_f32;
        for i in 0..n {
            let [x0, y0] = self.points[i];
            let [x1, y1] = self.points[(i + 1) % n];
            sum += x0 * y1 - x1 * y0;
        }
        sum * 0.5
    }
}

// ---------------------------------------------------------------------------
// ExtrudeOp
// ---------------------------------------------------------------------------

/// Sweep a [`Polygon2D`] profile along `+Z` to produce a closed solid.
///
/// `length` must be finite and strictly positive. Profile validity is
/// re-checked at [`ExtrudeOp::evaluate`] time so that intermediate graph
/// states (where a profile may be momentarily degenerate while being edited)
/// don't poison construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtrudeOp {
    /// 2D profile swept along the extrusion direction.
    pub profile: Polygon2D,
    /// Sweep distance along `+Z`. Must be finite and `> 0.0`.
    pub length: f32,
}

impl ExtrudeOp {
    /// Build an [`ExtrudeOp`] after validating `length`.
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `length` is not finite or not
    ///   strictly positive.
    pub fn new(profile: Polygon2D, length: f32) -> Result<Self, OpError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(OpError::InvalidParameter(format!(
                "ExtrudeOp.length must be finite and > 0 (got {length})"
            )));
        }
        Ok(Self { profile, length })
    }
}

impl Operator for ExtrudeOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Extrude
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"extrude:");
        hasher.update(&self.length.to_le_bytes());
        // try_from is infallible at any plausible profile size, but using it
        // satisfies clippy::cast_possible_truncation. Fall back to u32::MAX
        // for the unreachable >4G-point case (Tessellation::new would have
        // rejected long before).
        let profile_len = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        hasher.update(&profile_len.to_le_bytes());
        for [x, y] in &self.profile.points {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if !inputs.is_empty() {
            return Err(OpError::WrongArity {
                expected: 0,
                got: inputs.len(),
            });
        }

        // Re-validate length defensively (the field is `pub` and may have
        // been mutated post-construction).
        if !self.length.is_finite() || self.length <= 0.0 {
            return Err(OpError::InvalidParameter(format!(
                "extrude length must be finite > 0 (got {})",
                self.length
            )));
        }

        // Re-validate profile invariants (defensive: `profile.points` was
        // private but `profile` is a pub field on `ExtrudeOp`, so the caller
        // could have swapped in a fresh Polygon2D — that path is also
        // construction-checked, but a future change might add unchecked
        // mutation hooks).
        if self.profile.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "extrude profile needs >= 3 points (got {})",
                self.profile.len()
            )));
        }
        for (i, [x, y]) in self.profile.points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "extrude profile has non-finite coordinate at index {i}"
                )));
            }
        }

        // Convexity gate.
        match self.profile.convexity() {
            Some(true) => {}
            Some(false) => {
                return Err(OpError::InvalidParameter(
                    "extrude profile must be strictly convex".to_string(),
                ));
            }
            None => {
                return Err(OpError::InvalidParameter(
                    "extrude profile is degenerate (all points collinear)".to_string(),
                ));
            }
        }

        // Winding correction: signed_area > 0 → CCW (already canonical);
        // signed_area < 0 → CW (reverse the iteration order); near-zero → reject.
        // Epsilon comparison rather than exact == 0.0 to defend against tiny
        // float-drift in the shoelace sum that would otherwise sneak through.
        let signed_area = self.profile.signed_area();
        if signed_area.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "extrude profile is degenerate (near-zero area)".to_string(),
            ));
        }
        let n = self.profile.len();
        let ordered: Vec<[f32; 2]> = if signed_area > 0.0 {
            self.profile.points.clone()
        } else {
            self.profile.points.iter().rev().copied().collect()
        };

        // Build vertex buffer: bottom ring (z=0) then top ring (z=length).
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(2 * n);
        for [x, y] in &ordered {
            positions.push([*x, *y, 0.0]);
        }
        for [x, y] in &ordered {
            positions.push([*x, *y, self.length]);
        }

        let n_u32 = u32::try_from(n).map_err(|_| {
            OpError::InvalidParameter(format!("extrude profile too large: {n} points"))
        })?;

        // Index buffer:
        //   caps  : 2 * (n - 2) triangles
        //   sides : 2 * n triangles
        //   total : 4n - 4
        let mut indices: Vec<u32> = Vec::with_capacity(3 * (4 * n - 4));

        // Bottom cap — outward normal -Z. The ordered ring is CCW when
        // viewed from +Z (signed_area > 0). For a -Z-facing triangle we want
        // CCW winding when viewed from -Z, i.e. the indices listed in 3D are
        // (0, i+1, i) — the reverse of the projected CCW ordering.
        for i in 1..(n_u32 - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i);
        }

        // Top cap — outward normal +Z. The ordered ring is CCW from +Z, so
        // (n, n+i, n+i+1) is CCW when viewed from +Z = correct outward
        // facing.
        for i in 1..(n_u32 - 1) {
            indices.push(n_u32);
            indices.push(n_u32 + i);
            indices.push(n_u32 + i + 1);
        }

        // Side walls. For each polygon edge (i, i+1), generate the quad
        // (bottom_i, bottom_{i+1}, top_{i+1}, top_i). With CCW polygon
        // ordering the outward normal of each side face points away from
        // the polygon interior.
        for i in 0..n_u32 {
            let i1 = (i + 1) % n_u32;
            let bot_i = i;
            let bot_i1 = i1;
            let top_i = n_u32 + i;
            let top_i1 = n_u32 + i1;

            // Triangle 1: (bot_i, bot_{i+1}, top_{i+1})
            indices.push(bot_i);
            indices.push(bot_i1);
            indices.push(top_i1);
            // Triangle 2: (bot_i, top_{i+1}, top_i)
            indices.push(bot_i);
            indices.push(top_i1);
            indices.push(top_i);
        }

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("ExtrudeOp produced invalid tessellation: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ccw_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("ccw unit square")
    }

    fn cw_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]])
            .expect("cw unit square")
    }

    fn ccw_triangle() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle")
    }

    fn ccw_pentagon() -> Polygon2D {
        Polygon2D::new(vec![
            [1.0, 0.0],
            [0.309, 0.951],
            [-0.809, 0.588],
            [-0.809, -0.588],
            [0.309, -0.951],
        ])
        .expect("regular pentagon")
    }

    /// L-shape (concave) polygon — 6 corners, one inward bend.
    ///
    /// ```text
    ///  (0,2)----(1,2)
    ///    |        |
    ///    |        |
    ///    |        |
    ///    |        +---- (2,1)
    ///    |              |
    ///    |              |
    ///  (0,0)--------(2,0)
    /// ```
    fn concave_l_shape() -> Polygon2D {
        Polygon2D::new(vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ])
        .expect("concave L-shape")
    }

    // -- Polygon2D constructor ------------------------------------------------

    #[test]
    fn polygon2d_rejects_too_few_points() {
        let err = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0]]).unwrap_err();
        assert_eq!(err, Polygon2DError::TooFewPoints { got: 2 });
    }

    #[test]
    fn polygon2d_rejects_non_finite() {
        let err = Polygon2D::new(vec![[0.0, 0.0], [f32::NAN, 0.0], [1.0, 1.0]]).unwrap_err();
        assert_eq!(err, Polygon2DError::NonFiniteCoordinate { index: 1 });
        let err = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [f32::INFINITY, 1.0]]).unwrap_err();
        assert_eq!(err, Polygon2DError::NonFiniteCoordinate { index: 2 });
    }

    #[test]
    fn polygon2d_rejects_coincident_adjacent_points() {
        let err = Polygon2D::new(vec![[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]).unwrap_err();
        assert_eq!(err, Polygon2DError::DegenerateEdge { index: 1 });
    }

    #[test]
    fn polygon2d_rejects_coincident_closing_edge() {
        // Last point identical to first ⇒ implicit closing edge is zero-length.
        let err = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]]).unwrap_err();
        assert_eq!(err, Polygon2DError::DegenerateEdge { index: 0 });
    }

    // -- signed_area / convexity ---------------------------------------------

    #[test]
    fn polygon2d_signed_area_positive_for_ccw() {
        let p = ccw_square();
        assert!(p.signed_area() > 0.0, "got {}", p.signed_area());
        assert!((p.signed_area() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn polygon2d_signed_area_negative_for_cw() {
        let p = cw_square();
        assert!(p.signed_area() < 0.0, "got {}", p.signed_area());
        assert!((p.signed_area() + 1.0).abs() < 1e-6);
    }

    #[test]
    fn polygon2d_convexity_detects_convex_square() {
        assert_eq!(ccw_square().convexity(), Some(true));
        assert_eq!(cw_square().convexity(), Some(true));
    }

    #[test]
    fn polygon2d_convexity_detects_convex_pentagon() {
        assert_eq!(ccw_pentagon().convexity(), Some(true));
    }

    #[test]
    fn polygon2d_convexity_detects_concave_l_shape() {
        assert_eq!(concave_l_shape().convexity(), Some(false));
    }

    #[test]
    fn polygon2d_len_and_is_empty() {
        let p = ccw_pentagon();
        assert_eq!(p.len(), 5);
        // is_empty is always false for a valid Polygon2D (>= 3 points).
        assert!(!p.is_empty());
        assert_eq!(p.points().len(), 5);
    }

    // -- ExtrudeOp::new ------------------------------------------------------

    #[test]
    fn extrude_op_new_rejects_zero_length() {
        let err = ExtrudeOp::new(ccw_square(), 0.0).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn extrude_op_new_rejects_negative_length() {
        let err = ExtrudeOp::new(ccw_square(), -1.0).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn extrude_op_new_rejects_non_finite_length() {
        let err = ExtrudeOp::new(ccw_square(), f32::NAN).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
        let err = ExtrudeOp::new(ccw_square(), f32::INFINITY).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    // -- evaluate vertex / triangle counts -----------------------------------

    #[test]
    fn extrude_triangle_profile_yields_6_vertices_8_triangles() {
        let op = ExtrudeOp::new(ccw_triangle(), 1.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // n=3 ⇒ 2n=6 vertices, 4n-4=8 triangles, 24 indices.
        assert_eq!(mesh.vertex_count(), 6);
        assert_eq!(mesh.triangle_count(), 8);
        assert_eq!(mesh.indices.len(), 24);
    }

    #[test]
    fn extrude_square_profile_yields_8_vertices_12_triangles() {
        let op = ExtrudeOp::new(ccw_square(), 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
        // Bottom ring (positions 0..4) at z=0; top ring (4..8) at z=2.
        for v in &mesh.positions[..4] {
            assert!(v[2].abs() < f32::EPSILON, "bottom z ≠ 0: {v:?}");
        }
        for v in &mesh.positions[4..8] {
            assert!((v[2] - 2.0).abs() < f32::EPSILON, "top z ≠ 2: {v:?}");
        }
    }

    #[test]
    fn extrude_pentagon_profile_yields_10_vertices_16_triangles() {
        let op = ExtrudeOp::new(ccw_pentagon(), 0.5).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // n=5 ⇒ 2n=10 vertices, 4n-4=16 triangles, 48 indices.
        assert_eq!(mesh.vertex_count(), 10);
        assert_eq!(mesh.triangle_count(), 16);
        assert_eq!(mesh.indices.len(), 48);
    }

    // -- evaluate rejection paths --------------------------------------------

    #[test]
    fn extrude_rejects_inputs_for_arity_0() {
        let op = ExtrudeOp::new(ccw_square(), 1.0).expect("op");
        let bogus = Tessellation::new(vec![[0.0_f32, 0.0, 0.0]], vec![]).expect("ok");
        let err = op.evaluate(&[&bogus]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 0,
                got: 1
            }
        ));
    }

    #[test]
    fn extrude_concave_profile_rejected_at_evaluate() {
        let op = ExtrudeOp::new(concave_l_shape(), 1.0).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn extrude_cw_profile_yields_correct_vertex_count() {
        // Same square footprint, but listed in CW order. Algorithm should
        // detect the negative signed area and reverse iteration order so
        // the produced solid still has the expected vertex/triangle counts.
        let op = ExtrudeOp::new(cw_square(), 1.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn extrude_post_construction_length_corruption_rejected() {
        // `length` is a pub field — a caller can flip it to bogus values
        // after construction. evaluate() must defensively re-check.
        let mut op = ExtrudeOp::new(ccw_square(), 1.0).expect("op");
        op.length = -1.0;
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    // -- structural_hash -----------------------------------------------------

    #[test]
    fn extrude_structural_hash_deterministic() {
        let a = ExtrudeOp::new(ccw_square(), 1.5).expect("a");
        let b = ExtrudeOp::new(ccw_square(), 1.5).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn extrude_structural_hash_changes_with_length() {
        let a = ExtrudeOp::new(ccw_square(), 1.5).expect("a");
        let b = ExtrudeOp::new(ccw_square(), 1.6).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn extrude_structural_hash_changes_with_profile_point_perturbation() {
        let a = ExtrudeOp::new(ccw_square(), 1.0).expect("a");
        let perturbed = Polygon2D::new(vec![[0.0, 0.0], [1.0 + 1e-3, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("perturbed");
        let b = ExtrudeOp::new(perturbed, 1.0).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn extrude_op_kind_is_extrude() {
        let op = ExtrudeOp::new(ccw_square(), 1.0).expect("op");
        assert_eq!(op.op_kind(), OpKind::Extrude);
        assert_eq!(op.arity(), 0);
    }
}
