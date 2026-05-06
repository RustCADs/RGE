//! Revolve operator: rotate a 2D profile around the Y-axis through a sweep
//! angle in `(0, 2π]`.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! # Geometry
//!
//! The profile is a closed [`Polygon2D`] in the XY plane with all `x >= 0`
//! (lying on the +X side of the Y-axis). Revolving each point `(x, y)` around
//! the Y-axis through `θ` produces `(x·cos θ, y, x·sin θ)` — a circle of
//! radius `x` at height `y` in the XZ plane.
//!
//! # Output topology
//!
//! For a profile with `n` points and `segments` rotational steps:
//!
//! ## Full revolution (`angle == 2π`)
//!
//! * Vertex count: `n * segments`        (no ring duplication — segment 0 == segment `segments` is implicit via index wrap)
//! * Triangle count: `2 * n * segments`  (each profile edge × each segment yields a quad split into 2 tris)
//! * Index count: `6 * n * segments`
//!
//! No cap faces — full revolution closes on itself.
//!
//! ## Partial revolution (`angle < 2π`)
//!
//! * Vertex count: `n * (segments + 1)`  (open ends — distinct rings at θ=0 and θ=angle)
//! * Side-wall triangle count: `2 * n * segments`
//! * Cap triangle count: `2 * (n - 2)`   (one fan-triangulated cap per open end, requires convex profile)
//! * Total triangle count: `2 * n * segments + 2 * (n - 2)`
//! * Index count: `3 *` triangle count
//!
//! # Concave profiles
//!
//! Full revolution emits side walls only (no caps), so concave profiles
//! project correctly. Partial revolution requires fan-triangulated caps
//! (mirrors [`crate::operators::ExtrudeOp`]'s convexity restriction) — caps
//! validated against [`Polygon2D::convexity`] at evaluate time. Self-
//! intersecting profiles produce incorrect output but are not detected —
//! caller's responsibility.
//!
//! # Winding convention
//!
//! Profile is interpreted as CCW in the XY plane (signed area > 0). CW input
//! is auto-reversed internally so the algorithm always processes CCW. The
//! side-wall outward-facing normals point radially outward + along the
//! polygon-edge normal (correct for CCW input). For partial revolution, the
//! start cap (ring 0, θ=0) has outward normal in -Z (away from the swept
//! volume which extends into +Z half-space as θ increases from 0); the end
//! cap (ring `segments`, θ=angle) has outward normal in the +tangent
//! direction at the end angle.

use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator, Polygon2D};
use crate::tessellation::Tessellation;

// ---------------------------------------------------------------------------
// RevolveOp
// ---------------------------------------------------------------------------

/// Sweep a [`Polygon2D`] profile around the Y-axis through `angle` radians to
/// produce a surface of revolution.
///
/// `segments` is the number of rotational steps and must be `>= 3`. `angle`
/// must lie in `(0, 2π]` and is finite. The profile must lie entirely on the
/// +X side of the Y-axis (`all x >= 0`), validated at [`RevolveOp::evaluate`]
/// time. For full revolution (`angle == 2π`) concave profiles are accepted;
/// for partial revolution (`angle < 2π`) caps require a strictly convex
/// profile (same fan-triangulation constraint as
/// [`crate::operators::ExtrudeOp`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevolveOp {
    /// 2D profile rotated around the Y-axis.
    pub profile: Polygon2D,
    /// Number of rotational steps. Must be `>= 3`.
    pub segments: u32,
    /// Sweep angle in radians, `(0, 2π]`. Defaults to `2π` (full revolution)
    /// for serde compatibility with pre-D-Partial-Revolve snapshots.
    #[serde(default = "default_angle_full_revolution")]
    pub angle: f32,
}

/// Serde default for [`RevolveOp::angle`] — `2π` (full revolution),
/// preserving legacy snapshot semantics.
fn default_angle_full_revolution() -> f32 {
    2.0 * PI
}

impl RevolveOp {
    /// Full-revolution constructor (`angle = 2π`). Backwards-compatible with
    /// pre-D-Partial-Revolve callers.
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `segments < 3`.
    pub fn new(profile: Polygon2D, segments: u32) -> Result<Self, OpError> {
        Self::partial(profile, segments, 2.0 * PI)
    }

