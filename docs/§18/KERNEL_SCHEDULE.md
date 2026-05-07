# KERNEL_SCHEDULE

| Companion to | PLAN.md §6 (frame loop / runtime heartbeat) + PLAN.md §1.6.8 (Replay-Stable v1.0 determinism mode) + PLAN.md §1.13 (failure-class taxonomy) + IMPLEMENTATION.md Phase 1.5 (kernel-schedule exit criteria) |
|---|---|
| Status | Stable v1; 24 tests passing (9 unit tests across `stage` / `system` / `schedule` modules + 15 integration tests in `kernel/schedule/tests/schedule_test.rs` covering all 13 IMPLEMENTATION.md Phase 1.5 scenarios + 2 supplementary cases); single-threaded synchronous execution with declared `AsyncBoundary` metadata for future async scheduler |
| Audience | Tier-2 authors registering systems against frame phases (cad-projection, gfx, physics, audio canaries today; editor-actions / script-host etc. tomorrow); orchestrator authors composing multi-system per-stage execution; reviewers verifying replay-stable execution order |
| Sibling doc | `KERNEL_APP_FRAME_LOOP.md` — frame-loop substrate; `App::run_frame` invokes the per-phase runner closure that typically drives `Schedule::run` for each stage (`Stage` + `FramePhase` are independent enums; the orchestrator maps between them); `KERNEL_DIAGNOSTICS.md` — every system receives `&mut dyn DiagnosticSink` for failure-routing |
| Reference impls | `kernel/schedule/src/{lib,schedule,stage,system}.rs` (substrate) · `kernel/schedule/tests/schedule_test.rs` (15-case integration suite) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the deterministic single-threaded scheduler substrate; subsystem-specific system registration patterns (e.g. how cad-projection registers its `BRepHandle`-update system, how physics registers its fixed-step) belong in their sibling §18 docs.

## 1. Why a substrate

Without a substrate, every subsystem authoring per-frame work would invent its own ordering rules. Sub-A would assume "I run before sub-B because I called `add` first"; sub-B would assume "I'm guaranteed to see sub-C's output because I'm in stage Update". Three months in, registration-order becomes load-bearing for correctness, the engine is one refactor away from a Heisenbug, and replay-stable golden tests are off the table.

PLAN §6 commits to **one canonical scheduler** with three load-bearing properties:

- **Determinism via Kahn's algorithm + alphabetical tiebreak.** Two `Schedule`s with the same registered systems and same dependency edges produce **identical** execution orders, regardless of insertion order. This is the substrate the Replay-Stable v1.0 determinism mode (PLAN §1.6.8) is built on.
- **Stage-isolated topological sort.** Each [`Stage`] has its own dependency graph; cross-stage forward edges are accepted (an `Update` system depending on an `EarlyUpdate` system is fine — stage order satisfies the edge implicitly), cross-stage **back-edges** are rejected at `build` time.
- **Single-threaded synchronous execution.** Phase 1.5 is the synchronous baseline; the [`AsyncBoundary`] metadata is **forward-declared but not yet acted on**. A future scheduler may inspect `AsyncBoundary::Async` to insert sync points or schedule on an async executor; today every system runs on the calling thread.

PLAN §1.13 line 572 promotes scheduler-detected deadlock to **kernel-fatal** — see §10.

## 2. `Stage` enum

Lives at `kernel/schedule/src/stage.rs`. Four ordered execution stages:

```rust
#[repr(u8)]
pub enum Stage {
    EarlyUpdate = 0,    // input processing, pre-simulation setup
    FixedUpdate = 1,    // fixed-timestep simulation (physics, deterministic logic)
    Update      = 2,    // general-purpose per-frame logic
    LateUpdate  = 3,    // post-processing, rendering prep, UI layout
}
```

> **Source-truth flag:** `Stage` is a four-variant enum (`EarlyUpdate` / `FixedUpdate` / `Update` / `LateUpdate`), distinct from `kernel/app::FramePhase` (six variants: `Input` / `FixedSim` / `Update` / `LateUpdate` / `StageRender` / `EndFrame`). The two enums **do not** share a common type — the orchestrator maps `FramePhase::FixedSim` onto `Stage::FixedUpdate`, `FramePhase::Update` onto `Stage::Update`, and so on, in the per-phase runner closure passed to `App::run_frame`. They are separate enums by design: `Stage` is scheduler ordering; `FramePhase` is loop dispatch.

