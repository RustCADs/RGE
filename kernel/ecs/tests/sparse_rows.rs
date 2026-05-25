//! Sparse-row tests for the catch-all archetype.
//!
//! These cover heterogeneous component sets within the single archetype model:
//! one entity carrying `A` while another carries `B`, first insertion of a
//! component type at a nonzero entity row, sparse remove/replace, sparse
//! despawn swap-remove, and a heterogeneous snapshot round-trip whose restore
//! exercises the erased insertion path at a nonzero row.

use rge_kernel_ecs::{Component, SnapshotComponent, World};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Health(u32);
impl Component for Health {}
impl SnapshotComponent for Health {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Name(String);
impl Component for Name {}
impl SnapshotComponent for Name {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Speed(f32);
impl Component for Speed {}
impl SnapshotComponent for Speed {}

// ---------------------------------------------------------------------------
// Heterogeneous insert / get / contains / query
// ---------------------------------------------------------------------------

#[test]
fn heterogeneous_typed_insert_get_contains_query() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    let c = w.spawn();

    // `a` only has Health, `b` only has Name, `c` has both.
    w.insert(a, Health(10));
    w.insert(b, Name("bob".into()));
    w.insert(c, Health(30));
    w.insert(c, Name("carla".into()));

    // get/contains row-specific.
    let ea = w.entity(a).unwrap();
    assert_eq!(ea.get::<Health>(), Some(&Health(10)));
    assert!(ea.contains::<Health>());
    assert_eq!(ea.get::<Name>(), None);
    assert!(!ea.contains::<Name>());

    let eb = w.entity(b).unwrap();
    assert_eq!(eb.get::<Health>(), None);
    assert!(!eb.contains::<Health>());
    assert_eq!(eb.get::<Name>(), Some(&Name("bob".into())));
    assert!(eb.contains::<Name>());

    let ec = w.entity(c).unwrap();
    assert_eq!(ec.get::<Health>(), Some(&Health(30)));
    assert!(ec.contains::<Health>());
    assert_eq!(ec.get::<Name>(), Some(&Name("carla".into())));
    assert!(ec.contains::<Name>());

    // Query yields only entities that actually carry the component.
    let mut hp_ids: Vec<_> = w.query::<Health>().map(|(id, _)| id).collect();
    hp_ids.sort();
    let mut expected_hp = vec![a, c];
    expected_hp.sort();
    assert_eq!(hp_ids, expected_hp);

    let mut name_ids: Vec<_> = w.query::<Name>().map(|(id, _)| id).collect();
    name_ids.sort();
    let mut expected_name = vec![b, c];
    expected_name.sort();
    assert_eq!(name_ids, expected_name);
}

#[test]
fn contains_agrees_with_get_after_remove() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    w.insert(a, Health(1));
    w.insert(b, Health(2));

    assert!(w.entity(a).unwrap().contains::<Health>());
    assert!(w.entity(b).unwrap().contains::<Health>());

    // Removing from `a` must not flip `b`'s `contains` and must agree with
    // `get` for the same row.
    w.remove::<Health>(a);
    let ea = w.entity(a).unwrap();
    assert_eq!(ea.contains::<Health>(), ea.get::<Health>().is_some());
    assert!(!ea.contains::<Health>());
    let eb = w.entity(b).unwrap();
    assert_eq!(eb.contains::<Health>(), eb.get::<Health>().is_some());
    assert!(eb.contains::<Health>());
    assert_eq!(eb.get::<Health>(), Some(&Health(2)));
}

// ---------------------------------------------------------------------------
// First insertion of a component type at a nonzero row
// ---------------------------------------------------------------------------

#[test]
fn typed_first_insert_at_nonzero_row() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    let c = w.spawn();

    // First-ever Speed value goes onto `c`, the third entity. This is the
    // canonical "first insert past row 0" case that the dense layout used to
    // panic on.
    w.insert(c, Speed(7.5));

    assert_eq!(w.entity(a).unwrap().get::<Speed>(), None);
    assert_eq!(w.entity(b).unwrap().get::<Speed>(), None);
    assert_eq!(w.entity(c).unwrap().get::<Speed>(), Some(&Speed(7.5)));
    assert!(!w.entity(a).unwrap().contains::<Speed>());
    assert!(!w.entity(b).unwrap().contains::<Speed>());
    assert!(w.entity(c).unwrap().contains::<Speed>());

    // The query must yield exactly the one entity carrying Speed.
    let speed_ids: Vec<_> = w.query::<Speed>().map(|(id, _)| id).collect();
    assert_eq!(speed_ids, vec![c]);
}

