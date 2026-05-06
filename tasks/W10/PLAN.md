# Wave W10 — editor-ui/dock

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.6 (dock + tab manager); UE Slate `FTabManager`/`FLayoutSaveRestore`/`FGlobalTabmanager`.

## Goal

`TabManager` + `LayoutService` + `SpawnerRegistry` on top of `egui_dock`. Layout-name versioning rule mandatory (`rge_main_v0.1.0` → `_v0.2.0`).

## Crate owned

`crates/editor-ui` (the `dock/` submodule).

## Files this wave touches

```
crates/editor-ui/src/dock/{mod.rs, tab_manager.rs, layout_service.rs, spawner_registry.rs, version.rs, tab_id.rs}
crates/editor-ui/tests/dock_persist.rs
crates/editor-ui/tests/dock_version_migration.rs
```

## Stubs needed

- `egui_dock` workspace dep — pinned, vendored to `third_party/egui_dock/` per §1.12 pressure tracking.
- `editor-ui/layout` (W09) for `LayoutNode` types — local stub.

## Implementation order

1. `tab_id.rs` — `TabId(String)`, stable identifiers.
2. `tab_manager.rs` — `FTabManager` equivalent. Declarative builder: `TabManager::new_layout("rge_main_v0.1.0").new_primary_area(...).new_splitter(...).new_stack().add_tab(...).done().build()`.
3. `layout_service.rs` — `FLayoutSaveRestore` equivalent. Persistence to `~/.config/rge/editor_layout.json` (or RON). Tamper detection via blake3 over layout content.
4. `spawner_registry.rs` — `FGlobalTabmanager::RegisterNomadTabSpawner` equivalent. Map `TabId → factory closure`. `register_default_spawners(&mut SpawnerRegistry)` for built-in tabs (scene_panel, hierarchy, viewport, property_panel, asset_browser, etc.).
5. `version.rs` — layout-name versioning (`rge_main_v0.1.0`); migration on suffix change (preserve geometry for unchanged tabs).
6. Test: declarative layout builder produces correct `egui_dock::Tree<Tab>`.
7. Test: persist + restore round-trips.
8. Test: version migration v0.1 → v0.2 preserves geometry for unchanged tabs.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| UE source (external ref): `Engine/Source/Runtime/Slate/Public/Framework/Docking/{TabManager.h, LayoutService.h, WorkspaceItem.h, SDockTab.h}` | Slate dock primitives (precursor) | adapt declarative builder pattern (no C++ copy) |
| `rustforge/apps/editor-app/src/egui_overlay.rs` | egui usage in editor-app | reference only (no docking there) |

Mostly greenfield. Build on `egui_dock` upstream; add the schema/persistence/spawner-registry layer.

## Exit criteria

- Declarative `TabManager::new_layout(...)` builder produces correct `Tree<Tab>`.
- Persist + restore round-trips.
- Version migration v0.1 → v0.2 preserves geometry for unchanged tabs.
- Plugin-registered spawners work (Tier-3 stub).
- `cargo test -p rge-editor-ui` passes for dock module.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single dock subsystem on top of vendored `egui_dock`. Pressure tracked per §1.12 (no vendor patches required at v0.0.1).

## Handoff

After merge: W09 layout RON references TabIds resolved through SpawnerRegistry; W08 menus may register Window-menu entries that toggle docked tabs.
