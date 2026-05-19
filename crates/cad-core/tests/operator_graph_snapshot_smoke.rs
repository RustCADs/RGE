//! Integration smoke test for GitHub issue #54: `OperatorGraph` reuses
//! `kernel/graph-foundation`'s `GraphSnapshot` for a CAD operator graph.
//!
//! The test builds a small `Cuboid -> Transform` operator graph through the
//! public cad-core API, captures `graph.inner()` with `GraphSnapshot`,
//! round-trips the snapshot through RON, materializes the deserialized
//! snapshot back into a `Graph`, and asserts that counts plus the exact
//! `NodeId -> OperatorNode` and `EdgeId -> EdgeRecord<EdgeKind>` payloads
//! survive the round-trip unchanged.
//!
//! Scope is test coverage only: it adds no snapshot wrapper API to cad-core
//! and exercises only the existing `GraphSnapshot::{from_graph, to_ron,
//! from_ron, to_graph}` paths.

use rge_cad_core::{CuboidOp, EdgeKind, OperatorGraph, OperatorNode, TransformOp};
use rge_kernel_graph_foundation::{EdgeId, EdgeRecord, GraphSnapshot, NodeId};

#[test]
fn operator_graph_snapshot_round_trip_preserves_nodes_and_edges() {
    // --- Build the source operator graph: Cuboid -> Transform ------------
    let cuboid_node = OperatorNode::Cuboid(CuboidOp {
        width: 2.0,
        height: 1.0,
        depth: 1.5,
    });
    // A non-default translation keeps the payload distinctive so the
    // post-round-trip equality assertions are meaningful.
    let transform_node = OperatorNode::Transform(TransformOp {
        translation: [3.0, -1.5, 2.0],
        ..TransformOp::default()
    });

    // Clone the expected payloads before insertion so the post-round-trip
    // assertions compare against values the graph itself never owned.
    let expected_cuboid = cuboid_node.clone();
    let expected_transform = transform_node.clone();

    let mut graph = OperatorGraph::new();
    let cuboid_id = graph.add_operator(cuboid_node).expect("add cuboid");
    let transform_id = graph.add_operator(transform_node).expect("add transform");
    let edge_id = graph
        .connect(cuboid_id, transform_id, 0)
        .expect("connect cuboid -> transform");

    assert_eq!(graph.node_count(), 2, "source graph has two operators");
    assert_eq!(graph.edge_count(), 1, "source graph has one edge");

    // --- Capture an immutable snapshot of the inner graph ----------------
    let original = GraphSnapshot::from_graph(graph.inner());
    assert_eq!(original.node_count(), 2, "original snapshot: two nodes");
    assert_eq!(original.edge_count(), 1, "original snapshot: one edge");

    let original_nodes: Vec<(NodeId, OperatorNode)> = original
        .nodes()
        .map(|(id, node)| (id, node.clone()))
        .collect();
    let original_edges: Vec<(EdgeId, EdgeRecord<EdgeKind>)> = original
        .edges()
        .map(|(id, rec)| (id, rec.clone()))
        .collect();

    // --- RON round-trip --------------------------------------------------
    let ron = original.to_ron().expect("serialize snapshot to RON");
    let restored_snapshot: GraphSnapshot<OperatorNode, EdgeKind> =
        GraphSnapshot::from_ron(&ron).expect("deserialize snapshot from RON");
    assert_eq!(
        restored_snapshot.node_count(),
        2,
        "deserialized snapshot: two nodes"
    );
    assert_eq!(
        restored_snapshot.edge_count(),
        1,
        "deserialized snapshot: one edge"
    );

    let restored_nodes: Vec<(NodeId, OperatorNode)> = restored_snapshot
        .nodes()
        .map(|(id, node)| (id, node.clone()))
        .collect();
    let restored_edges: Vec<(EdgeId, EdgeRecord<EdgeKind>)> = restored_snapshot
        .edges()
        .map(|(id, rec)| (id, rec.clone()))
        .collect();

    // Concrete records — not just counts — survive the RON round-trip.
    assert_eq!(
        original_nodes, restored_nodes,
        "snapshot (NodeId, OperatorNode) pairs preserved through RON"
    );
    assert_eq!(
        original_edges, restored_edges,
        "snapshot (EdgeId, EdgeRecord<EdgeKind>) pairs preserved through RON"
    );

    // --- Materialize the deserialized snapshot back into a graph ---------
    let restored_graph = restored_snapshot.to_graph();
    assert_eq!(restored_graph.node_count(), 2, "restored graph: two nodes");
    assert_eq!(restored_graph.edge_count(), 1, "restored graph: one edge");

    // --- Exact NodeId -> OperatorNode preservation -----------------------
    // The original ids returned by `add_operator` still resolve to the
    // exact expected operator payloads after RON round-trip and restore.
    assert_eq!(
        restored_graph.node(cuboid_id),
        Some(&expected_cuboid),
        "cuboid NodeId resolves to the exact CuboidOp payload after round-trip"
    );
    assert_eq!(
        restored_graph.node(transform_id),
        Some(&expected_transform),
        "transform NodeId resolves to the exact TransformOp payload after round-trip"
    );

    // --- Exact EdgeId -> EdgeRecord<EdgeKind> preservation ---------------
    let restored_edge = restored_graph
        .edge(edge_id)
        .expect("original EdgeId resolves in the restored graph");
    assert_eq!(restored_edge.src, cuboid_id, "edge src preserved");
    assert_eq!(restored_edge.dst, transform_id, "edge dst preserved");
    assert_eq!(
        restored_edge.data,
        EdgeKind::Input(0),
        "edge payload preserved as Input(0)"
    );

    // Compare the whole record, not only its individual fields.
    let expected_edge = EdgeRecord {
        src: cuboid_id,
        dst: transform_id,
        data: EdgeKind::Input(0),
    };
    assert_eq!(
        *restored_edge, expected_edge,
        "restored EdgeRecord matches the expected (src, dst, data) triple"
    );
}
