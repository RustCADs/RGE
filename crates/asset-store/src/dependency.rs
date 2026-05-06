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
//! # Cycles
//!
//! Cycles are a configuration error — an asset can't depend on
//! itself, directly or transitively. [`DepGraph::add_edge`] checks for
//! self-edges; transitive cycles are caught by
//! [`DepGraph::transitive_closure`] returning a [`DepError::Cycle`].
//!
//! # Anti-pattern check
//!
//! This is *not* a build system. The graph here only carries edges;
//! the rebuild logic (which cooker to run, in what order) lives in
//! `crates/build-pipeline` (W-future). Adding cooker plumbing here
//! would re-create the kind of "store paralleling cad-core or ECS"
//! that PLAN §1.4 anti-patterns warn against.

use std::collections::{BTreeMap, BTreeSet};

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
/// Stored as adjacency sets in both directions so both forward
/// (`who do I depend on?`) and reverse (`who depends on me?`) queries
/// are O(degree) rather than O(N).
///
/// `BTreeMap`/`BTreeSet` over the `Hash` variants for stable iteration
/// order — important for tests and for diffing two cached graphs.
#[derive(Debug, Default, Clone)]
pub struct DepGraph {
    /// `consumer → {producers it depends on}`.
    forward: BTreeMap<AssetId, BTreeSet<AssetId>>,
    /// `producer → {consumers that depend on it}`.
    reverse: BTreeMap<AssetId, BTreeSet<AssetId>>,
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
        self.forward.entry(consumer).or_default().insert(producer);
        self.reverse.entry(producer).or_default().insert(consumer);
        Ok(())
    }

    /// Remove a single edge. No-op if the edge isn't present.
    pub fn remove_edge(&mut self, consumer: AssetId, producer: AssetId) {
        if let Some(set) = self.forward.get_mut(&consumer) {
            set.remove(&producer);
            if set.is_empty() {
                self.forward.remove(&consumer);
            }
        }
        if let Some(set) = self.reverse.get_mut(&producer) {
            set.remove(&consumer);
            if set.is_empty() {
                self.reverse.remove(&producer);
            }
        }
    }

    /// All assets that `consumer` directly depends on.
    #[must_use]
    pub fn direct_dependencies(&self, consumer: &AssetId) -> Vec<AssetId> {
        self.forward
            .get(consumer)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    /// All assets that directly depend on `producer`.
    #[must_use]
    pub fn direct_dependents(&self, producer: &AssetId) -> Vec<AssetId> {
        self.reverse
            .get(producer)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
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
        self.forward.values().map(BTreeSet::len).sum()
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
        // Remove forward entries where `id` is the consumer; for each
        // producer touched, also drop the reverse mirror.
        if let Some(producers) = self.forward.remove(id) {
            for p in producers {
                if let Some(set) = self.reverse.get_mut(&p) {
                    set.remove(id);
                    if set.is_empty() {
                        self.reverse.remove(&p);
                    }
                }
            }
        }
        // Then remove reverse entries where `id` is the producer.
        if let Some(consumers) = self.reverse.remove(id) {
            for c in consumers {
                if let Some(set) = self.forward.get_mut(&c) {
                    set.remove(id);
                    if set.is_empty() {
                        self.forward.remove(&c);
                    }
                }
            }
        }
    }
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
