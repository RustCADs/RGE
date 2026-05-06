// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets

//! Asset dependency graph — track "asset A depends on B and C" edges
//! so a content change to a leaf can cascade-invalidate everything
//! downstream.
//!
//! # Why this lives next to the cache
//!
//! Cooked assets are content-addressed by *their own bytes*, but their
//! freshness depends on the bytes of every input that fed the cooker.
//! Example: a cooked-pak (W15) referencing an imported glTF (W17) is
//! "fresh" only as long as the glTF source bytes haven't changed.
//! When the source changes, the cooker re-runs, produces new bytes, a
//! new `AssetId` — and everyone downstream needs to be told.
//!
//! The graph here is the in-memory mechanism for that "told"
//! propagation. Persisting it across runs is a future concern (would
//! sit alongside the cache `.index` file).
//!
//! # Substrate
//!
//! Backed by [`rge_kernel_graph_foundation::Graph<AssetId, ()>`] per PLAN §1.14
//! (graph-foundation substrate doctrine — asset-store is a Tier-2 graph
//! consumer that must build on the substrate rather than reinvent
//! `BTreeMap<K, BTreeSet<K>>` adjacency). Mirrors the migration applied to
//! `kernel/asset::DependencyGraph` (audit-1 followup, 2026-05-09).
//!
//! Each [`AssetId`] used as an endpoint of [`add_edge`](DepGraph::add_edge)
//! is auto-promoted to a graph node, with its [`NodeId`] derived via
//! [`NodeId::from_bytes`] over the `AssetId`'s raw 32-byte blake3 digest. The
//! mapping is therefore deterministic and reversible: we recover the `AssetId`
//! from a `NodeId` by looking up the node payload in the underlying `Graph`.
//!
//! # Idempotence vs. graph-foundation's stricter contract
//!
//! `Graph::insert_node` errors on duplicate ids and `Graph::insert_edge` errors
//! on duplicate edge ids. The original `DepGraph::add_edge` semantics are
//! idempotent — calling twice with the same pair has no extra effect — so the
//! wrappers below swallow `DuplicateNode` / `DuplicateEdge` rather than
//! propagating them.
//!
//! # Cycles
//!
//! Cycles are a configuration error — an asset can't depend on
//! itself, directly or transitively. [`DepGraph::add_edge`] checks for
//! self-edges; transitive cycles are caught by
//! [`DepGraph::transitive_closure`] / [`DepGraph::invalidation_cascade`]
//! returning a [`DepError::Cycle`].
//!
//! graph-foundation's `Graph<N, E>` does NOT itself detect cycles (consistent
//! with `cad-core::OperatorGraph` and `kernel/asset::DependencyGraph`, which
//! both implement their own cycle detection on top of the substrate). We
//! preserve the existing iterative walks below, swapping only the underlying
//! adjacency storage.
//!
//! # Anti-pattern check
//!
//! This is *not* a build system. The graph here only carries edges;
//! the rebuild logic (which cooker to run, in what order) lives in
//! `crates/build-pipeline` (W-future). Adding cooker plumbing here
//! would re-create the kind of "store paralleling cad-core or ECS"
//! that PLAN §1.4 anti-patterns warn against.

use std::collections::BTreeSet;

use rge_kernel_graph_foundation::{EdgeId, Graph, NodeId};

use crate::AssetId;

/// Errors emitted by [`DepGraph`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DepError {
    /// Tried to add a self-edge (`add_edge(a, a)`).
    #[error("dep_graph: self-edges are not permitted (asset {0})")]
    SelfEdge(AssetId),
    /// Detected a cycle while walking the graph; the cycle includes
    /// the listed asset.
    #[error("dep_graph: cycle detected involving asset {0}")]
    Cycle(AssetId),
}

/// Directed graph of "consumer depends on producer" edges.
///
/// Backed by [`rge_kernel_graph_foundation::Graph<AssetId, ()>`]; iteration
/// order is deterministic (BTreeMap-backed in the substrate). Both forward
/// (`who do I depend on?`) and reverse (`who depends on me?`) queries are
/// O(degree) via the substrate's `outgoing` / `incoming` adjacency caches.
#[derive(Debug, Clone)]
pub struct DepGraph {
    /// Substrate-backed directed graph: nodes carry the original [`AssetId`]
    /// payload, edges are unit (no per-edge metadata).
    inner: Graph<AssetId, ()>,
}

