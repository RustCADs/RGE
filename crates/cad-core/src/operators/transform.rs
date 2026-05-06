//! `TransformOp` — affine TRS transform on a single upstream tessellation
//! (arity 1).
//!
//! Failure class: snapshot-recoverable
//!
//! Builds the standard scale → rotate → translate matrix via
//! [`glam::Mat4::from_scale_rotation_translation`] and applies it to every
//! position in the upstream mesh. Indices pass through unchanged. Normals are
//! NOT carried in this Phase-7.1 dispatch (positions only).

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;

/// Affine TRS transform applied to one upstream `Tessellation`.
///
/// `rotation_quat_xyzw` is `[x, y, z, w]` to match glam's
/// [`glam::Quat::from_xyzw`] convention. The identity rotation is
/// `[0.0, 0.0, 0.0, 1.0]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformOp {
    /// Translation in object space `[x, y, z]`.
    pub translation: [f32; 3],
    /// Rotation as a quaternion `[x, y, z, w]` (glam ordering).
    pub rotation_quat_xyzw: [f32; 4],
    /// Per-axis scale `[sx, sy, sz]`.
    pub scale: [f32; 3],
}

impl Default for TransformOp {
    /// Identity transform: no translation, identity rotation, unit scale.
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Operator for TransformOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Transform
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"transform");
        for v in self.translation {
            hasher.update(&v.to_le_bytes());
        }
        for v in self.rotation_quat_xyzw {
            hasher.update(&v.to_le_bytes());
        }
        for v in self.scale {
            hasher.update(&v.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];

        let mat = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::from(self.scale),
            glam::Quat::from_xyzw(
                self.rotation_quat_xyzw[0],
                self.rotation_quat_xyzw[1],
                self.rotation_quat_xyzw[2],
                self.rotation_quat_xyzw[3],
            ),
            glam::Vec3::from(self.translation),
        );

        let positions: Vec<[f32; 3]> = upstream
            .positions
            .iter()
            .map(|&p| {
                let v = mat.transform_point3(glam::Vec3::from(p));
                [v.x, v.y, v.z]
            })
            .collect();

        // Indices pass through unchanged.
        let indices = upstream.indices.clone();

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("TransformOp produced invalid tessellation: {e}"))
        })
    }

    /// `TransformOp::evaluate` calls [`Tessellation::new`] on the transformed
    /// positions, which produces an unlabeled output regardless of whether
    /// the upstream input carried `face_labels`. The current Phase-7.1
    /// implementation strips labels (positions-only — labels would need
    /// per-triangle pass-through, deferred to a future dispatch).
    ///
    /// Until that dispatch lands, override the default [`Operator::output_is_labeled`]
    /// (which would propagate the input's labeled-state under the
    /// `iter().any` rule) to return `false` so the cache-key prediction
    /// matches reality.
    fn output_is_labeled(&self, _inputs_labeled: &[bool]) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Tessellation {
        Tessellation::new(
            vec![
                [0.0_f32, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        )
        .expect("quad ok")
    }

    #[test]
    fn identity_transform_preserves_vertices_bit_identical() {
        let upstream = quad();
        let op = TransformOp::default();
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        assert_eq!(out.positions, upstream.positions);
        assert_eq!(out.indices, upstream.indices);
    }

    #[test]
    fn translation_shifts_positions_on_x() {
        let upstream = quad();
        let op = TransformOp {
            translation: [1.0, 0.0, 0.0],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        for (i, [x, y, z]) in out.positions.iter().enumerate() {
            let [ox, oy, oz] = upstream.positions[i];
            assert!(
                (*x - (ox + 1.0)).abs() < 1e-6,
                "x not shifted by 1.0 at idx {i}"
            );
            assert!((*y - oy).abs() < 1e-6);
            assert!((*z - oz).abs() < 1e-6);
        }
    }

    #[test]
    fn arity_violation_returns_wrong_arity() {
        let op = TransformOp::default();
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let a = TransformOp {
            translation: [1.0, 2.0, 3.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let b = a.clone();
        let c = TransformOp {
            translation: [1.0, 2.0, 3.5], // changed
            ..a.clone()
        };
        assert_eq!(a.structural_hash(), b.structural_hash());
        assert_ne!(a.structural_hash(), c.structural_hash());
    }

    /// `TransformOp::evaluate` strips labels (calls `Tessellation::new`,
    /// which always produces an unlabeled mesh) — so [`Operator::output_is_labeled`]
    /// must return `false` regardless of the input's labeled-state.
    /// Overrides the trait default which would propagate via `iter().any`.
    #[test]
    fn transform_output_is_labeled_strips() {
        let op = TransformOp::default();
        // Unlabeled input → unlabeled output (matches default).
        assert!(!op.output_is_labeled(&[false]));
        // Labeled input → STILL unlabeled output (overrides default).
        assert!(!op.output_is_labeled(&[true]));
    }
}
