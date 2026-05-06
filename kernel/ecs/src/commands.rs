//! Deferred-mutation buffer: [`Commands`].
//!
//! Mutations enqueued through [`Commands`] are **not** applied immediately;
//! they are queued and applied in order when [`World::flush_commands`] is
//! called.  This is the only deferred mutation path in the ECS substrate.

use crate::component::Component;
use crate::entity::EntityId;
use crate::world::World;

// ---------------------------------------------------------------------------
// CommandOp — type-erased deferred operation
// ---------------------------------------------------------------------------

/// A single deferred world-mutation operation.
///
/// `Box<dyn CommandOp>` is the unit stored in the [`Commands`] buffer.
pub(crate) trait CommandOp: Send + Sync {
    /// Apply this operation to `world`.
    fn apply(self: Box<Self>, world: &mut World);
}

// ---- Concrete ops ----------------------------------------------------------

struct SpawnWithOp<C: Component> {
    component: C,
    /// Optionally record the spawned id somewhere after flush.
    /// (Not exposed in the minimal API; future extension point.)
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Component> CommandOp for SpawnWithOp<C> {
    fn apply(self: Box<Self>, world: &mut World) {
        world.spawn_with(self.component);
    }
}

struct InsertOp<C: Component> {
    entity: EntityId,
    component: C,
}

impl<C: Component> CommandOp for InsertOp<C> {
    fn apply(self: Box<Self>, world: &mut World) {
        world.insert(self.entity, self.component);
    }
}

struct RemoveOp<C: Component> {
    entity: EntityId,
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Component> CommandOp for RemoveOp<C> {
    fn apply(self: Box<Self>, world: &mut World) {
        world.remove::<C>(self.entity);
    }
}

struct DespawnOp {
    entity: EntityId,
}

impl CommandOp for DespawnOp {
    fn apply(self: Box<Self>, world: &mut World) {
        world.despawn(self.entity);
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Deferred-mutation buffer flushed by [`World::flush_commands`].
///
/// All mutations are applied in **insertion order** when `flush_commands` is
/// called.  Between enqueue and flush, no mutation is visible to queries.
///
/// # Example
///
/// ```rust
/// # use rge_kernel_ecs::{World, Component};
/// # #[derive(Debug)] struct Marker;
/// # impl Component for Marker {}
/// let mut world = World::new();
/// world.commands().spawn_with(Marker);
/// // Marker entity is NOT visible yet.
/// assert_eq!(world.entity_count(), 0);
/// world.flush_commands();
/// // Now it is.
/// assert_eq!(world.entity_count(), 1);
/// ```
pub struct Commands {
    ops: Vec<Box<dyn CommandOp>>,
}

impl Commands {
    /// Create an empty [`Commands`] buffer.
    #[must_use]
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Enqueue a `spawn_with` command.
    pub fn spawn_with<C: Component>(&mut self, component: C) {
        self.ops.push(Box::new(SpawnWithOp {
            component,
            _phantom: std::marker::PhantomData,
        }));
    }

    /// Enqueue an `insert` command.
    pub fn insert<C: Component>(&mut self, entity: EntityId, component: C) {
        self.ops.push(Box::new(InsertOp { entity, component }));
    }

    /// Enqueue a `remove` command.
    pub fn remove<C: Component>(&mut self, entity: EntityId) {
        self.ops.push(Box::new(RemoveOp::<C> {
            entity,
            _phantom: std::marker::PhantomData,
        }));
    }

    /// Enqueue a `despawn` command.
    pub fn despawn(&mut self, entity: EntityId) {
        self.ops.push(Box::new(DespawnOp { entity }));
    }

    /// Returns `true` when no commands are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Number of pending commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Drain all pending operations (consumed by [`World::flush_commands`]).
    pub(crate) fn into_ops(self) -> Vec<Box<dyn CommandOp>> {
        self.ops
    }
}

impl Default for Commands {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    #[derive(Debug, PartialEq)]
    struct Mark(u32);
    impl Component for Mark {}

    #[test]
    fn spawn_deferred() {
        let mut world = World::new();
        world.commands().spawn_with(Mark(1));
        assert_eq!(world.entity_count(), 0, "not yet applied");
        world.flush_commands();
        assert_eq!(world.entity_count(), 1, "applied after flush");
    }

    #[test]
    fn insert_deferred() {
        let mut world = World::new();
        let id = world.spawn();
        world.commands().insert(id, Mark(42));
        assert_eq!(world.entity(id).unwrap().get::<Mark>(), None);
        world.flush_commands();
        assert_eq!(world.entity(id).unwrap().get::<Mark>(), Some(&Mark(42)));
    }

    #[test]
    fn despawn_deferred() {
        let mut world = World::new();
        let id = world.spawn();
        world.commands().despawn(id);
        assert_eq!(world.entity_count(), 1, "still alive before flush");
        world.flush_commands();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn ordering_preserved() {
        // Commands must apply in enqueue order.
        let mut world = World::new();
        let id = world.spawn();
        world.commands().insert(id, Mark(1));
        world.commands().insert(id, Mark(2)); // replaces
        world.flush_commands();
        assert_eq!(world.entity(id).unwrap().get::<Mark>(), Some(&Mark(2)));
    }

    #[test]
    fn buffer_empty_after_flush() {
        let mut world = World::new();
        world.commands().spawn_with(Mark(0));
        world.flush_commands();
        assert!(world.commands().is_empty());
    }
}
