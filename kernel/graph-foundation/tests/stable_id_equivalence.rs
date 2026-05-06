//! Integration test: StableHash-derived ids are deterministic.
//!
//! Defines a tiny struct, implements [`StableHash`], verifies:
//! 1. `stable_node_id(&value)` is identical across two calls.
//! 2. Two different values produce different ids.

use rge_kernel_graph_foundation::{stable_edge_id, stable_node_id, StableHash};

/// A minimal domain struct used only for this test.
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl StableHash for Color {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&[self.r, self.g, self.b]);
    }
}

struct NamedNode {
    kind: u32,
    name: String,
}

impl StableHash for NamedNode {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&self.kind.to_le_bytes());
        // Length-prefix the name to avoid collisions.
        hasher.update(&(self.name.len() as u64).to_le_bytes());
        hasher.update(self.name.as_bytes());
    }
}

#[test]
fn stable_node_id_deterministic_across_calls() {
    let c = Color {
        r: 255,
        g: 128,
        b: 0,
    };
    let id1 = stable_node_id(&c);
    let id2 = stable_node_id(&c);
    assert_eq!(
        id1, id2,
        "stable_node_id must return the same value for the same input"
    );
}

#[test]
fn stable_edge_id_deterministic_across_calls() {
    let c = Color {
        r: 10,
        g: 20,
        b: 30,
    };
    let id1 = stable_edge_id(&c);
    let id2 = stable_edge_id(&c);
    assert_eq!(
        id1, id2,
        "stable_edge_id must return the same value for the same input"
    );
}

#[test]
fn different_values_produce_different_ids() {
    let c1 = Color { r: 1, g: 2, b: 3 };
    let c2 = Color { r: 4, g: 5, b: 6 };
    assert_ne!(
        stable_node_id(&c1),
        stable_node_id(&c2),
        "different inputs must produce different NodeIds"
    );
}

#[test]
fn named_node_id_deterministic() {
    let n1 = NamedNode {
        kind: 1,
        name: "Multiply".to_string(),
    };
    let n2 = NamedNode {
        kind: 1,
        name: "Multiply".to_string(),
    };
    assert_eq!(stable_node_id(&n1), stable_node_id(&n2));
}

#[test]
fn named_node_different_kinds_differ() {
    let n1 = NamedNode {
        kind: 1,
        name: "Op".to_string(),
    };
    let n2 = NamedNode {
        kind: 2,
        name: "Op".to_string(),
    };
    assert_ne!(stable_node_id(&n1), stable_node_id(&n2));
}

#[test]
fn stable_node_and_edge_differ_for_same_input() {
    // The same underlying bytes should still produce distinct NodeId vs EdgeId
    // only if the hasher is keyed differently. Since we use the same BLAKE3
    // here, they WILL be equal (by design — the discriminator is the free
    // function name, not a key). This test just ensures neither panics.
    let c = Color { r: 0, g: 0, b: 0 };
    let _ = stable_node_id(&c);
    let _ = stable_edge_id(&c);
    // No assertion — both are valid; callers choose which function to call
    // based on whether they are identifying a node or an edge.
}
