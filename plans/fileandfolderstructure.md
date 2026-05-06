# RGE — File and Folder Structure (v0.8)

> **Status:** Workspace skeleton specification. Companion to [`PLAN.md`](./PLAN.md) (architecture, frozen at v0.8), [`WAVES.md`](./WAVES.md) (parallel dispatch), and [`IMPLEMENTATION.md`](./IMPLEMENTATION.md) (sequencing).
>
> **Drafted:** 2026-05-05.
>
> **Purpose:** The folder structure encodes the architecture. The goal is not "where do files live" — it is **make illegal architecture physically difficult**.

---

## 0. Relationship to other docs

- **PLAN.md** = *what* the architecture is (constitutional)
- **IMPLEMENTATION.md** = *order* of implementation (de-risking)
- **WAVES.md** = *parallel* dispatch view
- **fileandfolderstructure.md (this doc)** = *physical layout* — how the architecture is embodied on disk

This doc is the deliverable of [`IMPLEMENTATION.md` Phase 0.1](./IMPLEMENTATION.md): the workspace skeleton that gets created in Week 1 before any engine code.

---

## 1. Top-level workspace

```text
rge/
├── Cargo.toml                # workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # pinned toolchain for reproducible builds
├── rustfmt.toml              # formatting policy
├── clippy.toml               # lint policy
├── deny.toml                 # cargo-deny config (license + audit + dup detection)
├── taplo.toml                # TOML formatting
├── .cargo/                   # workspace cargo config (target-specific flags)
├── .github/                  # CI workflows (lint, build, test, architecture)
│
├── kernel/                   # Tier 1 — constitutional substrate
├── crates/                   # Tier 2 — privileged systems
├── plugins/                  # Tier 3 — sandbox/plugin examples
│
├── runtime/                  # executable app targets (desktop/mobile/web/headless)
├── editor/                   # editor host app
│
├── golden-projects/          # regression validation fixtures
├── tools/                    # architecture enforcement + CI tooling
├── schemas/                  # WIT / reflection / validation schemas
├── docs/                     # ADRs + constitutional docs + companion plans
├── tests/                    # cross-workspace integration tests
├── examples/                 # standalone usage examples
├── assets/                   # vendored fonts, icons, templates
├── scripts/                  # dev scripts (dep-dag.rs validator, etc.)
├── third_party/              # vendored deps / forks
└── target/                   # build output (gitignored)
```

---

## 2. Root-level folder philosophy

| Folder | Tier / Purpose | PLAN.md cross-ref |
|---|---|---|
| `kernel/` | **Tier 1** — constitutional substrate; no Tier-2 deps allowed | §1.1, §10.1 |
| `crates/` | **Tier 2** — privileged plugins; same Plugin API as Tier 3 | §1.1, §10.2 |
| `plugins/` | **Tier 3** — sandboxed WASM plugin examples | §1.1, §10.3 |
| `runtime/` | executable targets; consume kernel + selected crates | §2.1 (target matrix) |
| `editor/` | editor host app (consumes editor-shell + editor-ui) | §6 |
| `golden-projects/` | canonical fixtures; CI runs on every major change | [`WAVES.md` W21](./WAVES.md), §13 |
| `tools/` | architecture enforcement, CI tooling, debug aids | [`IMPLEMENTATION.md` Phase 0.2](./IMPLEMENTATION.md) |
| `schemas/` | WIT files, reflection schema, format specs | §0.3.1, §1.6 |
| `docs/` | ADRs (001–111), companion design docs | §16, §18 |
| `tests/` | cross-workspace integration; per-subsystem in crate `tests/` | §13 quality gates |
| `third_party/` | vendored `egui_dock`, etc. — version-pinned, audited | §6.1, §1.12 |

**Why this top-level split:** the forbidden-dep DAG (§1.8) is enforced by where things live. `kernel/` cannot import from `crates/` (filesystem cousin = build error). `crates/cad-core/` cannot import from `crates/cad-projection/` (alphabetical neighbors = checked by lint). The structure makes violations physically obvious to reviewers.

