# RGE — Game Editor Plan (v0.8 — architecture freeze)

> **Status:** Plan-of-record and **architecture freeze point**. Supersedes v0.7 after fifth external review pass. v0.8 is intended as the final architecture revision before implementation; further structural changes require demonstrated implementation pressure (§0.6).
>
> **Drafted:** 2026-05-04, end of fifth architecture review thread.
>
> **Relationship to master plan:** [`../RGE_MASTER_PLAN.md`](../RGE_MASTER_PLAN.md) is canon for pivot, license, B-Rep positioning, RCAD disposition, rival map, vendor-borrow list. This document is canon for everything else.

---

## 0. The four pillars and the moat

### 0.1 Four pillars

1. **Native, multi-platform** — Win/macOS/Linux Tier-0; iOS/Android/web Tier-1; consoles post-v1.
2. **Multi-device** — mouse+kbd/gamepad/touch/stylus/OpenXR through one input stack.
3. **Realtime industrial-quality editor rendering** (REVISED v0.7) — Bevy/Godot-tier at v1.0; competitive with mid-tier engines on selective features at Phase 5; photoreal parity is post-v2.
4. **Fastest script engine** — WASM-based, AOT-cooked, near-native runtime, published benchmarks.

### 0.2 The moat

> **Unified CAD-native deterministic WASM-scripted authoring environment.**

What makes RGE structurally hard to copy in <2 years:
- **CAD-native** — B-Rep first-class via dedicated transactional graph core (`cad-core`, §1.5.4) with persistent topological identity backed by lineage graph (§1.5.4.3). UE/Unity/Godot/Bevy are mesh-first.
- **Deterministic (gameplay)** — Replay-Stable mode scoped to gameplay only (§1.6.8). Honest narrowing.
- **Unified** — one runtime per execution domain (§0.3.1), one source format family, one hot-reload path per domain, no siblings.
- **WASM-scripted** — capability-gated, AOT-cooked, native-speed.

If Phase 5-Rendering slips, the moat holds.

### 0.3 Engine constitution — eight immutable principles

1. **One runtime per execution domain.** All executable user content runs through the canonical runtime for its domain (§0.3.1). No siblings within a domain.
2. **One source format family.** RON for game data; TOML for plugin manifests.
3. **One hot-reload path per domain.** WASM swap for CPU gameplay; shader recompile for GPU; expr-wasm cache invalidate for inline expressions.
4. **Plugin-first.** Tier-2 privileged plugins use the same public API as Tier-3; CI dogfood test enforces.
5. **Determinism before optimization.** Reproducible builds + content-addressed assets + replay-stable gameplay > raw perf when they conflict (scoped per §1.6.8).
6. **Data-oriented over inheritance.** ECS components, relations, content hashes. No class hierarchies.
7. **Anti-pattern audit (the Rhai-test) on every PR.** Every architectural change must answer: "what unified system does this overlap with?"
8. **Editor extends runtime, never replaces** (NEW v0.7). Editor systems may add capabilities (selection, gizmos, transactional batching, async previews, retained UI state) on top of runtime semantics, but never duplicate or override them. Editor mutations flow through the Command Bus (§6.16) into runtime; never bypass.

**Constitutional escape clause** (ADR-077): four documented conditions allow alternative execution. Currently none active. See §1.4.

### 0.3.1 Execution domains (NEW v0.7)

Different physical execution targets need different runtimes; that's not a sibling-system problem if the boundary is clear. Four canonical domains:

| Domain | Runtime | Reflection | Capability | Hot-reload |
|---|---|---|---|---|
| **CPU gameplay** | wasmtime | shared schema via `#[rge::reflect]` | `runtime-wasmtime` cap-gate | WASM module swap |
| **GPU shading** | WGSL via wgpu | shared schema (material params) | wgpu pipeline state | shader recompile + PSO swap |
| **GPU compute** | WGSL compute via wgpu | shared schema | wgpu compute pipeline | shader recompile + dispatch swap |
| **Expression microcode** | expr-wasm (single-expr WASM) | shared schema | inline whitelist | recompile + cache invalidate |

**Shared across domains** (no duplication):
- Reflection schema (`kernel/types`)
- Capability model (one taxonomy, enforced per-domain)
- Diagnostic spans (`kernel/diagnostics`)
- Hot-reload orchestration (`hot-reload-watcher` triggers per-domain handlers)

**Not shared** (correctly distinct):
- The execution backends (wasmtime ≠ wgpu)
- Hot-reload mechanics per domain
- Profiler integrations

This naming prevents "GPU scripting becoming an accidental sibling ecosystem" — when GPU compute scripting demand surfaces, it slots into the existing domain. New domains (XR shaders, neural compute) require formal review and an ADR.

ADR-099. Companion: `RGE/EXECUTION_DOMAINS.md`.

### 0.4 Floor vs reach product

#### Floor product (must ship at v1.0)

| Subsystem | Floor capability |
|---|---|
| Renderer | Bevy/Godot-tier wgpu forward+. PBR. Cascaded shadows. **Industrial-quality realtime editor**, not photoreal. |
| B-Rep | First-class via cad-core. Persistent topology IDs via lineage graph. Adaptive tessellation. Triangle fallback always available. |
| Scripting | Rust→WASM, visual graph→WASM, expr-wasm. Hot-reload <100ms p95. Cook to `.cwasm`. |
| Editor | Theme/menu/widgets/layout/dock. PIE. Reflection inspector. Undo/redo via Command Bus (two-layer). |
| Determinism | Replay-Stable for gameplay only. |
| Platforms | Win/macOS/Linux Tier-0. |
| Authoring | Material editor (PBR + instances). Animation graph. Skinning (LBS). Physics (rigid bodies + character controller). |
| File formats | RON source + `.rge-pak` cooked + glTF/STEP/PNG/WAV import. |
| Marketplace | Plugin signing + revocation + cap-gated WASM. |
| Accessibility | WCAG-AA contrast lint, reduced-motion, dark+light variants. |
| **Recovery** (NEW v0.7) | Failure containment model active (§1.13). |

#### Reach product (desirable; can slip)

| Subsystem | Reach capability |
|---|---|
| Renderer | Lumen-equivalent / VSM / TSR (selective features competitive with mid-tier engines). Photoreal parity = post-v2. |
| Determinism | Lockstep-Stable cross-machine same-arch. CAD-output determinism. |
| Platforms | iOS/Android/web Tier-1 preview. |
| Authoring | DQS skinning. Compute-shader skinning. Free-form retargeting. |
| Networking | Authoritative CAD serialization (§6.17). Replication. |
| Tooling | Script debugger, profiler, source maps, panic recovery. |
| Languages | AssemblyScript lane (demand-gated). |
| Editor sophistication | Multi-monitor workspaces. Sub-graph composition. Full theme editor. |

#### Cut-priority order if Phase 4 slips

1. Reach renderer features
2. Tier-1 platform previews
3. Editor sophistication
4. AssemblyScript (already deferred)
5. DQS / compute skinning
6. Free-form retargeting
7. Script debugger (post-v1)
8. Networking scaffolding (keep markers; defer impl)

Floor items are not negotiable.

### 0.5 Review cadence as discipline

Mandatory external review at every minor version bump. Five rounds caught: ECS storage, B-Rep topology IDs, async streaming, undo/redo, render snapshot staging, ECS-scene-graph honesty, CAD/ECS impedance (v0.6 cad-core split), graph-domain fragmentation (v0.7 graph-foundation), execution-domain naming, failure containment, cad-projection bridge accumulation, **editor-state coordination** (v0.8 — narrow scope, accepted via discipline rather than full-subsystem promotion).

### 0.6 Architecture freeze policy (NEW v0.8)

After v0.8, the architecture is **frozen for the duration of Phase 4-Foundation**. Further first-class subsystem additions require all four:

1. **Demonstrated implementation pressure** — at least one failure case observed in code, not forecast from analogy with other tools
2. **3+ concrete failure scenarios** documented with reproducible examples
3. **Cost/benefit analysis** vs alternative non-architectural fixes (Resource, module, trait, CI rule)
4. **Justification why a smaller primitive wouldn't suffice** — the bar for promotion-to-first-class rises

**Why:** planning ROI is diminishing. The plan is ~1300 lines of architecture; implementation is 0 lines. Real pressure (profiler traces, invalidation bugs, hot-reload failures, topology corruption, undo edge cases, GPU residency failures, UI interaction friction) reveals false abstractions, missing abstractions, and invalid assumptions in ways no amount of paper architecture can predict.

**The transition observation:** v0.6 (cad-core) and v0.7 (graph-foundation) were responses to *demonstrated* architectural collisions already specified in the plan. v0.8 (editor-state) is the first round responding to a *forecasted* failure mode without implementation evidence. That's the line — past v0.8, only demonstrated pressure justifies new subsystems.

**Architecture serves implementation; implementation does not serve architecture.**

This is a constitutional commitment, not a policy preference. ADR-111.

---

## 1. Architecture commitments

### 1.1 Three-tier kernel + plugin model

**Tier 1 — Kernel (~14 crates, statically linked, source-level stable API):**
`kernel/ecs` · `kernel/schedule` · `kernel/asset` · `kernel/asset-view` · `kernel/asset-streaming` · `kernel/io-scheduler` · `kernel/job-system` · `kernel/plugin-host` · `kernel/types` · `kernel/events` · `kernel/audit-ledger` · `kernel/diagnostics` · `kernel/graph-foundation` (NEW v0.7) · `kernel/app`.

**Tier 2 — Privileged plugins (~70 crates, statically linked default bundle):**
Use the *same* public Plugin API as Tier 3.

**Tier 3 — Sandboxed plugins (WASM, capability-gated, WIT-typed):**
Through `runtime-wasmtime-engine`, gated by `runtime-wasmtime` cap tickets.

**ABI clarification:** Tier 1+2 = source-level Rust API stability. Tier 3 = WIT-typed.

**Dogfood rule:** the public Plugin API is acceptable only if `gfx`, `physics`, `editor-ui`, `cad-projection` can be expressed through it.

### 1.2 ECS substrate

#### 1.2.1 What ECS is, and is not

ECS is the **runtime representation** of the world — data layout for systems that iterate, substrate for entity composition, runtime relations.

ECS is **not** the authoritative store for CAD-graph state (`cad-core`), the transactional/rollback store (`cad-core` + `editor-actions::ActionHistory` via Command Bus), or a general graph engine.

#### 1.2.2 Relations and storage

| Relation | Storage | Purpose |
|---|---|---|
| `parent_of` | `TreeRelationStorage` | scene tree, transform propagation |
| `bone_of` | `DenseLinearRelationStorage` | skeletal hierarchy |
| `lod_of` | `SparseRelationStorage` | LOD groups |
| `template_of` | `SparseRelationStorage` | prefab → instance link |

#### 1.2.3 Change detection (`Changed<T>`)

Per-archetype mutation generation counters. Scripts subscribe via WIT `rge:ecs/observer`.

#### 1.2.4 Zero-copy asset views

`kernel/asset-view` exposes read-only slices of GPU-ready buffers (mesh vertices, texel data, cad-core tessellation output) directly to WASM linear memory.

### 1.3 Code organization rules

