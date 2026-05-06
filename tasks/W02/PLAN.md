# Wave W02 — kernel/types + macros-reflect

> Self-contained agent dispatch. Phase 1.1 deliverable per IMPLEMENTATION.md. **Architectural root** — everything depends on this.
> Cross-refs: PLAN.md §1.2.4 (zero-copy asset views consumes), §6.15 (UI hints); IMPLEMENTATION.md Phase 1.1.

## Goal

Implement the reflection registry + `#[derive(Reflect)]` proc-macro. This is THE architectural root — every later subsystem (editor inspector, hot-reload migration, scripting bridge, asset metadata) depends on it.

## Crates owned by this wave

`kernel/types`, `crates/macros-reflect`.

## Files this wave touches

```
kernel/types/src/{lib.rs, reflect.rs, type_id.rs, field_descriptor.rs, ui_hint.rs, schema_version.rs, serde_bridge.rs}
kernel/types/tests/reflect_round_trip.rs
crates/macros-reflect/src/{lib.rs, derive.rs, attrs.rs, codegen.rs}
crates/macros-reflect/tests/{derive_test.rs, ui_hints_test.rs, validate_attr_test.rs}
crates/macros-reflect/tests/fixtures/render_pass.rs           # pilot type
```

## Stubs needed

None — this wave is the foundation. It stubs nothing.

## Implementation order

1. `kernel/types::TypeId` — interned, stable across builds, content-derived hash.
2. `kernel/types::FieldDescriptor` { name, ty: TypeId, range: Option<RangeMeta>, default: DefaultValue, ui_hint: UiHint, serde_skip: bool }.
3. `kernel/types::UiHint` — closed-set enum: `Default`, `Slider { min, max, step }`, `ColorRgb`, `ColorRgba`, `FilePath { extensions }`, `EnumDropdown`, `Multiline { lines }`, `Curve`, `Gradient`, `Foldout { default_open }`, `Inline`, `Hidden`.
4. `kernel/types::Reflect` trait — `type_name()`, `fields()`, `get_field_dyn()`, `set_field_dyn()`.
5. `kernel/types::serde_bridge` — round-trip via reflect walk.
6. `kernel/types::schema_version` — every reflected type carries `version: SchemaVersion`.
7. `crates/macros-reflect::derive` — proc-macro that emits `Reflect` impl from `#[derive(Reflect)]`.
8. `crates/macros-reflect::attrs` — `#[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.01)]`, `#[reflect(validate = "...")]`, `#[reflect(custom_drawer = "...")]`, `#[reflect(skip)]`.
9. Pilot type test: derive `Reflect` on `RenderPass` (fixture), round-trip via RON byte-identically.
10. Compile-time budget check: `cargo-llvm-lines` on a single reflected type; document baseline.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/macros/rcad-property/` | property derive macro (direct precursor) | adapt — most patterns transfer directly |
| `rustforge/crates/core/` | reflect IR types, IR-driven editor | inspect for existing reflect surface |
| `rustforge/apps/editor-app/src/egui_overlay.rs` | property-grid renderer using reflection | study the consumer side; informs UiHint design |
| `rustforge/crates/runtime-wasmtime/tests/` | trybuild compile-fail harness | adapt for macros-reflect compile-fail tests |

Header pattern: `// adapted from rustforge::macros::rcad-property on 2026-05-05 — added UiHint / SchemaVersion / validate-attr`.

## Exit criteria

- `cargo test -p rge-kernel-types` passes.
- `cargo test -p rge-macros-reflect` passes.
- `RenderPass` round-trips via RON serde byte-identically.
- `cargo-llvm-lines` baseline documented in `kernel/types/BUDGET.md`.
- **Compile-time gate (CRITICAL):** if reflection compile time on 5 pilot types > 30s, STOP and replan reflection strategy before W01 merges.

## Duration estimate

3 days. **Highest-risk Phase 1 wave** — extrapolation to ~100 reflected types must stay within §13.3 budgets.

## Anti-pattern check

PASS — single reflection registry, single proc-macro. No new runtime. UiHint is closed-set (CI lint flags additions for review per §6.15).

## Handoff

After merge: W01 components depend on `#[derive(Reflect)]`. W08 menus uses `UiHint` for inspector binding. W14 rge-data uses serde-bridge for project schema. W17 io-gltf uses reflection for component populating.
