# Execution Report (Correction Round 2)

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_21-51-40+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md — original TASK.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md — superseded EXEC (initial implementation).
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md — Reviewer round-1.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_19-33-13+0300.md — Planner correction round-1.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_20-28-09+0300.md — superseded EXEC (correction round-1 / `(60, 10)`).
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_20-58-29+0300.md — Reviewer round-2 (this round's input).
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_20-58-30+0300.md — Planner correction round-2 (this round's input).
- crates/editor-shell/src/render_frame_e2e_perf.rs — modified (constants bumped to `(240, 50)`).
- crates/editor-shell/src/render_path.rs — unchanged in this correction (carried in amended commit from initial implementation).
- crates/editor-shell/src/lib.rs — unchanged in this correction (carried in amended commit from initial implementation).
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md — this packet (the only new ai_handoffs file).
STATUS: AWAITING_REVIEW

## Task Packet Reference

Original TASK: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`
Correction packet (round-2; consumed this round): `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_20-58-30+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this correction round. The CORRECT packet routed `NEXT_ROLE: EXECUTOR_AI` directly under v2 Rule 7 (single pre-execution reviewer; no duplicate Reviewer2 rubber-stamp for correction rounds). The OPENAItoCLAUDE root note (2026-05-14 20:58:31) confirmed the routing and the bounded scope. Executor proceeded to execution.

## What I Changed in This Correction Round

### Source

- `crates/editor-shell/src/render_frame_e2e_perf.rs`:
  - **`WARMUP_FRAMES: usize = 60` → `WARMUP_FRAMES: usize = 240`** (4× warmup) with an expanded docstring explaining the cold-binary first-run tail rationale.
  - **`FRAMES_PER_SAMPLE: usize = 10` → `FRAMES_PER_SAMPLE: usize = 50`** (5× per-batch unit) with an expanded docstring explaining the Windows scheduler / `Instant` resolution noise floor that the prior `(60, 10)` shape clipped on cold runs.
  - `SAMPLE_BATCHES: usize = 600` retained.
  - Module-level `# Measurement contract` doc-comment updated to describe the `(240, 50)` shape + the `(60, 10) → (240, 50)` rationale (Reviewer round-2 observed 54.6 % variance on a cold-binary invocation).
  - Printed run header auto-updates via const interpolation: now reads `"240 warmup + 600 sample batches x 50 frames x 3 runs"` (CORRECT packet acceptance: "the harness output names the new batch shape").
  - No other functional change.

- `crates/editor-shell/src/render_path.rs`: **unchanged in this round** (carried forward in the amended commit from the initial implementation + the round-1 correction-amend).
- `crates/editor-shell/src/lib.rs`: **unchanged in this round** (carried forward in the amended commit from the initial implementation).

### Tests

- No new tests. The single ignored release-only test
  `render_frame_e2e_p95_minus_surface_acquire_present_recorder_host`
  is the only change vehicle and its scope is unchanged (encode/submit
  minus surface acquire/present; recorder-host-only; single-cuboid;
  1024×768 Bgra8UnormSrgb).

### Docs

- `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md`: this packet (the only new ai_handoffs file in this round).

### NOT modified in this round (per CORRECT-round-2 MUST NOT)

- `crates/editor-shell/src/render_path.rs`: unchanged this round.
- `crates/editor-shell/src/lib.rs`: unchanged this round.
- `crates/editor-shell/src/lifecycle.rs`: untouched.
- `crates/editor-shell/src/render_input.rs`: untouched.
- `crates/editor-shell/tests/**`: untouched.
- All other paths excluded by the original TASK MUST NOT envelope: untouched.

### CORRECT-round-2 scope discipline

The CORRECT-round-2 packet's `## Approved Corrections` requires editing only `crates/editor-shell/src/render_frame_e2e_perf.rs`. Confirmed: `git diff HEAD~1 HEAD` against `origin/main` shows the only files in the amended commit are the original allowed three, and the round-2 correction-diff (this amend vs the prior `1f4876c` amend) is confined to `crates/editor-shell/src/render_frame_e2e_perf.rs`.

## Local Commit (Amended Again)

Per the CORRECT-round-2 packet ("amend the existing local commit again"), the prior amended commit `1f4876c` was amended again to **`f8b8ed4`** (new hash because the amended tree differs from `1f4876c`). Branch state: 1 commit ahead of `origin/main`. **No push performed**.

`git show --stat --oneline --name-only HEAD`:
```
f8b8ed4 test(editor-shell): add render_frame encode-submit perf harness
crates/editor-shell/src/lib.rs
crates/editor-shell/src/render_frame_e2e_perf.rs
crates/editor-shell/src/render_path.rs
```

3 files in the amended commit, matching the original allowed-set; no `ai_handoffs/*` content; no root-level handoff files; no logs.

Commit hash history (all local-only):
- Round-0 (initial): `cd2ecd3`
- Round-1 amend: `1f4876c`
- Round-2 amend (this round): **`f8b8ed4`**

The amended commit message preserves the original structure and dispatch ID, updates the `Harness shape` section to describe the `(240, 50)` shape, and records both consecutive run outputs in the captured-measurements section.

## Re-Verification Gates (from CORRECT-round-2)

| Gate | Result |
| --- | --- |
| `git status --short --untracked-files=no` (post-amend) | empty (tracked tree clean) |
| `git rev-list --left-right --count origin/main...HEAD` (post-amend) | `0 1` (one local commit ahead; no push) |
| `git show --stat --oneline --name-only HEAD` (post-amend) | 3 allowed files only |
| `cargo +nightly fmt --check -p rge-editor-shell` | **PASS** exit 0 |
| `cargo check -p rge-editor-shell` | **PASS** exit 0 (`Finished dev profile target(s) in 0.40s`) |
| `cargo test -p rge-editor-shell --lib --no-fail-fast` | **PASS** exit 0; 67 passed / 0 failed / 1 ignored |
| `cargo run -q -p rge-tool-architecture-lints -- all` | **PASS** exit 0; 9 enforcement + 1 supplementary, 0 violations |
| `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` (1st consecutive) | **PASS** exit 0; variance **1.7 %** |
| `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` (2nd consecutive) | **PASS** exit 0; variance **3.0 %** |

All re-verification gates PASS. Both variance numbers are an order of magnitude under the 30 % hard gate.

## Halt-Condition Checks (CORRECT-round-2)

| Halt condition | Status |
| --- | --- |
| Two consecutive release-harness passes not achievable | NOT TRIPPED — both consecutive runs PASS (1.7 % and 3.0 %); both well under the 30 % gate |
| Correction requires editing any file other than `crates/editor-shell/src/render_frame_e2e_perf.rs` | NOT TRIPPED — only that one file edited in this round |
| Branch ceases to be exactly one local commit ahead of `origin/main` | NOT TRIPPED — `0 1` preserved across the amend |
| Push attempted | NOT TRIPPED — no push performed |
| Hard 30 % variance gate weakened or deleted | NOT TRIPPED — gate is unchanged (still `<= 30 %`, still asserted in source) |

## Captured Measurements

Recorder host: **NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu** (unchanged from prior rounds). Two consecutive release-harness invocations after the corrected harness compiled. The first invocation was the cold-binary case (`Finished release profile [optimized] target(s) in 19.71s`); the second reused the cached binary (`Finished release profile target(s) in 0.41s`).

### Consecutive run A (cold binary, just-compiled)

```
POSTV0-EDITOR-SHELL-PERF-HARNESS-001 — encode/submit minus surface acquire/present
(recorder-host-only, single-cuboid, 1024x768, Bgra8UnormSrgb,
 240 warmup + 600 sample batches x 50 frames x 3 runs; each sample = per-frame batch mean)
  run 0: P50 = 0.022044 ms, P95 = 0.052750 ms, min = 0.015818 ms, max = 0.228798 ms, worst-sample = 0.228798 ms
  run 1: P50 = 0.019764 ms, P95 = 0.053632 ms, min = 0.016080 ms, max = 0.079064 ms, worst-sample = 0.079064 ms
  run 2: P50 = 0.019766 ms, P95 = 0.052922 ms, min = 0.016936 ms, max = 0.074424 ms, worst-sample = 0.074424 ms
  cross-run: median P50 = 0.019766 ms; median P95 = 0.052922 ms;
             min P95 = 0.052750 ms; max P95 = 0.053632 ms;
             worst-sample = 0.228798 ms; min-sample = 0.015818 ms; max-sample = 0.228798 ms;
             variance across run P95s = 1.7%
  soft P95 target = 1.000 ms; observed median P95 is UNDER the soft target
test render_frame_e2e_perf::render_frame_e2e_p95_minus_surface_acquire_present_recorder_host ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 67 filtered out; finished in 2.83s
```

### Consecutive run B (hot binary, immediately after run A)

```
POSTV0-EDITOR-SHELL-PERF-HARNESS-001 — encode/submit minus surface acquire/present
(recorder-host-only, single-cuboid, 1024x768, Bgra8UnormSrgb,
 240 warmup + 600 sample batches x 50 frames x 3 runs; each sample = per-frame batch mean)
  run 0: P50 = 0.019498 ms, P95 = 0.054016 ms, min = 0.015692 ms, max = 0.068242 ms, worst-sample = 0.068242 ms
  run 1: P50 = 0.020360 ms, P95 = 0.055566 ms, min = 0.016584 ms, max = 0.313938 ms, worst-sample = 0.313938 ms
  run 2: P50 = 0.019810 ms, P95 = 0.053972 ms, min = 0.016030 ms, max = 0.092044 ms, worst-sample = 0.092044 ms
  cross-run: median P50 = 0.019810 ms; median P95 = 0.054016 ms;
             min P95 = 0.053972 ms; max P95 = 0.055566 ms;
             worst-sample = 0.313938 ms; min-sample = 0.015692 ms; max-sample = 0.313938 ms;
             variance across run P95s = 3.0%
  soft P95 target = 1.000 ms; observed median P95 is UNDER the soft target
test render_frame_e2e_perf::render_frame_e2e_p95_minus_surface_acquire_present_recorder_host ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 67 filtered out; finished in 2.86s
```

### Headline numbers (consolidated)

| Metric | Run A (cold) | Run B (hot) | Notes |
| --- | --- | --- | --- |
| Median P50 (ms/frame) | 0.01977 | 0.01981 | Identical to 4 sig figs |
| **Median P95 (ms/frame)** | **0.05292** | **0.05402** | **Stable to ≈ 2 % across consecutive runs** |
| Worst-sample (ms/frame) | 0.229 | 0.314 | Worst batch mean (50-frame average); OS scheduler tail |
| **Variance across run P95s** | **1.7 %** | **3.0 %** | **Both ~10× under the 30 % gate** |
| Soft target (1.0 ms P95) | UNDER (~19×) | UNDER (~19×) | Reported only — not asserted |
| Test wall-clock | 2.83 s | 2.86 s | 240 + 600×50 = 30 240 frames × 3 runs ≈ 90 720 frames / 2.83 s ≈ 32 M frames/sec wall-clock (single-cuboid; encode-only) |

### Cross-round comparison

| Shape | Round | Run A var | Run B var | Comment |
| --- | --- | --- | --- | --- |
| `(60, 1)` | Round-0 initial | 25.0 % | — (Reviewer cold rerun later: 59.4 %) | Single-frame timing; per-frame unit clipped Windows noise floor |
| `(60, 10)` | Round-1 amend | 29.4 % | 4.3 % | Batched; cold-binary first run still tight against gate; Reviewer cold rerun later: 54.6 % |
| **`(240, 50)`** | **Round-2 amend (this round)** | **1.7 %** | **3.0 %** | **Both well under gate; cold-binary tail absorbed by the 240-warmup; per-batch unit (~1.0 ms) well above the noise floor** |

The `(240, 50)` shape closes the loop the Reviewer's round-2 critique opened: the gate now measures the render path, not scheduler jitter.

### Production behaviour invariance (cross-check)

The median P95 of ≈ 0.053 ms (this round) vs ≈ 0.054 ms (round-1) vs ≈ 0.054 ms (round-0 single-frame timing, less the per-frame noise that biased the original 0.054 ms P95 upward toward 0.10 ms range). All three rounds measure the same render path. The encode/submit cost is invariant — only the measurement aggregation changed. This is a strong signal that no production regression occurred during the refactor.

## Why `(240, 50)` Works

The Reviewer's round-2 critique pinpointed the root cause: the prior `(60, 10)` shape's per-batch timing unit (~0.2 ms) was close enough to the Windows scheduler / `Instant` resolution noise floor (~0.1 ms) that a single OS preemption inside one batch could swing that batch's measured time by 50 %+. On a cold-binary first run, the warmup window (60 frames ≈ 0.06 s) wasn't long enough to absorb page-cache / code-TLB / branch-predictor / GPU command pool cold tails, so the first run's P95 was systematically higher than runs 1+2 — the prior shape's 29.4 % variance in round-1 ran A came from exactly this asymmetry.

The `(240, 50)` shape lifts both axes off the floor:
- **240 warmup frames ≈ 0.24 s of execution** — enough for the cold-binary tails to dissipate before sampling begins.
- **50 frames per sample × ~0.02 ms/frame = ~1.0 ms per batch** — well above the ~0.1 ms Windows timer noise floor. One OS preemption inside a 1.0 ms batch perturbs the per-frame batch mean by ≤ 5 %, vs ≤ 50 % under the prior 0.2 ms batch.

This is the canonical "raise the timing window above the noise floor" stabilisation pattern; same shape as Gate A's recorder-host harness (`plans/BASELINE.md:240` — 60 warmup + 600 sample, full-frame timing because Gate A measures 1 k cubes at ~0.122 ms/frame which is already above the noise floor).

## Recommendation for Future Hard Threshold (carried forward, refined)

The captured median P95 = ~0.0535 ms is stable across two consecutive cold + hot recorder-host invocations and ≈ 19× under the 1.0 ms soft target. Refined thresholds:

| Metric | Observed | Recommended hard gate | Headroom |
| --- | --- | --- | --- |
| Median P95 | 0.0535 ms | **0.5 ms** | ≈ 9× |
| Worst-sample (batch mean) | 0.314 ms | **5.0 ms** | ≈ 16× |
| Variance across runs | 1.7–3.0 % | **30 %** (current; consider tightening to 15 %) | the observed 3.0 % suggests a 15 % gate would still pass; tightening is a separate dispatch |

Pinning thresholds in source remains deferred per the CORRECT-round-2 `Deferred Findings` item #3.

## v0 Certification State

**v0 certification at commit `6aaf7f1` (cert commit `b13c176`) remains valid.** This correction round changed only the sampling constants inside the post-v0 perf harness. No cert docs were edited; no v0 deferral list was retargeted; no production `render_frame` behavior changed. The amended commit's diff against `origin/main` is structurally identical to the prior amended commit's diff modulo the `(60, 10) → (240, 50)` constant bumps inside the new harness file.

## Deferred Findings Acknowledged

The CORRECT-round-2 packet's `## Deferred Findings (NOT Approved for This Round)` carries three items unchanged from round-1; all are acknowledged and NOT touched in this round:

1. **GPU completion wait semantics** — deferred. The harness measures CPU encode/submit, not GPU completion. Module doc + test name both name this scope explicitly.
2. **Non-perf branch tests for `render_frame` wrapper** — deferred. 67 lib tests still pass; the perf harness compile/run covers the broad shape.
3. **Hard P95 threshold pinning** — deferred. The corrected harness still REPORTS the soft 1.0 ms target without asserting it; future certification dispatch is the right scope for pinning.

## Worktree State

- Tracked files: clean post-amend (`git status --short --untracked-files=no` empty).
- New untracked items from this turn: 1 (this EXEC packet under `ai_handoffs/`).
- Branch: `main`.
- HEAD: `f8b8ed4 test(editor-shell): add render_frame encode-submit perf harness` (1 commit ahead of `origin/main`; amended from `1f4876c`).
- `origin/main...HEAD`: `0 1` (preserved per CORRECT-round-2 requirement).

## Open Questions for Reviewer / Planner

1. **Closeout readiness** — both consecutive runs cleared the 30 % gate by ≈ 10× margin. The CORRECT-round-2 acceptance ("two consecutive independent release-harness invocations pass; a pass that depends on ignoring a first failed invocation is not acceptable") appears fully satisfied. Reviewer is invited to run the harness twice consecutively on the recorder host to confirm closeout-readiness.
2. **Variance gate tightening** — the observed 1.7 % / 3.0 % variance suggests a 15 % gate would still pass. Whether to tighten is a separate dispatch; flagged here for visibility.
3. **Hard P95 threshold scope** — the soft 1.0 ms P95 target now has ≈ 19× headroom against the observed 0.053 ms median P95. A future "pin the threshold" dispatch may want a tighter soft target.

## Deviations from CORRECT-round-2

None. All `## Approved Corrections` requirements met:

- Only `crates/editor-shell/src/render_frame_e2e_perf.rs` edited in this round. ✓
- `WARMUP_FRAMES = 240`. ✓
- `SAMPLE_BATCHES = 600` (unchanged from round-1). ✓
- `FRAMES_PER_SAMPLE = 50`. ✓
- One `Instant::elapsed` window per batch; stored value = `batch_total_ms / FRAMES_PER_SAMPLE as f64`. ✓
- Module docs and printed output updated to reflect `240 warmup + 600 sample batches x 50 frames x 3 runs`. ✓
- Hard 30 % variance gate retained. ✓
- Soft P95 target reported but not asserted. ✓
- Two consecutive release-harness invocations pass (1.7 % and 3.0 %). ✓
- Existing local commit amended (`1f4876c` → `f8b8ed4`); branch stays at `0 1`. ✓
- No push. ✓

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
