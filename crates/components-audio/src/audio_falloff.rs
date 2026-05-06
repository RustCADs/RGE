//! [`AudioFalloff`] — distance attenuation for spatial audio sources.
//!
//! Optional companion to [`crate::AudioSource`] when the default
//! distance-attenuation curve doesn't fit the asset (e.g. wide ambient
//! emitters or razor-sharp dialog sources).

use serde::{Deserialize, Serialize};

/// Falloff shape between `min_distance` (full volume) and `max_distance`
/// (silent).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FalloffShape {
    /// Linear volume rolloff: `gain = 1 - (d - min) / (max - min)`.
    Linear,
    /// Inverse-square (more physically correct for point sources).
    #[default]
    InverseSquare,
    /// Exponential: `gain = 0.5^((d - min) / half_distance)` — half the
    /// volume for every `half_distance` past `min`.
    Exponential,
}

/// Audio falloff curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioFalloff {
    /// Distance below which gain is clamped to 1.0, meters.
    pub min_distance: f32,
    /// Distance beyond which gain is clamped to 0.0, meters.
    pub max_distance: f32,
    /// Curve shape.
    pub shape: FalloffShape,
}

impl Default for AudioFalloff {
    fn default() -> Self {
        Self {
            min_distance: 1.0,
            max_distance: 25.0,
            shape: FalloffShape::InverseSquare,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default() {
        let f = AudioFalloff::default();
        let s = ron::to_string(&f).expect("serialize");
        let back: AudioFalloff = ron::from_str(&s).expect("deserialize");
        assert_eq!(f, back);
    }

    #[test]
    fn round_trip_ron_linear() {
        let f = AudioFalloff {
            min_distance: 0.5,
            max_distance: 100.0,
            shape: FalloffShape::Linear,
        };
        let s = ron::to_string(&f).expect("serialize");
        let back: AudioFalloff = ron::from_str(&s).expect("deserialize");
        assert_eq!(f, back);
    }

    #[test]
    fn round_trip_ron_exponential() {
        let f = AudioFalloff {
            min_distance: 1.0,
            max_distance: 50.0,
            shape: FalloffShape::Exponential,
        };
        let s = ron::to_string(&f).expect("serialize");
        let back: AudioFalloff = ron::from_str(&s).expect("deserialize");
        assert_eq!(f, back);
    }
}
