//! Unit tests for [`crate::graph::Graph`].
//!
//! Sub-module of [`crate::graph`]; see that module's `//!` docs for the
//! Tier-A counter / Tier-B fanout substrate design rationale these tests
//! exercise (ADR-115 phase-1 + phase-2).
//!
//! # Layout
//!
//! Pre-emptive Phase 5 split (mirrors the `kernel/plugin-host/src/host/`
//! split landed 2026-05-09): the original tests block in `graph.rs`
//! pushed the file past the 1000-line hard cap once the ADR-115 phase-2
//! Tier-B fanout tests landed, so the `#[cfg(test)] mod tests` block
//! was extracted here. Tests are grouped in source order rather than
//! sub-divided per concern because the substrate is small enough that
//! a single file (~440 lines, well under cap) keeps the test surface
//! discoverable for future ADR-115 phase-N additions.

use super::*;

fn n(v: u128) -> NodeId {
    NodeId::from_raw(v)
}
fn e(v: u128) -> EdgeId {
    EdgeId::from_raw(v)
}

#[test]
fn insert_and_retrieve_node() {
    let mut g: Graph<&str, ()> = Graph::new();
    g.insert_node(n(1), "hello").unwrap();
    assert_eq!(g.node(n(1)), Some(&"hello"));
}

#[test]
fn duplicate_node_fails() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.insert_node(n(1), 10).unwrap();
    let err = g.insert_node(n(1), 20).unwrap_err();
    assert_eq!(err, GraphError::DuplicateNode(n(1)));
}

#[test]
fn insert_edge_dangling_fails() {
    let mut g: Graph<i32, &str> = Graph::new();
    g.insert_node(n(1), 1).unwrap();
    // n(2) is absent
    let err = g.insert_edge(e(1), n(1), n(2), "x").unwrap_err();
    assert!(matches!(err, GraphError::DanglingEndpoint { .. }));
}

#[test]
fn remove_node_cascades_edges() {
    let mut g: Graph<i32, i32> = Graph::new();
    g.insert_node(n(1), 1).unwrap();
    g.insert_node(n(2), 2).unwrap();
    g.insert_edge(e(10), n(1), n(2), 99).unwrap();
    assert_eq!(g.edge_count(), 1);
    g.remove_node(n(1)).unwrap();
    assert_eq!(
        g.edge_count(),
        0,
        "removing src node must cascade edge removal"
    );
    assert_eq!(g.node_count(), 1);
}

#[test]
fn outgoing_incoming_consistent() {
    let mut g: Graph<i32, i32> = Graph::new();
    g.insert_node(n(1), 1).unwrap();
    g.insert_node(n(2), 2).unwrap();
    g.insert_edge(e(10), n(1), n(2), 0).unwrap();

    let out: Vec<_> = g.outgoing(n(1)).collect();
    assert_eq!(out, vec![e(10)]);
    let inc: Vec<_> = g.incoming(n(2)).collect();
    assert_eq!(inc, vec![e(10)]);
}

#[test]
fn outgoing_incoming_after_remove() {
    let mut g: Graph<i32, i32> = Graph::new();
    g.insert_node(n(1), 1).unwrap();
    g.insert_node(n(2), 2).unwrap();
    g.insert_edge(e(10), n(1), n(2), 0).unwrap();
    g.remove_edge(e(10)).unwrap();

    assert_eq!(g.outgoing(n(1)).count(), 0);
    assert_eq!(g.incoming(n(2)).count(), 0);
}

#[test]
fn replace_node() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.insert_node(n(1), 10).unwrap();
    let old = g.replace_node(n(1), 20).unwrap();
    assert_eq!(old, 10);
    assert_eq!(g.node(n(1)), Some(&20));
}

#[test]
fn replace_edge() {
    let mut g: Graph<i32, i32> = Graph::new();
    g.insert_node(n(1), 1).unwrap();
    g.insert_node(n(2), 2).unwrap();
    g.insert_edge(e(10), n(1), n(2), 5).unwrap();
    let old = g.replace_edge(e(10), 99).unwrap();
    assert_eq!(old, 5);
    assert_eq!(g.edge(e(10)).map(|r| r.data), Some(99));
}

// ---------------------------------------------------------------------
// Tier-A counter tests (ADR-115 phase-1)
// ---------------------------------------------------------------------
//
// These tests pin the Tier-A counters' transactional-update contract:
// every node/edge insert and remove is reflected in O(1) by the
// counter accessors. Cascading-remove behaviour (remove_node drops
// touching edges) is exercised so the counters stay consistent
// across the most complex substrate-level mutation.

