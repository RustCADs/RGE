//! Audit-ledger projection tests.
//!
//! - Submit 3 actions → ledger has 3 events with `EventKind::Action`.
//! - Ledger cursor advances with each submit.
//! - Undo → ledger cursor retreats.

mod test_actions;
use rge_editor_actions::CommandBus;
use rge_kernel_audit_ledger::EventKind;
use rge_kernel_ecs::World;
use test_actions::InsertAction;

#[test]
fn three_submits_produce_three_ledger_events() {
    // Use three different entities so each InsertAction has a unique ActionId
    // and coalescing can never apply.
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let e1 = world.spawn();
    let e2 = world.spawn();
    let e3 = world.spawn();

    bus.submit(
        Box::new(InsertAction {
            entity: e1,
            value: 1,
        }),
        &mut world,
    )
    .unwrap();
    bus.submit(
        Box::new(InsertAction {
            entity: e2,
            value: 2,
        }),
        &mut world,
    )
    .unwrap();
    bus.submit(
        Box::new(InsertAction {
            entity: e3,
            value: 3,
        }),
        &mut world,
    )
    .unwrap();

    let events: Vec<_> = bus.ledger().iter().collect();
    assert_eq!(events.len(), 3, "ledger must record exactly 3 events");
    for event in &events {
        assert_eq!(
            event.kind,
            EventKind::Action,
            "every projected event must carry EventKind::Action"
        );
    }
}

#[test]
fn ledger_cursor_advances_with_submits() {
    // Use three different entities so each InsertAction has a unique ActionId.
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let e1 = world.spawn();
    let e2 = world.spawn();
    let e3 = world.spawn();

    assert_eq!(bus.ledger().cursor(), 0);

    bus.submit(
        Box::new(InsertAction {
            entity: e1,
            value: 1,
        }),
        &mut world,
    )
    .unwrap();
    assert_eq!(bus.ledger().cursor(), 1);

    bus.submit(
        Box::new(InsertAction {
            entity: e2,
            value: 2,
        }),
        &mut world,
    )
    .unwrap();
    assert_eq!(bus.ledger().cursor(), 2);

    bus.submit(
        Box::new(InsertAction {
            entity: e3,
            value: 3,
        }),
        &mut world,
    )
    .unwrap();
    assert_eq!(bus.ledger().cursor(), 3);
}

#[test]
fn undo_retreats_ledger_cursor() {
    // Use three different entities so each InsertAction has a unique ActionId.
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let e1 = world.spawn();
    let e2 = world.spawn();
    let e3 = world.spawn();

    bus.submit(
        Box::new(InsertAction {
            entity: e1,
            value: 10,
        }),
        &mut world,
    )
    .unwrap();
    bus.submit(
        Box::new(InsertAction {
            entity: e2,
            value: 20,
        }),
        &mut world,
    )
    .unwrap();
    bus.submit(
        Box::new(InsertAction {
            entity: e3,
            value: 30,
        }),
        &mut world,
    )
    .unwrap();

    assert_eq!(bus.ledger().cursor(), 3);

    bus.undo(&mut world).unwrap();
    assert_eq!(
        bus.ledger().cursor(),
        2,
        "undo must retreat the ledger cursor by 1"
    );

    bus.undo(&mut world).unwrap();
    assert_eq!(bus.ledger().cursor(), 1);

    // Redo should advance cursor again.
    bus.redo(&mut world).unwrap();
    assert_eq!(bus.ledger().cursor(), 2);
}

#[test]
fn payload_bytes_recorded_in_ledger() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();

    bus.submit(Box::new(InsertAction { entity, value: 5 }), &mut world)
        .unwrap();

    let event = bus.ledger().iter().next().expect("must have one event");
    // Default payload is `name().as_bytes()`.
    assert_eq!(event.payload, b"insert-test-val");
}
