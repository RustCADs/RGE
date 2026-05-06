//! `rge-components-animation` — animation-side ECS components.
//!
//! Failure class: recoverable
//!
//! Skeleton handle + bone transform buffer + clip / graph players + IK chain
//! parameters + per-entity event listener config. Animation evaluation lives
//! in `crates/anim-clip` / `anim-graph` / `anim-ik` (W11+); this crate is
//! state-only.
//!
//! ## Wave W01 stubs
//!
//! `Entity` and `AssetId` ship as local newtypes — same shape as the
//! eventual canonical types so RON files survive the swap.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod animation_event_listener;
mod animation_graph_instance;
mod animation_player;
mod asset_id;
mod bone_transforms;
mod entity;
mod ik_chain;
mod skeleton;

pub use animation_event_listener::AnimationEventListener;
pub use animation_graph_instance::AnimationGraphInstance;
pub use animation_player::{AnimationPlayer, PlaybackState};
pub use asset_id::{AssetId, NULL_ASSET_ID};
pub use bone_transforms::BoneTransforms;
pub use entity::Entity;
pub use ik_chain::IkChain;
pub use skeleton::Skeleton;