---

## 3. Tier 1 — `kernel/` (constitutional substrate)

```text
kernel/
├── app/                  # main loop driver
├── ecs/                  # entity/component/system substrate
├── schedule/             # ordered system pass execution
├── asset/                # content-addressed asset loader
├── asset-view/           # zero-copy WASM linear-memory mapping
├── asset-streaming/      # residency manager (priority IO)
├── io-scheduler/         # priority IO queue
├── job-system/           # work-stealing thread pool, job graph
├── plugin-host/          # plugin lifecycle, dep resolution, manifest
├── diagnostics/          # unified miette/ariadne-style diagnostics
├── events/               # typed event bus
├── types/                # type registry, reflection bridge, UI hints
├── audit-ledger/         # recording / replay substrate
├── graph-foundation/     # graph primitives substrate (NodeId, EdgeId, hash, diff, snapshot, invalidation)
└── shared/               # truly cross-kernel utilities (kept minimal)
```

14 crates. Cross-references PLAN.md §10.1.

**`shared/` discipline** — this folder is suspect by default. Anything here must justify why it can't live in a specific kernel crate. CI lint flags new crates added to `shared/` for review. (Defends against the `utils.rs`-as-crate version of the no-utils-files rule.)

### Example kernel crate layout (using `kernel/ecs/` as canonical)

```text
kernel/ecs/
├── Cargo.toml
├── README.md             # what this crate is + Tier-1 contract
├── benches/              # criterion benchmarks
├── tests/                # integration tests
└── src/
    ├── lib.rs            # re-exports + module wiring only (no logic)
    ├── archetype.rs      # one major type per file
    ├── entity.rs
    ├── world.rs
    ├── query.rs
    ├── storage/          # specialized relation storage
    │   ├── mod.rs
    │   ├── tree.rs       # TreeRelationStorage
    │   ├── dense_linear.rs
    │   ├── dag.rs
    │   └── sparse.rs
    ├── relations/
    │   ├── mod.rs
    │   ├── parent_of.rs
    │   ├── bone_of.rs
    │   ├── lod_of.rs
    │   └── template_of.rs
    ├── change_detection/
    │   ├── mod.rs
    │   ├── tracker.rs
    │   └── observer.rs
    ├── scheduler_bridge/  # integration point with kernel/schedule
    └── internals/         # crate-private helpers
```

Same pattern for every kernel crate.

---

## 4. Tier 2 — `crates/` (privileged plugins, ~70 target)

Grouped by category. Each group separated by blank lines for readability:

