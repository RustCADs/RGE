//! Change-detection primitives: [`Mut<T>`] and [`Changed<T>`].
//!
//! # Design
//!
//! Change detection is driven by a monotonically-increasing *world tick*
//! (`u64`) stored on [`World`](crate::world::World).  Each component slot in
//! an archetype column also carries a `change_tick: u64` (see
//! [`Archetype`](crate::archetype::Archetype)).
//!
//! When a mutable borrow of a component is released ([`Mut<T>`] drops), the
//! slot's `change_tick` is written with the world's current tick.  A
//! [`Changed<T>`] query then filters for entities whose slot tick is greater
//! than the *last-observed tick* (the tick value at the previous call to
//! [`World::advance_tick`](crate::world::World::advance_tick)).

use std::any::TypeId;
use std::ops::{Deref, DerefMut};

use crate::component::Component;

// ---------------------------------------------------------------------------
// Mut<'a, T>
// ---------------------------------------------------------------------------

/// A mutable component access guard that records a mutation on [`Drop`].
///
/// Obtained via [`EntityMut::get_mut`](crate::entity::EntityMut::get_mut).
///
/// Derefs transparently to `T`.  When the guard is dropped the per-slot
/// `change_tick` is written with the current world tick so that
/// [`Changed<T>`] queries can detect the mutation.
pub struct Mut<'a, T: Component> {
    /// Reference into the column's erased box.
    value: &'a mut T,
    /// Reference into the column's per-row change tick.
    tick: &'a mut u64,
    /// World tick captured at guard construction.
    world_tick: u64,
}

impl<'a, T: Component> Mut<'a, T> {
    /// Construct a `Mut` guard.
    ///
    /// `world_tick` is the tick to record when the guard is dropped; it should
    /// be `World::current_tick()` at the time the guard is created.
    ///
    /// When constructed from [`Archetype::get_mut`](crate::archetype::Archetype::get_mut)
    /// the `world_tick` defaults to `1` (or the caller supplies it via the world
    /// entry point).  The exact value only matters for filtering — as long as it is
    /// strictly greater than the last-observed tick the mutation will be detected.
    #[must_use]
    pub(crate) fn new(value: &'a mut T, tick: &'a mut u64) -> Self {
        // Default world_tick of 1 is set here; callers that go through
        // World::entity_mut override it via `with_world_tick`.
        Self {
            value,
            tick,
            world_tick: 1,
        }
    }

    /// Override the world tick that will be recorded on drop.
    #[must_use]
    pub(crate) fn with_world_tick(mut self, tick: u64) -> Self {
        self.world_tick = tick;
        self
    }
}

impl<T: Component> Deref for Mut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T: Component> DerefMut for Mut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<T: Component> Drop for Mut<'_, T> {
    fn drop(&mut self) {
        *self.tick = self.world_tick;
    }
}

// ---------------------------------------------------------------------------
// Changed<T> — query-filter marker
// ---------------------------------------------------------------------------

/// Query-filter marker type that selects entities whose component `T` was
/// mutated since the last [`World::advance_tick`](crate::world::World::advance_tick).
///
/// Used as a type parameter to [`World::query`](crate::world::World::query):
///
/// ```rust
/// # use rge_kernel_ecs::{World, Changed};
/// # struct Pos { x: f32 }
/// # impl rge_kernel_ecs::Component for Pos {}
/// # let world = World::new();
/// for (id, pos) in world.query::<Changed<Pos>>() {
///     // pos.x was written since the last advance_tick()
/// }
/// ```
///
/// # Internals
///
/// `Changed<T>` is a zero-sized marker; the actual filtering logic lives in
/// [`World::query`].  The world compares each slot's `change_tick` against the
/// *last-observed tick* (set by `advance_tick`).  Slots with
/// `change_tick > last_observed` are yielded.
pub struct Changed<T: Component> {
    _marker: std::marker::PhantomData<T>,
}

/// Sealed helper trait that extracts the inner component type from a
/// `Changed<T>` marker so `World::query` can be generic over both
/// `T` (raw component) and `Changed<T>` (filtered).
pub trait QueryFilter: 'static {
    /// The underlying component type being queried.
    type Component: Component;

    /// Returns the [`TypeId`] used for change-tick filtering.
    ///
    /// `None` means "no tick filter" (plain component query);
    /// `Some(type_id)` means "only yield rows whose tick > `last_observed`".
    fn filter_type_id() -> Option<TypeId>;
}

impl<T: Component> QueryFilter for T {
    type Component = T;
    fn filter_type_id() -> Option<TypeId> {
        None
    }
}

impl<T: Component> QueryFilter for Changed<T> {
    type Component = T;
    fn filter_type_id() -> Option<TypeId> {
        Some(TypeId::of::<T>())
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Speed(f32);
    impl Component for Speed {}

    #[test]
    fn mut_deref_and_tick() {
        let mut val = Speed(1.0);
        let mut tick: u64 = 0;
        {
            let mut guard = Mut::new(&mut val, &mut tick).with_world_tick(5);
            guard.0 = 2.0;
        }
        assert_eq!(val, Speed(2.0));
        assert_eq!(tick, 5, "tick should be bumped to world_tick on drop");
    }

    #[test]
    fn mut_no_write_still_bumps() {
        // Even a read-only use bumps the tick (conservative — same as Bevy).
        let mut val = Speed(1.0);
        let mut tick: u64 = 0;
        {
            let guard = Mut::new(&mut val, &mut tick).with_world_tick(3);
            let _ = guard.0; // read only
        }
        assert_eq!(tick, 3);
    }
}
