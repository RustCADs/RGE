//! [`Hidden`] — zero-sized "hidden right now" marker.
//!
//! Equivalent of `Visibility::Hidden`; carried as a marker so high-traffic
//! systems (the renderer's culler in particular) can use a fast `Without<Hidden>`
//! filter instead of reading the tri-state enum.

use serde::{Deserialize, Serialize};

/// Zero-sized "hidden" override marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hidden;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let h = Hidden;
        let s = ron::to_string(&h).expect("serialize");
        let back: Hidden = ron::from_str(&s).expect("deserialize");
        assert_eq!(h, back);
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<Hidden>(), 0);
    }
}
