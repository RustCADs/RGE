//! `rge-components-lifecycle` — spawn / despawn / age components.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: state-only marker crate; pure component definitions
//! consumed by spawner / despawner / aging systems. Owns no PIE state and
//! emits no runtime errors. Mirrors the components-render /
//! components-animation / components-audio / components-identity
//! classification.
//!
//! [`Spawn`] is a one-tick marker the spawner adds so the next frame's
//! systems can run "born this tick" logic without a separate event channel.
//! [`Despawn`] is the deferred-removal marker — the despawner removes it
//! along with the entity at end-of-frame. [`Age`] tracks ticks since spawn
//! for cooldowns / TTLs.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod age;
mod despawn;
mod spawn;

pub use age::Age;
pub use despawn::Despawn;
pub use spawn::Spawn;
