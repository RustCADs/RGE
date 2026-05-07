//! [`Parent`] — relation component pointing at the entity above this one in
//! the scene tree.
//!
//! Stored separately from the actual `kernel/ecs::TreeRelationStorage` index so
//! that scene RON files can carry the relation in plain component form;
//! transform propagation systems read both and assert agreement.

use serde::{Deserialize, Serialize};

use crate::Entity;

/// "My parent in the scene tree is this entity."
///
/// Absent on root entities. Pair with [`crate::ChildOf`] (zero-sized marker)
/// when the entity *role* — not just the relation — is "I exist as a child of
/// some parent" (see PLAN.md §1.5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Parent(pub Entity);

impl Parent {
    /// Construct a Parent component referring to `entity`.
    #[inline]
    #[must_use]
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    /// Borrow the parent entity handle.
    #[inline]
    #[must_use]
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "accessor takes `&self` deliberately so call sites read like a borrow even though `Entity` is currently `Copy`; if a future revision threads non-`Copy` payloads (e.g. generation-counted handles with extra metadata) the signature stays stable"
    )]
    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let p = Parent::new(Entity(7));
        let s = ron::to_string(&p).expect("serialize");
        let back: Parent = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn entity_accessor_returns_inner() {
        let p = Parent::new(Entity(123));
        assert_eq!(p.entity(), Entity(123));
    }
}
