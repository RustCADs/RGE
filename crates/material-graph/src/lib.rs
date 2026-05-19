//! `rge-material-graph` — material graph foundation wrapper.
//!
//! Failure class: snapshot-recoverable
//!
//! Phase 8 foundation slice. [`MaterialGraph`] is a thin wrapper over
//! `rge_kernel_graph_foundation::Graph` that stores opaque material nodes
//! keyed by content-derived [`NodeId`] and connects them with typed-port
//! [`MaterialEdge`] payloads. Like `cad-core`'s operator-graph wrapper, the
//! material graph is rebuildable structural state that participates in
//! snapshot/restore — a rejected mutation is recovered by restoring the last
//! good snapshot rather than terminating the session.
//!
//! This crate is the foundation layer only: it carries no WGSL generation,
//! runtime evaluation, editor behavior, traversal, cycle detection, or gfx
//! integration. The [`PortType`] surface is a data-only tag with no shader,
//! evaluator, or renderer semantics.

use rge_kernel_graph_foundation::{EdgeId, Graph, GraphError, NodeId};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error returned by mutating operations on a [`MaterialGraph`].
///
/// A thin newtype over the substrate [`GraphError`]; the wrapped value
/// preserves the exact graph-foundation failure (duplicate node, duplicate
/// edge, or dangling endpoint) for callers that need to inspect it.
#[derive(Debug, PartialEq, Eq)]
pub struct MaterialGraphError(pub GraphError);

impl std::fmt::Display for MaterialGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "material graph error: {}", self.0)
    }
}

impl std::error::Error for MaterialGraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<GraphError> for MaterialGraphError {
    fn from(err: GraphError) -> Self {
        Self(err)
    }
}

// ---------------------------------------------------------------------------
// Typed ports
// ---------------------------------------------------------------------------

/// Data-only tag identifying the type carried by a material connection port.
///
/// This is a minimal classification used only to record what a connection
/// transports; it carries no shader, evaluator, editor, or renderer
/// semantics, and the wrapper performs no type-compatibility validation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortType {
    /// A single scalar channel.
    Scalar = 0,
    /// A multi-component vector channel.
    Vector = 1,
    /// A color channel.
    Color = 2,
    /// A texture-sample channel.
    Texture = 3,
}

/// Payload stored on a material connection: the typed source and destination
/// ports it joins.
///
/// Data-only. The pair `(src_port, dst_port)` participates in the connection's
/// content-derived [`EdgeId`], so two connections between the same nodes that
/// use different port types are distinct edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialEdge {
    /// Typed port on the source node that the connection leaves from.
    pub src_port: PortType,
    /// Typed port on the destination node that the connection arrives at.
    pub dst_port: PortType,
}

// ---------------------------------------------------------------------------
// Node payload
// ---------------------------------------------------------------------------

/// Opaque material node payload.
///
/// The wrapper treats the node `key` as an uninterpreted string; the substrate
/// [`NodeId`] is derived deterministically from its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MaterialNode {
    key: String,
}

// ---------------------------------------------------------------------------
// MaterialGraph
// ---------------------------------------------------------------------------

/// Minimal material graph: opaque nodes connected by typed-port edges,
/// backed by `rge_kernel_graph_foundation::Graph`.
#[derive(Debug, Clone)]
pub struct MaterialGraph {
    graph: Graph<MaterialNode, MaterialEdge>,
}

impl MaterialGraph {
    /// Construct an empty material graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
        }
    }

    /// Add an opaque material node identified by `key`.
    ///
    /// The returned [`NodeId`] is derived deterministically from the key, so
    /// the same key yields the same id in any [`MaterialGraph`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`MaterialGraphError`] wrapping [`GraphError::DuplicateNode`]
    /// when a node with the same key (hence the same [`NodeId`]) is already
    /// present in this graph.
    pub fn add_node(&mut self, key: &str) -> Result<NodeId, MaterialGraphError> {
        let id = NodeId::from_bytes(key.as_bytes());
        self.graph.insert_node(
            id,
            MaterialNode {
                key: key.to_owned(),
            },
        )?;
        Ok(id)
    }

    /// Connect two existing nodes with the typed-port payload `edge`.
    ///
    /// The returned [`EdgeId`] is derived deterministically from the endpoint
    /// ids together with both port types.
    ///
    /// # Errors
    ///
    /// Returns [`MaterialGraphError`] wrapping:
    /// - [`GraphError::DuplicateEdge`] when an identical connection (same
    ///   endpoints and same port types) already exists; or
    /// - [`GraphError::DanglingEndpoint`] when `src` or `dst` is not currently
    ///   a node in this graph.
    pub fn connect(
        &mut self,
        src: NodeId,
        dst: NodeId,
        edge: MaterialEdge,
    ) -> Result<EdgeId, MaterialGraphError> {
        let id = material_edge_id(src, dst, edge);
        self.graph.insert_edge(id, src, dst, edge)?;
        Ok(id)
    }

    /// Returns the number of nodes currently in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of edges currently in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

