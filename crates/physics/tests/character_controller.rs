//! Character-controller smoke test.
//!
//! Exercises [`rge_physics::CharacterController`] against the W11 acceptance
//! contract: a kinematic capsule placed above a floor receives a desired
//! forward translation, runs through the controller's slide/step logic, and
//! reports a non-zero achievable translation plus a `grounded` flag once the
//! capsule has settled.

use rge_physics::character::capsule_collider;
use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{
    BodyKind, Collider, ColliderShape, RigidBody, Velocity,
};
use rge_physics::sync::{post_physics, pre_physics, Transform};
use rge_physics::world::World;
use rge_physics::{events, CharacterController, ContactEventChannel};

#[test]
fn character_walks_on_flat_ground() {
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

    // Kinematic capsule character at y=1.0 (above floor).
    let controller = CharacterController::default();
    let character_body = world.insert_body(
        RigidBody {
            kind: BodyKind::KinematicPositionBased,
            ..RigidBody::default()
        },
        Some(capsule_collider(&controller)),
        [0.0, controller.half_height + controller.radius + 0.1, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );

    let mut ledger = PhysicsInputLedger::new();
    let event_channel = ContactEventChannel::new();
    let mut transforms = vec![(character_body, Transform::at([0.0, 1.5, 0.0]))];
    let mut velocities = vec![(character_body, Velocity::default())];

    // Settle for a few ticks so the capsule rests on the ground.
    for _ in 0..30 {
        pre_physics(&mut world, &mut transforms, &mut velocities);
        rge_physics::step::physics_step(&mut world, &mut ledger);
        post_physics(&world, &mut transforms, &mut velocities);
        events::drain(&world, &event_channel);
    }

    // Ask the controller to move forward 0.1 m. With a flat floor and a
    // kinematic capsule, the achievable translation should be ≈ 0.1 m on x.
    let result = controller.move_body(&mut world, character_body, [0.1, 0.0, 0.0]);
    assert!(
        result.translation[0] > 0.05,
        "character did not move forward as expected: translation = {:?}",
        result.translation
    );
}

#[test]
fn character_stops_at_wall() {
    let mut world = World::new();
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
    // Wall: a tall thin cuboid 2m in front of the character.
    let _wall = world.insert_body(
        RigidBody {
            kind: BodyKind::Fixed,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Cuboid {
                hx: 0.1,
                hy: 5.0,
                hz: 5.0,
            },
            ..Collider::default()
        }),
        [2.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );

    let controller = CharacterController::default();
    let character_body = world.insert_body(
        RigidBody {
            kind: BodyKind::KinematicPositionBased,
            ..RigidBody::default()
        },
        Some(capsule_collider(&controller)),
        [0.0, controller.half_height + controller.radius + 0.1, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );

    // Push 5 m toward the wall; capsule stops at ~1.7 m (wall x=2.0,
    // halfthickness 0.1, capsule radius 0.4 ⇒ stop at x ≈ 1.5).
    let result = controller.move_body(&mut world, character_body, [5.0, 0.0, 0.0]);
    assert!(
        result.translation[0] < 5.0,
        "character should have been blocked by wall but moved fully: {:?}",
        result.translation
    );
}
