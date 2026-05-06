//! [`Query<T>`] — a lazy iterator over components.

use std::any::TypeId;

use crate::archetype::Archetype;
use crate::component::Component;
use crate::entity::EntityId;

// ---------------------------------------------------------------------------
// Query<T>
// ---------------------------------------------------------------------------

/// Iterator over `(EntityId, &C)` pairs for all entities carrying component `C`.
///
/// Constructed by [`World::query`](crate::world::World::query).  When the
/// world tick filter is `Some(type_id)`, only entities whose slot tick for
/// `type_id` is strictly greater than `last_tick` are yielded.
///
/// `Query<C>` is itself an `Iterator`; use it directly in `for` loops or with
/// iterator combinators (`map`, `collect`, `filter`, etc.).
///
/// No filter combinators (e.g. `With<T>`, `Without<T>`) are implemented yet;
/// those are a future-phase addition.
pub struct Query<'w, C: Component> {
    /// All archetypes in the world (we iterate all of them).
    archetypes: &'w [Archetype],
    /// Index of the current archetype being visited.
    arch_idx: usize,
    /// Current row within the active archetype.
    row: usize,
    /// When `Some`, only yield rows whose change tick > `last_tick`.
    filter_type_id: Option<TypeId>,
    /// Last-observed tick (set by `World::advance_tick`).
    last_tick: u64,
    _marker: std::marker::PhantomData<&'w C>,
}

impl<'w, C: Component> Query<'w, C> {
    /// Construct a new query.
    ///
    /// Called by `World::query`; not public API.
    pub(crate) fn new(
        archetypes: &'w [Archetype],
        filter_type_id: Option<TypeId>,
        last_tick: u64,
    ) -> Self {
        Self {
            archetypes,
            arch_idx: 0,
            row: 0,
            filter_type_id,
            last_tick,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'w, C: Component> Iterator for Query<'w, C> {
    type Item = (EntityId, &'w C);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let arch = self.archetypes.get(self.arch_idx)?;

            if self.row >= arch.len() {
                // Move to next archetype.
                self.arch_idx += 1;
                self.row = 0;
                continue;
            }

            let row = self.row;
            self.row += 1;

            // Check for the component.
            let Some(component) = arch.get::<C>(row) else {
                continue;
            };

            // Apply change-tick filter.
            if let Some(tid) = self.filter_type_id {
                if arch.change_tick_for(tid, row) <= self.last_tick {
                    continue;
                }
            }

            let entity = arch.entities()[row];
            return Some((entity, component));
        }
    }
}