Iteration via the `&'static [Stage]` constant:

```rust
impl Stage {
    pub const ALL: &'static [Stage] = &[
        Stage::EarlyUpdate, Stage::FixedUpdate,
        Stage::Update,      Stage::LateUpdate,
    ];
    pub const fn label(self) -> &'static str;
}
```

`Stage::ALL` is the canonical iteration order — `Schedule::build` walks it; `Schedule::run` walks it; `execution_order` walks it. New stages (none planned in v1) must be inserted at a stable discriminant in `ALL` so existing serialized order strings remain valid. The three unit tests (`all_is_sorted`, `ordering_is_correct`, `label_is_non_empty`) regression-pin the canonical ordering + label discipline.

## 3. `SystemId` + `AsyncBoundary`

Lives at `kernel/schedule/src/system.rs`.

```rust
pub struct SystemId(pub &'static str);
```

A stable identifier interned via a `&'static str` name. `Ord + PartialOrd` are lexicographic on the inner string — this is the **alphabetical tiebreak** used by the Kahn topo-sort to disambiguate when multiple zero-in-degree nodes are eligible simultaneously (§5). `Display` writes the bare name.

```rust
pub enum AsyncBoundary { Sync, Async }
```

Per-system metadata declaring whether the system performs async I/O / yield points. **Currently metadata only** — Phase 1.5 always runs systems synchronously on the calling thread. A future scheduler may use `AsyncBoundary::Async` to insert sync points or dispatch to an async executor; today the variant is recorded for forward compatibility and tested via the `test_async_boundary_metadata_stored` integration test.

## 4. `SystemDescriptor`

```rust
pub struct SystemDescriptor {
    pub id: SystemId,
    pub stage: Stage,
    pub depends_on: Vec<SystemId>,
    pub async_boundary: AsyncBoundary,
    pub run: SystemFn,    // Box<dyn FnMut(&mut dyn DiagnosticSink) + Send>
}
```

The registration record. Constructed via `SystemDescriptor::new(id, stage, run_fn)` and refined with `with_dependency(...)` / `with_async_boundary(...)` builder methods:

```rust
let sys = SystemDescriptor::new(SystemId("physics-step"), Stage::FixedUpdate, |sink| {
        // do work; emit diagnostics through sink on failure paths
    })
    .with_dependency(SystemId("input-drain"))
    .with_async_boundary(AsyncBoundary::Sync);
```

> **Source-truth flag:** the dispatch spec described `SystemDescriptor` as having "ordering rules" with explicit `runs_after` / `runs_before` keywords. Source-truth: there is **only `depends_on: Vec<SystemId>`** (the runs-after relation; "X depends on Y" means "Y runs before X"). Cross-stage forward dependencies are accepted implicitly; cross-stage back-edges are rejected. There is no `runs_before` accessor — the inverse edge is encoded by adding the dependency from the other side.

The `run` callback receives `&mut dyn DiagnosticSink` so systems route failures into the unified diagnostic stream per `KERNEL_DIAGNOSTICS.md`. The `Send` bound is required so descriptors can later be shipped across threads when an async scheduler lands; today execution stays on the calling thread.

The `Debug` impl is hand-rolled (`finish_non_exhaustive`) because the `run` field is a `Box<dyn FnMut>` which doesn't implement `Debug`.

## 5. `Schedule` — Kahn's algorithm with alphabetical tiebreak

Lives at `kernel/schedule/src/schedule.rs`.

```rust
pub struct Schedule {
    systems: Vec<SystemDescriptor>,                 // owned, in insertion order
    built: bool,                                    // gate for run()
    stage_order: BTreeMap<Stage, Vec<usize>>,       // post-build deterministic order
}
```

### Public surface

```rust
impl Schedule {
    pub fn new() -> Self;
    pub fn add_system(&mut self, descriptor: SystemDescriptor) -> Result<(), ScheduleError>;
    pub fn build(&mut self) -> Result<(), ScheduleError>;
    pub fn run(&mut self, sink: &mut dyn DiagnosticSink) -> Result<(), ScheduleError>;
    pub fn execution_order(&self) -> Result<Vec<SystemId>, ScheduleError>;
    pub fn system_count(&self) -> usize;
}
```