    /// Partial-revolution constructor. Validates `segments >= 3`,
    /// `angle ∈ (0, 2π]` and finite. The profile-shape validity (all
    /// `x >= 0`, `signed_area != 0`, plus convexity check when
    /// `angle < 2π`) is checked at [`RevolveOp::evaluate`] time.
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `segments < 3`.
    /// * [`OpError::InvalidParameter`] if `angle` is not finite.
    /// * [`OpError::InvalidParameter`] if `angle <= 0` or `angle > 2π + 1e-5`.
    pub fn partial(profile: Polygon2D, segments: u32, angle: f32) -> Result<Self, OpError> {
        if segments < 3 {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.segments must be >= 3 (got {segments})"
            )));
        }
        if !angle.is_finite() {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.angle must be finite (got {angle})"
            )));
        }
        let two_pi = 2.0 * PI;
        if angle <= 0.0 || angle > two_pi + 1e-5 {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.angle must be in (0, 2π] (got {angle})"
            )));
        }
        // Clamp to exactly 2π if within epsilon — protects the
        // full-revolution fast path from float drift in the
        // `angle == two_pi` comparison.
        let clamped = if (angle - two_pi).abs() < 1e-5 {
            two_pi
        } else {
            angle
        };
        Ok(Self {
            profile,
            segments,
            angle: clamped,
        })
    }

    /// Number of segments (always `>= 3` once constructed via
    /// [`RevolveOp::new`] or [`RevolveOp::partial`]).
    #[must_use]
    pub fn segments(&self) -> u32 {
        self.segments
    }

    /// Sweep angle in radians.
    #[must_use]
    pub fn angle(&self) -> f32 {
        self.angle
    }

    /// Returns `true` if this is a full-revolution operator (no caps emitted,
    /// concave profiles allowed). Uses an epsilon comparison against `2π` to
    /// absorb float drift; constructors clamp inputs within `1e-5` of `2π` to
    /// exactly `2π`, so this check uses a tighter `1e-6` epsilon to match
    /// post-clamp values bit-for-bit while still tolerating any residual
    /// arithmetic noise.
    #[must_use]
    pub fn is_full_revolution(&self) -> bool {
        let two_pi = 2.0 * PI;
        (self.angle - two_pi).abs() < 1e-6
    }
}

impl Operator for RevolveOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Revolve
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"revolve:");
        hasher.update(&self.segments.to_le_bytes());
        hasher.update(&self.angle.to_le_bytes());
        let profile_len = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        hasher.update(&profile_len.to_le_bytes());
        for [x, y] in self.profile.points() {
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

        // Defensive — `RevolveOp::new` already enforces, but `segments` is a
        // pub field and a caller could have mutated it post-construction.
        if self.segments < 3 {
            return Err(OpError::InvalidParameter(format!(
                "revolve segments must be >= 3 (got {})",
                self.segments
            )));
        }

        // Defensive angle re-validation — `angle` is a pub field.
        if !self.angle.is_finite() {
            return Err(OpError::InvalidParameter(format!(
                "revolve angle must be finite (got {})",
                self.angle
            )));
        }
        let two_pi = 2.0 * PI;
        if self.angle <= 0.0 || self.angle > two_pi + 1e-5 {
            return Err(OpError::InvalidParameter(format!(
                "revolve angle must be in (0, 2π] (got {})",
                self.angle
            )));
        }

        // Defensive profile-shape re-validation. `Polygon2D::new` already
        // checked `len >= 3` and finiteness, but `profile` is pub.
        if self.profile.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "revolve profile needs >= 3 points (got {})",
                self.profile.len()
            )));
        }
        for (i, [x, y]) in self.profile.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "revolve profile has non-finite coordinate at index {i}"
                )));
            }
        }

        // +X-side restriction.
        for (i, [x, _y]) in self.profile.points().iter().enumerate() {
            if *x < 0.0 {
                return Err(OpError::InvalidParameter(format!(
                    "revolve profile must lie on +X side of Y-axis (all x >= 0); index {i} has x = {x}"
                )));
            }
        }

        // Reject near-zero-area / collinear profiles. Epsilon comparison
        // rather than exact == 0.0 to defend against tiny float-drift in
        // the shoelace sum that would otherwise sneak through.
        let signed_area = self.profile.signed_area();
        if signed_area.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "revolve profile is degenerate (near-zero area)".to_string(),
            ));
        }

        // Convexity gate — only for partial revolution (caps need
        // fan-triangulation). Full revolution allows concave profiles since
        // it emits no caps.
        let full_revolution = self.is_full_revolution();
        if !full_revolution {
            match self.profile.convexity() {
                Some(true) => {}
                Some(false) => {
                    return Err(OpError::InvalidParameter(
                        "partial revolution requires convex profile (got concave)".to_string(),
                    ));
                }
                None => {
                    return Err(OpError::InvalidParameter(
                        "revolve profile is degenerate (all points collinear)".to_string(),
                    ));
                }
            }
        }

        // Winding correction: signed_area > 0 → CCW already; < 0 → reverse.
        let n_points = self.profile.len();
        let ordered: Vec<[f32; 2]> = if signed_area > 0.0 {
            self.profile.points().to_vec()
        } else {
            self.profile.points().iter().rev().copied().collect()
        };

        let segments_usize = self.segments as usize;
        let n_u32 = u32::try_from(n_points).map_err(|_| {
            OpError::InvalidParameter(format!("revolve profile too large: {n_points} points"))
        })?;

        if full_revolution {
            self.evaluate_full(&ordered, n_u32, segments_usize)
        } else {
            self.evaluate_partial(&ordered, n_u32, segments_usize)
        }
    }
}