// Manual `Default` impl: graph-foundation's `Graph<N, E>` derives `Default`
// which the macro expands into a `where N: Default` bound. `AssetId` has no
// `Default` (a default 32-byte hash would be meaningless), so we construct
// the empty inner graph directly via `Graph::new` instead. Mirrors the
// equivalent impl in `kernel/asset::DependencyGraph`.
impl Default for DepGraph {
    fn default() -> Self {
        Self {
            inner: Graph::new(),
        }
    }
}

impl DepGraph {
    /// Construct an empty dep graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `consumer` depends on `producer`.
    ///
    /// Idempotent: re-adding the same edge is a no-op. The producer
    /// and consumer are *not* required to already be in the graph —
    /// they spring into existence as endpoints of edges. (The graph
    /// doesn't know about isolated nodes, by design — they have no
    /// invalidation effect.)
    ///
    /// # Errors
    ///
    /// Returns [`DepError::SelfEdge`] if `consumer == producer`.
    pub fn add_edge(&mut self, consumer: AssetId, producer: AssetId) -> Result<(), DepError> {
        if consumer == producer {
            return Err(DepError::SelfEdge(consumer));
        }
        let consumer_id = node_id_for(consumer);
        let producer_id = node_id_for(producer);
        // Auto-promote both endpoints to nodes if not yet present. Both
        // `insert_node` calls swallow `DuplicateNode` because re-adding is a
        // no-op in the original API.
        let _ = self.inner.insert_node(consumer_id, consumer);
        let _ = self.inner.insert_node(producer_id, producer);
        // Edge id derived from endpoints so the same pair always produces the
        // same EdgeId; `insert_edge` swallows `DuplicateEdge` so a second
        // `add_edge(consumer, producer)` is a no-op.
        let edge_id = edge_id_for(consumer_id, producer_id);
        let _ = self
            .inner
            .insert_edge(edge_id, consumer_id, producer_id, ());
        Ok(())
    }

    /// Remove a single edge. No-op if the edge isn't present.
    pub fn remove_edge(&mut self, consumer: AssetId, producer: AssetId) {
        let consumer_id = node_id_for(consumer);
        let producer_id = node_id_for(producer);
        let edge_id = edge_id_for(consumer_id, producer_id);
        // Swallow `EdgeNotFound`: the original API silently no-ops on absent
        // edges. Endpoint nodes are intentionally left in the graph; the
        // original `BTreeMap`-backed impl pruned empty adjacency entries so
        // they re-emerged on the next `add_edge`. The substrate-backed impl
        // keeps zero-degree nodes resident (the public `forget`/`is_empty`
        // surface is unaffected because `edge_count()` is the canonical
        // emptiness measure).
        let _ = self.inner.remove_edge(edge_id);
    }

    /// All assets that `consumer` directly depends on.
    ///
    /// Returned in deterministic [`AssetId`] sort order so test snapshots and
    /// graph diffs are reproducible.
    #[must_use]
    pub fn direct_dependencies(&self, consumer: &AssetId) -> Vec<AssetId> {
        let node_id = node_id_for(*consumer);
        // Collect through a BTreeSet to match the original BTreeSet-backed
        // ordering (sorted by AssetId, deduplicated).
        let mut out: BTreeSet<AssetId> = BTreeSet::new();
        for eid in self.inner.outgoing(node_id) {
            if let Some(rec) = self.inner.edge(eid) {
                if let Some(asset) = self.inner.node(rec.dst) {
                    out.insert(*asset);
                }
            }
        }
        out.into_iter().collect()
    }

    /// All assets that directly depend on `producer`.
    ///
    /// Returned in deterministic [`AssetId`] sort order so test snapshots and
    /// graph diffs are reproducible.
    #[must_use]
    pub fn direct_dependents(&self, producer: &AssetId) -> Vec<AssetId> {
        let node_id = node_id_for(*producer);
        let mut out: BTreeSet<AssetId> = BTreeSet::new();
        for eid in self.inner.incoming(node_id) {
            if let Some(rec) = self.inner.edge(eid) {
                if let Some(asset) = self.inner.node(rec.src) {
                    out.insert(*asset);
                }
            }
        }
        out.into_iter().collect()
    }