```text
crates/
├── components/           # cross-crate ECS components (organized by domain)
├── resources/            # cross-crate ECS resources (singletons)
├── math/                 # math primitives (Vec3, Mat4, Quat — shared)
├── errors/               # cross-crate error types
│
├── gfx/                  # wgpu wrapper, render-graph compiler
├── gfx-ir/               # render IR types
├── brep-render/          # B-Rep tessellation → render path
│
├── cad-core/             # transactional graph + operators + persistent IDs + lineage + history
├── cad-projection/       # ECS view layer (split into 6 internal modules)
├── cad-native/            # truck-backed kernel adapter
├── cad-occt/             # OCCT-backed kernel adapter (opt-in for STEP/IGES)
│
├── material-graph/       # graph IR + WGSL codegen + naga validation
├── material-runtime/     # pipeline binding, parameter buffers
├── material-graph-editor/# egui node-graph widget for materials
│
├── anim-clip/            # clip data, sampling, looping
├── anim-graph/           # state machine + blend tree IR + runtime
├── anim-graph-editor/    # egui node-graph widget for animation
├── anim-ik/              # IK solvers (two-bone, CCD, FABRIK, look-at)
├── anim-retarget/        # skeleton retargeting (humanoid-only at v1.0)
├── anim-events/          # frame-keyed event system
│
├── physics/              # Rapier3D wrap
├── physics-debug/        # collider wireframes, joints, raycast traces (editor-only)
├── audio/                # Kira wrap
├── input/                # winit + gilrs fan-in
├── input-gestures/       # touch/stylus gesture recognition
│
├── runtime-wasmtime/     # cap-gate API (effect specifiers)
├── runtime-wasmtime-engine/  # actual wasmtime execution
├── script-host/          # ECS bridge + WIT bindings
├── script-graph/         # visual scripting graph → WASM
├── script-aot/           # cook-time AOT (wasmtime compile)
├── expr-wasm/            # inline-expression compiler (string → AST → WASM)
├── script-bench/         # benchmark suite
│
├── editor-shell/         # winit handler + lifecycle + PIE
├── editor-actions/       # Command Bus + UndoStack + audit projection
├── editor-state/         # selection / hover / active-tool / modal-state / drag-drop
├── editor-ui/            # theme/menus/widgets/layout/dock/workspace
│
├── ui-theme/             # token registry, themes, hot-reload
├── ui-icons/             # SVG icon registry, tinting
├── ui-fonts/             # cosmic-text wrap, family resolution
│
├── asset-store/          # content-addressed local cache
├── asset-pipeline/       # cook orchestrator
├── pak-format/           # .rge-pak writer/reader
├── rge-data/             # project / scene / prefab schemas + migrations
│
├── io-gltf/              # glTF 2.0 import/export
├── io-step/              # STEP/IGES via cad-occt
├── io-stl/               # STL import/export
├── io-obj/               # OBJ import/export
├── io-image/             # PNG/JPEG/EXR/HDR via image + exr
├── io-audio/             # WAV/OGG/FLAC/MP3 via Kira
│
├── marketplace/          # plugin manifest, signing, revocation client
├── marketplace-server/   # static registry generator
│
├── plugin-discovery/     # descriptor / registry / watcher (existing in rustforge)
├── hot-reload-watcher/   # notify-based file watcher
│
├── build-pipeline/       # multi-target cook orchestrator
├── replication/          # networking placeholder (stub at v1.0; impl Phase 5-Scale)
│
└── macros-reflect/       # #[rge::reflect] proc-macro
```

**Categorical alignment with PLAN.md §10.2:**

| Group | Crates | PLAN.md cross-ref |
|---|---|---|
| Reusable types | components, resources, math, errors | §1.3 (rule 1), §10.2 |
| Rendering | gfx, gfx-ir, brep-render | §8 |
| CAD core | cad-core, cad-projection, cad-native, cad-occt | §1.5.4 |
| Material | material-graph, material-runtime, material-graph-editor | §6.9 |
| Animation | anim-clip, anim-graph, anim-graph-editor, anim-ik, anim-retarget, anim-events | §6.11 |
| Physics/audio/input | physics, physics-debug, audio, input, input-gestures | §6.10, §3 |
| Scripting | runtime-wasmtime*, script-host, script-graph, script-aot, expr-wasm, script-bench | §5 |
| Editor | editor-shell, editor-actions, editor-state, editor-ui | §6, §1.15 |
| UI substrate | ui-theme, ui-icons, ui-fonts | §6.2 |
| Asset pipeline | asset-store, asset-pipeline, pak-format, rge-data | §1.6 |
| Importers/Exporters | io-* (one per format) | §1.6.4 |
| Marketplace | marketplace, marketplace-server | §9 |
| Plugin substrate | plugin-discovery, hot-reload-watcher | §1.1 |
| Build/cook | build-pipeline, runtime-platform-* (in `runtime/`) | §2 |
| Networking placeholder | replication (stub) | §6.17 |
| Reflection | macros-reflect | §6.15 |

---

## 5. cad-core internal structure

Per [PLAN.md §1.5.4](./PLAN.md):

