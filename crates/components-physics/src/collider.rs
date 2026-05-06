//! [`Collider`] — collider shape + material parameters.
//!
//! Per PLAN.md §1.5.1 the trigger-volume role pairs `Collider` + `Trigger`.
//! `Collider` itself is shared by all colliding entities; the marker family
//! (`Trigger`, `Sensor` from components-interaction) tags the role.

use serde::{Deserialize, Serialize};

/// Geometric collider shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ColliderShape {
    /// Axis-aligned box, full extents.
    Box {
        /// Full extents along X, Y, Z.
        extents: [f32; 3],
    },
    /// Sphere.
    Sphere {
        /// Radius, meters.
        radius: f32,
    },
    /// Capsule (cylinder with hemispherical caps).
    Capsule {
        /// Radius of the cylinder + caps.
        radius: f32,
        /// Length of the cylindrical mid-section, axis-aligned.
        height: f32,
    },
    /// Cylinder (no caps).
    Cylinder {
        /// Radius.
        radius: f32,
        /// Height along axis.
        height: f32,
    },
    /// Cone (apex pointing along +Y).
    Cone {
        /// Base radius.
        radius: f32,
        /// Apex height above base.
        height: f32,
    },
    /// Triangle mesh — references a tessellation in the asset store. Stored
    /// as a `u64` so the components crate stays free of asset-handle deps.
    Mesh {
        /// Asset id of the collision mesh.
        asset_id: u64,
    },
}

impl Default for ColliderShape {
    fn default() -> Self {
        Self::Box {
            extents: [1.0, 1.0, 1.0],
        }
    }
}

/// Collider component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Collider {
    /// Shape geometry.
    pub shape: ColliderShape,
    /// Coulomb friction coefficient.
    pub friction: f32,
    /// Coefficient of restitution (bounciness), 0..=1.
    pub restitution: f32,
    /// Density in kg/m^3 (used to derive mass when no [`crate::Mass`]
    /// component is present).
    pub density: f32,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::default(),
            friction: 0.5,
            restitution: 0.0,
            density: 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default_box() {
        let c = Collider::default();
        let s = ron::to_string(&c).expect("serialize");
        let back: Collider = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn round_trip_ron_capsule() {
        let c = Collider {
            shape: ColliderShape::Capsule {
                radius: 0.5,
                height: 1.8,
            },
            friction: 0.7,
            restitution: 0.1,
            density: 985.0,
        };
        let s = ron::to_string(&c).expect("serialize");
        let back: Collider = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn round_trip_ron_mesh() {
        let c = Collider {
            shape: ColliderShape::Mesh { asset_id: 0xc011 },
            friction: 0.3,
            restitution: 0.0,
            density: 2400.0,
        };
        let s = ron::to_string(&c).expect("serialize");
        let back: Collider = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }
}
