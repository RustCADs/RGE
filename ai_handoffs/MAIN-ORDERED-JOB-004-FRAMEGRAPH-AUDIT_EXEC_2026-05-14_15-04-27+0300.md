# Execution Report

DISPATCH_ID: MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_15-04-27+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_TASK_2026-05-14_03-37-03+0300.md — TASK consumed.
- ai_handoffs/MAIN-ORDERED-JOB-003-STATUS-RECONCILE_CLOSEOUT_2026-05-14_14-46-29+0300.md — Job 3 dependency, STATUS: CLOSED.
- ai_handoffs/MAIN-ORDERED-SERIAL_STATE_2026-05-14_14-46-30+0300.md — release signal: Job 4 RELEASED NOW; Jobs 5-10 HELD; stop after EXEC with `NEXT_ROLE: REVIEWER_AI`.
- crates/gfx/src/frame_graph/mod.rs — inspected (read-only).
- crates/gfx/src/frame_graph/{buffer_pool,compile,descriptor,pass,resource,resource_map,texture_pool}.rs — inspected.
- crates/gfx/tests/frame_graph_smoke.rs — inspected (test file existence + naming).
- crates/gfx/tests/frame_graph_umbrella_smoke.rs — inspected (chapter close-out claim).
- crates/editor-shell/src/render_path.rs — inspected (production consumption site).
- plans/IMPLEMENTATION.md — inspected (line 473 chapter close-out).
- plans/BASELINE.md — inspected (line 248 post-depth Gate A note re: editor-shell end-to-end deferral).
- ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_EXEC_2026-05-14_15-04-27+0300.md — this packet (the only new file).
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_TASK_2026-05-14_03-37-03+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. The Planner's TASK packet's footer `NEXT_ROLE: EXECUTOR_AI` plus the serial-state marker `MAIN-ORDERED-SERIAL_STATE_2026-05-14_14-46-30+0300.md` (release signal) plus `OPENAItoCLAUDE_2026-05-14_14-46-31+0300.md` ("Execute Job 4 only") route directly to the Executor under v2 Rule 7 from `d017a35`. Executor proceeded to execution.

## Job 3 Closeout Verification (TASK Halt Condition)

**Job 3 closeout exists.** `ai_handoffs/MAIN-ORDERED-JOB-003-STATUS-RECONCILE_CLOSEOUT_2026-05-14_14-46-29+0300.md` is present with `STATUS: CLOSED`. The dependency rule ("Job 3 closeout exists before this job executes") is satisfied.

## What I Changed

### Source
- None.

### Tests
- None.

### Docs
- `ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_EXEC_2026-05-14_15-04-27+0300.md`: created this audit execution packet (the only filesystem change).

## Per-File Summary

Read-only audit. Zero tracked-file edits. Single filesystem effect: this new untracked EXEC packet under `ai_handoffs/`.

## Verification Results

Per the TASK's `Verification Gates` section:

### Gate 1: File listing for `crates/gfx/src/frame_graph/`

```
crates/gfx/src/frame_graph/buffer_pool.rs       10892 bytes
crates/gfx/src/frame_graph/compile.rs           29174 bytes  (largest; aliasing + lifetime analysis)
crates/gfx/src/frame_graph/descriptor.rs        13113 bytes
crates/gfx/src/frame_graph/mod.rs               17877 bytes  (top-level + re-exports)
crates/gfx/src/frame_graph/pass.rs               3978 bytes
crates/gfx/src/frame_graph/resource.rs           4061 bytes
crates/gfx/src/frame_graph/resource_map.rs      14639 bytes
crates/gfx/src/frame_graph/texture_pool.rs      11288 bytes
```

→ 8 source files, ~104 KB total. The directory exists and is structurally complete.

### Gate 2: Search for frame-graph tests under `crates/gfx/tests` and inline modules

**Integration tests** (`crates/gfx/tests/`):
- `frame_graph_smoke.rs`
- `frame_graph_umbrella_smoke.rs`

**Inline `#[cfg(test)]` modules + per-file `#[test]` counts**:

| File | Inline `#[test]` count |
|---|---|
| `buffer_pool.rs` | 5 |
| `compile.rs` | 21 |
| `descriptor.rs` | 15 |
| `mod.rs` | 11 |
| `pass.rs` | 5 |
| `resource.rs` | 5 |
| `resource_map.rs` | 5 |
| `texture_pool.rs` | 5 |
| **Total inline** | **72** |

Total frame-graph test surface: 2 integration tests + 72 inline unit tests = **74 tests**.

### Gate 3: `git status --short --untracked-files=no`

→ empty output (tracked tree clean). No in-flight edits.

## Public Surface Summary

Re-exported via `crates/gfx/src/frame_graph/mod.rs`:

