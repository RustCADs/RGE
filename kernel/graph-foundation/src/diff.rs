//! Structural diff between two graph snapshots.
//!
//! [`GraphDiff`] records which nodes and edges were added, removed, or had
//! their payloads changed when transitioning from one snapshot to another.
//! Domain-specific semantics (e.g., topological changes) are out of scope.

use std::collections::BTreeMap;

use crate::graph::EdgeRecord;
use crate::id::{EdgeId, NodeId};
use crate::snapshot::GraphSnapshot;

/// Structural diff between two graph snapshots: nodes/edges added, removed,
/// or changed.
///
/// Compute with [`GraphDiff::between`]. The old → new convention matches
/// standard VCS terminology: `added_nodes` are in `new` but not in `old`.
#[derive(Debug, Clone)]
pub struct GraphDiff<N, E> {
    /// Nodes present in `new` but absent in `old`.
    pub added_nodes: BTreeMap<NodeId, N>,
    /// Nodes present in `old` but absent in `new`.
    pub removed_nodes: BTreeMap<NodeId, N>,
    /// Nodes present in both snapshots whose payload changed; value is `(old, new)`.
    pub changed_nodes: BTreeMap<NodeId, (N, N)>,
    /// Edges present in `new` but absent in `old`.
    pub added_edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    /// Edges present in `old` but absent in `new`.
    pub removed_edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    /// Edges present in both snapshots whose record changed; value is `(old, new)`.
    pub changed_edges: BTreeMap<EdgeId, (EdgeRecord<E>, EdgeRecord<E>)>,
}

impl<N, E> Default for GraphDiff<N, E> {
    fn default() -> Self {
        Self {
            added_nodes: BTreeMap::new(),
            removed_nodes: BTreeMap::new(),
            changed_nodes: BTreeMap::new(),
            added_edges: BTreeMap::new(),
            removed_edges: BTreeMap::new(),
            changed_edges: BTreeMap::new(),
        }
    }
}

impl<N: Clone + PartialEq, E: Clone + PartialEq> GraphDiff<N, E>
where
    EdgeRecord<E>: PartialEq,
{
    /// Compute `diff(old, new)`: what changed when transitioning `old → new`.
    #[must_use]
    pub fn between(old: &GraphSnapshot<N, E>, new: &GraphSnapshot<N, E>) -> Self {
        let mut diff = Self::default();

        // --- Nodes ---
        // Collect old nodes into a map for O(n) comparison.
        let old_nodes: BTreeMap<NodeId, &N> = old.nodes().collect();
        let new_nodes: BTreeMap<NodeId, &N> = new.nodes().collect();

        for (&id, new_val) in &new_nodes {
            match old_nodes.get(&id) {
                None => {
                    diff.added_nodes.insert(id, (*new_val).clone());
                }
                Some(old_val) => {
                    if *old_val != *new_val {
                        diff.changed_nodes
                            .insert(id, ((*old_val).clone(), (*new_val).clone()));
                    }
                }
            }
        }
        for (&id, old_val) in &old_nodes {
            if !new_nodes.contains_key(&id) {
                diff.removed_nodes.insert(id, (*old_val).clone());
            }
        }

        // --- Edges ---
        let old_edges: BTreeMap<EdgeId, &EdgeRecord<E>> = old.edges().collect();
        let new_edges: BTreeMap<EdgeId, &EdgeRecord<E>> = new.edges().collect();

        for (&id, new_rec) in &new_edges {
            match old_edges.get(&id) {
                None => {
                    diff.added_edges.insert(id, (*new_rec).clone());
                }
                Some(old_rec) => {
                    if *old_rec != *new_rec {
                        diff.changed_edges
                            .insert(id, ((*old_rec).clone(), (*new_rec).clone()));
                    }
                }
            }
        }
        for (&id, old_rec) in &old_edges {
            if !new_edges.contains_key(&id) {
                diff.removed_edges.insert(id, (*old_rec).clone());
            }
        }

        diff
    }

    /// True when no changes exist (empty diff).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.changed_nodes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.changed_edges.is_empty()
    }

    /// Total number of node-level changes (add + remove + change).
    #[must_use]
    pub fn node_change_count(&self) -> usize {
        self.added_nodes.len() + self.removed_nodes.len() + self.changed_nodes.len()
    }

    /// Total number of edge-level changes (add + remove + change).
    #[must_use]
    pub fn edge_change_count(&self) -> usize {
        self.added_edges.len() + self.removed_edges.len() + self.changed_edges.len()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::id::{EdgeId, NodeId};

    fn n(v: u128) -> NodeId {
        NodeId::from_raw(v)
    }
    fn e(v: u128) -> EdgeId {
        EdgeId::from_raw(v)
    }

    #[test]
    fn diff_empty_snapshots() {
        let g: Graph<i32, i32> = Graph::new();
        let s1 = GraphSnapshot::from_graph(&g);
        let s2 = GraphSnapshot::from_graph(&g);
        let diff = GraphDiff::between(&s1, &s2);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_detects_added_node() {
        let g1: Graph<i32, i32> = Graph::new();
        let mut g2: Graph<i32, i32> = Graph::new();
        g2.insert_node(n(1), 10).unwrap();
        let diff = GraphDiff::between(
            &GraphSnapshot::from_graph(&g1),
            &GraphSnapshot::from_graph(&g2),
        );
        assert_eq!(diff.added_nodes.len(), 1);
        assert_eq!(diff.added_nodes[&n(1)], 10);
    }

    #[test]
    fn diff_detects_removed_node() {
        let mut g1: Graph<i32, i32> = Graph::new();
        g1.insert_node(n(1), 10).unwrap();
        let g2: Graph<i32, i32> = Graph::new();
        let diff = GraphDiff::between(
            &GraphSnapshot::from_graph(&g1),
            &GraphSnapshot::from_graph(&g2),
        );
        assert_eq!(diff.removed_nodes.len(), 1);
    }

    #[test]
    fn diff_detects_changed_node() {
        let mut g1: Graph<i32, i32> = Graph::new();
        g1.insert_node(n(1), 10).unwrap();
        let mut g2: Graph<i32, i32> = Graph::new();
        g2.insert_node(n(1), 20).unwrap();
        let diff = GraphDiff::between(
            &GraphSnapshot::from_graph(&g1),
            &GraphSnapshot::from_graph(&g2),
        );
        assert_eq!(diff.changed_nodes.len(), 1);
        let (old, new) = &diff.changed_nodes[&n(1)];
        assert_eq!(*old, 10);
        assert_eq!(*new, 20);
    }

    #[test]
    fn diff_detects_added_edge() {
        let mut g1: Graph<i32, i32> = Graph::new();
        g1.insert_node(n(1), 1).unwrap();
        g1.insert_node(n(2), 2).unwrap();
        let mut g2 = g1.clone();
        g2.insert_edge(e(10), n(1), n(2), 5).unwrap();
        let diff = GraphDiff::between(
            &GraphSnapshot::from_graph(&g1),
            &GraphSnapshot::from_graph(&g2),
        );
        assert_eq!(diff.added_edges.len(), 1);
    }
}
