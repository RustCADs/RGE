//! Bidirectional ECS ↔ Rapier sync.
//!
//! Two systems run around each step:
//!
//! - [`pre_physics`] — before [`crate::physics_step`]. Reads ECS [`Transform`]
//!   and [`Velocity`] components and pushes them into the Rapier body if the
//!   ECS component changed since last tick. Without change-detection we'd
//!   stomp the solver's integrated state every frame.
//! - [`post_physics`] — after [`crate::physics_step`]. Reads Rapier and
//!   updates the ECS components.
//!
//! The "who is authoritative" question is resolved by **change detection**:
//! the side that wrote last wins for that component. Both sides have to
//! cooperate — the ECS scripting layer must not retain stale `Transform`
//! values across ticks.
//!
//! ## Component shape (stub)
//!
//! `Transform` here is the v0 stub: position + rotation. The real
//! [`rge_components_spatial::Transform`] adds scale, parent-relativity, and a
//! cached world matrix. None of those are needed for the W11 sync surface.

use rapier3d::math::{Pose, Rotation, Vector};
use serde::{Deserialize, Serialize};

use crate::stubs::components_physics::Velocity;
use crate::world::{PhysicsHandle, World};

/// Local twin of the ECS `Transform` component.
///
/// Quaternion convention is `[x, y, z, w]` to match every other ECS surface
/// and the GLTF on-disk format. Internally we re-pack to nalgebra's `[w, x,
/// y, z]` order at the boundary.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// World-space position (m).
    pub position: [f32; 3],
    /// World-space rotation as quaternion `[x, y, z, w]`.
    pub rotation: [f32; 4],
    /// Whether this Transform was written this tick by a non-physics
    /// authority (script, gameplay code). Drives [`pre_physics`].
    pub changed_externally: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            changed_externally: false,
        }
    }
}

impl Transform {
    /// Convenience constructor placing the entity at `position` with identity
    /// rotation.
    #[must_use]
    pub fn at(position: [f32; 3]) -> Self {
        Self {
            position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            changed_externally: false,
        }
    }

    /// Mark as changed externally (script wrote, undo applied, etc.).
    pub fn mark_changed(&mut self) {
        self.changed_externally = true;
    }
}

/// Pre-step sync: ECS → Rapier.
///
/// For each `(handle, transform)` whose `changed_externally` bit is set, push
/// the position into Rapier and clear the bit. Velocities likewise: if the
/// linear or angular vector is non-zero we treat it as a kinematic command
/// for that tick (dynamics keep their integrated velocity untouched).
///
/// Vec slices are mutable so we can clear `changed_externally` in-place. The
/// real ECS query layer will pass `Mut<'_, Transform>` and the change bit
/// goes through `Mut::set_changed()`; the slice-of-tuples shape is just the
/// W11 stub.
pub fn pre_physics(
    world: &mut World,
    transforms: &mut [(PhysicsHandle, Transform)],
    velocities: &mut [(PhysicsHandle, Velocity)],
) {
    for (handle, t) in transforms.iter_mut() {
        if !t.changed_externally {
            continue;
        }
        if let Some(b) = world.bodies.get_mut(handle.body) {
            let translation = Vector::new(t.position[0], t.position[1], t.position[2]);
            let q = Rotation::from_xyzw(t.rotation[0], t.rotation[1], t.rotation[2], t.rotation[3]);
            let pose: Pose = Pose::from_parts(translation, q);
            // KinematicPositionBased uses set_next_kinematic_position so the
            // solver interpolates; everything else uses an instantaneous
            // teleport which wakes neighbours.
            if b.is_kinematic() {
                b.set_next_kinematic_position(pose);
            } else {
                b.set_position(pose, true);
            }
        }
        t.changed_externally = false;
    }

    for (handle, v) in velocities.iter_mut() {
        let any = v.linear.iter().any(|&x| x != 0.0) || v.angular.iter().any(|&x| x != 0.0);
        if !any {
            continue;
        }
        if let Some(b) = world.bodies.get_mut(handle.body) {
            // Setting velocity is non-destructive: scripts opt-in by writing a
            // non-zero vector, dynamics use their solver-integrated velocity
            // when the component is zero.
            b.set_linvel(Vector::new(v.linear[0], v.linear[1], v.linear[2]), true);
            b.set_angvel(Vector::new(v.angular[0], v.angular[1], v.angular[2]), true);
            // Consume the command: subsequent ticks won't re-stomp unless the
            // script writes again.
            *v = Velocity::default();
        }
    }
}

/// Post-step sync: Rapier → ECS.
///
/// After the solver has run, lift the body's pose and velocities back into
/// the ECS. We don't set `changed_externally` here — that bit is reserved
/// for non-physics writers.
pub fn post_physics(
    world: &World,
    transforms: &mut [(PhysicsHandle, Transform)],
    velocities: &mut [(PhysicsHandle, Velocity)],
) {
    for (handle, t) in transforms.iter_mut() {
        if let Some((p, r)) = world.body_pose(*handle) {
            t.position = p;
            t.rotation = r;
        }
    }
    for (handle, v) in velocities.iter_mut() {
        if let Some((linear, angular)) = world.body_velocity(*handle) {
            v.linear = linear;
            v.angular = angular;
        }
    }
}
