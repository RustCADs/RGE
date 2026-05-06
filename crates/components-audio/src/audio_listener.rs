//! [`AudioListener`] — receiver component, typically attached to the camera.
//!
//! Per PLAN.md §1.5.1 the camera role optionally carries `AudioListener`.
//! Only one listener should be active at a time; the audio crate enforces
//! that and emits a warning when multiple are present.

use serde::{Deserialize, Serialize};

/// Audio listener.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioListener {
    /// Master output gain (linear).
    pub volume: f32,
    /// Whether this listener is currently active. The audio system picks
    /// the highest-priority active listener; default is `true`.
    pub is_active: bool,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self {
            volume: 1.0,
            is_active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let l = AudioListener::default();
        let s = ron::to_string(&l).expect("serialize");
        let back: AudioListener = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }
}
