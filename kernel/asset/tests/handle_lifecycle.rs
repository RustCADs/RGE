//! Integration test: Handle ref-count lifecycle and `sweep_orphans` behaviour.

use rge_kernel_asset::{AssetId, Registry};

#[test]
fn handle_lifecycle_and_sweep() {
    let mut reg = Registry::new();
    let aid = AssetId::from_bytes(b"lifecycle-asset");

    // Insert → strong_count is 1.
    let h1 = reg.insert(aid, String::from("hello"));
    assert_eq!(h1.strong_count(), 1);
    assert_eq!(reg.len(), 1);

    // Clone → strong_count is 2.
    let h2 = h1.clone();
    assert_eq!(h1.strong_count(), 2);
    assert_eq!(h2.strong_count(), 2);

    // Drop original → strong_count drops back to 1.
    drop(h1);
    assert_eq!(h2.strong_count(), 1);

    // sweep_orphans must NOT evict while h2 is alive.
    let swept = reg.sweep_orphans();
    assert_eq!(swept, 0);
    assert_eq!(reg.len(), 1);

    // Drop the last handle.
    drop(h2);

    // Now sweep_orphans should evict the entry.
    let swept = reg.sweep_orphans();
    assert_eq!(swept, 1);
    assert!(reg.is_empty());
}

#[test]
fn multiple_assets_independent_lifecycles() {
    let mut reg = Registry::new();
    let a = AssetId::from_bytes(b"aa");
    let b = AssetId::from_bytes(b"bb");

    let ha = reg.insert(a, 1u32);
    let hb = reg.insert(b, 2u32);

    // Drop a's handle; b's handle still alive.
    drop(ha);
    let swept = reg.sweep_orphans();
    assert_eq!(swept, 1, "only 'a' should be swept");
    assert_eq!(reg.len(), 1);
    assert!(reg.get::<u32>(b).expect("ok").is_some());

    drop(hb);
    let swept = reg.sweep_orphans();
    assert_eq!(swept, 1);
    assert!(reg.is_empty());
}
