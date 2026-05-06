//! `CuboidOp` — origin-centered axis-aligned box primitive (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! Produces a closed 8-vertex / 12-triangle box centered at the origin with
//! the half-extents `(width/2, height/2, depth/2)`. Right-handed CCW winding.

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;

/// Origin-centered axis-aligned cuboid primitive.
///
/// All three dimensions must be positive and finite — `evaluate` rejects
/// `0.0`, negatives, infinities, and NaN with [`OpError::InvalidParameter`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CuboidOp {
    /// Extent along the X axis (positive, finite).
    pub width: f32,
    /// Extent along the Y axis (positive, finite).
    pub height: f32,
    /// Extent along the Z axis (positive, finite).
    pub depth: f32,
}

impl Default for CuboidOp {
    /// Default unit cube: `1.0 x 1.0 x 1.0`.
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }
}

impl CuboidOp {
    /// Validate that all three dimensions are finite and `> 0.0`.
    fn validate(&self) -> Result<(), OpError> {
        for (label, value) in [
            ("width", self.width),
            ("height", self.height),
            ("depth", self.depth),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(OpError::InvalidParameter(format!(
                    "CuboidOp.{label} must be finite and > 0 (got {value})"
                )));
            }
        }
        Ok(())
    }
}

impl Operator for CuboidOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Cuboid
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cuboid");
        hasher.update(&self.width.to_le_bytes());
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.depth.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        self.validate()?;

        let hx = self.width * 0.5;
        let hy = self.height * 0.5;
        let hz = self.depth * 0.5;

        // 8 corner vertices. Indexing convention:
        //   0: (-x,-y,-z)  1: (+x,-y,-z)  2: (+x,+y,-z)  3: (-x,+y,-z)
        //   4: (-x,-y,+z)  5: (+x,-y,+z)  6: (+x,+y,+z)  7: (-x,+y,+z)
        let positions = vec![
            [-hx, -hy, -hz],
            [hx, -hy, -hz],
            [hx, hy, -hz],
            [-hx, hy, -hz],
            [-hx, -hy, hz],
            [hx, -hy, hz],
            [hx, hy, hz],
            [-hx, hy, hz],
        ];

        // 12 triangles, two per face. Right-handed CCW winding when viewed
        // from outside the box (along the outward face normal).
        #[rustfmt::skip]
        let indices = vec![
            // -Z face (back, normal -z): viewed from -z, CCW order is 0,3,2,1.
            0, 3, 2,  0, 2, 1,
            // +Z face (front, normal +z): viewed from +z, CCW is 4,5,6,7.
            4, 5, 6,  4, 6, 7,
            // -Y face (bottom, normal -y): viewed from -y, CCW is 0,1,5,4.
            0, 1, 5,  0, 5, 4,
            // +Y face (top, normal +y): viewed from +y, CCW is 3,7,6,2.
            3, 7, 6,  3, 6, 2,
            // -X face (left, normal -x): viewed from -x, CCW is 0,4,7,3.
            0, 4, 7,  0, 7, 3,
            // +X face (right, normal +x): viewed from +x, CCW is 1,2,6,5.
            1, 2, 6,  1, 6, 5,
        ];

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("CuboidOp produced invalid tessellation: {e}"))
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
    fn default_returns_unit_cube() {
        let op = CuboidOp::default();
        assert!((op.width - 1.0).abs() < f32::EPSILON);
        assert!((op.height - 1.0).abs() < f32::EPSILON);
        assert!((op.depth - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn evaluate_produces_8_vertices_and_12_triangles() {
        let op = CuboidOp::default();
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        // Spot-check that vertices are within ±0.5 (half-extents of unit cube).
        for [x, y, z] in &mesh.positions {
            assert!(x.abs() <= 0.5 + 1e-6);
            assert!(y.abs() <= 0.5 + 1e-6);
            assert!(z.abs() <= 0.5 + 1e-6);
        }
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let a = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.75,
        };
        let b = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.75,
        };
        let c = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.76,
        };
        assert_eq!(a.structural_hash(), b.structural_hash());
        assert_ne!(a.structural_hash(), c.structural_hash());
    }

    #[test]
    fn negative_dimension_rejected() {
        let op = CuboidOp {
            width: -1.0,
            height: 1.0,
            depth: 1.0,
        };
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    /// `CuboidOp` is arity 0 and emits an unlabeled `Tessellation::new(...)`
    /// — so the trait-default [`Operator::output_is_labeled`] (which returns
    /// `false` on an empty `inputs_labeled` slice via `iter().any`) matches
    /// the actual `evaluate` semantics. No override needed.
    #[test]
    fn cuboid_output_is_labeled_returns_false() {
        let op = CuboidOp::default();
        assert!(!op.output_is_labeled(&[]));
    }
}
