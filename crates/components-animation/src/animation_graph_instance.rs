//! [`AnimationGraphInstance`] — bound instance of an `anim-graph` asset.
//!
//! Carries the asset reference plus the runtime evaluation state slot index
//! (the actual blend-state buffer lives in a `Resource<AnimGraphRuntime>`
//! owned by the animation system). Authored by editor's anim-graph-editor.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// Animation graph instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnimationGraphInstance {
    /// Bound `anim-graph` asset.
    pub graph: AssetId,
    /// Slot index into the global anim-graph runtime arena. `u32::MAX` =
    /// "not yet allocated"; the animation system fills this on insert.
    pub runtime_slot: u32,
}

impl AnimationGraphInstance {
    /// Sentinel "no slot allocated yet".
    pub const UNALLOCATED: u32 = u32::MAX;

    /// Construct an instance bound to `graph` with no runtime slot yet.
    #[inline]
    #[must_use]
    pub const fn new(graph: AssetId) -> Self {
        Self {
            graph,
            runtime_slot: Self::UNALLOCATED,
        }
    }
}

impl Default for AnimationGraphInstance {
    fn default() -> Self {
        Self::new(NULL_ASSET_ID)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let i = AnimationGraphInstance::new(AssetId::from_bytes(b"anim-graph-instance-1"));
        let s = ron::to_string(&i).expect("serialize");
        let back: AnimationGraphInstance = ron::from_str(&s).expect("deserialize");
        assert_eq!(i, back);
    }

    #[test]
    fn unallocated_sentinel() {
        let i = AnimationGraphInstance::default();
        assert_eq!(i.runtime_slot, AnimationGraphInstance::UNALLOCATED);
    }
}