| Type | Module | Purpose |
|---|---|---|
| `FrameGraph` | mod.rs L129 | Top-level graph builder (`add_pass`, `compile`) |
| `FrameGraphError` | mod.rs L76 | Top-level error enum |
| `BufferPool` | buffer_pool.rs L80 | Transient buffer pool with cross-frame freshness |
| `AliasingGroup`, `AliasingGroupId` | compile.rs L88 / texture_pool.rs L45 | Lifetime-disjoint resource aliasing |
| `CompileError` | compile.rs L46 | Compilation errors |
| `CompiledFrameGraph` | compile.rs L130 | Output of compile pass; aliasing_groups + descriptors |
| `ResourceLifetime` | compile.rs L66 | Per-resource [start, end] write/read range |
| `BufferDescriptor`, `TextureDescriptor`, `ResourceClassDescriptor` | descriptor.rs | Resource shape descriptors |
| `PassNode` | pass.rs L26 | Pass-node graph element |
| `ResourceId`, `ResourceUsage` | resource.rs | Resource identity + access mode |
| `ResourceMap`, `ResourceMapError`, `build_resource_map` | resource_map.rs | Per-frame resource handle map (the production consumption entry-point) |
| `TexturePool` | texture_pool.rs L75 | Transient texture pool with N=2 ring policy |

## Production Consumption (verified `crates/editor-shell/src/render_path.rs`)

The frame-graph substrate is **actively consumed in production** in `editor-shell::render_path`:

```
L24:    build_resource_map, BufferPool, Camera as GfxCamera, CompiledFrameGraph, DepthStateKey,
L111:   ) -> CompiledFrameGraph {
L123:       let mut fg = FrameGraph::new();
L243:       let texture_pool = TexturePool::new();
L295:       // acquisition flows through `build_resource_map` which the
L312:       let map = match build_resource_map(compiled, gfx_ctx.device(), tex_pool, buf_pool) {
L317:                   "skip frame: build_resource_map: {e:?}"
L399:       // [`build_resource_map`] above. Matches the pipeline's
```

The path is end-to-end: `FrameGraph::new()` → `add_pass(...)` → `compile()` → `CompiledFrameGraph` → `build_resource_map(compiled, device, tex_pool, buf_pool)` → `ResourceMap` consumed in render passes.

## TODO / FIXME / unimplemented! Scan

```
grep -nE "TODO|FIXME|unimplemented!|todo!\(\)" crates/gfx/src/frame_graph/*.rs
```

→ **zero matches**. No deferred work markers in the frame-graph source.

## Docs Reconciliation: What the chapter already records

### `plans/IMPLEMENTATION.md:473` — frame-graph chapter close-out

> **Frame-graph chapter umbrella close-out 2026-05-12**: substrate complete + cross-frame composable. The Phase 6.1 frame-graph minimal substrate ships analytical (`FrameGraph` / `compile` / `CompiledFrameGraph::{aliasing_groups, descriptors}`) + ADR-118-pinned policy + descriptors (dispatch 119) + `TexturePool` (120) + `BufferPool` (121) + `ResourceMap` builder + `AliasingGroup::max_descriptor` (122) + umbrella analytical-composition smoke `crates/gfx/tests/frame_graph_umbrella_smoke.rs` (123). [...]
> **Pass-record-site integration (`FrameRecorder` / `record_lit_mesh_pass` consuming transient resources) is intentional future work and lands when those sites grow consumers; the substrate is complete enough to enable that wiring at zero cost.** [...]
> Runtime-perf re-validation against Gate A's recorder-host CLOSED marker (line 468 above [...]) is deferred until real pass-record sites grow transient-resource consumers; the just-shipped substrate is NOT what Gate A certified, and re-measurement is appropriate when consumer pressure surfaces.

### Umbrella smoke test docstring (`frame_graph_umbrella_smoke.rs`)

> Frame-graph chapter umbrella analytical smoke (post-dispatch-122). Composes the chapter's substrate end-to-end without touching `FrameRecorder` or real pass-record sites [...]. Asserts the cross-frame freshness invariant per ADR-118 D4 — the one compositional invariant not covered by isolated pool / map tests today. [...]
> Phase 6 chapter scope honesty: this dispatch closes the chapter for "substrate complete + cross-frame composable"; runtime-perf re-validation against Gate A's recorder-host CLOSED marker [...] is deferred until real pass-record sites grow transient-resource consumers (`FrameRecorder` is currently triangle-only and bypassed by `editor-shell::render_frame`). This is NOT a 60 fps re-run.

### Timeline reconciliation (critical for the Job 5 recommendation)

There is a subtle but important state-shift since the umbrella close-out:

