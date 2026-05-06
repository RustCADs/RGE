//! Wave-W01 local stub for the canonical ECS entity handle.

use serde::{Deserialize, Serialize};

/// Opaque ECS entity handle (W01-local stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Entity(pub u64);

impl Entity {
    /// Sentinel "no entity" value.
    pub const PLACEHOLDER: Entity = Entity(u64::MAX);
}

impl Default for Entity {
    fn default() -> Self {
        Self::PLACEHOLDER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let e = Entity(3);
        let s = ron::to_string(&e).expect("serialize");
        let back: Entity = ron::from_str(&s).expect("deserialize");
        assert_eq!(e, back);
    }
}
