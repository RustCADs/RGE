//! [`Joint`] — physics joint between two entities.
//!
//! Anchor points are local-space (each entity's frame). The wave-W11 physics
//! crate translates these into rapier joint definitions.

use serde::{Deserialize, Serialize};

use crate::Entity;

/// Joint kind.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JointKind {
    /// Pinned at a single point. 3 rotational `DOF`, 0 linear.
    #[default]
    Spherical,
    /// Hinge — 1 rotational `DOF` along an axis.
    Revolute,
    /// Slider — 1 linear `DOF` along an axis.
    Prismatic,
    /// Fully rigid attachment — 0 `DOF` (used for compound bodies that the
    /// authoring tool wants to keep as separate entities).
    Fixed,
}

/// Joint component.
///
/// Sits on a "joint entity" that references the two bodies it constrains.
/// Stored separately from the constrained bodies so the same joint can be
/// re-targeted without rebuilding the rapier handle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    /// Joint type.
    pub kind: JointKind,
    /// First constrained body.
    pub body_a: Entity,
    /// Second constrained body.
    pub body_b: Entity,
    /// Anchor in `body_a`'s local frame.
    pub anchor_a: [f32; 3],
    /// Anchor in `body_b`'s local frame.
    pub anchor_b: [f32; 3],
}

impl Default for Joint {
    fn default() -> Self {
        Self {
            kind: JointKind::Spherical,
            body_a: Entity::PLACEHOLDER,
            body_b: Entity::PLACEHOLDER,
            anchor_a: [0.0, 0.0, 0.0],
            anchor_b: [0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default() {
        let j = Joint::default();
        let s = ron::to_string(&j).expect("serialize");
        let back: Joint = ron::from_str(&s).expect("deserialize");
        assert_eq!(j, back);
    }

    #[test]
    fn round_trip_ron_revolute() {
        let j = Joint {
            kind: JointKind::Revolute,
            body_a: Entity(1),
            body_b: Entity(2),
            anchor_a: [0.5, 0.0, 0.0],
            anchor_b: [-0.5, 0.0, 0.0],
        };
        let s = ron::to_string(&j).expect("serialize");
        let back: Joint = ron::from_str(&s).expect("deserialize");
        assert_eq!(j, back);
    }
}
