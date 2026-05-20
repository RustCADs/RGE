//! ISSUE-61: integration smoke proving `NodeGraphWidget` consumes a real
//! `rge_anim_graph::AnimGraph` through the existing
//! `rge_kernel_graph_foundation::VizAdapter` bridge.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic (knows nothing about
//! materials, animation, CAD, scripts, operators) and must NOT name
//! `rge-anim-graph`. Only this integration test target consumes the
//! animation graph crate, via the editor-ui `[dev-dependencies]` edge added
//! for ISSUE-61.

use rge_anim_graph::{AnimGraph, AnimTransition};
use rge_editor_ui::widgets::node_graph::NodeGraphWidget;

#[test]
fn node_graph_widget_consumes_real_anim_graph_through_viz_adapter() {
    // Build a real animation graph: two states joined by one transition,
    // exercising the AnimGraph adapter's nodes/edges projection through the
    // editor-ui generic surface.
    let mut graph = AnimGraph::new();
    let idle = graph.add_state("idle").expect("add idle state");
    let run = graph.add_state("run").expect("add run state");

    let start_run = graph
        .add_transition(idle, run, AnimTransition::new("start_run"))
        .expect("add idle -> run transition");

    // Ask the editor-ui widget for a model through the existing adapter
    // bridge — no production API was changed for this test. The widget
    // accepts any `&dyn VizAdapter`; passing `&AnimGraph` here proves the
    // anim-graph crate's adapter implementation is exposed verbatim through
    // editor-ui's generic surface.
    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    // Counts flow from the animation graph unchanged.
    assert_eq!(model.node_count(), 2, "two animation states are exposed");
    assert_eq!(model.edge_count(), 1, "one animation transition is exposed");

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

    let idle_node = node_by_id(idle);
    let run_node = node_by_id(run);

    // Animation node `display_name` flows through unchanged from the anim
    // graph's adapter (the uninterpreted animation state `key`).
    assert_eq!(idle_node.display_name, "idle");
    assert_eq!(run_node.display_name, "run");

    // The anim-graph adapter pins every node `kind` to the static
    // "AnimState" string — proves the widget routed it through
    // `VizAdapter::nodes`.
    assert_eq!(idle_node.kind, "AnimState");
    assert_eq!(run_node.kind, "AnimState");

    // The edge record exposes the exact endpoints and the deterministic
    // transition-trigger label coined by the anim-graph adapter.
    let start_run_record = edge_by_id(start_run);
    assert_eq!(start_run_record.src, idle);
    assert_eq!(start_run_record.dst, run);
    assert_eq!(start_run_record.label, "start_run");
}
