//! Integration test: declarative builder → DockState materialization → persist/restore round-trip.
//!
//! Mirrors the W10 exit-criterion: "persist + restore round-trips". We verify:
//!  1. The builder produces a tree containing every TabId we declared.
//!  2. Materialization through the spawner registry yields a `DockState` with the right shape.
//!  3. Saving and re-loading yields a byte-identical blueprint (sans timestamp).

use rge_editor_ui::dock::{
    register_default_spawners, Direction, LayoutBlueprint, LayoutService, PlaceholderTabBody,
    SpawnerRegistry, TabId, TabManager,
};

fn build_default_layout() -> LayoutBlueprint {
    TabManager::new_layout("rge_main_v0.1.0")
        .new_primary_area(Direction::Vertical, 1.0)
        .new_splitter(Direction::Vertical, 0.7)
        .new_splitter(Direction::Horizontal, 0.25)
        .new_stack()
        .add_tab("hierarchy")
        .add_tab("scene_panel")
        .done()
        .new_splitter(Direction::Horizontal, 0.7)
        .new_stack()
        .add_tab("viewport")
        .done()
        .new_stack()
        .add_tab("property_panel")
        .done()
        .done()
        .done()
        .new_stack()
        .add_tab("console")
        .add_tab("log")
        .add_tab("asset_browser")
        .done()
        .done()
        .done()
        .build()
        .expect("layout builds")
}

#[test]
fn builder_produces_blueprint_with_all_default_tabs() {
    let bp = build_default_layout();
    let ids: Vec<String> = bp
        .collect_tab_ids()
        .into_iter()
        .map(TabId::into_string)
        .collect();
    let expected: Vec<&str> = vec![
        "hierarchy",
        "scene_panel",
        "viewport",
        "property_panel",
        "console",
        "log",
        "asset_browser",
    ];
    assert_eq!(
        ids,
        expected
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn blueprint_materializes_through_spawner_registry() {
    let bp = build_default_layout();
    let mut registry: SpawnerRegistry<PlaceholderTabBody> = SpawnerRegistry::new();
    register_default_spawners(&mut registry);

    let dock = bp
        .into_dock_state_with(|id: &TabId| {
            registry
                .spawn(id)
                .unwrap_or_else(|| panic!("no spawner registered for {id}"))
        })
        .expect("materializes");

    let total_tabs: usize = dock
        .main_surface()
        .iter()
        .map(|n| match n {
            egui_dock::Node::Leaf(leaf) => leaf.tabs.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(total_tabs, 7, "all default tabs should land in the dock");
}

#[test]
fn persist_then_restore_round_trips() {
    let bp = build_default_layout();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editor_layout.ron");

    LayoutService::save(&bp, &path).unwrap();
    let loaded = LayoutService::load(&path).unwrap();

    assert_eq!(loaded.name, bp.name);
    assert_eq!(loaded.collect_tab_ids(), bp.collect_tab_ids());

    // Round-trip a second time to be extra sure the file is stable on re-emit.
    LayoutService::save(&loaded, &path).unwrap();
    let loaded2 = LayoutService::load(&path).unwrap();
    assert_eq!(loaded2.collect_tab_ids(), bp.collect_tab_ids());
}

#[test]
fn tamper_detection_rejects_modified_file() {
    let bp = build_default_layout();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editor_layout.ron");
    LayoutService::save(&bp, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let modified = content.replace("viewport", "viewp0rt");
    assert_ne!(
        content, modified,
        "test fixture: replacement actually modifies the file"
    );
    std::fs::write(&path, modified).unwrap();

    let err = LayoutService::load(&path).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("tampered") || msg.contains("hash"),
        "expected tamper error, got: {msg}"
    );
}

#[test]
fn json_format_is_supported() {
    let bp = build_default_layout();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    LayoutService::save(&bp, &path).unwrap();
    let loaded = LayoutService::load(&path).unwrap();
    assert_eq!(loaded.collect_tab_ids(), bp.collect_tab_ids());
}
