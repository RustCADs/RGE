//! `editor_state::selection` — entity selection set.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15).

use std::collections::BTreeSet;

use rge_kernel_ecs::EntityId;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EntityId serde helpers
//
// `rge-kernel-ecs` does not enable `ulid`'s optional `serde` feature, so
// `EntityId` has no `Serialize`/`Deserialize` impl.  We bridge the gap by
// serialising the underlying `u128` value (Ulid's canonical numeric repr).
// We enable `ulid/serde` in *this* crate's Cargo.toml so that `ulid::Ulid`
// itself picks up the impls; we then go through `EntityId::ulid()` /
// `Ulid::from` conversions for the public API.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// EntityId serde helpers
//
// `rge-kernel-ecs` does not enable `ulid`'s optional `serde` feature, so
// `EntityId` has no `Serialize`/`Deserialize` impl.  We bridge the gap by
// serialising via the `Ulid` value (which does implement serde when the
// `ulid/serde` feature is enabled in *this* crate's Cargo.toml).
// Reconstruction uses `EntityId::from_ulid`.
// ---------------------------------------------------------------------------

/// Serde-transparent newtype used only inside the `BTreeSet` serialisation.
/// Ordering is preserved: `EntityId: Ord` matches `Ulid: Ord` (ULID order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct EntityIdSerde(ulid::Ulid);

impl From<EntityId> for EntityIdSerde {
    fn from(id: EntityId) -> Self {
        Self(id.ulid())
    }
}

impl From<EntityIdSerde> for EntityId {
    fn from(s: EntityIdSerde) -> Self {
        EntityId::from_ulid(s.0)
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// The set of entities the user has selected. Coordination state — does NOT
/// own component data; only references entities by ID.
///
/// Backed by `BTreeSet<EntityId>` for deterministic iteration order
/// (required for inspector display + audit-log recording).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Selection {
    entities: BTreeSet<EntityId>,
}

// Manual Serialize/Deserialize: round-trip via BTreeSet<EntityIdSerde>.
impl Serialize for Selection {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let set: BTreeSet<EntityIdSerde> = self
            .entities
            .iter()
            .copied()
            .map(EntityIdSerde::from)
            .collect();
        set.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let set = BTreeSet::<EntityIdSerde>::deserialize(d)?;
        Ok(Self {
            entities: set.into_iter().map(EntityId::from).collect(),
        })
    }
}

impl Selection {
    /// Construct an empty selection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entity to the selection. Returns `true` if newly added.
    pub fn add(&mut self, entity: EntityId) -> bool {
        self.entities.insert(entity)
    }

    /// Remove an entity from the selection. Returns `true` if it was present.
    pub fn remove(&mut self, entity: EntityId) -> bool {
        self.entities.remove(&entity)
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.entities.clear();
    }

    /// Selection size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// True if no entity is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// True if `entity` is in the selection.
    #[must_use]
    pub fn contains(&self, entity: EntityId) -> bool {
        self.entities.contains(&entity)
    }

    /// Iterate selected entity IDs in deterministic ascending order.
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.iter().copied()
    }

    /// Replace the entire selection set.
    pub fn replace_with<I: IntoIterator<Item = EntityId>>(&mut self, entities: I) {
        self.entities = entities.into_iter().collect();
    }

    /// Toggle: add if absent, remove if present. Returns the new membership.
    pub fn toggle(&mut self, entity: EntityId) -> bool {
        if self.entities.contains(&entity) {
            self.entities.remove(&entity);
            false
        } else {
            self.entities.insert(entity);
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> EntityId {
        EntityId::new()
    }

    #[test]
    fn add_is_idempotent() {
        let mut s = Selection::new();
        let e = id();
        assert!(s.add(e), "first add must return true");
        assert!(!s.add(e), "duplicate add must return false");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn remove_returns_presence() {
        let mut s = Selection::new();
        let e = id();
        assert!(!s.remove(e), "remove of absent entity is false");
        s.add(e);
        assert!(s.remove(e), "remove of present entity is true");
        assert!(s.is_empty());
    }

    #[test]
    fn contains_reflects_membership() {
        let mut s = Selection::new();
        let e = id();
        assert!(!s.contains(e));
        s.add(e);
        assert!(s.contains(e));
    }

    #[test]
    fn iter_is_sorted() {
        // Create three IDs in rapid succession; ULIDs are time-ordered so
        // they will compare in creation order (all within same ms, random bits differ).
        // We verify the iterator returns them in the BTreeSet's natural order.
        let mut s = Selection::new();
        let ids: Vec<EntityId> = (0..5).map(|_| id()).collect();
        // Insert in reverse.
        for e in ids.iter().rev() {
            s.add(*e);
        }
        let collected: Vec<EntityId> = s.iter().collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(collected, sorted, "iter must match BTreeSet sorted order");
    }

    #[test]
    fn toggle_round_trips() {
        let mut s = Selection::new();
        let e = id();
        assert!(s.toggle(e), "first toggle adds, returns true");
        assert!(s.contains(e));
        assert!(!s.toggle(e), "second toggle removes, returns false");
        assert!(!s.contains(e));
    }

    #[test]
    fn replace_with_fully_replaces() {
        let mut s = Selection::new();
        let old = id();
        s.add(old);
        let new_ids: Vec<EntityId> = (0..3).map(|_| id()).collect();
        s.replace_with(new_ids.iter().copied());
        assert_eq!(s.len(), 3);
        assert!(!s.contains(old), "old entity must be gone");
        for e in &new_ids {
            assert!(s.contains(*e));
        }
    }

    #[test]
    fn clear_empties_selection() {
        let mut s = Selection::new();
        s.add(id());
        s.add(id());
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn default_is_empty() {
        let s = Selection::default();
        assert!(s.is_empty());
    }
}
