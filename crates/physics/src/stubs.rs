//! Local twins of stub-state upstream crates so `rge-physics` compiles
//! standalone during W11.
//!
//! Per the W11 dispatch (`tasks/W11/PLAN.md` "Stubs needed"), the upstream
//! crates `components-physics` (W01), `kernel/events`, and
//! `kernel/audit-ledger` are still stubs. We mirror **only** the surface this
//! crate consumes. When those waves land, this module is deleted and `pub use`
//! re-exports in [`crate::lib`] swap to the real crates.
//!
//! ### Migration map (post-merge)
//!
//! | this crate path | future canonical path |
//! |---|---|
//! | `stubs::components_physics::RigidBody` | `rge_components_physics::RigidBody` |
//! | `stubs::kernel_events::Channel<T>` | `rge_kernel_events::Channel<T>` |
//!
//! `stubs::audit_ledger::AuditLedger` was promoted out of `stubs` 2026-05-09
//! audit-debt closure and renamed to [`crate::physics_input_ledger::PhysicsInputLedger`]
//! — it was never structurally compatible with the kernel/audit-ledger
//! generic event substrate (different domain, different API). Physics
//! intentionally owns its own per-tick input ledger; the rename makes that
//! explicit. See [`crate::physics_input_ledger`] module-level docs.

/// Mirror of the to-be-W01 `components-physics` surface.
pub mod components_physics {
    use serde::{Deserialize, Serialize};

    /// How a body participates in simulation.
    ///
    /// Mirrors Rapier's `RigidBodyType` but in ECS-component form: this is the
    /// authoring layer's view, [`crate::sync`] is responsible for translating.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
    pub enum BodyKind {
        /// Driven by the solver (forces, gravity, contacts).
        Dynamic,
        /// Solver-immovable; receives no forces. Geometry only.
        Fixed,
        /// Position-controlled by gameplay code (character controllers,
        /// elevators, doors). Solver pushes other dynamics out of the way but
        /// never moves the kinematic itself.
        KinematicPositionBased,
        /// Velocity-controlled kinematic.
        KinematicVelocityBased,
    }

    /// ECS rigid-body component. Drives [`crate::sync::pre_physics`].
    #[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct RigidBody {
        /// Simulation participation kind.
        pub kind: BodyKind,
        /// Mass (kg). Ignored for non-dynamic bodies.
        pub mass: f32,
        /// Linear damping (per-axis multiplier; 0 = none).
        pub linear_damping: f32,
        /// Angular damping.
        pub angular_damping: f32,
        /// If true the body never sleeps (use sparingly — defeats islanding).
        pub never_sleep: bool,
    }

    impl Default for RigidBody {
        fn default() -> Self {
            Self {
                kind: BodyKind::Dynamic,
                mass: 1.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                never_sleep: false,
            }
        }
    }

    /// Collider shape vocabulary at the ECS authoring layer.
    ///
    /// Closed enum on purpose: arbitrary mesh colliders force per-frame upload
    /// hot paths and break determinism guarantees. Add cases here only after a
    /// dedicated wave.
    #[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub enum ColliderShape {
        /// Axis-aligned box centred on the entity (half-extents in metres).
        Cuboid {
            /// Half-extent on X.
            hx: f32,
            /// Half-extent on Y.
            hy: f32,
            /// Half-extent on Z.
            hz: f32,
        },
        /// Sphere of given radius.
        Ball {
            /// Radius in metres.
            radius: f32,
        },
        /// Y-axis-aligned capsule.
        Capsule {
            /// Half height of the cylindrical section.
            half_height: f32,
            /// Radius.
            radius: f32,
        },
        /// Infinite Y-up plane (we model it as a very flat large cuboid for
        /// solver convenience). Used for ground in tests.
        Plane,
    }

    /// ECS collider component.
    #[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct Collider {
        /// Geometry.
        pub shape: ColliderShape,
        /// Density (kg/m³); used to derive mass when [`RigidBody::mass`] is
        /// not explicitly authored.
        pub density: f32,
        /// Coulomb friction coefficient.
        pub friction: f32,
        /// Restitution (bounciness), 0–1.
        pub restitution: f32,
        /// If true the collider only fires sensor events; no contact response.
        pub is_sensor: bool,
    }

    impl Default for Collider {
        fn default() -> Self {
            Self {
                shape: ColliderShape::Cuboid {
                    hx: 0.5,
                    hy: 0.5,
                    hz: 0.5,
                },
                density: 1.0,
                friction: 0.5,
                restitution: 0.0,
                is_sensor: false,
            }
        }
    }

    /// ECS velocity component (linear + angular).
    #[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    pub struct Velocity {
        /// Linear velocity (m/s) in world space.
        pub linear: [f32; 3],
        /// Angular velocity (rad/s) about world axes.
        pub angular: [f32; 3],
    }
}

/// Mirror of the to-be `kernel/events` `Channel<T>`.
///
/// Production version is MPSC + reader-cursor; v0 here is a `Vec<T>` flushed
/// once per frame, which is enough for the W11 contact-event tests and keeps
/// the determinism story trivial (insertion order = solver order).
pub mod kernel_events {
    use std::cell::RefCell;

    /// Single-producer, drain-once-per-frame channel.
    #[derive(Debug)]
    pub struct Channel<T> {
        buf: RefCell<Vec<T>>,
    }

    // Manual Default impl: deriving requires `T: Default`, but `Vec<T>::new()`
    // doesn't. Avoiding the derive lets event payloads stay non-Default.
    impl<T> Default for Channel<T> {
        fn default() -> Self {
            Self {
                buf: RefCell::new(Vec::new()),
            }
        }
    }

    impl<T> Channel<T> {
        /// Construct empty channel.
        #[must_use]
        pub fn new() -> Self {
            Self {
                buf: RefCell::new(Vec::new()),
            }
        }

        /// Push an event.
        pub fn push(&self, event: T) {
            self.buf.borrow_mut().push(event);
        }

        /// Take all events. Subsequent reads return empty until next push.
        pub fn drain(&self) -> Vec<T> {
            std::mem::take(&mut *self.buf.borrow_mut())
        }

        /// Number of pending events without consuming.
        pub fn len(&self) -> usize {
            self.buf.borrow().len()
        }

        /// Whether the channel is empty.
        pub fn is_empty(&self) -> bool {
            self.buf.borrow().is_empty()
        }
    }
}

// `audit_ledger` module moved to `crate::physics_input_ledger` 2026-05-09
// (audit-debt MEDIUM closure: type renamed AuditLedger → PhysicsInputLedger
// to stop presenting it as a "stub" of the kernel substrate; see
// physics_input_ledger.rs module-level docs for full rationale).