1. **2026-05-12 (dispatch 123)**: Umbrella smoke landed. At that moment, "`FrameRecorder` is currently triangle-only and bypassed by `editor-shell::render_frame`" — meaning the production wire did NOT yet consume the frame-graph substrate. Pass-record-site integration was explicitly marked "intentional future work."
2. **Between then and now**: Phase 6 pass-record-integration sub-β (commit `999e9ff`, referenced in `BASELINE.md:248`) wired the production consumption: `editor-shell::render_path::render_frame` now constructs `LitMeshPipeline::new_with_depth(..)` AND acquires a per-frame depth texture via `build_resource_map(...)` over a `CompiledFrameGraph`-driven `TexturePool`. The pass-record-site integration that was "intentional future work" in `IMPLEMENTATION.md:473` HAS subsequently landed.
3. **2026-05-14 (this session)**: `03d3f05` post-depth Gate A harness exercised the depth-attached gfx primitives via `record_lit_mesh_pass(.., Some(&depth_view))` and produced a recorded measurement. The harness is **gfx-level synthetic**, NOT editor-shell `render_frame` end-to-end. BASELINE.md:248 explicitly says: "the harness exercises the gfx-level primitives that editor-shell production consumes post-sub-β; it does not exercise editor-shell's winit + `SurfaceContext` + `FrameGraph` + `build_resource_map` substrate ceremony — that remains a separate non-winit-perf-harness scope, blocked on `EditorShell::render_frame` accepting a mock event loop."

So **the frame-graph substrate itself is complete; pass-record-site integration HAS landed (`999e9ff`); the gfx-level depth measurement HAS landed (`03d3f05`)**. The remaining open gap is the **editor-shell `render_frame` end-to-end perf measurement** — but that's blocked on the editor-shell side (mock event loop), not the frame-graph side.

## Halt-Condition Checks

| Halt condition | Status |
|---|---|
| No Job 3 closeout exists | NOT TRIPPED — present at 2026-05-14_14-46-29+0300 with STATUS: CLOSED |
| Frame-graph state is ambiguous enough that Job 5 would be stale or unsafe | NOT TRIPPED — state is unambiguous: substrate complete, production-consumed, 74-test coverage, zero TODO/FIXME markers, chapter close-out recorded in IMPLEMENTATION.md:473 |
| The audit finds the docs contradict the code in a way that requires Planner correction | NOT TRIPPED — docs are accurate. The IMPLEMENTATION.md:473 close-out's "intentional future work" line refers to pass-record-site integration, which has since landed (`999e9ff`). The BASELINE.md:248 entry honestly records that the remaining gap is editor-shell `render_frame` end-to-end perf, blocked on a non-winit harness — that gap exists on the editor-shell side, not the frame-graph side. No contradiction requiring Planner correction |

## Deliverables

### Deliverable 1: `crates/gfx/src/frame_graph/` exists; public surface summarized

**Yes, exists.** See "Public Surface Summary" above. 8 source files; 14+ public types/fns; full pipeline from `FrameGraph::new()` → `compile()` → `build_resource_map(...)`.

### Deliverable 2: Smallest concrete frame-graph follow-up

**There is no warranted frame-graph follow-up at this time.** Concretely:

- The frame-graph substrate is closed by `IMPLEMENTATION.md:473` ("substrate complete + cross-frame composable").
- The pass-record-site integration that was "intentional future work" at chapter close has since landed (`999e9ff`).
- The chapter's outstanding deferral (runtime-perf re-validation against Gate A) is now partially closed by `03d3f05` (gfx-level depth-attached measurement on NVIDIA RTX 4060 Ti / min-P95 = 0.122 ms). The full editor-shell end-to-end version remains deferred, but it is blocked on **editor-shell mock-event-loop infrastructure**, not on frame-graph work.
- Zero TODO/FIXME/unimplemented! markers in the frame-graph source.
- 74 tests pin the substrate behaviour (5 to 21 per file, plus 2 integration tests).

The smallest hypothetical "frame-graph follow-up" candidates I considered and rejected:

| Candidate | Rejected because |
|---|---|
| Add MSAA / multi-pass primitives to the substrate | No consumer pressure; would be speculative; ADR-118 didn't commit to it |
| Profile `compile.rs` for hotspots | No perf-regression signal; the umbrella smoke ran clean |
| Add a `FrameRecorder`-consuming smoke test | `editor-shell::render_path::render_frame` already exercises the substrate end-to-end in production — the missing piece is a non-winit perf harness, which is editor-shell scope, not frame-graph scope |
| Refactor `compile.rs` (29 KB; largest file) for legibility | Cosmetic; no functional benefit; would expand surface area without clear pressure |

### Deliverable 3: Job 5 recommendation

