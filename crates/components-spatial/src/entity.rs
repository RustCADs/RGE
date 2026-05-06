//! Wave-W01 local stub for the canonical ECS entity handle.
//!
//! Promoted to `rge-kernel-types::Entity` by W02; this module is removed at that
//! point. Until then the W01 component types reference this newtype so that
//! `Parent(Entity)` etc. compile in isolation.

use serde::{Deserialize, Serialize};

/// Opaque ECS entity handle (W01-local stub — see module docs).
///
/// `u64` packs a generational arena index in W02; today it's just an opaque
/// integer to keep the `Parent(Entity)` shape stable across future kernel
/// changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Entity(pub u64);

impl Entity {
    /// Sentinel used when "no entity" must be representable in an owned slot.
    /// Real ECS code should use `Option<Entity>`; this exists for FFI / scratch
    /// buffers that want a `Copy` zero-init.
    pub const PLACEHOLDER: Entity = Entity(u64::MAX);

    /// Construct an entity handle from a raw integer.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw integer payload.
    #[inline]
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        self.0
    }
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
        let e = Entity(42);
        let s = ron::to_string(&e).expect("serialize");
        let back: Entity = ron::from_str(&s).expect("deserialize");
        assert_eq!(e, back);
    }

    #[test]
    fn placeholder_is_max() {
        assert_eq!(Entity::PLACEHOLDER.to_raw(), u64::MAX);
        assert_eq!(Entity::default(), Entity::PLACEHOLDER);
    }
}
