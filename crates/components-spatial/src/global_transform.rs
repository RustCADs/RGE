//! [`GlobalTransform`] — world-space transform produced by tree propagation.
//!
//! Same shape as [`crate::Transform`] but lives in world space. A propagation
//! system reads `Transform` + `kernel/ecs::TreeRelationStorage::parent_of` and
//! writes this. Render systems consume `GlobalTransform` exclusively; they
//! never re-walk the parent chain.

use serde::{Deserialize, Serialize};

use crate::Transform;

/// World-space transform for an entity (translation, rotation, scale).
///
/// Stored as a separate component (not just a flag) so render snapshot staging
/// (PLAN.md §1.5.2) can copy world transforms cheaply without re-deriving
/// from local + parent chain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlobalTransform {
    /// World-space translation.
    pub translation: [f32; 3],
    /// World-space rotation quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// World-space per-axis scale.
    pub scale: [f32; 3],
}

impl GlobalTransform {
    /// Identity world transform.
    pub const IDENTITY: GlobalTransform = GlobalTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    };
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl From<Transform> for GlobalTransform {
    /// Lift a local [`Transform`] into world space, treating it as already-
    /// world-space (i.e. an entity with no parent). Real propagation lives
    /// downstream; this conversion is for root entities and tests.
    fn from(t: Transform) -> Self {
        Self {
            translation: t.translation,
            rotation: t.rotation,
            scale: t.scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip_ron() {
        let g = GlobalTransform::IDENTITY;
        let s = ron::to_string(&g).expect("serialize");
        let back: GlobalTransform = ron::from_str(&s).expect("deserialize");
        assert_eq!(g, back);
    }

    #[test]
    fn from_local_transform_passes_fields_through() {
        let t = Transform::from_translation([3.0, 4.0, 5.0]);
        let g: GlobalTransform = t.into();
        for (got, want) in g.translation.iter().zip([3.0_f32, 4.0, 5.0].iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
        for (got, want) in g.rotation.iter().zip(t.rotation.iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
        for (got, want) in g.scale.iter().zip(t.scale.iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
    }
}