> **Source-truth flag:** the dispatch spec listed "build / register / run" as the public API. Source-truth: the registration method is **`add_system`** (not `register`). Otherwise `build` and `run` match. There is also an additional `execution_order` accessor (used by tests + future tooling) that returns the deterministic `Vec<SystemId>` produced by `build`.

### Lifecycle

1. **Register** — `add_system(descriptor)` pushes onto `systems`, sets `built = false`, clears `stage_order`. Duplicates (same `SystemId` already registered) return `ScheduleError::DuplicateSystem`.
2. **Build** — `build()` validates every dependency edge, then runs Kahn's algorithm per stage. Sets `built = true`. Calling `add_system` after `build` resets the flag — you must rebuild before the next `run`. The `test_add_after_build_requires_rebuild` regression test pins this.
3. **Run** — `run(sink)` requires `built == true`; flat-iterates `Stage::ALL` × `stage_order[stage]` and invokes each system's `run` callback once. Returns `ScheduleError::NotBuilt` if called pre-build (or post-`add_system` without a re-build).

### `build` algorithm

```text
1. Build SystemId → (index, stage) lookup in a BTreeMap.
2. Validate every dep edge:
   - If dep_id is not registered, error MissingDependency.
   - If dep_id's stage > sys's stage, error MissingDependency
     (cross-stage back-edge — would violate stage ordering).
   - Cross-stage forward edges (dep stage < sys stage) are satisfied
     implicitly by stage iteration order; no intra-stage edge added.
3. For each stage in Stage::ALL:
   a. Filter systems belonging to this stage (stage_indices).
   b. Build local in-degree + adjacency (intra-stage deps only).
   c. Seed BTreeSet<(SystemId, local_idx)> with zero-in-degree nodes.
      The BTreeSet ordering is (SystemId-lexicographic, then local-idx),
      which gives the alphabetical tiebreak property for free.
   d. Pop-first repeatedly; for each popped node, decrement successors'
      in-degree and re-insert if their in-degree hits zero.
   e. If sorted_globals.len() != local_count, a cycle exists in this
      stage — collect IDs with non-zero in-degree and error Cycle.
4. Store stage_order; set built = true.
```

