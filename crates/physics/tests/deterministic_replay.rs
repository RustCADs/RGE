//! Deterministic-replay test.
//!
//! Per `tasks/W11/PLAN.md` exit criteria: 1000-tick replay produces
//! byte-identical world state on re-run (same machine, same binary).
//!
//! Strategy: build the same scene twice, step it 1000 ticks each time, and
//! assert `World::serialize_state()` matches at every checkpoint.

use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{
    BodyKind, Collider, ColliderShape, RigidBody, Velocity,
};
use rge_physics::sync::{post_physics, pre_physics, Transform};
use rge_physics::world::World;
use rge_physics::{events, ContactEventChannel};

/// Build a small scene that will exercise contacts, joints, and sleeping.
#[allow(
    clippy::type_complexity,
    reason = "local-only fixture-builder return; tuple is destructured immediately at every call site so naming a one-shot record type adds noise without clarity"
)]
fn build_scene() -> (
    World,
    Vec<(rge_physics::PhysicsHandle, Transform)>,
    Vec<(rge_physics::PhysicsHandle, Velocity)>,
) {
    let mut world = World::new();
    // Ground.
    let _ground = world.insert_body(
        RigidBody {
            kind: BodyKind::Fixed,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Plane,
            ..Collider::default()
        }),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );

    // Stack of three cubes to provoke contact-driven trajectories.
    let mut transforms = Vec::new();
    let mut velocities = Vec::new();
    for i in 0..3 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "test fixture loop bound; i is in 0..3, far below f32 mantissa limit"
        )]
        let y = 1.0 + i as f32 * 1.2;
        let cube = world.insert_body(
            RigidBody {
                kind: BodyKind::Dynamic,
                mass: 1.0,
                ..RigidBody::default()
            },
            Some(Collider {
                shape: ColliderShape::Cuboid {
                    hx: 0.5,
                    hy: 0.5,
                    hz: 0.5,
                },
                ..Collider::default()
            }),
            [0.0, y, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        transforms.push((cube, Transform::at([0.0, y, 0.0])));
        velocities.push((cube, Velocity::default()));
    }
    (world, transforms, velocities)
}

fn run_for(ticks: u64) -> Vec<u8> {
    let (mut world, mut transforms, mut velocities) = build_scene();
    let mut ledger = PhysicsInputLedger::new();
    let events = ContactEventChannel::new();
    for _ in 0..ticks {
        pre_physics(&mut world, &mut transforms, &mut velocities);
        rge_physics::step::physics_step(&mut world, &mut ledger);
        post_physics(&world, &mut transforms, &mut velocities);
        events::drain(&world, &events);
    }
    world.serialize_state()
}

#[test]
fn replay_byte_identical_at_100_ticks() {
    let a = run_for(100);
    let b = run_for(100);
    assert_eq!(
        a.len(),
        b.len(),
        "serialized state lengths diverged at 100 ticks"
    );
    assert_eq!(a, b, "world state diverged at 100 ticks");
}

#[test]
fn replay_byte_identical_at_500_ticks() {
    let a = run_for(500);
    let b = run_for(500);
    assert_eq!(a, b, "world state diverged at 500 ticks");
}

#[test]
fn replay_byte_identical_at_1000_ticks() {
    let a = run_for(1000);
    let b = run_for(1000);
    assert_eq!(a, b, "world state diverged at 1000 ticks");
}

#[test]
fn ledger_records_every_tick() {
    let (mut world, mut transforms, mut velocities) = build_scene();
    let mut ledger = PhysicsInputLedger::new();
    let events = ContactEventChannel::new();
    for _ in 0..50 {
        pre_physics(&mut world, &mut transforms, &mut velocities);
        rge_physics::step::physics_step(&mut world, &mut ledger);
        post_physics(&world, &mut transforms, &mut velocities);
        events::drain(&world, &events);
    }
    assert_eq!(
        ledger.len(),
        50,
        "ledger should have one record per tick (no skipped frames)"
    );
    for (i, record) in ledger.records.iter().enumerate() {
        assert_eq!(
            record.tick, i as u64,
            "ledger tick monotonicity broken at {i}"
        );
    }
}

#[test]
fn ledger_replay_with_external_force_matches() {
    use rge_physics::step::apply_force;

    // Run 1: apply a sideways force every tick for the first 20 ticks, record
    // the trajectory.
    let (mut world1, mut t1, mut v1) = build_scene();
    let mut ledger1 = PhysicsInputLedger::new();
    let events1 = ContactEventChannel::new();
    let cube_a = t1[0].0;
    for tick in 0..60 {
        if tick < 20 {
            apply_force(&mut world1, &mut ledger1, cube_a, [0.5, 0.0, 0.0]);
        }
        pre_physics(&mut world1, &mut t1, &mut v1);
        rge_physics::step::physics_step(&mut world1, &mut ledger1);
        post_physics(&world1, &mut t1, &mut v1);
        events::drain(&world1, &events1);
    }
    let trajectory_1 = world1.serialize_state();

    // Run 2: replay using the recorded ledger only.
    let (mut world2, mut t2, mut v2) = build_scene();
    let mut ledger2 = PhysicsInputLedger::new();
    let events2 = ContactEventChannel::new();
    let cube_a2 = t2[0].0;
    // Build a body-id → handle map so the replay can resolve recorded ids.
    // Both worlds were built identically so the order is the same.
    let id_to_handle = |id: u64| -> Option<rge_physics::PhysicsHandle> {
        if id == cube_a2.id() {
            Some(cube_a2)
        } else {
            None
        }
    };
    for _ in 0..60 {
        // Apply the recorded inputs onto world2 instead of generating fresh ones.
        rge_physics::step::apply_recorded_inputs(&mut world2, &ledger1, id_to_handle);
        pre_physics(&mut world2, &mut t2, &mut v2);
        rge_physics::step::physics_step(&mut world2, &mut ledger2);
        post_physics(&world2, &mut t2, &mut v2);
        events::drain(&world2, &events2);
    }
    let trajectory_2 = world2.serialize_state();
    assert_eq!(
        trajectory_1, trajectory_2,
        "ledger replay diverged from original run"
    );
}
