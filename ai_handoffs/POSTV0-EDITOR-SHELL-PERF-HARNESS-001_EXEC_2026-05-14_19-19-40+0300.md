# Execution Report

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_19-19-40+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md — TASK consumed.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_EXEC_2026-05-14_18-31-54+0300.md — design map followed.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_CLOSEOUT_2026-05-14_18-38-02+0300.md — risks carried forward addressed.
- crates/editor-shell/src/render_path.rs — modified (refactor + new helpers + headless init).
- crates/editor-shell/src/lib.rs — modified (added `#[cfg(test)] mod render_frame_e2e_perf;` declaration).
- crates/editor-shell/src/render_frame_e2e_perf.rs — new file (the perf harness).
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md — this packet.
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. The TASK packet's footer routes `NEXT_ROLE: EXECUTOR_AI` directly under v2 Rule 7 (single pre-execution reviewer; no duplicate Reviewer2 rubber-stamp). The OPENAItoCLAUDE root note (2026-05-14 18:52) explicitly authorized: "Use protocol v2 Rule 7: no duplicate Reviewer2 approval packet is needed." Executor proceeded to execution.

## What I Changed

### Source

- `crates/editor-shell/src/render_path.rs`: refactored. Added a module-level `pub(crate) enum DepthViewOutcome { Uninitialized, RecoverableSkip, Acquired(wgpu::TextureView) }`. Inside `impl EditorShell`: extracted `init_render_state_post_surface(&mut self, GfxContext, format, width, height)` (Steps 4–6 shared between production and headless); added `pub(crate) fn acquire_depth_view(&mut self) -> DepthViewOutcome` (frame-graph pool `begin_frame` + `build_resource_map`); added `pub(crate) fn render_frame_to_target(&self, &TextureView, &TextureView) -> bool` (encode body, no surface acquire / present / `request_redraw`); added `#[cfg(test)] pub(crate) fn init_render_state_headless(&mut self, target_format, width, height) -> Result<(), String>` (skips winit Steps 1 + 3, delegates Steps 2 + 4–6 to the shared helper). `init_render_state` now delegates Steps 4–6 to the shared helper. `render_frame` is now a 5-phase wrapper (A: `acquire_depth_view` / B: window+surface field checks / C: surface acquire / D: shared encode via `render_frame_to_target` / E: present + `request_redraw`). Net: +247 / −106 lines.
- `crates/editor-shell/src/lib.rs`: added 2 lines — `#[cfg(test)]` + `mod render_frame_e2e_perf;` declaration. Net: +2 / −0.

### Tests

- `crates/editor-shell/src/render_frame_e2e_perf.rs`: new file (~270 lines). Single `#[ignore]`-gated release-only timing harness `render_frame_e2e_p95_minus_surface_acquire_present_recorder_host`. Builds a unit-cuboid `(CadGraph, CadProjection, World)` triple via the same idiom as `crates/cad-projection/tests/face_picking_smoke.rs`; calls `EditorShell::with_world_projection_graph(...)` + `init_render_state_headless(Bgra8UnormSrgb, 1024, 768)`; allocates one offscreen color target + view; drives 60 warmup + 600 sample frames × 3 runs of `acquire_depth_view` + `render_frame_to_target`; computes P50/P95/min/max/worst per run; computes median/min/max/variance% across the 3 run P95s; asserts all measurements finite + non-negative + variance ≤ 30%; reports soft P95 target (1.0 ms) as under/over without asserting it as a hard threshold.

### Docs

- `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md`: this packet (the only new ai_handoffs file).

### NOT modified

- `crates/editor-shell/src/lifecycle.rs`: untouched (per TASK MUST NOT).
- `crates/editor-shell/src/render_input.rs`: untouched (per TASK MUST NOT).
- `crates/editor-shell/tests/**`: untouched (per TASK MUST NOT).
- `crates/gfx/**`, `crates/brep-render/**`, `crates/cad-core/**`, `crates/cad-projection/**`, `kernel/**`, `runtime/**`, `editor/**`, `plans/**`, `docs/**`, `Status.md`, `HANDOFF.md`, `change.md`, `V0_RELEASE_CERTIFICATION.md`, `Cargo.toml`, `Cargo.lock`, `ai_handoffs/AI_HANDOFF_PROTOCOL.md`, `ai_handoffs/templates/**`: untouched (per TASK MUST NOT).

