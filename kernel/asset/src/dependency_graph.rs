//! Directed asset dependency graph.
//!
//! An edge `A → B` means "A depends on B".  When B changes, walk
//! [`DependencyGraph::dependents`] (reverse edges) to find A and mark A stale.
//!
//! # Substrate
//!
//! Backed by [`rge_kernel_graph_foundation::Graph<AssetId, ()>`] per PLAN §1.14
//! (graph-foundation substrate doctrine — kernel/asset is a Tier-1 graph
//! consumer that must build on the substrate rather than reinvent
//! `BTreeMap<K, BTreeSet<K>>` adjacency).
//!
//! Each [`AssetId`] used as an endpoint of [`add_edge`](DependencyGraph::add_edge)
//! is auto-promoted to a graph node, with its [`NodeId`] derived via
//! [`NodeId::from_bytes`] over the `AssetId`'s raw 32-byte blake3 digest. The
//! mapping is therefore deterministic and reversible: we recover the `AssetId`
//! from a `NodeId` by looking up the node payload in the underlying `Graph`.
//!
//! # Idempotence vs. graph-foundation's stricter contract
//!
//! `Graph::insert_node` errors on duplicate ids and `Graph::insert_edge` errors
//! on duplicate edge ids. The original `DependencyGraph::add_edge` semantics
//! are idempotent — calling twice with the same pair has no extra effect — so
//! the wrappers below check for presence before inserting and silently skip
//! duplicates rather than propagating `GraphError::DuplicateNode` /
//! `GraphError::DuplicateEdge`.
//!
//! # Cycle detection
//!
//! graph-foundation's `Graph<N, E>` does NOT itself detect cycles (consistent
//! with `cad-core::OperatorGraph`, which also implements its own cycle
//! detection on top of the substrate). We preserve the existing DFS-based
//! [`detect_cycle`](DependencyGraph::detect_cycle) algorithm, operating over
//! `NodeId`s internally and resolving back to `AssetId`s for the returned
//! cycle path.
//!
//! The graph is serialisable via serde (RON) so the [`Registry`] can persist
//! and restore it across sessions without re-cooking assets.
//!
//! [`Registry`]: crate::registry::Registry

use std::collections::{BTreeSet, VecDeque};

use rge_kernel_graph_foundation::{EdgeId, Graph, NodeId};
use serde::{Deserialize, Serialize};

use crate::id::AssetId;

/// Directed dependency graph over [`AssetId`]s.
///
/// Edge `A → B` means "A depends on B".  Useful for propagating invalidation:
/// when B changes, `transitive_dependents(B)` returns every asset that
/// (transitively) depends on it.
///
/// Backed by [`rge_kernel_graph_foundation::Graph<AssetId, ()>`]; iteration
/// order is deterministic (BTreeMap-backed in the substrate).
///
/// The graph is serde-stable — serialize to RON via
/// [`Registry::serialize_deps`](crate::registry::Registry::serialize_deps) and
/// restore with
/// [`Registry::restore_deps`](crate::registry::Registry::restore_deps).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Substrate-backed directed graph: nodes carry the original [`AssetId`]
    /// payload, edges are unit (no per-edge metadata).
    inner: Graph<AssetId, ()>,
}

// Manual `Default` impl: graph-foundation's `Graph<N, E>` derives `Default`
// which the macro expands into a `where N: Default` bound. `AssetId` has no
// `Default` (a default 32-byte hash would be meaningless), so we construct
// the empty inner graph directly via `Graph::new` instead.
impl Default for DependencyGraph {
    fn default() -> Self {
        Self {
            inner: Graph::new(),
        }
    }
}

impl DependencyGraph {
    /// Create an empty dependency graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `dependent` depends on `dep`.
    ///
    /// Idempotent — calling with the same pair twice has no extra effect.
    pub fn add_edge(&mut self, dependent: AssetId, dep: AssetId) {
        let dependent_id = node_id_for(dependent);
        let dep_id = node_id_for(dep);
        // Auto-promote both endpoints to nodes if not yet present. Both
        // `insert_node` calls swallow `DuplicateNode` because re-adding is a
        // no-op in the original API.
        let _ = self.inner.insert_node(dependent_id, dependent);
        let _ = self.inner.insert_node(dep_id, dep);
        // Edge id derived from endpoints so the same pair always produces the
        // same EdgeId; `insert_edge` swallows `DuplicateEdge` so a second
        // `add_edge(parent, child)` is a no-op.
        let edge_id = edge_id_for(dependent_id, dep_id);
        let _ = self.inner.insert_edge(edge_id, dependent_id, dep_id, ());
    }

