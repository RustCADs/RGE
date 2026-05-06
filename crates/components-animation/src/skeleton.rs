//! [`Skeleton`] — references the joint-hierarchy asset for an animated entity.
//!
//! Per PLAN.md §1.5.1 the skeleton entity role pairs `Transform`, `Skeleton`,
//! `BoneTransforms`, and `Name`. Bone children are spawned as separate
//! entities linked via `bone_of` (`kernel/ecs::DenseLinearRelationStorage`).

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// "This entity is the root of the skeleton with the given asset id."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Skeleton(pub AssetId);

impl Default for Skeleton {
    fn default() -> Self {
        Self(NULL_ASSET_ID)
    }
}

impl Skeleton {
    /// Construct a skeleton component from an asset id.
    #[inline]
    #[must_use]
    pub const fn new(id: AssetId) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = Skeleton::new(AssetId::from_bytes(b"skeleton-asset-1"));
        let txt = ron::to_string(&s).expect("serialize");
        let back: Skeleton = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }
}
