//! [`EntityId`], [`EntityRef`], and [`EntityMut`].

use ulid::Ulid;

use crate::change_detection::Mut;
use crate::component::{Component, ComponentId};

// ---------------------------------------------------------------------------
// EntityId
// ---------------------------------------------------------------------------

/// Opaque, monotonically-increasing handle to a world entity.
///
/// Backed by a [ULID](https://github.com/ulid/spec) so that IDs are:
/// - globally unique across world instances,
/// - monotonically increasing (time-ordered),
/// - human-readable (Crockford base-32).
///
/// Two successive calls to [`EntityId::new`] in the same millisecond are still
/// unique because the ULID spec includes 80 bits of randomness in the low bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(Ulid);

impl EntityId {
    /// Generate a fresh, unique [`EntityId`].
    #[must_use]
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Reconstruct an [`EntityId`] from a previously obtained [`Ulid`] value.
    ///
    /// Intended for deserialization and snapshot-restore paths only.
    /// The caller is responsible for ensuring the `Ulid` originated from a
    /// valid [`EntityId`] (e.g. obtained via [`EntityId::ulid`]).
    #[must_use]
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Return the raw [`Ulid`] value.
    #[must_use]
    pub fn ulid(self) -> Ulid {
        self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// EntityRef — immutable entity handle
// ---------------------------------------------------------------------------

/// Immutable view into a single entity and its components.
///
/// Obtained via [`World::entity`](crate::world::World::entity).
/// All accessor methods take `&self`; no mutation is possible through this handle.
pub struct EntityRef<'w> {
    id: EntityId,
    archetype: &'w crate::archetype::Archetype,
    row: usize,
}

impl<'w> EntityRef<'w> {
    /// Construct an [`EntityRef`] from its archetype + row.
    #[must_use]
    pub(crate) fn new(
        id: EntityId,
        archetype: &'w crate::archetype::Archetype,
        row: usize,
    ) -> Self {
        Self { id, archetype, row }
    }

    /// The entity's stable identifier.
    #[must_use]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Borrow a component of type `C`, if present.
    #[must_use]
    pub fn get<C: Component>(&self) -> Option<&C> {
        self.archetype.get::<C>(self.row)
    }

    /// Returns `true` when the entity carries a component of type `C`.
    #[must_use]
    pub fn contains<C: Component>(&self) -> bool {
        self.archetype.has_component(ComponentId::of::<C>())
    }
}

// ---------------------------------------------------------------------------
// EntityMut — mutable entity handle
// ---------------------------------------------------------------------------

/// Mutable handle to a single entity.
///
/// Obtained via [`World::entity_mut`](crate::world::World::entity_mut).
/// Mutations through this handle update component data in-place and bump
/// the archetype's per-component change counter when the returned [`Mut<T>`]
/// guard is dropped.
pub struct EntityMut<'w> {
    id: EntityId,
    archetype: &'w mut crate::archetype::Archetype,
    row: usize,
    /// World tick captured at construction; forwarded to [`Mut`] guards.
    world_tick: u64,
}

impl<'w> EntityMut<'w> {
    /// Construct an [`EntityMut`] with an explicit world tick.
    #[must_use]
    pub(crate) fn new_with_tick(
        id: EntityId,
        archetype: &'w mut crate::archetype::Archetype,
        row: usize,
        world_tick: u64,
    ) -> Self {
        Self {
            id,
            archetype,
            row,
            world_tick,
        }
    }

    /// The entity's stable identifier.
    #[must_use]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Borrow a component immutably, if present.
    #[must_use]
    pub fn get<C: Component>(&self) -> Option<&C> {
        self.archetype.get::<C>(self.row)
    }

    /// Borrow a component mutably.
    ///
    /// Returns a [`Mut<C>`] guard whose `Drop` impl bumps the archetype's
    /// change counter for `C` to the current world tick.
    /// Returns `None` when the entity does not carry `C`.
    pub fn get_mut<C: Component>(&mut self) -> Option<Mut<'_, C>> {
        let tick = self.world_tick;
        self.archetype
            .get_mut::<C>(self.row)
            .map(|g| g.with_world_tick(tick))
    }

    /// Insert (or replace) a component of type `C` on this entity.
    pub fn insert<C: Component>(&mut self, component: C) {
        self.archetype.insert_component::<C>(self.row, component);
    }

    /// Remove and return a component of type `C`, if present.
    pub fn remove<C: Component>(&mut self) -> Option<C> {
        self.archetype.remove_component::<C>(self.row)
    }
}
