//! `rge-kernel-types` — reflection registry (architectural root).
//!
//! Failure class: recoverable
//!
//! adapted from `rustforge::macros::rcad-property` on 2026-05-05 — generalized to
//! generic `Reflect` trait + `UiHint` closed-set + `SchemaVersion`.
//!
//! # Why this crate is the architectural root
//!
//! Per `IMPLEMENTATION.md` Phase 1.1 and `PLAN.md` §1.2.4 / §6.15: every later
//! subsystem (editor inspector, hot-reload migration, scripting bridge, asset
//! metadata, RON serde for project files) walks values through `Reflect`. The
//! reflection layer therefore cannot be slow (compile-time gate: 5 pilot types
//! must compile in <30s — see `BUDGET.md`).
//!
//! # Surface
//!
//! - [`TypeId`] — content-derived 128-bit identity, interned at compile time.
//! - [`FieldDescriptor`] + [`RangeMeta`] + [`DefaultValue`] — per-field metadata
//!   emitted by `#[derive(Reflect)]` (see `rge-macros-reflect`).
//! - [`UiHint`] — closed-set vocabulary for inspector binding (§6.15). New
//!   variants require the CI lint to allow them; this list is intentionally
//!   small.
//! - [`Reflect`] — the trait `#[derive(Reflect)]` implements. Carries the
//!   `SCHEMA_VERSION` constant (every reflected type MUST declare a version).
//! - [`serde_bridge`] — RON round-trip via reflection walk; pilot test asserts
//!   byte-identical re-serialization.
//!
//! # Hand-rolled vs dep-pull (PLAN.md §1.10)
//!
//! Uses only the workspace-pinned `serde`, `ron`, `thiserror`. No
//! `bevy_reflect`, no `inventory`, no `linkme`, no `paste`, no `blake3` —
//! the type-id hash is hand-rolled FNV-1a-128 (see `type_id.rs`). Every
//! helper is either inlined or built on workspace floor crates. This keeps
//! the incremental-invalidation radius small (PLAN.md §1.10.4 last metric)
//! and avoids the workspace's `cpufeatures 0.3.0 / edition2024` blocker.
//!
//! # Forbidden patterns
//!
//! - **No global registry at runtime.** Each reflected type exposes its
//!   `FieldDescriptor` slice as a `&'static` constant. A future `inventory!`
//!   crate is explicitly out of scope (PLAN.md §1.10 dynamic-island policy:
//!   tooling layers can use `dyn Reflect` trait objects, no global table).
//! - **No `Any` downcast for field access.** `set_field_dyn` works on a
//!   purpose-shaped `ReflectValue` enum (in `serde_bridge`), not `dyn Any`.
//!   This keeps the surface auditable.

#![cfg_attr(not(test), forbid(unsafe_code))]
#![warn(missing_docs)]

pub mod field_descriptor;
pub mod reflect;
pub mod schema_version;
pub mod serde_bridge;
pub mod type_id;
pub mod ui_hint;

pub use field_descriptor::{DefaultValue, FieldDescriptor, RangeMeta};
pub use reflect::{Reflect, ReflectError, ReflectKind, ReflectObject};
pub use schema_version::SchemaVersion;
pub use serde_bridge::{from_ron, to_ron, to_ron_pretty, ReflectValue, SerdeBridgeError};
pub use type_id::TypeId;
pub use ui_hint::UiHint;
