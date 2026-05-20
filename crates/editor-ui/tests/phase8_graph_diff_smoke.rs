//! ISSUE-64: integration smoke proving the shared
//! `rge_kernel_graph_foundation::GraphDiff::between` path reports a single
//! added node and a single added edge for all three Phase 8 graph domains —
//! `rge_material_graph::MaterialGraph`, `rge_anim_graph::AnimGraph`, and
//! `rge_cad_core::OperatorGraph` — over snapshots projected through their
//! existing public `VizAdapter` surfaces.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic and the test target
//! is the only place that names `rge-material-graph`, `rge-anim-graph`, and
//! `rge-cad-core`. The wrapper domains do not expose their inner
//! `Graph<N, E>` publicly to editor-ui, so the snapshot helper bridges
//! through the public `VizAdapter` projection into a test-local
//! `Graph<DiffNode, String>` — keeping the smoke outside domain-private
//! internals while still exercising the shared `GraphSnapshot` / `GraphDiff`
//! path over the stable ids and edge records each domain already emits.

use rge_anim_graph::{AnimGraph, AnimTransition};
use rge_cad_core::{CuboidOp, OperatorGraph, OperatorNode, TransformOp};
use rge_kernel_graph_foundation::{EdgeRecord, Graph, GraphDiff, GraphSnapshot, VizAdapter};
use rge_material_graph::{MaterialEdge, MaterialGraph, PortType};

/// Test-local node payload that captures only the projected `VizAdapter`
/// fields the diff needs to inspect: the display name and kind strings.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DiffNode {
    display_name: String,
    kind: String,
}

/// Project a domain graph's public `VizAdapter` surface into a graph-foundation
/// `GraphSnapshot<DiffNode, String>`. The snapshot preserves every adapter
/// `NodeId`, `EdgeId`, edge `src`, edge `dst`, node display name, node kind,
/// and edge label exactly as the adapter exposes them.
fn snapshot_via_adapter(adapter: &dyn VizAdapter) -> GraphSnapshot<DiffNode, String> {
    let mut graph: Graph<DiffNode, String> = Graph::new();
    for view in adapter.nodes() {
        graph
            .insert_node(
                view.id,
                DiffNode {
                    display_name: view.display_name.to_owned(),
                    kind: view.kind.to_owned(),
                },
            )
            .expect("adapter node ids are unique");
    }
    for view in adapter.edges() {
        graph
            .insert_edge(view.id, view.src, view.dst, view.label.to_owned())
            .expect("adapter edge endpoints refer to inserted nodes");
    }
    GraphSnapshot::from_graph(&graph)
}

#[test]
fn graph_diff_between_reports_one_added_node_and_edge_for_all_three_phase8_domains() {
    assert_material_domain();
    assert_anim_domain();
    assert_operator_domain();
}

fn assert_material_domain() {
    let mut graph = MaterialGraph::new();
    let albedo = graph.add_node("albedo").expect("add albedo node");
    let output = graph.add_node("output").expect("add output node");
    graph
        .connect(
            albedo,
            output,
            MaterialEdge {
                src_port: PortType::Color,
                dst_port: PortType::Color,
            },
        )
        .expect("connect albedo -> output");
    let old_snapshot = snapshot_via_adapter(&graph);

    let normal = graph.add_node("normal").expect("add normal node");
    let new_edge_id = graph
        .connect(
            normal,
            output,
            MaterialEdge {
                src_port: PortType::Vector,
                dst_port: PortType::Texture,
            },
        )
        .expect("connect normal -> output");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one material node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&normal),
        Some(&DiffNode {
            display_name: "normal".to_owned(),
            kind: "MaterialNode".to_owned(),
        }),
        "the added node is the new 'normal' material node, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one material edge was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: normal,
            dst: output,
            data: "vector->texture".to_owned(),
        }),
        "the added edge record carries the new node, destination, and projected port-pair label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no material node was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no material edge was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing material node projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing material edge projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the material diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the material diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the material diff is non-empty (one add each on node and edge sides)"
    );
}

fn assert_anim_domain() {
    let mut graph = AnimGraph::new();
    let idle = graph.add_state("idle").expect("add idle state");
    let run = graph.add_state("run").expect("add run state");
    graph
        .add_transition(idle, run, AnimTransition::new("start_run"))
        .expect("add idle -> run transition");
    let old_snapshot = snapshot_via_adapter(&graph);

    let jump = graph.add_state("jump").expect("add jump state");
    let new_edge_id = graph
        .add_transition(run, jump, AnimTransition::new("leap"))
        .expect("add run -> jump transition");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one animation state was added"
    );
    assert_eq!(
        diff.added_nodes.get(&jump),
        Some(&DiffNode {
            display_name: "jump".to_owned(),
            kind: "AnimState".to_owned(),
        }),
        "the added node is the new 'jump' animation state, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one animation transition was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: run,
            dst: jump,
            data: "leap".to_owned(),
        }),
        "the added edge record carries the existing source, the new state, and the trigger label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no animation state was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no animation transition was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing animation state projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing animation transition projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the animation diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the animation diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the animation diff is non-empty (one add each on node and edge sides)"
    );
}

fn assert_operator_domain() {
    // Distinct transform payloads keep the two Transform NodeIds apart: the
    // CAD operator graph derives `NodeId` from the serialized operator
    // content, so two identical Transform payloads would collide on insert.
    let first_transform_op = OperatorNode::Transform(TransformOp {
        translation: [1.0, 0.0, 0.0],
        ..TransformOp::default()
    });
    let second_transform_op = OperatorNode::Transform(TransformOp {
        translation: [2.0, 0.0, 0.0],
        ..TransformOp::default()
    });

    let mut graph = OperatorGraph::new();
    let cuboid = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid");
    let first_transform = graph
        .add_operator(first_transform_op)
        .expect("add first transform");
    graph
        .connect(cuboid, first_transform, 0)
        .expect("connect cuboid -> first_transform port 0");
    let old_snapshot = snapshot_via_adapter(&graph);

    let second_transform = graph
        .add_operator(second_transform_op)
        .expect("add second transform with a distinct payload");
    let new_edge_id = graph
        .connect(first_transform, second_transform, 0)
        .expect("connect first_transform -> second_transform port 0");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one operator node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&second_transform),
        Some(&DiffNode {
            display_name: "Transform".to_owned(),
            kind: "Transform".to_owned(),
        }),
        "the added node is the second Transform operator, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one operator edge was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: first_transform,
            dst: second_transform,
            data: "input[0]".to_owned(),
        }),
        "the added edge record carries the first Transform, the new Transform, and the input-port-0 label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no operator node was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no operator edge was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing operator node projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing operator edge projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the operator diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the operator diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the operator diff is non-empty (one add each on node and edge sides)"
    );
}
