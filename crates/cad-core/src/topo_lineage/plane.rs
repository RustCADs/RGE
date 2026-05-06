//! Quantized-plane representation used to group triangles by face plane.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage`]; see that module's `//!` docs for
//! the design rationale + v0 simplifications vs PLAN §1.5.4.3.

use crate::topo_lineage::types::LineageError;

/// Quantization scale used by [`QuantizedPlane::from_triangle`]. `1e-4`
/// precision is generous enough to absorb the f32 error from a single
/// normalize-then-quantize, and tight enough that the unit cube's six
/// distinct planes hash distinctly.
const PLANE_QUANTIZATION_SCALE: f32 = 10_000.0;

/// Squared-magnitude threshold below which a triangle is rejected as
/// degenerate. The cross product `(b-a) × (c-a)` has magnitude `2 * area`,
/// so `1e-12` here corresponds to area below ~5e-7 (well under any practical
/// CAD tolerance).
const DEGENERATE_CROSS_MAGNITUDE_SQUARED: f32 = 1e-12;

/// Quantize one f32 component to an i32 at the configured scale. The cast
/// saturates on out-of-range inputs (Rust 1.45+ semantics), which is
/// exactly what we want — two triangles with the same plane equation
/// quantize to the same i32 even if the normalize step introduced jitter.
#[allow(
    clippy::cast_possible_truncation,
    reason = "deliberate saturation: huge offsets clamp to i32::MAX/MIN and still produce a stable hash"
)]
fn quantize(value: f32) -> i32 {
    (value * PLANE_QUANTIZATION_SCALE).round() as i32
}

/// Plane (normal, offset) in Hesse normal form: `n · p == offset`.
///
/// Quantized to ~1e-4 precision for hash / equality stability across f32
/// drift. Sign-canonicalized so opposite-facing duplicates of the same
/// plane (e.g. front-vs-back triangles on a wall) hash identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct QuantizedPlane {
    /// Normal x-component quantized as `(component * 10_000).round() as i32`.
    nx: i32,
    /// Normal y-component (same scaling).
    ny: i32,
    /// Normal z-component (same scaling).
    nz: i32,
    /// Plane offset = `n · a` quantized at the same scaling. Sign-flipped
    /// alongside the normal during canonicalization so the plane equation
    /// `nx*x + ny*y + nz*z == offset` continues to identify the same plane.
    offset: i32,
}

impl QuantizedPlane {
    /// Compute the plane of triangle `(a, b, c)` (right-hand rule:
    /// `normal = (b - a) × (c - a)` then normalized) and quantize it.
    ///
    /// # Errors
    ///
    /// * [`LineageError::DegenerateTriangle`] if `|cross|² < 1e-12`.
    /// * [`LineageError::NonFiniteNormal`] if any component of the
    ///   normalized normal or the offset is non-finite.
    pub(crate) fn from_triangle(
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        triangle_idx: usize,
    ) -> Result<Self, LineageError> {
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let mag2 = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
        if !mag2.is_finite() {
            return Err(LineageError::NonFiniteNormal { triangle_idx });
        }
        if mag2 < DEGENERATE_CROSS_MAGNITUDE_SQUARED {
            return Err(LineageError::DegenerateTriangle { triangle_idx });
        }
        let mag = mag2.sqrt();
        let nx = cross[0] / mag;
        let ny = cross[1] / mag;
        let nz = cross[2] / mag;
        let offset = nx * a[0] + ny * a[1] + nz * a[2];
        if !nx.is_finite() || !ny.is_finite() || !nz.is_finite() || !offset.is_finite() {
            return Err(LineageError::NonFiniteNormal { triangle_idx });
        }

        // Sign-canonicalize: ensure the first non-zero quantized component
        // is positive so opposite-facing duplicates of the same plane hash
        // identically. We canonicalize on the QUANTIZED components (not the
        // raw f32) so two triangles whose normals quantize to the same i32
        // tuple end up with the same canonical sign — otherwise tiny f32
        // jitter could classify them differently.
        //
        // The f32 → i32 cast saturates on out-of-range; that's the desired
        // behavior for our use case (huge offsets get clamped to i32::MAX
        // / i32::MIN and still produce a stable hash). Cast saturation is
        // guaranteed by Rust 1.45+; clippy's `cast_possible_truncation` is
        // a lossy-cast warning that does not apply here.
        let mut qx = quantize(nx);
        let mut qy = quantize(ny);
        let mut qz = quantize(nz);
        let mut qo = quantize(offset);

        // Determine the first non-zero quantized component and flip the
        // sign if it's negative.
        let leading_sign = if qx != 0 {
            qx.signum()
        } else if qy != 0 {
            qy.signum()
        } else if qz != 0 {
            qz.signum()
        } else {
            // All three quantized components zero — shouldn't happen for a
            // non-degenerate triangle (the post-normalize magnitude is 1.0
            // so at least one component quantizes to ±10_000) but guard
            // anyway: treat as degenerate.
            return Err(LineageError::DegenerateTriangle { triangle_idx });
        };
        if leading_sign < 0 {
            qx = -qx;
            qy = -qy;
            qz = -qz;
            qo = -qo;
        }

        Ok(Self {
            nx: qx,
            ny: qy,
            nz: qz,
            offset: qo,
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
    fn quantized_plane_from_degenerate_triangle_errors() {
        // Three collinear points → zero area → DegenerateTriangle.
        let err =
            QuantizedPlane::from_triangle([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0], 42)
                .unwrap_err();
        assert!(matches!(
            err,
            LineageError::DegenerateTriangle { triangle_idx: 42 }
        ));

        // Two coincident points.
        let err =
            QuantizedPlane::from_triangle([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 7)
                .unwrap_err();
        assert!(matches!(
            err,
            LineageError::DegenerateTriangle { triangle_idx: 7 }
        ));
    }

    #[test]
    fn quantized_plane_from_non_finite_errors() {
        // NaN in input propagates to a non-finite cross-product magnitude.
        let err = QuantizedPlane::from_triangle(
            [f32::NAN, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            3,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LineageError::NonFiniteNormal { triangle_idx: 3 }
                | LineageError::DegenerateTriangle { triangle_idx: 3 }
        ));
    }

    #[test]
    fn quantized_plane_canonicalizes_opposite_normals_to_same_hash() {
        // Same triangle (a, b, c) and reversed (c, b, a) should produce
        // identical QuantizedPlane values once sign-canonicalized.
        let a = [0.0, 0.0, 1.0];
        let b = [1.0, 0.0, 1.0];
        let c = [0.0, 1.0, 1.0];
        let forward = QuantizedPlane::from_triangle(a, b, c, 0).expect("forward plane");
        let reversed = QuantizedPlane::from_triangle(c, b, a, 0).expect("reversed plane");
        assert_eq!(
            forward, reversed,
            "opposite-winding triangles must canonicalize to the same plane: {forward:?} != {reversed:?}"
        );
    }
}