```text
crates/cad-core/
├── Cargo.toml
├── README.md
├── benches/
├── tests/
├── examples/
└── src/
    ├── lib.rs                # re-exports
    ├── operators/            # Extrude, Revolve, Boolean, Fillet, Loft, Sweep, Shell
    │   ├── mod.rs
    │   ├── extrude.rs
    │   ├── revolve.rs
    │   ├── boolean.rs
    │   ├── fillet.rs
    │   └── ...
    ├── graph/                # operator DAG
    │   ├── mod.rs
    │   ├── node.rs
    │   ├── edge.rs
    │   └── traversal.rs
    ├── topology/             # TopoId, PersistentFaceId/EdgeId/VertexId
    ├── topo_lineage/         # TopologyEvolution + lineage edges (NEW v0.8)
    │   ├── mod.rs
    │   ├── evolution.rs      # enum TopologyEvolution
    │   ├── edge.rs           # LineageEdge
    │   ├── reconcile.rs      # rebuild ID remapping
    │   └── confidence.rs     # semantic continuity scoring
    ├── constraints/          # graph-level constraint solving
    ├── history/              # immutable graph snapshots, structural sharing
    ├── checkpoints/          # CadCheckpointId, transactional API
    │   ├── mod.rs
    │   ├── api.rs            # begin_operation / commit / rollback / restore_to
    │   └── store.rs          # in-memory + disk-backed retention
    ├── tessellation/         # tessellation cache keyed on (cad_node_id, tolerance, lod_bucket)
    ├── adapters/             # kernel adapters (truck, OCCT)
    │   ├── mod.rs
    │   ├── truck.rs          # capability surface
    │   ├── occt.rs           # capability surface
    │   └── capabilities.rs   # KernelCapabilities struct (per §1.5.4.4)
    ├── diagnostics/          # cad-core specific diagnostics
    ├── persistence/          # serde for cad-core graph state
    └── internals/
```

`topo_lineage/` is a module within cad-core (not a standalone crate, per §1.5.4.3).

---

## 6. cad-projection internal split (§1.5.4.5)

Split into 6 modules **inside one crate** to prevent god-bridge accumulation:

```text
crates/cad-projection/
├── Cargo.toml
└── src/
    ├── lib.rs                # public API; orchestration only
    ├── projection_structural/ # entity existence, hierarchy emission
    │   ├── mod.rs
    │   ├── entity_map.rs
    │   └── hierarchy.rs
    ├── projection_geometry/   # tessellation projection, bounds
    │   ├── mod.rs
    │   ├── tessellation.rs
    │   └── bounds.rs
    ├── projection_semantic/   # material slots, selection sets, layer membership
    ├── projection_runtime/    # collision proxies, visibility filters, render queue feeders
    ├── projection_editor/     # gizmos, picking handles, debug overlays (editor-only)
    └── projection_cache/      # memoization, invalidation tracking, dirty bits
```

CI rule (per §1.8): `projection_structural` cannot import `projection_runtime` or `projection_editor`. Each module has documented "what triggers me" / "what I emit."

---

## 7. editor-state structure (§1.15)

Five modules — fixed scope at v0.8 per architecture freeze:

```text
crates/editor-state/
├── Cargo.toml
└── src/
    ├── lib.rs                # public API; coordination only
    ├── selection.rs          # entity sets, component sets, face/edge/vertex sets
    ├── hover.rs              # per-panel hover state with stable IDs
    ├── active_tool.rs        # current tool per viewport, tool stack
    ├── modal_state.rs        # drag-in-progress, brush-down, dial-input
    └── drag_drop.rs          # in-progress drag/drop transactions across panels
```

Adding a 6th module requires ADR + §0.6 freeze-policy gate.

---

## 8. editor-ui internal layout

```text
crates/editor-ui/
├── Cargo.toml
├── menus/                # MenuRegistry (UE5 UToolMenus pattern)
│   └── src/{lib.rs, registry.rs, entry.rs, extension_point.rs, order_hint.rs, shortcut.rs, command.rs}
├── widgets/              # one file per widget (Rule 3)
│   └── src/{button, icon_button, toggle, slider, number_input, color_picker, file_picker, tabs,
│           tooltip, context_menu, toast, popover, resize_handle, property_grid, tree_view,
│           search_box, node_graph}.rs
├── layout/               # Workspace RON loader/saver
│   └── src/{lib.rs, workspace.rs, node.rs, io.rs, reconcile.rs}
│   └── assets/defaults/{default,animation,sculpt,code}-workspace.ron
├── dock/                 # TabManager + LayoutService + SpawnerRegistry on egui_dock
│   └── src/{lib.rs, tab_manager.rs, layout_service.rs, spawner_registry.rs, version.rs}
└── workspace/            # workspace switcher, store
    └── src/{lib.rs, switcher.rs, store.rs}
```

