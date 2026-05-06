# Wave W06 — ui-icons

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.2.6.

## Goal

Tintable SVG icon registry, name lookup, vendor Lucide as default icon set.

## Crate owned

`crates/ui-icons`.

## Files this wave touches

```
crates/ui-icons/src/{lib.rs, registry.rs, tint.rs, loader.rs, icon_handle.rs}
crates/ui-icons/assets/sets/lucide/                  # vendored MIT-licensed Lucide SVGs
crates/ui-icons/assets/sets/lucide.icons.ron         # name → file mapping
crates/ui-icons/tests/{lookup_test.rs, tint_test.rs}
```

## Stubs needed

- `ui-theme::Color` for tinting — local stub if W05 not merged.

## Implementation order

1. `icon_handle.rs` — `IconHandle(IconSetId, IconName)`.
2. `registry.rs` — `IconRegistry`: load icon sets, lookup by name, switch active set.
3. `loader.rs` — RON `.icons.ron` parser (name → SVG file path).
4. `tint.rs` — apply theme color to monochrome SVG; rasterize for egui display.
5. Vendor Lucide MIT-licensed icons (subset: ~200 most-used: folder-open, save, undo, redo, play, pause, stop, eye, eye-off, plus, minus, edit, trash, etc.).
6. Test: `icons.lookup("folder-open")` returns IconHandle; tint at 3 different theme colors verifies correctness.
7. CI test renders every icon × every vendored theme; flags unreadable contrast.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/shared-ui/` | shared egui types | check for icon-rendering helpers |
| `rustforge/apps/editor-app/` | egui usage | reference for embedding pattern |

Mostly greenfield — rustforge doesn't have a dedicated icon system.

## Exit criteria

- `icons.lookup("folder-open")` returns IconHandle.
- Tint at 3 theme colors (accent.action, error.500, text.muted) verifies correctness.
- 200+ Lucide icons available.
- Hot-reload of icon set < 50ms.
- `cargo test -p rge-ui-icons` passes.
- CI test: every icon × every theme renders with WCAG AA-passing contrast on the icon's intended background.

## Duration estimate

2 days.

## Anti-pattern check

PASS — separate registry from `ui-theme` (different lifecycle: icon-set swap ≠ theme swap). Per ADR-034.

## Handoff

After merge: W08 menus uses icons in MenuEntry; editor-ui widgets render IconButton.
