// adapted from rustforge::runtime-curves::curves on 2026-05-05 — kept the
//                                                  per-element flat layout idea
//                                                  but stripped the AttributeStorage
//                                                  indirection: a v0 ECS component
//                                                  needs to be Clone+Default+Copy-ish,
//                                                  so we use `Vec<[f32; 4]>` of
//                                                  pre-multiplied dual-quat slots.
//
//! [`BoneTransforms`] — per-frame bone matrices ready for skinning.
//!
//! Filled by the animation evaluator each tick; consumed by the skinning
//! system at draw time. Stored as a flat `Vec` of dual-quat slots
//! (`[rotation_xyzw, dual_xyzw]` per bone) so cache-line traversal is
//! linear. Per-bone count is determined by the bound `Skeleton`.

use serde::{Deserialize, Serialize};

/// Per-frame skinning palette.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoneTransforms {
    /// One entry per bone: `[real_xyzw, dual_xyzw]` dual quaternion.
    /// Index = bone index in the bound `Skeleton`.
    pub palette: Vec<[[f32; 4]; 2]>,
}

impl BoneTransforms {
    /// Construct an empty palette (zero bones).
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a palette with `bone_count` identity dual quaternions.
    #[must_use]
    pub fn with_identity_bones(bone_count: usize) -> Self {
        let identity = [[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]];
        Self {
            palette: vec![identity; bone_count],
        }
    }

    /// Number of bones in the palette.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.palette.len()
    }

    /// Whether the palette has zero bones.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.palette.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_empty() {
        let b = BoneTransforms::new();
        let s = ron::to_string(&b).expect("serialize");
        let back: BoneTransforms = ron::from_str(&s).expect("deserialize");
        assert_eq!(b, back);
    }

    #[test]
    fn round_trip_ron_with_bones() {
        let b = BoneTransforms::with_identity_bones(8);
        assert_eq!(b.len(), 8);
        let s = ron::to_string(&b).expect("serialize");
        let back: BoneTransforms = ron::from_str(&s).expect("deserialize");
        assert_eq!(b, back);
    }
}
