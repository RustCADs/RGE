//! The [`World`]: the root container for all ECS state.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, HashMap};

use crate::archetype::Archetype;
use crate::change_detection::QueryFilter;
use crate::commands::Commands;
use crate::component::Component;
use crate::entity::{EntityId, EntityMut, EntityRef};
use crate::query::Query;
use crate::relations::RelationTag;
use crate::snapshot::SnapshotFns;

// ---------------------------------------------------------------------------
// ArchetypeLocation
// ---------------------------------------------------------------------------

/// The location of an entity: which archetype bucket it lives in, and which row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchetypeLocation {
    /// Index into `World::archetypes`.
    pub(crate) archetype_index: usize,
    /// Row within that archetype.
    pub(crate) row: usize,
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// The root ECS container.
///
/// Holds all entities, their component data, resources, and relation storage.
/// All mutation goes through `&mut World` — no interior mutability, no locks.
///
/// # Tick model
///
/// Every call to [`advance_tick`] increments the internal `change_tick` counter
/// and saves the previous value as `last_tick`.  Change-detection queries compare
/// per-slot ticks against `last_tick`: a slot is "changed" when its tick is
/// strictly greater than `last_tick`.
///
/// # Archetype strategy
///
/// This minimal implementation uses a **single catch-all archetype** rather than
/// a per-component-set archetype per entity.  This means:
/// - Queries iterate the full entity list even when most don't carry the target
///   component (they return `None` from `get` and are skipped).
/// - No archetype migration cost when inserting new components.
/// - Future optimisation: real per-component-set buckets with migration.
pub struct World {
    /// All entity data lives in archetypes.  Currently one archetype is used.
    pub(crate) archetypes: Vec<Archetype>,
    /// Maps [`EntityId`] → archetype location.
    pub(crate) entity_map: HashMap<EntityId, ArchetypeLocation>,
    /// Non-component shared state keyed by type.
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Deferred-mutation buffer.
    pub(crate) commands_buffer: Commands,
    /// Current world tick; bumped by [`advance_tick`].
    change_tick: u64,
    /// Tick at the end of the previous [`advance_tick`] call.
    last_tick: u64,
    /// Relation storages keyed by tag type id.
    relations: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Type-erased snapshot functions, keyed by [`TypeId`] in a [`BTreeMap`]
    /// for deterministic iteration order during serialization.
    pub(crate) snapshot_fns: BTreeMap<TypeId, SnapshotFns>,
}

impl World {
    /// Create an empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            archetypes: vec![Archetype::new()],
            entity_map: HashMap::new(),
            resources: HashMap::new(),
            commands_buffer: Commands::new(),
            change_tick: 1,
            last_tick: 0,
            relations: HashMap::new(),
            snapshot_fns: BTreeMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Tick
    // -----------------------------------------------------------------------

    /// The current world tick.
    #[must_use]
    pub fn current_tick(&self) -> u64 {
        self.change_tick
    }

    /// The tick at the end of the last [`advance_tick`] call.
    #[must_use]
    pub fn last_tick(&self) -> u64 {
        self.last_tick
    }

    /// Advance the world tick.
    ///
    /// After this call:
    /// - `current_tick()` is incremented by 1.
    /// - `last_tick()` equals the value of `current_tick()` before this call.
    /// - Any subsequent [`Changed<T>`](crate::change_detection::Changed) query
    ///   will only see mutations that happened **after** this point.
    pub fn advance_tick(&mut self) {
        self.last_tick = self.change_tick;
        self.change_tick += 1;
    }

    // -----------------------------------------------------------------------
    // Entity lifecycle
    // -----------------------------------------------------------------------

    /// Spawn a new entity with no components.  Returns its [`EntityId`].
    pub fn spawn(&mut self) -> EntityId {
        let id = EntityId::new();
        self.spawn_with_id(id);
        id
    }

