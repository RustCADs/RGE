//! `rge-kernel-asset` — canonical content-addressed asset substrate.
//!
//! Failure class: snapshot-recoverable
//!
//! Content-addressed asset substrate per IMPLEMENTATION.md Phase 4.1.
//!
//! Registry corruption (missing asset, dangling Handle) is recoverable by
//! re-loading payloads from disk and replaying the dependency graph. Plain
//! `recoverable` would imply "drop and continue" which loses the dep graph;
//! `snapshot-recoverable` is the precise class.
//!
//! # Modules
//!
//! - [`id`] — [`AssetId`], blake3 content-addressed identifier.
//! - [`handle`] — [`Handle<T>`], typed ref-counted references.
//! - [`registry`] — [`Registry`], in-memory + disk-backed asset store.
//! - [`dependency_graph`] — [`DependencyGraph`], directed dep tracking.

pub mod dependency_graph;
pub mod handle;
pub mod id;
pub mod registry;

pub use dependency_graph::DependencyGraph;
pub use handle::Handle;
pub use id::{AssetId, AssetIdParseError};
pub use registry::{Registry, RegistryError};