#[test]
fn entity_mut_first_insert_at_nonzero_row() {
    let mut w = World::new();
    let _ = w.spawn();
    let _ = w.spawn();
    let target = w.spawn();

    {
        let mut em = w.entity_mut(target).unwrap();
        em.insert(Name("late".into()));
    }
    assert_eq!(
        w.entity(target).unwrap().get::<Name>(),
        Some(&Name("late".into()))
    );
    // No earlier entity should have picked up a Name as a side effect.
    let names: Vec<_> = w.query::<Name>().map(|(id, _)| id).collect();
    assert_eq!(names, vec![target]);
}

#[test]
fn spawn_with_after_nonzero_row_insert_remains_aligned() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    // First insert of Health is at row 1.
    w.insert(b, Health(50));
    // Now `spawn_with` adds entity c at row 2 with Health(60).
    let c = w.spawn_with(Health(60));
    // And `spawn_with` for a different component on a fresh row.
    let d = w.spawn_with(Name("d".into()));

    assert_eq!(w.entity(a).unwrap().get::<Health>(), None);
    assert_eq!(w.entity(b).unwrap().get::<Health>(), Some(&Health(50)));
    assert_eq!(w.entity(c).unwrap().get::<Health>(), Some(&Health(60)));
    assert_eq!(w.entity(d).unwrap().get::<Health>(), None);
    assert_eq!(w.entity(d).unwrap().get::<Name>(), Some(&Name("d".into())));
}

// ---------------------------------------------------------------------------
// Sparse remove / replace
// ---------------------------------------------------------------------------

#[test]
fn sparse_remove_does_not_shift_other_rows() {
    let mut w = World::new();
    let a = w.spawn_with(Health(100));
    let b = w.spawn_with(Health(200));
    let c = w.spawn_with(Health(300));

    // Remove the middle entity's component. The dense layout would swap
    // c's value into b's row; the sparse layout must keep both intact.
    let removed = w.remove::<Health>(b);
    assert_eq!(removed, Some(Health(200)));
    assert_eq!(w.entity(a).unwrap().get::<Health>(), Some(&Health(100)));
    assert_eq!(w.entity(b).unwrap().get::<Health>(), None);
    assert_eq!(w.entity(c).unwrap().get::<Health>(), Some(&Health(300)));

    // The query reports exactly two carriers, not three.
    let mut ids: Vec<_> = w.query::<Health>().map(|(id, _)| id).collect();
    ids.sort();
    let mut expected = vec![a, c];
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn sparse_replace_inserts_when_absent_and_returns_old_when_present() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    w.insert(a, Health(1));

    // Replace on an entity without the component should insert and return None.
    let old = w.replace(b, Health(2));
    assert_eq!(old, None);
    assert_eq!(w.entity(b).unwrap().get::<Health>(), Some(&Health(2)));

    // Replace on an entity that already has the component returns the old one.
    let old = w.replace(a, Health(11));
    assert_eq!(old, Some(Health(1)));
    assert_eq!(w.entity(a).unwrap().get::<Health>(), Some(&Health(11)));

    // No spurious changes on the other row.
    assert_eq!(w.entity(b).unwrap().get::<Health>(), Some(&Health(2)));
}

// ---------------------------------------------------------------------------
// Sparse despawn swap-remove
// ---------------------------------------------------------------------------

#[test]
fn sparse_despawn_preserves_swapped_entity_components() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    let c = w.spawn();

    // Heterogeneous setup: a has Health only, b has Name only, c has both.
    w.insert(a, Health(10));
    w.insert(b, Name("bob".into()));
    w.insert(c, Health(30));
    w.insert(c, Name("carla".into()));

    // Despawn `a` (row 0). The catch-all archetype's swap-remove moves `c`
    // into row 0; `c` must keep both of its component values, and `b` (row 1)
    // must be untouched.
    assert!(w.despawn(a));
    assert_eq!(w.entity_count(), 2);

    let eb = w.entity(b).unwrap();
    assert_eq!(eb.get::<Health>(), None);
    assert_eq!(eb.get::<Name>(), Some(&Name("bob".into())));
    let ec = w.entity(c).unwrap();
    assert_eq!(ec.get::<Health>(), Some(&Health(30)));
    assert_eq!(ec.get::<Name>(), Some(&Name("carla".into())));
}