    /// All assets transitively reachable from `consumer` via the
    /// `depends-on` direction. `consumer` itself is *not* included.
    ///
    /// # Errors
    ///
    /// Returns [`DepError::Cycle`] if the graph contains a cycle that
    /// would make the closure non-terminating.
    pub fn transitive_closure(&self, consumer: &AssetId) -> Result<Vec<AssetId>, DepError> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<AssetId> = self.direct_dependencies(consumer);
        while let Some(node) = stack.pop() {
            if node == *consumer {
                return Err(DepError::Cycle(node));
            }
            if !out.insert(node) {
                continue;
            }
            for dep in self.direct_dependencies(&node) {
                if dep == *consumer {
                    return Err(DepError::Cycle(dep));
                }
                if !out.contains(&dep) {
                    stack.push(dep);
                }
            }
        }
        Ok(out.into_iter().collect())
    }

    /// All assets that transitively depend on `producer` (the
    /// invalidation cascade set). Includes everything reachable in the
    /// reverse direction; does not include `producer` itself.
    ///
    /// # Errors
    ///
    /// Returns [`DepError::Cycle`] if the graph has a cycle reachable
    /// from `producer`.
    pub fn invalidation_cascade(&self, producer: &AssetId) -> Result<Vec<AssetId>, DepError> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<AssetId> = self.direct_dependents(producer);
        while let Some(node) = stack.pop() {
            if node == *producer {
                return Err(DepError::Cycle(node));
            }
            if !out.insert(node) {
                continue;
            }
            for dep in self.direct_dependents(&node) {
                if dep == *producer {
                    return Err(DepError::Cycle(dep));
                }
                if !out.contains(&dep) {
                    stack.push(dep);
                }
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Total number of edges. Useful for tests.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Whether the graph has zero edges.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edge_count() == 0
    }

    /// Drop every edge mentioning `id` (either as consumer or
    /// producer). Used when an asset is permanently evicted from the
    /// cache and shouldn't surface in cascade results anymore.
    pub fn forget(&mut self, id: &AssetId) {
        let node_id = node_id_for(*id);
        // `Graph::remove_node` cascades all edges that touch the node and
        // returns `Err(NodeNotFound)` if the node isn't present — both
        // outcomes match the original semantics (no-op when absent).
        let _ = self.inner.remove_node(node_id);
    }
}

// ---------------------------------------------------------------------------
// AssetId ↔ NodeId / EdgeId derivation
// ---------------------------------------------------------------------------

/// Derive a stable [`NodeId`] for an [`AssetId`].
///
/// Uses [`NodeId::from_bytes`] over the raw 32-byte blake3 digest so the
/// mapping is deterministic across processes and platforms. Mirrors the
/// derivation used in `kernel/asset::DependencyGraph`.
fn node_id_for(asset_id: AssetId) -> NodeId {
    NodeId::from_bytes(asset_id.raw())
}

