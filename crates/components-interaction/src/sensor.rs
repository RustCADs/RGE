//! [`Sensor`] — zero-sized "collider does not affect physics integration" marker.
//!
//! Distinct from [`crate::Trigger`]: a `Sensor` does not emit events at all —
//! it exists for spatial-query systems (line-of-sight, AI vision cones,
//! script raycasts) that need a collider to register against without
//! changing how the rest of the world moves.

use serde::{Deserialize, Serialize};

/// Zero-sized "non-physical query collider" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sensor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = Sensor;
        let txt = ron::to_string(&s).expect("serialize");
        let back: Sensor = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }
}
