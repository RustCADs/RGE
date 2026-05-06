# Wave W05 — ui-theme

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.2 (theme support); fileandfolderstructure.md.

## Goal

Token registry + RON theme files + inheritance + dark/light defaults + variant stacking + hot-reload.

## Crate owned

`crates/ui-theme`.

## Files this wave touches

```
crates/ui-theme/src/{lib.rs, theme.rs, token.rs, style.rs, registry.rs, variant.rs, migration.rs, contrast.rs}
crates/ui-theme/assets/themes/{dark-default.theme.ron, light-default.theme.ron, studio-pro.theme.ron, daylight.theme.ron}
crates/ui-theme/tests/{inheritance_test.rs, variant_stacking_test.rs, wcag_contrast_lint.rs, hot_reload_test.rs}
```

## Stubs needed

- `kernel/diagnostics` for warnings on missing tokens — local stub.
- `notify` crate dep for hot-reload watcher.

## Implementation order

1. `token.rs` — `Token` enum: Color (sRGB+linear pair), Length (Px/Em/Pt/%), Font (family+size+weight), Padding/Margin (4-sided), Shadow (offset/blur/color), Animation (duration+curve).
2. `theme.rs` — `Theme { name, version, extends: Option<String>, variants: Vec<VariantTag>, tokens: HashMap, styles: HashMap }` + serde RON.
3. `variant.rs` — variant axes: scheme (dark/light), accessibility (high-contrast/reduced-motion/large-text/reduced-transparency), color-blind (protanopia/deuteranopia/tritanopia). Stacking resolution: base → scheme → a11y → color-blind → user override.
4. `style.rs` — `Style` struct with token references; resolution walks tokens to concrete values.
5. `registry.rs` — `ThemeRegistry`: load themes, switch active, scope resolution (widget → panel → window → workspace → global).
6. `migration.rs` — `version:` field; loader runs migrations on token renames; depreciation warnings 2 minor versions before removal.
7. `contrast.rs` — WCAG AA ratio computation; CI lint on vendored themes.
8. Hot-reload via `notify` watcher; <50ms file-save → repaint.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/egui_overlay.rs` | theme picker UI (partial) | extract pattern; rebuild as ThemeRegistry |
| `rustforge/crates/shared-ui/` | shared egui types | check for token-like primitives |
| `rustforge/apps/editor-app/assets/` | RON IR file patterns | adapt format conventions |

Header pattern: `// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry`.

## Exit criteria

- `dark-default` + `light-default` load correctly.
- Inheritance chain works (max depth 3, lint enforced).
- Variant stacking: `dark + high-contrast + protanopia + reduced-transparency` resolves correctly.
- WCAG AA contrast lint passes on all 4 vendored themes.
- Hot-reload swap < 50ms (file-save → repaint).
- `reduced-motion` zeros all `motion.*` durations.
- `cargo test -p rge-ui-theme` passes.

## Duration estimate

3 days.

## Anti-pattern check

PASS — single theme registry. ui-icons / ui-fonts are separate crates with their own lifecycles (per ADR-034 / ADR-035). Inline-styling CI lint forbids color/font/spacing constants in editor crates.

## Handoff

After merge: W08 menus consumes Style for menu entries; W09 layout consumes for workspace styling; W06 ui-icons tints icons via theme color tokens; W07 ui-fonts resolves font tokens.
