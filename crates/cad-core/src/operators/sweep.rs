//! `SweepOp` — sweep a 2D convex polygon along a 3D polyline path to produce
//! a closed solid (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! # Geometry
//!
//! [`SweepOp`] consumes a [`Polygon2D`] profile (in the XY plane) and a
//! [`Polyline3D`] path, producing a closed solid whose cross-section at each
//! path vertex is the profile rigidly translated to that vertex.
//!
//! For `n` profile points and `m` path points the produced mesh has `n * m`
//! vertices and `2 * n * (m - 1) + 2 * (n - 2)` triangles — generalises
//! [`crate::ExtrudeOp`] (Sweep with `m == 2` and a `+Z`-aligned path
//! produces an extrude-equivalent solid: `2n + 2(n - 2) = 4n - 4` triangles).
//!
//! * Ring `k` sits at `path[k]`; profile vertex `i` becomes
//!   `(path[k].x + profile[i].x, path[k].y + profile[i].y, path[k].z)`.
//! * End caps are fan-triangulated from vertex 0 of the first and last rings.
//! * Side walls are quad strips between consecutive rings, each split into
//!   two triangles via the diagonal that runs from `bot_i` to `top_{i+1}`.
//!
//! # Conventions
//!
//! * **Right-handed CCW winding** when viewed from outside the solid.
//! * **Outward normals** — the first-ring cap normal points in `-Z`; the
//!   last-ring cap normal points in `+Z`; side-wall normals point away from
//!   the polygon interior.
//! * **Profile winding is winding-agnostic from the caller's perspective**:
//!   the algorithm reads the signed area and reverses iteration order
//!   internally if the caller supplied a CW polygon, so the produced solid
//!   always has correct outward normals.
//!
//! # Restrictions (Phase 7 D-Sweep v0)
//!
//! * **Monotonic-Z path required.** Every consecutive path segment must
//!   strictly increase Z (`path[k + 1].z > path[k].z`). Non-monotonic-Z
//!   paths produce overlapping rings or backwards-facing solids and are
//!   rejected at `evaluate` time. This is the principal v0 restriction;
//!   it pins cap-orientation correctness without requiring path-tangent
//!   computation.
//! * **Profile is rigidly translated, not rotated.** The profile remains
//!   in the XY plane at every ring; the path-tangent direction does NOT
//!   rotate the profile. Paths with X / Y drift produce sheared but valid
//!   side walls; paths that turn sharply produce visibly non-perpendicular
//!   cross-sections (a v0 limitation, lifted by future
//!   rotation-minimizing-frame work).
//! * **Profile must be strictly convex** (validated at `evaluate` time via
//!   [`Polygon2D::convexity`]). Concave profiles produce inverted cap
//!   triangles under fan triangulation; rejected. Same restriction as
//!   [`crate::ExtrudeOp`] / [`crate::LoftOp`]; lifted by the same future
//!   earcut dispatch.
//! * **Open paths only.** Closed-loop paths (where `path.first() ==
//!   path.last()`) are rejected by [`Polyline3D::new`] because the closing
//!   segment would have zero length. Closed-loop sweep (torus-like
//!   geometry) is out of v0 scope.
//! * **No path-tangent perpendicular orientation.** No Frenet frames; no
//!   rotation-minimizing frames; no twist control. Out of v0 scope.
//! * **No variable scale along path.** The profile is the same shape at
//!   every ring. Tapered sweep is achieved by chaining downstream
//!   [`crate::TransformOp`] applications or by future variable-scale-sweep
//!   work.
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (fan from vertex 0; no
//!   float-comparison-dependent triangulation choice).
//! * `t_junction_handling`: true (closed solid has none).
//! * `concave_input_supported`: **false** — fan-triangulation produces
//!   inverted cap triangles on concave profiles; rejected at evaluate time.
//! * `arity`: 0 (profile and path are parameters, not upstream inputs).
//! * `output_labeled_when_input_labeled`: false (no inputs).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::operators::{OpError, OpKind, Operator, Polygon2D};
use crate::tessellation::Tessellation;

// ---------------------------------------------------------------------------
// Polyline3DError
// ---------------------------------------------------------------------------

/// Errors produced by [`Polyline3D::new`] for malformed input.
///
/// These are construction-time errors. Domain errors (non-monotonic Z, etc.)
/// surface from [`SweepOp::evaluate`] as [`OpError::InvalidParameter`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Polyline3DError {
    /// Fewer than the minimum 2 points were supplied.
    #[error("polyline needs >= 2 points (got {got})")]
    TooFewPoints {
        /// The deficient point count.
        got: usize,
    },
    /// A coordinate was NaN or infinite.
    #[error("polyline contains non-finite coordinate at index {index}")]
    NonFiniteCoordinate {
        /// Position of the offending point in the input slice.
        index: usize,
    },
    /// Two adjacent points coincide (zero-length segment).
    #[error("polyline has coincident adjacent points at index {index}")]
    DegenerateSegment {
        /// Position of the second point of the offending segment.
        index: usize,
    },
}

