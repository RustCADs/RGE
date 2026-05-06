//! `rge-kernel-graph-foundation` — Tier-1 graph substrate for the RGE engine.
//!
//! Failure class: snapshot-recoverable
//!
//! Substrate for all 8+ graph systems per PLAN.md §1.14. Provides
//! [`NodeId`]/[`EdgeId`] primitives, structural hashing via [`StableHash`],
//! snapshot/restore, structural diff, and invalidation propagation.
//!
//! Does NOT provide domain-specific traversal, evaluation, or semantics —
//! each graph domain owns those. Cross-domain semantic unification is
//! explicitly out of scope (would be the "god-substrate" anti-pattern).
//!
//! # Architecture
//!
//! * [`id`] — stable, content-derived 128-bit node and edge identifiers.
//! * [`stable_hash`] — generic interface for structural hashing into BLAKE3.
//! * [`graph`] — generic mutable graph container (BTreeMap-backed, deterministic).
//! * [`snapshot`] — immutable Arc-wrapped snapshot with RON serialization.
//! * [`diff`] — structural diff between two snapshots.
//! * [`invalidation`] — dirty-bit propagation through dependency DAGs.
//! * [`viz_adapter`] — trait surface for editor graph-viewer widgets.

#![forbid(unsafe_code)]

pub mod diff;
pub mod graph;
pub mod id;
pub mod invalidation;
pub mod snapshot;
pub mod stable_hash;
pub mod viz_adapter;

pub use diff::GraphDiff;
pub use graph::{EdgeRecord, Graph, GraphError};
pub use id::{EdgeId, NodeId};
pub use invalidation::{Invalidation, InvalidationListener, ListenerHandle};
pub use snapshot::{GraphSnapshot, SnapshotError};
pub use stable_hash::{stable_edge_id, stable_node_id, StableHash};
pub use viz_adapter::{EdgeView, NodeView, VizAdapter};