**Rule 1 — Reusable components in `components/`:** per-crate `src/components/<thing>.rs`; top-level `crates/components/` for cross-crate. Promotion: imported by 2+ crates → moves up.

**Rule 2 — Steal from rustforge, change for purpose:** grep first; copy with attribution.

**Rule 3 — Every `.rs` split to minimum, with exemption:** one major type per file; soft 300, lint-warn 600, hard 1000 with `// SPLIT-EXEMPTION: <reason>`. No `utils.rs` / `helpers.rs`.

### 1.4 Anti-pattern audit (the Rhai-test)

| Mistake caught | Status | Replacement |
|---|---|---|
| Rhai sibling interpreter | reverted | `expr-wasm` |
| `live-coding` libloading | dropped | WASM hot-reload only |
| `script-aot-llvm` sibling backend | dropped | Cranelift only |
| AssemblyScript at v1.0 launch | deferred | demand-gated |
| Stack-bytecode evaluator | rejected | `expr-wasm` |
| Two parallel scene formats | not attempted | layered |
| CAD-as-ECS-substrate (v0.6) | fixed | cad-core + cad-projection split |
| **Graph-domain fragmentation (v0.7)** | **fixed** | **`kernel/graph-foundation` substrate** |

**Constitutional escape clause** (ADR-077): four conditions trigger formal review. None active.

**Pending audit triggers — apply on every PR:**
- New runtime / interpreter / VM / evaluator
- New hot-reload path
- New ABI bridge or serialization format
- "Stretch alternative backend" idea
- "Convenience" feature overlapping existing capability
- New authoritative store paralleling cad-core or ECS
- New UI subsystem paralleling egui
- **New graph type that reinvents node/edge/diff/snapshot primitives** (NEW v0.7) — must build on `kernel/graph-foundation`
- **Any state container that mixes authoritative content with coordination context** (NEW v0.8) — must split per §1.15
- **Any new first-class subsystem proposal** (NEW v0.8) — must clear §0.6 freeze-policy gate (4 conditions)

### 1.5 Scene / world model

#### 1.5.1 Canonical entity roles

| Role | Required components | Optional |
|---|---|---|
| Mesh entity | `Transform` · `MeshHandle` · `MaterialHandle` · `Visibility` · `Name` | `SkinnedMesh`, `LOD`, `Highlight` |
| **B-Rep entity** | `Transform` · `BRepHandle(CadRef)` · `MaterialHandle` · `Visibility` · `Name` | `LOD` |
| Camera | `Transform` · `Camera` · `Name` | `PostProcessStack`, `AudioListener` |
| Light (Directional) | `Transform` · `Light::Directional` · `Name` | `ShadowMap` |
| Light (Point/Spot) | `Transform` · `Light::Point\|Spot` · `Name` | `ShadowMap` |
| Audio source | `Transform` · `AudioSource` · `Name` | `AudioFalloff` |
| Particle emitter | `Transform` · `Particle` · `Name` | `EmitterParameters` |
| Trigger volume | `Transform` · `Collider` · `Trigger` · `Name` | `TriggerHandler` |
| Reflection probe | `Transform` · `ReflectionProbe` · `Name` | |
| Skeleton | `Transform` · `Skeleton` · `BoneTransforms` · `Name` | `bone_of` children |

Roots: `SceneRoot` · `EditorOnlyRoot`.

#### 1.5.2 Render-side snapshot staging

Standard render-thread / sim-thread separation with double-buffered scene state. Render thread sees immutable snapshot of `(ECS_tick_N, CadCheckpointId_N)` while sim builds N+1.

#### 1.5.3 Roots & non-hierarchy items

Not in hierarchy: ECS resources, assets (referenced by handle), GPU pipeline state, editor dock layout, plugin instances, **cad-core graph state** (referenced by handle).

#### 1.5.4 CAD transactional core

CAD has its own transactional graph store, not embedded in ECS components. Resolves the impedance mismatch (CAD = transaction-heavy / identity-remapping; ECS = iteration-centric / cache-linear).

##### 1.5.4.1 Architecture

```
cad-core (authoritative)
├── operator graph (DAG, built on graph-foundation primitives)
├── persistent IDs (cad-topo module)
├── topology lineage graph (cad-topo-lineage — NEW v0.7)
├── constraints
├── history (immutable graph snapshots, structural sharing)
├── tessellation cache (keyed on (cad_node_id, tolerance, lod_bucket))
├── kernel adapters (truck primary; OCCT opt-in; capability-aware — §1.5.4.4)
└── transactional API: begin_operation/commit/rollback/restore_to
        ↓ (snapshot projection)
cad-projection (split internally per §1.5.4.5)
        ↓
ECS (runtime view)
```

##### 1.5.4.2 Persistent topological identity

| Type | Stable across | Generation |
|---|---|---|
| `TopoId` | session lifetime | spawn order in operator graph |
| `PersistentFaceId` | history rebuilds, save/load | content hash + lineage path |
| `PersistentEdgeId` | same | same |
| `PersistentVertexId` | same | same |

##### 1.5.4.3 Topology lineage graph (NEW v0.7)

Hash-based persistent identity is unstable under boolean reorder, tolerance healing, edge merge/split, kernel switching. The fix: explicit lineage tracking, not just remapping.

**Module:** `cad-core::topo-lineage` (internal to cad-core, not a separate crate).

**Core type:**

```rust
enum TopologyEvolution {
    Preserved,                              // identity unchanged
    Split(Vec<PersistentFaceId>),           // one face → multiple
    Merged(Vec<PersistentFaceId>),          // multiple → one
    Deleted,                                // gone
    Reinterpreted,                          // semantic continuity but identity unclear
}

struct LineageEdge {
    from: PersistentFaceId,                 // (or PersistentEdgeId/VertexId)
    evolution: TopologyEvolution,
    operator: OperatorId,
    confidence: f32,                        // 0.0–1.0 — heuristic confidence
    semantic_continuity: SemanticScore,     // does the resulting topology mean the same thing?
}
```

**Why this matters:**
- **Constraints:** if a fillet edge survives via Split into two, both inherit the constraint with `confidence` annotation; user resolves on conflict
- **Replication:** authoritative-server CAD (§6.17) sends operations + lineage diffs; clients reconcile via lineage walk
- **Diffing:** graph-diff between two cad-core checkpoints uses lineage to align identities, not just hashes
- **History UI:** "this fillet came from face X which was the result of boolean Y…" — explicit, browsable
- **Undo visualization:** undo timeline shows `Split` / `Merged` / `Reinterpreted` events for review before commit
- **Collaborative editing:** lineage gives meaningful conflict markers ("you split this face; they merged it back")

Without lineage, the persistent-ID story is "we'll remap and hope." With it, identity becomes traceable.

ADR-098. Companion: `RGE/CAD_TOPOLOGY_LINEAGE.md` (Phase 4-Geometry deliverable).

##### 1.5.4.4 CAD kernel non-equivalence doctrine (NEW v0.7)

truck and OCCT are NOT semantically equivalent. Differences include: boolean robustness, healing behavior, tolerance models, NURBS handling, sewing, edge orientation, shell validity, STEP fidelity.

Pretending they're interchangeable creates "portable abstraction layer syndrome" — lowest-common-denominator API.

**Doctrine:**
- `cad-core` owns the **semantic model** (operators, persistent IDs, history, lineage).
- Kernel adapters expose **capabilities** (which operators each kernel supports, with which guarantees).
- Non-portable operations are explicitly surfaced (e.g., "STEP-fidelity sewing is OCCT-only").
- Choice of kernel per project is a project setting; cad-core orchestrates which kernel handles which operation.

**Capability surface (sketch):**

```rust
struct KernelCapabilities {
    boolean_robust_under_tolerance: bool,
    healing_strategies: HashSet<HealingStrategy>,
    nurbs_eval: NurbsEvalQuality,
    step_round_trip_fidelity: StepFidelity,
    deterministic_triangulation: bool,
}
```

Non-portable ops surface as compile-time errors when the project's kernel choice doesn't support them; user must either change kernel or rewrite the operation.

ADR-104. Companion: `RGE/CAD_KERNEL_CAPABILITIES.md` (Phase 4-Geometry deliverable).

##### 1.5.4.5 cad-projection internal split (NEW v0.7)

cad-projection was on a path to becoming a "god bridge" — bridges accumulate hidden policy, silently become orchestration engines, and become impossible to refactor late.

**Internal split** (one crate, six modules — refactor to crates if growth demands):

```
crates/cad-projection/src/
├── lib.rs                       (re-exports, projection orchestration)
├── projection_structural/       (entity existence, hierarchy emission)
├── projection_geometry/         (tessellation projection, bounds)
├── projection_semantic/         (material slots, selection sets, layer membership)
├── projection_runtime/          (collision proxies, visibility filters, render queue feeders)
├── projection_editor/           (gizmos, picking handles, debug overlays)
└── projection_cache/            (memoization, invalidation tracking, dirty-flag propagation)
```

**Projection categories:**

| Category | Owns | Updates on |
|---|---|---|
| Structural | BRepHandle entity ↔ cad-node mapping; hierarchy emission | cad-core entity add/remove |
| Geometric | tessellation handles; bounds | cad-core operator commit (tessellation cache invalidate) |
| Semantic | material slot bindings; selection-set membership; layer info | cad-core annotation changes; user selection |
| Runtime | collision proxies for physics; visibility filters; render queue input | per-frame, throttled |
| Editor | gizmos, picking, debug visualizers | per-frame, editor-only (stripped on cook) |
| Cache | memoized projections; dirty bits | piggyback on all of the above |

**Why split:**
- Each category has a different mutation frequency (structural = rare; runtime = per-frame)
- Each has different invalidation rules (geometry invalidates with cad-core commit; editor invalidates with user input)
- Without split, cad-projection accumulates orchestration logic — exactly the failure mode we avoided in cad-core

**CI rules:**
- `projection_structural` cannot import `projection_runtime` or `projection_editor`
- Each module has documented "what triggers me" and "what I emit"
- Adding a 7th category requires an ADR

ADR-097.

### 1.6 Save file standards & import/export

#### 1.6.1 Three-layer file format discipline

| Layer | Format(s) | Purpose |
|---|---|---|
| Source (human-edited) | RON for game data; TOML for plugin manifests | one source-format family |
| Cooked (binary, ships) | `.rge-pak` (zstd, content-addressed) | one binary format |
| Import/Export (interop) | per-format industry standards | one importer per format |

#### 1.6.2 Source files

Project (`.rge-project`) · Scene (`.rge-scene`) · Prefab (`.rge-prefab`) · Material (`.material.ron`) · Anim (`.anim-graph.ron`) · Theme/Icons/Workspace · sidecars (`.meta.ron`) · plugin manifest (`plugin.toml`) · Rust scripts (`Cargo.toml` + `src/*.rs`).

#### 1.6.3 Identity scheme

| Kind | Format | Stability |
|---|---|---|
| EntityId | ULID; `Display` truncates `e_<8 chars>` | scene-stable |
| AssetId | `blake3:<hash>` | content-stable |
| ComponentTypeId | interned `crate::path::Type` | engine-version-stable |
| CadCheckpointId | `cad:<blake3>` | content-stable |
| PersistentFaceId / EdgeId / VertexId | `face:<hash>`, `edge:<hash>`, `vert:<hash>` + lineage path (NEW v0.7) | content-stable across rebuilds via lineage |

