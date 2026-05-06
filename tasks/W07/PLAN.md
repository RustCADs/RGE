# Wave W07 — ui-fonts

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.2.7.

## Goal

cosmic-text wrap + family resolver + vendored Inter and JetBrainsMono fonts.

## Crate owned

`crates/ui-fonts`.

## Files this wave touches

```
crates/ui-fonts/src/{lib.rs, registry.rs, resolver.rs, measure.rs, glyph_cache.rs}
crates/ui-fonts/assets/fonts/Inter/                  # vendored OFL-licensed
crates/ui-fonts/assets/fonts/JetBrainsMono/          # vendored OFL-licensed
crates/ui-fonts/tests/{measure_test.rs, fallback_test.rs}
```

## Stubs needed

- `cosmic-text` workspace dep.

## Implementation order

1. `registry.rs` — `FontRegistry`: load fonts from assets/, register family names.
2. `resolver.rs` — family name → file path; system font fallback chain.
3. `measure.rs` — cosmic-text shaping API wrap; text measurement (width, height, glyph positions).
4. `glyph_cache.rs` — atlas; invalidates on font swap (target: <100ms swap, glyph cache rebuild).
5. Vendor Inter Regular/Bold/Italic, JetBrainsMono Regular/Bold (all OFL).
6. Test: Inter Regular 13pt measurement matches reference within 1px tolerance.
7. Test: system-font lookup falls back gracefully when family not found.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/` | egui font usage | mostly default egui fonts; no specific stealing |

Mostly greenfield. rustforge uses egui defaults; we're upgrading to cosmic-text for proper shaping.

## Exit criteria

- Inter Regular 13pt: `measure("Hello World")` ≈ 75px ± 1px.
- Font swap < 100ms (file load + glyph cache rebuild).
- System fallback: missing family resolves to OS default.
- `cargo test -p rge-ui-fonts` passes.

## Duration estimate

2 days.

## Anti-pattern check

PASS — separate font subsystem; no sibling text-shaping engine. Single cosmic-text dep.

## Handoff

After merge: W05 ui-theme references font family names that resolve through `ui-fonts`; W08 menus / W04 widgets use measured text for layout.
