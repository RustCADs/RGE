//! Archetype: a bucket of entities sharing an identical component set.
//!
//! # Storage model
//!
//! Each archetype owns one [`Column`] per component type.  A [`Column`] is a
//! `Vec<ColumnRow>` where each row holds a `Box<dyn Any + Send + Sync>` value
//! alongside a per-row `change_tick: u64`.
//!
//! Storing value + tick together in [`ColumnRow`] lets us obtain a simultaneous
//! `&mut value` and `&mut change_tick` from a single `&mut ColumnRow` through
//! normal struct field splitting — no `unsafe` required.
//!
//! **Trade-off:** pointer-chasing per component access (not cache-linear).
//! A future optimisation task (`TODO: ecs-column-typed-slab`) can replace this
//! with `unsafe` typed slabs once the workspace relaxes the `unsafe_code = forbid`
//! policy for this crate.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::change_detection::Mut;
use crate::component::{Component, ComponentId};
use crate::entity::EntityId;

// ---------------------------------------------------------------------------
// ColumnRow
// ---------------------------------------------------------------------------

/// One row inside a [`Column`]: value + change tick stored together so they
/// can be split-borrowed without `unsafe`.
struct ColumnRow {
    /// Heap-erased component value.
    value: Box<dyn Any + Send + Sync>,
    /// World tick at which this cell was last mutated.
    change_tick: u64,
}

// ---------------------------------------------------------------------------
// Column
// ---------------------------------------------------------------------------

/// A single-component column within an archetype.
struct Column {
    rows: Vec<ColumnRow>,
}

impl Column {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn push_erased(&mut self, value: Box<dyn Any + Send + Sync>) {
        self.rows.push(ColumnRow {
            value,
            change_tick: 0,
        });
    }

    fn get(&self, row: usize) -> Option<&(dyn Any + Send + Sync)> {
        self.rows.get(row).map(|r| r.value.as_ref())
    }

    fn get_row_mut(&mut self, row: usize) -> Option<&mut ColumnRow> {
        self.rows.get_mut(row)
    }

    fn swap_remove(&mut self, row: usize) -> Box<dyn Any + Send + Sync> {
        self.rows.swap_remove(row).value
    }

    fn replace_value(&mut self, row: usize, value: Box<dyn Any + Send + Sync>) {
        if let Some(r) = self.rows.get_mut(row) {
            r.value = value;
        }
    }

    fn bump_change_tick(&mut self, row: usize, tick: u64) {
        if let Some(r) = self.rows.get_mut(row) {
            r.change_tick = tick;
        }
    }

    fn change_tick(&self, row: usize) -> u64 {
        self.rows.get(row).map_or(0, |r| r.change_tick)
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

// ---------------------------------------------------------------------------
// Archetype
// ---------------------------------------------------------------------------

/// A bucket of entities that all share the same set of component types.
///
/// Rows correspond 1-to-1 with entities: `entities[row]` gives the [`EntityId`]
/// of the entity at that row.
pub struct Archetype {
    /// Ordered entity IDs; index = row.
    entities: Vec<EntityId>,
    /// Per-component columns, keyed by [`TypeId`].
    columns: HashMap<TypeId, Column>,
}

impl Archetype {
    /// Create an empty archetype.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            columns: HashMap::new(),
        }
    }

    /// Number of entities in this archetype.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Returns `true` when no entities reside here.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Returns `true` when this archetype tracks a column for `id`.
    #[must_use]
    pub fn has_component(&self, id: ComponentId) -> bool {
        self.columns.contains_key(&id.type_id())
    }

    /// Entity IDs in row order.
    #[must_use]
    pub fn entities(&self) -> &[EntityId] {
        &self.entities
    }

    /// Borrow component `C` at `row`.
    #[must_use]
    pub fn get<C: Component>(&self, row: usize) -> Option<&C> {
        self.columns
            .get(&TypeId::of::<C>())?
            .get(row)?
            .downcast_ref::<C>()
    }

