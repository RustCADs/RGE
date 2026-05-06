//! `rge-components-spatial` — cross-crate ECS components for hierarchy + transform.
//!
//! Per [`PLAN.md`](../../plans/PLAN.md) §1.5.1 every camera/mesh/light/audio entity
//! carries a [`Transform`]. [`Parent`] / [`ChildOf`] / [`GlobalTransform`] provide
//! the scene-tree relations consumed by `kernel/ecs::TreeRelationStorage` and
//! transform-propagation systems.
//!
//! ## Wave W01 stub
//!
//! Per the W01 dispatch package, `Entity` is an `u64` newtype stub local to this
//! wave; W02 promotes the canonical type into `kernel/types`. When that lands,
//! callers should `use rge_kernel_types::Entity;` and the local stub is removed.
//!
//! State-only — no behavior, no orchestration (see W01 PLAN exit criteria).

#![forbid(unsafe_code)]

mod child_of;
mod entity;
mod global_transform;
mod parent;
mod transform;

pub use child_of::ChildOf;
pub use entity::Entity;
pub use global_transform::GlobalTransform;
pub use parent::Parent;
pub use transform::Transform;
