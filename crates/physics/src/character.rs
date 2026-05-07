//! Kinematic capsule character controller.
//!
//! Wraps Rapier's `KinematicCharacterController` for the most common gameplay
//! shape: an upright capsule that walks the world without participating in
//! the dynamics solver itself. It collides correctly (slides along walls,
//! steps up small offsets, doesn't tunnel through floors) and produces a
//! `CharacterMove` describing the actual translation that was achievable
//! given a desired translation.
//!
//! Out of scope for v0.0.1 (W11):
//! - Crouching (height swap mid-frame)
//! - Capsule auto-orientation to slope normal
//! - Custom collision filtering beyond solid/sensor
//!
//! These are addressed in W04+ once script-host needs them.

use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::math::Vector;

use crate::stubs::components_physics::{Collider, ColliderShape};
use crate::world::{PhysicsHandle, World};

/// ECS character-controller component.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CharacterController {
    /// Capsule half-height (cylindrical section).
    pub half_height: f32,
    /// Capsule radius.
    pub radius: f32,
    /// Maximum slope (radians) the character can ascend.
    pub slope_limit: f32,
    /// Vertical offset that's auto-stepped without losing ground contact.
    pub step_offset: f32,
    /// Whether the character snaps to ground when within `step_offset`.
    pub snap_to_ground: bool,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            half_height: 0.9,
            radius: 0.4,
            // 50° default — same as Unity's default character controller.
            slope_limit: std::f32::consts::FRAC_PI_4 + std::f32::consts::FRAC_PI_8,
            step_offset: 0.3,
            snap_to_ground: true,
        }
    }
}

/// Result of a single move call: the achievable translation and contact flags.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct CharacterMove {
    /// Translation actually applied (may be shorter than the desired vector if
    /// the controller hit a wall).
    pub translation: [f32; 3],
    /// Whether the controller is currently grounded.
    pub grounded: bool,
    /// Whether the move slid along a wall during this frame.
    pub slid: bool,
}

impl CharacterController {
    /// Move a kinematic-position-based body by `desired` metres, sliding off
    /// walls and clamping to slopes per the controller config.
    pub fn move_body(
        &self,
        world: &mut World,
        handle: PhysicsHandle,
        desired: [f32; 3],
    ) -> CharacterMove {
        let Some(body) = world.bodies.get(handle.body) else {
            return CharacterMove::default();
        };
        let Some(collider_handle) = body.colliders().first().copied() else {
            return CharacterMove::default();
        };
        let Some(collider) = world.colliders.get(collider_handle).cloned() else {
            return CharacterMove::default();
        };
        let body_pos = *body.position();

        let controller = KinematicCharacterController {
            up: Vector::Y,
            offset: CharacterLength::Absolute(0.01),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(self.step_offset),
                min_width: CharacterLength::Absolute(self.radius * 0.5),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: self.slope_limit,
            min_slope_slide_angle: self.slope_limit * 0.9,
            snap_to_ground: if self.snap_to_ground {
                Some(CharacterLength::Absolute(self.step_offset))
            } else {
                None
            },
            ..KinematicCharacterController::default()
        };

        let desired_v = Vector::new(desired[0], desired[1], desired[2]);
        // Filter excludes the character's own body so we don't self-collide.
        let filter = rapier3d::pipeline::QueryFilter::default().exclude_rigid_body(handle.body);

        // rapier 0.32: `QueryPipeline` is a transient view borrowed from the
        // broadphase BVH (`as_query_pipeline()`) rather than an owned struct
        // we update directly. The broadphase BVH is rebuilt by
        // `pipeline.step`, but consumers may call `move_body` *before* any
        // step (or between steps). We force-refresh here by feeding every
        // collider handle through `BroadPhaseBvh::update`. The internal
        // `needs_broad_phase_update()` change-flag prevents redundant work
        // for unchanged colliders, so the cost amortises to ~insert cost on
        // first call and near-zero after that.
        //
        // We deliberately do NOT call `colliders.take_modified()` here:
        // pairing it with `set_modified()` to restore the deltas would
        // require a `pub(crate)` setter we don't have access to. Instead we
        // walk `iter()` and pass every handle — `update`'s change-flag
        // dedup makes this idempotent. Iter order is arena-slot order, which
        // is stable across runs, preserving §1.6.8 determinism.
        let modified_handles: Vec<rapier3d::geometry::ColliderHandle> =
            world.colliders.iter().map(|(h, _)| h).collect();
        let mut events = Vec::new();
        world.broadphase.update(
            &world.params,
            &world.colliders,
            &world.bodies,
            &modified_handles,
            &[],
            &mut events,
        );

        let query_pipeline = world.broadphase.as_query_pipeline(
            world.narrowphase.query_dispatcher(),
            &world.bodies,
            &world.colliders,
            filter,
        );

        let solved = controller.move_shape(
            crate::step::FIXED_DT,
            &query_pipeline,
            collider.shape(),
            &body_pos,
            desired_v,
            |_| {},
        );

        // Apply the solver's translation to the body.
        if let Some(b) = world.bodies.get_mut(handle.body) {
            let new_pos = body_pos.translation + solved.translation;
            let mut pose = body_pos;
            pose.translation = new_pos;
            if b.is_kinematic() {
                b.set_next_kinematic_position(pose);
            } else {
                b.set_position(pose, true);
            }
        }

        CharacterMove {
            translation: [
                solved.translation.x,
                solved.translation.y,
                solved.translation.z,
            ],
            grounded: solved.grounded,
            slid: solved.is_sliding_down_slope,
        }
    }
}

/// Convenience: build a [`Collider`] matching this controller's capsule.
///
/// Wave W11 doesn't insert the collider for you (the ECS spawn surface still
/// belongs to W01) but tests need a shorthand.
#[must_use]
pub fn capsule_collider(controller: &CharacterController) -> Collider {
    Collider {
        shape: ColliderShape::Capsule {
            half_height: controller.half_height,
            radius: controller.radius,
        },
        density: 1.0,
        friction: 0.5,
        restitution: 0.0,
        is_sensor: false,
    }
}
