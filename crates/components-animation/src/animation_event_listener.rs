//! [`AnimationEventListener`] — opt-in marker for entities that want to
//! receive animation track events.
//!
//! `anim-clip` assets carry typed event tracks (footstep, attack-window,
//! etc). Without this component, the animation system skips event emission
//! for the entity entirely — this matters because event dispatch is the
//! single most expensive step in the animation pipeline.

use serde::{Deserialize, Serialize};

/// Animation event listener configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnimationEventListener {
    /// If true, the animation system also forwards track events to a WASM
    /// script callback registered for this entity.
    pub forward_to_script: bool,
}

impl Default for AnimationEventListener {
    fn default() -> Self {
        Self {
            forward_to_script: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let l = AnimationEventListener::default();
        let s = ron::to_string(&l).expect("serialize");
        let back: AnimationEventListener = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }
}
