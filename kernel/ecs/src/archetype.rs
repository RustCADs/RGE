//! Archetype: a bucket of entities sharing the same catch-all bucket.
//!
//! # Storage model
//!
//! Each archetype owns one [`Column`] per component type. A [`Column`] is a
//! `Vec<Option<ColumnRow>>` whose row index aligns 1-to-1 with
//! [`Archetype::entities`]: `entities[row]` is the entity living at that row,
//! and `column.rows[row]` is `Some(_)` when that entity carries the component
//! and `None` when it does not. Trailing absent cells may be elided — a column
//! whose length is less than `entities.len()` implicitly reports `None` for
//! every row past its tail.
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

/// One populated cell inside a [`Column`]: value + change tick stored together
/// so they can be split-borrowed without `unsafe`.
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
///
/// Rows align 1-to-1 with the owning [`Archetype`]'s entity list, but a row
/// may be `None` to represent "this entity has no value for this component".
struct Column {
    rows: Vec<Option<ColumnRow>>,
}

impl Column {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Set the value at `row`, padding the column with `None` placeholders if
    /// `row` is past the current tail. Preserves the existing change tick when
    /// overwriting a populated cell; resets the tick to `0` when transitioning
    /// from absent to present, matching the historical "fresh insert" behavior.
    fn set(&mut self, row: usize, value: Box<dyn Any + Send + Sync>) {
        while self.rows.len() < row {
            self.rows.push(None);
        }
        if row < self.rows.len() {
            match &mut self.rows[row] {
                Some(existing) => existing.value = value,
                slot @ None => {
                    *slot = Some(ColumnRow {
                        value,
                        change_tick: 0,
                    });
                }
            }
        } else {
            self.rows.push(Some(ColumnRow {
                value,
                change_tick: 0,
            }));
        }
    }

    fn get(&self, row: usize) -> Option<&(dyn Any + Send + Sync)> {
        self.rows.get(row)?.as_ref().map(|r| r.value.as_ref())
    }

    fn get_row_mut(&mut self, row: usize) -> Option<&mut ColumnRow> {
        self.rows.get_mut(row)?.as_mut()
    }

    /// Take the value at `row`, leaving a `None` placeholder in its place so
    /// every other row keeps its entity alignment. Returns `None` when the
    /// cell was already empty or out of bounds.
    fn take(&mut self, row: usize) -> Option<Box<dyn Any + Send + Sync>> {
        let slot = self.rows.get_mut(row)?;
        slot.take().map(|r| r.value)
    }

    fn contains(&self, row: usize) -> bool {
        matches!(self.rows.get(row), Some(Some(_)))
    }

    fn bump_change_tick(&mut self, row: usize, tick: u64) {
        if let Some(Some(r)) = self.rows.get_mut(row) {
            r.change_tick = tick;
        }
    }

    fn change_tick(&self, row: usize) -> u64 {
        self.rows
            .get(row)
            .and_then(|s| s.as_ref())
            .map_or(0, |r| r.change_tick)
    }
}

// ---------------------------------------------------------------------------
// Archetype
// ---------------------------------------------------------------------------

