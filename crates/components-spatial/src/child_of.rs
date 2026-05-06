//! [`ChildOf`] — zero-sized role marker for "this entity participates as a
//! scene-tree child".
//!
//! Distinct from [`crate::Parent`], which carries the actual link. Some
//! systems (selection / hierarchy queries / archetype matching) only need the
//! presence of the marker; using `ChildOf` instead of `With<Parent>` keeps
//! those filters from accidentally matching prefab-template ghosts that have
//! a `Parent` for replication-context but should not be drawn as scene-tree
//! children.

use serde::{Deserialize, Serialize};

/// Zero-sized marker — the entity is rendered as a child in the scene tree.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChildOf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let c = ChildOf;
        let s = ron::to_string(&c).expect("serialize");
        let back: ChildOf = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<ChildOf>(), 0);
    }
}
