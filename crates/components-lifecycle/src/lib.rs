//! `rge-components-lifecycle` — spawn / despawn / age components.
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
