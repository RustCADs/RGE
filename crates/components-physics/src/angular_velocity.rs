//! [`AngularVelocity`] — angular velocity component (rad/s).

use serde::{Deserialize, Serialize};

/// Angular velocity in world space (rad/s, axis-angle vector).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(transparent)]
pub struct AngularVelocity(pub [f32; 3]);

impl AngularVelocity {
    /// Construct an angular velocity from per-axis components.
    #[inline]
    #[must_use]
    pub const fn new(v: [f32; 3]) -> Self {
        Self(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let a = AngularVelocity::new([0.0, std::f32::consts::PI, 0.0]);
        let s = ron::to_string(&a).expect("serialize");
        let back: AngularVelocity = ron::from_str(&s).expect("deserialize");
        assert_eq!(a, back);
    }
}