#[test]
fn empty_graph_has_zero_node_and_edge_count() {
    let g: Graph<i32, i32> = Graph::new();
    assert_eq!(g.node_count(), 0);
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn node_count_reflects_add_node_calls() {
    let mut g: Graph<i32, ()> = Graph::new();
    assert_eq!(g.node_count(), 0);
    g.insert_node(n(1), 10).unwrap();
    assert_eq!(g.node_count(), 1);
    g.insert_node(n(2), 20).unwrap();
    assert_eq!(g.node_count(), 2);
    g.insert_node(n(3), 30).unwrap();
    assert_eq!(g.node_count(), 3);
    // edge_count untouched by node-only mutations.
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn edge_count_reflects_add_edge_calls() {
    let mut g: Graph<(), ()> = Graph::new();
    // Set up 4 nodes so we have somewhere to attach 3 edges.
    g.insert_node(n(1), ()).unwrap();
    g.insert_node(n(2), ()).unwrap();
    g.insert_node(n(3), ()).unwrap();
    g.insert_node(n(4), ()).unwrap();
    assert_eq!(g.edge_count(), 0);
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    assert_eq!(g.edge_count(), 1);
    g.insert_edge(e(11), n(2), n(3), ()).unwrap();
    assert_eq!(g.edge_count(), 2);
    g.insert_edge(e(12), n(3), n(4), ()).unwrap();
    assert_eq!(g.edge_count(), 3);
    // node_count untouched by edge-only mutations.
    assert_eq!(g.node_count(), 4);
}

#[test]
fn node_count_reflects_remove_node_cascading_edges() {
    // Build a 3-node fan: n(1) → n(2), n(1) → n(3); plus n(2) → n(3).
    // Removing n(1) must drop n(1)→n(2) and n(1)→n(3) (2 edges
    // cascaded), leaving n(2)→n(3) intact.
    let mut g: Graph<(), ()> = Graph::new();
    g.insert_node(n(1), ()).unwrap();
    g.insert_node(n(2), ()).unwrap();
    g.insert_node(n(3), ()).unwrap();
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    g.insert_edge(e(12), n(2), n(3), ()).unwrap();
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.edge_count(), 3);

    g.remove_node(n(1)).unwrap();

    assert_eq!(g.node_count(), 2, "removed n(1); count drops by 1");
    assert_eq!(
        g.edge_count(),
        1,
        "edges (1,2) + (1,3) cascade-removed; only (2,3) remains"
    );
}

/// Documents the O(1) property for `node_count`. No perf benchmark —
/// the property is structural: `BTreeMap::len()` is O(1) per the
/// `std::collections::BTreeMap::len` contract, and `node_count` is a
/// single-line forwarder. Tier-A invariant per ADR-115 sub-decision 2:
/// counter accessors MUST be queryable in constant time and MUST NOT
/// allocate. Asserting only that successive calls return the same
/// value (i.e. the accessor is stable) — the deeper guarantee is
/// enforced by the source-level shape, not by a runtime test.
#[test]
fn node_count_o1_property() {
    let mut g: Graph<u32, ()> = Graph::new();
    for i in 0u32..16 {
        g.insert_node(n(u128::from(i)), i).unwrap();
    }
    // Successive calls return identical values without mutating state.
    let first = g.node_count();
    let second = g.node_count();
    let third = g.node_count();
    assert_eq!(first, 16);
    assert_eq!(first, second);
    assert_eq!(second, third);
}

// ---------------------------------------------------------------------
// Tier-B fanout tests (ADR-115 phase-2)
// ---------------------------------------------------------------------
//
// These tests pin the Tier-B fanout substrate's invariants: per-node
// degree accessors are O(1) and reflect adjacency-cache state across
// mutations; workspace-wide max accessors are cache-maintained on
// insert (O(1)) and via partial recomputation on remove only when
// the cached value was potentially invalidated; average fanout is
// O(1) derivable from edge_count / node_count and handles the
// empty-graph case.

#[test]
fn empty_graph_has_zero_degrees_and_max_fanout() {
    let g: Graph<i32, i32> = Graph::new();
    assert_eq!(g.node_in_degree(n(1)), 0);
    assert_eq!(g.node_out_degree(n(1)), 0);
    assert_eq!(g.max_out_fanout(), 0);
    assert_eq!(g.max_in_fanout(), 0);
    // Average fanout for an empty graph is defined as 0.0
    // (no division by zero).
    assert!((g.average_fanout() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn unknown_node_returns_zero_degrees() {
    // Node accessors return 0 for nodes that are NOT in the graph
    // (no panic, no error). Documented contract per ADR-115 phase-2.
    let mut g: Graph<i32, i32> = Graph::new();
    g.insert_node(n(1), 0).unwrap();
    g.insert_node(n(2), 0).unwrap();
    g.insert_edge(e(10), n(1), n(2), 0).unwrap();
    assert_eq!(g.node_in_degree(n(99)), 0);
    assert_eq!(g.node_out_degree(n(99)), 0);
}

#[test]
fn add_edge_increments_src_out_and_dst_in_degree() {
    // Inserting an edge n(1) → n(2) bumps src out-degree and dst
    // in-degree by exactly 1; other endpoints are untouched.
    let mut g: Graph<(), ()> = Graph::new();
    g.insert_node(n(1), ()).unwrap();
    g.insert_node(n(2), ()).unwrap();
    g.insert_node(n(3), ()).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 0);
    assert_eq!(g.node_in_degree(n(2)), 0);

    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 1, "src out-degree += 1");
    assert_eq!(g.node_in_degree(n(2)), 1, "dst in-degree += 1");
    assert_eq!(g.node_out_degree(n(2)), 0, "n(2)'s out-degree untouched");
    assert_eq!(g.node_in_degree(n(1)), 0, "n(1)'s in-degree untouched");
    assert_eq!(g.node_out_degree(n(3)), 0);
    assert_eq!(g.node_in_degree(n(3)), 0);

    // Second edge n(1) → n(3) bumps src out-degree to 2.
    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 2);
    assert_eq!(g.node_in_degree(n(3)), 1);
}

#[test]
fn remove_edge_decrements_src_out_and_dst_in_degree() {
    let mut g: Graph<(), ()> = Graph::new();
    g.insert_node(n(1), ()).unwrap();
    g.insert_node(n(2), ()).unwrap();
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(1), n(2), ()).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 2);
    assert_eq!(g.node_in_degree(n(2)), 2);

    g.remove_edge(e(10)).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 1, "src out-degree -= 1");
    assert_eq!(g.node_in_degree(n(2)), 1, "dst in-degree -= 1");

    g.remove_edge(e(11)).unwrap();
    assert_eq!(g.node_out_degree(n(1)), 0);
    assert_eq!(g.node_in_degree(n(2)), 0);
}

