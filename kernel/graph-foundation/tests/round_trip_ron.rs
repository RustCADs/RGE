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

#[test]
fn restored_graph_rebuilds_fanout_metrics() {
    // Nontrivial fanout shape: 6 nodes, 5 directed edges.
    //
    //   1 ──▶ 2 ◀── 5
    //   │  ▲  ▲
    //   ▼  │  │
    //   3 ─┘  │
    //   │     │
    //   ▼     │
    //   4     │
    //         │
    //   6 (isolated)
    //
    // Per-node degrees the restored graph must report:
    //   node 1: out 3 (→2, →3, →4), in 0
    //   node 2: out 0,              in 3 (from 1, 3, 5)
    //   node 3: out 1 (→2),         in 1 (from 1)
    //   node 4: out 0,              in 1 (from 1)
    //   node 5: out 1 (→2),         in 0
    //   node 6: out 0,              in 0  (isolated)
    let mut g = Graph::<String, u32>::new();
    for i in 1..=6u128 {
        g.insert_node(NodeId::from_raw(i), format!("node-{i}"))
            .unwrap();
    }
    let edges: [(u128, u128, u128); 5] = [
        (200, 1, 2),
        (201, 1, 3),
        (202, 1, 4),
        (203, 5, 2),
        (204, 3, 2),
    ];
    for (eid, src, dst) in edges {
        #[allow(clippy::cast_possible_truncation)]
        let weight: u32 = eid as u32;
        g.insert_edge(
            EdgeId::from_raw(eid),
            NodeId::from_raw(src),
            NodeId::from_raw(dst),
            weight,
        )
        .unwrap();
    }

    // Round-trip via RON.
    let snap = GraphSnapshot::from_graph(&g);
    let ron = snap.to_ron().expect("serialize");
    let restored: GraphSnapshot<String, u32> = GraphSnapshot::from_ron(&ron).expect("deserialize");
    let restored_graph = restored.to_graph();

    // Counts.
    assert_eq!(restored_graph.node_count(), 6);
    assert_eq!(restored_graph.edge_count(), 5);

    // Per-node Tier-B fanout: rebuilt from restored edge records, so these
    // pin down that to_graph() replays adjacency, not just node/edge counts.
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(1)), 3);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(1)), 0);
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(2)), 0);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(2)), 3);
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(3)), 1);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(3)), 1);
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(4)), 0);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(4)), 1);
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(5)), 1);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(5)), 0);
    assert_eq!(restored_graph.node_out_degree(NodeId::from_raw(6)), 0);
    assert_eq!(restored_graph.node_in_degree(NodeId::from_raw(6)), 0);

    // Workspace-wide Tier-B caches.
    assert_eq!(restored_graph.max_out_fanout(), 3);
    assert_eq!(restored_graph.max_in_fanout(), 3);
    let expected_avg = 5.0_f64 / 6.0_f64;
    assert!(
        (restored_graph.average_fanout() - expected_avg).abs() < f64::EPSILON,
        "average_fanout = {} (expected {})",
        restored_graph.average_fanout(),
        expected_avg
    );
}