---

## 9. `runtime/` — executable app targets

```text
runtime/
├── runtime-desktop/      # Win/macOS/Linux native binary
├── runtime-mobile/       # iOS/Android (4-Polish)
├── runtime-web/          # wasm-bindgen + WebGPU (4-Polish)
└── runtime-headless/     # cook tool, dedicated server (no gfx, no editor)
```

Each is a binary crate consuming the appropriate subset of kernel + crates. Per `cfg(target_os)` the platform-specific glue lives in respective `runtime-platform-*` crates (under `crates/`, not `runtime/` — those are libraries).

PLAN.md cross-ref: §2.1 target matrix.

---

## 10. `editor/` — editor host app

```text
editor/
└── rge-editor/
    ├── Cargo.toml
    └── src/
        ├── main.rs               # entry point
        ├── bootstrap.rs          # workspace + plugin discovery
        ├── workspace_boot.rs     # load default workspace, restore last
        ├── diagnostics_boot.rs   # wire kernel/diagnostics → console panel
        └── plugin_boot.rs        # load Tier-2 + Tier-3 plugins
```

The app is small. Logic lives in `editor-shell` + `editor-ui` + `editor-actions` + `editor-state`. The app composes them.

---

## 11. `golden-projects/` — regression validation fixtures

```text
golden-projects/
├── simple-scene/         # basic load, transform, camera + light
├── material-zoo/         # 10+ materials covering PBR/unlit/skinned/blend-shape/B-Rep
├── skinned-character/    # glTF, skeleton, animation, skinning
├── physics-puzzle/       # rigid bodies, joints, triggers, deterministic replay
├── cad-parametric/       # B-Rep edits, lineage, projection invalidation
└── stress-world/         # 50k+ entities, scene-streaming, perf regression detection
```

`cad-parametric` and `stress-world` are NEW additions over WAVES.md W21 — they cover Phase 7 (CAD validation) and Phase 9 (production pressure) golden tests.

CI runs all six on every major change. Exit criteria: load, run 60 ticks, screenshot match within tolerance, byte-identical cook output.

---

## 12. `tools/` — architecture enforcement (Phase 0.2 deliverable)

```text
tools/
├── architecture-lints/       # forbidden-dep DAG validator, line-cap, no-utils, ownership rules
│   ├── Cargo.toml
│   └── src/{forbidden_dep, split_exemption, no_utils, graph_foundation, editor_state_ownership,
│            command_bus, projection_modules, kernel_isolation}.rs
├── dependency-auditor/       # cargo-deny + cargo-udeps + custom rules wrapper
├── graph-metrics/            # entropy metrics tracker (§1.10.4)
│   └── tracks: cross-crate dep edges, archetype count, invalidation fanout,
│              hot-reload migration LOC, public API surface, etc.
├── invalidation-profiler/    # cad-projection invalidation density measurement
├── snapshot-debugger/        # PIE snapshot diff visualizer
├── schema-diff/              # rge-data schema migration verifier
├── wasm-bench/               # script-bench harness
├── topology-debugger/        # cad-topo lineage visualizer
└── ci/                       # CI workflow definitions, regression fixtures
    └── workflows/{lint, build, test, architecture, perf}.yml
```

Each tool is a binary crate. Run as part of CI gates. Most are written first (Phase 0.2) — they enforce the architecture *before* engine code lands.

PLAN.md cross-ref: §13 quality gates, §1.8 dep governance, §1.10.4 entropy.

---

## 13. `tests/` — cross-workspace integration

