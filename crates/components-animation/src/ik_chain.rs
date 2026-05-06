//! [`IkChain`] — inverse kinematics chain configuration on a skeletal entity.
//!
//! Names a target entity that the chain should point its end-effector at.
//! The actual solver lives in `crates/anim-ik` (W11 sibling); this component
//! authors which bones participate, the iteration count, and target.

use serde::{Deserialize, Serialize};

use crate::Entity;

/// IK chain configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IkChain {
    /// Index (into the bound skeleton) of the chain's root bone.
    pub root_bone: u16,
    /// Index of the chain's end-effector bone.
    pub effector_bone: u16,
    /// Number of bones in the chain (must be at least 2 — root and effector
    /// itself; intermediate bones are inferred via the skeleton hierarchy).
    pub chain_length: u16,
    /// Solver iteration count (FABRIK / CCD).
    pub iterations: u16,
    /// Target entity the effector should reach for. [`Entity::PLACEHOLDER`]
    /// disables the chain without removing the component.
    pub target: Entity,
    /// Solver weight, 0..=1. `0.0` = pass-through; `1.0` = full solve.
    pub weight: f32,
}

impl Default for IkChain {
    fn default() -> Self {
        Self {
            root_bone: 0,
            effector_bone: 0,
            chain_length: 2,
            iterations: 8,
            target: Entity::PLACEHOLDER,
            weight: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default() {
        let c = IkChain::default();
        let s = ron::to_string(&c).expect("serialize");
        let back: IkChain = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn round_trip_ron_arm() {
        let c = IkChain {
            root_bone: 4,
            effector_bone: 9,
            chain_length: 4,
            iterations: 16,
            target: Entity(42),
            weight: 0.75,
        };
        let s = ron::to_string(&c).expect("serialize");
        let back: IkChain = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }
}
