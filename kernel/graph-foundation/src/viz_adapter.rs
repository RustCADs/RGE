//! Trait surface for editor graph-viewer widgets.
//!
//! Each domain (material / animation / script / CAD) implements [`VizAdapter`]
//! on its own graph wrapper. The editor's `widgets/node_graph.rs` consumes the
//! trait without coupling to any domain's concrete types.

use crate::id::{EdgeId, NodeId};

// ---------------------------------------------------------------------------
// View types
// ---------------------------------------------------------------------------

/// View into a single node for editor graph viewers.
///
/// Borrowed from the underlying domain graph; zero-copy.
pub struct NodeView<'a> {
    /// Stable id of this node.
    pub id: NodeId,
    /// Human-readable display name (e.g., `"Multiply"`, `"PositionRead"`).
    pub display_name: &'a str,
    /// Domain-specific category string (e.g., `"ColorOp"`, `"ParameterRead"`).
    pub kind: &'a str,
}

/// View into a single edge for editor graph viewers.
///
/// Borrowed from the underlying domain graph; zero-copy.
pub struct EdgeView<'a> {
    /// Stable id of this edge.
    pub id: EdgeId,
    /// Source (origin) node.
    pub src: NodeId,
    /// Destination (target) node.
    pub dst: NodeId,
    /// Human-readable edge label (e.g., port name or relationship type).
    pub label: &'a str,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Domain-specific adapter exposing a graph to the editor's graph-viewer
/// widget without coupling the widget to the domain's concrete types.
///
/// Implement this on your domain's graph wrapper. The editor's
/// `widgets/node_graph.rs` consumes only this trait surface.
///
/// # Example
///
/// ```rust,ignore
/// impl VizAdapter for MyMaterialGraph {
///     fn node_count(&self) -> usize { self.nodes.len() }
///     fn edge_count(&self) -> usize { self.edges.len() }
///     fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_> {
///         Box::new(self.nodes.iter().map(|(id, n)| NodeView {
///             id: *id,
///             display_name: &n.name,
///             kind: n.kind.as_str(),
///         }))
///     }
///     fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
///         Box::new(self.edges.iter().map(|(id, e)| EdgeView {
///             id: *id,
///             src: e.src,
///             dst: e.dst,
///             label: &e.port_name,
///         }))
///     }
/// }
/// ```
pub trait VizAdapter {
    /// Number of nodes in the adapted graph.
    fn node_count(&self) -> usize;
    /// Number of edges in the adapted graph.
    fn edge_count(&self) -> usize;
    /// Iterate over all node views. The iterator is boxed to remain object-safe.
    fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_>;
    /// Iterate over all edge views. The iterator is boxed to remain object-safe.
    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_>;
}
