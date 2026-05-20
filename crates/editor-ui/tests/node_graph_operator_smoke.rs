//! ISSUE-62: integration smoke proving `NodeGraphWidget` consumes a real
//! `rge_cad_core::OperatorGraph` through the existing
//! `rge_kernel_graph_foundation::VizAdapter` bridge.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic (knows nothing about
//! materials, animation, CAD, scripts, operators) and must NOT name
//! `rge-cad-core`. Only this integration test target consumes the CAD core
//! crate, via the editor-ui `[dev-dependencies]` edge added for ISSUE-62.

use rge_cad_core::{BooleanOp, CuboidOp, OperatorGraph, OperatorNode};
use rge_editor_ui::widgets::node_graph::NodeGraphWidget;

#[test]
fn node_graph_widget_consumes_real_operator_graph_through_viz_adapter() {
    // Build a real operator graph: two distinct cuboid operators feeding one
    // Boolean union operator on ports 0 and 1. Distinct cuboid payloads
    // ensure content-derived `NodeId`s do not collide.
    let mut graph = OperatorGraph::new();
    let cuboid_left = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid_left");
    let cuboid_right = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid_right");
    let boolean = graph
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("add boolean union");

    let edge_left = graph
        .connect(cuboid_left, boolean, 0)
        .expect("connect cuboid_left -> boolean port 0");
    let edge_right = graph
        .connect(cuboid_right, boolean, 1)
        .expect("connect cuboid_right -> boolean port 1");

    // Ask the editor-ui widget for a model through the existing adapter
    // bridge — no production API was changed for this test. The widget
    // accepts any `&dyn VizAdapter`; passing `&OperatorGraph` here proves the
    // cad-core crate's adapter implementation is exposed verbatim through
    // editor-ui's generic surface.
    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    // Counts flow from the operator graph unchanged.
    assert_eq!(model.node_count(), 3, "three operator nodes are exposed");
    assert_eq!(model.edge_count(), 2, "two operator edges are exposed");

    // Look up node/edge records by their stable ids returned from graph
    // construction. The underlying substrate iterates BTreeMap-sorted by id,
    // not by insertion order, so depend on stable ids rather than positional
    // index.
    let node_by_id = |id| {
        model
            .nodes()
            .iter()
            .copied()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("node {id:?} missing from NodeGraphModel"))
    };
    let edge_by_id = |id| {
        model
            .edges()
            .iter()
            .copied()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("edge {id:?} missing from NodeGraphModel"))
    };

    let cuboid_left_node = node_by_id(cuboid_left);
    let cuboid_right_node = node_by_id(cuboid_right);
    let boolean_node = node_by_id(boolean);

    // The cad-core adapter pins both `display_name` and `kind` to the stable
    // CAD operator kind name — "Cuboid" for `OperatorNode::Cuboid` and
    // "Boolean" for `OperatorNode::Boolean`.
    assert_eq!(cuboid_left_node.display_name, "Cuboid");
    assert_eq!(cuboid_left_node.kind, "Cuboid");
    assert_eq!(cuboid_right_node.display_name, "Cuboid");
    assert_eq!(cuboid_right_node.kind, "Cuboid");
    assert_eq!(boolean_node.display_name, "Boolean");
    assert_eq!(boolean_node.kind, "Boolean");

    // Edge records expose the exact endpoints returned by `connect` and the
    // deterministic input-port labels coined by the operator-graph adapter.
    let edge_left_record = edge_by_id(edge_left);
    assert_eq!(edge_left_record.src, cuboid_left);
    assert_eq!(edge_left_record.dst, boolean);
    assert_eq!(edge_left_record.label, "input[0]");

    let edge_right_record = edge_by_id(edge_right);
    assert_eq!(edge_right_record.src, cuboid_right);
    assert_eq!(edge_right_record.dst, boolean);
    assert_eq!(edge_right_record.label, "input[1]");
}
