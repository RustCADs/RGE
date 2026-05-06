//! Integration test: load the vendored Lucide manifest and verify
//! that `icons.lookup("folder-open")` returns a valid handle that
//! resolves back to non-empty SVG bytes.
//!
//! This satisfies the W06 exit criterion:
//!     `icons.lookup("folder-open")` returns `IconHandle`.

use std::path::PathBuf;
use std::time::Duration;

use rge_ui_icons::IconRegistry;

fn lucide_manifest_path() -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root.join("assets/sets/lucide.icons.ron")
}

#[test]
fn lookup_folder_open_returns_handle() {
    let mut registry = IconRegistry::new();
    let id = registry
        .register_set(&lucide_manifest_path())
        .expect("manifest should load");
    assert_eq!(id.as_str(), "lucide");

    let handle = registry.lookup("folder-open").expect("folder-open exists");
    assert_eq!(handle.set.as_str(), "lucide");
    assert_eq!(handle.name.as_str(), "folder-open");

    let bytes = registry.svg_bytes(&handle).expect("read SVG");
    assert!(bytes.contains("<svg"), "must be SVG content");
    assert!(
        bytes.contains("currentColor"),
        "Lucide icons should use currentColor for tinting"
    );
}

#[test]
fn lookup_common_icons() {
    let mut registry = IconRegistry::new();
    registry.register_set(&lucide_manifest_path()).unwrap();

    // Spot-check the icons explicitly called out in the W06 plan.
    for required in &[
        "folder-open",
        "save",
        "undo",
        "redo",
        "play",
        "pause",
        "stop",
        "eye",
        "eye-off",
        "plus",
        "minus",
        "edit",
        "trash",
    ] {
        assert!(
            registry.lookup(required).is_some(),
            "icon {required:?} must be vendored"
        );
    }
}

#[test]
fn lookup_unknown_returns_none() {
    let mut registry = IconRegistry::new();
    registry.register_set(&lucide_manifest_path()).unwrap();
    assert!(registry.lookup("absolutely-not-a-real-icon").is_none());
}

#[test]
fn at_least_30_icons_vendored() {
    let mut registry = IconRegistry::new();
    let id = registry.register_set(&lucide_manifest_path()).unwrap();
    let info = registry.set_info(&id).unwrap();
    assert!(
        info.entries.len() >= 30,
        "at least 30 icons required, got {}",
        info.entries.len()
    );
}

#[test]
fn hot_reload_under_50ms() {
    let mut registry = IconRegistry::new();
    let id = registry.register_set(&lucide_manifest_path()).unwrap();

    // Warm caches by reading a handful of icons.
    for n in &["folder-open", "save", "play"] {
        if let Some(h) = registry.lookup(n) {
            let _bytes = registry.svg_bytes(&h);
        }
    }

    // Reload and assert the SLO. We tolerate one outlier (cold disk
    // cache, AV scanner, etc.) by retrying once.
    let mut elapsed = registry.reload_set(&id).expect("reload");
    if elapsed >= Duration::from_millis(50) {
        elapsed = registry.reload_set(&id).expect("reload");
    }
    assert!(
        elapsed < Duration::from_millis(50),
        "hot-reload SLO breach: {elapsed:?} >= 50ms"
    );
}

#[test]
fn license_field_populated() {
    let mut registry = IconRegistry::new();
    let id = registry.register_set(&lucide_manifest_path()).unwrap();
    let info = registry.set_info(&id).unwrap();
    assert!(!info.license.is_empty(), "license must be declared");
    assert!(!info.attribution.is_empty(), "attribution must be declared");
}
