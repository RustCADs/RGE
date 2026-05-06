//! [`Velocity`] — linear velocity component (m/s).

use serde::{Deserialize, Serialize};

/// Linear velocity in world space (m/s).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Velocity(pub [f32; 3]);

impl Velocity {
    /// Construct a velocity from per-axis components.
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
        let v = Velocity::new([1.0, -2.0, 3.5]);
        let s = ron::to_string(&v).expect("serialize");
        let back: Velocity = ron::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(Velocity::default(), Velocity::new([0.0, 0.0, 0.0]));
    }
}
