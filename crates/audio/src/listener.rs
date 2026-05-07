//! [`AudioListener`](crate::AudioListener) ↔ Kira listener bridge.
//!
//! Per [`PLAN.md`](../../plans/PLAN.md) §1.5.1 the Camera entity is the
//! canonical listener carrier. Position + orientation come from the entity's
//! [`Transform`](crate::Transform); only listener-specific state (master gain
//! plus the engine-side Kira `ListenerHandle`) lives in this module.
//!
//! The schedule step calls [`ListenerState::sync_pose`] each tick to pipe
//! Transform updates into Kira's spatial mixer.

use kira::listener::ListenerHandle;
use kira::Tween;

use crate::components::Transform;

/// Engine-side bookkeeping for a registered [`AudioListener`](crate::AudioListener).
///
/// One per ECS world — multi-listener (split-screen) is a post-W12 reach
/// item. Held inside the [`AudioManager`](crate::AudioManager) keyed by the
/// listener's [`Entity`](crate::components::Entity).
///
/// Kira 0.12 model: a single shared "anchor" [`ListenerHandle`] is created on
/// the [`AudioManager`](crate::AudioManager) at construction time. This struct
/// caches the last-applied pose so that [`Self::sync_pose`] can skip the Kira
/// command channel when nothing changed; the actual `set_position` /
/// `set_orientation` calls go through the shared anchor handle owned by the
/// manager.
#[derive(Debug)]
pub struct ListenerState {
    /// Cached last-applied position; skip Kira command if pose hasn't moved.
    pub(crate) last_position: [f32; 3],
    /// Cached last-applied orientation quaternion `(x, y, z, w)`.
    pub(crate) last_rotation: [f32; 4],
}

impl ListenerState {
    /// Construct an entry that mirrors the manager's anchor listener at the
    /// given pose. The pose is treated as the "last applied" snapshot so a
    /// subsequent identical `sync_pose` is a no-op (matches the manager's
    /// register-time anchor reposition).
    #[must_use]
    pub(crate) fn new_anchor(transform: &Transform) -> Self {
        Self {
            last_position: transform.position,
            last_rotation: transform.rotation,
        }
    }

    /// Push the entity's pose into Kira if it changed since the last call.
    /// Uses Kira's instant tween — listener pose rarely needs smoothing
    /// because the ECS update is already running at the simulation rate.
    ///
    /// `anchor` is the manager's shared anchor [`ListenerHandle`].
    pub fn sync_pose(&mut self, transform: &Transform, anchor: &mut ListenerHandle) {
        if approx_eq3(self.last_position, transform.position)
            && approx_eq4(self.last_rotation, transform.rotation)
        {
            return;
        }

        let position = mint::Vector3 {
            x: transform.position[0],
            y: transform.position[1],
            z: transform.position[2],
        };
        let orientation = mint::Quaternion {
            v: mint::Vector3 {
                x: transform.rotation[0],
                y: transform.rotation[1],
                z: transform.rotation[2],
            },
            s: transform.rotation[3],
        };

        // Tween::default() = 10ms linear — small enough to feel immediate,
        // large enough to avoid audible zipper noise on rapid camera moves.
        anchor.set_position(position, Tween::default());
        anchor.set_orientation(orientation, Tween::default());

        self.last_position = transform.position;
        self.last_rotation = transform.rotation;
    }
}

fn approx_eq3(a: [f32; 3], b: [f32; 3]) -> bool {
    (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5 && (a[2] - b[2]).abs() < 1e-5
}

fn approx_eq4(a: [f32; 4], b: [f32; 4]) -> bool {
    (a[0] - b[0]).abs() < 1e-5
        && (a[1] - b[1]).abs() < 1e-5
        && (a[2] - b[2]).abs() < 1e-5
        && (a[3] - b[3]).abs() < 1e-5
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `approx_eq3` catches identical, equal-within-tolerance, and far-apart inputs.
    #[test]
    fn approx_eq3_correct() {
        assert!(approx_eq3([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]));
        assert!(approx_eq3([1.0, 2.0, 3.0], [1.0 + 1e-7, 2.0, 3.0]));
        assert!(!approx_eq3([1.0, 2.0, 3.0], [1.5, 2.0, 3.0]));
    }

    /// `new_anchor` records the supplied transform as last-applied; `sync_pose`
    /// for an identical pose is therefore a no-op (no anchor mutation needed).
    #[test]
    fn new_anchor_seeds_last_pose() {
        let xform = crate::components::Transform::from_position([1.0, 2.0, 3.0]);
        let state = super::ListenerState::new_anchor(&xform);
        assert!(approx_eq3(state.last_position, [1.0, 2.0, 3.0]));
        assert!(approx_eq4(state.last_rotation, [0.0, 0.0, 0.0, 1.0]));
    }
}
