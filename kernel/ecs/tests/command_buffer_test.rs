//! Integration tests for the deferred [`Commands`] buffer.

use rge_kernel_ecs::{Commands, Component, World};

#[derive(Debug, Clone, PartialEq)]
struct Tag(u32);
impl Component for Tag {}

#[derive(Debug, Clone, PartialEq)]
struct Score(i32);
impl Component for Score {}

// ---------------------------------------------------------------------------
// Deferred semantics
// ---------------------------------------------------------------------------

#[test]
fn spawn_with_deferred() {
    let mut world = World::new();
    world.commands().spawn_with(Tag(1));
    assert_eq!(world.entity_count(), 0, "not applied before flush");
    world.flush_commands();
    assert_eq!(world.entity_count(), 1, "applied after flush");
}

#[test]
fn insert_deferred() {
    let mut world = World::new();
    let id = world.spawn();
    world.commands().insert(id, Tag(42));
    assert!(
        world.entity(id).unwrap().get::<Tag>().is_none(),
        "not visible before flush"
    );
    world.flush_commands();
    assert_eq!(world.entity(id).unwrap().get::<Tag>(), Some(&Tag(42)));
}

#[test]
fn remove_deferred() {
    let mut world = World::new();
    let id = world.spawn_with(Tag(99));
    world.commands().remove::<Tag>(id);
    assert!(
        world.entity(id).unwrap().get::<Tag>().is_some(),
        "still present before flush"
    );
    world.flush_commands();
    assert!(
        world.entity(id).unwrap().get::<Tag>().is_none(),
        "removed after flush"
    );
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

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn insert_order_apply_order() {
    let mut world = World::new();
    let id = world.spawn();
    // Enqueue two inserts for the same component — second must win.
    world.commands().insert(id, Tag(1));
    world.commands().insert(id, Tag(2));
    world.flush_commands();
    assert_eq!(world.entity(id).unwrap().get::<Tag>(), Some(&Tag(2)));
}

#[test]
fn multiple_commands_in_order() {
    let mut world = World::new();
    let id = world.spawn();
    world.commands().insert(id, Tag(10));
    world.commands().insert(id, Score(100));
    world.flush_commands();
    assert_eq!(world.entity(id).unwrap().get::<Tag>(), Some(&Tag(10)));
    assert_eq!(world.entity(id).unwrap().get::<Score>(), Some(&Score(100)));
}

// ---------------------------------------------------------------------------
// Buffer cleared after flush
// ---------------------------------------------------------------------------

#[test]
fn buffer_empty_after_flush() {
    let mut world = World::new();
    world.commands().spawn_with(Tag(0));
    assert!(!world.commands().is_empty());
    world.flush_commands();
    assert!(
        world.commands().is_empty(),
        "buffer must be empty after flush"
    );
}

#[test]
fn second_flush_is_noop() {
    let mut world = World::new();
    world.commands().spawn_with(Tag(1));
    world.flush_commands();
    world.flush_commands(); // should not panic or duplicate
    assert_eq!(world.entity_count(), 1);
}

// ---------------------------------------------------------------------------
// Commands struct is directly usable
// ---------------------------------------------------------------------------

#[test]
fn commands_struct_accessible() {
    // Verify Commands can be constructed stand-alone (for use in systems).
    let mut cmds = Commands::new();
    assert!(cmds.is_empty());
    assert_eq!(cmds.len(), 0);
    let id = rge_kernel_ecs::EntityId::new();
    cmds.insert(id, Tag(5));
    assert_eq!(cmds.len(), 1);
}
