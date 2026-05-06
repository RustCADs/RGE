//! `rge-components-interaction` — trigger & sensor markers.
//!
//! [`Trigger`] is the canonical "fires events on volume entry/exit" component
//! (PLAN.md §1.5.1: trigger volume role pairs `Collider` + `Trigger`).
//! [`Sensor`] is the no-collide variant — useful for line-of-sight queries
//! that should not influence physics integration.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod sensor;
mod trigger;

pub use sensor::Sensor;
pub use trigger::Trigger;
