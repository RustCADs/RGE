//! `rge-components-render` — render-side ECS components.
//!
//! Failure class: recoverable
//!
//! Mesh / material handles, camera, light variants, B-Rep tessellation
//! handle, reflection probes, skinning binding, LOD slot. Per PLAN.md
//! §1.5.1, mesh + B-Rep + camera + light + reflection-probe entity roles all
//! live here.
//!
//! ## Wave W01 stub
//!
//! `AssetId` and `CadNodeId` ship as local newtypes — same wire format as
//! the eventual `kernel/asset` and `cad-core` types so RON files survive the
//! cross-crate promotion.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod asset_id;
mod brep_handle;
mod cad_node_id;
mod camera;
mod light;
mod lod;
mod material_handle;
mod mesh_handle;
mod reflection_probe;
mod skinned_mesh;

pub use asset_id::{AssetId, NULL_ASSET_ID};
pub use brep_handle::BRepHandle;
pub use cad_node_id::CadNodeId;
pub use camera::{Camera, Projection};
pub use light::{Light, LightKind};
pub use lod::{Lod, LodLevel};
pub use material_handle::MaterialHandle;
pub use mesh_handle::MeshHandle;
pub use reflection_probe::ReflectionProbe;
pub use skinned_mesh::SkinnedMesh;
