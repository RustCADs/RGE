//! `rge-components-identity` — naming + cross-store identity references.
//!
//! Failure class: recoverable
//!
//! [`Name`] is the user-visible label that shows up in the editor outliner;
//! [`AssetRef`] points at a `kernel/asset` payload (mesh, texture, audio
//! clip); [`CadRef`] points into the `cad-core` graph for B-Rep entities
//! (PLAN.md §1.5.1 — B-Rep role).
//!
//! ## Wave W01 stubs
//!
//! `AssetId` and `CadNodeId` ship locally as `u64` newtypes. W14 (`rge-data`)
//! / cad-core promotion replace them; the wire format (`u64` payload) is
//! chosen so RON files survive the eventual swap.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod asset_id;
mod asset_ref;
mod cad_node_id;
mod cad_ref;
mod name;

pub use asset_id::{AssetId, NULL_ASSET_ID};
pub use asset_ref::AssetRef;
pub use cad_node_id::CadNodeId;
pub use cad_ref::CadRef;
pub use name::Name;
