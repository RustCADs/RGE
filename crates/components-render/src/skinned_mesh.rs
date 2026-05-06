//! [`SkinnedMesh`] — binds a mesh entity to a skeleton for skeletal animation.
//!
//! Per PLAN.md §1.5.1 mesh role optional component. The component carries a
//! reference to the (separately-stored) skeleton asset; the runtime hooks
//! the skeleton's `BoneTransforms` (in components-animation) into the
//! skinning system at draw time.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// "This mesh deforms via the named skeleton's bone matrices."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkinnedMesh {
    /// Skeleton asset (joint hierarchy + inverse-bind matrices).
    pub skeleton: AssetId,
    /// Optional retargeting profile: maps source skeleton bones to a
    /// destination rig. [`NULL_ASSET_ID`] = no retarget.
    pub retarget_profile: AssetId,
}

impl Default for SkinnedMesh {
    fn default() -> Self {
        Self {
            skeleton: NULL_ASSET_ID,
            retarget_profile: NULL_ASSET_ID,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = SkinnedMesh {
            skeleton: AssetId::from_bytes(b"skinned-mesh-skeleton-1"),
            retarget_profile: NULL_ASSET_ID,
        };
        let txt = ron::to_string(&s).expect("serialize");
        let back: SkinnedMesh = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }
}