#[test]
fn remove_node_cascades_neighbour_degrees() {
    // Build n(1) → n(2), n(1) → n(3), n(2) → n(3). After removing
    // n(1), n(2)'s in-degree drops 1 → 0 (cascade), and n(3)'s
    // in-degree drops 2 → 1 (loses n(1)→n(3); n(2)→n(3) survives).
    let mut g: Graph<(), ()> = Graph::new();
    g.insert_node(n(1), ()).unwrap();
    g.insert_node(n(2), ()).unwrap();
    g.insert_node(n(3), ()).unwrap();
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    g.insert_edge(e(12), n(2), n(3), ()).unwrap();
    assert_eq!(g.node_in_degree(n(2)), 1);
    assert_eq!(g.node_in_degree(n(3)), 2);
    assert_eq!(g.node_out_degree(n(2)), 1);

    g.remove_node(n(1)).unwrap();

    assert_eq!(
        g.node_in_degree(n(2)),
        0,
        "n(2)'s in-degree drops 1 → 0 (n(1)→n(2) cascaded)"
    );
    assert_eq!(
        g.node_in_degree(n(3)),
        1,
        "n(3)'s in-degree drops 2 → 1 (n(1)→n(3) cascaded; n(2)→n(3) survives)"
    );
    assert_eq!(g.node_out_degree(n(2)), 1, "n(2)→n(3) survives");
    // The removed node's degrees are 0 (not present).
    assert_eq!(g.node_in_degree(n(1)), 0);
    assert_eq!(g.node_out_degree(n(1)), 0);
}

#[test]
fn max_fanout_updates_after_add() {
    // Build an unbalanced fan: n(1) → {n(2), n(3), n(4)} (out 3),
    // and n(5) → n(2) (incoming on n(2) becomes 2). The cached
    // max_out_fanout should track 3; max_in_fanout should track 2.
    let mut g: Graph<(), ()> = Graph::new();
    for i in 1u128..=5 {
        g.insert_node(n(i), ()).unwrap();
    }
    assert_eq!(g.max_out_fanout(), 0);
    assert_eq!(g.max_in_fanout(), 0);

    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 1);
    assert_eq!(g.max_in_fanout(), 1);

    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 2);
    assert_eq!(g.max_in_fanout(), 1);

    g.insert_edge(e(12), n(1), n(4), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 3);
    assert_eq!(g.max_in_fanout(), 1);

    // Bump n(2)'s in-degree to 2.
    g.insert_edge(e(13), n(5), n(2), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 3, "n(1) still leads with 3");
    assert_eq!(g.max_in_fanout(), 2, "n(2) now leads with 2");
}

