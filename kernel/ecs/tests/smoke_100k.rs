//! Smoke test: 100 k entities — spawn, iterate, mutate, verify `Changed<T>`.
//!
//! Per IMPLEMENTATION.md Phase 2.1.

use rge_kernel_ecs::{Changed, Component, EntityId, World};

#[derive(Debug, Clone, PartialEq)]
struct Transform {
    x: f32,
    y: f32,
    z: f32,
}
impl Component for Transform {}

#[test]
fn smoke_100k_entities_changed_filter() {
    let mut world = World::new();
    let mut ids = Vec::with_capacity(100_000);
    #[allow(clippy::cast_precision_loss)]
    for i in 0..100_000_u32 {
        ids.push(world.spawn_with(Transform {
            x: i as f32,
            y: 0.0,
            z: 0.0,
        }));
    }
    assert_eq!(world.entity_count(), 100_000);

    world.advance_tick();

    // Mutate every 10th entity's Transform.
    for &id in ids.iter().step_by(10) {
        if let Some(mut e) = world.entity_mut(id) {
            if let Some(mut t) = e.get_mut::<Transform>() {
                t.y = 42.0;
            }
        }
    }

    let changed: Vec<EntityId> = world
        .query::<Changed<Transform>>()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        changed.len(),
        10_000,
        "10k entities should be flagged Changed"
    );

    // After advance_tick, Changed should be empty again until the next mutation.
    world.advance_tick();
    let still_changed: Vec<EntityId> = world
        .query::<Changed<Transform>>()
        .map(|(id, _)| id)
        .collect();
    assert!(
        still_changed.is_empty(),
        "advance_tick should clear Changed flags"
    );
}

#[test]
fn plain_query_returns_all() {
    let mut world = World::new();
    #[allow(clippy::cast_precision_loss)]
    for i in 0..1_000_u32 {
        world.spawn_with(Transform {
            x: i as f32,
            y: 0.0,
            z: 0.0,
        });
    }
    let count = world.query::<Transform>().count();
    assert_eq!(count, 1_000);
}

#[test]
fn changed_query_without_advance_tick_is_empty() {
    // Entities were spawned at tick 1; last_tick is 0.
    // Spawning does NOT bump the change tick, so Changed should be empty initially.
    let mut world = World::new();
    world.spawn_with(Transform {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    });
    // Do NOT advance_tick; last_tick = 0, change_tick = 1.
    // The slot tick starts at 0 (set when column row is pushed, before any mutation).
    // 0 <= 0 so not changed.
    let changed: Vec<EntityId> = world
        .query::<Changed<Transform>>()
        .map(|(id, _)| id)
        .collect();
    assert!(
        changed.is_empty(),
        "spawn should not mark entity as Changed"
    );
}