impl Default for MaterialGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the content-stable [`EdgeId`] for a connection from its endpoints
/// and typed ports, so identical connections collide (duplicate detection)
/// while connections that differ only in port type stay distinct.
fn material_edge_id(src: NodeId, dst: NodeId, edge: MaterialEdge) -> EdgeId {
    let mut bytes = [0u8; 34];
    bytes[..16].copy_from_slice(&src.0.to_le_bytes());
    bytes[16..32].copy_from_slice(&dst.0.to_le_bytes());
    bytes[32] = edge.src_port as u8;
    bytes[33] = edge.dst_port as u8;
    EdgeId::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::GraphError;

    use super::*;

    fn edge(src_port: PortType, dst_port: PortType) -> MaterialEdge {
        MaterialEdge { src_port, dst_port }
    }

    #[test]
    fn node_ids_are_stable_across_graphs() {
        let mut a = MaterialGraph::new();
        let mut b = MaterialGraph::new();
        let id_a = a.add_node("albedo").unwrap();
        let id_b = b.add_node("albedo").unwrap();
        assert_eq!(
            id_a, id_b,
            "the same key in two fresh graphs must yield the same NodeId"
        );
    }

    #[test]
    fn distinct_keys_get_distinct_ids() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        assert_ne!(a, b, "distinct keys must yield distinct NodeIds");
    }

    #[test]
    fn connect_succeeds_and_updates_counts() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 0);

        g.connect(a, b, edge(PortType::Color, PortType::Color))
            .unwrap();

        assert_eq!(
            g.edge_count(),
            1,
            "a successful connect increments edge count"
        );
        assert_eq!(g.node_count(), 2, "connect must preserve node count");
    }

    #[test]
    fn duplicate_node_is_rejected() {
        let mut g = MaterialGraph::new();
        g.add_node("a").unwrap();
        let err = g
            .add_node("a")
            .expect_err("re-adding the same node key must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateNode(_)),
            "expected DuplicateNode, got {err:?}"
        );
    }

    #[test]
    fn duplicate_edge_is_rejected() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        let e = edge(PortType::Scalar, PortType::Scalar);
        g.connect(a, b, e).unwrap();

        let err = g
            .connect(a, b, e)
            .expect_err("re-adding an identical connection must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateEdge(_)),
            "expected DuplicateEdge, got {err:?}"
        );
        assert_eq!(g.edge_count(), 1, "rejected connect must not add an edge");
    }

    #[test]
    fn differing_ports_are_not_duplicates() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        g.connect(a, b, edge(PortType::Scalar, PortType::Scalar))
            .unwrap();
        g.connect(a, b, edge(PortType::Color, PortType::Texture))
            .unwrap();
        assert_eq!(
            g.edge_count(),
            2,
            "same endpoints with different port types are distinct edges"
        );
    }

    #[test]
    fn dangling_endpoint_is_rejected() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let ghost = NodeId::from_bytes(b"never-added");

        let err = g
            .connect(a, ghost, edge(PortType::Color, PortType::Color))
            .expect_err("connecting to an absent node must fail");
        assert!(
            matches!(err.0, GraphError::DanglingEndpoint { .. }),
            "expected DanglingEndpoint, got {err:?}"
        );
        assert_eq!(g.edge_count(), 0, "rejected connect must not add an edge");
    }

    #[test]
    fn empty_graph_has_zero_counts() {
        let g = MaterialGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }
}
