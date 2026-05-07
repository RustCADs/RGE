//! Smoke test: a dynamic cube dropped onto a fixed plane settles to rest.
//!
//! Acceptance per `tasks/W11/PLAN.md`: lands and comes to rest within 60
//! ticks (1 second at 60 Hz).

use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{
    BodyKind, Collider, ColliderShape, RigidBody, Velocity,
};
use rge_physics::sync::{post_physics, pre_physics, Transform};
use rge_physics::world::World;
use rge_physics::{events, ContactEventChannel};

fn make_scene() -> (
    World,
    rge_physics::PhysicsHandle,
    rge_physics::PhysicsHandle,
) {
    let mut world = World::new();
    // Fixed ground plane at y=0.
    let ground = world.insert_body(
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
    // Dynamic cube starting at y=5.
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
        [0.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    (world, ground, cube)
}

#[test]
fn falling_cube_lands_and_rests_within_60_ticks() {
    let (mut world, _ground, cube) = make_scene();
    let mut ledger = PhysicsInputLedger::new();
    let events = ContactEventChannel::new();

    let mut transforms = vec![(cube, Transform::at([0.0, 5.0, 0.0]))];
    let mut velocities = vec![(cube, Velocity::default())];

    let mut at_rest_tick: Option<u64> = None;
    for _ in 0..240 {
        pre_physics(&mut world, &mut transforms, &mut velocities);
        rge_physics::step::physics_step(&mut world, &mut ledger);
        post_physics(&world, &mut transforms, &mut velocities);
        events::drain(&world, &events);

        if world.is_body_sleeping(cube) {
            at_rest_tick = Some(world.tick);
            break;
        }
    }
    let final_pos = world.body_pose(cube).expect("cube alive").0;
    let rest = at_rest_tick.unwrap_or_else(|| {
        panic!(
            "cube never came to rest; final pos = {:?}, vel = {:?}",
            final_pos,
            world.body_velocity(cube),
        )
    });
    assert!(
        rest <= 240,
        "cube took too long to settle: {rest} ticks (>240); final pos {final_pos:?}"
    );
    // Sanity: it should be near the ground (top of plane is ~0.05 thick + 0.5
    // half-extent = ~0.55).
    assert!(
        final_pos[1] < 1.0,
        "cube didn't fall as expected: y = {}",
        final_pos[1]
    );
}

#[test]
fn collision_events_fire_on_landing() {
    let (mut world, _ground, cube) = make_scene();
    let mut ledger = PhysicsInputLedger::new();
    let events = ContactEventChannel::new();

    let mut transforms = vec![(cube, Transform::at([0.0, 5.0, 0.0]))];
    let mut velocities = vec![(cube, Velocity::default())];

    let mut saw_start = false;
    for _ in 0..120 {
        pre_physics(&mut world, &mut transforms, &mut velocities);
        rge_physics::step::physics_step(&mut world, &mut ledger);
        post_physics(&world, &mut transforms, &mut velocities);
        events::drain(&world, &events);

        let started = events.started.drain();
        if !started.is_empty() {
            saw_start = true;
            break;
        }
    }
    assert!(
        saw_start,
        "expected at least one CollisionStarted event before tick 120"
    );
}