#[test]
fn sparse_despawn_when_swapped_entity_lacks_a_column() {
    let mut w = World::new();
    let a = w.spawn();
    let b = w.spawn();
    let c = w.spawn();

    // Only `a` and `b` carry Health; `c` does not. Despawning `a` swaps `c`
    // into row 0. Row 0 must report Health absent, row 1 (still `b`) keeps it.
    w.insert(a, Health(10));
    w.insert(b, Health(20));

    assert!(w.despawn(a));
    assert_eq!(w.entity_count(), 2);

    // `c` is now at the former row 0; it must not have inherited a Health
    // value from `a`.
    assert_eq!(w.entity(c).unwrap().get::<Health>(), None);
    assert!(!w.entity(c).unwrap().contains::<Health>());
    assert_eq!(w.entity(b).unwrap().get::<Health>(), Some(&Health(20)));

    let ids: Vec<_> = w.query::<Health>().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![b]);
}

#[test]
fn sparse_despawn_of_last_row_truncates_columns() {
    let mut w = World::new();
    let a = w.spawn_with(Health(1));
    let b = w.spawn_with(Health(2));
    // Despawn the last entity. The remaining `a` must still own its value.
    assert!(w.despawn(b));
    assert_eq!(w.entity_count(), 1);
    assert_eq!(w.entity(a).unwrap().get::<Health>(), Some(&Health(1)));
    let ids: Vec<_> = w.query::<Health>().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![a]);
}

// ---------------------------------------------------------------------------
// Heterogeneous snapshot round-trip
// ---------------------------------------------------------------------------

#[test]
fn heterogeneous_snapshot_round_trip_exercises_erased_nonzero_row_insert() {
    let mut w = World::new();
    w.register_snapshot_component::<Health>();
    w.register_snapshot_component::<Name>();
    w.register_snapshot_component::<Speed>();

    // Spawn three entities, then sort by EntityId to make the restore order
    // deterministic. `Ulid::new` is not guaranteed to produce a strictly
    // increasing sequence within a single millisecond, so we cannot rely on
    // spawn order matching sorted order. Restore replays entities in sorted
    // EntityId order, so placing `Speed` on the entity that sorts last
    // guarantees its first ever erased restore insert lands at row 2 (>0).
    let mut ids = [w.spawn(), w.spawn(), w.spawn()];
    ids.sort();
    let [first, middle, last] = ids;

    w.insert(first, Health(1));
    w.insert(first, Name("alpha".into()));
    w.insert(middle, Name("beta".into()));
    w.insert(last, Health(3));
    w.insert(last, Speed(9.0));

    let snap = w.serialize_snapshot().expect("serialize");

    // Mutate everything so the restore must overwrite, then restore.
    w.insert(first, Health(99));
    w.insert(first, Name("CORRUPT".into()));
    w.insert(middle, Name("CORRUPT".into()));
    w.insert(last, Health(99));
    w.insert(last, Speed(-1.0));

    w.restore_from_snapshot(&snap).expect("restore");

    // Original sparse presence/absence must be reproduced, including the
    // case where `Speed`'s first ever restore insert lands on row 2.
    assert_eq!(w.entity_count(), 3);

    // Pull values back keyed by entity id.
    let ea = w.entity(first).expect("first restored");
    assert_eq!(ea.get::<Health>(), Some(&Health(1)));
    assert_eq!(ea.get::<Name>(), Some(&Name("alpha".into())));
    assert_eq!(ea.get::<Speed>(), None);
    assert!(!ea.contains::<Speed>());

    let eb = w.entity(middle).expect("middle restored");
    assert_eq!(eb.get::<Health>(), None);
    assert!(!eb.contains::<Health>());
    assert_eq!(eb.get::<Name>(), Some(&Name("beta".into())));
    assert_eq!(eb.get::<Speed>(), None);

    let ec = w.entity(last).expect("last restored");
    assert_eq!(ec.get::<Health>(), Some(&Health(3)));
    assert_eq!(ec.get::<Name>(), None);
    assert!(!ec.contains::<Name>());
    assert_eq!(ec.get::<Speed>(), Some(&Speed(9.0)));

    // Re-serializing after restore must produce byte-identical output —
    // the sparse fix must not change the wire format.
    let snap2 = w.serialize_snapshot().expect("re-serialize");
    assert_eq!(
        snap, snap2,
        "snapshot wire format must be unchanged by sparse storage"
    );
}