    /// Remove an edge.
    ///
    /// Returns `true` if the edge existed, `false` otherwise.
    pub fn remove_edge(&mut self, dependent: AssetId, dep: AssetId) -> bool {
        let dependent_id = node_id_for(dependent);
        let dep_id = node_id_for(dep);
        let edge_id = edge_id_for(dependent_id, dep_id);
        self.inner.remove_edge(edge_id).is_ok()
    }

    /// All direct dependencies of `id` (outgoing edges: what `id` depends on).
    pub fn dependencies(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_ {
        let node_id = node_id_for(id);
        self.inner.outgoing(node_id).filter_map(move |eid| {
            self.inner
                .edge(eid)
                .and_then(|rec| self.inner.node(rec.dst).copied())
        })
    }

    /// All direct dependents of `id` (incoming edges: what depends on `id`).
    pub fn dependents(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_ {
        let node_id = node_id_for(id);
        self.inner.incoming(node_id).filter_map(move |eid| {
            self.inner
                .edge(eid)
                .and_then(|rec| self.inner.node(rec.src).copied())
        })
    }

    /// Transitive closure of dependents — every asset reachable from `id` via
    /// reverse edges (BFS, deterministic order via `BTreeSet` sorted queues).
    ///
    /// Used to compute "what gets invalidated if `id` changes".  The result
    /// does not include `id` itself.
    #[must_use]
    pub fn transitive_dependents(&self, id: AssetId) -> Vec<AssetId> {
        let mut visited: BTreeSet<AssetId> = BTreeSet::new();
        let mut queue: VecDeque<AssetId> = VecDeque::new();

        // Seed from direct dependents (sorted, deterministic via BTreeSet).
        let mut direct: BTreeSet<AssetId> = BTreeSet::new();
        for dep in self.dependents(id) {
            direct.insert(dep);
        }
        for dep in direct {
            if visited.insert(dep) {
                queue.push_back(dep);
            }
        }

        while let Some(node) = queue.pop_front() {
            // Iterate parents in sorted (BTreeSet) order for determinism.
            let mut parents: BTreeSet<AssetId> = BTreeSet::new();
            for parent in self.dependents(node) {
                parents.insert(parent);
            }
            for parent in parents {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }

        // Return in BTreeSet order (deterministic).
        visited.into_iter().collect()
    }

    /// Drop all references to `id` from the graph (both forward and reverse
    /// edges).
    pub fn remove_node(&mut self, id: AssetId) {
        let node_id = node_id_for(id);
        // `Graph::remove_node` cascades all edges that touch the node and
        // returns `Err(NodeNotFound)` if the node isn't present — both
        // outcomes match the original semantics (no-op when absent).
        let _ = self.inner.remove_node(node_id);
    }

    /// Number of directed edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Number of nodes currently held in the underlying graph.
    ///
    /// Post-2026-05-09 migration to `kernel/graph-foundation::Graph<AssetId, ()>`
    /// the count reflects substrate-level node residency: nodes are added on
    /// `add_edge` (both endpoints) and removed only by explicit `remove_node`.
    /// `remove_edge` does NOT prune zero-degree endpoint nodes (the substrate's
    /// invariant is that node identity is content-derived from `AssetId` so a
    /// node remains discoverable as long as the AssetId has been mentioned).
    /// Mirrors the asset-store::DepGraph behavior; observable only via this
    /// internal accessor (no public API depends on auto-prune).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Detect a cycle using DFS.
    ///
    /// Returns `Some(cycle)` containing a sequence of [`AssetId`]s that form a
    /// cycle if one exists, or `None` for a DAG.
    ///
    /// The returned cycle is not necessarily minimal — it is the first cycle
    /// found in DFS traversal order.
    ///
    /// graph-foundation's substrate `Graph<N, E>` does NOT itself detect
    /// cycles (consistent with `cad-core::OperatorGraph`), so this method
    /// implements the DFS locally on top of the substrate's iteration API.
    #[must_use]
    pub fn detect_cycle(&self) -> Option<Vec<AssetId>> {
        // Standard DFS cycle detection with a "gray" (in-stack) set.
        // Operate over AssetId directly so the returned cycle path is in the
        // domain type without an extra NodeId → AssetId resolution step.
        let mut visited: BTreeSet<AssetId> = BTreeSet::new();
        let mut stack: Vec<AssetId> = Vec::new();
        let mut in_stack: BTreeSet<AssetId> = BTreeSet::new();

        // Iterate starts in deterministic AssetId order. Collect into a
        // BTreeSet first so the traversal is reproducible regardless of the
        // substrate's NodeId-keyed iteration order (NodeIds are u128-keyed,
        // not directly AssetId-ordered).
        let starts: BTreeSet<AssetId> = self.inner.nodes().map(|(_, asset_id)| *asset_id).collect();

        for start in starts {
            if visited.contains(&start) {
                continue;
            }
            if let Some(cycle) = self.dfs_cycle(start, &mut visited, &mut stack, &mut in_stack) {
                return Some(cycle);
            }
        }
        None
    }

    /// Recursive DFS helper for cycle detection.
    fn dfs_cycle(
        &self,
        node: AssetId,
        visited: &mut BTreeSet<AssetId>,
        stack: &mut Vec<AssetId>,
        in_stack: &mut BTreeSet<AssetId>,
    ) -> Option<Vec<AssetId>> {
        visited.insert(node);
        stack.push(node);
        in_stack.insert(node);

        // Sorted (BTreeSet) view of outgoing neighbours for deterministic
        // traversal, mirroring the original BTreeSet-backed adjacency.
        let neighbors: BTreeSet<AssetId> = self.dependencies(node).collect();
        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                if let Some(cycle) = self.dfs_cycle(neighbor, visited, stack, in_stack) {
                    return Some(cycle);
                }
            } else if in_stack.contains(&neighbor) {
                // Found cycle — extract it from the stack.
                let cycle_start = stack.iter().position(|&n| n == neighbor).unwrap_or(0);
                let mut cycle: Vec<AssetId> = stack[cycle_start..].to_vec();
                cycle.push(neighbor); // close the loop
                return Some(cycle);
            }
        }

        stack.pop();
        in_stack.remove(&node);
        None
    }
}

// ---------------------------------------------------------------------------
// AssetId ↔ NodeId / EdgeId derivation
// ---------------------------------------------------------------------------

/// Derive a stable [`NodeId`] for an [`AssetId`].
///
/// Uses [`NodeId::from_bytes`] over the raw 32-byte blake3 digest so the
/// mapping is deterministic across processes and platforms.
fn node_id_for(asset_id: AssetId) -> NodeId {
    NodeId::from_bytes(asset_id.raw())
}

/// Derive a stable [`EdgeId`] for an `(src, dst)` `NodeId` pair so re-adding
/// the same edge produces the same id (and thus a `DuplicateEdge` we silently
/// swallow — matching the original idempotence).
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
    fn add_edge_populates_both_forward_and_reverse() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b);

        assert!(g.dependencies(a).any(|d| d == b));
        assert!(g.dependents(b).any(|d| d == a));
        // Converse should be empty.
        assert_eq!(g.dependencies(b).count(), 0);
        assert_eq!(g.dependents(a).count(), 0);
    }

    #[test]
    fn add_edge_is_idempotent() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b);
        g.add_edge(a, b);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn remove_edge_returns_true_when_existed() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b);
        assert!(g.remove_edge(a, b));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn remove_edge_returns_false_when_not_present() {
        let mut g = DependencyGraph::new();
        assert!(!g.remove_edge(id(b"x"), id(b"y")));
    }

    #[test]
    fn transitive_dependents_returns_deterministic_order() {
        // scene → mesh → material → texture
        let mut g = DependencyGraph::new();
        let scene = id(b"scene");
        let mesh = id(b"mesh");
        let material = id(b"material");
        let texture = id(b"texture");
        g.add_edge(scene, mesh);
        g.add_edge(mesh, material);
        g.add_edge(material, texture);

        let result = g.transitive_dependents(texture);
        // All three dependents must appear.
        assert!(result.contains(&material));
        assert!(result.contains(&mesh));
        assert!(result.contains(&scene));
        // Deterministic: same call twice returns the same slice.
        assert_eq!(result, g.transitive_dependents(texture));
    }

    #[test]
    fn detect_cycle_finds_two_cycle() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        g.add_edge(a, b);
        g.add_edge(b, a);
        assert!(g.detect_cycle().is_some());
    }

    #[test]
    fn detect_cycle_finds_three_cycle() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b);
        g.add_edge(b, c);
        g.add_edge(c, a);
        assert!(g.detect_cycle().is_some());
    }

    #[test]
    fn detect_cycle_returns_none_for_dag() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b);
        g.add_edge(a, c);
        g.add_edge(c, b);
        assert!(g.detect_cycle().is_none());
    }

    #[test]
    fn remove_node_cleans_both_directions() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        // a depends on b, b depends on c.
        g.add_edge(a, b);
        g.add_edge(b, c);
        g.remove_node(b);
        // a should no longer list b as a dependency.
        assert_eq!(g.dependencies(a).count(), 0);
        // c should no longer list b as a dependent.
        assert_eq!(g.dependents(c).count(), 0);
    }

    #[test]
    fn node_count_and_edge_count() {
        let mut g = DependencyGraph::new();
        let a = id(b"a");
        let b = id(b"b");
        let c = id(b"c");
        g.add_edge(a, b);
        g.add_edge(a, c);
        assert_eq!(g.edge_count(), 2);
        // All three nodes present.
        assert_eq!(g.node_count(), 3);
    }
}
