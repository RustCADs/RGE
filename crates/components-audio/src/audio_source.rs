//! [`AudioSource`] — emitter component.
//!
//! Per PLAN.md §1.5.1 the audio-source role pairs `Transform` + `AudioSource`
//! + `Name` (and optionally `AudioFalloff`).

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// Audio source (sample-playback emitter).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioSource {
    /// Currently bound clip asset.
    pub clip: AssetId,
    /// Output gain multiplier (linear; `1.0` = unity).
    pub volume: f32,
    /// Pitch / playback rate (`1.0` = source rate).
    pub pitch: f32,
    /// Loop the clip on reaching the end.
    pub looping: bool,
    /// Whether the source is currently playing. Authored false; flipped to
    /// true by gameplay code or scene activation.
    pub is_playing: bool,
    /// True if the source emits in 3D space (with falloff & spatialisation);
    /// false for music / 2D UI sounds that should bypass the spatial mix.
    pub is_spatial: bool,
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            clip: NULL_ASSET_ID,
            volume: 1.0,
            pitch: 1.0,
            looping: false,
            is_playing: false,
            is_spatial: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = AudioSource {
            clip: AssetId::from_bytes(b"audio-clip-fixture-1"),
            volume: 0.75,
            pitch: 1.0,
            looping: true,
            is_playing: true,
            is_spatial: false,
        };
        let txt = ron::to_string(&s).expect("serialize");
        let back: AudioSource = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn default_is_silent() {
        let s = AudioSource::default();
        assert!(!s.is_playing);
    }
}
