//! Immutable graph snapshot with RON serialization.
//!
//! [`GraphSnapshot`] wraps the node and edge maps in `Arc`s so multiple
//! subscribers can hold the same snapshot without duplicating heap storage.
//! Serialization uses RON so snapshots are human-inspectable.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::{EdgeRecord, Graph};
use crate::id::{EdgeId, NodeId};

// ---------------------------------------------------------------------------
// Serde-serializable snapshot internals (no Arc in the wire format)
// ---------------------------------------------------------------------------

/// Wire-format twin of [`GraphSnapshot`] that uses plain `BTreeMap` (no Arc)
/// so serde's standard derive works without the "rc" feature.
#[derive(Serialize, Deserialize)]
#[serde(rename = "GraphSnapshot")]
struct SnapshotWire<N, E> {
    nodes: BTreeMap<NodeId, N>,
    edges: BTreeMap<EdgeId, EdgeRecord<E>>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by snapshot serialization / deserialization.
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// RON serialization failed.
    #[error("ron serialize: {0}")]
    Serialize(String),
    /// RON deserialization failed.
    #[error("ron deserialize: {0}")]
    Deserialize(String),
}

// ---------------------------------------------------------------------------
// GraphSnapshot
// ---------------------------------------------------------------------------

/// Immutable snapshot of a graph at a point in time.
///
/// Cheap to clone (Arc-wrapped `BTreeMap`s) so multiple subscribers can hold
/// the same snapshot without duplicating heap storage.
///
/// Serialization uses a plain-`BTreeMap` wire format (no Arc in the serialized
/// form) so standard `serde` derives work without the `"rc"` feature.
#[derive(Debug, Clone)]
pub struct GraphSnapshot<N, E> {
    /// Immutable node map, shared via Arc.
    nodes: Arc<BTreeMap<NodeId, N>>,
    /// Immutable edge map, shared via Arc.
    edges: Arc<BTreeMap<EdgeId, EdgeRecord<E>>>,
}

// Manual Serialize: flatten Arc→BTreeMap via the wire struct.
impl<N: Clone + Serialize, E: Clone + Serialize> Serialize for GraphSnapshot<N, E> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = SnapshotWire {
            nodes: (*self.nodes).clone(),
            edges: (*self.edges).clone(),
        };
        wire.serialize(serializer)
    }
}

// Manual Deserialize: deserialize the wire struct then wrap in Arc.
impl<'de, N: Deserialize<'de>, E: Deserialize<'de>> Deserialize<'de> for GraphSnapshot<N, E> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = SnapshotWire::<N, E>::deserialize(deserializer)?;
        Ok(Self {
            nodes: Arc::new(wire.nodes),
            edges: Arc::new(wire.edges),
        })
    }
}

impl<N: Clone, E: Clone> GraphSnapshot<N, E> {
    /// Capture an immutable snapshot from a live graph.
    #[must_use]
    pub fn from_graph(graph: &Graph<N, E>) -> Self {
        let nodes: BTreeMap<NodeId, N> = graph.nodes().map(|(id, n)| (id, n.clone())).collect();
        let edges: BTreeMap<EdgeId, EdgeRecord<E>> =
            graph.edges().map(|(id, e)| (id, e.clone())).collect();
        Self {
            nodes: Arc::new(nodes),
            edges: Arc::new(edges),
        }
    }

    /// Iterate over all (id, node) pairs in deterministic order.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &N)> {
        self.nodes.iter().map(|(&id, n)| (id, n))
    }

    /// Iterate over all (id, edge record) pairs in deterministic order.
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &EdgeRecord<E>)> {
        self.edges.iter().map(|(&id, e)| (id, e))
    }

    /// Number of nodes in this snapshot.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in this snapshot.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Materialize this snapshot back into a mutable [`Graph`].
    ///
    /// Reconstructs adjacency caches from the edge records; the resulting
    /// graph is fully functional for further mutations.
    ///
    /// # Panics
    ///
    /// Never panics in practice: snapshots are always internally consistent
    /// (unique node ids, edges referencing present nodes).
    #[must_use]
    pub fn to_graph(&self) -> Graph<N, E> {
        let mut g = Graph::new();
        for (&id, n) in self.nodes.iter() {
            g.insert_node(id, n.clone())
                .expect("snapshot nodes are unique");
        }
        for (&id, rec) in self.edges.iter() {
            g.insert_edge(id, rec.src, rec.dst, rec.data.clone())
                .expect("snapshot edges reference valid nodes");
        }
        g
    }
}

impl<N: Clone + Serialize + DeserializeOwned, E: Clone + Serialize + DeserializeOwned>
    GraphSnapshot<N, E>
{
    /// Serialize this snapshot to a RON string.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError::Serialize`] if RON serialization fails.
    pub fn to_ron(&self) -> Result<String, SnapshotError> {
        ron::to_string(self).map_err(|e| SnapshotError::Serialize(e.to_string()))
    }

    /// Deserialize a snapshot from a RON string.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError::Deserialize`] if RON deserialization fails.
    pub fn from_ron(s: &str) -> Result<Self, SnapshotError> {
        ron::from_str(s).map_err(|e| SnapshotError::Deserialize(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{EdgeId, NodeId};

    fn build_small_graph() -> Graph<String, u32> {
        let mut g = Graph::new();
        let n1 = NodeId::from_raw(1);
        let n2 = NodeId::from_raw(2);
        let e1 = EdgeId::from_raw(10);
        g.insert_node(n1, "a".to_string()).unwrap();
        g.insert_node(n2, "b".to_string()).unwrap();
        g.insert_edge(e1, n1, n2, 42).unwrap();
        g
    }

    #[test]
    fn snapshot_from_graph_counts() {
        let g = build_small_graph();
        let snap = GraphSnapshot::from_graph(&g);
        assert_eq!(snap.node_count(), 2);
        assert_eq!(snap.edge_count(), 1);
    }

    #[test]
    fn snapshot_ron_round_trip() {
        let g = build_small_graph();
        let snap1 = GraphSnapshot::from_graph(&g);
        let ron_str = snap1.to_ron().unwrap();
        let snap2: GraphSnapshot<String, u32> = GraphSnapshot::from_ron(&ron_str).unwrap();
        let ron_str2 = snap2.to_ron().unwrap();
        assert_eq!(ron_str, ron_str2, "RON round-trip must be byte-identical");
    }

    #[test]
    fn snapshot_to_graph_restores_structure() {
        let g = build_small_graph();
        let snap = GraphSnapshot::from_graph(&g);
        let g2 = snap.to_graph();
        assert_eq!(g2.node_count(), g.node_count());
        assert_eq!(g2.edge_count(), g.edge_count());
    }
}
