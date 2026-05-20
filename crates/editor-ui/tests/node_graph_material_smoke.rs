//! ISSUE-60: integration smoke proving `NodeGraphWidget` consumes a real
//! `rge_material_graph::MaterialGraph` through the existing
//! `rge_kernel_graph_foundation::VizAdapter` bridge.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic (knows nothing about
//! materials, animation, CAD, scripts, operators) and must NOT name
//! `rge-material-graph`. Only this integration test target consumes the
//! material graph crate, via the editor-ui `[dev-dependencies]` edge added
//! for ISSUE-60.

use rge_editor_ui::widgets::node_graph::NodeGraphWidget;
use rge_material_graph::{MaterialEdge, MaterialGraph, PortType};

#[test]
fn node_graph_widget_consumes_real_material_graph_through_viz_adapter() {
    // Build a real material graph: three material nodes joined by two
    // typed-port connections (distinct port-type pairs).
    let mut graph = MaterialGraph::new();
    let albedo = graph.add_node("albedo").expect("add albedo node");
    let normal = graph.add_node("normal").expect("add normal node");
    let output = graph.add_node("output").expect("add output node");

    let albedo_edge = graph
        .connect(
            albedo,
            output,
            MaterialEdge {
                src_port: PortType::Color,
                dst_port: PortType::Color,
            },
        )
        .expect("connect albedo -> output");
    let normal_edge = graph
        .connect(
            normal,
            output,
            MaterialEdge {
                src_port: PortType::Vector,
                dst_port: PortType::Texture,
            },
        )
        .expect("connect normal -> output");

    // Ask the editor-ui widget for a model through the existing adapter
    // bridge — no production API was changed for this test. The widget
    // accepts any `&dyn VizAdapter`; passing `&MaterialGraph` here proves the
    // material crate's adapter implementation is exposed verbatim through
    // editor-ui's generic surface.
    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    // Counts flow from the material graph unchanged.
    assert_eq!(model.node_count(), 3, "three material nodes are exposed");
    assert_eq!(model.edge_count(), 2, "two material edges are exposed");

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

    let albedo_node = node_by_id(albedo);
    let normal_node = node_by_id(normal);
    let output_node = node_by_id(output);

    // Material node `display_name` flows through unchanged from the material
    // graph's adapter (the opaque material node `key`).
    assert_eq!(albedo_node.display_name, "albedo");
    assert_eq!(normal_node.display_name, "normal");
    assert_eq!(output_node.display_name, "output");

    // The material adapter pins every node `kind` to the static
    // "MaterialNode" string — proves the widget routed it through
    // `VizAdapter::nodes`.
    assert_eq!(albedo_node.kind, "MaterialNode");
    assert_eq!(normal_node.kind, "MaterialNode");
    assert_eq!(output_node.kind, "MaterialNode");

    // Edge records expose the exact endpoints and the deterministic
    // `src->dst` port-pair label coined by the material-graph adapter.
    let albedo_record = edge_by_id(albedo_edge);
    assert_eq!(albedo_record.src, albedo);
    assert_eq!(albedo_record.dst, output);
    assert_eq!(albedo_record.label, "color->color");

    let normal_record = edge_by_id(normal_edge);
    assert_eq!(normal_record.src, normal);
    assert_eq!(normal_record.dst, output);
    assert_eq!(normal_record.label, "vector->texture");
}