impl RevolveOp {
    /// Full-revolution algorithm (`angle == 2π`). Emits `n * segments`
    /// vertices and `2 * n * segments` triangles via index wrap (no caps).
    fn evaluate_full(
        &self,
        ordered: &[[f32; 2]],
        n_u32: u32,
        segments_usize: usize,
    ) -> Result<Tessellation, OpError> {
        let n_points = ordered.len();
        // Build vertex buffer: `segments` rings × `n_points` profile points
        // each.
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n_points * segments_usize);
        let two_pi = 2.0 * PI;
        // Cast u32 → f32 for trig parameter. `segments` is bounded in
        // practice to a few thousand (UI knob); precision loss is irrelevant.
        #[allow(clippy::cast_precision_loss)]
        let inv_segments = 1.0 / self.segments as f32;
        for ring in 0..self.segments {
            #[allow(clippy::cast_precision_loss)]
            let theta = (ring as f32) * two_pi * inv_segments;
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            for [x, y] in ordered {
                positions.push([x * cos_t, *y, x * sin_t]);
            }
        }

        // Build side-wall triangles. For each profile edge `edge_idx` (wrap
        // edge_idx+1 → 0) and each segment ring `ring` (wrap ring+1 → 0),
        // emit a quad split into two CCW-from-outside triangles.
        let triangle_count = 2 * n_points * segments_usize;
        let mut indices: Vec<u32> = Vec::with_capacity(3 * triangle_count);
        for ring in 0..self.segments {
            let ring_next = (ring + 1) % self.segments;
            for edge_idx in 0..n_u32 {
                let edge_next = (edge_idx + 1) % n_u32;
                let bottom_left = ring * n_u32 + edge_idx;
                let bottom_right = ring * n_u32 + edge_next;
                let top_right = ring_next * n_u32 + edge_next;
                let top_left = ring_next * n_u32 + edge_idx;

                // Quad split into 2 triangles — CCW from radially-outward
                // viewpoint.
                indices.push(bottom_left);
                indices.push(bottom_right);
                indices.push(top_right);
                indices.push(bottom_left);
                indices.push(top_right);
                indices.push(top_left);
            }
        }

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("RevolveOp produced invalid tessellation: {e}"))
        })
    }

    /// Partial-revolution algorithm (`angle < 2π`). Emits
    /// `n * (segments + 1)` vertices, `2 * n * segments` side-wall triangles,
    /// and `2 * (n - 2)` cap triangles (start cap + end cap, both
    /// fan-triangulated from profile vertex 0).
    fn evaluate_partial(
        &self,
        ordered: &[[f32; 2]],
        n_u32: u32,
        segments_usize: usize,
    ) -> Result<Tessellation, OpError> {
        let n_points = ordered.len();
        let rings = segments_usize + 1; // open ends — ring at θ=angle distinct from ring at θ=0

        // Build vertex buffer: `segments + 1` rings × `n_points` profile
        // points each. Step is `angle / segments` (NOT `2π / segments`).
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n_points * rings);
        // Cast u32 → f32 for trig parameter. `segments` is bounded in
        // practice to a few thousand (UI knob); precision loss is irrelevant.
        #[allow(clippy::cast_precision_loss)]
        let step = self.angle / self.segments as f32;
        for ring in 0..=self.segments {
            #[allow(clippy::cast_precision_loss)]
            let theta = (ring as f32) * step;
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            for [x, y] in ordered {
                positions.push([x * cos_t, *y, x * sin_t]);
            }
        }

        // Triangle counts: side walls + 2 caps.
        let side_tris = 2 * n_points * segments_usize;
        let cap_tris = 2 * (n_points - 2);
        let total_tris = side_tris + cap_tris;
        let mut indices: Vec<u32> = Vec::with_capacity(3 * total_tris);

        // Side-wall triangles. NOTE: loop is `0..segments` (NOT
        // `0..=segments`) — ring `s` and ring `s+1` form `n` quads each, so
        // `segments` quad strips × `n` quads = `n*segments` quads × 2 tris.
        // No wrap on the `s+1` axis (open ends).
        for ring in 0..self.segments {
            let ring_next = ring + 1;
            for edge_idx in 0..n_u32 {
                let edge_next = (edge_idx + 1) % n_u32;
                let bottom_left = ring * n_u32 + edge_idx;
                let bottom_right = ring * n_u32 + edge_next;
                let top_right = ring_next * n_u32 + edge_next;
                let top_left = ring_next * n_u32 + edge_idx;

                indices.push(bottom_left);
                indices.push(bottom_right);
                indices.push(top_right);
                indices.push(bottom_left);
                indices.push(top_right);
                indices.push(top_left);
            }
        }

        // Start cap (ring 0, θ=0): profile lies in the XY plane. The swept
        // volume extends into +Z half-space as θ increases from 0, so the
        // outward normal of the start cap is -Z. Fan triangulation from
        // vertex 0 of the ordered (CCW from +Z) profile, listed in 3D as
        // `(0, i+1, i)` to flip handedness from +Z-CCW to -Z-CCW (matches
        // ExtrudeOp's bottom-cap winding convention).
        for i in 1..(n_u32 - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i);
        }

        // End cap (ring `segments`, θ=angle): profile lies in the plane
        // normal to the swept tangent at the end angle. Outward normal
        // continues in the +tangent direction. Fan triangulation from
        // vertex 0 of the end ring, listed `(end_base, end_base+i,
        // end_base+i+1)` for CCW-from-+tangent winding.
        let segments_u32 = u32::try_from(segments_usize).map_err(|_| {
            OpError::InvalidParameter(format!("revolve segments too large: {segments_usize}"))
        })?;
        let end_base = segments_u32 * n_u32;
        for i in 1..(n_u32 - 1) {
            indices.push(end_base);
            indices.push(end_base + i);
            indices.push(end_base + i + 1);
        }

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("RevolveOp produced invalid tessellation: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ccw_right_triangle_on_plus_x() -> Polygon2D {
        // Right triangle on +X side: (1,0) → (2,0) → (1,1) → close.
        // signed_area = 0.5 (CCW).
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [1.0, 1.0]]).expect("right triangle")
    }

    fn ccw_square_on_plus_x() -> Polygon2D {
        // Unit square, x in [1, 2], y in [0, 1] — CCW.
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ccw +x square")
    }

    fn cw_square_on_plus_x() -> Polygon2D {
        // Same +X-side square footprint, listed CW.
        Polygon2D::new(vec![[1.0, 0.0], [1.0, 1.0], [2.0, 1.0], [2.0, 0.0]]).expect("cw +x square")
    }

    fn ccw_concave_l_on_plus_x() -> Polygon2D {
        // L-shape on +X side: outer corners (1,0)..(3,0)..(3,1)..(2,1)..(2,2)..(1,2).
        // signed_area > 0 (CCW); concave at (2,1).
        Polygon2D::new(vec![
            [1.0, 0.0],
            [3.0, 0.0],
            [3.0, 1.0],
            [2.0, 1.0],
            [2.0, 2.0],
            [1.0, 2.0],
        ])
        .expect("ccw +x L-shape")
    }

    fn ccw_axis_touching_triangle() -> Polygon2D {
        // Right triangle with one vertex on the Y-axis: (0,0) → (1,0) → (0,1).
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]).expect("axis-touching triangle")
    }

    // -- RevolveOp::new ------------------------------------------------------

    #[test]
    fn revolve_new_rejects_segments_below_3() {
        let err = RevolveOp::new(ccw_square_on_plus_x(), 2).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("segments"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
        let err = RevolveOp::new(ccw_square_on_plus_x(), 0).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
        let err = RevolveOp::new(ccw_square_on_plus_x(), 1).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn revolve_new_accepts_segments_3() {
        let op = RevolveOp::new(ccw_square_on_plus_x(), 3).expect("min valid");
        assert_eq!(op.segments(), 3);
    }

    #[test]
    fn revolve_new_defaults_to_full_revolution() {
        let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
        assert!(op.is_full_revolution());
        assert!((op.angle() - 2.0 * PI).abs() < 1e-6);
    }

    // -- evaluate rejection paths --------------------------------------------

    #[test]
    fn revolve_evaluate_rejects_inputs_for_arity_0() {
        let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
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
    fn revolve_evaluate_rejects_negative_x_in_profile() {
        // Square that crosses the Y-axis (x in [-0.5, 0.5]).
        let crossing = Polygon2D::new(vec![[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]])
            .expect("crossing square");
        let op = RevolveOp::new(crossing, 4).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("x >= 0"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn revolve_post_construction_segments_corruption_rejected() {
        // Post-construction mutation: `segments` is pub.
        let mut op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
        op.segments = 2;
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn revolve_post_construction_angle_corruption_rejected() {
        // `angle` is also a pub field — defensively re-checked.
        let mut op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
        op.angle = -1.0;
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("angle"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }

        let mut op2 = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
        op2.angle = f32::NAN;
        let err = op2.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    // -- vertex / triangle counts (full revolution) -------------------------

    #[test]
    fn revolve_triangle_profile_4_segments() {
        // n=3 × 4 segments → 12 verts, 24 tris, 72 indices.
        let op = RevolveOp::new(ccw_right_triangle_on_plus_x(), 4).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.triangle_count(), 24);
        assert_eq!(mesh.indices.len(), 72);
    }

    #[test]
    fn revolve_square_profile_6_segments() {
        // n=4 × 6 segments → 24 verts, 48 tris, 144 indices.
        let op = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.triangle_count(), 48);
        assert_eq!(mesh.indices.len(), 144);
    }

    #[test]
    fn revolve_concave_profile_accepted() {
        // L-shape (concave) — full revolution doesn't fan-triangulate caps,
        // so concavity is allowed. Verify a non-empty mesh comes back with
        // expected counts: n=6 × 5 = 30 verts, 2*6*5 = 60 tris.
        let op = RevolveOp::new(ccw_concave_l_on_plus_x(), 5).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate concave");
        assert_eq!(mesh.vertex_count(), 30);
        assert_eq!(mesh.triangle_count(), 60);
        assert_eq!(mesh.indices.len(), 180);
    }

    #[test]
    fn revolve_axis_touching_profile_yields_degenerate_triangles_but_valid_mesh() {
        // Profile touches the axis at (0,0) and (0,1). Revolved 4 segments:
        // n=3 × 4 = 12 verts. Some triangles are degenerate (zero area at
        // the axis-collapsed vertices) but Tessellation::new only validates
        // index bounds + multiple-of-3, so the mesh constructs fine.
        let op = RevolveOp::new(ccw_axis_touching_triangle(), 4).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.triangle_count(), 24);
        assert_eq!(mesh.indices.len(), 72);

        // Spot-check axis vertices: profile points (0,0) and (0,1) collapse
        // to the same 3D position across all rings.
        // Profile is `ordered` after winding correction. signed_area of
        // [(0,0),(1,0),(0,1)] = 0.5 → CCW already, no reversal.
        // ring s=0 vertices: (0,0,0), (1,0,0), (0,1,0).
        // ring s=1 vertices: (0,0,0), (cos π/2, 0, sin π/2)=(0,0,1), (0,1,0).
        // Axis-touching vertices (indices 0, 2 in each ring) all share x=0, z=0.
        for s in 0..4 {
            let axis0 = mesh.positions[s * 3];
            let axis2 = mesh.positions[s * 3 + 2];
            assert!(axis0[0].abs() < 1e-5 && axis0[2].abs() < 1e-5);
            assert!(axis2[0].abs() < 1e-5 && axis2[2].abs() < 1e-5);
        }
    }

    #[test]
    fn revolve_cw_profile_handled() {
        // Same square footprint, CW order — algorithm reverses internally.
        let op = RevolveOp::new(cw_square_on_plus_x(), 6).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate cw");
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.triangle_count(), 48);
        assert_eq!(mesh.indices.len(), 144);
    }

    // -- structural_hash (full + parameter sensitivity) ---------------------

    #[test]
    fn revolve_structural_hash_deterministic() {
        let a = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("a");
        let b = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn revolve_structural_hash_changes_with_segments() {
        let a = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("a");
        let b = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn revolve_structural_hash_changes_with_profile_perturbation() {
        let a = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("a");
        let perturbed = Polygon2D::new(vec![
            [1.0, 0.0],
            [2.0 + 1.0e-3, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
        ])
        .expect("perturbed");
        let b = RevolveOp::new(perturbed, 6).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    // -- geometric correctness (full revolution) -----------------------------

    #[test]
    fn revolve_at_segment_zero_first_ring_lies_in_xy_plane() {
        // At s=0, theta=0 so cos=1, sin=0. Ring-0 vertices should be
        // (x, y, 0) — i.e. z = 0 with x, y matching the (winding-corrected)
        // profile coords.
        let op = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        let ordered = ccw_square_on_plus_x().points().to_vec(); // already CCW
        for (i, [x, y]) in ordered.iter().enumerate() {
            let v = mesh.positions[i];
            assert!((v[0] - x).abs() < 1.0e-5, "x mismatch at {i}: {v:?}");
            assert!((v[1] - y).abs() < 1.0e-5, "y mismatch at {i}: {v:?}");
            assert!(v[2].abs() < 1.0e-5, "z != 0 at ring 0 idx {i}: {v:?}");
        }
    }

    #[test]
    fn revolve_op_kind_is_revolve() {
        let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
        assert_eq!(op.op_kind(), OpKind::Revolve);
        assert_eq!(op.arity(), 0);
    }

    #[test]
    fn revolve_full_2pi_closes_seamlessly() {
        // Last segment must wrap back to ring 0 (closure check). Triangles
        // emitted across the s=segments-1 → s=0 seam reference indices in
        // ring 0 directly. Every vertex should lie on a circle of correct
        // radius.
        let op = RevolveOp::new(ccw_square_on_plus_x(), 12).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // Each of the 4 profile points has its own radius (1, 2, 2, 1); the
        // 12 rings produce 12 vertices on each circle. Verify every vertex
        // is on circle of radius 1 or 2.
        for [x, y, z] in &mesh.positions {
            let r2 = x * x + z * z;
            let near_1 = (r2 - 1.0).abs() < 1.0e-4;
            let near_4 = (r2 - 4.0).abs() < 1.0e-4;
            assert!(near_1 || near_4, "unexpected r²={r2} at vertex {x},{y},{z}");
            assert!(*y >= -1.0e-5 && *y <= 1.0 + 1.0e-5);
        }
    }

    #[test]
    fn revolve_first_quad_has_outward_radial_normal() {
        // Triangle profile [(1,0),(2,0),(1,1)] × 4 segs. The first side-wall
        // triangle sits on the y=0 bottom rim — its outward normal must
        // point in -Y (away from the +Y interior of the closed prism).
        let op = RevolveOp::new(ccw_right_triangle_on_plus_x(), 4).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // First triangle indices (a, b, c) for s=0, p=0:
        let i0 = mesh.indices[0] as usize;
        let i1 = mesh.indices[1] as usize;
        let i2 = mesh.indices[2] as usize;
        let a = mesh.positions[i0];
        let b = mesh.positions[i1];
        let c = mesh.positions[i2];
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        // For this triangle on the y=0 rim, expect a strongly -Y component
        // in the normal (face points downward = away from the +Y interior).
        assert!(
            n[1] < 0.0,
            "expected -Y outward normal on bottom-rim quad, got {n:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Partial-revolution tests (D-Partial-Revolve)
    // -----------------------------------------------------------------------

    #[test]
    fn revolve_partial_rejects_zero_angle() {
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, 0.0).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("angle"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn revolve_partial_rejects_negative_angle() {
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, -1.0).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("angle"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn revolve_partial_rejects_non_finite_angle() {
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::NAN).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("angle"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::INFINITY).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::NEG_INFINITY).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn revolve_partial_rejects_angle_exceeding_2pi() {
        // Just above 2π+1e-5 must reject — anything in [2π, 2π+1e-5] is
        // tolerated by the constructor and clamped to exactly 2π.
        let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, 2.0 * PI + 0.01).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("angle"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn revolve_partial_clamps_near_2pi_to_full_revolution() {
        // Tiny epsilon above 2π → constructor accepts it and clamps to exactly
        // 2π. is_full_revolution() returns true.
        let op =
            RevolveOp::partial(ccw_square_on_plus_x(), 4, 2.0 * PI + 1.0e-7).expect("clamps to 2π");
        assert!(op.is_full_revolution());
        assert!((op.angle() - 2.0 * PI).abs() < 1e-6);
    }

    #[test]
    fn revolve_full_2pi_via_partial_constructor_matches_new() {
        // partial(p, segs, 2π) and new(p, segs) must produce byte-identical
        // tessellations. We compare via `to_bits` to satisfy clippy::float_cmp
        // — exact bitwise equality is what we genuinely want here (both paths
        // run the same algorithm with the same inputs, so the outputs MUST
        // match bit-for-bit).
        let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, 2.0 * PI).expect("partial");
        let b = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("new");
        let mesh_a = a.evaluate(&[]).expect("eval a");
        let mesh_b = b.evaluate(&[]).expect("eval b");
        assert_eq!(mesh_a.positions.len(), mesh_b.positions.len());
        assert_eq!(mesh_a.indices, mesh_b.indices);
        for (va, vb) in mesh_a.positions.iter().zip(mesh_b.positions.iter()) {
            for (a_i, b_i) in va.iter().zip(vb.iter()) {
                assert_eq!(
                    a_i.to_bits(),
                    b_i.to_bits(),
                    "vertex bit-mismatch: {va:?} vs {vb:?}"
                );
            }
        }
    }

    #[test]
    fn revolve_partial_pi_triangle_profile_yields_correct_counts() {
        // Triangle profile (n=3) × 4 segments × angle=π:
        //   vertex count: 3 * (4+1) = 15
        //   side tris: 2*3*4 = 24
        //   cap tris: 2*(3-2) = 2
        //   total tris: 26
        //   indices: 78
        let op = RevolveOp::partial(ccw_right_triangle_on_plus_x(), 4, PI).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 15);
        assert_eq!(mesh.triangle_count(), 26);
        assert_eq!(mesh.indices.len(), 78);
    }

    #[test]
    fn revolve_partial_half_pi_square_profile_yields_correct_counts() {
        // Square (n=4) × 8 segments × angle=π/2:
        //   vertex count: 4 * (8+1) = 36
        //   side tris: 2*4*8 = 64
        //   cap tris: 2*(4-2) = 4
        //   total tris: 68
        //   indices: 204
        let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 36);
        assert_eq!(mesh.triangle_count(), 68);
        assert_eq!(mesh.indices.len(), 204);
    }

    #[test]
    fn revolve_partial_concave_profile_rejected() {
        // L-shape (concave) is disallowed for partial revolution because the
        // caps would need a non-fan triangulation.
        let op = RevolveOp::partial(ccw_concave_l_on_plus_x(), 4, PI).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "msg = {msg}");
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn revolve_full_concave_profile_still_accepted() {
        // Regression check: full revolution should still allow concave
        // profiles (no caps emitted).
        let op = RevolveOp::new(ccw_concave_l_on_plus_x(), 4).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate concave full");
        assert_eq!(mesh.vertex_count(), 24); // n=6 × 4 = 24
        assert_eq!(mesh.triangle_count(), 48); // 2*6*4 = 48
    }

    #[test]
    fn revolve_partial_start_cap_lies_in_xy_plane() {
        // For angle=π/2, ring 0 vertices have z=0 and (x,y) match (winding-
        // corrected) profile coords.
        let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        let ordered = ccw_square_on_plus_x().points().to_vec(); // already CCW
        for (i, [x, y]) in ordered.iter().enumerate() {
            let v = mesh.positions[i];
            assert!((v[0] - x).abs() < 1e-5, "x mismatch at {i}: {v:?}");
            assert!((v[1] - y).abs() < 1e-5, "y mismatch at {i}: {v:?}");
            assert!(v[2].abs() < 1e-5, "z != 0 at ring 0 idx {i}: {v:?}");
        }
    }

    #[test]
    fn revolve_partial_end_cap_at_angle_pi_lies_in_minus_x_plane() {
        // For angle=π, the end ring (s=segments) has cos(π)=-1, sin(π)=0, so
        // every end-ring vertex (x', y, z') satisfies x' = -x_profile,
        // y = y_profile, z' ≈ 0.
        let segments: u32 = 6;
        let op = RevolveOp::partial(ccw_square_on_plus_x(), segments, PI).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        let n = ccw_square_on_plus_x().len();
        let ordered = ccw_square_on_plus_x().points().to_vec();
        let end_base = (segments as usize) * n;
        for (i, [x, y]) in ordered.iter().enumerate() {
            let v = mesh.positions[end_base + i];
            assert!(
                (v[0] + x).abs() < 1e-5,
                "x' should equal -x_profile at end ring idx {i}: v={v:?} expected x'≈{}",
                -x
            );
            assert!(
                (v[1] - y).abs() < 1e-5,
                "y mismatch at end ring idx {i}: {v:?}"
            );
            assert!(v[2].abs() < 1e-4, "z should be ≈ 0 at θ=π, idx {i}: {v:?}");
        }
    }

    #[test]
    fn revolve_partial_structural_hash_changes_with_angle() {
        let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("a");
        let b = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn revolve_partial_structural_hash_deterministic_across_constructions() {
        // Same params via partial() twice → identical hash.
        let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("a");
        let b = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
        // And new() and partial(2π) also identical-hash since `clamped`
        // behavior gives both exactly 2π.
        let c = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("c");
        let d = RevolveOp::partial(ccw_square_on_plus_x(), 6, 2.0 * PI).expect("d");
        assert_eq!(c.structural_hash(), d.structural_hash());
    }

    #[test]
    fn revolve_partial_cw_profile_handled() {
        // CW-ordered square + partial revolution. Algorithm reverses
        // internally; vertex/tri counts must match the CCW case.
        let op = RevolveOp::partial(cw_square_on_plus_x(), 8, PI / 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate cw partial");
        assert_eq!(mesh.vertex_count(), 36);
        assert_eq!(mesh.triangle_count(), 68);
        assert_eq!(mesh.indices.len(), 204);
    }

    #[test]
    fn revolve_partial_start_cap_normal_points_minus_z() {
        // Verify the start cap at θ=0 has its normal pointing in -Z (into
        // the half-space the revolution sweeps AWAY from).
        let op = RevolveOp::partial(ccw_right_triangle_on_plus_x(), 4, PI / 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // Side-wall tris come first (2*3*4 = 24 tris = 72 indices), then the
        // start-cap fan (n-2=1 tri = 3 indices) at offset 72.
        let cap_start = 2 * 3 * 4 * 3; // side-wall index count
        let i0 = mesh.indices[cap_start] as usize;
        let i1 = mesh.indices[cap_start + 1] as usize;
        let i2 = mesh.indices[cap_start + 2] as usize;
        let a = mesh.positions[i0];
        let b = mesh.positions[i1];
        let c = mesh.positions[i2];
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        // Start cap is in XY plane (z=0); normal must be ±Z; outward = -Z.
        assert!(n[2] < 0.0, "start-cap normal should be -Z, got {n:?}");
    }
}