```text
tests/
├── integration/      # multi-subsystem integration tests
├── determinism/      # Replay-Stable verification on golden projects
├── hot_reload/       # WASM hot-reload swap correctness
├── snapshots/        # PIE snapshot/restore round-trip
├── topology/         # cad-topo lineage stability tests (1000+ random edits)
├── stress/           # 100k entity scenes, large-scene scrolling
├── perf/             # benchmarks vs targets (§13.1, §13.2)
└── migration/        # project + player-state migration tests
```

Per-crate unit tests live in each crate's `tests/` directory. Cross-crate / cross-subsystem tests live here.

---

## 14. `schemas/` — WIT, reflection, validation

```text
schemas/
├── wit/              # vendored WASI Component Model + custom interfaces
│   ├── upstream/     # WASI Preview 2 spec snapshot
│   ├── rge-game.wit  # game-script WIT world
│   ├── rge-ecs/      # ecs/query, ecs/observer, ecs/events
│   ├── rge-asset/    # asset/view (zero-copy buffers)
│   └── rge-net/      # networking (Phase 5-Scale, reserved)
├── reflection/       # JSON-schema for reflected types (consumed by editor inspector)
├── formats/          # .rge-pak header schema, .rge-scene schema
└── ci-rules/         # schema validation rules consumed by tools/architecture-lints
```

WIT files are the Tier-3 ABI contract (per §1.1). Pinned to wasmtime LTS; bumped deliberately, never on minor.

---

## 15. `docs/` — ADRs + architectural docs

```text
docs/
├── adr/                  # Architecture Decision Records (one file per ADR)
│   ├── 001-pillars.md
│   ├── 077-runtime-escape-clause.md
│   ├── 089-cad-core-split.md
│   ├── 097-cad-projection-internal-split.md
│   ├── 098-topology-lineage.md
│   ├── 099-execution-domains.md
│   ├── 100-editor-extends-runtime.md
│   ├── 101-graph-foundation.md
│   ├── 102-failure-containment.md
│   ├── 103-authoritative-cad-serialization.md
│   ├── 104-cad-kernel-non-equivalence.md
│   ├── 105-wasm-lock-in-top-risk.md
│   ├── 110-editor-state-narrow.md
│   └── 111-architecture-freeze.md
├── architecture/         # PLAN.md, IMPLEMENTATION.md, WAVES.md, fileandfolderstructure.md
├── cad/                  # CAD_CORE_MODEL, CAD_TOPOLOGY_LINEAGE, CAD_KERNEL_CAPABILITIES, CAD_DETERMINISM
├── scripting/            # SCRIPT_BENCH_METHODOLOGY, EXECUTION_DOMAINS, WASM_TOOLING_FALLBACKS
├── rendering/            # RENDERER_MODEL, STREAMING_MODEL
├── editor/               # SCENE_MODEL, PIE_MODEL, UI_LAYOUT_SCHEMA, UNDO_REDO_MODEL, EDITOR_STATE_MODEL
├── networking/           # NETWORKING_PLAN
├── governance/           # MARKETPLACE_GOVERNANCE, PLUGIN_API, CONVENTIONS
├── risks/                # RECOVERY_MODEL, EGUI_PRESSURE
└── benchmarks/           # SCRIPT_BENCH baselines, perf regression tracking
```

ADRs are append-only; superseding is allowed but old ADRs stay as historical record.

---

## 16. `plugins/` — Tier 3 sandbox examples

```text
plugins/
├── examples/         # canonical example plugins (USD importer, Lua-as-script, etc.)
├── marketplace/      # plugins shipped with engine bundle
├── experimental/     # works-in-progress, non-shipping
└── internal/         # team-only plugins (CI helpers, internal tools)
```

Each plugin is a separate WASM compilation target. Per §9 marketplace governance.

---

## 17. `third_party/` — vendored dependencies

```text
third_party/
├── egui_dock/        # vendored per §6.6 (deliberate bumps only)
├── wasmtime/         # LTS pin per §1.4 escape clause discipline
├── rapier3d/         # pinned for determinism (§1.6.8)
├── cosmic-text/      # font shaping
└── README.md         # what's vendored, why, version policy
```