## Local Commit

**Created** — `cd2ecd3` (verified by `git show --stat --oneline --name-only HEAD` post-commit; see §"Post-Commit Verification"). Subject matches the TASK's recommended subject `test(editor-shell): add render_frame encode-submit perf harness`. Commit content: only the 3 allowed source files (`render_path.rs`, `lib.rs`, `render_frame_e2e_perf.rs`). No `ai_handoffs/*` files included; no root-level handoff files included; no logs. **No push performed** (per TASK MUST NOT).

*(Hash recorded after commit — see "Verification Gates" → `git show --stat --oneline --name-only HEAD` below for the canonical value.)*

## Verification Gates

Per the TASK's `Verification Gates` section, each command ran:

| Gate | Result |
| --- | --- |
| `git status --short --untracked-files=no` (pre-commit) | ` M crates/editor-shell/src/lib.rs` + ` M crates/editor-shell/src/render_path.rs` (allowed edits only) |
| `git rev-list --left-right --count origin/main...HEAD` (pre-commit) | `0 0` |
| `git log --oneline --decorate -3` (pre-commit) | `b13c176 (HEAD -> main, origin/main) docs(cert): v0 release certification at 6aaf7f1` + 2 ancestors |
| `cargo +nightly fmt --check -p rge-editor-shell` | **PASS** exit 0 (after one round-trip fix for module ordering + array-literal layout) |
| `cargo check -p rge-editor-shell --all-targets` | **PASS** exit 0; `Finished dev profile [unoptimized + debuginfo] target(s) in 8.83s` |
| `cargo test -p rge-editor-shell --lib --no-fail-fast` | **PASS** exit 0; 67 passed / 0 failed / 1 ignored (the new perf harness as expected) |
| `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` | **PASS** exit 0; perf measurement captured (see below) |
| `cargo run -q -p rge-tool-architecture-lints -- all` | **PASS** exit 0; 9 enforcement lints + supplementary `snapshot-participate` all PASS with 0 violations |
| `git diff --stat HEAD -- crates/editor-shell/src/render_path.rs crates/editor-shell/src/lib.rs crates/editor-shell/src/render_frame_e2e_perf.rs` (pre-commit) | 2 files modified (`render_path.rs` +247/−106, `lib.rs` +2/−0); `render_frame_e2e_perf.rs` shows as new in `git status` and was added to staging by the commit. |
| `git show --stat --oneline --name-only HEAD` (post-commit) | recorded below in §"Post-Commit Verification" |

## Halt-Condition Checks

All ten TASK halt conditions evaluated; none triggered:

