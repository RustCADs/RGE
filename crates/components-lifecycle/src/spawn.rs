//! [`Spawn`] — zero-sized "born this tick" marker.
//!
//! Stripped at end-of-frame by the lifecycle system. Systems that want to
//! run only the first tick of an entity's life filter `With<Spawn>`.

use serde::{Deserialize, Serialize};

/// Zero-sized "spawned this tick" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Spawn;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let s = Spawn;
        let txt = ron::to_string(&s).expect("serialize");
        let back: Spawn = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<Spawn>(), 0);
    }
}
