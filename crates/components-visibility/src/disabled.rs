//! [`Disabled`] — zero-sized marker that freezes simulation.
//!
//! Distinct from [`crate::Hidden`]: a `Disabled` entity still draws but does
//! not tick (no physics integration, no animation advance, no script
//! callbacks). Editor uses this to "park" half-built entities while the
//! author iterates on a sibling.

use serde::{Deserialize, Serialize};

/// Zero-sized "do not simulate" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Disabled;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let d = Disabled;
        let s = ron::to_string(&d).expect("serialize");
        let back: Disabled = ron::from_str(&s).expect("deserialize");
        assert_eq!(d, back);
    }
}
