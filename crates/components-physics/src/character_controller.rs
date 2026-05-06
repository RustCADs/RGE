//! [`CharacterController`] — kinematic-character configuration.
//!
//! Drives the rapier character-controller path: capsule shape implied
//! through a sibling [`crate::Collider`], slope / step / push parameters
//! authored here.

use serde::{Deserialize, Serialize};

/// Character controller component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterController {
    /// Maximum slope the controller can climb, radians (45° default).
    pub max_slope_radians: f32,
    /// Maximum step height that is auto-climbed, meters.
    pub step_offset_m: f32,
    /// Distance below the feet at which the controller still considers
    /// itself "grounded" (anti-jitter on uneven terrain), meters.
    pub grounded_offset_m: f32,
    /// Whether this controller pushes other dynamic bodies on contact.
    pub pushes_dynamics: bool,
    /// Cached "grounded" state from the last simulation tick. Read-only
    /// from gameplay code's perspective; the physics system writes it.
    pub is_grounded: bool,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            max_slope_radians: std::f32::consts::FRAC_PI_4,
            step_offset_m: 0.3,
            grounded_offset_m: 0.05,
            pushes_dynamics: true,
            is_grounded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let c = CharacterController::default();
        let s = ron::to_string(&c).expect("serialize");
        let back: CharacterController = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }
}