    /// Spawn an entity with a pre-existing [`EntityId`].
    ///
    /// Used by the snapshot restore path to preserve original entity IDs
    /// across a serialize → restore cycle.  Calling this with an ID that is
    /// already live in the world is a logic error; the method emits a
    /// `tracing::warn` and returns without modifying the world.
    pub fn spawn_with_id(&mut self, id: EntityId) {
        if self.entity_map.contains_key(&id) {
            tracing::warn!(
                target: "rge::kernel::ecs::world",
                entity = %id,
                "spawn_with_id: entity already exists — no-op"
            );
            return;
        }
        // All entities go into archetype 0 (the single catch-all archetype).
        let arch = &mut self.archetypes[0];
        let row = arch.len();
        arch.push_entity(id);
        self.entity_map.insert(
            id,
            ArchetypeLocation {
                archetype_index: 0,
                row,
            },
        );
    }

    /// Insert a type-erased component value into `entity`.
    ///
    /// Used by the snapshot restore path. The `type_id` must correspond to a
    /// registered snapshot component type. No-op (with a `tracing::warn`) when
    /// the entity does not exist.
    pub(crate) fn insert_erased(
        &mut self,
        entity: EntityId,
        value: Box<dyn Any + Send + Sync>,
        type_id: TypeId,
    ) {
        let Some(loc) = self.entity_map.get(&entity).copied() else {
            tracing::warn!(
                target: "rge::kernel::ecs::world",
                entity = %entity,
                "insert_erased: entity not found — no-op"
            );
            return;
        };
        self.archetypes[loc.archetype_index].insert_erased(type_id, loc.row, value);
    }

    /// Spawn a new entity with one component.  Returns its [`EntityId`].
    pub fn spawn_with<C: Component>(&mut self, component: C) -> EntityId {
        let id = self.spawn();
        let loc = self.entity_map[&id];
        self.archetypes[loc.archetype_index].insert_component::<C>(loc.row, component);
        id
    }

    /// Despawn an entity and remove all its components.
    ///
    /// Returns `true` if the entity existed, `false` if it was absent.
    /// Emits a `tracing::warn` for absent entities.
    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let Some(loc) = self.entity_map.remove(&entity) else {
            tracing::warn!("despawn: entity {entity} not found — no-op");
            return false;
        };
        let arch = &mut self.archetypes[loc.archetype_index];
        let swapped_out = arch.swap_remove_entity(loc.row);
        debug_assert_eq!(swapped_out, entity);