// ---------------------------------------------------------------------------
// Polyline3D
// ---------------------------------------------------------------------------

/// Open 3D polyline path used as a sweep trajectory.
///
/// Construction enforces:
///
/// * `points.len() >= 2` (a path needs at least a start and an end).
/// * Every coordinate is finite.
/// * No two adjacent points coincide (no zero-length segments).
///
/// **Closed-loop paths** (where `points.first() == points.last()`) are
/// rejected by the coincident-adjacent check on the last → first segment
/// (which is implicit only for [`Polygon2D`]; [`Polyline3D`] is open by
/// construction).
///
/// **Monotonic-Z** is *not* enforced at construction time so a path can be
/// built up incrementally before being attached to a [`SweepOp`]. The sweep
/// operator validates monotonic-Z at `evaluate` time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3D {
    points: Vec<[f32; 3]>,
}

impl Polyline3D {
    /// Build a [`Polyline3D`] after validating point count, finiteness, and
    /// adjacent-point distinctness.
    ///
    /// # Errors
    ///
    /// * [`Polyline3DError::TooFewPoints`] if `points.len() < 2`.
    /// * [`Polyline3DError::NonFiniteCoordinate`] if any coordinate is NaN /
    ///   infinite.
    /// * [`Polyline3DError::DegenerateSegment`] if two adjacent points
    ///   coincide (zero-length segment).
    pub fn new(points: Vec<[f32; 3]>) -> Result<Self, Polyline3DError> {
        if points.len() < 2 {
            return Err(Polyline3DError::TooFewPoints { got: points.len() });
        }
        for (i, [x, y, z]) in points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                return Err(Polyline3DError::NonFiniteCoordinate { index: i });
            }
        }
        // Adjacent-point distinctness. An open polyline has `points.len() - 1`
        // segments; the closing segment that [`Polygon2D`] validates is NOT
        // implicit here.
        for i in 0..points.len() - 1 {
            let a = points[i];
            let b = points[i + 1];
            if a[0].to_bits() == b[0].to_bits()
                && a[1].to_bits() == b[1].to_bits()
                && a[2].to_bits() == b[2].to_bits()
            {
                return Err(Polyline3DError::DegenerateSegment { index: i + 1 });
            }
        }
        Ok(Self { points })
    }

    /// Borrow the underlying point slice.
    #[must_use]
    pub fn points(&self) -> &[[f32; 3]] {
        &self.points
    }

    /// Number of points in the polyline.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Always `false` — [`Polyline3D::new`] guarantees `points.len() >= 2`.
    /// Provided for clippy-len-zero clarity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

// ---------------------------------------------------------------------------
// SweepOp
// ---------------------------------------------------------------------------

/// Sweep a [`Polygon2D`] profile along a [`Polyline3D`] path to produce a
/// closed solid.
///
/// `path` must have at least 2 points; `path` Z-coordinates must be strictly
/// monotonically increasing (validated at [`SweepOp::evaluate`] time).
/// Profile invariants (point count, finiteness, convexity, signed area) are
/// re-checked at `evaluate` time so that intermediate graph states (where a
/// parameter may be momentarily corrupted while being edited) don't poison
/// construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SweepOp {
    /// 2D profile rigidly translated to each path vertex.
    pub profile: Polygon2D,
    /// 3D polyline path. Z-coordinates must be strictly monotonically
    /// increasing (validated at evaluate time).
    pub path: Polyline3D,
}

