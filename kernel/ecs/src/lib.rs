//! `rge-kernel-ecs` — RGE Tier-1 kernel: entity/component/system substrate.
//!
//! Failure class: recoverable
//!
//! Minimal ECS substrate per [IMPLEMENTATION.md](../../plans/IMPLEMENTATION.md) Phase 2.1.
//! Per [PLAN.md §1.2](../../plans/PLAN.md).
//!
//! # Architecture
//!
//! Entities are identified by [`EntityId`] (ULID-based, monotonically increasing).
//! Components are stored in per-archetype columns (`Vec<Box<dyn Any>>`).
//! Change detection uses per-archetype generation counters bumped by [`Mut<T>`] on `Drop`.
//!
//! # Safety
//!
//! `unsafe_code = forbid`.  Column storage uses `Vec<Box<dyn Any + Send + Sync>>` instead
//! of the traditional type-erased pointer dance.  This sacrifices cache linearity in
//! exchange for full safe-Rust compliance.  A future optimisation task can replace columns
//! with `unsafe` typed slabs behind a dedicated safety proof.

pub mod archetype;
pub mod change_detection;
pub mod commands;
pub mod component;
pub mod entity;
mod internals;
pub mod participate;
pub mod query;
pub mod relations;
pub mod resource;
pub mod scheduler_bridge;
pub mod snapshot;
pub mod storage;
pub mod world;

pub use change_detection::{Changed, Mut};
pub use commands::Commands;
pub use component::{Component, ComponentId};
pub use entity::{EntityId, EntityMut, EntityRef};
pub use participate::{ParticipantId, ParticipateError, PieSnapshot, SnapshotParticipate};
pub use query::Query;
pub use relations::{bone_of, parent_of, BoneOf, ParentOf};
pub use resource::Res;
pub use snapshot::{SnapshotComponent, SnapshotError};
pub use storage::{DenseLinearRelationStorage, SparseRelationStorage, TreeRelationStorage};
pub use world::World;

// ---------------------------------------------------------------------------
// Free-function mutation helpers
// (These are the symbols the command-bus lint flags when imported from
// `kernel_ecs::` outside `crates/editor-actions/`.)
// ---------------------------------------------------------------------------

/// Insert `component` into `entity`, replacing any existing component of the same type.
///
/// Free-function alias for [`World::insert`].
/// Emits a `tracing::warn` and returns if `entity` does not exist.
pub fn insert<C: Component>(world: &mut World, entity: EntityId, component: C) {
    world.insert(entity, component);
}

/// Remove and return the component of type `C` from `entity`, if present.
///
/// Free-function alias for [`World::remove`].
/// Returns `None` when the entity does not exist or does not carry the component.
pub fn remove<C: Component>(world: &mut World, entity: EntityId) -> Option<C> {
    world.remove::<C>(entity)
}

/// Replace the component `C` on `entity` with a new value, returning the old one.
///
/// Free-function alias for [`World::replace`].
/// Returns `None` when the entity is absent or did not carry `C` previously.
pub fn replace<C: Component>(world: &mut World, entity: EntityId, component: C) -> Option<C> {
    world.replace::<C>(entity, component)
}

/// Insert `component` into `entity`, replacing any existing component of the same type.
///
/// Emits a `tracing::warn` and returns if `entity` does not exist.
pub fn insert_component<C: Component>(world: &mut World, entity: EntityId, component: C) {
    world.insert(entity, component);
}

/// Remove and return the component of type `C` from `entity`, if present.
///
/// Returns `None` when the entity does not exist or does not carry the component.
pub fn remove_component<C: Component>(world: &mut World, entity: EntityId) -> Option<C> {
    world.remove::<C>(entity)
}

/// Despawn `entity`, removing it and all its components from the world.
///
/// Returns `true` if the entity existed, `false` if it was already absent.
pub fn despawn(world: &mut World, entity: EntityId) -> bool {
    world.despawn(entity)
}

/// Spawn a new entity carrying `component` and return its [`EntityId`].
pub fn spawn_with<C: Component>(world: &mut World, component: C) -> EntityId {
    world.spawn_with(component)
}
