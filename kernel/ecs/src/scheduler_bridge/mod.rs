//! `kernel_ecs::scheduler_bridge` — re-exports for `kernel/schedule` integration.
//!
//! This module re-exports the subset of the ECS public API that `kernel/schedule`
//! systems need to receive as parameters.  The actual type definitions live in
//! their respective modules; only re-exports live here.
//!
//! Currently a thin re-export layer.  Future phases may add system-parameter
//! derivation helpers and injection utilities.

pub use crate::commands::Commands;
pub use crate::query::Query;
pub use crate::resource::Res;
pub use crate::world::World;
