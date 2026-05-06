//! Directed asset dependency graph.
//!
//! An edge `A → B` means "A depends on B".  When B changes, walk
//! [`DependencyGraph::dependents`] (reverse edges) to find A and mark A stale.
//!
//! Both forward (`deps`) and reverse (`reverse`) adjacency maps are maintained
//! in sync at all times so [`dependents`](DependencyGraph::dependents) is O(1)
//! without requiring a scan.
//!
//! The graph is serialisable via serde (RON) so the [`Registry`] can persist
//! and restore it across sessions without re-cooking assets.
//!
//! [`Registry`]: crate::registry::Registry

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::id::AssetId;

/// Directed dependency graph over [`AssetId`]s.
///
/// Edge `A → B` means "A depends on B".  Useful for propagating invalidation:
/// when B changes, `transitive_dependents(B)` returns every asset that
/// (transitively) depends on it.
///
/// The graph is serde-stable — serialize to RON via
/// [`Registry::serialize_deps`](crate::registry::Registry::serialize_deps) and
/// restore with
/// [`Registry::restore_deps`](crate::registry::Registry::restore_deps).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// For each asset, the set of assets it depends on (outgoing edges).
    deps: BTreeMap<AssetId, BTreeSet<AssetId>>,
    /// For each asset, the set of assets that depend on it (incoming edges).
    ///
    /// Maintained alongside `deps` for O(1) `dependents` lookup.
    reverse: BTreeMap<AssetId, BTreeSet<AssetId>>,
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
        self.deps.entry(dependent).or_default().insert(dep);
        self.reverse.entry(dep).or_default().insert(dependent);
        // Ensure both nodes appear in both maps even if they have no edges of
        // the other direction, so `node_count` stays consistent.
        self.deps.entry(dep).or_default();
        self.reverse.entry(dependent).or_default();
    }

    /// Remove an edge.
    ///
    /// Returns `true` if the edge existed, `false` otherwise.
    pub fn remove_edge(&mut self, dependent: AssetId, dep: AssetId) -> bool {
        let removed = self
            .deps
            .get_mut(&dependent)
            .is_some_and(|set| set.remove(&dep));
        if removed {
            if let Some(set) = self.reverse.get_mut(&dep) {
                set.remove(&dependent);
            }
        }
        removed
    }

    /// All direct dependencies of `id` (outgoing edges: what `id` depends on).
    pub fn dependencies(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_ {
        self.deps
            .get(&id)
            .into_iter()
            .flat_map(|s| s.iter().copied())
    }

    /// All direct dependents of `id` (incoming edges: what depends on `id`).
    pub fn dependents(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_ {
        self.reverse
            .get(&id)
            .into_iter()
            .flat_map(|s| s.iter().copied())
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

        // Seed from direct dependents (sorted, deterministic).
        if let Some(direct) = self.reverse.get(&id) {
            for &dep in direct {
                if visited.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }

        while let Some(node) = queue.pop_front() {
            if let Some(parents) = self.reverse.get(&node) {
                // Iterate in sorted order for determinism.
                for &parent in parents {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }
            }
        }

        // Return in BTreeSet order (deterministic).
        visited.into_iter().collect()
    }

    /// Drop all references to `id` from the graph (both forward and reverse
    /// edges).
    pub fn remove_node(&mut self, id: AssetId) {
        // Remove outgoing edges: for each dep that `id` depends on, remove id
        // from the dep's reverse set.
        if let Some(deps_of_id) = self.deps.remove(&id) {
            for dep in deps_of_id {
                if let Some(rev) = self.reverse.get_mut(&dep) {
                    rev.remove(&id);
                }
            }
        }
        // Remove incoming edges: for each node that depended on `id`, remove
        // `id` from that node's forward dep set.
        if let Some(dependents_of_id) = self.reverse.remove(&id) {
            for dependent in dependents_of_id {
                if let Some(fwd) = self.deps.get_mut(&dependent) {
                    fwd.remove(&id);
                }
            }
        }
    }

    /// Number of directed edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.deps.values().map(BTreeSet::len).sum()
    }

    /// Number of nodes (any node with at least one edge in or out).
    ///
    /// A node with no edges at all does not appear in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        // Both maps are kept in sync, so either gives the same count.
        self.deps.len()
    }

    /// Detect a cycle using DFS.
    ///
    /// Returns `Some(cycle)` containing a sequence of [`AssetId`]s that form a
    /// cycle if one exists, or `None` for a DAG.
    ///
    /// The returned cycle is not necessarily minimal — it is the first cycle
    /// found in DFS traversal order.
    #[must_use]
    pub fn detect_cycle(&self) -> Option<Vec<AssetId>> {
        // Standard DFS cycle detection with a "gray" (in-stack) set.
        let mut visited: BTreeSet<AssetId> = BTreeSet::new();
        let mut stack: Vec<AssetId> = Vec::new();
        let mut in_stack: BTreeSet<AssetId> = BTreeSet::new();

        for &start in self.deps.keys() {
            if visited.contains(&start) {
                continue;
            }
            if let Some(cycle) =
                dfs_cycle(&self.deps, start, &mut visited, &mut stack, &mut in_stack)
            {
                return Some(cycle);
            }
        }
        None
    }
}

/// Recursive DFS helper for cycle detection.
fn dfs_cycle(
    deps: &BTreeMap<AssetId, BTreeSet<AssetId>>,
    node: AssetId,
    visited: &mut BTreeSet<AssetId>,
    stack: &mut Vec<AssetId>,
    in_stack: &mut BTreeSet<AssetId>,
) -> Option<Vec<AssetId>> {
    visited.insert(node);
    stack.push(node);
    in_stack.insert(node);

    if let Some(neighbors) = deps.get(&node) {
        for &neighbor in neighbors {
            if !visited.contains(&neighbor) {
                if let Some(cycle) = dfs_cycle(deps, neighbor, visited, stack, in_stack) {
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
    }

    stack.pop();
    in_stack.remove(&node);
    None
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