#[test]
fn max_fanout_recomputes_after_remove_of_max_node() {
    // Set up a max-out-degree node n(1) with out-degree 3, and a
    // separate node n(5) with out-degree 1. After removing n(1),
    // the cached max_out_fanout MUST drop from 3 to 1 (recomputed)
    // — NOT remain stale at 3.
    let mut g: Graph<(), ()> = Graph::new();
    for i in 1u128..=6 {
        g.insert_node(n(i), ()).unwrap();
    }
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    g.insert_edge(e(12), n(1), n(4), ()).unwrap();
    g.insert_edge(e(13), n(5), n(6), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 3);
    assert_eq!(g.max_in_fanout(), 1);

    // Remove the max-holder: max_out_fanout MUST recompute to 1.
    g.remove_node(n(1)).unwrap();
    assert_eq!(g.node_count(), 5);
    assert_eq!(g.edge_count(), 1, "only n(5)→n(6) survives");
    assert_eq!(
        g.max_out_fanout(),
        1,
        "cache MUST recompute to 1 after removing the max-holder"
    );
    assert_eq!(g.max_in_fanout(), 1);

    // Remove the last edge: both maxima MUST drop to 0.
    g.remove_edge(e(13)).unwrap();
    assert_eq!(g.max_out_fanout(), 0);
    assert_eq!(g.max_in_fanout(), 0);
}

#[test]
fn max_fanout_unchanged_when_non_max_node_removed() {
    // Cached max_out_fanout = 3 (held by n(1)). Removing an edge
    // from a NON-max node (n(5)→n(6)) MUST leave the cached max at
    // 3 without triggering recomputation. The end value being
    // correct is the observable contract; the no-recompute pathway
    // is the documented O(1) optimization (verified structurally
    // here — the result is identical either way).
    let mut g: Graph<(), ()> = Graph::new();
    for i in 1u128..=6 {
        g.insert_node(n(i), ()).unwrap();
    }
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(1), n(3), ()).unwrap();
    g.insert_edge(e(12), n(1), n(4), ()).unwrap();
    g.insert_edge(e(13), n(5), n(6), ()).unwrap();
    assert_eq!(g.max_out_fanout(), 3);

    g.remove_edge(e(13)).unwrap();
    assert_eq!(
        g.max_out_fanout(),
        3,
        "non-max-node remove MUST NOT decrement the cached max"
    );
}

#[test]
fn average_fanout_after_various_states() {
    // Empty graph → 0.0.
    let mut g: Graph<(), ()> = Graph::new();
    assert!((g.average_fanout() - 0.0).abs() < f64::EPSILON);

    // 4 nodes, 0 edges → 0.0.
    for i in 1u128..=4 {
        g.insert_node(n(i), ()).unwrap();
    }
    assert!((g.average_fanout() - 0.0).abs() < f64::EPSILON);

    // 4 nodes, 2 edges → 0.5.
    g.insert_edge(e(10), n(1), n(2), ()).unwrap();
    g.insert_edge(e(11), n(2), n(3), ()).unwrap();
    assert!(
        (g.average_fanout() - 0.5).abs() < f64::EPSILON,
        "2 edges / 4 nodes = 0.5"
    );

    // 4 nodes, 4 edges → 1.0.
    g.insert_edge(e(12), n(3), n(4), ()).unwrap();
    g.insert_edge(e(13), n(4), n(1), ()).unwrap();
    assert!(
        (g.average_fanout() - 1.0).abs() < f64::EPSILON,
        "4 edges / 4 nodes = 1.0"
    );

    // After removing an edge, the average reflects the new state.
    g.remove_edge(e(13)).unwrap();
    assert!((g.average_fanout() - 0.75).abs() < f64::EPSILON);

    // After removing a node (cascades 1 outgoing + 1 incoming edge),
    // the average reflects the new state.
    g.remove_node(n(2)).unwrap();
    // 3 nodes remain; 1 edge remains (n(3)→n(4); the surviving edge
    // among (10) src-cascaded and (11) src-cascaded and (12) intact).
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.edge_count(), 1);
    let expected = 1.0_f64 / 3.0;
    assert!((g.average_fanout() - expected).abs() < f64::EPSILON);
}