**SKIP Job 5** as currently scoped. Rationale:

1. Job 5 is `MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP` — a conditional-implementation job that fires only if Job 4 recommends frame-graph work.
2. Job 4's audit finds **no frame-graph follow-up is warranted**. The chapter is closed; the substrate is production-consumed; zero TODOs; 74 tests.
3. The remaining open gap from the original umbrella close-out (runtime-perf re-validation) has been substantively addressed by `03d3f05` at the gfx level. The editor-shell `render_frame` end-to-end version is genuinely separate scope and belongs in a future editor-shell mock-event-loop dispatch.
4. Forcing Job 5 to implement something speculative against a closed chapter would violate the empirical posture established by the existing IMPLEMENTATION.md:473 close-out.

**Alternative if Planner disagrees**: rather than skipping Job 5, the Planner could redirect it via a CORRECTION packet to target the **editor-shell mock-event-loop perf harness** — that's the genuine outstanding deferral. But that scope is substantially larger than "frame-graph follow-up" implies and would be a fresh substrate dispatch, not a conditional implementation of frame-graph work.

### Deliverable 4: Relevant tests and docs

**Tests** (74 total):
- Integration: `crates/gfx/tests/frame_graph_smoke.rs`, `crates/gfx/tests/frame_graph_umbrella_smoke.rs`
- Unit (inline `#[cfg(test)] mod tests`): 5 + 21 + 15 + 11 + 5 + 5 + 5 + 5 = 72 across the 8 source files
- Plus indirect coverage via `editor-shell::render_path` exercising `build_resource_map(...)` in production

**Docs** (already recording the frame-graph state):
- `plans/IMPLEMENTATION.md:473` — chapter umbrella close-out + intentional-future-work note + Gate A re-validation deferral
- `plans/BASELINE.md:248` — post-depth Gate A close-out, includes the "editor-shell `render_frame` end-to-end" deferral with explicit blocker (`EditorShell::render_frame` accepting a mock event loop)
- `ADR-118` (referenced in IMPLEMENTATION.md:473 + umbrella smoke docstring) — frame-graph policy doctrine

## Deviations from Task Packet

None. Execution stayed strictly within the TASK scope:
- Exactly one new file produced (this EXEC packet).
- Zero tracked-file edits.
- Zero edits to Status.md / HANDOFF.md / change.md / source / test / Cargo / ADR / lint / protocol-doc / template (TASK MUST NOT envelope honored).
- No expensive test run (the umbrella smoke + smoke + the 72 inline unit tests were NOT re-run by this audit; the audit relies on docs-recorded prior gate-passing state).
- No commit, no push.

## Open Questions for Reviewer / Planner

- **Job 5 SKIP vs Job 5 CORRECT-then-redirect**: this EXEC recommends SKIP. Alternative is redirecting Job 5 to target the editor-shell mock-event-loop perf harness, which is genuinely deferred but is editor-shell scope, not frame-graph scope. Planner discretion.
- **Frame-graph compile.rs size**: 29 KB / ~700 LoC is the largest file in the substrate. The architecture-lints split-exemption rule has a 1000-line cap (from earlier in this session's CAD-core work); compile.rs is under that cap. No action needed; just observation.
- **Inline-vs-integration test split**: 72 inline + 2 integration is the current shape. The umbrella smoke is the load-bearing cross-frame composition test. The dual `frame_graph_smoke.rs` + `frame_graph_umbrella_smoke.rs` may be partially redundant; reviewing them is out of scope for this audit but might be a candidate for future cleanup if Codex sees value.

## Job 5 Proceed Recommendation

**Recommended: SKIP** (per Deliverable 3 above).

If Planner accepts SKIP, the serial controller can release **Job 6 (`MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT`)** next, in keeping with the serial-rule's "later queued jobs must be reconsidered before the next release" requirement.

Executor will not start Job 5 from inside this Job 4 dispatch regardless of recommendation. Per the queue's serial rule from `MAIN-ORDERED-SERIAL_STATE_2026-05-14_14-46-30+0300.md`:

> Stop Condition: stop after EXEC and route to `NEXT_ROLE: REVIEWER_AI`. Do not start Job 5.

## Worktree State

- Tracked files: clean (verified by empty `git status --short --untracked-files=no` pre-EXEC; this EXEC packet is the only filesystem change and it lands as untracked).
- New untracked items from this turn: 1 (this EXEC packet).
- Branch: `main`.
- HEAD: `bcfe902 docs(status): MAIN-ORDERED-JOB-003 - queue status reconciliation`.
- Local main is **4 commits ahead** of `origin/main` (`03d3f05` + `2b64241` + `d017a35` + `bcfe902`); none pushed. Unchanged by this Job 4 read-only audit.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
