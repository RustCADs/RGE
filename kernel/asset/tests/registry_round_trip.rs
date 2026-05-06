//! Integration test: Registry dependency-graph RON round-trip.

use rge_kernel_asset::{AssetId, Registry};

#[test]
fn dependency_graph_round_trips_via_ron() {
    let mut reg = Registry::new();
    let a = AssetId::from_bytes(b"a");
    let b = AssetId::from_bytes(b"b");
    let c = AssetId::from_bytes(b"c");
    reg.deps_mut().add_edge(a, b);
    reg.deps_mut().add_edge(a, c);
    reg.deps_mut().add_edge(c, b);
    let serialized = reg.serialize_deps().expect("serialize");

    let mut restored = Registry::new();
    restored.restore_deps(&serialized).expect("restore");
    let restored_serialized = restored.serialize_deps().expect("re-serialize");
    assert_eq!(
        serialized, restored_serialized,
        "dep graph round-trip stable"
    );
}