#### 1.6.4 Cooked binary `.rge-pak`

Header (magic `RGEP`, engine version, player-state schema version, flags) + sorted asset index + zstd-compressed asset blobs + optional Ed25519 signature.

#### 1.6.5 Import / export authority

One crate per format. CI lint blocks dual paths. `io-gltf` · `io-3mf` · `io-obj` · `io-stl` · `io-step` · `io-image` · `io-audio` · `io-fbx` (4-Polish) · `io-usd` (5-Scale stretch).

#### 1.6.6 Project layout

```
my-game/
├── .rge-project · .gitignore
├── assets/{meshes,textures,audio,...}/<file>{,.meta.ron}
├── scenes/{main-menu,level-1}.rge-scene
├── prefabs/{enemy,player}.rge-prefab
├── materials/*.material.ron
├── scripts/<name>/{Cargo.toml,src/lib.rs}
├── plugins/
└── target/cook/{desktop,mobile,...}.rge-pak
```

#### 1.6.7 Versioning + migration

Every source file `version: "x.y.z"`. Loader runs migrations.

#### 1.6.8 Determinism modes

| Mode | What's deterministic | What's NOT |
|---|---|---|
| None | nothing | — |
| **Replay-Stable v1.0** (gameplay only) | ECS systems with `DeterministicSystem` marker · fixed-timestep physics · script ticks · pinned HashMap iteration | GPU output · cad-core rebuild order · tessellation order · async streaming residency · file-system I/O timing |
| Lockstep-Stable | bit-identical across same-arch machines | post-v1.0 |
| Authoritative-Server | multi-machine via authoritative state | post-v1.0 |

CAD-output determinism is a separate concern, tracked in `RGE/CAD_DETERMINISM.md`. Best-effort at v1.0; not gated.

#### 1.6.9 Player-state versioning

Separate from cook versioning. CI gate on every game major bump.

#### 1.6.10 Determinism guarantees

Content-addressed assets give same source → same ID. EntityId ULID seeded → reproducible scene-cook. Cook output byte-identical given identical source tree.

### 1.7 Diagnostics philosophy

One unified system across subsystems (`kernel/diagnostics`, miette/ariadne-style). Rich span info, error aggregation (not first-fail), suggestion engine, editor integration.

### 1.8 Dependency governance

`cargo-deny` + `cargo-udeps` + custom DAG validator.

**Forbidden-dependency rules:**
- Tier 1 cannot depend on Tier 2
- Tier 2 cannot depend on Tier 3
- `editor-ui/*` cannot depend on `physics`/`audio`/`input` directly
- `physics` cannot depend on `script-host`
- Renderer cannot depend on game-domain crates
- `cad-core` cannot depend on ECS or any other Tier-2 crate (cad-core stands alone)
- Only `cad-projection` may import cad-core types into ECS code
- **`projection_structural` cannot import `projection_runtime` or `projection_editor`** (NEW v0.7)
- **Graph-using crates (material/anim/script/cad/render) must use `kernel/graph-foundation` primitives, not invent their own** (NEW v0.7)

### 1.9 Non-goals until v2

| Cut | Reason | Revisit |
|---|---|---|
| OpenXR / VR / AR | scope; spec moving | post-v1 |
| USD import/export | `usd-rs` immature | when matures |
| DLSS / FSR direct integration | NDA / vendor SDKs | Tier-3 plugins |
| Path-tracing viewport preview | Phase-5 | 5-Scale |
| Vehicle physics | defer exposure | 4-Polish stretch |
| Soft bodies + cloth | XPBD complexity | 5-Scale |
| AssemblyScript at launch | demand-gated | 4-Polish |
| Free-form animation retargeting | humanoid-only at v1.0 | post-v1 |
| Distributed cooking | scope | post-v1 |
| Lockstep-Stable cross-machine | requires soft-floats | 5-Scale |
| CAD-output determinism guarantees | individually hard | post-v1 |
| Console targets | NDA / cert | 5-Scale; pick one |
| Visual scripting AOT-to-Rust | post-launch |
| Multi-monitor workspaces | editor sophistication | post-v1 |
| Sub-graph composition in graph editors | reach product | post-v1 |
| **Photoreal rendering parity (UE5-class)** (REVISED v0.7) | reach product framing was over-claimed | post-v2 |
| **Replicated topology state (multi-peer concurrent CAD edit)** (NEW v0.7) | replaced by authoritative CAD serialization (§6.17) — much narrower | post-v1 |

### 1.10 Build governance

#### 1.10.1 Monomorphization budgeting

Generic instantiations per crate (warn 5000, hard 15000) · codegen units · trait expansion depth (warn 8, hard 16) · binary size by crate. Tools: `cargo-llvm-lines`, `cargo-bloat`.

#### 1.10.2 Dynamic-island policy

Default for new code: **dynamic dispatch unless on a hot path.**

| Class | Dispatch |
|---|---|
| Hot core (ECS iteration, render submit, physics step, script-host hot path) | generic monomorphized |
| Tooling, authoring graphs, plugin host, importers | dyn |
| cad-core | partial: graph traversal generic; user-facing API dyn |

#### 1.10.3 Crate fusion criteria

Symmetric to splitting. Merge if same-PR ≥80% over 6 months AND not separately consumed by Tier-3 AND combined <5000 lines AND coherent unified responsibility. Merge requires ADR.

#### 1.10.4 Architecture entropy metrics (extended v0.7)

Track at every minor version bump. Watch thresholds = warning, not auto-fail.

| Metric | Watch threshold |
|---|---|
| Cross-crate dep edges | doubling per release |
| Relation type count | adding >2/release |
| ECS archetype count | 2× growth per release |
| Invalidation fanout (when X changes, systems re-run) | >20 systems = lint warn |
| Hot-reload migration LOC per type | >50 = lint |
| Public API surface (symbols) per crate | >500 = signal for fusion |
| Kernel/Tier-2 boundary calls | tracked for ABI churn |
| cad-projection invalidation density | >30% per minor bump |
| **Async boundary count** (sync↔async crossings) (NEW v0.7) | >15% growth = warn |
| **Snapshot crossing count** (PIE + render-side + cad-core) (NEW v0.7) | tracked |
| **Graph invalidation propagation depth** (max across all graphs) (NEW v0.7) | >5 hops = lint |
| **Reflection schema size** (typed components × fields) (NEW v0.7) | >10K = warn |
| **Editor-state ↔ runtime-state synchronization edges** (NEW v0.7) | >50 = warn |
| **Capability surface growth** (host functions per WIT version) (NEW v0.7) | >3 added per minor = warn |
| **WIT interface growth rate** (NEW v0.7) | tracked |
| **Plugin ABI churn rate** (NEW v0.7) | tracked |
| **Incremental invalidation radius** (crates rebuilt after touching one core type) (NEW v0.7) | >30% of workspace = lint warn |

The last metric (invalidation radius) is potentially more important than raw compile time — it measures the cost of changing core types. ADR-092 extended.

### 1.11 Memory governance

Streaming residency (§7) is one piece. Per-subsystem owners. Residency budgets, VRAM pressure, allocator strategy, transient arenas, ECS archetype compaction, WASM memory ceilings, cad-core history retention (last N=50 in memory; older to disk; structural sharing), editor cache sizes.

### 1.12 egui pressure tracking

Vendor patches (target 0) · `// EGUI-PRESSURE: <reason>` workarounds (target <5) · unavailable wanted features (target <3) · upstream-break cycles (target <1). Escape conditions documented; if any threshold breaches, formal review.

### 1.13 Failure containment model (NEW v0.7)

Critical for industrial tools. Five failure classes mapping subsystem failures to engine response:

| Class | Examples | Response |
|---|---|---|
| **Recoverable** | tessellation crash on bad input, shader compile timeout, plugin panic during init, expr-wasm parse error | Isolate; surface diagnostic; subsystem stays online with reduced capability |
| **Snapshot-recoverable** | cad-core rebuild partial failure, hot-reload migration failure, projection invalidation cycle, schema-divergence-on-load | Rollback to last known-good checkpoint; user gets undo entry surface |
| **Plugin-fatal** | sandbox escape attempt, repeated panic, persistent resource quota breach, manifest tamper | Unload plugin; revoke trust if Verified-Community; surface diagnostic; editor continues |
| **Session-fatal** | asset DB corruption, schema version unrecoverable, cad-core graph corruption, render device lost beyond recovery | Save recovery dump; restart editor; offer to reload from autosave |
| **Kernel-fatal** | OOM, deadlock in core scheduler, kernel ABI mismatch detected, audit-ledger corruption | Crash report; terminate; user restarts from clean state |

**Per-subsystem failure-class table** (selected):

| Subsystem | Failure | Class |
|---|---|---|
| cad-core | rebuild partial failure | snapshot-recoverable |
| cad-core | graph corruption (audit-ledger checksum fail) | session-fatal |
| tessellation | bad input topology | recoverable |
| material-graph | shader compile hang | recoverable (timeout 30s) |
| material-graph | naga validation fail | recoverable (placeholder pipeline) |
| anim-graph | invalid state-machine cycle | recoverable (skip frame, log) |
| physics | Rapier internal panic | recoverable (entity quarantined) |
| script-host | WASM trap | plugin-fatal if Tier-3; recoverable if Tier-2 |
| script-host | hot-reload migration fail | snapshot-recoverable |
| projection | invalidation cycle (>1000 iterations) | snapshot-recoverable + cycle-source diagnostic |
| asset-streaming | residency exceeds VRAM by 2× | recoverable (force eviction) |
| pak-format | read corruption | session-fatal |
| egui | panic in draw | recoverable (frame skipped) |
| kernel/scheduler | deadlock detected | kernel-fatal |
| audit-ledger | checksum fail | kernel-fatal |

**CI verifies** failure-class declarations are present for every Tier-1 + Tier-2 crate. Recovery paths tested via fault injection on golden test projects.

ADR-102. Companion: `RGE/RECOVERY_MODEL.md`.

### 1.14 graph-foundation substrate (NEW v0.7)

The fourth-round review caught: we have eight different graph systems (material, anim, script, cad, render, dependency, workspace, ECS relations). Each will invent identical primitives — node IDs, edge IDs, stable hashing, diff, snapshot, invalidation propagation, visualization. Without coordination, these diverge silently.

**Crate:** `kernel/graph-foundation` (Tier 1).

**What it provides** (substrate, infrastructure):

```
kernel/graph-foundation
├── NodeId / EdgeId types (stable, sortable, hashable)
├── stable-hash policy (BLAKE3-keyed structural hashing)
├── diff primitives (3-way merge, structural diff)
├── snapshot serialization (immutable + structural sharing)
├── invalidation propagation API (subscribe to dirty bits)
├── audit-ledger integration (graph mutations as events)
├── visualization-adapter trait (for editor graph viewers)
└── persistence helpers (RON serde for graph state, content-addressed)
```