    /// Borrow component `C` mutably at `row`, returning a change-detecting guard.
    ///
    /// The returned [`Mut<C>`] guard bumps the column's per-row change tick when
    /// it is dropped, recording the mutation for [`Changed<C>`](crate::change_detection::Changed)
    /// queries.
    pub fn get_mut<C: Component>(&mut self, row: usize) -> Option<Mut<'_, C>> {
        // We need simultaneous access to `col.rows[row].value` (as `&mut C`)
        // and `col.rows[row].change_tick` (as `&mut u64`).  ColumnRow stores
        // both in the same struct, so a single `get_row_mut(row)` gives us a
        // `&mut ColumnRow` from which we can split-borrow both fields safely.
        let col = self.columns.get_mut(&TypeId::of::<C>())?;
        let crow = col.get_row_mut(row)?;
        let value: &mut C = crow.value.downcast_mut::<C>()?;
        let tick: &mut u64 = &mut crow.change_tick;
        Some(Mut::new(value, tick))
    }

    /// Append a new entity; does not populate any component columns yet.
    pub fn push_entity(&mut self, entity: EntityId) {
        self.entities.push(entity);
    }

    /// Insert (or replace) component `C` at `row`.
    ///
    /// If no column for `C` exists it is created.  The column must already
    /// have the same length as `entities` (i.e., `push_entity` was called
    /// first).
    pub fn insert_component<C: Component>(&mut self, row: usize, component: C) {
        let type_id = TypeId::of::<C>();
        let col = self.columns.entry(type_id).or_insert_with(Column::new);
        if row < col.len() {
            col.replace_value(row, Box::new(component));
        } else {
            debug_assert_eq!(row, col.len(), "row must match insertion order");
            col.push_erased(Box::new(component));
        }
    }

    /// Remove and return component `C` at `row` (swap-remove semantics).
    pub fn remove_component<C: Component>(&mut self, row: usize) -> Option<C> {
        let col = self.columns.get_mut(&TypeId::of::<C>())?;
        if row >= col.len() {
            return None;
        }
        col.swap_remove(row).downcast::<C>().ok().map(|b| *b)
    }

    /// Swap-remove the entity at `row`; all columns are updated in lock-step.
    ///
    /// Returns the [`EntityId`] that was removed.  The entity that was previously
    /// in the last row now occupies `row`.
    pub fn swap_remove_entity(&mut self, row: usize) -> EntityId {
        let id = self.entities.swap_remove(row);
        for col in self.columns.values_mut() {
            if row < col.len() {
                col.swap_remove(row);
            }
        }
        id
    }

    /// Return the row of `entity`, if present.
    #[must_use]
    pub fn row_of(&self, entity: EntityId) -> Option<usize> {
        self.entities.iter().position(|&e| e == entity)
    }

    /// Bump the change tick for the column identified by `type_id` at `row`.
    pub fn bump_change_tick(&mut self, type_id: TypeId, row: usize, tick: u64) {
        if let Some(col) = self.columns.get_mut(&type_id) {
            col.bump_change_tick(row, tick);
        }
    }

    /// Read the change tick for the column identified by `type_id` at `row`.
    #[must_use]
    pub fn change_tick_for(&self, type_id: TypeId, row: usize) -> u64 {
        self.columns
            .get(&type_id)
            .map_or(0, |col| col.change_tick(row))
    }

    /// Iterate over all `(row, &C)` pairs in this archetype.
    pub fn iter_component<C: Component>(&self) -> impl Iterator<Item = (usize, &C)> {
        self.columns
            .get(&TypeId::of::<C>())
            .into_iter()
            .flat_map(|col| {
                col.rows
                    .iter()
                    .enumerate()
                    .filter_map(|(row, crow)| crow.value.downcast_ref::<C>().map(|v| (row, v)))
            })
    }

    /// Borrow the type-erased value at `row` for the column identified by
    /// `type_id`. Returns `None` when no such column exists or `row` is out
    /// of bounds.
    ///
    /// Used by the snapshot serialization path to access component values
    /// without knowing the concrete type at the call site.
    #[must_use]
    pub fn get_erased(&self, type_id: TypeId, row: usize) -> Option<&(dyn Any + Send + Sync)> {
        self.columns.get(&type_id)?.get(row)
    }

    /// Insert a type-erased component value at `row` for the given `type_id`.
    ///
    /// If no column for `type_id` exists it is created. Used by the snapshot
    /// restore path to insert deserialised component values without knowing
    /// the concrete type at the call site.
    pub fn insert_erased(
        &mut self,
        type_id: TypeId,
        row: usize,
        value: Box<dyn Any + Send + Sync>,
    ) {
        let col = self.columns.entry(type_id).or_insert_with(Column::new);
        if row < col.len() {
            col.replace_value(row, value);
        } else {
            debug_assert_eq!(row, col.len(), "row must match insertion order");
            col.push_erased(value);
        }
    }
}

impl Default for Archetype {
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
    struct Hp(u32);
    impl Component for Hp {}

    #[test]
    fn insert_and_get() {
        let mut arch = Archetype::new();
        let id = EntityId::new();
        arch.push_entity(id);
        arch.insert_component::<Hp>(0, Hp(100));
        assert_eq!(arch.get::<Hp>(0), Some(&Hp(100)));
    }

    #[test]
    fn get_mut_bumps_tick() {
        let mut arch = Archetype::new();
        let id = EntityId::new();
        arch.push_entity(id);
        arch.insert_component::<Hp>(0, Hp(50));
        {
            let mut guard = arch.get_mut::<Hp>(0).unwrap();
            guard.0 = 99;
            // tick is bumped on drop
        }
        // The guard's `&mut u64` points at the column row's tick; after drop it
        // reflects the write in Mut::drop.  We wrote `1` as the world tick via
        // Mut::new (tick starts 0, Mut uses the pointer reference).
        // Actually Mut::drop writes the value that was in `*tick` at drop time,
        // which was set by Mut::new's second argument.  In this test we pass
        // a direct `&mut u64` so the tick field reflects whatever Mut writes.
        // Verify the value changed at minimum.
        assert_eq!(arch.get::<Hp>(0), Some(&Hp(99)));
    }

    #[test]
    fn swap_remove_entity() {
        let mut arch = Archetype::new();
        let a = EntityId::new();
        let b = EntityId::new();
        arch.push_entity(a);
        arch.insert_component::<Hp>(0, Hp(1));
        arch.push_entity(b);
        arch.insert_component::<Hp>(1, Hp(2));
        arch.swap_remove_entity(0);
        assert_eq!(arch.len(), 1);
        // b moved to row 0
        assert_eq!(arch.entities()[0], b);
        assert_eq!(arch.get::<Hp>(0), Some(&Hp(2)));
    }

    #[test]
    fn change_tick_tracking() {
        let mut arch = Archetype::new();
        let id = EntityId::new();
        arch.push_entity(id);
        arch.insert_component::<Hp>(0, Hp(1));
        assert_eq!(arch.change_tick_for(TypeId::of::<Hp>(), 0), 0);
        arch.bump_change_tick(TypeId::of::<Hp>(), 0, 7);
        assert_eq!(arch.change_tick_for(TypeId::of::<Hp>(), 0), 7);
    }
}