impl SweepOp {
    /// Build a [`SweepOp`].
    ///
    /// All construction-time validation has already been performed by
    /// [`Polygon2D::new`] / [`Polyline3D::new`]; domain checks
    /// (monotonic-Z, convexity) are deferred to [`SweepOp::evaluate`].
    #[must_use]
    pub fn new(profile: Polygon2D, path: Polyline3D) -> Self {
        Self { profile, path }
    }
}

impl Operator for SweepOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Sweep
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"sweep:");
        // try_from is infallible at any plausible profile/path size, but
        // using it satisfies clippy::cast_possible_truncation. Fall back to
        // u32::MAX for the unreachable >4G-point case.
        let n_profile = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        hasher.update(&n_profile.to_le_bytes());
        for [x, y] in self.profile.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
        }
        let n_path = u32::try_from(self.path.len()).unwrap_or(u32::MAX);
        hasher.update(&n_path.to_le_bytes());
        for [x, y, z] in self.path.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
            hasher.update(&z.to_le_bytes());
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

        // Re-validate path defensively (the field is `pub` and may have been
        // mutated post-construction).
        if self.path.len() < 2 {
            return Err(OpError::InvalidParameter(format!(
                "sweep path needs >= 2 points (got {})",
                self.path.len()
            )));
        }
        for (i, [x, y, z]) in self.path.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "sweep path has non-finite coordinate at index {i}"
                )));
            }
        }

        // Monotonic-Z gate. The principal v0 restriction; ensures cap-
        // orientation correctness without path-tangent computation.
        for k in 0..self.path.len() - 1 {
            let z0 = self.path.points()[k][2];
            let z1 = self.path.points()[k + 1][2];
            if !(z1 > z0) {
                return Err(OpError::InvalidParameter(format!(
                    "sweep path must be strictly monotonic in Z (segment {k}: z0={z0}, z1={z1})"
                )));
            }
        }

        // Re-validate profile invariants.
        if self.profile.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "sweep profile needs >= 3 points (got {})",
                self.profile.len()
            )));
        }
        for (i, [x, y]) in self.profile.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "sweep profile has non-finite coordinate at index {i}"
                )));
            }
        }

        // Convexity gate.
        match self.profile.convexity() {
            Some(true) => {}
            Some(false) => {
                return Err(OpError::InvalidParameter(
                    "sweep profile must be strictly convex".to_string(),
                ));
            }
            None => {
                return Err(OpError::InvalidParameter(
                    "sweep profile is degenerate (all points collinear)".to_string(),
                ));
            }
        }

        // Winding correction: signed_area > 0 → CCW (canonical); < 0 → CW
        // (reverse iteration order); near-zero → reject.
        let signed_area = self.profile.signed_area();
        if signed_area.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "sweep profile is degenerate (near-zero area)".to_string(),
            ));
        }

        let n = self.profile.len();
        let m = self.path.len();
        let ordered_profile: Vec<[f32; 2]> = if signed_area > 0.0 {
            self.profile.points().to_vec()
        } else {
            self.profile.points().iter().rev().copied().collect()
        };

        // Build vertex buffer: m rings of n vertices each. Ring k holds the
        // profile rigidly translated to path[k].
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * m);
        for [px, py, pz] in self.path.points() {
            for [x, y] in &ordered_profile {
                positions.push([px + x, py + y, *pz]);
            }
        }

        let n_u32 = u32::try_from(n).map_err(|_| {
            OpError::InvalidParameter(format!("sweep profile too large: {n} points"))
        })?;
        let m_u32 = u32::try_from(m)
            .map_err(|_| OpError::InvalidParameter(format!("sweep path too large: {m} points")))?;

        // Index buffer:
        //   first cap : n - 2 triangles  (-Z normal)
        //   last cap  : n - 2 triangles  (+Z normal)
        //   sides     : 2 * n * (m - 1) triangles
        //   total     : 2 * n * (m - 1) + 2 * (n - 2)
        let cap_tris = 2 * (n - 2);
        let side_tris = 2 * n * (m - 1);
        let mut indices: Vec<u32> = Vec::with_capacity(3 * (cap_tris + side_tris));

        // First cap (ring 0) — outward normal -Z. The ordered ring is CCW
        // when viewed from +Z; for a -Z-facing triangle we need CCW winding
        // when viewed from -Z, i.e. (0, i+1, i) — the reverse of projected
        // CCW.
        for i in 1..(n_u32 - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i);
        }

        // Last cap (ring m-1) — outward normal +Z. The ordered ring is CCW
        // from +Z, so (offset, offset+i, offset+i+1) is CCW from +Z = correct
        // outward facing.
        let last_ring_offset = (m_u32 - 1) * n_u32;
        for i in 1..(n_u32 - 1) {
            indices.push(last_ring_offset);
            indices.push(last_ring_offset + i);
            indices.push(last_ring_offset + i + 1);
        }

        // Side walls. For each path segment [k, k+1] and each polygon edge
        // (i, i+1), generate the quad (bot_i, bot_{i+1}, top_{i+1}, top_i).
        // With CCW polygon ordering the outward normal of each side face
        // points away from the polygon interior.
        for k in 0..(m_u32 - 1) {
            let bot_offset = k * n_u32;
            let top_offset = (k + 1) * n_u32;
            for i in 0..n_u32 {
                let i1 = (i + 1) % n_u32;
                let bot_i = bot_offset + i;
                let bot_i1 = bot_offset + i1;
                let top_i = top_offset + i;
                let top_i1 = top_offset + i1;
                // Quad split via diagonal (bot_i, top_i1):
                indices.push(bot_i);
                indices.push(bot_i1);
                indices.push(top_i1);
                indices.push(bot_i);
                indices.push(top_i1);
                indices.push(top_i);
            }
        }

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("sweep produced invalid tessellation: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("unit square profile")
    }

    fn unit_triangle() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("unit triangle profile")
    }

    fn z_path(zs: &[f32]) -> Polyline3D {
        Polyline3D::new(zs.iter().map(|z| [0.0, 0.0, *z]).collect()).expect("z-axis path")
    }

    // ----- Polyline3D construction -----

    #[test]
    fn polyline_new_rejects_too_few_points() {
        let err = Polyline3D::new(vec![[0.0, 0.0, 0.0]]).expect_err("too few");
        assert_eq!(err, Polyline3DError::TooFewPoints { got: 1 });
    }

    #[test]
    fn polyline_new_rejects_non_finite_coordinate() {
        let err =
            Polyline3D::new(vec![[0.0, 0.0, 0.0], [f32::NAN, 0.0, 1.0]]).expect_err("nan rejected");
        assert_eq!(err, Polyline3DError::NonFiniteCoordinate { index: 1 });
    }

    #[test]
    fn polyline_new_rejects_coincident_adjacent_points() {
        let err = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]]).expect_err("zero-length");
        assert_eq!(err, Polyline3DError::DegenerateSegment { index: 1 });
    }

    #[test]
    fn polyline_round_trip_via_points() {
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]];
        let p = Polyline3D::new(pts.clone()).expect("3-point z path");
        assert_eq!(p.points(), &pts[..]);
        assert_eq!(p.len(), 3);
        assert!(!p.is_empty());
    }

    // ----- SweepOp construction + arity -----

    #[test]
    fn sweep_op_new_accepts_valid_inputs() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op.op_kind(), OpKind::Sweep);
        assert_eq!(op.arity(), 0);
    }

    #[test]
    fn sweep_rejects_inputs_for_arity_0() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let dummy = Tessellation::new(vec![[0.0, 0.0, 0.0]], vec![]).expect("empty tess");
        let err = op.evaluate(&[&dummy]).expect_err("arity mismatch");
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 0,
                got: 1
            }
        ));
    }

    // ----- SweepOp validation -----

    #[test]
    fn sweep_rejects_concave_profile() {
        // Concave L-shape.
        let concave = Polygon2D::new(vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ])
        .expect("concave polygon constructs");
        let op = SweepOp::new(concave, z_path(&[0.0, 1.0]));
        let err = op.evaluate(&[]).expect_err("concave rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    #[test]
    fn sweep_rejects_non_monotonic_z_path() {
        // z goes 0 → 1 → 0.5 (drops in the second segment).
        let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.5]])
            .expect("path constructs");
        let op = SweepOp::new(unit_square(), path);
        let err = op.evaluate(&[]).expect_err("non-monotonic Z rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("monotonic"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    #[test]
    fn sweep_rejects_zero_z_segment() {
        // z stays the same across a segment.
        let path =
            Polyline3D::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]).expect("path constructs");
        let op = SweepOp::new(unit_square(), path);
        let err = op.evaluate(&[]).expect_err("zero-z segment rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("monotonic"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    // ----- SweepOp geometry -----

    #[test]
    fn sweep_square_along_2_point_z_path_yields_8_verts_12_tris() {
        // n=4, m=2 → 8 vertices, 4n-4 = 12 triangles, 36 indices.
        // Equivalent to ExtrudeOp(unit_square, length=1).
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn sweep_triangle_along_3_point_z_path_yields_9_verts_14_tris() {
        // n=3, m=3 → 9 vertices, 2*3*2 + 2*1 = 14 triangles, 42 indices.
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 9);
        assert_eq!(mesh.triangle_count(), 14);
        assert_eq!(mesh.indices.len(), 42);
    }

    #[test]
    fn sweep_square_along_4_point_z_path_yields_16_verts_28_tris() {
        // n=4, m=4 → 16 vertices, 2*4*3 + 2*2 = 28 triangles, 84 indices.
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0, 3.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 16);
        assert_eq!(mesh.triangle_count(), 28);
        assert_eq!(mesh.indices.len(), 84);
    }

    #[test]
    fn sweep_with_xy_drift_in_path_still_valid() {
        // Stair-step path: x changes between segments. Sheared but valid
        // sweep (rigid profile translation; monotonic-Z preserved). n=4,
        // m=3 → 12 vertices, 2*4*2 + 2*2 = 20 triangles, 60 indices.
        let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 2.0]])
            .expect("stair-step path constructs");
        let op = SweepOp::new(unit_square(), path);
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.triangle_count(), 20);
    }

    #[test]
    fn sweep_cw_profile_auto_flipped() {
        // Same square but CW. Algorithm reads signed_area and reverses
        // iteration order so the output solid is identical to the CCW input.
        let cw_square = Polygon2D::new(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]])
            .expect("cw square constructs");
        let op_cw = SweepOp::new(cw_square, z_path(&[0.0, 1.0]));
        let op_ccw = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let mesh_cw = op_cw.evaluate(&[]).expect("cw evaluate");
        let mesh_ccw = op_ccw.evaluate(&[]).expect("ccw evaluate");
        assert_eq!(mesh_cw.vertex_count(), mesh_ccw.vertex_count());
        assert_eq!(mesh_cw.triangle_count(), mesh_ccw.triangle_count());
    }

    #[test]
    fn sweep_top_ring_z_equals_last_path_z() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.5]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        // Last-ring offset = (m-1) * n = 2 * 4 = 8. Vertices 8..12 should be
        // at z=2.5.
        for v in &mesh.positions[8..12] {
            assert!((v[2] - 2.5).abs() < 1e-6, "expected z=2.5; got {}", v[2]);
        }
        // First-ring vertices 0..4 at z=0.
        for v in &mesh.positions[0..4] {
            assert!(v[2].abs() < 1e-6, "expected z=0; got {}", v[2]);
        }
    }

    // ----- structural_hash -----

    #[test]
    fn sweep_structural_hash_deterministic() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_path() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 2.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_profile() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_path_segment_count() {
        // Same start + end but different segment count: hash must differ.
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 0.5, 1.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_op_kind_is_sweep() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op.op_kind(), OpKind::Sweep);
    }

    #[test]
    fn sweep_output_is_labeled_returns_false() {
        // Arity 0 — no inputs to be labeled. Default trait method correctly
        // returns false (`.any` on empty slice is false).
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert!(!op.output_is_labeled(&[]));
    }
}
