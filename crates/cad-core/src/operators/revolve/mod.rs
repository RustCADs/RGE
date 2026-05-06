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
//! * **Full** (`angle == 2π`): `n*segments` verts, `2*n*segments` tris (no caps —
//!   index wrap closes the surface).
//! * **Partial** (`angle < 2π`): `n*(segments+1)` verts, `2*n*segments` side
//!   tris + `2*(n-2)` cap tris (fan-triangulated start+end caps; convex only).
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
//!
//! # Module layout
//!
//! * `full_path` — full-2π revolution algorithm (no caps; concave profiles
//!   accepted).
//! * `partial_path` — partial-revolution algorithm (fan-triangulated start /
//!   end caps; convexity required).

mod full_path;
mod partial_path;
#[cfg(test)]
mod tests;

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
            full_path::evaluate_full(self.segments, &ordered, n_u32, segments_usize)
        } else {
            partial_path::evaluate_partial(
                self.segments,
                self.angle,
                &ordered,
                n_u32,
                segments_usize,
            )
        }
    }
}
