// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for Project/Scene/Prefab.
//
//! `rge-data` — RON schemas for `.rge-project`, `.rge-scene`, `.rge-prefab`.
//!
//! Phase 4 deliverable per `IMPLEMENTATION.md` §4.3 and `PLAN.md` §1.6
//! (file format discipline) + §1.6.7 (versioning + migration).
//!
//! # Modules
//!
//! - [`schema_version`] — `SchemaVersion(major: u8, minor: u8, patch: u8)`,
//!   the `version: "x.y.z"` field at the top of every source file.
//! - [`entity_ref`] — [`EntityId`], scene-stable ULID.
//!   `Display` truncates to `e_<8 hex chars>` for diagnostics.
//! - [`asset_ref`] — [`AssetId`], content-addressed (`blake3:<hex>`).
//! - [`project`] — `.rge-project` schema.
//! - [`scene`] — `.rge-scene` schema.
//! - [`prefab`] — `.rge-prefab` schema.
//! - [`migration`] — registered migrations and the `migrate(from, to, text)`
//!   chain walker.
//!
//! # Stubs
//!
//! Per the W14 dispatch package this crate keeps a **local** [`Reflect`]
//! marker stub until the W02 `kernel/types::Reflect` lands. Component
//! payloads on a [`scene::ComponentValue`] are stored as RON text and
//! never instantiated as `dyn Reflect` here — the editor / asset pipeline
//! does that downstream once W02 ships. Removing this stub when W02 lands
//! is a one-line change (`pub use rge_kernel_types::Reflect;`).
//!
//! # File format invariants
//!
//! - Every source file opens with `version: "x.y.z"` (PLAN.md §1.6.7).
//! - Loader walks the [`migration`] chain to bring the payload up to the
//!   current schema before deserialization succeeds.
//! - RON is the single source-format family (PLAN.md §1.6.1); no JSON,
//!   no YAML, no binary fallback at this layer.
//! - Round-trip RON → struct → RON is byte-identical for files at the
//!   current schema (verified by `tests/round_trip.rs`).

#![cfg_attr(not(test), forbid(unsafe_code))]
#![warn(missing_docs)]

pub mod asset_ref;
pub mod entity_ref;
pub mod migration;
pub mod prefab;
pub mod project;
pub mod scene;
pub mod schema_version;

pub use asset_ref::{AssetId, AssetIdParseError};
pub use entity_ref::EntityId;
pub use migration::{builtin, migrate, FileKind, Migration, MigrationError, MigrationRegistry};
pub use prefab::{ExposedOverride, ParamSpec, Prefab};
pub use project::{PluginRef, Project, ScenePath, TargetTier};
pub use scene::{ComponentValue, Entity, Relation, Scene};
pub use schema_version::{SchemaVersion, SchemaVersionParseError};

/// Local `Reflect` stub until `kernel/types::Reflect` (W02) merges.
///
/// This crate never invokes any methods on a `Reflect` impl — the trait
/// only exists so downstream calling code can refer to a stable name when
/// staging integration. Once W02 lands, replace this module with
/// `pub use rge_kernel_types::Reflect;`.
pub mod stub {
    /// Marker trait — the upstream W02 type will replace this. Kept inert
    /// so accidental downstream usage is a no-op rather than a hard error.
    pub trait Reflect: 'static {}
}
