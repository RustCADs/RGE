//! [`AnimationPlayer`] — single-clip player with playback state.
//!
//! Drives a single `anim-clip` asset; for blend trees / state machines, use
//! [`crate::AnimationGraphInstance`] instead. Both can coexist on the same
//! entity (rare); the evaluator runs the player first, then the graph
//! overlays its result.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// Playback state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlaybackState {
    /// Idle / stopped at the start of the clip.
    #[default]
    Stopped,
    /// Playing forward at the configured speed.
    Playing,
    /// Held at the current `time_seconds`.
    Paused,
}

/// Animation player component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimationPlayer {
    /// Currently bound clip asset.
    pub clip: AssetId,
    /// Current playhead position in seconds.
    pub time_seconds: f32,
    /// Playback rate. `1.0` = real-time; negative values rewind; `0.0` is a
    /// degenerate "paused via speed" form (prefer [`PlaybackState::Paused`]).
    pub speed: f32,
    /// Loop the clip on reaching the end.
    pub looping: bool,
    /// Current state.
    pub state: PlaybackState,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self {
            clip: NULL_ASSET_ID,
            time_seconds: 0.0,
            speed: 1.0,
            looping: false,
            state: PlaybackState::Stopped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default() {
        let p = AnimationPlayer::default();
        let s = ron::to_string(&p).expect("serialize");
        let back: AnimationPlayer = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn round_trip_ron_playing() {
        let p = AnimationPlayer {
            clip: AssetId::from_bytes(b"anim-player-clip-1"),
            time_seconds: 1.25,
            speed: 1.5,
            looping: true,
            state: PlaybackState::Playing,
        };
        let s = ron::to_string(&p).expect("serialize");
        let back: AnimationPlayer = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }
}
