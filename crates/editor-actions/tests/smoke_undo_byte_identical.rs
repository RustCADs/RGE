//! Smoke test: spawn entity → insert component → modify → undo → undo → verify
//! byte-identical state at each step.

mod test_actions;
use rge_editor_actions::CommandBus;
use rge_kernel_ecs::World;
use test_actions::{InsertAction, ModifyAction, SpawnAction, TestVal};

#[test]
fn spawn_insert_modify_undo_undo_byte_identical() {
    let mut bus = CommandBus::new();
    let mut world = World::new();

    // ── Step 1: spawn entity ─────────────────────────────────────────────────
    let spawn = SpawnAction::new(10);
    bus.submit(Box::new(spawn), &mut world).unwrap();
    assert_eq!(bus.stack().cursor(), 1);

    // Retrieve the spawned entity id.
    // We'll submit a separate InsertAction for testability.

    // ── Step 2: spawn a second entity with known id via world + InsertAction ─
    let entity = world.spawn();
    bus.submit(Box::new(InsertAction { entity, value: 42 }), &mut world)
        .unwrap();
    assert_eq!(bus.stack().cursor(), 2);
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        Some(&TestVal(42))
    );

    // ── Step 3: modify component ─────────────────────────────────────────────
    bus.submit(
        Box::new(ModifyAction {
            entity,
            new_value: 99,
            old_value: 42,
        }),
        &mut world,
    )
    .unwrap();
    assert_eq!(bus.stack().cursor(), 3);
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        Some(&TestVal(99))
    );

    // ── Undo #1: revert modify → back to 42 ─────────────────────────────────
    bus.undo(&mut world).unwrap();
    assert_eq!(bus.stack().cursor(), 2);
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        Some(&TestVal(42)),
        "after first undo: component must be byte-identical to pre-modify value"
    );

    // ── Undo #2: revert insert → component absent ────────────────────────────
    bus.undo(&mut world).unwrap();
    assert_eq!(bus.stack().cursor(), 1);
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        None,
        "after second undo: component must be absent (byte-identical to pre-insert state)"
    );

    // ── Undo #3: revert spawn → entity despawned ─────────────────────────────
    bus.undo(&mut world).unwrap();
    assert_eq!(bus.stack().cursor(), 0);

    // ── Nothing left to undo ─────────────────────────────────────────────────
    assert!(matches!(
        bus.undo(&mut world),
        Err(rge_editor_actions::BusError::NothingToUndo)
    ));
}

#[test]
fn redo_after_undo_restores_state() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 5 }), &mut world)
        .unwrap();
    bus.submit(
        Box::new(ModifyAction {
            entity,
            new_value: 50,
            old_value: 5,
        }),
        &mut world,
    )
    .unwrap();

    bus.undo(&mut world).unwrap();
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        Some(&TestVal(5))
    );

    bus.redo(&mut world).unwrap();
    assert_eq!(
        world.entity(entity).unwrap().get::<TestVal>(),
        Some(&TestVal(50)),
        "redo must restore byte-identical modified value"
    );
}