CI verifies vendored versions match the lockfile. Bumping any of these is a deliberate decision (PR + ADR if architectural impact).

---

## 18. `crates/components/` — components doctrine

```text
crates/components/
└── src/
    ├── lib.rs
    ├── spatial/          # Transform, Parent, ChildOf, GlobalTransform
    ├── identity/         # Name, EntityId helpers, AssetRef, CadRef
    ├── visibility/       # Visibility, Hidden, Disabled, Highlight
    ├── lifecycle/        # Spawn, Despawn markers, age tracking
    ├── interaction/      # Trigger, Sensor (collider markers)
    ├── render/           # MeshHandle, MaterialHandle, Camera, Light, BRepHandle, ReflectionProbe
    ├── physics/          # RigidBody, Collider, Velocity, AngularVelocity, Joint, Mass, CharacterController
    ├── animation/        # Skeleton, BoneTransforms, AnimationPlayer, AnimationGraphInstance, IKChain
    ├── audio/            # AudioSource, AudioListener, AudioFalloff
    ├── networking/       # Replicated, NetworkOwner, Authoritative, RemotePeer (markers, stub at v1.0)
    └── editor/           # editor-only markers stripped on cook (EditorOnlyRoot, etc.)
```

Components in `crates/components/` must be:
- **Semantically stable** — schema versioned; minor bumps additive only
- **Low-policy** — no orchestration logic; just data + small inherent operations
- **Authority-neutral** — components don't own behavior; systems do
- **Orchestration-free** — no scheduling decisions in component code

**Components define:** state · handles · metadata · contracts.
**Systems define:** behavior · orchestration · mutation policy · scheduling.

This separation prevents components from accumulating into god-types. PLAN.md cross-ref: §1.3 rule 1 (promotion rule); §1.5.1 (entity roles).

---

## 19. `examples/` — standalone usage examples

```text
examples/
├── hello-world/          # minimal RGE app: window + clear screen
├── spinning-cube/        # one entity, rotated by gameplay system
├── load-gltf/            # glTF import, render
├── hot-reload-demo/      # edit gameplay system, see change live
├── pie-demo/             # Play/Stop snapshot/restore
├── parametric-cube/      # B-Rep entity, edit operators in real-time
└── material-graph-demo/  # node-graph editor with PBR output
```

Examples are tiny binary crates. Used for documentation, onboarding, and as smoke tests.

---

## 20. `assets/` — vendored fonts, icons, templates

```text
assets/
├── fonts/                # Inter, JetBrainsMono (vendored per ui-fonts)
├── icons/                # Lucide, Tabler, Material-Symbols (vendored per ui-icons)
├── themes/               # dark-default, light-default, studio-pro, daylight (per ui-theme)
└── templates/            # project templates: empty, 2D-platformer, FPS-starter, CAD-tool, etc.
```

`assets/` is workspace-shared. Per-crate assets (test fixtures, examples) live in respective `tests/fixtures/` or `examples/<name>/assets/`.

---

## 21. `scripts/` — dev scripts

```text
scripts/
├── dep-dag.rs            # forbidden-dep DAG validator (Phase 0.2 deliverable)
├── build-budget.rs       # monomorphization + binary-size budget tracker
├── golden-cook.sh        # cook all golden projects, compare byte-for-byte
├── hot-reload-stress.rs  # stress test for WASM hot-reload swap correctness
└── snapshot-diff.rs      # diff two PIE snapshots for debugging
```

Scripts are run by CI and developers. Many are referenced from `tools/` workflows.

---

## 22. How the structure enforces architecture

Below: each constitutional rule from PLAN.md mapped to how the folder structure makes it physically obvious / enforceable:

| Constitutional rule | How structure enforces |
|---|---|
| Tier 1 cannot depend on Tier 2 (§1.8) | `kernel/` and `crates/` are sibling folders; CI lint blocks any `kernel/*/Cargo.toml` from referencing `crates/*` |
| Tier 2 cannot depend on Tier 3 (§1.8) | `crates/` and `plugins/` separate; same lint |
| `editor-ui/*` cannot depend on `physics`/`audio`/`input` (§1.8) | Forbidden-dep DAG in `tools/architecture-lints/forbidden_dep.rs` |
| `cad-core` stands alone (§1.8) | `crates/cad-core/Cargo.toml` cannot list any other Tier-2 crate as dep |
| Only `cad-projection` may import cad-core types into ECS code (§1.8) | Lint scans `crates/*/src/**/*.rs` for `use rge_cad_core::*` outside `cad-projection` |
| `editor-state` cannot own authoritative content (§1.15) | Lint forbids component bodies / cad-core nodes / asset payloads being imported into `crates/editor-state/src/**/*.rs` |
| Subsystems may not invent own `Selection`/`Hover`/`ActiveTool` (§1.15) | Lint scans Tier-2 crates for `pub struct Selection` outside `editor-state` |
| All editor mutations through Command Bus (§6.16) | Lint forbids direct world mutation imports outside `crates/editor-actions/` |
| One importer per format (§1.6.4) | One `crates/io-<format>/` per supported format; CI lint flags duplicate-format crates |
| Graph systems use graph-foundation primitives (§1.14) | Lint forbids redefining `NodeId`/`EdgeId`/`StableHash` outside `kernel/graph-foundation/` |
| No `utils.rs` / `helpers.rs` (§1.3) | Filename lint |
| No `.rs` >1000 lines without exemption (§1.3) | Line-count lint with `// SPLIT-EXEMPTION` annotation acknowledgment |
| New first-class subsystem requires §0.6 gate | Lint flags new top-level folders under `kernel/` or new categorical groups in `crates/` for review |

**The folder structure encodes the architecture.** Violating the structure should require deliberately editing CI lints, not just moving files.

---

## 23. Phase 0.1 deliverable mapping

[`IMPLEMENTATION.md` Phase 0.1](./IMPLEMENTATION.md) (Week 1) ships exactly this structure:

- All 14 `kernel/*/` crates with stub `Cargo.toml` + `src/lib.rs`
- All ~70 `crates/*/` crates with stub `Cargo.toml` + `src/lib.rs`
- `runtime/runtime-{desktop,mobile,web,headless}/` stubs
- `editor/rge-editor/` stub binary crate
- `tools/architecture-lints/` with the 13 lint binaries (active and passing on stubs)
- `golden-projects/{simple-scene,material-zoo,skinned-character,physics-puzzle,cad-parametric,stress-world}/` with empty-but-valid project schemas
- `schemas/wit/` vendored from WASI Preview 2 + `rge-game.wit` placeholder
- `docs/adr/001-pillars.md` through `docs/adr/111-architecture-freeze.md` (existing decisions formalized)
- Workspace `Cargo.toml` listing all members
- `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `taplo.toml`
- `.github/workflows/{lint,build,test,architecture}.yml`

Phase 0.1 exit criterion: `cargo build --workspace` succeeds (all stubs compile); all CI lints active and passing on the stub workspace.

---

## 24. Structural philosophy (meta-principle)

The structure optimizes for:

| Goal | Meaning |
|---|---|
| **Architectural enforcement** | Illegal dependencies are physically difficult to write |
| **Compile isolation** | Low invalidation radius (touching one type doesn't recompile half the workspace) |
| **Subsystem discoverability** | Predictable layout — anyone can find any subsystem in <30 seconds |
| **Future extraction** | Crates separable into independent repos if needed |
| **Observability** | Diagnostics live near the systems they describe |
| **Bounded growth** | No god-module accumulating cross-cutting concerns |
| **Implementation velocity** | Easy navigation reduces friction |

> **The folder structure should encode the architecture, not merely store files.**

If a contributor can predict where a piece of code lives based purely on what it does, the structure is working. If they have to grep, the structure has accumulated implicit policy that should be made explicit (or restructured).

This is the deliverable that turns the v0.8 frozen architecture into something a team can actually build in.