/// Derive a stable [`EdgeId`] for a `(consumer, producer)` `NodeId` pair so
/// re-adding the same edge produces the same id (and thus a `DuplicateEdge`
/// we silently swallow — matching the original idempotence).
fn edge_id_for(src: NodeId, dst: NodeId) -> EdgeId {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(&src.0.to_le_bytes());
    bytes[16..].copy_from_slice(&dst.0.to_le_bytes());
    EdgeId::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &[u8]) -> AssetId {
        AssetId::from_bytes(s)
    }

    #[test]
    fn empty_graph_has_no_edges() {
        let g = DepGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn add_edge_records_in_both_directions() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b).unwrap();

        let deps = g.direct_dependencies(&a);
        assert_eq!(deps, vec![b]);
        let dependents = g.direct_dependents(&b);
        assert_eq!(dependents, vec![a]);
    }

    #[test]
    fn add_edge_is_idempotent() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b).unwrap();
        g.add_edge(a, b).unwrap();
        g.add_edge(a, b).unwrap();
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn add_self_edge_is_rejected() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let err = g.add_edge(a, a).unwrap_err();
        assert_eq!(err, DepError::SelfEdge(a));
    }

    #[test]
    fn transitive_closure_walks_chain() {
        // a → b → c → d.  closure(a) = {b, c, d}.
        let mut graph = DepGraph::new();
        let asset_a = id(b"a");
        let asset_b = id(b"b");
        let asset_c = id(b"c");
        let asset_d = id(b"d");
        graph.add_edge(asset_a, asset_b).unwrap();
        graph.add_edge(asset_b, asset_c).unwrap();
        graph.add_edge(asset_c, asset_d).unwrap();
        let mut got = graph.transitive_closure(&asset_a).unwrap();
        got.sort();
        let mut want = vec![asset_b, asset_c, asset_d];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn invalidation_cascade_walks_reverse() {
        // a depends on b, b depends on c. Changing c invalidates {a, b}.
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b).unwrap();
        g.add_edge(b, c).unwrap();
        let mut got = g.invalidation_cascade(&c).unwrap();
        got.sort();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn invalidation_cascade_handles_diamond() {
        // a depends on b, c.  b and c both depend on d.
        // Changing d invalidates {a, b, c} (a is reached via two paths).
        let mut graph = DepGraph::new();
        let asset_a = id(b"a");
        let asset_b = id(b"b");
        let asset_c = id(b"c");
        let asset_d = id(b"d");
        graph.add_edge(asset_a, asset_b).unwrap();
        graph.add_edge(asset_a, asset_c).unwrap();
        graph.add_edge(asset_b, asset_d).unwrap();
        graph.add_edge(asset_c, asset_d).unwrap();
        let got = graph.invalidation_cascade(&asset_d).unwrap();
        // Order: BTreeSet sorts by AssetId → unspecified relative order
        // for arbitrary content. Just check membership.
        assert_eq!(got.len(), 3);
        assert!(got.contains(&asset_a));
        assert!(got.contains(&asset_b));
        assert!(got.contains(&asset_c));
    }

    #[test]
    fn transitive_closure_detects_cycle() {
        // a → b → a. Cycle.
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b).unwrap();
        g.add_edge(b, a).unwrap();
        let res = g.transitive_closure(&a);
        assert!(matches!(res, Err(DepError::Cycle(_))));
    }

    #[test]
    fn invalidation_cascade_detects_cycle() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b).unwrap();
        g.add_edge(b, a).unwrap();
        let res = g.invalidation_cascade(&a);
        assert!(matches!(res, Err(DepError::Cycle(_))));
    }

    #[test]
    fn remove_edge_undoes_add() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b).unwrap();
        g.remove_edge(a, b);
        assert!(g.is_empty());
        assert!(g.direct_dependencies(&a).is_empty());
        assert!(g.direct_dependents(&b).is_empty());
    }

    #[test]
    fn remove_edge_only_targets_specified_pair() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b).unwrap();
        g.add_edge(a, c).unwrap();
        g.remove_edge(a, b);
        assert_eq!(g.direct_dependencies(&a), vec![c]);
    }

    #[test]
    fn forget_drops_all_edges_touching_asset() {
        let mut g = DepGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b).unwrap();
        g.add_edge(c, b).unwrap();
        g.add_edge(b, a).unwrap();
        // After forgetting b, no edge mentions b.
        g.forget(&b);
        assert!(g.direct_dependencies(&a).is_empty());
        assert!(g.direct_dependencies(&c).is_empty());
        assert!(g.direct_dependents(&a).is_empty());
        assert!(g.direct_dependents(&b).is_empty());
    }

    #[test]
    fn missing_node_queries_return_empty() {
        let g = DepGraph::new();
        let a = id(b"a");
        assert!(g.direct_dependencies(&a).is_empty());
        assert!(g.direct_dependents(&a).is_empty());
        assert!(g.transitive_closure(&a).unwrap().is_empty());
        assert!(g.invalidation_cascade(&a).unwrap().is_empty());
    }

    #[test]
    fn long_chain_does_not_overflow_stack() {
        // Iterative DFS via Vec → no recursion → no stack-overflow risk.
        // Build a 1000-link chain and walk it.
        let mut g = DepGraph::new();
        let nodes: Vec<AssetId> = (0..1000u32)
            .map(|i| AssetId::from_bytes(&i.to_le_bytes()))
            .collect();
        for w in nodes.windows(2) {
            g.add_edge(w[0], w[1]).unwrap();
        }
        let got = g.transitive_closure(&nodes[0]).unwrap();
        assert_eq!(got.len(), 999);
    }
}
