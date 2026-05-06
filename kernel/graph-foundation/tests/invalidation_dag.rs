//! Integration test: Invalidation propagates through a 4-node DAG.
//!
//! DAG: A → B, B → C, B → D
//! `dependents_of(A)` = [B]; `dependents_of(B)` = [C, D]; C/D → []
//!
//! `mark_dirty(A)` must invoke the listener for A, B, C, D
//! (in some order) and each exactly once (deduplicated).

use std::sync::{Arc, Mutex};

use rge_kernel_graph_foundation::{Invalidation, InvalidationListener, NodeId};

fn n(v: u128) -> NodeId {
    NodeId::from_raw(v)
}

#[derive(Clone)]
struct SharedRecorder(Arc<Mutex<Vec<NodeId>>>);

impl InvalidationListener for SharedRecorder {
    fn on_invalidated(&mut self, node: NodeId) {
        self.0.lock().unwrap().push(node);
    }
}

#[test]
fn mark_dirty_propagates_dag() {
    let a = n(0);
    let b = n(1);
    let c = n(2);
    let d = n(3);

    let log = Arc::new(Mutex::new(Vec::new()));
    let mut inv = Invalidation::new();
    inv.register(Box::new(SharedRecorder(log.clone())));

    inv.mark_dirty(a, |node| {
        if node == a {
            vec![b]
        } else if node == b {
            vec![c, d]
        } else {
            vec![]
        }
    });

    let calls = log.lock().unwrap().clone();

    // All four nodes must be present.
    for node in [a, b, c, d] {
        assert!(
            calls.contains(&node),
            "node {node} missing from invalidation calls: {calls:?}"
        );
    }

    // Each exactly once.
    assert_eq!(
        calls.len(),
        4,
        "listener must be called exactly once per node: {calls:?}"
    );
}

#[test]
fn mark_dirty_deduplicates_diamond() {
    // Diamond: A → B, A → C, B → D, C → D
    let a = n(10);
    let b = n(11);
    let c = n(12);
    let d = n(13);

    let log = Arc::new(Mutex::new(Vec::new()));
    let mut inv = Invalidation::new();
    inv.register(Box::new(SharedRecorder(log.clone())));

    inv.mark_dirty(a, |node| {
        if node == a {
            vec![b, c]
        } else if node == b || node == c {
            vec![d]
        } else {
            vec![]
        }
    });

    let calls = log.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        4,
        "four distinct nodes, each exactly once: {calls:?}"
    );
    assert_eq!(
        calls.iter().filter(|&&x| x == d).count(),
        1,
        "d must appear exactly once despite two paths: {calls:?}"
    );
}

#[test]
fn mark_dirty_single_node() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut inv = Invalidation::new();
    inv.register(Box::new(SharedRecorder(log.clone())));

    inv.mark_dirty(n(42), |_| vec![]);

    let calls = log.lock().unwrap().clone();
    assert_eq!(calls, vec![n(42)]);
}

#[test]
fn mark_dirty_no_listeners_ok() {
    let mut inv = Invalidation::new();
    // Should not panic even with no listeners.
    inv.mark_dirty(n(1), |_| vec![n(2), n(3)]);
}
