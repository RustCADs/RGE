# Wave W09 — editor-ui/layout

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.5 (page layout); ADR-018 (RON over JSON/XML).

## Goal

RON workspace loader/saver, layout-tree types, hot-reload watcher. Workspaces: `Default`, `Animation`, `Sculpt`, `Code`. Plugins ship workspaces.

## Crate owned

`crates/editor-ui` (the `layout/` submodule).

## Files this wave touches

```
crates/editor-ui/src/layout/{mod.rs, workspace.rs, node.rs, io.rs, reconcile.rs, version.rs}
crates/editor-ui/assets/defaults/{default-workspace.ron, animation-workspace.ron, sculpt-workspace.ron, code-workspace.ron}
crates/editor-ui/tests/{workspace_round_trip.rs, layout_migration.rs}
```

## Stubs needed

- `notify` for file watcher hot-reload.
- `editor-ui/dock` (W10) for tab IDs — local stub.

## Implementation order

1. `node.rs` — `LayoutNode` enum: `HSplit { ratio, left, right }`, `VSplit { ratio, top, bottom }`, `Stack { tabs: Vec<TabId> }`, `Toolbar { position, extension_point, visible: Option<String> }`.
2. `workspace.rs` — `Workspace { name, version, theme, layout, main_menu, toolbars, shortcuts_overlay }`.
3. `io.rs` — RON read/write; serde-typed.
4. `reconcile.rs` — diff-based hot-reload (stable IDs preserve scroll/selection/focus).
5. `version.rs` — workspace versioning (`v0.1.0` → `v0.2.0` migration).
6. Vendor 4 workspace defaults: Default (3-pane scene+viewport+inspector), Animation (anim graph + timeline), Sculpt (large viewport + brush panel), Code (script editor + inspector).
7. Test: load default workspace; serialize → deserialize byte-identically.
8. Test: migration v0.1 → v0.2 lossless on fixture.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/ir_bridge.rs` | RON load/save pattern (direct precursor) | direct adapt for Workspace I/O |
| `rustforge/apps/editor-app/assets/*.ron` | RON file format conventions | adapt formatting / commenting style |
| UE source (ref): `LayoutService.h` | FLayoutSaveRestore | adapt JSON-to-RON pattern |

Header pattern: `// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized for Workspace`.

## Exit criteria

- Load all 4 default workspaces from RON.
- Serialize workspace → byte-identical RON round-trip (CI gate).
- Migration v0.1 → v0.2 lossless on fixture.
- Hot-reload swap < 50ms (file-save → repaint).
- Diff-based reconcile preserves scroll/selection/focus.
- `cargo test -p rge-editor-ui` passes for layout module.

## Duration estimate

2 days.

## Anti-pattern check

PASS — RON only (single source-format family). Hot-reload via single watcher (notify).

## Handoff

After merge: W03 editor-shell loads default workspace at startup; W08 menus references extension-point IDs from workspace RON; W10 dock builds Tree<Tab> from layout nodes.
