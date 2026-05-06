//! [`RigidBody`] — body-class component for physics integration.
//!
//! [`BodyType`] picks the integration mode; the wrapping struct also holds
//! gravity scale and CCD opt-in.

use serde::{Deserialize, Serialize};

/// Integration mode for a rigid body.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BodyType {
    /// Driven by forces / impulses; mass and gravity apply.
    #[default]
    Dynamic,
    /// Moved by user code (animation, scripted motion). Pushes Dynamic
    /// bodies but is not pushed back.
    Kinematic,
    /// Never moves. Maximally cheap to simulate.
    Static,
}

/// Rigid body component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RigidBody {
    /// Integration class.
    pub body_type: BodyType,
    /// Per-body multiplier on gravity. `1.0` = world default.
    pub gravity_scale: f32,
    /// Enable continuous-collision detection (more expensive, no tunneling).
    pub ccd_enabled: bool,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            gravity_scale: 1.0,
            ccd_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_dynamic() {
        let b = RigidBody::default();
        let s = ron::to_string(&b).expect("serialize");
        let back: RigidBody = ron::from_str(&s).expect("deserialize");
        assert_eq!(b, back);
    }

    #[test]
    fn round_trip_ron_kinematic() {
        let b = RigidBody {
            body_type: BodyType::Kinematic,
            gravity_scale: 0.0,
            ccd_enabled: true,
        };
        let s = ron::to_string(&b).expect("serialize");
        let back: RigidBody = ron::from_str(&s).expect("deserialize");
        assert_eq!(b, back);
    }
}