        // If a different entity was moved into `loc.row` by swap_remove,
        // update its location in the map.
        if loc.row < arch.len() {
            let moved_entity = arch.entities()[loc.row];
            if let Some(moved_loc) = self.entity_map.get_mut(&moved_entity) {
                moved_loc.row = loc.row;
            }
        }
        true
    }

    /// Total number of live entities.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entity_map.len()
    }

    // -----------------------------------------------------------------------
    // Immutable entity access
    // -----------------------------------------------------------------------

    /// Return an immutable handle to `entity`, or `None` if it does not exist.
    #[must_use]
    pub fn entity(&self, entity: EntityId) -> Option<EntityRef<'_>> {
        let loc = self.entity_map.get(&entity)?;
        let arch = &self.archetypes[loc.archetype_index];
        Some(EntityRef::new(entity, arch, loc.row))
    }

    // -----------------------------------------------------------------------
    // Mutable entity access
    // -----------------------------------------------------------------------

    /// Return a mutable handle to `entity`, or `None` if it does not exist.
    pub fn entity_mut(&mut self, entity: EntityId) -> Option<EntityMut<'_>> {
        let loc = *self.entity_map.get(&entity)?;
        let tick = self.change_tick;
        let arch = &mut self.archetypes[loc.archetype_index];
        Some(EntityMut::new_with_tick(entity, arch, loc.row, tick))
    }

    // -----------------------------------------------------------------------
    // Component mutation
    // -----------------------------------------------------------------------

    /// Insert (or replace) a component on `entity`.
    ///
    /// No-op with a `tracing::warn` if `entity` does not exist.
    pub fn insert<C: Component>(&mut self, entity: EntityId, component: C) {
        let Some(loc) = self.entity_map.get(&entity).copied() else {
            tracing::warn!("insert: entity {entity} not found — no-op");
            return;
        };
        self.archetypes[loc.archetype_index].insert_component::<C>(loc.row, component);
    }

    /// Remove and return component `C` from `entity`.
    ///
    /// Returns `None` when the entity is absent or does not carry `C`.
    pub fn remove<C: Component>(&mut self, entity: EntityId) -> Option<C> {
        let loc = self.entity_map.get(&entity).copied()?;
        self.archetypes[loc.archetype_index].remove_component::<C>(loc.row)
    }

    /// Replace component `C` on `entity` with a new value, returning the old value.
    ///
    /// If the entity does not carry `C`, `component` is inserted and `None` is returned.
    /// No-op (returning `None`) when the entity is absent.
    pub fn replace<C: Component>(&mut self, entity: EntityId, component: C) -> Option<C> {
        let loc = self.entity_map.get(&entity).copied()?;
        let arch = &mut self.archetypes[loc.archetype_index];
        let old = arch.remove_component::<C>(loc.row);
        arch.insert_component::<C>(loc.row, component);
        old
    }

    // -----------------------------------------------------------------------
    // Query
    // -----------------------------------------------------------------------

    /// Iterate over all entities that carry component `F::Component`.
    ///
    /// When `F` is [`Changed<T>`](crate::change_detection::Changed), only
    /// entities mutated since the last [`advance_tick`] are yielded.
    ///
    /// Returns a [`Query`] iterator of `(EntityId, &F::Component)` pairs.
    pub fn query<F: QueryFilter>(&self) -> Query<'_, F::Component> {
        let filter_type_id = F::filter_type_id();
        let last_tick = self.last_tick;
        Query::new(&self.archetypes, filter_type_id, last_tick)
    }

    // -----------------------------------------------------------------------
    // Resources
    // -----------------------------------------------------------------------

    /// Insert a resource (non-entity global state).
    ///
    /// Replaces any existing resource of the same type.
    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, resource: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(resource));
    }

    /// Borrow a resource, wrapped in [`Res<R>`](crate::resource::Res).
    ///
    /// Returns `None` when no resource of type `R` has been inserted.
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(&self) -> Option<crate::resource::Res<'_, R>> {
        let boxed = self.resources.get(&TypeId::of::<R>())?;
        boxed.downcast_ref::<R>().map(crate::resource::Res::new)
    }

    /// Remove and return a resource.
    pub fn remove_resource<R: Send + Sync + 'static>(&mut self) -> Option<R> {
        let boxed = self.resources.remove(&TypeId::of::<R>())?;
        boxed.downcast::<R>().ok().map(|b| *b)
    }

    // -----------------------------------------------------------------------
    // Commands (deferred mutation)
    // -----------------------------------------------------------------------

    /// Apply all pending [`Commands`] to the world, then clear the buffer.
    pub fn flush_commands(&mut self) {
        // Drain the commands buffer without holding a reference to self.
        let cmds = std::mem::take(&mut self.commands_buffer);
        for cmd in cmds.into_ops() {
            cmd.apply(self);
        }
    }

    /// Borrow the [`Commands`] buffer for deferred mutation.
    ///
    /// Mutations are not visible until [`flush_commands`] is called.
    pub fn commands(&mut self) -> &mut Commands {
        &mut self.commands_buffer
    }

    // -----------------------------------------------------------------------
    // Relation storage
    // -----------------------------------------------------------------------

    /// Borrow the relation storage for `R`, creating it if absent.
    ///
    /// # Panics
    ///
    /// Panics if the internal type map contains a storage for `R` that was
    /// registered with a different concrete type.  This is an invariant
    /// violation that should never occur in practice.
    pub fn relations_mut<R: RelationTag>(&mut self) -> &mut R::Storage {
        self.relations
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Box::new(R::Storage::default()))
            .downcast_mut::<R::Storage>()
            .expect("relation storage type mismatch — should never happen")
    }

    /// Borrow the relation storage for `R` immutably.
    #[must_use]
    pub fn relations<R: RelationTag>(&self) -> Option<&R::Storage> {
        self.relations
            .get(&TypeId::of::<R>())
            .and_then(|b| b.downcast_ref::<R::Storage>())
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("entity_count", &self.entity_map.len())
            .field("archetype_count", &self.archetypes.len())
            .field("change_tick", &self.change_tick)
            .field("last_tick", &self.last_tick)
            .field("snapshot_component_count", &self.snapshot_fns.len())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    #[derive(Debug, Clone, PartialEq)]
    struct Pos {
        x: f32,
    }
    impl Component for Pos {}

    #[derive(Debug, Clone, PartialEq)]
    struct Vel {
        dx: f32,
    }
    impl Component for Vel {}

    #[test]
    fn spawn_despawn() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        assert_eq!(w.entity_count(), 2);
        assert!(w.despawn(a));
        assert_eq!(w.entity_count(), 1);
        assert!(!w.despawn(a), "second despawn should return false");
        assert!(w.despawn(b));
        assert_eq!(w.entity_count(), 0);
    }

    #[test]
    fn spawn_with_and_query() {
        let mut w = World::new();
        w.spawn_with(Pos { x: 1.0 });
        w.spawn_with(Pos { x: 2.0 });
        let xs: Vec<f32> = w.query::<Pos>().map(|(_, p)| p.x).collect();
        assert_eq!(xs.len(), 2);
    }

    #[test]
    fn insert_remove_replace() {
        let mut w = World::new();
        let id = w.spawn();
        w.insert(id, Pos { x: 1.0 });
        assert_eq!(w.entity(id).unwrap().get::<Pos>(), Some(&Pos { x: 1.0 }));
        let old = w.replace(id, Pos { x: 2.0 });
        assert_eq!(old, Some(Pos { x: 1.0 }));
        assert_eq!(w.entity(id).unwrap().get::<Pos>(), Some(&Pos { x: 2.0 }));
        let removed = w.remove::<Pos>(id);
        assert_eq!(removed, Some(Pos { x: 2.0 }));
        assert_eq!(w.entity(id).unwrap().get::<Pos>(), None);
    }

    #[test]
    fn changed_query_filters() {
        use crate::change_detection::Changed;
        let mut w = World::new();
        let a = w.spawn_with(Pos { x: 0.0 });
        let b = w.spawn_with(Pos { x: 0.0 });
        w.advance_tick();

        // Mutate only `a`.
        if let Some(mut em) = w.entity_mut(a) {
            if let Some(mut p) = em.get_mut::<Pos>() {
                p.x = 99.0;
            }
        }

        let changed: Vec<EntityId> = w.query::<Changed<Pos>>().map(|(id, _)| id).collect();
        assert!(changed.contains(&a));
        assert!(!changed.contains(&b));
        assert_eq!(changed.len(), 1);

        w.advance_tick();
        let still: Vec<EntityId> = w.query::<Changed<Pos>>().map(|(id, _)| id).collect();
        assert!(still.is_empty(), "advance_tick should clear changed set");
    }

    #[test]
    fn entity_mut_insert_remove() {
        let mut w = World::new();
        let id = w.spawn();
        {
            let mut em = w.entity_mut(id).unwrap();
            em.insert(Vel { dx: 3.0 });
        }
        assert_eq!(w.entity(id).unwrap().get::<Vel>(), Some(&Vel { dx: 3.0 }));
        {
            let mut em = w.entity_mut(id).unwrap();
            em.remove::<Vel>();
        }
        assert_eq!(w.entity(id).unwrap().get::<Vel>(), None);
    }

    #[test]
    fn entity_id_unique() {
        // Verify that all spawned EntityIds are distinct.
        // ULID within the same millisecond uses random low bits so ordering
        // is not guaranteed; only uniqueness is required.
        use std::collections::HashSet;
        let mut w = World::new();
        let ids: Vec<EntityId> = (0..100).map(|_| w.spawn()).collect();
        let unique: HashSet<EntityId> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "all EntityIds must be distinct");
    }
}