The `BTreeSet<(SystemId, usize)>` is the keystone of the determinism guarantee: a `BinaryHeap` would tie-break on insertion order (non-deterministic across runs); `BTreeSet::pop_first` returns the lexicographically smallest entry every time. Combined with `BTreeMap<Stage, Vec<usize>>` for stage iteration (also lexicographically ordered by `Stage`'s `Ord` impl), the entire `build` is deterministic.

## 6. Determinism guarantees + cross-ref Replay-Stable v1.0

The substrate's contract: **same registered systems + same dependency edges → identical execution order, every time, on every machine.** Specifically:

- Stages execute in `Stage::ALL` order (ascending discriminant: `EarlyUpdate < FixedUpdate < Update < LateUpdate`). The `test_04_stage_isolation` integration test pins this.
- Within each stage, Kahn's topo-sort produces a unique order given the dependency graph.
- When multiple zero-in-degree nodes are eligible, the lexicographically smallest `SystemId` runs first (the `BTreeSet::pop_first` tiebreak). The `test_03_insertion_independent_order` test pins this — register `["charlie", "bravo", "alpha"]`, expect execution order `["alpha", "bravo", "charlie"]`.
- Two freshly-built `Schedule`s with the same `SystemDescriptor` set produce the same `execution_order()` output. The `test_12_ten_system_smoke_test` covers a 10-system multi-stage scenario with multiple intra-stage deps + a cross-stage forward dep, asserting identical orders across two independent builds.

This is the property that makes PLAN §1.6.8 Replay-Stable v1.0 feasible: a per-frame system invocation order that is byte-identical across runs is the prerequisite for byte-identical event-log replay (`KERNEL_AUDIT_LEDGER.md` §11). Without scheduler determinism, two replays of the same frame would interleave system effects differently.

## 7. `ScheduleError`

```rust
pub enum ScheduleError {
    DuplicateSystem(SystemId),
    Cycle(Vec<SystemId>),
    MissingDependency { dependent: SystemId, missing: SystemId },
    NotBuilt,
}
```

- **`DuplicateSystem(id)`** — `add_system` rejects a registration whose `SystemId` is already in the schedule. The `test_09_duplicate_system` integration test pins this.
- **`Cycle(ids)`** — `build` detects a cycle within one stage; `ids` lists all `SystemId`s with non-zero remaining in-degree after Kahn's terminates. The `test_08_cycle_detect` test pins the two-system case (a→b, b→a both in `Stage::Update`) and asserts both ids appear in the error.
- **`MissingDependency { dependent, missing }`** — `build` saw a `depends_on` reference to either an unregistered system or a system in a later stage (cross-stage back-edge). The `test_07_cross_stage_backward_dep_error` (back-edge) and `test_10_missing_dependency` (unregistered) tests pin both arms; both surface as the same variant by design (the back-edge is conceptually "the late-stage system isn't visible in the early-stage view, so it's missing from this stage's perspective").
- **`NotBuilt`** — `run` or `execution_order` was called without a successful preceding `build`. The `test_11_run_before_build` test pins this.

All four variants derive `thiserror::Error`; the workspace prints them through the diagnostic substrate when surfaced from build failures.

## 8. Diagnostic flow-through

Each system's `run` callback receives `&mut dyn DiagnosticSink` (from `kernel/diagnostics`). The `test_13_diagnostics_flow_through` integration test pins the routing: register a system whose body emits `Diagnostic::warning("system emitted a warning")`, drive `Schedule::run(&mut aggregator)`, assert the aggregator captured exactly one warning with that message.

This is the per-system route into the unified failure stream. A system that needs to report "I observed an inconsistency this frame" emits via the supplied sink; the frame loop's `phase_runner` closure (per `KERNEL_APP_FRAME_LOOP.md` §4) passes the same sink down so per-phase / per-system / per-frame diagnostics converge on one stream. Systems that don't need to emit can ignore the parameter; the `()` no-op `DiagnosticSink` impl supports tests that pass `&mut ()`.

## 9. The 24-test coverage breakdown

The 24 tests split across two test surfaces:

### Unit tests in `src/` (9 total)

- `stage.rs` (3): `all_is_sorted`, `ordering_is_correct`, `label_is_non_empty`.
- `schedule.rs` (6): `new_schedule_is_empty`, `default_equals_new`, `add_system_increments_count`, `duplicate_system_errors`, `run_before_build_errors`, `execution_order_before_build_errors`.
- `system.rs` — no `#[cfg(test)]` block today (the integration suite covers `SystemDescriptor` end-to-end).

### Integration tests in `tests/schedule_test.rs` (15 total)

The file's module-doc declares "Covers all 13 required test cases from IMPLEMENTATION.md Phase 1.5", numbered `test_01` through `test_13`. Two supplementary tests close out the suite at 15:

| # | Test | Property |
|---|---|---|
| 1 | `test_01_stage_all_is_sorted` | `Stage::ALL` ascending |
| 2 | `test_02_single_system_executes` | `run` invokes the registered callback |
| 3 | `test_03_insertion_independent_order` | Alphabetical tiebreak across 3 systems |
| 4 | `test_04_stage_isolation` | Stage-order respected (early < update < late) |
| 5 | `test_05_intra_stage_dep` | Intra-stage `depends_on` enforced |
| 6 | `test_06_cross_stage_forward_dep_ok` | Update→EarlyUpdate dep accepted |
| 7 | `test_07_cross_stage_backward_dep_error` | EarlyUpdate→Update dep rejected |
| 8 | `test_08_cycle_detect` | Two-cycle (a→b, b→a) detected; both ids reported |
| 9 | `test_09_duplicate_system` | `DuplicateSystem` on repeat-id |
| 10 | `test_10_missing_dependency` | `MissingDependency` for unregistered dep |
| 11 | `test_11_run_before_build` | `NotBuilt` on unbuild run |
| 12 | `test_12_ten_system_smoke_test` | 10 systems × 4 stages × multi-deps; determinism across two builds |
| 13 | `test_13_diagnostics_flow_through` | Per-system warning reaches aggregator |
| 14 | `test_async_boundary_metadata_stored` | `AsyncBoundary::Async` survives builder |
| 15 | `test_add_after_build_requires_rebuild` | `add_system` post-build resets `built` |

Together the 24 tests cover every public method, every error variant, the determinism contract across two independent builds, and the diagnostic flow-through.

## 10. Failure class — kernel-fatal

`kernel/schedule/src/lib.rs` lines 1–11 declare:

```rust
//! Failure class: kernel-fatal
//!
//! Per PLAN.md §1.13 (line 572): a deadlock detected by the scheduler is
//! kernel-fatal — the engine cannot recover and must exit. API-level errors
//! (duplicate-system registration, dependency cycle at build time, missing
//! dependency) are caught BEFORE `run()` and surface as `ScheduleError`; those
//! are recoverable for the caller. The kernel-fatal class applies to runtime
//! invariant violations during `run()` (deadlock, system panic that the
//! supervisor cannot quarantine).
```

The class is **scoped**: build-time errors (`DuplicateSystem`, `Cycle`, `MissingDependency`, `NotBuilt`) are caller-recoverable — fix the registration and rebuild. The kernel-fatal escalation applies specifically to `run()`-time invariants:

- A scheduler-detected deadlock (Phase 1.5 is single-threaded synchronous, so deadlock requires a future async-execution path — kept on the menu defensively).
- A system panic the supervisor cannot quarantine. Currently `run` does **not** catch panics — a panicking system unwinds into the orchestrator, which is responsible for recovery (`kernel/plugin-host` does its own `catch_unwind` per `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §5; non-plugin systems run un-shielded today).

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/schedule` does not appear in `tools/architecture-lints/exemptions.toml`.

## 11. Cross-substrate composition

The substrate composes with the rest of the kernel as follows:

- **`kernel/app`** — `App::run_frame` invokes a per-phase runner closure for each `FramePhase`. The orchestrator typically constructs one `Schedule` per `Stage` (or one `Schedule` covering all stages and runs it during `FramePhase::Update`); the mapping `FramePhase ↔ Stage` is the orchestrator's responsibility (the two enums are independent). See `KERNEL_APP_FRAME_LOOP.md` §10.
- **`kernel/diagnostics`** — every `SystemDescriptor::run` callback receives `&mut dyn DiagnosticSink`; per-system diagnostics flow through the unified stream. See `KERNEL_DIAGNOSTICS.md` §10.
- **`kernel/plugin-host`** — plugins are not `SystemDescriptor`s; `Plugin::tick` is the per-frame entry, invoked by `PluginHost::tick_all`. The orchestrator may register a single `SystemDescriptor` whose body calls `host.tick_all(&mut ctx)` so plugins participate in the scheduler's deterministic order alongside non-plugin systems.

## 12. References

- **PLAN.md §6** — frame loop / runtime heartbeat.
- **PLAN.md §1.6.8** — Replay-Stable v1.0 determinism mode; the byte-identical replay gate that scheduler determinism enables.
- **PLAN.md §1.13 line 572** — failure-class taxonomy; "scheduler deadlock = kernel-fatal".
- **IMPLEMENTATION.md Phase 1.5** — kernel-schedule exit criteria; the 13 required test cases pinned by `test_01` through `test_13`.
- **`KERNEL_APP_FRAME_LOOP.md`** — sibling §18 doc; frame-loop substrate. `FramePhase` (six variants) is distinct from `Stage` (four variants); the orchestrator maps between them in the per-phase runner closure.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; per-system diagnostic routing through `DiagnosticSink`.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; plugins integrate via `tick_all` rather than via direct `SystemDescriptor` registration.
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; deterministic event-id substrate that complements scheduler determinism for the Replay-Stable v1.0 gate.
- **`kernel/schedule/src/lib.rs`** — module roots + failure-class declaration + design summary.
- **`kernel/schedule/src/stage.rs`** — `Stage` enum + `ALL` + `label` + 3 unit tests.
- **`kernel/schedule/src/system.rs`** — `SystemId` + `AsyncBoundary` + `SystemDescriptor` + `Debug`/builder methods.
- **`kernel/schedule/src/schedule.rs`** — `Schedule` + `ScheduleError` + `SystemFn` + Kahn's algorithm with `BTreeSet` tiebreak + 6 unit tests.
- **`kernel/schedule/tests/schedule_test.rs`** — 15 integration tests covering all 13 IMPLEMENTATION.md Phase 1.5 cases + 2 supplementary cases.