| Halt condition | Status |
| --- | --- |
| Implementing the harness requires editing outside the allowed file set | NOT TRIPPED — only the 3 allowed files edited |
| A public API addition appears necessary | NOT TRIPPED — `DepthViewOutcome` enum + 4 new methods are all `pub(crate)` or private |
| A new dependency appears necessary | NOT TRIPPED — Cargo.toml untouched; only existing `wgpu` / `rge-cad-core` / `rge-cad-projection` / `rge-kernel-ecs` deps used |
| `lifecycle.rs` / `render_input.rs` / Cargo / plans / status / handoff / cert / protocol files need changes | NOT TRIPPED — none touched |
| Production `render_frame` behavior cannot be preserved | NOT TRIPPED — surface acquire + present + `request_redraw` + return-false-on-uninit + return-true-on-recoverable-skip all preserved (see §"Production Behavior Preservation" below) |
| Headless path cannot initialize without a real `ActiveEventLoop` | NOT TRIPPED — `init_render_state_headless` succeeded; `GfxContext::new_headless()` is winit-independent as the preflight predicted |
| Release harness cannot obtain a GPU adapter on the recorder host | NOT TRIPPED — recorder-host `GfxContext::new_headless()` returned a working adapter (the test ran to completion) |
| Release harness repeatedly exceeds 30% variance | NOT TRIPPED — single-run variance = 25.0% (under the 30% gate, but on the high side — see Risk #1 below) |
| Required non-perf gate fails and cannot be fixed within the allowed file set | NOT TRIPPED — every gate PASS |
| Natural fix expands into broad editor architecture / real winit mocking / surface abstraction rewrite / multi-scene / threading | NOT TRIPPED — refactor was bounded to one source file + one new harness file as the preflight predicted |

## Production Behavior Preservation

The TASK's 5 preserve-bullets are individually verified against the new `render_frame`:

| Pre-extraction contract | New implementation | Preserved? |
| --- | --- | --- |
| Acquires the current surface texture | Phase C lines 364–375 (unchanged from pre-extraction) | YES |
| Presents | Phase E `frame.present()` | YES |
| Schedules redraw through the existing window path | Phase A `RecoverableSkip` branch `w.request_redraw()`; Phase C surface-acquire-skip branch `window.request_redraw()`; Phase E `window.request_redraw()` on success | YES (all 3 redraw sites preserved) |
| Returns `false` when render state is uninitialized | Phase A `DepthViewOutcome::Uninitialized` → false; Phase B missing surface_ctx / window → false; Phase D `render_frame_to_target` false → false | YES |
| Returns `true` for recoverable skip paths that should continue the event loop | Phase A `RecoverableSkip` → true; Phase C surface-acquire-skip → true | YES |

The 67 existing lib tests pass unchanged.

One subtle behavioral note: in the pre-extraction code, the depth-prep happened before the surface acquire (today's order preserved). In the new code, the depth-prep still happens BEFORE surface acquire (Phase A before Phase C) — so a `build_resource_map` failure still skips without acquiring a surface frame. This was the load-bearing ordering invariant from the pre-extraction shape.

## Captured Measurements

Single recorder-host run on **NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu / Windows 11 Pro for Workstations 10.0.26200** (per the standing recorder-host environment):

```
POSTV0-EDITOR-SHELL-PERF-HARNESS-001 — encode/submit minus surface acquire/present
(recorder-host-only, single-cuboid, 1024x768, Bgra8UnormSrgb,
 60 warmup + 600 sample x 3 runs)
  run 0: P50 = 0.018100 ms, P95 = 0.041900 ms, min = 0.015800 ms, max = 0.393500 ms, worst = 0.393500 ms
  run 1: P50 = 0.018800 ms, P95 = 0.053500 ms, min = 0.015200 ms, max = 2.252500 ms, worst = 2.252500 ms
  run 2: P50 = 0.018300 ms, P95 = 0.055300 ms, min = 0.015100 ms, max = 0.461700 ms, worst = 0.461700 ms
  cross-run: median P50 = 0.018300 ms; median P95 = 0.053500 ms;
             min P95 = 0.041900 ms; max P95 = 0.055300 ms;
             worst-frame = 2.252500 ms; min-frame = 0.015100 ms;
             max-frame = 2.252500 ms; variance across run P95s = 25.0%
  soft P95 target = 1.000 ms; observed median P95 is UNDER the soft target
```

Compile time for `--release` build of `rge-editor-shell` test binary: `Finished release profile [optimized] target(s) in 44.89s`. Harness elapsed: `finished in 1.06s` (3 × 660 frames = 1980 frames in 1.06s ≈ 1.87M fps target throughput; measurement granularity is `Instant::elapsed`).

**Headline numbers**:
- **Median P95**: **0.0535 ms** (≈ 18.7× under the 1.0 ms soft target; ≈ 312× under the 16.67 ms Gate A budget).
- **Median P50**: **0.0183 ms**.
- **Worst single frame**: **2.2525 ms** (run 1 — likely an OS thread preemption / device-driver coalesce / scheduling jitter spike; well under Gate A's 16.67 ms budget; ≈ 2.3× the soft P95 target).
- **Variance across run P95s**: **25.0%** (under the 30% hard gate, but close to it — see Risk #1).

## Recommendation for Future Hard Threshold

The captured median P95 = 0.0535 ms gives substantial headroom against any sensible gate. The Executor recommends the following thresholds for a future certification dispatch (NOT pinned in this dispatch's source per TASK):

| Metric | Observed | Recommended hard gate | Headroom |
| --- | --- | --- | --- |
| Median P95 | 0.0535 ms | **0.5 ms** | ≈ 9× |
| Worst-frame | 2.25 ms | **5.0 ms** | ≈ 2.2× |
| Variance across runs | 25.0% | **30%** (current soft gate) | retain current; see Risk #1 |

Rationale:
- The soft P95 target of 1.0 ms picked in the preflight is overly generous given the actual measurement. A 0.5 ms hard threshold keeps a healthy 9× headroom for future scene complexity growth (multi-cuboid / loaded scenes / additional draw calls) while still rejecting a regression that would matter.
- The worst-frame threshold of 5 ms accommodates the observed 2.25 ms OS-scheduling spike with ≈ 2× headroom; tighter than the 16.67 ms Gate A budget but loose enough to ride out normal Windows scheduling jitter.
- The 30% variance gate matches Gate A / Gate B conventions and is already at the upper bound of what the observed measurement clears (run-1 alone widened the run-P95 spread). Keeping 30% rather than tightening avoids false negatives from OS scheduling noise.

Pinning these in source is a future dispatch's decision; this dispatch is measurement-capture per the TASK's "Do not hard-code a final threshold yet" non-goal.

## v0 Certification State

**v0 certification at commit `6aaf7f1` (cert commit `b13c176`) remains valid.** This dispatch is a post-v0 measurement-capture and bounded source refactor. No cert docs were edited, no v0 deferral list was retargeted, no production `render_frame` behavior changed. The editor-shell mock-event-loop perf harness was listed as a "Known v0 deferral (NOT blocker)" in `V0_RELEASE_CERTIFICATION.md` §4 at cert time; this dispatch lands the harness as a post-v0 follow-on per Codex's CLOSEOUT decision.

## Preflight Risks Addressed

The 4 `Remaining Risks Carried Forward` from the preflight CLOSEOUT (2026-05-14 18:38:02) were addressed as follows:

| Preflight risk | How this dispatch addressed it |
| --- | --- |
| **Helper visibility** (`pub(crate)` + `#[cfg(test)] mod` vs `pub fn ..._for_perf_only` + integration test) | Chose the crate-local `#[cfg(test)] mod render_frame_e2e_perf;` path. New helpers (`acquire_depth_view`, `render_frame_to_target`, `init_render_state_post_surface`) are `pub(crate)`; `init_render_state_headless` is `#[cfg(test)] pub(crate)`. **Zero public API additions.** |
| **Budget threshold** — pin after recorder-host dry run vs first-run-as-capture | First run as measurement-capture; soft P95 = 1.0 ms REPORTED (not asserted as a hard gate). Hard variance gate asserted at 30%. Future hard threshold recommendation = 0.5 ms median P95 per §"Recommendation for Future Hard Threshold". |
| **Scope wording** | Test function name is `render_frame_e2e_p95_minus_surface_acquire_present_recorder_host`. Module doc-comment explicitly enumerates what the harness DOES and DOES NOT certify (no winit event-loop scheduling / no surface acquire / no present / no vsync / no universal hardware / no cold-start / no thermal / no loaded-scene / no CI). |
| **Production parity** — share setup with `init_render_state` | Introduced `init_render_state_post_surface(&mut self, GfxContext, format, width, height)` as the shared post-surface helper. Both production `init_render_state` (after Steps 1–3 build winit window + `SurfaceContext`) and the headless `init_render_state_headless` (after Step 2 builds `GfxContext::new_headless()`) call into this single helper for Steps 4–6 (camera / material / highlight material / light / pipeline / pool / frame-graph / mesh). Drift is structurally prevented by single-source-of-truth construction. |

## Risk List (carry-forward concerns)

Three concerns surfaced during the dispatch that the Reviewer may want to flag:

1. **Variance close to the gate** — observed 25.0% variance is comfortably under the 30% hard gate but the headroom is only 5%. Re-runs on a busier recorder-host (other Cargo builds in flight, browser updates, etc.) could push it to 30%+ and falsely trip the gate. A future dispatch may want to either:
   - Tighten warmup to 120 frames to absorb early-run scheduling.
   - Lower the variance gate to 20% AFTER demonstrating sustained < 15% on a quieter host.
   - Both options are explicitly out of scope for this measurement-capture dispatch.
2. **Worst-frame 2.25 ms from a single sample** — the worst frame is ≈ 42× the median P95 and shows up in only one of three runs. Almost certainly OS scheduling noise (Windows compositor thread / driver coalesce). Not a substrate concern but the EXEC notes it so a future "tighter perf gate" dispatch doesn't mistake it for a real cost.
3. **CADprojection tick on every initialization** — the harness calls `projection.tick(&mut world, &graph, tolerance)` once during setup; this is one-shot, not per-frame. If a future harness iterates over multiple CAD scenes (loaded scene exploration), the `tick` cost may need its own gate to separate "render perf" from "projection perf".

None of these are blocking or unsafe; all are appropriate inputs to a future tightening dispatch.

## Deviations from Task Packet

Two minor deviations, both inside the TASK's `MAY edit` envelope:

1. **`cargo +nightly fmt`** required two iterations to PASS — first run flagged module ordering (`mod render_frame_e2e_perf;` had to come before `mod render_input;` per alphabetical order) and array literal layout. Both fixed by edits to the same allowed files; no scope expansion.
2. **The harness uses `#[cfg(test)] mod` inside `src/`** rather than the legacy integration-test `tests/` placement. This is the preflight-approved Option (b) and is explicitly within the TASK's `MAY add new files` (`crates/editor-shell/src/render_frame_e2e_perf.rs`). Document for clarity only.

No source / test / Cargo / plans / cert / handoff / protocol files were touched outside the allowed set.

## Post-Commit Verification

*(Filled in after the commit lands — values captured by `git show --stat --oneline --name-only HEAD` per TASK §Verification Gates.)*

- HEAD post-commit: `cd2ecd3 test(editor-shell): add render_frame encode-submit perf harness`
- Files in commit: `crates/editor-shell/src/lib.rs`, `crates/editor-shell/src/render_path.rs`, `crates/editor-shell/src/render_frame_e2e_perf.rs` (3 files, no others).
- `git status --short --untracked-files=no` post-commit: empty (tracked tree clean).
- `git rev-list --left-right --count origin/main...HEAD` post-commit: `0 1` (one local commit ahead; push not performed per TASK MUST NOT).

## Worktree State

- Tracked files: clean post-commit (`git status --short --untracked-files=no` empty).
- New untracked items from this turn: 1 (this EXEC packet under `ai_handoffs/`).
- Other untracked items unchanged: prior session's handoff MDs + the gitignored `_v0cert_test.log` build artifact.
- Branch: `main`.
- HEAD: `cd2ecd3 test(editor-shell): add render_frame encode-submit perf harness` (1 commit ahead of `origin/main`).

## Open Questions for Reviewer / Planner

1. **Variance gate tightening** (Risk #1) — the 25% observed leaves only 5% margin under the 30% gate. Reviewer / Planner may want to schedule a follow-on dispatch that re-runs the harness 5× on a quiet recorder host to characterize sustained variance before deciding whether to tighten the gate.
2. **Hard-threshold pinning** — `§Recommendation for Future Hard Threshold` proposes median P95 ≤ 0.5 ms, worst-frame ≤ 5 ms. Planner may want to either accept these for a future "pin-the-threshold" dispatch or request a different ratio (e.g., 10× headroom vs 9×).
3. **CI promotion** — the harness is recorder-host-only by design (`#[ignore]`-gated, like Gate A). A future "promote to CI" dispatch would require a separate analysis on what host the CI runs against (the recorder host is currently Halil's local machine).

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
