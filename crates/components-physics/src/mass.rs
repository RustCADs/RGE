//! [`Mass`] — explicit mass + inertia override component.
//!
//! When absent, mass is derived from `(Collider.density * Collider.shape
//! volume)` by the physics crate. When present, the explicit values win and
//! the collider density is ignored.

use serde::{Deserialize, Serialize};

/// Mass + diagonal inertia tensor override.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mass {
    /// Mass in kilograms. Must be > 0 for finite-mass bodies; sentinel
    /// `f32::INFINITY` represents an "infinite mass" override (equivalent
    /// to a kinematic body for force-receiving purposes).
    pub kg: f32,
    /// Diagonal of the inertia tensor in kg·m^2 (Ixx, Iyy, Izz). Off-axis
    /// terms are assumed zero — adequate for axis-aligned primitives;
    /// arbitrary triangle-mesh inertia is computed by the physics crate
    /// directly when this component is absent.
    pub inertia_diag: [f32; 3],
}

impl Default for Mass {
    fn default() -> Self {
        Self {
            kg: 1.0,
            inertia_diag: [1.0, 1.0, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let m = Mass {
            kg: 75.0,
            inertia_diag: [10.0, 5.0, 10.0],
        };
        let s = ron::to_string(&m).expect("serialize");
        let back: Mass = ron::from_str(&s).expect("deserialize");
        assert_eq!(m, back);
    }
}
