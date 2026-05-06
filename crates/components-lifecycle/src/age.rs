//! [`Age`] — tick counter since the entity was spawned.
//!
//! Incremented by the lifecycle system once per simulation tick. Stored as
//! `u32` (~2 years at 60 Hz) — long enough for any practical TTL without
//! introducing a `u64` cache pressure cost on hot ECS columns.

use serde::{Deserialize, Serialize};

/// Tick count since spawn.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Age(pub u32);

impl Age {
    /// Construct an Age from a raw tick count.
    #[inline]
    #[must_use]
    pub const fn from_ticks(ticks: u32) -> Self {
        Self(ticks)
    }

    /// Number of ticks elapsed.
    #[inline]
    #[must_use]
    pub const fn ticks(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let a = Age::from_ticks(42);
        let txt = ron::to_string(&a).expect("serialize");
        let back: Age = ron::from_str(&txt).expect("deserialize");
        assert_eq!(a, back);
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(Age::default().ticks(), 0);
    }
}
