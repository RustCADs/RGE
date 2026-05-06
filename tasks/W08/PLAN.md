# Wave W08 — editor-ui/menus

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.3 (menu/toolbar registry); UE5 `UToolMenus` precedent.

## Goal

UE5 `UToolMenus`-inspired menu/toolbar registry. Data-driven; plugins register entries by ID with `OrderHint::Before/After`. Predicates: Rust closure or `expr-wasm`.

## Crate owned

`crates/editor-ui` (the `menus/` submodule specifically).

## Files this wave touches

```
crates/editor-ui/src/menus/{mod.rs, registry.rs, extension_point.rs, entry.rs, order_hint.rs, shortcut.rs, command.rs, predicate.rs}
crates/editor-ui/tests/menus_ordering.rs
```

## Stubs needed

- `ui-theme::Style` for menu styling — local stub if W05 not merged.
- `ui-icons::IconHandle` — local stub.
- `expr-wasm` for hot-reloadable predicates — optional; closure fallback always available.

## Implementation order

1. `extension_point.rs` — `ExtensionPoint(String)` (e.g., `"editor.main_menu.file"`); `register_extension_point()` registers a named slot.
2. `entry.rs` — `MenuEntry { id, label, icon, shortcut, command, section, order_hint, predicate, visible }`.
3. `order_hint.rs` — `OrderHint { Before(EntryId), After(EntryId), AtStart, AtEnd, InSection(String) }`.
4. `shortcut.rs` — `Shortcut { modifiers, key }`; global accelerator table; conflict detection.
5. `command.rs` — `Command` enum: open file, save, undo, redo, etc. (extension-pointed for plugins to add).
6. `predicate.rs` — `Predicate::Closure(fn) | Predicate::Expr(String)` (expr-wasm).
7. `registry.rs` — `MenuRegistry`: declare extension points, register entries, resolve order, build trees.
8. Test: declare 1 extension point; register 5 entries with mixed `Before/After/InSection`; resulting order matches expected.
9. Test: shortcut conflict detection; accelerator table O(1) lookup.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/egui_overlay.rs` | menu bar (existing) | study the consumer-side pattern; rebuild as data-driven registry |
| UE source (external ref): `Engine/Source/Editor/UnrealEd/.../ToolMenus/` | UE5 ToolMenus reference | adapt the data-driven model (no copy — it's C++) |

Header pattern: `// adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05 — rebuilt as data-driven MenuRegistry`.

## Exit criteria

- Declare extension point; register 5 entries with mixed ordering hints; resolved order matches expected.
- Plugins (Tier-3 stub) can register entries via the same API as core.
- Shortcut accelerator table is O(1) lookup; conflict detection works.
- Predicates work with both Closure and Expr variants.
- `cargo test -p rge-editor-ui` passes for menus module.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single menu registry. Plugins extend by ID via the same public API (dogfood rule).

## Handoff

After merge: editor app composes main menu + toolbars from the registry. W03 editor-shell registers `editor.play_mode.toolbar` extension point. W09 layout references extension-point IDs from workspace RON.
