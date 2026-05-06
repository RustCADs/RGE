//! `rge-components-physics` — physics-side ECS components.
//!
//! Rigid body class, collider shape, linear & angular velocity, joints, mass
//! / inertia, character controller. Physics simulation lives in
//! `crates/physics` (W11) and consumes these components; this crate is
//! state-only.
//!
//! ## Wave W01 stub
//!
//! `Entity` ships locally as a `u64` newtype (same shape as
//! `components-spatial::Entity`). When `kernel/types::Entity` lands, joints
//! switch to the canonical type and the local stub is removed.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod angular_velocity;
mod character_controller;
mod collider;
mod entity;
mod joint;
mod mass;
mod rigid_body;
mod velocity;

pub use angular_velocity::AngularVelocity;
pub use character_controller::CharacterController;
pub use collider::{Collider, ColliderShape};
pub use entity::Entity;
pub use joint::{Joint, JointKind};
pub use mass::Mass;
pub use rigid_body::{BodyType, RigidBody};
pub use velocity::Velocity;
