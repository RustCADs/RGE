# RGE — Wave Task Dispatch Packages

Each `WNN/PLAN.md` is a self-contained dispatch package for one parallel agent. After workspace bootstrap (Phase 0.1, completed), waves W01–W20 can run in parallel without merge conflicts.

## Non-interference contract (per all waves)

Every wave:

1. Touches only its declared crate(s)
2. Modifies no other crate's files
3. Modifies no workspace-shared file (root `Cargo.toml`, `.gitignore`, `README.md`, etc.)
4. Stubs cross-crate deps locally if needed
5. Has tests that compile and pass without other waves' implementations

After all 20 merge, an integration phase wires real implementations in place of stubs.

## Wave index

| # | Crate(s) | Goal | Phase (per IMPLEMENTATION.md) |
|---|---|---|---|
| W01 | `components-spatial` ... `components-editor` (11 crates) | seed reusable component types | Phase 2 |
| W02 | `kernel/types`, `macros-reflect` | reflection registry + derive macro | Phase 1 |
| W03 | `editor-shell` | PIE skeleton (PlayState, snapshot/restore) | Phase 5 |
| W04 | `runtime-wasmtime-engine` | activate wasmtime; hello-world | Phase 3 |
| W05 | `ui-theme` | token registry, RON themes, hot-reload | Phase 5 |
| W06 | `ui-icons` | SVG icon registry, tinting | Phase 5 |
| W07 | `ui-fonts` | cosmic-text wrap | Phase 5 |
| W08 | `editor-ui/menus` | UE5 ToolMenus-style registry | Phase 5 |
| W09 | `editor-ui/layout` | RON workspace loader | Phase 5 |
| W10 | `editor-ui/dock` | TabManager + LayoutService on egui_dock | Phase 5 |
| W11 | `physics` | Rapier3D wrap, falling-cube smoke test | Phase 5+ |
| W12 | `audio` | Kira wrap | Phase 5+ |
| W13 | `input` | winit + gilrs fan-in | Phase 5 |
| W14 | `rge-data` | project/scene/prefab schemas + migrations | Phase 4 |
| W15 | `pak-format` | `.rge-pak` binary format | Phase 4 |
| W16 | `asset-store` | content-addressed cache | Phase 4 |
| W17 | `io-gltf` | glTF 2.0 import/export | Phase 4 |
| W18 | `io-image` | PNG/JPEG/EXR/HDR | Phase 4 |
| W19 | `expr-wasm` | string→AST→WASM compiler | Phase 3 |
| W20 | `script-bench` | benchmark suite skeleton | Phase 3 |

W21 (golden test projects) lives at `golden-projects/` and is not a wave dispatch — it's CI fixtures.

## Steal-and-adapt rule

Each wave references rustforge prior art under "Rustforge prior art." Per [PLAN.md §1.3 Rule 2](../plans/PLAN.md), agents must:

1. Read the rustforge source first
2. Copy + adapt with header `// adapted from rustforge::<path> on YYYY-MM-DD — <what changed>`
3. Run tests against rustforge's test fixtures where applicable
