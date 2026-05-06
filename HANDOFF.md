# RGE Handoff Document

> **Snapshot**: 2026-05-09 07:40. Continuation pointer for the next session.
>
> **Read first**: this file. Then [`Status.md`](./Status.md) (current snapshot) and [`change.md`](./change.md) (full history).

---

## Current state — one-page summary

| Pillar | State |
|---|---|
| Workspace tests | **1702 / 1702 pass** across 200 binaries (2 ignored intentionally hardware-gated) |
| Architecture lints | **9 / 9 PASS** exit 0 (forbidden-dep, split-exemption, no-utils, graph-foundation, editor-state-ownership, command-bus, projection-modules, kernel-isolation, failure-class) |
| `cargo +nightly fmt --check` | exit 0 |
| `cargo check --workspace --all-targets` | 0 errors, ~130 pre-existing ui-theme `missing_docs` warnings (deferred per Status.md) |
| Implementation footprint | **43 IMPLEMENTED / 3 PARTIAL / 48 EMPTY-STUB** of 94 workspace members (~46%) |
| Tier 1 kernel | **10 of 15 implemented**: types / diagnostics / events / app / schedule / ecs / audit-ledger / asset / graph-foundation / **plugin-host**. **5 stubs**: shared, asset-view, asset-streaming, io-scheduler, job-system |
| Phase 7 operator catalog | **Cuboid + Transform + Extrude + Revolve + Boolean** (5 operators) + topology-lineage prototype (D-7.4) |
| Failure-class exemptions | **23 of original 81 cleared** (58 remain — rollout debt; cleared as each crate gets first real impl) |
| Substantive non-rollout-debt exemption | **1**: `crates/editor-ui/src/layout/node.rs` graph-foundation NodeId rename TODO (file-doc'd as conceptually distinct from substrate NodeId; rename to `LayoutNodeId` later) |

## What just shipped (this session — completed work)

1. **Deep audit 2026-05-09 round 2** (5-parallel-agent pattern; 2-round corrective close):
   - **5 audits returned**: 1 CRITICAL + 7 HIGH + ~13 MEDIUM + 1 LOW.
   - **Round 1 inline batch** (orchestrator stale-reference fixes; zero test-count delta): audio plugin_adapter doc-links → physics_input_ledger; PLUGIN_API.md + PLUGIN_HOST_PATTERNS.md AuditLedger references (4 sites including quoted `resource_type` match string); physics test fn rename; physics_input_ledger.rs backtick fix; HANDOFF.md L414 self-contradiction; README.md 1695→1696 + 21→23 cleared + 60→58 remain; §18 CAD docs stale paths post-Phase-5-split (CAD_CORE_MODEL / CAD_TOPOLOGY_LINEAGE / CAD_CORE_KERNEL_ADAPTERS); KERNEL_AUDIT_LEDGER.md PhysicsInputLedger disambiguation callout; GRAPH_FOUNDATION.md §9 Check 2 subsection; kernel/asset DepGraph node_count docstring; 6 Phase 5 split #[allow] sites gained `reason=` field.
   - **Round 2 parallel dispatch** (3 agents in parallel; +6 net workspace tests):
     - **Lint test fixtures (CRITICAL + MEDIUM)**: graph_foundation_test 5→9 tests (Check 2 BTreeMap-K-K + HashMap-K-K + permission-map false-positive guard + substrate-exemption); kernel_isolation_test 7→8 tests (manifest-path-only branch via rge-loader/rge-saver names at crates/io-*/Cargo.toml). +5 workspace tests.
     - **cad_projection_smoke split (HIGH + MEDIUM)**: 987L → 565L (substrate-specific: 4 PIE/invalidation + 4 Pairing-6 SSoT regression) + NEW plugin_adapter_smoke.rs 477L (3 plugin-lifecycle + 1 sibling-panic isolation + PanickingTickPlugin fixture). 12 tests redistributed 8/4 (zero test logic changes). Largest cad-projection test file 565L (was 987L; 435L below cap). **4/4 Tier-2 plugin canary crates now follow `tests/plugin_adapter_smoke.rs` filename convention.**
     - **gfx RuntimeFault refactor + labeled-Tess cache-key test (HIGH × 2)**: gfx 427→444L (extracted `pub(crate) fn map_pipeline_err`; production + test call same helper — eliminates tautological-test regression); cad-core operator_graph.rs 737→769L (promoted `effective_hash_and_label` to pub + added `effective_hash_and_label_root` convenience wrapper); labeled_tessellation_pipeline.rs 215→441L (new test `transform_cache_key_distinguishes_labeled_vs_unlabeled_upstream` exercising recursive-hash-propagation + cache hit/miss; doc-comment acknowledges bitmap-fold "labeled vs unlabeled upstream" case requires a labeled-emitting operator that doesn't exist today). +1 workspace test.
   - **All audit findings closed**: 1 CRITICAL + 7 HIGH + ~13 MEDIUM + 1 LOW resolved.
   - **Workspace state**: 1696 → 1702 (+6 net). 9/9 lints PASS. fmt clean. 1 substantive lint exemption. 17/27 §18 docs.
2. **Physics AuditLedger option-(b) rename** (MEDIUM audit-1 carryover closure; orchestrator inline; pure rename — zero test-count delta):
   - **MEDIUM-batch Deliverable 2 carry-on closure**: prior STOP'd with clean divergence report; option (b) was the recommended lightest-touch path. Local stub `stubs::audit_ledger::AuditLedger` recorded physics-domain `PhysicsInput { Force / Impulse / JointMotor }` per-tick TickRecords; kernel substrate is generic event ledger with `EventKind { Action / CadCheckpoint / Custom(String) }`; no API-compatible swap. Option (b) = rename the type so it stops presenting as a "stub" of the kernel substrate.
   - **New module**: `crates/physics/src/physics_input_ledger.rs` (132L) — public; contains `PhysicsInput` enum + `TickRecord` struct + `PhysicsInputLedger` struct (renamed from `AuditLedger`). Module-level doc explains the divergence rationale + cross-refs ADR-114.
   - **Renames across production + tests**: `lib.rs` (run_tick signature) + `step.rs` (5 sites: import + record_current + physics_step + private fns) + `plugin_adapter.rs` (9 sites: import + take<> + contains<> + ContractViolation `resource_type: "AuditLedger"` → `"PhysicsInputLedger"` + test fixture) + 4 test files (imports + `AuditLedger::new()` → `PhysicsInputLedger::new()` + doc-comments).
   - **Module deletion**: `crates/physics/src/stubs.rs::audit_ledger` mod (~88 lines) replaced with 4-line redirect-comment + migration-map header rewritten with rationale paragraph. Stubs file shrunk; only `components_physics` + `kernel_events` stubs remain.
   - **ADR-114 amendment table updated**: `(World + AuditLedger)` → `(World + PhysicsInputLedger)`; added footnote explaining the rename and the divergence-from-kernel-substrate rationale.
   - 24/24 physics tests pass unchanged; 1696/1696 workspace tests pass; 9/9 lints PASS; fmt clean.
2. **MEDIUM batch dispatch** (bounded scope: dep-style + physics AuditLedger + clippy pedantic; 2 of 3 deliverables landed; 1 STOPPED with clean divergence report):
   - **Deliverable 1 dep-style normalization**: `rge-kernel-plugin-host` + `rge-kernel-diagnostics` deps normalized to `{ workspace = true }` in cad-projection + gfx Cargo.toml (physics + audio already compliant). cad-projection's `rge-kernel-ecs` + `rge-kernel-graph-foundation` still path-style — broader sweep deferred to next clean-up.
   - **Deliverable 2 physics AuditLedger**: **STOPPED with clean divergence report**. Local `crate::stubs::audit_ledger::AuditLedger` records physics-domain `PhysicsInput { Force / Impulse / JointMotor }` per-tick TickRecords; kernel substrate is generic event ledger with `EventKind { Action / CadCheckpoint / Custom(String) }`. No API-compatible swap. **Recommended option (b)**: rename local stub `stubs::audit_ledger::AuditLedger` → `physics::PhysicsInputLedger` to stop presenting it as a "stub" of the kernel ledger; closes audit-1 carryover without forcing structural migration. Decision pending.
   - **Deliverable 3 clippy pedantic in canary tests**: 8 warnings resolved (6 audio + 2 physics canary tests; backticks + reason-annotated allow). Pre-existing pedantic in physics lib + audio lib (~17 must_use_candidate + doc-backticks) noted as out-of-scope.
   - Workspace 1696/1696 unchanged; 9/9 lints PASS; fmt clean.
2. **graph-foundation lint extension + asset-store DepGraph migration** (HIGH audit carryover closures + bonus catch):
   - **Lint extension** (orchestrator inline; `tools/architecture-lints/src/graph_foundation.rs` 186 → 316L). Added Check 2: detect `BTreeMap<K, BTreeSet<K>>` and `HashMap<K, HashSet<K>>` struct field shapes via `syn` AST walk; uses `syn::Type`'s native `PartialEq` (via `extra-traits` feature) for K==V comparison. False-positive guard: `BTreeMap<UserId, BTreeSet<Permission>>` (different types) NOT flagged.
   - **Bonus catch**: lint immediately surfaced `crates/asset-store/src/dependency.rs::DepGraph` — same `BTreeMap<AssetId, BTreeSet<AssetId>>` adjacency pattern audit-1 found in kernel/asset (not in original audit scope). Forward + reverse fields detected.
   - **asset-store migration** (background agent; mirrors kernel/asset Option B template). `dependency.rs` 425 → 502L: 2 BTreeMap fields collapsed to single `inner: Graph<AssetId, ()>` with content-derived NodeId via `NodeId::from_bytes(asset_id.raw())`. Added `rge-kernel-graph-foundation = { workspace = true }` dep. Public API surface bit-identical (every method retained). 62/62 asset-store tests pass unchanged. **graph-foundation exemption for asset-store/src/dependency.rs DELETED** post-migration.
   - **Substantive lint exemption count**: 1 (only editor-ui/LayoutNodeId-rename-pending remains; was 2 with asset-store transient).
   - **2 HIGH audit findings closed** with bonus catch.
2. **Deep-audit corrective round** (3 parallel dispatches; +27 net workspace tests; all 4 audit findings closed):
   - **forbidden-dep rge- prefix fix + lint test fixtures** (CRITICAL audit-5 closure): `forbidden_dep.rs` 175 → 699L (Approach A: prefix literals with `rge-`); refactored rule-checking into pure helper; 22 inline fixture tests added (one PASS + one FAIL per rule + real-workspace regression). **Discovery**: pre-existing `tests/forbidden_dep_test.rs` had 6 fixtures but used bare names — green for the wrong reason; updated to rge-prefixed. Lint crate 69 → 91 tests; workspace 1668 → 1691. Rules 3-6 now genuinely fire; real workspace 0 violations confirmed. **NEW carryover finding**: `kernel_isolation.rs` has same-class rge- prefix bug (`pkg.name.starts_with("io-")` is dead code).
   - **Test-coverage corrective pack** (CRITICAL audit-2 closures; 5 gaps closed): cad-projection canary gained `cad_projection_plugin_isolation_with_sibling_panic` + `PanickingTickPlugin` sibling fixture (Gap 1, achieves canary parity); `cross_substrate_determinism.rs` PIE round-trip rewritten as 100-iter loop (Gap 2, corrects single-shot claimed-100-iter); gfx gained unit-level RuntimeFault mapping test (Gap 3, end-to-end limitation documented since `TrianglePipeline::new` is infallible at csgrs-equivalent stability); `host.rs` 1766 → 1899L gained `init_all_detects_resource_leak` + `shutdown_all_detects_resource_leak` (Gap 4); new file `crates/cad-core/tests/labeled_tessellation_pipeline.rs` 214L (Gap 5, Approach (b) — manually constructed labeled `Tessellation` via `with_labels` public API, fed through `BooleanOp::evaluate` which auto-dispatches to labeled path, then through Transform which strips labels). Workspace 1691 → 1695. **Carryover finding**: host.rs at 1899L approaching natural split point.
   - **kernel/asset DependencyGraph migration** (HIGH audit-1 closure): `dependency_graph.rs` 360 → 427L. Decision **Option B**: `Graph<AssetId, ()>` with content-derived NodeId via `NodeId::from_bytes(asset_id.raw())` (mirrors cad-core::OperatorGraph precedent); EdgeId derived via `EdgeId::from_bytes(src.0 \|\| dst.0)`. New dep: `rge-kernel-graph-foundation = { workspace = true }` (Tier-1 → Tier-1 allowed per forbidden-dep). Public API surface bit-identical (every method retained verbatim). 53/53 kernel/asset tests pass unchanged. Cycle detection preserved as local DFS (graph-foundation::Graph doesn't detect cycles per cad-core precedent). **Suggested follow-up**: extend graph-foundation lint to flag `BTreeMap<K, BTreeSet<K>>` adjacency-pair shapes inside Tier-1 crates (current lint only forbids redefining NodeId/EdgeId/StableHash, which is why DependencyGraph slipped through).
2. **Phase 5 proactive split** (URGENT corrective dispatch from deep audit 2026-05-09; pure-refactor — zero test-count delta):
   - **CRITICAL closure**: audit-5 verified `boolean.rs` was at 1000L hard cap; one functional edit away from lint failure (confirmed empirically when ADR-104-alignment edit pushed it to 1008L before being compacted back to 1000L). Phase 5 split decomposes 3 cap-pressed single-files into 14 sub-module files.
   - **`operators/boolean/`** (5 files): `mod.rs 163L` + `csgrs_bridge.rs 247L` + `labeled_path.rs 69L` + `unlabeled_path.rs 36L` + `tests.rs 539L`. Original `boolean.rs 1000L` deleted. `BooleanMode::discriminant()` promoted private→`pub(super)` so tests sub-module can drive `structural_hash`. Internal-only API change.
   - **`operators/revolve/`** (4 files): `mod.rs 307L` + `full_path.rs 66L` + `partial_path.rs 96L` + `tests.rs 550L`. Original `revolve.rs 991L` deleted. Full-2π path (no caps; concave allowed) split from partial-revolution path (fan-triangulated start/end caps; convexity required).
   - **`topo_lineage/infer/`** (5 files): `mod.rs 91L` + `label_by_plane.rs 119L` + `infer_unlabeled.rs 156L` + `infer_labeled.rs 134L` + `tests.rs 535L`. Original `infer.rs 990L` deleted. csgrs-metadata-passthrough fast path split from plane-equation-matching heuristic path.
   - **All 183 cad-core tests pass unchanged** via `super::*` imports in tests sub-modules. Public API surface bit-identical (lib.rs re-exports + topo_lineage/mod.rs re-exports keep working).
   - **Cap-watch pressure RELIEVED**: largest single file post-split is 550L (revolve/tests.rs) with 450L headroom. NO SPLIT-EXEMPTION annotations needed across the 14 new files. Future functional edits have ample room.
2. **Deep audit 2026-05-09 (5-parallel-agent pattern)** + inline corrections + audit-debt registry update:
   - **5 audits ran in parallel**: architectural coherence / test coverage smells / doc drift / code smells / cross-architecture coherence. Key findings consolidated below.
   - **Inline corrections landed**:
     - **HANDOFF.md / Status.md / change.md**: prior claim "warning-zone trio shrunk to 898L/904L/930L after fmt-canonical state" was a measurement error that propagated through multiple updates. Actual: **boolean.rs 999L / revolve.rs 991L / infer.rs 990L** per audit-5 `wc -l`. Phase 5 split was wrongly claimed "demoted to opportunistic" — it is **URGENT**.
     - **change.md correction**: prior claim "cross_substrate_determinism.rs runs 100-iter PIE round-trip soak" was wrong — test is single-shot. Phase 3 dispatch brief claimed 100-iter, agent shipped single-shot, orchestrator did not verify.
     - **ADR-098 §"Implementation guidance" lines 96-97**: documented wrong public API signatures for `label_by_plane` / `infer_lineage` (predates HIGH #3 unified-Tessellation collapse). Fixed inline to match source-truth.
     - **README.md line 41**: stale `host.rs at 1743L` (pack-4 correction propagated to HANDOFF/Status/§18 doc but not README). Fixed inline to 1766L.
     - **boolean.rs capability-surface doc-block**: aligned to ADR-104's 6 canonical fields (was missing `concave_input_supported / arity / output_labeled_when_input_labeled`; carrying unauthorized `healing_strategies: none`). My initial ADR-104 alignment edit pushed boolean.rs to **1008L tripping the cap** before being compacted back to **exactly 1000L** — confirms audit-5's Phase 5 urgency finding empirically.
   - **Open audit findings (corrective dispatches pending)**:
     - **CRITICAL — forbidden-dep lint rules 3-6 are DEAD CODE** (audit-5): lint compares `pkg.name == "cad-core"` against unprefixed names but workspace uses `rge-` prefix. Rules 3-6 NEVER fire. PASS status is meaningless for those rules. Compounded by zero test fixtures on the lint impl.
     - **CRITICAL — Phase 5 proactive split URGENT** (audit-5): boolean.rs at exactly 1000L hard cap; one functional edit away from lint failure.
     - **CRITICAL — cad-projection canary missing PanickingTickPlugin sibling + multi-tick fixtures** (audit-2): 3 of 4 canaries have them; cad-projection doesn't.
     - **CRITICAL — PIE round-trip is single-shot, not soaked** (audit-2): cross_substrate_determinism.rs needs `for _ in 0..100` wrap to match the dispatch brief's claim.
     - **HIGH — kernel/asset::DependencyGraph reinvents graph storage** (audit-1): BTreeMap<AssetId, BTreeSet<AssetId>> instead of building on kernel/graph-foundation::Graph<N,E>; PLAN §1.14 spirit violation.
     - **HIGH — gfx RuntimeFault path advertised but never tested** (audit-2): doc says "pipeline-build failure surfaces as RuntimeFault"; zero tests fire it.
     - **HIGH — Init/Shutdown resource-leak detection paths untested** (audit-2): only tick has leak-detection coverage; init+shutdown leak diagnostics are dead code.
     - **HIGH — Labeled-Tessellation flowing through Boolean→Transform pipeline untested** (audit-2): Phase 2 cache-key extension never exercises labeled-input → label-stripping integration.
     - **HIGH — Cargo.toml dep hygiene drift** (audit-4): 4 canaries use 3 different dep-style patterns (path / mixed / workspace=true).
     - **MEDIUM** items: physics canary takes local stub AuditLedger not kernel substrate; ADR-104 healing_strategies addition (now resolved); `#[allow(...)]` reason= convention not enforced; clippy pedantic warnings in canary tests; csgrs catch_unwind shield's recovery branch never exercised; PluginError × PluginPhase 15-cell matrix has gaps.
2. **§18 docs pack 6** (3 more substrate docs; pure-docs — zero test-count delta; 726L total):
   - **`docs/§18/KERNEL_AUDIT_LEDGER.md`** (227L): companion to PLAN §6.16.6 + §1.13; sibling to EDITOR_ACTIONS_COMMAND_BUS.md (consumer). Documents `EventId::compute` (source-truth: takes `&EventKind` not `&str` as brief assumed); `AuditLedger` methods (`record / set_cursor / cursor / redo_stream / undo_stream` per source-truth — NOT `append / cursor_advance / cursor_position / iter_from_cursor` as brief listed); replay handler with halt-on-false; cursor pattern for undo/redo projection; failure-class kernel-fatal nuance (integrity-violation = kernel-fatal; benign corruption recovery = snapshot-recoverable per lib.rs lines 22-25); determinism gate cross-ref to W11 1000-tick replay byte-identical; consumer surface (editor-actions Action commits via EventId).
   - **`docs/§18/KERNEL_APP_FRAME_LOOP.md`** (267L): companion to PLAN §6 frame loop + §1.5.2; sibling to KERNEL_PLUGIN_HOST_LIFECYCLE.md. Documents `FramePhase` (**6 phases** per source-truth: `Input / FixedSim / Update / LateUpdate / StageRender / EndFrame` — NOT 5 phases as brief assumed); `FixedStepAccumulator` Fiedler pattern; `App` + `AppBuilder` (**NO `add_system` on builder; NO `run()` on App** per source-truth — systems supplied via per-frame `phase_runner` closure; caller drives via `run_frame` / `run_frames`); `FrameStats [f64; 16]` ring buffer for allocation-free per-frame stats; 60Hz allocation-free steady-state; diagnostics integration for frame-stat anomalies.
   - **`docs/§18/CAD_CORE_KERNEL_ADAPTERS.md`** (232L): companion to PLAN §1.5.4.4 + ADR-104 + ADR-113-deferred; sibling to CAD_CORE_MODEL.md + CAD_TOPOLOGY_LINEAGE.md. Documents non-equivalence doctrine + capability-surface doc-comment-canonical surface per ADR-104 §"Decision sub-decision 1" + materialization triggers (editor-ui operator-picker / second CAD kernel / capability-aware tessellation cache); **NO `KernelAdapter` trait exists yet** — csgrs is called directly from `BooleanOp::evaluate` (source-truth correction; trait flagged as future design-intent not current API); future truck adapter design space for ADR-113-deferred materialization; `kernel_id` discriminant flagged as ADR-104 §"Initial field set" gap.
   - **6 source/spec inconsistencies surfaced + documented in source-truth-wins fashion** (mirrors packs 3/4/5).
   - **§18 companion-doc count: 17 of 27** (was 14; +3 from this dispatch). Total §18 LoC: 4817.
2. **§18 docs pack 5** (3 more substrate docs; pure-docs — zero test-count delta; 872L total):
   - **`docs/§18/EDITOR_ACTIONS_COMMAND_BUS.md`** (332L): companion to PLAN §6.16; sibling to KERNEL_ECS_WORLD.md. Documents Action trait (**6 methods per source-truth**: `apply / revert / merge / name / id / payload` — `id` mandatory no default; `payload` default = name as bytes; NOT 4 as brief assumed); `ActionResult` error type with `ApplyFailed(String)` / `RevertFailed(String)` / `MissingEntity(EntityId)` variants (NOT `ActionError`); `ActionId` for coalesce target identity; `CompoundAction` atomic rollback; `UndoStack.entries: Vec<BusEntry>` where `BusEntry` is `#[non_exhaustive]` enum with one variant today (`Action(Box<dyn Action>)`) so future `CadCheckpoint` per §6.16.4 can co-exist (NOT `Vec<Box<dyn Action>>`); `SaveMark` + `CoalesceWindow` 500ms-window; `AuditLedger` projection via `EventId(BLAKE3(kind+payload))`; `command-bus` architecture lint actively enforces direct-mutation-API imports outside `crates/editor-actions`.
   - **`docs/§18/KERNEL_EVENTS_CHANNEL.md`** (263L): companion to PLAN §1.7 + §6 frame loop; sibling to KERNEL_DIAGNOSTICS.md. Documents `EventChannel<E>` double-buffer with `pending: VecDeque<E>` + `delivered: VecDeque<E>` (NOT `front: Vec<E>` + `back: Vec<E>` as brief assumed); `EventBus` storage `HashMap<TypeId, Box<dyn AnyChannel>>` where `AnyChannel` is a private trait (NOT `BTreeMap<TypeId, Box<dyn Any+Send>>`); `SubscriptionId` advisory-only (NO callbacks; NO `bus.iter<E>(SubscriptionId)` — consumers call `bus.channel::<E>()?.iter_current()` directly); frame-queued delivery via `advance_frame` swap; **NO overflow/drop semantics — channels are unbounded VecDeques**; the diagnostic emitted by `advance_frame` is `Severity::Info` ("events: advanced channel `{name}` with {count} pending event(s)") NOT a Warning about dropped events.
   - **`docs/§18/GFX_RENDER_TIER.md`** (277L): companion to PLAN §1.5.2 + §6 frame loop; sibling to PLUGIN_HOST_PATTERNS.md (GfxPlugin canary consumer) + PIE_SNAPSHOT.md (future participant). Documents `GfxContext` / `HeadlessTarget` / `FrameRecorder` + `ReadbackBuffer` / `Vertex+VertexBuffer+Mesh` / `Transform` UBO / PBR-lite types / GfxPlugin canary cross-ref Pattern B / full wgpu 29 quirks list / pending Phase 6 work (frame-graph minimal / render-snapshot separation / material-runtime + PSO cache / 60fps simple-scene golden gate).
   - **4 source/spec inconsistencies surfaced + documented in source-truth-wins fashion**: (a) Action trait 6 methods not 4; (b) ActionResult variants not ActionError; (c) UndoStack uses non_exhaustive BusEntry enum; (d) kernel/events uses VecDeque + HashMap + advisory-only subscriptions + NO overflow semantics + Info-severity advance_frame diagnostic.
   - **§18 companion-doc count: 14 of 27** (was 11; +3 from this dispatch). Total §18 LoC: 4091.
2. **§18 docs pack 4** (3 more substrate docs; pure-docs — zero test-count delta; 879L total):
   - **`docs/§18/KERNEL_ECS_WORLD.md`** (326L): companion to PLAN §6.13 + §1.5.2; sibling doc to PIE_SNAPSHOT.md (uses `World::serialize_snapshot` for the world_bytes layer). Documents ULID-based EntityId, archetype-store (`HashMap<EntityId, ArchetypeLocation>` + `Vec<Archetype>` per source-truth — NOT BTreeMap as brief assumed), three relation storages (Tree without cycle detection per source-truth / DenseLinear / Sparse), Changed<T> per-archetype tick + Mut<T> Drop-bumped, mutation surface, `SnapshotComponent` trait with 1 method `snapshot_name` + supertrait bounds (NOT 3 methods as brief assumed), RGES envelope format, performance characteristics (100k spawn + 10k mutate in 0.24s; 10k-entity round-trip 13.6ms in --release vs 500ms gate = 36× headroom).
   - **`docs/§18/KERNEL_PLUGIN_HOST_LIFECYCLE.md`** (297L): companion to ADR-114 + PLAN §10.4 + §1.13. Documents host SIDE of plugin substrate (deeper than PLUGIN_API.md / PLUGIN_HOST_PATTERNS.md). PluginRecord state machine (5 states per source-truth: Pending/Initialized/Failed/ShuttingDown/Shutdown — Initialized covers both pre-first-tick and post-first-tick stable operation; NOT 6 states with distinct Active as brief assumed). InitReport / TickReport / ShutdownReport with `String` failure-side payload (NOT `PluginError` because PluginError doesn't impl Clone — host pre-formats; source-truth correction). catch_unwind shield + resource-leak detection invariant via BTreeSet<TypeId> snapshot diff. Auto-emit policy. host.rs at **1766L** (corrected from stale 1743L claim) with SPLIT-EXEMPTION annotation. LIFO shutdown rationale + plugin-fatal isolation + "untrusted execution domains" framing.
   - **`docs/§18/EDITOR_STATE_MODEL.md`** (256L): companion to PLAN §1.15. Documents coordination-not-authority pattern (editor-state COORDINATES selection/hover/tool state across panels but does NOT own authoritative content); `Selection` BTreeSet<EntityId>; `Hover` BTreeMap<PanelId, EntityId>; `ActiveTool` enum (Select/Translate/Rotate/Scale/Brush); `ModalState` + `DragDrop` stubs per IMPLEMENTATION.md "delay until demonstrated demand"; `PanelId` opaque newtype; **editor-state-ownership architecture lint enforcement** (both halves: types only in editor-state + no upward imports of authoritative content; lint PASS 0 violations 0 exemptions); W03 → editor-state migration history brief.
   - **5 source/spec inconsistencies surfaced + documented in source-truth-wins fashion** (mirrors pack-3): (a) World archetype store HashMap<EntityId, ArchetypeLocation> + Vec<Archetype> not BTreeMap-of-BTreeMap; (b) TreeRelationStorage has no cycle detection; (c) SnapshotComponent has 1 method + supertrait bounds not 3 methods; (d) PluginRecord has 5 states without distinct Active; (e) InitReport/TickReport/ShutdownReport carry String failure-side payload not PluginError. **Plus 1 quantitative correction**: host.rs is 1766L not 1743L (referenced in HANDOFF.md / Status.md / change.md prior entries; stale references corrected).
   - **§18 companion-doc count: 11 of 27** (was 8; +3 from this dispatch). Total §18 LoC: 3219.
2. **§18 docs pack 3** (3 more substrate docs; pure-docs — zero test-count delta; 893L total):
   - **`docs/§18/CAD_PROJECTION.md`** (321L): companion to PLAN §1.5.4.5; pairs with CAD_CORE_MODEL.md sibling. Documents the cad-graph ↔ ECS bridge: BRepHandle (cad_node DROPPED per MEDIUM #4 closure 2026-05-08), EntityCadMap bidirectional with private EntityIdProxy serde bridge, ProjectedMesh + ProjectedMeshId + free `project()` fn, ProjectionCache with dirty-tracking + observe_checkpoint, CadProjection orchestrator + tick + accessors (node_for/entity_for/remap_entity) + `validate_handles` for divergent-restore orphan detection, SnapshotParticipate impl with ParticipantId `cad-projection.brep-handles`, CadProjectionPlugin Tier-2 dogfood cross-ref.
   - **`docs/§18/PIE_SNAPSHOT.md`** (290L): companion to PLAN §13.2 + §6.13. Documents PIE (Parameter/Identity/Entity-state) composition substrate. `SnapshotParticipate` trait (3 methods: `participant_id` / `capture` / `restore` — source-truth, NOT 4 as dispatch brief assumed). `ParticipantId` convention `<crate-name>.<subsystem>`. `PieSnapshot` aggregator with deterministic envelope (RGEP magic + LE numerics + sort by ParticipantId). Format policy (RON for human-readable; postcard for binary). Current participants registry table. Divergent-restore tolerance via `CadProjection::validate_handles`.
   - **`docs/§18/KERNEL_DIAGNOSTICS.md`** (282L): companion to PLAN §1.7. Documents unified diagnostic-routing substrate. `Diagnostic` struct (5 fields: `severity / failure_class / span / message / suggestion` — source-truth, NOT 7 as brief assumed; no `DiagnosticId`). `Severity` enum (4 variants: `Suggestion(0) / Info(1) / Warning(2) / Error(3)` — source-truth, NOT 6 as brief assumed). **Plugin-host auto-emit policy** per audit-2 Phase 0 + LOW #5 (`ContractViolation` → Warning; `RuntimeFault`/`Panic`/leak → Error; `unregister`-shutdown → Warning). Cross-workspace consumer table.
   - **3 source/spec inconsistencies surfaced + documented in source-truth-wins fashion**: (a) Diagnostic has 5 fields not 7; (b) Severity has 4 variants not 6; (c) SnapshotParticipate has 3 methods not 4. These are now canonical reference data.
   - **§18 companion-doc count: 8 of 27** (was 5; +3 from this dispatch). Total §18 LoC: 2340.
2. **§18 docs pack 2** (3 new substrate docs; pure-docs — zero test-count delta; 876L total):
   - **`docs/§18/GRAPH_FOUNDATION.md`** (224L): companion to PLAN §1.14. Documents `kernel/graph-foundation` primitives — `NodeId(u128)` / `EdgeId(u128)` (BLAKE3-derived; hex-string serde) / `StableHash` trait / `Graph<N, E>` / `GraphSnapshot` / `GraphDiff` / `Invalidation` (BFS propagation + visited-set dedup) / `VizAdapter` + `NodeView` + `EdgeView`. Includes `graph-foundation` architecture-lint enforcement details + 1 substantive exemption (LayoutNodeId rename pending in editor-ui). Tight at 224L (vs 250-400L target) — every primitive covered without filler.
   - **`docs/§18/CAD_TOPOLOGY_LINEAGE.md`** (279L): companion to ADR-098 + PLAN §1.5.4.3. Documents v0 lineage substrate — quick concept (Preserved/Split/Merged/Deleted/Reinterpreted), public types (`TopologyFaceId` with `DEGENERATE = u64::MAX` sentinel, `TopologyEvolution`, `LineageEdge`, `LineageGraph`, `LineageError`), unified Tessellation with `face_labels: Option<Vec<TopologyFaceId>>` per HIGH #3 collapse, hybrid csgrs-metadata-passthrough + plane-equation-fallback path, `label_by_plane` + `infer_lineage` public APIs, TessellationCache labeled-state defense (Phase 2 substrate via `Operator::output_is_labeled` + `effective_hash_and_label`).
   - **`docs/§18/CAD_CORE_MODEL.md`** (373L): companion to PLAN §1.5.4 + ADR-104 + ADR-112; references ADR-098. Documents three-layer model (OperatorGraph → CheckpointHistory → TessellationCache layered on `graph-foundation::Graph<OperatorNode, EdgeKind>`) + `Operator` trait (op_kind/structural_hash/arity/evaluate/output_is_labeled) + 5-operator catalog (Cuboid + Transform + Extrude + Revolve + Boolean) + `CadGraph` `SnapshotParticipate` impl per PLAN §13.2 (CRITICAL #1 closure) + capability surface per ADR-104 (doc-comment-canonical until trigger).
   - All 3 docs cross-link reciprocally + cite ADRs by ID + section + use the 5-row header table convention from PLUGIN_API.md/PLUGIN_HOST_PATTERNS.md.
   - **§18 companion-doc count: 5 of 27** (was 2; +3 from this dispatch). Total §18 LoC: 1447.
   - Followups discovered: GRAPH_FOUNDATION.md tight at 224L (substrate small enough that filler would be filler — extends naturally when LineageGraph migration lands or VizAdapter gets first consumer); no source/ADR cross-reference inconsistencies; LayoutNodeId rename still pending (already tracked in exemption reason).
2. **Physics + Audio failure-class exemptions cleared** (orchestrator inline edit; pure doc-comment + exemptions registry; zero functional code; tests unchanged at 1668):
   - Both crates have first real implementations via plugin canaries (physics::Plugin + audio::Plugin both shipped 2026-05-08), satisfying the audit-1 "Phase 1.x rollout debt - declaration added when crate gets first real implementation per IMPLEMENTATION.md" condition.
   - `crates/physics/src/lib.rs`: added `//! Failure class: snapshot-recoverable` per PLAN §1.13 + §1.6.8 (Replay-Stable v1.0 same-machine determinism implies snapshot-based recovery; physics state is part of PIE; matches cad-core / cad-projection / editor-actions classification).
   - `crates/audio/src/lib.rs`: added `//! Failure class: recoverable` per PLAN §1.13 (audio failures transient — `ManagerError::UnknownClip` from canary's RuntimeFault path doesn't affect PIE state; recovery is in-place; matches gfx / kernel/diagnostics / kernel/ecs classification).
   - `tools/architecture-lints/exemptions.toml`: removed both physics + audio failure-class entries.
   - **23 of original 81 rollout-debt exemptions cleared cumulatively** (was 21); 58 remain. failure-class lint now actively enforces declarations on both crates.
2. **ADR-114 four-substrate amendment update** (orchestrator inline edit; pure-docs — zero test-count delta; ADR-114 214L → 260L):
   - Appended `## Amendment 2026-05-08 — Four-substrate validation` section (+46L). Four-substrate proof table (cad-projection / gfx / physics / audio with file-paths + dates + resource families); empirical Send confirmation via permanent `assert_send_static<T>()` lib test; nuance documenting Kira's wrapper layer satisfies the cpal-style-handle "wrapper newtype" pattern (NOT plugin-host machinery — clarifies the three-substrate amendment's anticipation).
   - **Pattern A + fallible inner work cross-canary intersection table** mapping all four canaries' tick shape × inner-work failure mode: cad-projection (Pattern A + fallible — unit-tested only); gfx (Pattern B + fallible — statically reachable, runtime-unreached); physics (Pattern A + INFALLIBLE — `RuntimeFault` reserved-but-unused per no-RuntimeFault subcase); audio (Pattern A + fallible — **first canary exercising Pattern A + fallible inner work end-to-end at integration-test level**).
   - Updated followup list: three-substrate-amendment audio-canary followup marked RESOLVED 2026-05-08 inline; Pattern C reserved as future-amendment material if fifth-or-later canary triggers it; CI-tier `AudioManager<DefaultBackend>` smoke deferred until CI gains audio-device capability; editor-ui canary remains open (defer until editor-ui Phase 5 stabilises singleton shape).
   - ADR-114 now carries TWO amendments: 2026-05-08 three-substrate validation (52L) + 2026-05-08 four-substrate validation (46L). Original ADR body (162L) unchanged. Total file 260L.
   - **ADR-114 design proof for owned-resources-handoff is now CLOSED across four structurally-distinct resource families** with zero kernel-side substrate change between any of them. Pattern dichotomy stable: Pattern A (cad-projection / physics / audio) vs Pattern B (gfx). Send taxonomy stable.
2. **audio::Plugin canary** (ADR-114 four-substrate-proof closure; +19 tests; audio 28 → 48; workspace 1649 → 1668):
   - **Send finding (central data point)**: `AudioManager<MockBackend>` + `AudioManager<DefaultBackend>` + `AudioFrame` all `Send + 'static` (verified empirically via permanent `assert_send_static<T>()` lib test). Kira's wrapper around cpal renders the `cpal::Stream` non-Send concern **moot at the public-API layer** — Kira keeps the platform handle on a backend-owned thread and routes commands through a `Send` channel. Plugin canary required NO Mutex / NO Arc / NO unsafe.
   - **`crates/audio/src/plugin_adapter.rs`** (609L; 13 unit tests at file foot) mirrors prior canaries: `id() = AUDIO_PLUGIN_ID`; `init = Ok(())`; `tick` takes `AudioManager<MockBackend>` + `AudioFrame` via `ctx.take<T>()`, advances one mix step via `audio_schedule_step`, puts both back; `shutdown = Ok(())`. ContractViolation paths: missing `AudioManager` → `ContractViolation { resource_type: "AudioManager" }`; missing `AudioFrame` after `AudioManager` already taken → idempotent put-back validated.
   - **First canary combining Pattern A (straight-line tick) + fallible inner work** — `audio_schedule_step` returns `Result<(), ManagerError>`; `ManagerError::UnknownClip` → `PluginError::RuntimeFault { reason }`. Distinct from physics's no-RuntimeFault subcase (rapier3d step infallible) and gfx's lazy-build pattern. The `RuntimeFault` mapping is finally exercised end-to-end at the integration-test level.
   - **`crates/audio/tests/plugin_adapter_smoke.rs`** (521L; 6 integration tests + `PanickingTickPlugin` sibling fixture): full lifecycle through `PluginHost`; ContractViolation-missing-`AudioManager`; ContractViolation-missing-`AudioFrame`-with-put-back-idempotent; resources-put-back invariant; multi-tick determinism (audio buffer advance + schedule consumption); multi-plugin isolation.
   - `crates/audio/src/lib.rs`: added `pub mod plugin_adapter; pub use plugin_adapter::{AudioFrame, AudioPlugin, FrameRecord, OwnedAudioSchedule, AUDIO_PLUGIN_ID};`. `crates/audio/Cargo.toml`: added `rge-kernel-plugin-host` + `rge-kernel-diagnostics` workspace deps.
   - **ADR-114 four-substrate proof CLOSED**. The amendment 2026-05-08 anticipated either (a) clean fourth-substrate confirmation OR (b) real boundary requiring Pattern C: Arc<Mutex<T>>. Outcome is (a) cleanly — but with a meaningful nuance: the "wrapper newtype" pattern anticipated for cpal-style handles is satisfied by the audio engine library (Kira), NOT by plugin-host machinery. **§10.4 dogfood-rule canary count: 4 of N** (cad-projection + gfx + physics + audio).
   - Followups discovered: (a) ADR-114 amendment update documenting four-substrate proof closure + Pattern-A-with-fallible-inner-work intersection (~30L append); (b) PLUGIN_HOST_PATTERNS.md optional §10 expansion citing audio as first Pattern-A-with-fallible-inner-work canary; (c) AudioManager<DefaultBackend> CI-tier canary parity (today's canary is mock-only).
2. **§18 companion docs first-batch + ADR-114 amendment** (pure-docs — zero test-count delta; first §18 directory landing):
   - **ADR-114 amendment** (`docs/adr/ADR-114-pluginctx-owned-handoff.md` 162L → 214L; +52L). Appended `## Amendment 2026-05-08 — Three-substrate validation` section: three-substrate proof explicit (cad-projection/gfx/physics — zero kernel-side substrate change between them); resource-Send taxonomy validated (no Mutex/Arc/unsafe needed in any canary; ADR-114 §Decision claim that owned-handoff is the only safe alternative confirmed); **pattern dichotomy documented** — straight-line tick (cad-projection + physics) vs lazy-build-on-first-tick (gfx because `TrianglePipeline` requires `&GfxContext`); **no-RuntimeFault subcase documented** (rapier3d step infallible — variant remains reserved for future fallible-step extensions but isn't exercised v0); **Followup #1 (gfx::Plugin canary) marked RESOLVED 2026-05-08**; new followup added (audio canary as 4th-substrate cross-check on cpal::Stream-style RAII handles which surface a different Send-shape).
   - **`docs/§18/PLUGIN_HOST_PATTERNS.md`** (282L) — Pattern-level guide for plugin authors. Sections: Overview / The owned-handoff contract (5-line lifecycle) / Pattern A straight-line tick (cad-projection + physics; pseudocode 15-25L) / Pattern B lazy-build-on-first-tick (gfx; pseudocode 15-25L) / Error classification cheat-sheet (table mapping ContractViolation/RuntimeFault/Panic/InitFailed/ShutdownFailed to scenarios + auto-emit Severity + caller/plugin/host blame attribution) / Idempotent-failure-put-back invariant / Multi-plugin isolation guarantees + PanickingTickPlugin sibling-fixture pattern / Test recipe template (12-16-test split common to all 3 canaries) / References.
   - **`docs/§18/PLUGIN_API.md`** (289L) — API-surface reference for kernel/plugin-host. Plugin trait + PluginContext API + PluginError 5-variant taxonomy + PluginPhase enum + PluginHost API + Layering invariants (load-bearing for `forbidden-dep` lint). Note: kernel/plugin-host/src/lib.rs:28 already cited PLUGIN_API.md as a deferred companion doc — **it now exists**.
   - **First `docs/§18/` directory landing**. Convention defined: each `docs/§18/<TOPIC>.md` carries a five-row header table (Companion-to / Status / Audience / Sibling doc / Reference impls). Future §18 dispatches inherit this convention.
   - Followups discovered during writing: (a) `PluginContext::diagnostics()` returns `&mut dyn DiagnosticSink` not read-only (PLUGIN_API.md captures actual signature); (b) `with_resource` consumes `self` chained-builder-style (documented); (c) cross-canary minor inconsistency: cad-projection unit suite doesn't test `Default::default == new()` round-trip (flagged in test-recipe template).
   - **§18 companion-doc count: 2 of 27** (was 0).
2. **physics::Plugin canary** (ADR-114 third-substrate proof point; +15 tests; physics 9 → 24; workspace 1634 → 1649):
   - **`crates/physics/src/plugin_adapter.rs`** (325L; 9 unit tests at file foot) mirrors cad-projection + gfx canary structure: `id() = PHYSICS_PLUGIN_ID`; `init = Ok(())`; `tick` takes `World` (rapier3d wrapper composing `RigidBodySet` + `ColliderSet` + `IslandManager` + `DefaultBroadPhase` + `NarrowPhase` + `ImpulseJointSet` + `MultibodyJointSet` + `CCDSolver` + `IntegrationParameters` + `PhysicsPipeline`) + `AuditLedger` via `ctx.take<T>()`, advances one simulation step, puts both resources back via `ctx.insert<T>(value)`; `shutdown = Ok(())`.
   - Missing-`World` → `ContractViolation { resource_type: "World" }` (auto-emit `Severity::Warning`); missing-`AuditLedger` after `World`-already-taken → idempotent-failure put-back invariant validated. **`RuntimeFault` NOT exercised** — rapier3d 0.32's `PhysicsPipeline::step` is infallible (returns `()`); the variant remains reserved for future fallible-step paths. Documented inline as the canonical "no-RuntimeFault straight-line subcase" pattern (alternative to gfx's lazy-build-on-first-tick).
   - **`crates/physics/tests/plugin_adapter_smoke.rs`** (499L; 6 integration tests + `PanickingTickPlugin` sibling fixture): full lifecycle through `PluginHost`; ContractViolation-missing-`World` path; ContractViolation-missing-`AuditLedger`-with-`World`-put-back-idempotent path; resources-put-back invariant; multi-tick determinism (10 ticks rapier3d `enhanced-determinism` feature byte-equal); multi-plugin isolation with `PanickingTickPlugin` sibling fixture (validates `catch_unwind` shield + plugin-fatal isolation per PLAN §1.13).
   - `crates/physics/src/lib.rs`: added `pub mod plugin_adapter; pub use plugin_adapter::{PhysicsPlugin, PHYSICS_PLUGIN_ID};`. `crates/physics/Cargo.toml`: added `rge-kernel-plugin-host` + `rge-kernel-diagnostics` workspace deps (Tier-2 → Tier-1; allowed per `forbidden-dep`; consistent with cad-projection + gfx canary deps).
   - **Third-substrate proof point for ADR-114**: rapier3d 0.32's `World` composition is Send straight out of the box with the `enhanced-determinism` feature; `AuditLedger` is also Send. **No Mutex / no Arc / no non-Send compromise / no `unsafe`.** The owned-handoff design generalizes cleanly across THREE substrate families (ECS-graph + GPU device handles + physics-world arenas) with **zero kernel-side substrate change** between them. **§10.4 dogfood-rule canary count: 3 of N** (cad-projection + gfx + physics).
   - Followups discovered: (a) **ADR-114 amendment candidate** documenting three-substrate-proof + lazy-init-vs-straight-line-pattern dichotomy + no-RuntimeFault subcase; (b) physics failure-class exemption is still ACTIVE in `tools/architecture-lints/exemptions.toml` (audit-1 rollout debt; physics now at first-real-implementation maturity per §1.13 — could clear in a separate small dispatch); (c) audio canary as fourth-substrate cross-check on `cpal::Stream`-style RAII handles (different Send shape).
2. **gfx::Plugin canary** (ADR-114 followup; 2nd real Tier-2 plugin proving PluginContext v1 generalizes beyond cad-projection; +16 tests; gfx 44 → 60; workspace 1618 → 1634):
   - **`crates/gfx/src/plugin_adapter.rs`** (357L) mirrors `cad-projection::CadProjectionPlugin` structure: `id() = GFX_PLUGIN_ID`; `init = Ok(())`; `tick` takes `GfxContext` + `HeadlessTarget` via `ctx.take<T>()`, **lazy-builds `TrianglePipeline` on first tick** (pipeline requires `&GfxContext` which orchestrator may stage AFTER init), records one frame via `FrameRecorder`, puts both resources back via `ctx.insert<T>(value)`; `shutdown = Ok(())`. Missing-`GfxContext` → `ContractViolation { resource_type: "GfxContext" }` (auto-emit `Severity::Warning`); missing-`HeadlessTarget` after `GfxContext` already taken validates idempotent put-back (registry has `GfxContext` after the failure). `RuntimeFault` path wired through `tick_inner` for pipeline-build failure (statically reachable for future custom-WGSL substitutions); not exercised at runtime since `TrianglePipeline::new` with embedded WGSL never fails in practice.
   - **`crates/gfx/tests/plugin_adapter_smoke.rs`** (499L; 7 integration tests): full lifecycle through `PluginHost`; both `ContractViolation` paths + idempotent put-back; resources-put-back invariant; multi-tick counter stability; lazy-pipeline build-on-first-tick correctness; multi-plugin isolation with `PanickingTickPlugin` sibling fixture (validates `catch_unwind` shield + plugin-fatal isolation per PLAN §1.13).
   - **9 unit tests** at `plugin_adapter.rs` foot: id/name returns, init success, tick missing-resource error variants, shutdown.
   - `crates/gfx/src/lib.rs`: added `pub mod plugin_adapter; pub use plugin_adapter::{GfxPlugin, GFX_PLUGIN_ID};`. `crates/gfx/Cargo.toml`: added `rge-kernel-plugin-host = { workspace = true }` (Tier-2 → Tier-1 dep allowed per `forbidden-dep`).
   - **Design-generalization data point for ADR-114**: wgpu 29 core types (`Device` / `Queue` / `Texture` / `TextureView`) are all `Send + Sync`; `GfxContext` + `HeadlessTarget` slot into `Box<dyn Any + Send>` without wrapping. **No `Mutex` needed; no non-Send compromise; no `unsafe`.** The owned-handoff pattern from cad-projection generalized cleanly to a wholly different resource family. **§10.4 dogfood-rule canary count: 2 of N** (cad-projection + gfx).
   - Subtle finding: lazy-build-on-first-tick pattern is the natural solution for plugins whose resources require `&GfxContext` to construct — useful template for future canaries (Camera/Material/etc. would follow same pattern).
2. **Phase 4 ADR backfill** (audit-2 Phase 4; 3 new ADRs at `docs/adr/` totaling 475 lines; pure-docs — zero test-count delta):
   - **ADR-098-topology-lineage-substrate.md** (150L): documents D-7.4 + D-7.4-followup + HIGH #3 unified collapse. Decision: hybrid lineage = csgrs metadata-passthrough where available + plane-equation-matching universal fallback. `Tessellation.face_labels: Option<Vec<TopologyFaceId>>` (NOT parallel `LabeledMesh`). `TopologyFaceId::DEGENERATE = u64::MAX` sentinel. v0 deferrals tracked: PersistentFaceId (Phase 7.2), OperatorId, SemanticScore, per-edge/per-vertex lineage, `kernel/graph-foundation::Graph` backing, connected-component analysis, csgrs Difference rhs-retag special-casing. Cache-key extension (Phase 2) via `Operator::output_is_labeled` + `effective_hash_and_label` upstream-bitmap fold-in documented.
   - **ADR-104-capability-surface.md** (163L): doc-comment-canonical `KernelCapabilities` acceptable until trigger fires; materialise on (a) editor-ui operator-picker filtering, (b) second CAD kernel (truck per ADR-113-deferred), (c) capability-aware tessellation cache. Canonical field names: `boolean_robust_under_tolerance / deterministic_triangulation / t_junction_handling / concave_input_supported / arity / output_labeled_when_input_labeled`. BooleanOp's existing capability-surface doc-block is the canonical reference. Followup: architecture-lint hook to enforce doc-comment-block presence on operator modules at materialisation time.
   - **ADR-114-pluginctx-owned-handoff.md** (162L): captures CRITICAL #2 + audit-2 Phase 0 unified design rationale. `BTreeMap<TypeId, Box<dyn Any + Send>>` owned-handoff (NOT borrowed-refs because `unsafe_code = forbid` is load-bearing); type-erased registry (NOT generic per-plugin context types because dogfood-rule combinatorial explosion); `catch_unwind` + pre/post `BTreeSet<TypeId>` snapshots (NOT trust-the-plugin because plugins are **untrusted-execution-domains** per kernel/userspace boundary framing); 5-variant `PluginError` taxonomy with `PluginPhase` enum. Rejected alternatives explicitly enumerated. First canary: cad-projection (16 tests). Followup: gfx::Plugin canary.
   - All 3 ADRs mirror ADR-112's structure (Status / Context / Decision / Consequences / Alternatives / Implementation guidance / References) and cross-link reciprocally. ADR-112's forward refs unchanged per no-existing-ADR-rewrite constraint. **ADR backlog: 4 of original 5 PLAN-referenced ADRs landed** (ADR-097/098/104/112/113-deferred + ADR-114 added by audit-2; ADR-101 not yet surfaced as urgent).
2. **Phase 3 test-gap-followup** (audit-2 Phase 3; 15 new integration tests + 16 fmt-incidental backfills; workspace 1587 → 1618; cad-core 174 → 183, cad-projection 39 → 45):
   - Closes audit-2's bounded test-coverage gaps. 5 new integration test files (1128 lines) under `crates/cad-core/tests/` + `crates/cad-projection/tests/`
   - `boolean_panic_recovery.rs` (224L; 2 tests): csgrs `catch_unwind` shield exercised against 4 pathological fixtures (zero-area / NaN coord / inverted-winding / coincident-vertex extreme). **csgrs 0.20.1 returned errors via the normal `Result` path on every fixture rather than panicking** — the `catch_unwind` wrap is classified **defensive-only-no-known-trigger** at this csgrs version. Future csgrs versions / weirder geometry may trigger it; the regression-detection capability is intact.
   - `cad_graph_corruption_recovery.rs` (210L; 3 tests): empty-CadGraph PIE restore preserves CadProjection.entity_cad_map but yields `ProjectionError::NodeNotInGraph` on tick (NOT panic — divergent-state tolerance per CRITICAL #1's design); `validate_handles` surfaces orphans deterministically; restore-then-mutate is atomic (no half-applied state).
   - `operator_edge_cases.rs` (232L; 4 tests): RevolveOp `new(p, segs)` ↔ `partial(p, segs, 2π)` byte-identical hash equality (closes audit-2 N3 carryover); Polygon2D zero-edge ctor rejection; ExtrudeOp `length=NaN/inf` rejection; BooleanOp on identical lhs+rhs uses 1.0001 perturbation idiom to avoid BLAKE3 NodeId collision (matches D-Boolean dispatch precedent).
   - `cross_substrate_determinism.rs` (166L; 1 test): 100-iter PIE round-trip soak — `capture(world, [&cad, &projection])` → fresh world+projection → restore → re-tick → byte-identical ProjectedMesh positions+indices via BLAKE3 across all 100 iterations. Substrate-level analog of D-Boolean's 200-iter operator determinism soak.
   - `projection_error_coverage.rs` (296L; 5 tests): every ProjectionError variant exercised (NotFound / NodeNotInGraph / DuplicateEntity / DuplicateNode / TolerationFailure-via-substrate-boundary). Closes the dead-error-variant audit-2 gap.
   - **CORRECTION 2026-05-09 audit-5 verification**: prior claim "warning-zone trio shrunk to 898L/904L/930L after fmt-canonical state" was **WRONG**. Actual line counts per `wc -l` 2026-05-09: **boolean.rs 999L (1 line under cap), revolve.rs 991L (9 lines under cap), infer.rs 990L (10 lines under cap)**. The 898/904/930 reading was a measurement error that propagated across multiple HANDOFF/Status/change.md updates. **Phase 5 proactive split is URGENT** (was wrongly claimed "demoted to opportunistic") — boolean.rs is one functional edit away from tripping the lint hard cap.
   - +31 workspace tests net (15 explicit + 16 fmt-incidental backfills). 9/9 architecture lints PASS. fmt clean.
2. **TessellationCache labeled-state defensive fix** (audit-2 Phase 2; 10 new tests; cad-core 164 → 174):
   - Closes audit-2 A1.4 / A5.2 / Pairing N2 — the latent-but-explosive cache-collision bug
   - New `Operator::output_is_labeled(inputs_labeled: &[bool]) -> bool` trait method with default `inputs.iter().any(|b| *b)` — labels propagate through any operator that takes labeled input
   - Per-operator audit: Cuboid/Extrude/Revolve match default (arity 0 → `false`); BooleanOp matches default (`any-labeled → labeled`); **TransformOp overrides to `false`** (Phase 7.1 positions-only impl strips labels; flagged for future labels-through-Transform dispatch)
   - Unified `effective_hash_and_label` helper folds upstream-labeled-bitmap (1 bit per port, modulo 32) into the BLAKE3 hash; both `eval_node` and `effective_hash` paths share this recursion (no double-evaluation)
   - Regression test verifies 4 distinct bitmap states produce 4 distinct effective hashes
   - File-size pressure noted at dispatch end: boolean.rs 999L (1L from cap), revolve.rs 991L (9L from cap). **Phase 3 wrongly claimed "fmt-canonical state recompacted to 898L/904L/930L"**. Audit-5 (2026-05-09) verified actual state per `wc -l`: 999L / 991L / 990L respectively — Phase 3's reading was a measurement error. **Phase 5 split is URGENT** before any next functional edit on these files.
2. **Phase 1 cleanup-pass** (7 inline doc/lint/governance items; zero functional code; tests unchanged):
   - `change.log` → `change.md` rename — restores public-repo audit-trail transparency (was caught by `*.log` gitignore)
   - HANDOFF L246 stale "Recommendation" framing rewritten to point at the canonical Architectural-debt registry
   - 5 `#[allow(clippy::cast_possible_truncation)]` reason= annotations per audit-1 cleanup convention
   - boolean.rs capability-surface explicit `§13.6 1000-iter periodic-soak gate is deferred` note
   - `kernel/ecs::participate::SnapshotParticipate` workspace-level RON/postcard format-policy documented at trait level
   - host.rs auto-emit allocation cost noted in module doc
   - `PluginRecord` + `PluginHost::get` pub-but-unused → documented as forward-API for reflective tooling
3. **Plugin-host `catch_unwind` hardening** (audit-2 Phase 0; 9 new tests; kernel/plugin-host 38 → 47):
   - Closes audit-2 A5.1 (Pairing 3 / N1) — **system-integrity-critical** finding: a plugin panicking AFTER `ctx.take::<World>()` and BEFORE `ctx.insert(world)` permanently lost World from the orchestrator
   - All 4 plugin call sites (`init_all` / `tick_all` / `shutdown_all` / `unregister`) wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))`; orchestrator snapshots `BTreeSet<TypeId>` of resources before each call and detects leaked resources (taken but not put back) on every outcome — Ok / Err / Panic
   - Refined `PluginError` taxonomy: 5 variants (InitFailed, ShutdownFailed, **RuntimeFault** renamed from `Runtime` tuple, **ContractViolation { resource_type: &'static str }**, **Panic { phase: PluginPhase, payload: String }**); new `PluginPhase` enum {Init, Tick, Shutdown}
   - Auto-emit policy: `ContractViolation` → `Diagnostic::warning` (caller misconfiguration — avoids 60Hz error-spam from misconfigured ctx); `RuntimeFault` / `Panic` / leak → `Diagnostic::error`
   - `unregister`-shutdown error/panic → `Diagnostic::warning` (host-initiated unregister is non-fatal by design — restores LOW #5 "diagnostic stream is the single source of truth" invariant for the previously silent-absorb path)
   - `CadProjectionPlugin` migrated: missing-World/CadGraph → `ContractViolation`; projection-tick errors → `RuntimeFault`
   - 9 new tests: panic recovery in all 3 lifecycle phases + multi-plugin tick-failure isolation (closes audit-1 carryover) + resource-leak detection + ContractViolation Warning-severity + unregister Warning-severity + Panic variant Display + PluginPhase Display
   - **Plugin host now treats plugins as untrusted execution domains** (kernel/userspace boundary equivalent per ChatGPT framing carried into the dispatch motivation). NO new `unsafe` (`AssertUnwindSafe` is a safe-API marker type)
   - host.rs at 1766L carries `// SPLIT-EXEMPTION:` annotation (impl ~670L; rest is the test fixture + 25+ lifecycle tests)
2. **Plugin diagnostic auto-emit** (post-audit LOW #5; 5 new tests; kernel/plugin-host 33 → 38):
   - Closes Pairing-5 of the 2026-05-07 deep audit (plugin failures swallowed into `*Report.failed[i].1` String, never reached the diagnostic sink)
   - `PluginHost::init_all` / `tick_all` / `shutdown_all` now auto-emit a synthetic `Diagnostic::error` with structured prefix `"plugin <id> init failed: <err>"` (or `tick failed` / `shutdown failed`) whenever a plugin returns `Err`
   - Plugin-fatal isolation preserved: emit is additive to whatever the plugin emits via `ctx.emit_diagnostic`; failure semantics unchanged
   - The diagnostic stream is now the single source of truth for plugin-failure surfacing
   - 5 new regression tests: init failure auto-emits, init success doesn't auto-emit, tick failure auto-emits, shutdown failure auto-emits, multi-plugin failures = one auto-emit per failure
   - Inline edit (no agent dispatch — too small to warrant the overhead); existing 33 plugin-host tests untouched and still pass
2. **BRepHandle SSoT refactor** (post-audit MEDIUM #4; 5 net new tests; cad-projection 34 → 39):
   - Closes Pairing-6 of the 2026-05-07 deep audit (cad-node FK dual-storage drift between BRepHandle ECS component and EntityCadMap)
   - `BRepHandle.cad_node: NodeId` field **dropped**; struct now carries only `mesh_id` + `last_projected_checkpoint`
   - `EntityCadMap` is the sole authoritative owner of entity↔cad-node mapping
   - New `CadProjection` accessors: `node_for(entity)` / `entity_for(node)` / `remap_entity(entity, new_node)` — replaces the `handle.cad_node = new_node` write idiom with an atomic, validated transition that auto-marks the entity dirty for re-projection
   - `BRepHandle::new()` no longer takes a NodeId
   - Plugin adapter audit confirmed (no edits needed): `plugin_adapter.rs` only goes through `CadProjection::tick`, which always used `entity_cad_map.node_for(*entity)` for the lookup
   - 5 new regression tests: struct-shape RON-no-cad_node check, node_for/entity_for accessor round-trip, remap-marks-dirty, remap-unknown-entity-error-NotFound, default-matches-new
   - **Axis 3 fully closed**: HIGH #3 unified Tessellation + this MEDIUM #4 BRepHandle SSoT — no more dual-source representations in cad-core or cad-projection
2. **Unified Mesh refactor** (post-audit HIGH #3; 9 net new tests; cad-core 155 → 164):
   - Closes Pairings 1+8 of the 2026-05-07 deep audit (labeled vs. unlabeled type duality)
   - `Tessellation` extended with `face_labels: Option<Vec<TopologyFaceId>>` (`#[serde(default, skip_serializing_if = "Option::is_none")]` for snapshot-format minimality); new `with_labels(...)` ctor + `is_labeled()` / `face_labels()` / `face_count()` accessors; new `LabelLengthMismatch` error variant
   - `LabeledMesh` type **deleted** entirely
   - `TopologyFaceId` moved from `topo_lineage::types` to `tessellation::mesh` (avoids tessellation→topo_lineage reverse import); re-exported through `topo_lineage::types` for back-compat — every existing import path keeps working
   - `BooleanOp::evaluate` collapsed to one method: dispatches on `lhs.is_labeled() || rhs.is_labeled()`; mixed inputs synthesize `TopologyFaceId::DEGENERATE` per-triangle labels on the unlabeled side (downstream lineage classifies as Reinterpreted)
   - `infer_lineage` collapsed to one function: requires labeled input (returns `LineageError::InvalidInput` otherwise); dispatches on output's labeled state (label-tracking when labeled, plane-equation heuristic when unlabeled)
   - `evaluate_labeled` + `infer_lineage_labeled` removed from public API
   - `label_by_plane` now returns `Tessellation` (was `LabeledMesh`)
   - The labeled-mesh substrate now composes through `OperatorGraph::evaluate` end-to-end — Pairing-1 closed
   - boolean.rs at 997 lines + infer.rs at 990 lines (both under 1000-cap; agent compressed redundant doc-comments to land under)
2. **PluginContext v1 + CadProjectionPlugin canary** (post-audit CRITICAL #2; 16 new tests; kernel/plugin-host 23 → 33, cad-projection 28 → 34):
   - Closes Pairing-3 of the 2026-05-07 deep audit ("PluginContext is a logger, not a context")
   - `kernel/plugin-host::PluginContext` extended with type-erased resource registry: `BTreeMap<TypeId, Box<dyn Any + Send>>` + `insert<T>` / `get_mut<T>` / `take<T>` / `contains<T>` / `resource_count()` / `with_resource<T>` builder
   - Existing `PluginContext::new(diagnostics)` + `emit_diagnostic` + `diagnostics()` v0 API bit-identical (no breaking changes)
   - **No new `unsafe` code** — owned-resources-handoff design avoids the unsafe normally needed for type-erased borrowed references. Plugins `take<T>` at start, do work, `insert<T>` back; orchestrator wraps the call by inserting before and taking after
   - First real Tier-2 plugin canary lands as `crates/cad-projection/src/plugin_adapter.rs` (~190L) — `CadProjectionPlugin` impls `Plugin`, extracts `&mut World` + `&CadGraph` + `Tolerance` from ctx via `take<T>`, drives `CadProjection::tick`, puts resources back via `insert<T>`. Missing required resources surface as `PluginError::Runtime`; `Tolerance` defaults to 0.001m
   - Tests: 10 unit (context.rs registry mechanics) + 3 unit (plugin_adapter id/init/into_projection) + 3 integration smoke (full lifecycle via PluginHost + missing-resource error path + resources-put-back invariant)
   - New cad-projection dep: `rge-kernel-plugin-host` (Tier-2 → Tier-1; allowed per `forbidden-dep`)
   - The "Real Tier-2 dogfood unblocked" claim is now substantiated, not optimistic
2. **CadGraph::SnapshotParticipate** (post-audit CRITICAL #1; 9 new tests; cad-core 148 → 155, cad-projection 26 → 28):
   - Closes the silent PIE inconsistency window (Pairing-4 of the 2026-05-07 deep audit) and PLAN §13.2 gate "all stateful Tier-2 has SnapshotParticipate"
   - `impl SnapshotParticipate for CadGraph` in new `crates/cad-core/src/checkpoints/participate.rs` (414L); ParticipantId `cad-core.cad-graph`; serialization via **RON** (postcard rejected `OperatorNode`'s `#[serde(tag = "kind")]` — non-self-describing format limitation; switched to RON which `kernel/graph-foundation::GraphSnapshot` already uses internally)
   - `Serialize+Deserialize` derives added to: `CheckpointId`, `Checkpoint`, `InProgress`, `CheckpointHistory`, `CadGraph`, `OperatorGraph`
   - `CadProjection::validate_handles(&self, &CadGraph) -> Vec<(EntityId, NodeId)>` returns orphan handles after divergent-state restore (caller decides recovery: log diagnostic / re-project / error)
   - PIE full round-trip verified with both `[&cad, &projection]` participants; divergent-state smoke verifies `tick(&empty_cad)` returns `ProjectionError::NodeNotInGraph` (not panic)
   - New cad-core dep: `rge-kernel-ecs` (Tier-2 → Tier-1; allowed per `forbidden-dep`)
   - "Temporal foreign-key constraint" framing per ChatGPT cross-review: `BRepHandle.cad_node` is FK; `CadGraph.nodes` is PK set; PIE restore = transaction rollback
2. **D-7.4-followup csgrs metadata-passthrough integration** (11 new tests; cad-core 137 → 148; closes the v0 plane-only false-positive in lineage inference):
   - Switched `BooleanOp` from `csgrs::Mesh<()>` to generic `Mesh<M>` where `M: Clone + Send + Sync + Debug + 'static`
   - Added `BooleanOp::evaluate_labeled(&LabeledMesh, &LabeledMesh) -> LabeledMesh` carrying `TopologyFaceId` through csgrs polygon metadata
   - Added `infer_lineage_labeled(input, output) -> LineageGraph` for high-confidence per-face classification
   - Existing `evaluate(&[&Tessellation])` API kept bit-identical (passes `()` metadata); both paths coexist
   - **v0 false-positive fix**: surviving partially-consumed Difference faces now classify as Split (not Merged) — labeled-Difference integration smoke verifies `merged_count == 0` and `split_count >= 1`
   - csgrs Difference quirk reflected in labels: rhs's clipped polygons get retagged with lhs's metadata (per ADR-112 spike) — `infer_lineage_labeled` doesn't need to know about it; the labels carry through correctly
2. **C kernel/plugin-host** (23 new tests; kernel/plugin-host 0 → 23; closes §10.4 dogfood-rule carry-over):
   - Replaced 4-line stub with `Plugin` trait + `PluginContext` + `PluginHost` (1,140 src lines + tests)
   - `Plugin` trait per PLAN §10.4: `id() / name() / init / tick (default no-op) / shutdown (default no-op)`; `Send + 'static` so the host can hold them as `Box<dyn Plugin>`
   - `PluginContext { &mut dyn DiagnosticSink }` v0 — EventBus / Commands handles deferred until concrete plugins demand
   - `PluginHost`: BTreeMap<PluginId, PluginRecord> + Vec for insertion order; Pending → Initialized → Failed/Active → ShuttingDown → Shutdown lifecycle; init in registration order; shutdown LIFO
   - **Plugin-fatal isolation**: one plugin's failure marks it Failed but doesn't block other plugins (per PLAN §1.13)
   - 2 dogfood-smoke integration tests with `TestTier2Plugin` fixture — foundation for the §10.4 contract test (future dispatches replace fixture with real gfx::Plugin / physics::Plugin / editor-ui::Plugin / cad-projection::Plugin)
   - Failure-class declaration `//! Failure class: plugin-fatal` per §1.13; exemption cleared from registry
   - **kernel/plugin-host promoted EMPTY-STUB → IMPLEMENTED**; **Tier-1 kernel now 10 of 15 implemented**
2. **Phase 7.4 D-7.4 topology lineage prototype** (21 new tests; cad-core 116 → 137; first prototype of the most-novel-system in the architecture per PLAN §1.5.4.3 / ADR-098):
   - New `crates/cad-core/src/topo_lineage/` split across 4 sub-files (anticipating growth): `types.rs` (517L), `plane.rs` (199L pub(crate)), `infer.rs` (499L), `mod.rs` (106L orchestrator) — all under split-exemption cap
   - Public types: `TopologyFaceId(u64)`, `TopologyEvolution { Preserved, Split, Merged, Deleted, Reinterpreted }`, `LineageEdge { from, to, evolution, confidence }`, `LineageGraph`, `LabeledMesh`, `LineageError`
   - Private `QuantizedPlane` (1e-4 precision, sign-canonicalized so opposite-winding triangles hash equal)
   - Free fns: `label_by_plane(tess, base_id)` groups triangles by plane equation; `infer_lineage(input, output, base_id)` plane-matching heuristic classifying Preserved (exact match) / Split (input>output triangles on plane) / Merged (input<output) / Deleted (no output match) / Reinterpreted (no input match)
   - **Real csgrs hardening win**: first integration run hit DegenerateTriangle errors from real BSP-tree zero-area slivers; hardened with `TopologyFaceId::DEGENERATE = u64::MAX` sentinel for the heuristic path while preserving strict error variants for the private API
   - **Heuristic limitation documented**: triangle-count heuristic classifies many partially-consumed Difference faces as Merged rather than Split (csgrs's BSP triangulation produces more output triangles per surviving plane than the 2-tri input). v0 false-positive class — future boundary-precision detector or csgrs-metadata path will fix
   - Boolean Union smoke verifies ≥1 Reinterpreted edge surfaces; Boolean Difference smoke verifies ≥1 Split/Deleted/Merged edge + preserved_count < 6
   - **v0 simplifications vs PLAN §1.5.4.3** documented: no `OperatorId` field; no `SemanticScore` field; no `Split(Vec<PersistentFaceId>)`/`Merged(Vec<PersistentFaceId>)` inner data (multi-edge representation instead); face-only no edge/vertex lineage; no `PersistentFaceId` (per-mesh sequential ids only — Phase 7.2 substrate); no csgrs metadata-passthrough integration yet (future small follow-up); no `kernel/graph-foundation::Graph` backing (Vec for v0)
2. **Phase 7 D-Partial-Revolve** (20 new tests; cad-core 96 → 116; RevolveOp extension):
   - Added `pub angle: f32` field to `RevolveOp` with `#[serde(default = "default_angle_full_revolution")]` for snapshot back-compat
   - New `RevolveOp::partial(profile, segments, angle)` constructor; existing `new(profile, segments)` delegates to `partial(p, segs, 2π)` so backwards-compat is bit-identical
   - Validates `angle ∈ (0, 2π]` finite; clamps near-2π (within 1e-5) to exactly 2π for the full-revolution fast path; `is_full_revolution()` accessor uses 1e-6 epsilon
   - `evaluate()` split: full path unchanged (no caps; concave allowed); partial path emits `n*(segments+1)` verts + `2*n*segments` side tris + `2*(n-2)` cap tris with fan-triangulated start/end caps; **convexity required** for partial-revolution
   - Cap winding: start cap at θ=0 has -Z normal; end cap at θ=angle has +tangent normal
   - `structural_hash` extended to include `angle.to_le_bytes()` — breaking change vs pre-D-Partial-Revolve hashes (cached tessellations recompute on first eval; acceptable for v0)
   - 18 new unit tests + 2 integration (pi-radian + half-pi-with-Boolean pipeline smoke)
   - Direct-struct-literal sites needing update: zero — all callers use the constructors
   - **Split-exemption lint caught a 1015-line file regression** — resolved by trimming a 22-line analysis comment to 3 lines (final 995). The 9-lint enforcement gate works as designed.
2. **Phase 7 D-Boolean** (18 new tests; cad-core 78 → 96; 5th cad-core operator; first with Tier-3 dep):
   - `BooleanOp { mode: BooleanMode { Union | Intersection | Difference } }` arity 2 (lhs=port 0, rhs=port 1) backed by `csgrs 0.20.1` (pure-Rust BSP-tree CSG, MIT)
   - Conversion bridge cad-core f32 `Tessellation` ↔ csgrs f64 `Mesh<()>` via `nalgebra::Point3<f64>` / `Vector3<f64>`; right-hand-rule outward normals from CCW winding; output polygons fan-triangulated; coincident-vertex dedup via BTreeMap-keyed f64 LE-byte equality (deterministic)
   - `std::panic::catch_unwind` wraps the csgrs call → `OpError::InvalidParameter("boolean failed: <diag>")` for pathological input
   - structural_hash = BLAKE3(b"boolean:" || mode_discriminant_u8) — local hash only per ADR-112 (lhs/rhs effective_hash folded in upstream by `OperatorGraph::evaluate`)
   - `OpKind::Boolean` + `OperatorNode::Boolean(BooleanOp)` threaded through; `as_operator()` extended
   - **Tests**: 12 unit (mode dispatch / arity / hash determinism / disjoint-union / overlap-union / disjoint-intersection / overlap-intersection / difference-dent / non-commutativity / near-degenerate / pathological / wrong-arity) + 1 dispatch + 3 integration smoke (pipeline_union with Cuboid+Transform-translated-Cuboid, pipeline_difference, with_extrude_input heterogeneous lhs/rhs) + 2 determinism soak (100 iterations × Union/Difference, byte-identical via BLAKE3(positions||indices))
   - **Capability surface declared via doc-comment per ADR-104** (full struct lands with future ADR-104 dispatch): `boolean_robust_under_tolerance: false` (BSP, no exact arithmetic); `deterministic_triangulation: true` (200-iter soak PASS via BTreeMap-keyed dedup)
   - **30-min Phase 7.4 lineage spike** per ADR-112 §"Followups": csgrs preserves per-polygon `Mesh<S>` metadata through Union/Intersection (cloned through plane splits and `clip_polygons`); **Difference retags rhs polygons with lhs's metadata** (known csgrs quirk — Phase 7.4 lineage reconstruction must special-case); per-triangle source-tracking is feasible
   - New deps: `csgrs 0.20.1` (`default-features = false, features = ["f64", "earcut"]` — f32 conflicts with workspace-pinned rapier3d 0.32; earcut required since one of delaunay/earcut must be enabled); `nalgebra 0.33`
   - T-junction handling deferred (csgrs upstream TODO; no visible artifacts in test fixtures)
   - Real bug caught: pipeline_difference initially failed `DuplicateNode(NodeId)` because two `CuboidOp(1,1,1)` collide on content-derived NodeId — fixed by perturbing depth to 1.0001 (matches pipeline_union idiom)
   - Match-exhaustiveness audit: zero downstream sites needed update
2. **ADR-112: D-Boolean CSG library scoping** (read-only research dispatch — zero Rust changes; first ADR landed in workspace):
   - `docs/adr/ADR-112-cad-boolean-csg-library.md` 196 lines / 14 sections
   - **Decision: csgrs** (pure-Rust BSP-tree CSG) over parry / truck / roll-our-own
   - Rejected parry: doesn't perform mesh booleans, only spatial queries / convex-hull / ACD
   - Rejected truck: would force migrating all 4 existing operators from `Tessellation` to `Solid` — deferred to a future ADR-113 placeholder gated on Phase 7.4 outcomes
   - Rejected roll-our-own: csgrs ships 5+ years of edge-case fixes vs ~1500 LoC of new code; not worth the maintenance burden
   - csgrs caveat: README explicitly lists T-junction handling as TODO — fragility class to watch in D-Boolean dispatch
   - Implementation guidance for D-Boolean inline: `BooleanOp { mode: Union | Intersection | Difference }` arity 2, structural_hash recipe, 7 test fixtures, failure-class snapshot-recoverable, determinism gate
   - 4 followups identified for separate ADRs / spike: truck migration trigger; csgrs polygon-metadata passthrough; T-junction policy; CI determinism soak
2. **Phase 7 D-Revolve** (19 new tests; cad-core 59 → 78; 4th cad-core operator):
   - `RevolveOp { profile: Polygon2D, segments: u32 }` arity 0; reuses `Polygon2D` substrate from D-Extrude; full 2π revolution around Y-axis with `segments` rotational steps
   - **Concave profiles ALLOWED** for full revolution (no fan-triangulated caps needed → Extrude's convexity restriction does not apply)
   - Validates profile lies on +X side of Y-axis (`all x >= 0`), `signed_area != 0`, `segments >= 3`; CW/CCW input both accepted
   - Algorithm: per profile point `(x, y)` at ring `s` with `θ = s · 2π/segments`, 3D position is `(x·cos θ, y, x·sin θ)`. No caps — full revolution closes via index wrap. Total `n·segments` verts + `2·n·segments` tris
   - `OpKind::Revolve` + `OperatorNode::Revolve(RevolveOp)` threaded through; `as_operator()` extended
   - Critical correctness validations PASS: triangle/square/hexagon vertex+triangle counts; concave acceptance; axis-touching profile yields degenerate-but-valid mesh; CW handling; structural_hash deterministic + parameter-sensitive; full 2π closure (every ring vertex on circle of correct radius); ring 0 lies in XY plane; outward radial normal verified by `revolve_first_quad_has_outward_radial_normal`
   - Square-profile × 8 segments integration smoke verifies r² ∈ {1, 4} for every output vertex (matching the unit-square cross-section's inner/outer radii)
   - Match-exhaustiveness audit: zero downstream sites needed update
2. **Phase 7 D-Extrude** (26 new tests; cad-core 33 → 59; first non-trivial cad-core operator):
   - `Polygon2D` 2D-profile type (closed XY-plane polygon; `Polygon2DError::{TooFewPoints, NonFiniteCoordinate, DegenerateEdge}` ctor validation; lazy `signed_area()` + `convexity()` for evaluate-time gating)
   - `ExtrudeOp { profile: Polygon2D, length: f32 }` arity 0; convex profile + linear +Z extrusion + fan-triangulated caps + side-wall quads (split to triangles); CW/CCW input both accepted (winding-agnostic from caller); concave rejected with `InvalidParameter` matching "convex"
   - `OpKind::Extrude` + `OperatorNode::Extrude(ExtrudeOp)` variants threaded through; `as_operator()` match arm extended
   - **No external triangulation library** — fan triangulation suffices for convex profiles; concave support deferred to a separate dispatch with library scoping ADR (earcutr / lyon options)
   - Critical correctness validations PASS: triangle/square/pentagon/hexagon vertex+triangle counts (n→2n verts, 4n-4 tris); concave reject; CW handling; structural_hash deterministic + parameter-sensitive
   - **Match-exhaustiveness audit**: zero downstream sites needed update — only `as_operator()` in cad-core itself pattern-matches `OperatorNode`; cad-projection only constructs variants
2. **Phase 7.3 cad-projection minimal D-7.3** (26 new tests, cad-projection 0 → 26; promoted from PARTIAL → IMPLEMENTED) — validates the v0.6 CAD/ECS impedance-fix critical-path bet for real. 4 of 6 modules per PLAN §1.5.4.5 now implemented (semantic / runtime / editor stay stubs per §0.6 freeze policy):
   - `projection_structural/` — `BRepHandle { cad_node, mesh_id, last_projected_checkpoint }` ECS component (impl `Component` + `SnapshotComponent`); `EntityCadMap` bidirectional `BTreeMap` with duplicate-key errors; private `EntityIdProxy` + manual Serialize/Deserialize bridge since `kernel::ecs::EntityId` doesn't enable `ulid/serde`
   - `projection_geometry/` — `ProjectedMesh { positions, indices, source_node, source_checkpoint }` + `ProjectedMeshId(u64)` + free `project(cad, node, &mut TessellationCache, Tolerance) -> Arc<ProjectedMesh>` calling `cad-core::OperatorGraph::evaluate`; `CheckpointTag(u64)` proxy serializable since `cad_core::CheckpointId` doesn't derive serde
   - `projection_cache/` — `ProjectionCache` with last_seen_checkpoint + entity_meshes + dirty BTreeSet + hits/misses/reprojections stats; `observe_checkpoint(head, all_entities)` triggers head-advance dirty-mark-all
   - Top-level `lib.rs` `CadProjection { entity_cad_map, cache, tess_cache }` orchestrator with `tick(world, cad, tolerance) -> TickReport` + `spawn_brep_entity` / `despawn_brep_entity` / `entity_for(node)` / `node_for(entity)` / `projected_mesh(entity)`
   - `SnapshotParticipate` impl with `ParticipantId::new("cad-projection.brep-handles")`; capture/restore via postcard binary serialization carrying EntityCadMap + entity↔ProjectedMeshId association + last_seen_checkpoint; meshes themselves re-derive on next tick
   - **Both Phase 7.3 exit criteria PASS** (verified by integration smoke tests): (1) cad-projection invalidation triggers ECS update within one tick of cad-core commit; (2) PIE round-trip preserves cad-projection state
   - **`projection-modules` lint actively enforces** the structural↛runtime/editor split (PASS 0 violations); **`forbidden-dep` lint confirms** cad-projection is the only Tier-2 importing cad-core
   - New deps added: postcard (binary SnapshotParticipate payload), ulid w/ serde (EntityIdProxy)
2. **Phase 7.1 cad-core MVP D-prime** (33 new tests, cad-core 0 → 33; promoted from PARTIAL → IMPLEMENTED) — substrate per IMPLEMENTATION.md §7.1 with 2 trivial operators to validate end-to-end. 7 new modules under `crates/cad-core/src/`:
   - `operators/{mod, cuboid, transform}.rs` — `Operator` trait (`op_kind` / `structural_hash` / `evaluate` / `arity`); `OperatorNode` enum dispatching to concrete impls; `EdgeKind::Input(port)` for ordered ports; `CuboidOp { width, height, depth }` arity 0 → 8-vertex/12-tri origin-centered axis-aligned box; `TransformOp { translation, rotation_quat_xyzw, scale }` arity 1 → applies `glam::Mat4::from_scale_rotation_translation` to upstream positions
   - `graph/operator_graph.rs` — `OperatorGraph` wraps `kernel::graph_foundation::Graph<OperatorNode, EdgeKind>`; content-derived NodeId via BLAKE3 over serialized OperatorNode; recursive `evaluate()` with `HashSet<NodeId>` ancestor stack for cycle detection (graph-foundation does NOT detect cycles itself); **`effective_hash` recursively combines local_hash + port + upstream effective_hash** so cache invalidates correctly when ANY upstream parameter changes (key correctness validation)
   - `checkpoints/mod.rs` — `CheckpointId(u64)`, `Checkpoint { id, snapshot, root, parent }`, `CheckpointHistory`; `CadGraph` wrapper owning both the graph + history; `begin_operation` eagerly captures `GraphSnapshot`; `commit` advances head; `rollback` restores from in-progress snapshot; `restore_to(id)` replays historical snapshot; `graph_mut()` guarded by `MutationOutsideOperation` error
   - `tessellation/{mod, mesh, cache}.rs` — `Tessellation { positions, indices }` with index-validity check; `Tolerance::new(t)` validates finite>0 and quantizes to `(t*1e9) as u64` for hash equality across float drift; `TessellationCache` HashMap keyed on `CacheKey { structural_hash: [u8; 32], tolerance }` with hit/miss tracking
   - `tests/cad_smoke.rs` — end-to-end integration test
   - **All 4 critical Phase 7 architectural bets validated**: (1) operator DAG works on graph-foundation primitives, (2) checkpoint/rollback/restore_to round-trips byte-identical via GraphSnapshot, (3) tessellation cache invalidates correctly on parameter change (recursive effective_hash test PASS), (4) cad-core sits cleanly under graph-foundation without redefining NodeId/EdgeId (lint PASS 0 violations)
   - Failure-class declaration `//! Failure class: snapshot-recoverable` added; cad-core exemption REMOVED from `tools/architecture-lints/exemptions.toml`
2. **(earlier this session)** Phase 6 PBR-lite in `crates/gfx/` (18 new tests, gfx 26 → 44) — single-light Lambert+Phong + texture sampling on top of the wgpu substrate. 5 new modules: `vertex_lit.rs` / `camera.rs` / `light.rs` / `material.rs` / `lit_mesh_pipeline.rs`. Pixel-level lit/backlit/checker assertions PASS on RTX 4060 Ti / Vulkan. wgpu 29 quirks discovered: `Queue::write_texture` takes `TexelCopyTextureInfo` by value; `SamplerDescriptor.mipmap_filter` is `MipmapFilterMode` (distinct type); `bytemuck::cast_slice(&[ubo])` lifetime issue → use `bytemuck::bytes_of(&ubo)`.
3. **(prior session)** `kernel/graph-foundation` (Tier 1, 47 tests) — substrate per PLAN §1.14: NodeId/EdgeId BLAKE3-derived, StableHash trait, Graph<N,E>, GraphSnapshot, GraphDiff, Invalidation propagation, VizAdapter trait. `graph-foundation` lint actively enforces reuse.
4. **(prior session)** `kernel/ecs::participate` (PIE composition substrate, 14 tests) — `SnapshotParticipate` trait + `PieSnapshot` aggregator. Composes existing `SnapshotComponent` with per-subsystem state into the unified PIE snapshot per PLAN §6.13.
5. **(prior session)** Two deep audits + cleanup passes — failure-class taxonomy correction; ui-theme indirection collapse; ~286 KB of stale transcripts removed; `.gitignore` hardened.

## Next-job options (dispatch-ready)

Pick one. All four are bounded single-agent dispatches.

### Option B — Phase 6 fill-in (renderer progress)

**Goal**: continue Phase 6 toward the 60fps simple-scene golden gate.

**State**: Phase 6.1 substrate done (wgpu init + headless triangle + mesh rendering + transforms via Transform UBO, 26 tests) AND **PBR-lite shipped this session** (single-light Lambert+Phong + texture sampling, 18 new tests, total gfx 44; verified pixel-level on real RTX 4060 Ti / Vulkan). Remaining Phase 6 items per IMPLEMENTATION.md:
- 6.1 follow-up: **frame-graph minimal** (transient resource lifetimes per frame; `TexturePool`/`BufferPool` keyed on frame index; declarative pass DAG with read/write resource declarations so transient resources can be aliased across non-overlapping passes). Recommended next sub-dispatch within B.
- 6.2 **render-snapshot separation** per §1.5.2 (sim-thread mutates N+1, render-thread reads frozen WorldSnapshot{N}; the shipped `PieSnapshot`/`SnapshotParticipate` substrate is what feeds this; gfx needs to impl `SnapshotParticipate` for whatever render-side state is replicated)
- 6.3 **material-runtime** — material UBOs already exist (this session); next is **WGSL+naga shader compile** (naga not yet workspace dep — bring it in) + **pipeline cache** (PSO keyed on shader hash + vertex layout) so 100 material instances share one PSO
- Exit criteria: 60fps on `simple-scene` golden project (1k cubes + 1 directional light); editor frame ≤ 8ms idle; render-thread sees stable snapshot; 100 material instances share one PSO

**Recommended next sub-dispatch within B**: frame-graph minimal. PBR-lite is done; frame-graph optimizes resource lifetimes and is the right substrate before scaling to many materials. Material-pipeline cache (6.3 latter half) is also a clean dispatch and can run in parallel with frame-graph since they touch different parts of gfx.

**wgpu 29 API quirks documented** (from Phase 6.1 + PBR-lite dispatches): `Instance::new_without_display_handle()`, `request_adapter` returns `Result<_, RequestAdapterError>`, `multiview` → `multiview_mask`, `Maintain::Wait` → `PollType::wait_indefinitely()`, `PipelineLayoutDescriptor.bind_group_layouts` is `&[Option<&BindGroupLayout>]` not `&[&BindGroupLayout]`, `BufferViewMut` doesn't impl IndexMut (use `queue.write_buffer` not `mapped_at_creation`), **`Queue::write_texture` takes `TexelCopyTextureInfo` by value (not by reference)**, **`SamplerDescriptor.mipmap_filter` is `MipmapFilterMode` (distinct re-exported type from `FilterMode`)**, **`bytemuck::cast_slice(&[ubo])` creates a temporary that drops before `queue.write_buffer` reads it (E0716) — use `bytemuck::bytes_of(&ubo)` for single-struct uploads**.

### ~~Option C — `kernel/plugin-host`~~ DONE 2026-05-07

`Plugin` trait + `PluginContext` + `PluginHost` lifecycle landed. 23 tests including dogfood-smoke integration. kernel/plugin-host promoted EMPTY-STUB → IMPLEMENTED. Tier-1 kernel now 10/15.

### Option D — Phase 7 cad-core continuation (HIGHEST SECONDARY RISK per IMPLEMENTATION.md)

**Status**: D-prime substrate + D-7.3 bridge both **DONE this session**. cad-core + cad-projection both PARTIAL → IMPLEMENTED. Subsequent Phase 7 dispatches each pick one bounded follow-up.

#### ~~D-7.3 — cad-projection minimal~~ DONE 2026-05-06

`BRepHandle` ECS component + bidirectional EntityCadMap + ProjectedMesh + ProjectionCache + `CadProjection::tick()` + `SnapshotParticipate` impl. 26 tests including invalidation-within-one-tick + PIE round-trip integration smoke. Both Phase 7.3 exit criteria PASS. Architecture lints `projection-modules` + `forbidden-dep` PASS.

#### ~~D-Extrude — first non-trivial operator~~ DONE 2026-05-06

`Polygon2D` 2D-profile type + `ExtrudeOp { profile, length }` operator (arity 0; +Z extrusion with fan-triangulated caps + side walls; convex-only with concave rejection; CW/CCW input both accepted). 26 tests including pentagon-prism integration smoke. Phase 7 operator catalog now: Cuboid + Transform + Extrude.

#### ~~D-Revolve — sweep-of-revolution~~ DONE 2026-05-06

`RevolveOp { profile, segments }` arity 0; full 2π around Y-axis; concave profiles ALLOWED (full revolution = no fan-triangulated caps); reuses `Polygon2D` from D-Extrude. 19 tests including square × 8-segments integration smoke verifying r²∈{1,4} radii.

#### ~~D-Boolean — CSG operations~~ DONE 2026-05-06

`BooleanOp { mode: Union | Intersection | Difference }` arity 2 backed by csgrs 0.20.1; conversion bridge cad-core f32 ↔ csgrs f64; 18 tests including 100-iter determinism soak across Union+Difference. Capability surface declared per ADR-104. csgrs metadata-passthrough confirmed for Union/Intersection; Difference retags as known csgrs quirk (Phase 7.4 must special-case).

#### ~~D-Partial-Revolve — angle < 2π extension~~ DONE 2026-05-07

`RevolveOp` extended with `pub angle: f32` field; `partial(profile, segments, angle)` constructor; full-revolution backwards-compat via `new()` delegating to `partial(p, segs, 2π)`; partial-revolution path emits fan-triangulated start/end caps (convexity required); 20 new tests including pi-radian + half-pi integration smoke + partial-revolve-through-Boolean pipeline smoke. cad-core 96 → 116.

#### ~~D-7.4 — topology lineage prototype~~ DONE 2026-05-07

`TopologyFaceId` + `TopologyEvolution` + `LineageEdge` + `LineageGraph` + `LabeledMesh` types per PLAN §1.5.4.3 (v0 — simplified spec; OperatorId/SemanticScore/inner-Vec data deferred). Plane-equation-matching heuristic with sign-canonicalized `QuantizedPlane`. Hardened against real csgrs degenerate-triangle output. 21 tests including Boolean-union + Boolean-difference integration smoke. cad-core 116 → 137.

#### ~~D-7.4-followup — csgrs metadata passthrough integration~~ DONE 2026-05-07

`BooleanOp::evaluate_labeled` carries `TopologyFaceId` through csgrs `Mesh<S>` metadata; `infer_lineage_labeled` consumes labeled output for high-confidence classification; v0 plane-only Merged-vs-Split false-positive fixed; both paths coexist. 11 tests including labeled-Difference integration smoke.

#### ~~D-Partial-Revolve — angle < 2π extension~~ DONE 2026-05-07

`RevolveOp { profile, segments, angle }` extended with `partial(profile, segments, angle)` constructor; full-revolution backwards-compat preserved via `new()` delegating to `partial(p, segs, 2π)`; partial-revolution path emits fan-triangulated start/end caps (convexity required); structural_hash includes angle bytes. 20 tests including pi-radian + half-pi integration smoke.

#### ~~D-Boolean — CSG operations~~ DONE 2026-05-06 (via ADR-112)

`BooleanOp { mode: Union | Intersection | Difference }` arity 2 backed by `csgrs 0.20.1`; conversion bridge cad-core f32 ↔ csgrs f64; 18 tests including 100-iter determinism soak across Union+Difference. Capability surface declared per ADR-104. csgrs metadata-passthrough integration shipped as D-7.4-followup.

#### D-7.2 — persistent topology IDs

**Goal**: validate face/edge IDs survive parameter rebuilds (per IMPLEMENTATION.md Phase 7.2; smoke test: 100 operator chains × 10 random parameter rebuilds with face/edge IDs preserved per `TopologyEvolution` enum).

**State**: needs a B-Rep model first (current `Tessellation` is triangle soup with no per-face / per-edge identity). Likely requires a `BRep` struct with named faces+edges, or a labeling scheme on triangle groups. Bigger dispatch. The plane-equation approach prototyped in D-7.4 is the input to this model's identity-stability story.

#### ~~D-7.4 — topology lineage prototype~~ DONE 2026-05-07

`TopologyFaceId` + `TopologyEvolution` + `LineageEdge` + `LineageGraph` + `LabeledMesh` types per PLAN §1.5.4.3 (v0 — simplified spec; OperatorId/SemanticScore/inner-Vec data deferred). Plane-equation-matching heuristic with sign-canonicalized `QuantizedPlane`. 21 tests including Boolean-union + Boolean-difference integration smoke. Strengthened by D-7.4-followup metadata-passthrough.

**Dispatch order recommendation** (post-2026-05-08 audit-2 Phase 0 closure): see the canonical [Architectural-debt registry](#architectural-debt-registry-post-2026-05-07-deep-audit) Dispatch order section above. The 5-finding audit-1 ledger + 1-finding audit-2 Phase 0 are all closed; the next ranked options live in the registry's "remaining items" list (Phase 1 cleanup-pass / Phase 2 TessellationCache labeled-state fix / Phase 3 test-gap-followup / Phase 4 ADR backfill / gfx::Plugin canary / kernel stubs / §18 docs). The "Real Tier-2 dogfood unblocked" claim is now substantiated by `CadProjectionPlugin` (CRITICAL #2) — a `gfx::Plugin` canary is the natural second proof point but is no longer the bottleneck for §10.4 dogfood-rule verification.

**Risk note**: PLAN explicitly says "Many architectures die here. This is where v0.6's CAD/ECS impedance fix gets tested by reality." Phase 7 dispatches need careful boundary-keeping.

### Option E — Phase 3.3+3.4 formal hot-reload bench gates

**Goal**: rewire `script-bench`'s 4 criterion benches against real `script-host` + a 1000-entity Counter fixture; close the formal Phase 3 exit gates.

**State**: Phase 3.2 substrate proven (script-host swap window 0.31ms in debug = 320× headroom on 100ms gate). The criterion benches in `crates/script-bench/benches/{cold_start,hot_reload_swap,memory_overhead,script_tick_1m}.rs` exist as code but are driven by `engine_stub.rs` placeholders. Formal Phase 3 exit criteria (per IMPLEMENTATION.md):
- Hot-reload p95 < 100ms on a **1000-entity scene** (substrate proven on 1-entity smoke; needs scaling)
- ECS iteration via WASM ≤ **1.5×** native Rust
- **1-hour** session without memory leak
- Component data preserved across **100 hot-reload cycles**

**Polish work** — substrate validated; this closes formal measurement debt + appends BASELINE.md.

## Persistent gaps (carry-over — none of B/C/D/E directly addresses, but worth tracking)

- **5 empty kernel stubs** (shared, asset-view, asset-streaming, io-scheduler, job-system) — partial subset addressed by future Phase 5+ work; plugin-host shipped 2026-05-07 (Option C)
- **`physics` has no kernel/diagnostics integration** (uses inline `physics_input_ledger::PhysicsInputLedger` per-tick domain ledger separate from `kernel/audit-ledger`'s generic event ledger; see physics_input_ledger.rs module-doc for the divergence rationale) — small refactor, not pressing
- **10 of 27 §18 companion docs missing** (was 13; +3 landed 2026-05-09: KERNEL_AUDIT_LEDGER.md / KERNEL_APP_FRAME_LOOP.md / CAD_CORE_KERNEL_ADAPTERS.md). Notable absences: RECOVERY_MODEL.md / EXECUTION_DOMAINS.md / KERNEL_SCHEDULE.md / KERNEL_TYPES.md / RUNTIME_ORCHESTRATOR.md / IO_FORMATS.md / etc. — governance debt; tackled in chunks. ADRs now: ADR-112 + ADR-098 + ADR-104 + ADR-114 (with two amendments) all landed; **ADR-097/101 still unwritten**.
- **`cargo bench` not wired in CI** — formal Phase 3 perf gates unrun (Option E addresses)
- **WASM cold-start baseline (904µs) measured on wasmtime 23**, not re-validated post bump to 44 — small re-run task
- **`io-3mf` crate entirely missing** from workspace despite PLAN §1.6.5 listing it as required
- **kernel/ecs snapshot warning routing** — currently uses `tracing::warn!` for unregistered components; could route through `&mut dyn DiagnosticSink` (deferred to align with future broader diagnostic-routing pass)
- **8 empty `docs/*` subdirectories** (PLAN-mandated placeholders; `.gitkeep` could make them git-trackable but not pressing)

## How to resume

1. **Verify env**: cargo at `A:\RustCache\cargo\bin\cargo.exe` (NOT on PATH); set `CARGO_HOME=A:\RustCache\cargo`, `RUSTUP_HOME=A:\RustCache\rustup`. Run from `A:\RCAD\RGE\`.
2. **Verify state matches this doc**: `cargo run -q -p rge-tool-architecture-lints -- all` should exit 0; `cargo test --workspace --all-targets --no-fail-fast` should report 1702 passed.
3. **Pick a dispatch option** (B/C/D/E above). Each has the spec inline; turn it into an Agent prompt with the same template structure as prior dispatches.
4. **After dispatch completes**: verify all 9 lints PASS, run workspace tests, append entries to [`change.md`](./change.md) with timestamp + test count delta + LLVM lines + any complications, update [`Status.md`](./Status.md) with new state, update [`README.md`](./README.md) test count if changed.

## Architectural-debt registry (post-2026-05-07 deep audit)

The 5-parallel-agent deep audit on 2026-05-07 surfaced architectural gaps not covered by the 9-lint enforcement. They decompose along three axes:

### Axis 1 — Temporal consistency model
Snapshot system is incomplete; graph-based stateful Tier-2 substrates are not all participating in PIE; referential integrity across capture/restore not enforced.

- **~~CRITICAL #1~~ DONE 2026-05-07**: `CadGraph` impls `SnapshotParticipate` via RON-based capture/restore in `crates/cad-core/src/checkpoints/participate.rs`; ParticipantId `cad-core.cad-graph`; new `CadProjection::validate_handles(&CadGraph) -> Vec<(EntityId, NodeId)>` for divergent-restore orphan detection. PLAN §13.2 gate ("all stateful Tier-2 has SnapshotParticipate") closed for cad-core. 9 tests including PIE full round-trip with both `[&cad, &projection]` participants + divergent-state smoke verifying `tick(&empty_cad)` returns `ProjectionError::NodeNotInGraph` (not panic).

### Axis 2 — Capability-based execution model
Plugin substrate is not real yet; current `PluginContext { &mut dyn DiagnosticSink }` is a logger, not a context; no stable ABI boundary.

- **~~CRITICAL #2~~ DONE 2026-05-07**: `PluginContext` v1 — type-erased resource registry (`BTreeMap<TypeId, Box<dyn Any + Send>>`) with `insert<T>` / `get_mut<T>` / `take<T>` / `contains<T>` / `with_resource<T>` builder. **Owned-resources-handoff** design (not borrowed references) keeps plugin-host Tier-1 with no `unsafe`. Existing `PluginContext::new(diagnostics)` v0 API bit-identical. First real Tier-2 plugin canary `CadProjectionPlugin` lives in `crates/cad-projection/src/plugin_adapter.rs` and exercises full lifecycle through PluginHost. 16 tests; kernel/plugin-host 23 → 33; cad-projection 28 → 34.
- **~~LOW #5~~ DONE 2026-05-08**: `PluginHost::init_all` / `tick_all` / `shutdown_all` auto-emit `Diagnostic::error` on plugin Err with structured `"plugin <id> {phase} failed: <err>"` prefix. Plugin-fatal isolation preserved (additive). 5 regression tests; plugin-host 33 → 38.

### Axis 3 — Unified data model
Parallel/duplicated representations (labeled vs unlabeled mesh; handle vs map cad_node) create dual-source-of-truth drift and pipeline composability failures.

- **~~HIGH #3~~ DONE 2026-05-08**: Unified Mesh refactor. `Tessellation { positions, indices, face_labels: Option<Vec<TopologyFaceId>> }` — single type. `LabeledMesh` deleted; `TopologyFaceId` moved tessellation::mesh; `BooleanOp::evaluate` + `infer_lineage` collapsed to one signature each that dispatches on labeled-state. The labeled-mesh substrate now composes through `OperatorGraph::evaluate` end-to-end. cad-core 155 → 164 tests; +9 net.
- **~~MEDIUM #4~~ DONE 2026-05-08**: `BRepHandle.cad_node` field dropped; `EntityCadMap` is the sole authoritative owner. New `CadProjection::{node_for, entity_for, remap_entity}` accessors expose the SSoT pattern. 5 regression tests; cad-projection 34 → 39. **Axis 3 fully closed.**

### Deferred (defensible until trigger fires)

- **`KernelCapabilities` struct**: doc-comment-only declaration acceptable until second CAD kernel lands (truck per ADR-113-deferred placeholder) or editor-ui needs to filter operator picker by capability.
- **`LineageGraph` as `kernel/graph-foundation::Graph`**: Vec backing acceptable until consumers materialize requiring traversal queries beyond linear history (constraint inheritance, conflict markers per PLAN §1.5.4.3).

### Dispatch order (post-audit)

1. **~~CRITICAL #1~~ DONE 2026-05-07** — `CadGraph::SnapshotParticipate` impl + handle-validation guard
2. **~~CRITICAL #2~~ DONE 2026-05-07** — `PluginContext` v1 capability registry + `cad-projection::Plugin` canary
3. **~~HIGH #3~~ DONE 2026-05-08** — Unified Mesh refactor (closes labeled/unlabeled duality, Pairing-1+8)
4. **~~MEDIUM #4~~ DONE 2026-05-08** — `BRepHandle` single-source-of-truth (closes Pairing-6)
5. **~~LOW #5~~ DONE 2026-05-08** — Plugin diagnostic auto-emit (closes Pairing-5)
6. **~~Test-gap-followup~~ DONE 2026-05-08** — Audit 2's bounded test-coverage gaps. 5 new integration test files (15 explicit tests + 16 fmt-incidental backfills); workspace 1587 → 1618 (+31 net). csgrs `catch_unwind` shield classified **defensive-only-no-known-trigger** at csgrs 0.20.1; cross-substrate 100-iter PIE-determinism soak byte-identical; CadGraph corruption-recovery atomic; RevolveOp `new↔partial(2π)` hash equality verified; all 5 ProjectionError variants exercised.
7. **~~gfx::Plugin canary~~ DONE 2026-05-08** — second real Tier-2 plugin (proves PluginContext v1 design isn't cad-projection-specific; ADR-114 followup). 16 tests; gfx 44 → 60; workspace 1618 → 1634. **Design-generalization data point**: wgpu 29 core types Send+Sync, no Mutex/unsafe needed; owned-handoff pattern from cad-projection generalized cleanly. Lazy-build-on-first-tick pattern surfaced as a useful template for future canaries.
8. §18 companion docs (governance debt; substrates stable: GRAPH_FOUNDATION.md / CAD_TOPOLOGY_LINEAGE.md / PLUGIN_API.md / CAD_CORE_MODEL.md)
9. Remaining kernel stubs (shared / asset-view / asset-streaming / io-scheduler / job-system)

**All 5 audit-1 architectural-debt findings + all 5 audit-2 phases shipped + four-substrate canary proof CLOSED + ADR-114 carries TWO amendments + 17 of 27 §18 companion docs landed + physics+audio failure-class declarations land + 23-of-81 audit-1 rollout-debt exemptions cleared. Deep audit 2026-05-09 surfaced 4 CRITICAL + 4 HIGH findings; ALL CLOSED via 4 corrective dispatches (Phase 5 split + forbidden-dep + test-coverage + DependencyGraph migration). 3 NEW findings surfaced during corrective work — track in debt registry.** **Open audit-debt registry post-corrective-round** (3 NEW findings + carryover MEDIUMs):

- **~~kernel_isolation.rs same-class rge- prefix~~ DONE 2026-05-09 03:40** — orchestrator inline fix; `is_io_crate` extended with `pkg.name.starts_with("rge-io-")` OR `starts_with("io-")` — both name-prefix paths now active; bare-name fixture-test convention preserved for backward compat; 1 new test `test_rge_prefixed_io_crates_overlap_detected_via_name_path` exercises rge-io- name-path explicitly via `pkgs/foo/Cargo.toml` (where manifest-path fallback can't fire). 6 → 7 tests; workspace 1695 → 1696.
- **NEW (carryover from test-coverage dispatch)**: `host.rs` at 1899L approaching natural split point (was 1766L pre-dispatch; +133L from Gap 4 init+shutdown leak-detection tests). SPLIT-EXEMPTION still honored, but future test additions may need compaction or splitting tests into a sibling integration-test file (Phase 5 pattern). **MEDIUM**.
- **~~graph-foundation lint coverage gap~~ DONE 2026-05-09 04:45** — Check 2 added detecting `BTreeMap<K, BTreeSet<K>>` / `HashMap<K, HashSet<K>>` adjacency-pair shapes via syn AST walk + native PartialEq for K==V comparison. Bonus catch: surfaced `asset-store::DepGraph` (not in original audit scope); migrated to `Graph<AssetId, ()>` mirroring kernel/asset Option B template; exemption deleted post-migration; 62/62 asset-store tests pass unchanged.
- **Carryover MEDIUM batch from deep audit**:
  - Cargo.toml dep-style normalization (4 canaries use 3 patterns: path / mixed / workspace=true)
  - physics local AuditLedger stub migration (W11 stub-migration debt)
  - `#[allow(...)]` reason= convention (3 sites have it; ~86 sites don't; never enforced)
  - Clippy pedantic warnings in canary tests (8 fresh warnings from gfx + audio + physics + cad-projection)
  - csgrs catch_unwind recovery branch never exercised (defensive-only-no-known-trigger)
  - PluginError × PluginPhase coverage gaps (ContractViolation × Init/Shutdown auto-emit; RuntimeFault × Init/Shutdown auto-emit untested)

**Recommended next dispatches** (rank order):

1. ~~CRITICAL kernel_isolation.rs same-class rge- prefix fix~~ DONE 2026-05-09 03:40
2. ~~HIGH graph-foundation lint extension + asset-store migration~~ DONE 2026-05-09 04:45
3. ~~MEDIUM batch (partial closure)~~ DONE 2026-05-09 05:25
4. ~~physics AuditLedger option-(b) rename~~ DONE 2026-05-09 06:35
5. **§18 docs pack 7** (KERNEL_SCHEDULE.md / KERNEL_TYPES.md / RUNTIME_ORCHESTRATOR.md; 10 of 27 still missing)
6. **host.rs pre-emptive split** (1899L SPLIT-EXEMPTION-honored; Phase 5 tests-sub-module pattern; pair with PluginError×PluginPhase 4-cell auto-emit tests to avoid further size growth)
7. **PluginError×PluginPhase 4-cell auto-emit tests** (audit-2 coverage gap — ContractViolation × Init/Shutdown + RuntimeFault × Init/Shutdown; pair with host.rs split)
8. **csgrs catch_unwind recovery branch test** (needs feature-flag design)
9. **clippy pedantic in physics+audio libs** (~17 warnings; small mechanical clean-up)
10. **cad-projection broader dep-style sweep** (rge-kernel-ecs + rge-kernel-graph-foundation; small mechanical)
11. **editor-ui::Plugin canary** (defer until editor-ui Phase 5 stabilises singleton shape)
12. **Remaining kernel stubs** (5 stubs)

## Reference index

- [`Status.md`](./Status.md) — live snapshot of current state, validation gates, immediate-next-job recommendations
- [`README.md`](./README.md) — public-facing project status + 9-lint table + workspace structure
- [`change.md`](./change.md) — running history (chronological, append-only)
- [`plans/PLAN.md`](./plans/PLAN.md) — architecture (frozen at v0.8)
- [`plans/IMPLEMENTATION.md`](./plans/IMPLEMENTATION.md) — phase ordering and de-risking gates
- [`plans/BASELINE.md`](./plans/BASELINE.md) — perf baselines (W03 PIE / Phase 3.2 script-host swap / Phase 5.3 PIE re-baseline / W04 wasmtime cold-start)
- [`plans/fileandfolderstructure.md`](./plans/fileandfolderstructure.md) — workspace layout spec
- [`tools/architecture-lints/exemptions.toml`](./tools/architecture-lints/exemptions.toml) — exemption registry (1 substantive + 60 failure-class rollout debt)
- [`versions.md`](./versions.md) — workspace dep table + MSRV (toolchain pinned 1.92.0)

## Operating conventions established this run

- **Dispatch pattern**: bounded scope agent prompts with explicit "files you MAY modify" / "files you MUST NOT modify"; report-back template ≤ 300-400 words; verification commands inline
- **Parallel dispatch**: orchestrator stays off shared files (`exemptions.toml`, `main.rs`, `common.rs`) during multi-agent rounds; clears them between rounds
- **Status.md / change.md discipline**: every dispatch ends with a Status.md update + change.md append; deep audits catch drift periodically
- **9-lint exit-0 ritual**: every dispatch ends with `cargo run -p rge-tool-architecture-lints -- all` exit 0 verified; `cargo +nightly fmt --check` exit 0 verified; full workspace test count tracked
- **Deep-audit cadence**: every ~5–10 dispatches the orchestrator runs a 5-parallel-agent read-only audit covering (1) architectural coherence, (2) test coverage smells, (3) doc drift, (4) code smells, (5) cross-architecture coherence — findings consolidated into a single cleanup-pass dispatch + separate dispatches for architectural debt
- **DONE-marking ritual**: when a dispatch completes, the corresponding "Next-job options" / "Subsequent dispatches" entry in HANDOFF.md/Status.md is rewritten with `~~strikethrough header~~ DONE YYYY-MM-DD` + a one-paragraph summary; the unstruck spec body is removed
- **Fmt CI**: `cargo +nightly fmt --check` — workspace uses nightly-only `imports_granularity = "Module"` + `group_imports = "StdExternalCrate"`; orchestrator runs `cargo +nightly fmt --all` after dispatches that add new files
- **Failure-class taxonomy**: 5 classes per PLAN §1.13 — recoverable / snapshot-recoverable / plugin-fatal / session-fatal / kernel-fatal. Per PLAN line 572 scheduler deadlock = kernel-fatal; per line 573 audit-ledger checksum fail = kernel-fatal.
