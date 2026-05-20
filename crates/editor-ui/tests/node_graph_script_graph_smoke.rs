//! ISSUE-69: integration smoke proving `NodeGraphWidget` consumes a real
//! `rge_script_graph::ScriptGraph` through the existing
//! `rge_kernel_graph_foundation::VizAdapter` bridge.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic (knows nothing about
//! materials, animation, CAD, scripts, operators) and must NOT name
//! `rge-script-graph`. Only this integration test target consumes the
//! script graph crate, via the editor-ui `[dev-dependencies]` edge added
//! for ISSUE-69.

use rge_editor_ui::widgets::node_graph::NodeGraphWidget;
use rge_script_graph::{ScriptEdge, ScriptGraph};

#[test]
fn node_graph_widget_consumes_real_script_graph_through_viz_adapter() {
    // Build a real script graph: three script nodes joined by two script
    // edges with distinct key payloads, exercising the ScriptGraph adapter's
    // nodes/edges projection through the editor-ui generic surface.
    let mut graph = ScriptGraph::new();
    let entry = graph.add_node("entry").expect("add entry node");
    let body = graph.add_node("body").expect("add body node");
    let exit = graph.add_node("exit").expect("add exit node");

    let flow_edge = graph
        .connect(entry, body, ScriptEdge::new("flow"))
        .expect("connect entry -> body");
    let done_edge = graph
        .connect(body, exit, ScriptEdge::new("done"))
        .expect("connect body -> exit");

    // Ask the editor-ui widget for a model through the existing adapter
    // bridge — no production API was changed for this test. The widget
    // accepts any `&dyn VizAdapter`; passing `&ScriptGraph` here proves the
    // script-graph crate's adapter implementation is exposed verbatim through
    // editor-ui's generic surface.
    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    // Counts flow from the script graph unchanged.
    assert_eq!(model.node_count(), 3, "three script nodes are exposed");
    assert_eq!(model.edge_count(), 2, "two script edges are exposed");

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

    let entry_node = node_by_id(entry);
    let body_node = node_by_id(body);
    let exit_node = node_by_id(exit);

    // Script node `display_name` flows through unchanged from the script
    // graph's adapter (the uninterpreted script node `key`).
    assert_eq!(entry_node.display_name, "entry");
    assert_eq!(body_node.display_name, "body");
    assert_eq!(exit_node.display_name, "exit");

    // The script-graph adapter pins every node `kind` to the static
    // "ScriptNode" string — proves the widget routed it through
    // `VizAdapter::nodes`.
    assert_eq!(entry_node.kind, "ScriptNode");
    assert_eq!(body_node.kind, "ScriptNode");
    assert_eq!(exit_node.kind, "ScriptNode");

    // Edge records expose the exact endpoints and the deterministic
    // script edge key labels coined by the script-graph adapter.
    let flow_record = edge_by_id(flow_edge);
    assert_eq!(flow_record.src, entry);
    assert_eq!(flow_record.dst, body);
    assert_eq!(flow_record.label, "flow");

    let done_record = edge_by_id(done_edge);
    assert_eq!(done_record.src, body);
    assert_eq!(done_record.dst, exit);
    assert_eq!(done_record.label, "done");
}
