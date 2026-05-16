# Correction Packet

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_19-33-13+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md
- crates/editor-shell/src/render_frame_e2e_perf.rs
STATUS: CORRECTION_OPEN

## References

- Task Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`
- Latest Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md`
- Latest Review Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md`

## Approved Corrections (Planner Sign-Off)

The Executor MUST act on exactly this correction and nothing else:

1. **Stabilize perf harness sampling** -- addresses Review finding "Perf harness hard gate is flaky under reviewer re-run." Required change: edit only `crates/editor-shell/src/render_frame_e2e_perf.rs` so each timing sample measures a small batch of consecutive frames and stores the per-frame mean for that batch, rather than timing each individual frame. Suggested concrete shape:
   - Keep `RUN_COUNT = 3`.
   - Keep the existing 60 warmup frames per run.
   - Replace `SAMPLE_FRAMES = 600` with `SAMPLE_BATCHES = 600` and `FRAMES_PER_SAMPLE = 10`, or an equivalent naming scheme.
   - For each sample batch, call `tick_one_frame` `FRAMES_PER_SAMPLE` times inside one `Instant` window and push `elapsed_ms / FRAMES_PER_SAMPLE` into the sample vector.
   - Update module docs and printed output so the harness clearly says `600 sample batches x 10 frames` or equivalent.
   - Keep the hard variance gate at 30%.
   - Keep the soft P95 target reported but not asserted.
   - Do not add public API, dependencies, docs/status/cert edits, or new source files.
   Acceptance:
   - The release harness passes twice consecutively on the recorder host.
   - Both consecutive outputs are recorded in the correction EXEC summary, including median P95 and variance.

## Deferred Findings (NOT Approved for This Round)

1. **GPU-completion wait semantics** -- deferred because the TASK explicitly scoped this dispatch to encode/submit minus surface acquire/present. Future trigger: if humans want a GPU-completed editor-shell perf gate comparable to Gate A.
2. **Non-perf branch tests for `render_frame` wrapper** -- deferred because existing compile/lib/perf gates cover this refactor enough for this bounded dispatch. Future trigger: if render-frame control flow changes again.
3. **Hard threshold pinning** -- deferred because this dispatch is measurement-capture. Future trigger: a separate threshold-certification dispatch.

## Updated Acceptance Criteria

Supersedes only the sampling mechanics from the original TASK:

- The harness may measure 600 sample batches of 10 frames each and compute per-frame batch means, rather than measuring 600 individual frames directly.
- The release harness command must pass twice consecutively after the correction.
- All other TASK acceptance criteria remain unchanged.

## Re-Verification Gates

The Executor MUST re-run and document:

- `git status --short --untracked-files=no`
- `git rev-list --left-right --count origin/main...HEAD`
- `cargo +nightly fmt --check -p rge-editor-shell` -> expected exit 0
- `cargo check -p rge-editor-shell` -> expected exit 0
- `cargo test -p rge-editor-shell --lib --no-fail-fast` -> expected 67 passed / 0 failed / 1 ignored
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` -> expected exit 0
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` -> run a second consecutive time, expected exit 0
- `cargo run -q -p rge-tool-architecture-lints -- all` -> expected exit 0
- `git show --stat --oneline --name-only HEAD`

## Halt Conditions (Updated if Any)

- Unchanged from Task Packet, plus:
  - If two consecutive release-harness passes cannot be achieved with the batched measurement shape, halt with `HANDOFF_STATUS: BLOCKED` and preserve the observed outputs.
  - If the correction requires editing outside `crates/editor-shell/src/render_frame_e2e_perf.rs`, halt with `HANDOFF_STATUS: BLOCKED`.

## Planner Notes

The first implementation is broadly correct, but the required perf gate is not stable enough to close: reviewer observed one immediate failure at 59.4% variance followed by a pass at 14.8%. The likely cause is the per-frame timing unit being too small for Windows scheduling/timer noise. Batching keeps the same encode/submit path, keeps the recorder-host-only posture, and avoids weakening the 30% variance gate.

If all correction gates pass, amend the existing local commit so the final branch remains one commit ahead of `origin/main`. Do not push.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
