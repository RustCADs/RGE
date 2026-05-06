//! Integration test: RON round-trip produces byte-identical output.
//!
//! Builds a 5-node graph (nodes typed as String, edges typed as u32 weights),
//! snapshots, serializes to RON, restores, re-snapshots, and asserts the
//! serialized bytes are identical.

use rge_kernel_graph_foundation::{EdgeId, Graph, GraphSnapshot, NodeId};

fn make_graph() -> Graph<String, u32> {
    let mut g = Graph::new();

    let ids: Vec<NodeId> = (0..5u128).map(NodeId::from_raw).collect();
    for (i, &id) in ids.iter().enumerate() {
        g.insert_node(id, format!("node-{i}")).unwrap();
    }

    // Add 4 edges: 0→1, 1→2, 2→3, 3→4
    for i in 0..4u128 {
        let eid = EdgeId::from_raw(100 + i);
        #[allow(clippy::cast_possible_truncation)]
        let weight: u32 = i as u32 * 10;
        g.insert_edge(eid, NodeId::from_raw(i), NodeId::from_raw(i + 1), weight)
            .unwrap();
    }

    g
}

#[test]
fn snapshot_ron_round_trip_byte_identical() {
    let g = make_graph();

    // First snapshot → RON
    let snap1 = GraphSnapshot::from_graph(&g);
    let ron1 = snap1.to_ron().expect("first serialize");

    // Restore → graph → second snapshot → RON
    let restored: GraphSnapshot<String, u32> = GraphSnapshot::from_ron(&ron1).expect("deserialize");
    let g2 = restored.to_graph();
    let snap2 = GraphSnapshot::from_graph(&g2);
    let ron2 = snap2.to_ron().expect("second serialize");

    assert_eq!(
        ron1, ron2,
        "RON bytes must be identical after round-trip through restore+re-snapshot"
    );

    // Structural integrity checks.
    assert_eq!(snap1.node_count(), snap2.node_count());
    assert_eq!(snap1.edge_count(), snap2.edge_count());
}

#[test]
fn restored_graph_is_mutable() {
    let g = make_graph();
    let snap = GraphSnapshot::from_graph(&g);
    let ron = snap.to_ron().unwrap();
    let restored: GraphSnapshot<String, u32> = GraphSnapshot::from_ron(&ron).unwrap();
    let mut g2 = restored.to_graph();

    // Should be able to insert a new node without errors.
    g2.insert_node(NodeId::from_raw(999), "extra".to_string())
        .unwrap();
    assert_eq!(g2.node_count(), 6);
}
