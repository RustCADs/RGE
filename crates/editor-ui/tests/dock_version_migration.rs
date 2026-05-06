//! Integration test: layout-name version migration.
//!
//! Mirrors the W10 exit-criterion: "version migration v0.1 → v0.2 preserves geometry for
//! unchanged tabs". We verify:
//!  1. Saving a v0.1 layout, loading with target v0.2, applies migration.
//!  2. Tabs whose IDs are unchanged retain their relative position in the tree.
//!  3. Tabs removed from the spawner registry are dropped during migration.
//!  4. Patch-level changes (v0.1.0 → v0.1.5) are transparent — no migration runs.
//!  5. Cross-base layouts fall back to the supplied default.

use std::collections::HashSet;

use rge_editor_ui::dock::{
    Direction, LayoutBlueprint, LayoutName, LayoutService, TabId, TabManager,
};

fn v0_1_layout() -> LayoutBlueprint {
    TabManager::new_layout("rge_main_v0.1.0")
        .new_primary_area(Direction::Vertical, 1.0)
        .new_splitter(Direction::Horizontal, 0.7)
        .new_stack()
        .add_tab("viewport")
        .add_tab("scene_panel")
        .done()
        .new_stack()
        .add_tab("property_panel")
        .done()
        .done()
        .done()
        .build()
        .unwrap()
}

fn v0_2_default_layout() -> LayoutBlueprint {
    // v0.2 introduces a `console` tab, removes `scene_panel`. Geometry mostly preserved.
    TabManager::new_layout("rge_main_v0.2.0")
        .new_primary_area(Direction::Vertical, 1.0)
        .new_splitter(Direction::Horizontal, 0.7)
        .new_stack()
        .add_tab("viewport")
        .add_tab("console")
        .done()
        .new_stack()
        .add_tab("property_panel")
        .done()
        .done()
        .done()
        .build()
        .unwrap()
}

fn expected_v0_2_tabs() -> HashSet<TabId> {
    ["viewport", "property_panel", "console"]
        .iter()
        .map(|s| TabId::new(*s))
        .collect()
}

#[test]
fn minor_bump_triggers_migration_preserving_unchanged_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editor_layout.ron");

    // Save a v0.1 layout to disk.
    LayoutService::save(&v0_1_layout(), &path).unwrap();

    // Load with target v0.2 → migration kicks in.
    let target = LayoutName::parse("rge_main_v0.2.0").unwrap();
    let migrated =
        LayoutService::load_or_migrate(&path, &target, &expected_v0_2_tabs(), v0_2_default_layout)
            .expect("load_or_migrate");

    // Name re-stamped to target.
    assert_eq!(migrated.name.to_string(), "rge_main_v0.2.0");

    // Tabs surviving in the migrated layout are exactly those present in BOTH v0.1 AND
    // expected_v0_2_tabs (i.e. viewport + property_panel). scene_panel was dropped.
    let surviving: HashSet<String> = migrated
        .collect_tab_ids()
        .into_iter()
        .map(TabId::into_string)
        .collect();
    assert!(
        surviving.contains("viewport"),
        "viewport survives migration"
    );
    assert!(
        surviving.contains("property_panel"),
        "property_panel survives migration"
    );
    assert!(
        !surviving.contains("scene_panel"),
        "scene_panel removed by registry filter"
    );
}

#[test]
fn patch_change_does_not_trigger_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editor_layout.ron");
    LayoutService::save(&v0_1_layout(), &path).unwrap();

    // Same major.minor, just a patch bump.
    let target = LayoutName::parse("rge_main_v0.1.5").unwrap();
    let loaded =
        LayoutService::load_or_migrate(&path, &target, &expected_v0_2_tabs(), v0_2_default_layout)
            .unwrap();

    // Persisted blueprint kept its original name (no re-stamping for patch-only diffs).
    assert_eq!(loaded.name.to_string(), "rge_main_v0.1.0");
    // All v0.1 tabs preserved (no filtering).
    let ids: HashSet<String> = loaded
        .collect_tab_ids()
        .into_iter()
        .map(TabId::into_string)
        .collect();
    assert!(ids.contains("scene_panel"));
}

#[test]
fn cross_base_falls_back_to_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editor_layout.ron");
    LayoutService::save(&v0_1_layout(), &path).unwrap();

    // Different base entirely.
    let target = LayoutName::parse("animation_v0.1.0").unwrap();
    let result = LayoutService::load_or_migrate(&path, &target, &expected_v0_2_tabs(), || {
        TabManager::new_layout("animation_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
            .new_stack()
            .add_tab("timeline")
            .done()
            .done()
            .build()
            .unwrap()
    })
    .unwrap();

    assert_eq!(result.name.to_string(), "animation_v0.1.0");
    let ids: Vec<String> = result
        .collect_tab_ids()
        .into_iter()
        .map(TabId::into_string)
        .collect();
    assert_eq!(ids, vec!["timeline".to_string()]);
}

#[test]
fn migration_preserves_relative_order_of_surviving_tabs() {
    let v0_1 = TabManager::new_layout("rge_main_v0.1.0")
        .new_primary_area(Direction::Vertical, 1.0)
        .new_splitter(Direction::Horizontal, 0.5)
        .new_stack()
        .add_tab("a")
        .add_tab("b")
        .add_tab("c")
        .done()
        .new_stack()
        .add_tab("d")
        .done()
        .done()
        .done()
        .build()
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ordered.ron");
    LayoutService::save(&v0_1, &path).unwrap();

    // v0.2 keeps a/c/d, drops b. Order of survivors should be a, c, d.
    let target = LayoutName::parse("rge_main_v0.2.0").unwrap();
    let keep: HashSet<TabId> = ["a", "c", "d"].iter().map(|s| TabId::new(*s)).collect();
    let migrated = LayoutService::load_or_migrate(&path, &target, &keep, || v0_1.clone()).unwrap();
    let ids: Vec<String> = migrated
        .collect_tab_ids()
        .into_iter()
        .map(TabId::into_string)
        .collect();
    assert_eq!(ids, vec!["a", "c", "d"]);
}
