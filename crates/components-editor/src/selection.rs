//! [`Selected`] — zero-sized selection-marker for editor entities.
//!
//! Distinct from a scene-tree highlight (which is a render concern):
//! `Selected` is the editor's authoritative "this is in the current
//! selection set" flag. Selection systems own this; everything else
//! reads.

use serde::{Deserialize, Serialize};

/// Zero-sized "in editor selection" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selected;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = Selected;
        let txt = ron::to_string(&s).expect("serialize");
        let back: Selected = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }
}