**What it does NOT provide** (avoiding god-substrate):
- Graph traversal algorithms (each domain has its own)
- Graph evaluation (each domain has its own evaluator)
- Graph-specific semantics (material codegen, anim state-eval, render scheduler, cad operator transform — all domain-specific)
- Universal graph runtime (domains aren't unifiable at runtime)

**Discipline (Rhai-test for graph-foundation):**

Anyone proposing to add functionality to graph-foundation must answer: "is this primitive infrastructure that all 8 graph systems would use the same way?" If no, it goes in the domain crate. If yes, it goes in graph-foundation. CI lint flags graph-foundation additions for review.

**Adoption per graph system:**

| Graph | Uses graph-foundation for | Owns its own |
|---|---|---|
| Material | NodeId, EdgeId, hash, diff, snapshot, viz adapter | WGSL codegen, naga validation, parameter buffers |
| Animation | same | state-machine eval, blend-tree eval |
| Script | same | wasm-encoder codegen |
| cad-core operator | same | operator transform, persistent IDs, lineage |
| Render | same | frame-graph compile, transient resource alloc |
| Dependency | same | build-order resolution |
| Workspace | same (lighter — small graphs) | layout reconcile |
| ECS relations | partial (NodeId only — ECS has its own storage) | specialized relation storage |

**Why this matters:**
- Eight invented-N-times primitives → one place. Maintenance burden divided by ~8.
- Cross-graph diff/snapshot/audit becomes possible (e.g., "show me what changed in material graph + cad operator graph between these two checkpoints").
- Editor graph-viewer widgets work uniformly across material/anim/script/cad without per-domain reimplementation.
- New graph types (someday: behavior trees, dialogue graphs, build pipelines) plug into the substrate; they don't reinvent.

**Failure mode to avoid:** graph-foundation becoming a god-substrate that tries to unify domain semantics. The discipline is "primitives, not runtime."

ADR-101. Companion: `RGE/GRAPH_FOUNDATION.md` (Phase 4-Foundation deliverable).

### 1.15 Editor state coordination (NEW v0.8 — narrow scope, last architecture commitment before freeze)

**Rule:** Editor-state is **coordination state, not authoritative content state.** It coordinates interaction context across editor panels — selection, hover, active tool, modal interaction, drag/drop. It does not own runtime data, CAD geometry, or mutation history.

**Authority table (the line that prevents editor-state from accumulating):**

| Authority | Owns |
|---|---|
| `kernel/ecs` | runtime entity state |
| `cad-core` | CAD graph, persistent IDs, lineage, history |
| Command Bus + `kernel/audit-ledger` | mutation log, undo |
| `kernel/asset` + `pak-format` | authoritative asset content |
| **`editor-state`** | **coordination of interaction context across panels — only** |

`editor-state` references runtime state via IDs/handles; it does not store content. Selecting an entity stores `EntityId`, not the entity's components.

**Crate:** `crates/editor-state` (Tier 2). Five categories — fixed scope at v0.8:

```
crates/editor-state/src/
├── lib.rs                       (public API; coordination only)
├── selection.rs                 (entity sets, component sets, face/edge/vertex sets, graph node sets)
├── hover.rs                     (per-panel hover state with stable IDs)
├── active_tool.rs               (current tool per viewport, tool stack)
├── modal_state.rs               (drag-in-progress, brush-down, dial-input — exclusive interaction states)
└── drag_drop.rs                 (in-progress drag/drop transactions across panels)
```

**Explicitly NOT in editor-state** (handled elsewhere; promoting them would over-architect):
- Viewport state → `editor-ui/workspace` per-viewport widget
- Workspace dock layout → `editor-ui/layout` (already)
- Transient preview → tool/subsystem-local
- Undo preview → Command Bus peek operation
- Collaborative cursors → reserved markers via §6.17 networking; impl Phase 5-Scale
- Editor-state-graph → not needed; selection is mostly flat-set + scopes, not relational

**Integration:**

| With | Behavior |
|---|---|
| Command Bus (§6.16) | `EditorAction` event class for selection / tool / modal changes; recorded in audit-ledger; undoable |
| audit-ledger | Editor-state mutations recorded as a separate event stream; subset of audit log |
| PIE (§6.13) | Editor-state persists across Play/Stop (selection survives, tool persists); does NOT participate in `WorldSnapshot` |
| Failure containment (§1.13) | Editor-state corruption = recoverable (re-init from defaults; surface diagnostic) |

**CI rules:**
- Subsystems may not invent their own `Selection` / `Hover` / `ActiveTool` / `ModalState` / `DragDrop` types — must use `editor-state`
- `editor-state` may not import authoritative content types (component bodies, cad-core nodes, asset payloads) — only IDs and handles
- New categories require ADR + demonstrated 2-subsystem pressure (per §0.6 freeze policy)

**Discipline (the meta-rule):**
- Editor-state is *coordination*, not *authority*. If a proposed addition stores content, it goes elsewhere.
- The five-category bound is fixed at v0.8. Promotion to a sixth requires §0.6 freeze-policy gate.

ADR-110. Companion: `RGE/EDITOR_STATE_MODEL.md` (Track F deliverable, narrow scope).


---

## 2. Pillar 1 — Native multi-platform

### 2.1 Target matrix

| Tier | Targets | Backend | Window/IO | Renderer | Phase |
|---|---|---|---|---|---|
| 0 v1.0 must-ship | windows-x86_64, linux-x86_64, macos-aarch64 | native | winit | Vulkan/Vulkan/Metal | 4-Foundation → 4-Polish |
| 1 v1.0 preview | macos-x86_64, web-wasm32 | native, wasm-bindgen | winit/web-sys | Metal, WebGPU | 4-Polish |
| 1 v1.0 preview | ios-aarch64, android-aarch64 | native | winit + glue | Metal, Vulkan | 4-Polish |
| 3 post-v1 | console-X | NDA | per-platform | per-platform | 5-Scale |

### 2.2 Single source of truth

One workspace, one `cargo build --target=…` matrix.

### 2.3 Anti-goals

No Electron / CEF / embedded browser. No platform-specific renderers. No fork-per-platform.

---

## 3. Pillar 2 — Multi-device

### 3.1 Input matrix

| Class | Source | Editor | Game-runtime |
|---|---|---|---|
| Mouse + kbd | winit | gizmos, viewport nav | `Input<KeyCode>`, `Input<MouseButton>` |
| Gamepad | gilrs | viewport play-mode | `Input<GamepadButton>` |
| Touch | winit | viewport pan/pinch | gestures |
| Stylus | winit | sculpt brush pressure | sculpt-only at v1 |
| XR | openxr-rs (Phase 5) | preview-in-VR | full XR runtime in 5-Scale |

### 3.2 Render-scaling tiers

Mobile · Laptop · Desktop · Workstation. `gfx::caps` selects at boot.

### 3.3 Asset variants

Per-tier (BCn vs ASTC, LOD chains, audio bitrate). DDC keyed on `(asset_id, tier)`.

---

## 4. Pillar 3 — Realtime industrial-quality editor rendering (REVISED v0.7)

### 4.1 Phase alignment

- **4-Foundation (m1–6):** wgpu forward+. PBR. Cascaded shadows.
- **4-Geometry (m7–12):** B-Rep first-class via cad-core. Adaptive tessellation. Persistent topology IDs in render path.
- **4-Polish (m13–18):** SSGI fallback. IBL. HDR.
- **5-Rendering (m19–30):** Lumen-equivalent. Selective Nanite-like features on cad-core cluster output. VSM. TSR. **Not full UE5 parity.**
- **5-Scale (m30–36):** Path-tracing preview. Selective competitive features.

**Reframe:** v1.0 = "industrial-quality realtime editor." Phase 5 = "competitive on selective features." Photoreal parity = post-v2. Honest reach-narrowing.

### 4.2 B-Rep fallback discipline

Triangle mesh always works. Every B-Rep entity falls back to baked triangle representation if cad-core integration not ready, kernel hits unhandled topology, target tier doesn't support B-Rep, or user opts out. Fallback generated at cook time, cached in DDC by `CadCheckpointId`.

---

## 5. Pillar 4 — Fastest script engine

### 5.1 One runtime: wasmtime

Editor sessions: wasmtime + Cranelift JIT. Cook target: `wasmtime compile` → `.cwasm`. Capability-gated. No `script-aot-llvm`, no `live-coding`.

### 5.2 Three authoring lanes at v1.0

| Lane | Audience | Compiles via |
|---|---|---|
| Rust → WASM (default) | gameplay, systems, AI, plugins | `cargo build --target wasm32-unknown-unknown` |
| Visual graph → WASM (`script-graph`) | artists, narrative designers | graph IR → wasm-encoder |
| `expr-wasm` | property fields, predicates | string → AST → WASM bytes |

AssemblyScript deferred to 4-Polish, demand-gated.

### 5.3 `expr-wasm`

```
string → parser → AST → codegen → WASM bytes
       → wasmtime::Module (cached) → Cranelift JIT'd → ~5ns/call
```

### 5.4 ECS bridge — `crates/script-host`

WIT world `rge-game.wit` imports `rge:ecs/query`, `rge:ecs/observer` (Changed<T>), `rge:ecs/events`, `rge:asset/view`. Capability tickets enforced.

### 5.5 Hot-reload

```
save Rust source → cargo build → wasmtime::Module::new (~50ms)
  → pause systems (~one frame) → reflect-roundtrip migrate
  → new instance takes over → p95 budget <100ms
```

### 5.6 "Fastest" benchmark suite (`crates/script-bench`)

Within 1.5× native Rust on ECS hot loop · strictly faster than Lua/mlua/Wasmer-singlepass/Bevy-extism · cold-start <50ms · hot-reload p95 <100ms · per-module <1MB.

### 5.7 Script tooling subsystem

`script-debugger` · `script-profiler` · `script-symbols` · `script-reflect` · `script-panic-recovery`.

### 5.8 Anti-goals

No custom language at v1.0. No embedded JS engine. No Mono/.NET. No Lua/Python in core. No stack-bytecode VM.

---

## 6. Editor & authoring subsystems

### 6.1 Toolkit: egui (locked, with §1.12 pressure tracking)

### 6.2 Theme support — `ui-theme`, `ui-icons`, `ui-fonts`

Token system · inheritance via `extends:` · variants composable along orthogonal axes (scheme · accessibility (high-contrast/reduced-motion/large-text/reduced-transparency) · color-blind) · scope resolution · "follow OS" first-launch · WCAG-AA contrast lint · minimal switcher in core, full theme editor as Tier-2 plugin.

### 6.3 Menu/toolbar registry — `editor-ui/menus`

UE5 `UToolMenus`-inspired. `OrderHint::Before/After`. Predicates: closure or `expr-wasm`.

### 6.4 Widget library — `editor-ui/widgets`

One file per widget. Standard set + `node_graph.rs` (built on `kernel/graph-foundation` viz adapter).

### 6.5 Page layout — `editor-ui/layout`

RON workspace files. Workspaces: `Default`, `Animation`, `Sculpt`, `Code`. Plugins ship workspaces.

### 6.6 Dock + tab manager

`tab_manager.rs` + `layout_service.rs` + `spawner_registry.rs` on `egui_dock`. Layout-name versioning rule mandatory.

### 6.7 Hot-reload mechanics

File change → notify → re-parse → diff → minimal egui-state mutations → repaint. <50ms.

### 6.8 UE pattern → RGE crate map

| UE | RGE |
|---|---|
| `FSlateStyleSet` | `ThemeRegistry` |
| `UToolMenus` | `MenuRegistry` |
| `EUMenuExtensionHook` | `OrderHint::Before/After(id)` |
| `FToolBarBuilder` | `Toolbar` widget |
| `FTabManager` / `FLayoutSaveRestore` | `TabManager` / `LayoutService` |
| `SCompoundWidget` | RON layout + spawner registry |

### 6.9 Material editor

`material-graph` (built on graph-foundation) · `material-graph-editor` · `material-runtime`. Anti-thrashing: variant cache, runtime parameter buffers, preview pipeline reuse, 50ms debounce.

### 6.10 Physics

`physics` (Rapier wrap) · `physics-debug` (separate). Components in `crates/components/`. Schedule: pre_physics → physics_step → post_physics → contact_events. Pinned Rapier; deterministic broadphase; same-platform replay via audit-ledger.

### 6.11 Animation — five-crate split

`anim-clip` · `anim-graph` (built on graph-foundation) · `anim-graph-editor` · `anim-ik` · `anim-retarget` · `anim-events`. Humanoid-only at v1.0.

### 6.12 Skinning — `crates/skinning`

LBS default (≤256 bones) · DQS opt-in · compute-shader (>256 bones).

### 6.13 Play-in-Editor

```
[Play] → ECS world snapshot + cad-core checkpoint reference → PlayState: Editing → Playing
[Stop] → restore snapshot → world byte-identical to pre-play
```

`SnapshotParticipate` trait required by `audio` · `physics` · `particles` · `gfx` · `cad-projection` · Tier-3 plugins. Selective serialization (full clone up to ~50k entities; diff mode above).

### 6.14 Subsystem integration map

```
cad-core graph mutates → cad-projection (split per §1.5.4.5) updates ECS BRepHandle entities
                              ↓
anim-graph → BoneTransforms → skinning → bone palette → GPU
                                ↓                          ↑
                              gfx ← material-graph → WGSL → wgpu
                              ↑
                        render-side snapshot — cad-core checkpoint frozen
                              ↑
                   sim mutates topology safely

anim-events → kernel/events → script-host → WASM scripts react
physics → BoneTransforms (ragdoll) → kernel/events → triggers → script-host

material edit → graph diff (graph-foundation) → naga (only on topology) → pipeline swap
anim edit    → graph asset reload → next anim tick
script edit  → cargo build → wasm swap → reflect-migrate
layout edit  → RON re-parse → diff egui state → repaint
theme edit   → re-bind tokens → repaint
cad edit     → cad-core operation → checkpoint commit (with lineage) → cad-projection update → next render
PIE Play/Stop → SnapshotParticipate hooks (incl. cad-projection)
```

### 6.15 Reflection UI hints

`#[rge::reflect]` with closed-set `UiHint` vocabulary. Validation via `validate = "<expr-wasm>"`. Custom drawers via `custom_drawer = "..."`.

### 6.16 Command Bus + Undo/Redo (REVISED v0.7 — promoted from "Action layer")

The Action+UndoStack model from v0.5/v0.6 is now framed explicitly as the **Command Bus**: every editor mutation, regardless of source (tool, shortcut, widget, graph editor, inspector, plugin, hot-reload), flows through one mediation layer.

#### 6.16.1 Command Bus principle

Editor systems do not directly mutate runtime state. They emit Commands (which become Actions or CadCheckpoints) into the bus. The bus dispatches to the runtime side via the Action::apply / cad_core.commit path. CI lint forbids any editor crate from accessing runtime mutation paths outside the bus.

This makes constitutional principle #8 (editor extends runtime, never replaces) enforceable in code.

#### 6.16.2 Action layer (editor mutations)

```rust
trait Action: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn apply(&self, world: &mut World) -> Result<()>;
    fn revert(&self, world: &mut World) -> Result<()>;
    fn merge(&mut self, next: &dyn Action) -> MergeOutcome;
}
```

Used for: entity spawn/despawn, component edit (non-CAD), asset import, script edit, material/anim graph node connect, prefab edit, layout edit, theme edit.

#### 6.16.3 CAD-graph-checkpoint layer

```rust
fn begin_operation() -> OpHandle;
fn commit(op: OpHandle) -> CadCheckpointId;  // creates lineage edges
fn rollback(op: OpHandle);
fn restore_to(checkpoint: CadCheckpointId);
```

Used for: B-Rep operator add/remove/edit, constraint changes, parametric value edits, B-Rep healing.

#### 6.16.4 Unified bus (UndoStack)

```rust
enum BusEntry {
    Action(Box<dyn Action>, ActionEffect),
    CadCheckpoint { id: CadCheckpointId, description: String },
}
struct CommandBus {
    history: Vec<BusEntry>,
    cursor: usize,
}
```

User sees one combined undo stream. Bus dispatches to `Action::revert` or `cad_core.restore_to` based on entry type. Two backends, one user-facing system. Passes Rhai-test.

#### 6.16.5 Hot-reload + play-mode compatibility

During Play, scripts may mutate state — those mutations are not undoable. Stop restores pre-play snapshot. Hot-reload of script module is an Action — undoable.

#### 6.16.6 Audit-ledger integration

Bus events recorded in `kernel/audit-ledger`. Undo log = filtered projection. No duplicate infrastructure.

#### 6.16.7 Coalescing

500ms-window same-target Action coalescing. CAD checkpoints don't coalesce.

#### 6.16.8 Plugin-shipped Commands

Tier-2 + Tier-3 plugins register Action impls via `kernel/plugin-host`. CAD operations are extension-pointed; Tier-3 plugins can register new operator types via cad-core's plugin API.

CI gate: every editor mutation site goes through the Command Bus; CI fails if a mutation path bypasses.

ADR-091 + ADR-100. Companion: `RGE/UNDO_REDO_MODEL.md`.

### 6.17 Networking — authoritative CAD serialization (REVISED v0.7)

Real impl deferred to Phase 5-Scale. Conceptual scaffolding ships at v1.0 to prevent painful retrofit.

**Reserved components in `crates/components/`** (zero-cost markers at v1.0): `Replicated` · `NetworkOwner(PeerId)` · `Authoritative` · `RemotePeer(PeerId)` · `ReplicationPolicy` · `LastReplicatedTick`.

**CAD networking model — authoritative serialization (NOT replicated topology state):**

The v0.6 wording "PersistentFaceId is replication-stable by design" was aspirational. Two peers concurrently editing parametric CAD will diverge — graph, topology, tolerance, rebuild ordering all drift. The honest model:

| Concept | Approach |
|---|---|
| Authoritative state | Authoritative peer holds canonical cad-core graph |
| Sync mechanism | Operations broadcast as serialized op-graph deltas (not topology snapshots) |
| Client behavior | Clients rebuild locally from received deltas |
| Topology IDs | Per-peer LOCAL — each peer has its own; reconciliation maps peer-local to authoritative |
| Conflict resolution | Authoritative wins; client rolls back |
| Concurrent edits | Client-side optimistic; reconciled on roundtrip via lineage graph (§1.5.4.3) |
| Reconciliation | Lineage-aware: explicit Split/Merged/Reinterpreted markers for user resolution |

This is **authoritative operation graph + per-peer rebuild + reconciliation**, not replicated topology state. Much narrower honest claim. Phase 5-Scale work.

**Documented intersects:** Lockstep-Stable required for true lockstep multiplayer (which CAD doesn't need — auth-server is sufficient) · streaming respects residency · audit-ledger uses Lamport clocks.

ADR-103. Companion: `RGE/NETWORKING_PLAN.md`.

---

## 7. Async / resource streaming

`kernel/job-system` · `kernel/io-scheduler` · `kernel/asset-streaming` · `kernel/asset-view`. Streaming priorities: in-frustum near (must be resident) · in-frustum far (queue) · out-of-frustum near (predictive) · out-of-frustum far (evictable). Hysteresis 1s + predictive prefetch.

---

## 8. Renderer architecture (brief)

Render graph compiler · double-buffered scene state with cad-core checkpoint reference · transient resources (aliased VRAM) · shader permutations on demand · PSO cache · async shader compile daemon · bindless · frustum + portal at v1.0 · Hi-Z at Phase 5 · GPU device-lost recovery.

---

## 9. Marketplace governance

### 9.1 Trust + signing

Ed25519 over manifest + WASM bytecode · trust levels: Verified / Community / Untrusted · revocation · OIDC author identity.

### 9.2 Sandbox security

Cap-gate · resource quotas (memory, wall-clock, network) · anti-crypto-miner via "background-low" CPU cap.

### 9.3 Supply chain

Reproducible plugin builds · SLSA provenance · BLAKE3 content-ID indexing · compatibility matrix · version pinning.

### 9.4 Operational policies

Public revocation · identity loss on repeat offense · in-app reporting · coordinated disclosure for sandbox vulnerabilities.

---

## 10. Crate inventory by tier

### 10.1 Tier 1 — Kernel (~14 crates)

| Crate | Purpose |
|---|---|
| `kernel/ecs` | specialized relation storage |
| `kernel/schedule` | ordered system pass execution |
| `kernel/asset` | content-addressed asset loader |
| `kernel/asset-view` | zero-copy WASM linear-memory mapping |
| `kernel/asset-streaming` | residency manager |
| `kernel/io-scheduler` | priority IO queue |
| `kernel/job-system` | work-stealing thread pool |
| `kernel/plugin-host` | plugin lifecycle, dep resolution |
| `kernel/types` | type registry, reflection bridge, UI hints |
| `kernel/events` | typed event bus |
| `kernel/audit-ledger` | recording / replay substrate |
| `kernel/diagnostics` | unified diagnostic / span |
| **`kernel/graph-foundation`** (NEW v0.7) | **graph substrate (NodeId/EdgeId/hash/diff/snapshot/invalidation), used by all 8 graph systems** |
| `kernel/app` | main loop driver |

### 10.2 Tier 2 — Privileged plugins (~70 target)

| Group | Crates |
|---|---|
| Reusable types | `components` · `events` · `resources` · `math` · `errors` |
| CAD core | `cad-core` (graph + operators + persistent IDs + lineage + history + tessellation cache; cad-topo + cad-topo-lineage as internal modules) · **`cad-projection` (split into projection_structural / projection_geometry / projection_semantic / projection_runtime / projection_editor / projection_cache modules)** · `cad-native` (truck-backed) · `cad-occt` (OCCT-backed, opt-in) |
| Rendering | `gfx` · `brep-render` · `gfx-ir` |
| Physics/audio/input | `physics` · `physics-debug` · `audio` · `input` · `input-gestures` |
| Asset pipeline | `asset-pipeline` · `asset-store` · `pak-format` · `rge-data` |
| Importers/Exporters | `io-gltf` · `io-3mf` · `io-obj` · `io-stl` · `io-step` · `io-image` · `io-audio` · `io-fbx` (4-Polish) · `io-usd` (5-Scale stretch) |
| UI substrate | `ui-theme` · `ui-icons` · `ui-fonts` |
| Editor UI | `editor-ui/{menus,widgets,layout,dock,workspace}` · `theme-editor` |
| Editor host | `editor-shell` |
| Editor mutations | `editor-actions` (Command Bus + UndoStack) |
| Editor coordination state (NEW v0.8) | `editor-state` (selection · hover · active-tool · modal-state · drag-drop — coordination only, no authoritative content) |
| Scripting | `runtime-wasmtime` · `runtime-wasmtime-engine` · `script-host` · `script-aot` · `script-bench` · `script-graph` · `expr-wasm` |
| Scripting tooling | `script-debugger` · `script-profiler` · `script-symbols` · `script-reflect` · `script-panic-recovery` |
| Material | `material-graph` · `material-graph-editor` · `material-runtime` |
| Animation | `anim-clip` · `anim-graph` · `anim-graph-editor` · `anim-ik` · `anim-retarget` · `anim-events` |
| Skinning | `skinning` |
| Authoring | `sculpt` · `particles` |
| Build/cook | `build-pipeline` · `runtime-platform-{windows,macos,linux,mobile,web}` |
| Reflection | `macros-reflect` |
| Hot-reload | `hot-reload-watcher` |
| Networking placeholder | `replication` (stub) |
| Marketplace | `marketplace` · `marketplace-server` |
| Plugin substrate | `plugin-discovery` (existing) |

### 10.3 Tier 3 — Sandboxed plugins

WASM, capability-gated. Examples: USD/FBX importers · DLSS/FSR extensions · localization packs · DCC bridges · Lua/AssemblyScript-as-marketplace-plugin · custom CAD operators (cad-core has plugin extension API).

### 10.4 Dogfood rule

Tier 2 uses the same `Plugin` trait as Tier 3. Includes `cad-core`, `cad-projection`, `kernel/graph-foundation` consumers in contract tests.

---

## 11. Roadmap (delta from master plan §10)

- **Month 1:** see §12 (21 parallel waves).
- **Months 2–6 (4-Foundation):** `script-host` + change detection · `script-bench` · theme/menu/layout · three-tier discipline · async/streaming substrate · diagnostics · Command Bus · `cad-core` MVP (Extrude/Revolve/Boolean) · `cad-projection` skeleton split · `kernel/graph-foundation` substrate · failure containment model active · build governance gates active. Hot-reload smoke by m4. First "fastest" benchmark by m6.
- **Months 7–12 (4-Geometry / 4-Authoring):** `script-aot` cooks `.cwasm` · `script-graph` emits WASM · cad-core full operator set + Fillet + lineage + history · cad-projection invalidation logic complete · CAD kernel non-equivalence doctrine enforced · B-Rep render integration with snapshot staging · Anim graph editor · Material instance/graph (graph-foundation-backed) · Skinning DQS + compute · Marketplace v1 with supply-chain.
- **Months 13–18 (4-Polish):** Mobile + web Tier-1 · AOT perf gates · "Fastest" benchmarks public · Lumen-equivalent baseline · AssemblyScript decision · Script tooling.
- **Months 19+ (5-Rendering, 5-Scale):** selective competitive features · Authoritative CAD serialization · Lockstep-Stable mode · CAD-output determinism evaluation.

### 11.5 Staffing assumptions

| Effective parallel agents | Floor | Reach | Cut first |
|---|---|---|---|
| 20+ | full | full | — |
| 10–20 | full | partial reach | console targets, advanced theme editor, multi-monitor, sub-graphs |
| 5–10 | full | minimal reach | + script tooling (debugger), DQS skinning, multi-instance PIE |
| 3–5 | floor at risk | none | + AssemblyScript, theme variants beyond dark/light, full marketplace governance |
| 1–3 (solo) | survival | none | floor reduced to: kernel + script-host + Rust→WASM lane + PIE + cad-core minimal — no fancy tessellation, no Visual graph, no marketplace |

---

## 12. Phase 4-Foundation dispatch — 21 parallel waves

Detailed: [`WAVES.md`](./WAVES.md). 21 waves run in parallel after a 1-day workspace bootstrap; ~46 wave-days, executable in ~3 calendar days at 21-way parallelism.

W1 components · W2 macros-reflect+kernel/types · W3 editor-shell PIE · W4 wasmtime-engine · W5 ui-theme · W6 ui-icons · W7 ui-fonts · W8 editor-ui/menus · W9 editor-ui/layout · W10 editor-ui/dock · W11 physics · W12 audio · W13 input · W14 rge-data · W15 pak-format · W16 asset-store · W17 io-gltf · W18 io-image · W19 expr-wasm · W20 script-bench · W21 golden test projects.

**Note:** `cad-core`, `cad-projection`, `kernel/graph-foundation` are Phase 4-Geometry work, not part of the W1-21 parallel dispatch. Foundation waves focus on substrate; CAD subsystem and graph-foundation build on substrate in months 7-12.

### 12.1 W21 — Golden test projects

Four projects: `simple-scene` · `skinned-character` · `physics-puzzle` · `material-zoo`. CI on every major change.

---

## 13. Quality gates

### 13.1 Functional gates (existing, abridged)

Tier-0 cook on 3 OSes · `script_tick_1m_iters` ≤ 1.5× native · hot-reload p95 < 100ms · theme/icon/layout < 50ms · font swap < 100ms · material parameter edit no recompile · material topology < 100ms p95 · anim edit < 100ms p95 · skinned 1k chars 60fps · physics 1000-tick replay byte-identical · trigger event < 16ms · PIE 10k entities < 100ms · PIE 100k diff mode < 500ms · all stateful Tier-2 has SnapshotParticipate · buggy WASM doesn't kill editor · kernel API contract test · WebGPU 30fps mid-tier · Phase-5 selective features at 60fps RTX 3070.

### 13.2 Editor runtime budget gates

Editor frame idle ≤8ms · heavy authoring ≤12ms · idle resident on `simple-scene` ≤350MB · idle on `material-zoo` ≤500MB · dock rebuild ≤30ms · 50-node material graph ≤4ms · reflection cache 1000 components ≤2MB · egui allocs/frame ≤500 · font atlas churn ≤10/hour.

### 13.3 Build governance gates

Compile-time clean ≤120s · incremental p95 ≤10s · gen instantiations per crate ≤5000 warn / ≤15000 hard · trait expansion depth ≤8/16 · binary size regression ≤10% · forbidden-dep DAG passes · `cargo-deny` + `cargo-udeps` pass · **incremental invalidation radius ≤30% of workspace** (NEW v0.7).

### 13.4 Format / determinism gates

Round-trip glTF · round-trip RON byte-identical · `.rge-pak` 100MB load <500ms · cook byte-identical given same source · no two import paths.

### 13.5 Theme / accessibility gates

Variant stacking resolves · WCAG AA contrast · inheritance depth ≤ 3 · `reduced-motion` zeros `motion.*`.

### 13.6 B-Rep / topology / cad-core gates

Persistent topo IDs survive 1000 random parametric edits · cad-core transactional rollback round-trip · cad-projection updates within one tick of cad-core commit · cad-core checkpoint storage <1MB typical · BRepHandle invalidation correctness · render-side snapshot: topology mutation during frame doesn't invalidate render thread · **lineage graph correctness on Split/Merged/Reinterpreted** (NEW v0.7) · **kernel capability mismatch surfaces compile error** (NEW v0.7).

### 13.7 Command Bus / undo gates

Every editor mutation goes through Command Bus · undo/redo round-trip on each Action type · CompoundAction atomicity · CAD checkpoint round-trip · unified bus dispatches correctly · coalescing in 500ms window · history serialized + restored · **CI lint: no editor crate touches runtime mutation outside the bus** (NEW v0.7).

### 13.8 Marketplace / supply chain gates

Reproducible build byte-identical · SLSA provenance verified · sandbox escape blocked · resource quota enforced.

### 13.9 Code organization gates

No `.rs` exceeds 1000 lines without `// SPLIT-EXEMPTION` · no `utils.rs`/`helpers.rs` · anti-pattern audit on architectural PRs · golden tests pass · crate fusion criteria checked at minor bumps · **graph-foundation discipline: graph systems use substrate primitives, don't reinvent** (NEW v0.7).

### 13.10 Architecture entropy gates (extended v0.7)

§1.10.4 metrics tracked. Trend review at every minor bump. Ten new metrics added: async boundary count, snapshot crossings, graph invalidation depth, reflection schema size, editor↔runtime sync edges, capability surface growth, WIT growth, plugin ABI churn, **incremental invalidation radius**, **cad-projection invalidation density**.

### 13.11 egui pressure gates

Vendor patches = 0 · workarounds < 5 · unavailable features < 3 · upstream-break cycles < 1.

### 13.12 Failure containment gates (NEW v0.7)

Every Tier-1 + Tier-2 crate has documented failure-class declarations · fault-injection tests on golden projects pass for each class · session-fatal recovery dump verified · plugin-fatal isolation verified.

### 13.13 Execution domain gates (NEW v0.7)

Cross-domain reflection schema is shared (no duplicate) · capability model unified across domains · hot-reload orchestrator routes to correct per-domain handler · adding a 5th domain requires ADR (CI lint).

### 13.14 graph-foundation gates (NEW v0.7)

Every graph system uses graph-foundation primitives (CI lint catches reinvention) · cross-graph diff/snapshot works on combined material+cad+anim checkpoint · viz-adapter trait usable from `editor-ui/widgets/node_graph.rs` for all graph types · **graph-foundation API additions require ADR** (avoid god-substrate).

### 13.15 Editor-state gates (NEW v0.8)

| Gate | Threshold | Phase |
|---|---|---|
| `editor-state` does not own authoritative content (CI lint: only IDs/handles, no component bodies / cad-core nodes / asset payloads) | green | every PR |
| Selection consistency: scene-tree ↔ viewport ↔ inspector ↔ cad-history all show same selection on a B-Rep entity | green | exit 4-Foundation |
| `editor-state` mutations all flow through Command Bus (CI lint: direct writes outside `editor-actions` blocked) | green | every PR |
| Editor-state persists across Play/Stop cycle | green | exit 4-Foundation |
| No subsystem owns `Selection`/`Hover`/`ActiveTool`/`ModalState`/`DragDrop` outside `editor-state` (CI lint) | green | every PR |
| Adding a 6th editor-state category requires ADR + §0.6 freeze-policy gate | green | every architectural PR |

### 13.16 Architecture freeze gates (NEW v0.8)

Per §0.6:
- New first-class subsystem proposals must include implementation-evidence section · 3+ reproducer failure cases · cost/benefit vs alternatives · justification why a smaller primitive wouldn't suffice
- Reviewer checklist enforces the four conditions before merge
- ADRs proposing new subsystems without all four conditions auto-blocked

---

## 14. Risks

[Existing risks preserved + new from v0.7:]

| Risk | Likelihood | Mitigation |
|---|---|---|
| **WASM constitutional lock-in is the deepest single bet (bigger than CAD)** (PROMOTED v0.7) | medium-high | CAD can fall back to meshes; WASM cannot fall back without violating constitution. Per-tooling-category fallbacks documented in `RGE/WASM_TOOLING_FALLBACKS.md`. Escape clause (ADR-077) is last-resort. |
| Cranelift AOT slower than LLVM by >2× | medium | Upstream-contribute |
| WASM component model spec churn | medium | Vendor snapshot; upgrade on RGE cadence |
| WASM tooling ecosystem maturity (debugger, source maps, mobile, Apple) | medium-high | Per-category fallback positions |
| "Fastest" claim contested | medium-high | Publish methodology + reproducer |
| Kernel API ossifies | high | Versioned ABI; major bumps with migration |
| Three-tier discipline erodes | medium-high | Privileged plugin contract test |
| ECS storage specialization leaks through API | medium | Internal-only types |
| CAD/ECS impedance leaks despite separation | medium | cad-projection only crossing; CI lint forbids cad-core types in non-projection ECS code |
| **cad-projection becomes "god bridge"** (NEW v0.7) | medium | Internal split into 6 modules per §1.5.4.5; CI rule `projection_structural` cannot import `projection_runtime`/`projection_editor`; adding 7th category requires ADR |
| **CAD kernel abstraction leaks (lowest-common-denominator)** (NEW v0.7) | medium | Capability surface explicit per kernel; non-portable ops compile-error when wrong kernel selected; doctrine in §1.5.4.4 |
| **Persistent topology IDs unstable across rebuilds despite lineage** (REVISED v0.7) | high → medium with lineage | `cad-topo-lineage` with `TopologyEvolution` enum; intensive test (1000+ random edit traces); CAD lit reference; semantic continuity score |
| **Graph-domain fragmentation: 8 graph systems invent identical primitives** (NEW v0.7) | low (was high pre-v0.7) | `kernel/graph-foundation` substrate; CI lint forbids reinvention |
| **graph-foundation becomes god-substrate** (NEW v0.7) | medium | Discipline documented (§1.14) — primitives only, not runtime; ADR required for additions |
| Render-side snapshot staging adds 2× scene memory | low | Components small; <2% overhead |
| Async streaming residency thrashes | medium | Hysteresis; 1s grace; predictive prefetch |
| Tier-2 plugins miss `SnapshotParticipate` impl | high | Per-plugin contract test |
| Compile times explode past 5min | medium-high | §1.10 build governance gates |
| **Incremental invalidation radius blows compile budget on core type changes** (NEW v0.7) | medium-high | §1.10.4 tracks; >30% radius = CI warn; refactor required at threshold |
| Material graph thrashes naga + GPU | medium | Anti-thrashing per §6.9 |
| Anim retargeting breaks on non-humanoid | medium | v1.0 humanoid-only |
| Skinning DQS extreme weights | low | LBS fallback |
| Multi-platform input divergence | medium | `input` normalizes early |
| Photorealism Phase-5 timeline slips | high | Moat holds without photoreal; reach product cut order documented |
| `expr-wasm` whitelist grows | medium | Closed `compile(&str) -> Module` API |
| Layout RON schema breaks user workspaces | medium | `version:`; migration |
| egui_dock / egui upstream churn | medium | Vendor; bump deliberately |
| Wasmtime adds 5–15 MB | low | Acceptable |
| Token churn breaks user themes | medium | `version:`; migrate; deprecation 2 minor versions |
| Icon × theme combo unreadable | medium | Tintable monochrome SVG; CI test |
| Plugin theme references undefined tokens | medium | Validation at install time |
| Reduced-motion conflicts with cross-fade | low | Zero `motion.*` |
| Game-runtime UI grows sibling theme system | medium | `ui-theme` is the only registry |
| Two import paths for same format | medium | One-crate-per-format; CI lint |
| RON scene file too big to hot-reload | medium | Per-entity diff; soft-cap warn 50k |
| Cooked `.rge-pak` non-deterministic | medium-high | Sorted iteration; CI compares cooks bit-for-bit |
| User project files break across major engine bumps | high | Migration script + example fixtures |
| FBX support delayed indefinitely | medium | 4-Polish target; FBX→glTF converter as workaround |
| USD support delayed indefinitely | high | Stretch only; revisit when `usd-rs` matures |
| Marketplace gets first malicious plugin | high (eventually) | Revocation infra at v1.0 |
| Player saves break across engine majors | high | CI gate on migration fixtures |
| Reflection UI hints proliferate | medium | Closed-set `UiHint` |
| Editor memory exceeds 500MB on simple-scene | medium | Budget gates |
| Generic instantiation explosion | high | Monomorphization gates; dynamic-island default |
| Crate count past 100 | medium-high | Fusion criteria |
| Undo missed on a mutation path | medium | CI lint catches mutations not via Command Bus |
| Undo across PIE Play boundaries corrupts world | medium | Stop restores pre-play; play-mode mutations not undoable |
| Networking retrofit breaks ECS / streaming / topology | medium | §6.17 conceptual scaffolding now narrower (auth-CAD, not replicated topology) |
| Wasmtime hits documented escape-clause condition | low at v1.0, growing | ADR-077 |
| Architectural exhaustion from many unified abstractions | high | Constitution + review cadence + golden tests + dogfood + entropy metrics |
| egui-on-top pattern grows into next-Rhai | medium | §1.12 pressure tracking; escape conditions |
| Determinism over-claim | medium | §1.6.8 narrowed |
| Roadmap density exceeds staffing | high | §11.5 staffing assumptions; cut order clear |
| **Editor/runtime semantic bifurcation** (NEW v0.7) | medium-high | Constitutional principle #8 + Command Bus enforces editor extends runtime; CI lint catches editor crates touching runtime outside bus |
| **Recovery paths missing for industrial-tool scenarios** (NEW v0.7) | high (was unaddressed) | §1.13 failure containment with 5 classes; per-subsystem declarations; fault-injection tests |
| **`editor-state` accidentally accumulates authoritative content** (NEW v0.8) | medium | Coordination-not-authority rule (§1.15); CI lint blocks content imports; reviewer checklist |
| **Editor-state categories grow past five** (NEW v0.8) | medium-high | §0.6 freeze policy gates additions; ADR + 4 conditions required |
| **Abstraction addiction — every observed risk gets a subsystem** (NEW v0.8 — meta-risk) | high (was implicit, now named) | §0.6 freeze policy; planning ROI is diminishing past v0.8; future structural insights require implementation evidence |
| **Premature ossification — paper architecture surface exceeds implementation capacity** (NEW v0.8) | medium-high | Architecture freeze; pivot to implementation; revisit only on demonstrated pressure |
| Sibling-system mistakes recur | high | Anti-pattern audit on architectural PRs |
| Plan gaps slip past internal review | high | §0.5 mandatory external review at every minor version bump |

---

## 15. Open decisions

**Locked v0.7 (newly):**
- ~~cad-projection god-bridge risk~~ → internal split into 6 modules (§1.5.4.5)
- ~~Persistent topology under-specification~~ → topology lineage graph with TopologyEvolution (§1.5.4.3)
- ~~CAD kernel abstraction leakage~~ → non-equivalence doctrine + capability surface (§1.5.4.4)
- ~~GPU scripting reality~~ → execution domains explicit (§0.3.1)
- ~~Editor/runtime boundary erosion~~ → constitutional principle #8 + Command Bus (§0.3, §6.16)
- ~~Failure containment missing~~ → §1.13 with 5 classes
- ~~Graph-domain fragmentation~~ → `kernel/graph-foundation` substrate (§1.14)
- ~~Networking topology over-claim~~ → authoritative CAD serialization (§6.17)
- ~~Renderer realism~~ → industrial-quality framing; photoreal post-v2

**Locked earlier:**
- cad-core + cad-projection split (v0.6)
- Determinism narrowed to gameplay (v0.6)
- Two-layer undo (v0.6)
- Floor vs reach (v0.5)
- Runtime escape clause (v0.5)
- 300/600/1000 line policy (v0.4)
- Stack-bytecode rejected (v0.4)
- ECS storage specialization (v0.4)

**Remaining open:**
1. `crates/components/` vs `rge-core::components` — recommend top-level
2. Plugin permission `editor.ui.extend` cap naming
3. Localization carryover
4. Workspace versioning suffix format — recommend integer
5. PIE multi-instance limit — recommend unbounded with warn at N>4
6. Console targets — pick one for 5-Scale spike?
7. AssemblyScript trigger threshold
8. Memory governance ownership
9. cad-core kernel choice for tessellation in 4-Geometry — truck primary (default), OCCT opt-in

---

## 16. Decision log → ADRs

ADR-001..ADR-096 from v0.5/v0.6 preserved.

**New ADRs from v0.7:**

- **ADR-097** — cad-projection internal split into 6 modules (god-bridge avoidance)
- **ADR-098** — Topology lineage graph with `TopologyEvolution` enum
- **ADR-099** — Execution domains naming (CPU gameplay / GPU shading / GPU compute / Expression)
- **ADR-100** — Constitutional principle #8: editor extends runtime, never replaces
- **ADR-101** — `kernel/graph-foundation` Tier-1 substrate (primitives, not runtime)
- **ADR-102** — Failure containment model with 5 classes
- **ADR-103** — Networking is authoritative CAD serialization, not replicated topology
- **ADR-104** — CAD kernel non-equivalence doctrine + capability surface
- **ADR-105** — WASM constitutional lock-in promoted to top-tier risk; per-category fallbacks documented

**New ADRs from v0.8:**

- **ADR-110** — `editor-state` as narrow coordination crate (5 categories: selection/hover/active-tool/modal-state/drag-drop). Coordination-not-authority rule. Five-category bound fixed at v0.8.
- **ADR-111** — Architecture freeze policy post-v0.8. Future first-class subsystems require: (1) demonstrated implementation pressure, (2) 3+ reproducible failure scenarios, (3) cost/benefit analysis vs alternatives, (4) justification why a smaller primitive (Resource, module, trait, CI rule) wouldn't suffice. Architecture serves implementation; implementation does not serve architecture.

ADRs land as files in `RGE/ADR/`.

---

## 17. References

- Existing references preserved.
- **CAD persistent-name + lineage literature (NEW v0.7):** Parasolid SDK persistent-attribute reference; ACIS topology naming papers; OpenCASCADE persistent-attribute documentation
- **Structural sharing literature:** Clojure, immer.js, im-rs (graph-foundation snapshot impl)
- **WGSL spec + wgpu shader compile pipeline (NEW v0.7):** for execution-domain GPU side
- **Failure-mode literature (NEW v0.7):** Erlang let-it-crash; Kubernetes restart policies; CAD-system recovery patterns

---

## 18. Companion documents

Spawned from Track F (Month 1) unless noted:

- `RGE/CONVENTIONS.md` — code-org rules
- `RGE/SCENE_MODEL.md` — entity-role catalog
- `RGE/PIE_MODEL.md` — Play-in-Editor lifecycle
- `RGE/UI_LAYOUT_SCHEMA.md` — RON workspace schema
- `RGE/PLUGIN_API.md` — Plugin trait + capability manifest
- `RGE/SCRIPT_BENCH_METHODOLOGY.md` — workload definitions
- `RGE/MATERIAL_MODEL.md` — graph IR (4-Authoring)
- `RGE/ANIM_MODEL.md` — five-crate split (4-Authoring)
- `RGE/PHYSICS_MODEL.md` — Rapier wrap (4-Foundation)
- `RGE/FILE_FORMATS.md` — RON schemas; `.rge-pak` byte layout
- `RGE/RENDERER_MODEL.md` — render graph, snapshot staging
- `RGE/STREAMING_MODEL.md` — async asset graph
- `RGE/MARKETPLACE_GOVERNANCE.md` — supply-chain detail
- `RGE/UNDO_REDO_MODEL.md` — Command Bus + two layers
- `RGE/MEMORY_GOVERNANCE.md` — per-subsystem owners
- `RGE/NETWORKING_PLAN.md` — authoritative CAD serialization (5-Scale primary)
- `RGE/CONTRIBUTING.md` — onboarding, ADR process
- `RGE/ADR/` — Architecture Decision Records
- `RGE/CAD_CORE_MODEL.md` — cad-core architecture (4-Geometry)
- `RGE/CAD_DETERMINISM.md` — best-effort discipline (post-v1)
- `RGE/EGUI_PRESSURE.md` — tracked workarounds
- `RGE/WASM_TOOLING_FALLBACKS.md` — per-category fallbacks
- **`RGE/CAD_TOPOLOGY_LINEAGE.md` (NEW v0.7)** — TopologyEvolution semantics; lineage graph design; reconciliation rules
- **`RGE/CAD_KERNEL_CAPABILITIES.md` (NEW v0.7)** — capability surface per kernel; non-portable ops; compile-error gate
- **`RGE/EXECUTION_DOMAINS.md` (NEW v0.7)** — four canonical domains; shared schema; cross-domain orchestration
- **`RGE/RECOVERY_MODEL.md` (NEW v0.7)** — five failure classes; per-subsystem declarations; fault-injection methodology
- **`RGE/GRAPH_FOUNDATION.md` (NEW v0.7)** — substrate primitives; what's in / what's out; discipline against god-substrate

---

## 19. v0.7 changelog (what changed from v0.6)

| Change | Driver | Where |
|---|---|---|
| §0.3 8th constitutional principle: editor extends runtime, never replaces | review CRITICAL | §0.3, ADR-100 |
| §0.3.1 Execution domains (CPU gameplay / GPU shading / GPU compute / Expression) | review CRITICAL | §0.3.1, ADR-099 |
| §1.5.4.3 Topology lineage graph with `TopologyEvolution` enum | review CRITICAL | §1.5.4.3, ADR-098 |
| §1.5.4.4 CAD kernel non-equivalence doctrine | review CRITICAL | §1.5.4.4, ADR-104 |
| §1.5.4.5 cad-projection internal split (6 modules) | review CRITICAL | §1.5.4.5, ADR-097 |
| §1.10.4 Extended entropy metrics (10 new) | review | §1.10.4 |
| §1.13 Failure containment model | review CRITICAL | §1.13, ADR-102 |
| §1.14 graph-foundation substrate (Tier 1) | review CRITICAL | §1.14, ADR-101 |
| §6.16 Promote to Command Bus framing | review CRITICAL | §6.16 |
| §6.17 Authoritative CAD serialization (not replicated topology) | review CRITICAL | §6.17, ADR-103 |
| Renderer realism reframe (industrial editor; photoreal post-v2) | review | §0.4, §4 |
| WASM constitutional lock-in promoted to top risk | review CRITICAL | §14, ADR-105 |
| §10.1 Tier-1 +`kernel/graph-foundation` (~14 crates) | cascade | §10.1 |
| §10.2 cad-projection split into modules; cad-topo-lineage internal | cascade | §10.2 |
| §13.6 lineage gates · §13.7 Command Bus CI lint · §13.10 entropy extensions · §13.12 failure gates · §13.13 execution domain gates · §13.14 graph-foundation gates | cascade | §13 |
| ADR-097 through ADR-105 (9 new) | various | §16 |
| 5 new companion docs | various | §18 |
| §1.4 audit triggers extended (graph reinvention, etc.) | cascade | §1.4 |
| §1.8 forbidden-dep rules: cad-projection internal split, graph-foundation usage | cascade | §1.8 |

**Rejected from review:** None. Reviewer made no Rhai-shaped suggestions.

**Self-acknowledged blind spots from this review pass:**
- **Graph-domain fragmentation.** Eight graph systems on a path to inventing identical primitives N times. I missed this entirely.
- **cad-projection accumulation pattern.** Split CAD from ECS in v0.6 but didn't anticipate the bridge becoming the new monolith.
- **Execution domains as a naming exercise.** Implicit in architecture; should have been explicit.
- "PersistentFaceId is replication-stable by design" was an over-claim — same pattern as v0.5 determinism over-claim.
- **Failure containment as a structural commitment.** Plan described happy paths thoroughly; didn't classify failures.
- **Editor extends runtime principle.** Constitutional principle missing despite the Command Bus pattern being half-implemented.

The §0.5 review cadence keeps surfacing structural insights of the same class as v0.6's CAD/ECS catch — patterns where domain X was being embedded inside domain Y in ways that would fight us at scale. Pattern recognition: each of these catches is "we have multiple things pretending to be one, or one thing pretending to be many."

## 19.5 v0.8 changelog (architecture freeze)

| Change | Driver | Where |
|---|---|---|
| §0.6 Architecture freeze policy | meta-review (abstraction addiction recognized) | §0.6, ADR-111 |
| §1.15 Editor-state coordination crate (narrow, 5 categories, coordination-not-authority rule) | review CRITICAL with discipline | §1.15, ADR-110 |
| §1.4 audit triggers: state mixing content with coordination · new first-class subsystem proposals must clear §0.6 gate | cascade | §1.4 |
| §10.2 Tier-2 +`editor-state` | cascade | §10.2 |
| §13.15 Editor-state gates (6) · §13.16 Architecture freeze gates (4 conditions enforced) | cascade | §13 |
| §14 +4 risks: editor-state content drift, category proliferation, abstraction addiction (named meta-risk), premature ossification | cascade | §14 |
| ADR-110, ADR-111 | various | §16 |
| Title bumped to "v0.8 — architecture freeze" | meta | header |

**Rejected from this review round:**
- Full 11-category editor-state subsystem (Option C). Over-architecture; violated §0.6 freeze conditions before it existed.
- Editor-state-graph using graph-foundation. Selection is mostly flat-set + scopes; no relational structure to justify.
- Collaborative cursors / undo-preview / transient-preview / viewport-state / workspace-state in editor-state. Already handled elsewhere or premature.

**Accepted:**
- Narrow editor-state (Option A — 5 cross-subsystem categories)
- Coordination-not-authority rule
- Architecture freeze policy as constitutional commitment

**Self-acknowledged blind spots from this round:**
- **Initial response was reflexive "yes, fold it in" without weighing freeze risk.** The reviewer had to push back to surface diminishing planning ROI.
- **Pattern of "promote to first-class" without a forcing-function test.** v0.6 (cad-core) and v0.7 (graph-foundation) had demonstrated collisions; v0.8 (editor-state) was forecasted from analogy. The line between the two wasn't being drawn.
- **Architectural exhaustion was named in v0.6 but kept growing through v0.7 anyway.** Naming a risk doesn't mitigate it; gating future additions does. §0.6 is the gate.

The freeze policy is the meta-fix: stops the planning loop from generating new architecture without implementation evidence.

---

## 20. Where the plan stands

Five rounds of external review (the last round triggered the freeze):

| Round | Biggest catch | Forcing function strength |
|---|---|---|
| Round 1 (v0.3 → v0.4) | ECS storage specialization, B-Rep topology IDs, async streaming, reflection UI hints, PIE participation | Demonstrated |
| Round 2 (v0.4 → v0.5) | undo/redo entirely missing, render snapshot staging, ECS-scene-graph honesty, compile-time realism, editor budgets, networking scaffolding | Demonstrated |
| Round 3 (v0.5 → v0.6) | **CAD/ECS impedance** (cad-core split) — load-bearing | Demonstrated structural collision |
| Round 4 (v0.6 → v0.7) | **graph-domain fragmentation** (graph-foundation), cad-projection split, execution domains, failure containment, editor-extends-runtime principle, authoritative CAD serialization | Demonstrated (8 graph systems already specified) |
| Round 5 (v0.7 → v0.8) | **editor-state coordination** (narrow), architecture freeze policy | **Forecasted** — accepted as narrow scope; freeze prevents further forecasted-only promotions |

The transition at round 5 is the meaningful one: from "respond to demonstrated structural collisions" to "freeze before forecasts drive further architecture."

The plan is converging. The remaining largest risks are:
- **Architectural exhaustion** from maintaining many unified abstractions simultaneously (§14)
- **Roadmap density vs staffing** (§11.5)
- **WASM constitutional lock-in** (§14, top-tier risk in v0.7)
- **graph-foundation becoming god-substrate** (NEW v0.7, §14, mitigated by discipline)
- **Editor/runtime semantic bifurcation** (NEW v0.7, §14, mitigated by principle #8 + Command Bus)

The constitution + review cadence + golden tests + dogfood rule + entropy metrics + Command Bus + graph-foundation discipline + failure containment + **architecture freeze (§0.6)** + **editor-state coordination-not-authority rule (§1.15)** together form the discipline that keeps these manageable. Without them, Unreal-scale architecture with indie staffing fails. With them, the moat (§0.2) is achievable.

## 21. Implementation pivot (v0.8 → execution)

**The architecture is frozen.** The next phase is execution.

What planning has produced (v0.1 → v0.8):
- 1300+ lines of architectural commitments
- 21 ADRs covering pillars, kernel tiers, scripting, CAD, undo, networking, failure, editor-state
- 21 parallel waves specified for Phase 4-Foundation (`WAVES.md`)
- ~14 Tier-1 + ~70 Tier-2 crates targeted

What planning **cannot** produce:
- Profiler traces showing real hot paths
- Invalidation bug scenarios from real edits
- Hot-reload failures across real component shapes
- Topology corruption cases from real CAD operations
- Undo edge cases from real user gestures
- Async race conditions from real concurrent activity
- GPU residency failures from real asset loads
- UI interaction friction from real usage

**Implementation reveals what planning cannot.** Further planning rounds are likely to be diminishing returns or actively harmful (the v0.7 → v0.8 cycle was 90% the latter, salvaged at the last moment by the freeze policy).

### Next steps

1. **Bootstrap (1 day, sequential):** Workspace skeleton per `WAVES.md` §0. All 21 crate directories created with stub `Cargo.toml` + `src/lib.rs`. Workspace root updated.
2. **21 parallel waves (3 calendar days at full parallelism):** W1 components · W2 macros-reflect+kernel/types · W3 editor-shell PIE · W4 wasmtime-engine · W5 ui-theme · W6 ui-icons · W7 ui-fonts · W8 editor-ui/menus · W9 editor-ui/layout · W10 editor-ui/dock · W11 physics · W12 audio · W13 input · W14 rge-data · W15 pak-format · W16 asset-store · W17 io-gltf · W18 io-image · W19 expr-wasm · W20 script-bench · W21 golden test projects.
3. **Integration phase (~5 days sequential):** Wire stubbed cross-crate deps; verify dogfood-rule contract; verify forbidden-dep DAG; verify build-governance gates pass.
4. **First demonstration:** PIE smoke test on a 100-entity scene with hot-reloaded Rust gameplay; theme/layout swap working; physics replay byte-identical.

After Foundation exits, revisit:
- Did editor-state's five categories prove sufficient or did real pressure surface a sixth?
- Did cad-projection's six-module split match real invalidation patterns?
- Did graph-foundation primitives serve all 8 graph systems uniformly?
- Did the determinism narrowing (gameplay-only at v1.0) hold under implementation?
- Did any §0.6 freeze conditions trigger?

These questions are answerable only with implementation evidence. The plan is done; the engine is not.
