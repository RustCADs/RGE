//! `SaveMark` and `is_dirty` tests.
//!
//! - New bus: `is_dirty()` == false.
//! - After submit: `is_dirty()` == true.
//! - After `mark_saved`: `is_dirty()` == false.
//! - After another submit: `is_dirty()` == true.

mod test_actions;
use rge_editor_actions::CommandBus;
use rge_kernel_ecs::World;
use test_actions::InsertAction;

#[test]
fn new_bus_is_not_dirty() {
    let bus = CommandBus::new();
    assert!(!bus.is_dirty(), "a freshly created bus must not be dirty");
}

#[test]
fn after_submit_is_dirty() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 1 }), &mut world)
        .unwrap();
    assert!(bus.is_dirty(), "bus must be dirty after first submit");
}

#[test]
fn after_mark_saved_not_dirty() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 1 }), &mut world)
        .unwrap();
    assert!(bus.is_dirty());

    bus.mark_saved();
    assert!(
        !bus.is_dirty(),
        "bus must not be dirty immediately after mark_saved"
    );
}

#[test]
fn dirty_again_after_submit_post_save() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 1 }), &mut world)
        .unwrap();
    bus.mark_saved();
    assert!(!bus.is_dirty());

    // Another edit makes it dirty again.
    let entity2 = world.spawn();
    bus.submit(
        Box::new(InsertAction {
            entity: entity2,
            value: 2,
        }),
        &mut world,
    )
    .unwrap();
    assert!(
        bus.is_dirty(),
        "bus must be dirty after an edit following mark_saved"
    );
}

#[test]
fn undo_to_save_mark_is_not_dirty() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 1 }), &mut world)
        .unwrap();
    bus.mark_saved();

    // Make a further edit.
    let entity2 = world.spawn();
    bus.submit(
        Box::new(InsertAction {
            entity: entity2,
            value: 2,
        }),
        &mut world,
    )
    .unwrap();
    assert!(bus.is_dirty());

    // Undo back to the save mark.
    bus.undo(&mut world).unwrap();
    assert!(
        !bus.is_dirty(),
        "undoing back to the save mark must clear dirty flag"
    );
}