/// A bucket of entities that all share the same catch-all archetype.
///
/// Rows correspond 1-to-1 with entities: `entities[row]` gives the [`EntityId`]
/// of the entity at that row. Each component column reports `Some(_)` for a
/// row iff that entity carries the component; otherwise `None`.
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
    ///
    /// This is a coarse "does any entity in this archetype have a value for
    /// this component?" check; it does not say whether a *specific* entity
    /// row currently has the component. For row-specific membership use
    /// [`EntityRef::contains`](crate::entity::EntityRef::contains).
    #[must_use]
    pub fn has_component(&self, id: ComponentId) -> bool {
        self.columns.contains_key(&id.type_id())
    }

    /// Returns `true` when the entity at `row` currently has a value in the
    /// column for `id`.
    #[must_use]
    pub(crate) fn has_component_at_row(&self, id: ComponentId, row: usize) -> bool {
        self.columns
            .get(&id.type_id())
            .is_some_and(|col| col.contains(row))
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
    /// `row` must reference a live entity row (i.e. `row < self.len()`); the
    /// column is padded with absent placeholders as needed so that this
    /// insertion does not shift any other entity's value. First insertion at a
    /// nonzero row is supported.
    pub fn insert_component<C: Component>(&mut self, row: usize, component: C) {
        debug_assert!(row < self.entities.len(), "row must be a live entity row");
        let type_id = TypeId::of::<C>();
        let col = self.columns.entry(type_id).or_insert_with(Column::new);
        col.set(row, Box::new(component));
    }

    /// Remove and return component `C` at `row`.
    ///
    /// Leaves an absent placeholder in the column so the rest of the column
    /// stays aligned with `entities`. Returns `None` when `row` had no value
    /// for `C`.
    pub fn remove_component<C: Component>(&mut self, row: usize) -> Option<C> {
        let col = self.columns.get_mut(&TypeId::of::<C>())?;
        col.take(row)?.downcast::<C>().ok().map(|b| *b)
    }

    /// Swap-remove the entity at `row`; all columns are updated in lock-step.
    ///
    /// Returns the [`EntityId`] that was removed. The entity that was previously
    /// in the last row now occupies `row`, and every column row is rewritten so
    /// that the value (or absence) at the new `row` matches what the moved
    /// entity carried at its previous position.
    pub fn swap_remove_entity(&mut self, row: usize) -> EntityId {
        let last_idx = self.entities.len() - 1;
        let id = self.entities.swap_remove(row);

        if row == last_idx {
            // Removing the trailing entity: just trim every column down to row.
            for col in self.columns.values_mut() {
                if col.rows.len() > row {
                    col.rows.truncate(row);
                }
            }
            return id;
        }

        for col in self.columns.values_mut() {
            let col_len = col.rows.len();
            if col_len > last_idx {
                // Source row is materialised in this column — Vec::swap_remove
                // moves the last cell (Some or None) into `row` and shortens
                // the column by one, keeping alignment with the shortened
                // entities vector.
                col.rows.swap_remove(row);
            } else if col_len > row {
                // The moved entity had no value in this column (it lived past
                // the column's tail), but the destination row was materialised.
                // The destination's new contents must reflect the moved
                // entity's absence, so clear the cell in place. The column
                // stays the same length and remains aligned with `entities`
                // (which is now also one shorter, since the implicit tail
                // never had a cell here).
                col.rows[row] = None;
            }
            // else: neither source nor destination was materialised — the
            // column does not need to change.
        }

        id
    }

    /// Return the row of `entity`, if present.
    #[must_use]
    pub fn row_of(&self, entity: EntityId) -> Option<usize> {
        self.entities.iter().position(|&e| e == entity)
    }

    /// Bump the change tick for the column identified by `type_id` at `row`.
    ///
    /// No-op if no column exists for `type_id` or the cell at `row` is absent.
    pub fn bump_change_tick(&mut self, type_id: TypeId, row: usize, tick: u64) {
        if let Some(col) = self.columns.get_mut(&type_id) {
            col.bump_change_tick(row, tick);
        }
    }

    /// Read the change tick for the column identified by `type_id` at `row`.
    ///
    /// Returns `0` when the cell is absent.
    #[must_use]
    pub fn change_tick_for(&self, type_id: TypeId, row: usize) -> u64 {
        self.columns
            .get(&type_id)
            .map_or(0, |col| col.change_tick(row))
    }

    /// Iterate over all `(row, &C)` pairs in this archetype that currently
    /// have a value for `C`. Rows with absent cells are skipped.
    pub fn iter_component<C: Component>(&self) -> impl Iterator<Item = (usize, &C)> {
        self.columns
            .get(&TypeId::of::<C>())
            .into_iter()
            .flat_map(|col| {
                col.rows.iter().enumerate().filter_map(|(row, slot)| {
                    slot.as_ref()
                        .and_then(|crow| crow.value.downcast_ref::<C>().map(|v| (row, v)))
                })
            })
    }

    /// Borrow the type-erased value at `row` for the column identified by
    /// `type_id`. Returns `None` when no such column exists, `row` is out of
    /// bounds, or the cell at `row` is absent.
    ///
    /// Used by the snapshot serialization path to access component values
    /// without knowing the concrete type at the call site.
    #[must_use]
    pub fn get_erased(&self, type_id: TypeId, row: usize) -> Option<&(dyn Any + Send + Sync)> {
        self.columns.get(&type_id)?.get(row)
    }

    /// Insert a type-erased component value at `row` for the given `type_id`.
    ///
    /// If no column for `type_id` exists it is created and padded with absent
    /// placeholders so that the insertion targets exactly `row`. Used by the
    /// snapshot restore path to insert deserialised component values without
    /// knowing the concrete type at the call site.
    pub fn insert_erased(
        &mut self,
        type_id: TypeId,
        row: usize,
        value: Box<dyn Any + Send + Sync>,
    ) {
        debug_assert!(row < self.entities.len(), "row must be a live entity row");
        let col = self.columns.entry(type_id).or_insert_with(Column::new);
        col.set(row, value);
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

    #[derive(Debug, PartialEq)]
    struct Tag(&'static str);
    impl Component for Tag {}

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
        }
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

    #[test]
    fn first_insert_at_nonzero_row_skips_earlier_rows() {
        let mut arch = Archetype::new();
        let a = EntityId::new();
        let b = EntityId::new();
        arch.push_entity(a);
        arch.push_entity(b);

        // First-ever insertion of Hp lands on row 1 — must not panic and must
        // not shift the value into row 0.
        arch.insert_component::<Hp>(1, Hp(42));
        assert_eq!(arch.get::<Hp>(0), None);
        assert_eq!(arch.get::<Hp>(1), Some(&Hp(42)));
        assert!(!arch.has_component_at_row(ComponentId::of::<Hp>(), 0));
        assert!(arch.has_component_at_row(ComponentId::of::<Hp>(), 1));
    }

    #[test]
    fn remove_keeps_other_rows_aligned() {
        let mut arch = Archetype::new();
        let a = EntityId::new();
        let b = EntityId::new();
        let c = EntityId::new();
        arch.push_entity(a);
        arch.push_entity(b);
        arch.push_entity(c);
        arch.insert_component::<Hp>(0, Hp(10));
        arch.insert_component::<Hp>(1, Hp(20));
        arch.insert_component::<Hp>(2, Hp(30));

        // Removing row 1 must not shift row 2's value into row 1.
        let removed = arch.remove_component::<Hp>(1);
        assert_eq!(removed, Some(Hp(20)));
        assert_eq!(arch.get::<Hp>(0), Some(&Hp(10)));
        assert_eq!(arch.get::<Hp>(1), None);
        assert_eq!(arch.get::<Hp>(2), Some(&Hp(30)));
    }

    #[test]
    fn sparse_swap_remove_preserves_alignment() {
        let mut arch = Archetype::new();
        let a = EntityId::new();
        let b = EntityId::new();
        let c = EntityId::new();
        arch.push_entity(a);
        arch.push_entity(b);
        arch.push_entity(c);

        // Heterogeneous columns: a has Tag only, b has Hp only, c has both.
        arch.insert_component::<Tag>(0, Tag("a"));
        arch.insert_component::<Hp>(1, Hp(20));
        arch.insert_component::<Hp>(2, Hp(30));
        arch.insert_component::<Tag>(2, Tag("c"));

        // Swap-remove the middle entity: `c` should move into row 1 carrying
        // its Hp(30) and Tag("c"). Row 0 (`a`) should still have Tag("a") and
        // no Hp.
        let removed = arch.swap_remove_entity(1);
        assert_eq!(removed, b);
        assert_eq!(arch.len(), 2);
        assert_eq!(arch.entities()[0], a);
        assert_eq!(arch.entities()[1], c);

        assert_eq!(arch.get::<Tag>(0), Some(&Tag("a")));
        assert_eq!(arch.get::<Hp>(0), None);
        assert_eq!(arch.get::<Tag>(1), Some(&Tag("c")));
        assert_eq!(arch.get::<Hp>(1), Some(&Hp(30)));
    }
}
