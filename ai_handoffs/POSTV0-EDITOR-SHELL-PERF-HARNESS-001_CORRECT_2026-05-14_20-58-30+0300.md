# Correction Packet

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_20-58-30+0300
RELATED_FILES:
- crates/editor-shell/src/render_frame_e2e_perf.rs
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_20-58-29+0300.md
STATUS: CORRECTION_OPEN

## References

- Task Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`
- Superseded Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md`
- Prior Review Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md`
- Prior Correction Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_19-33-13+0300.md`
- Latest Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_20-28-09+0300.md`
- Latest Review Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_20-58-29+0300.md`

## Approved Corrections (Planner Sign-Off)

The Executor MUST act on exactly these corrections - nothing more, nothing less.

1. **Stabilize the recorder-host timing unit** - addresses Review finding "Variance gate still flakes on an independent cold run." Required change: edit only `crates/editor-shell/src/render_frame_e2e_perf.rs`; increase the pre-measurement warmup and per-sample batch size so the first independent release invocation clears the 30% variance gate without relying on a retry. Use this canonical shape unless local evidence forces a smaller equivalent:
   - `WARMUP_FRAMES = 240`
   - `SAMPLE_BATCHES = 600`
   - `FRAMES_PER_SAMPLE = 50`
   - keep one `Instant::elapsed` window per batch and store `batch_total_ms / FRAMES_PER_SAMPLE as f64`
   - update module docs and printed output to reflect `240 warmup + 600 sample batches x 50 frames x 3 runs`
   - keep the hard 30% variance gate
   - keep the soft P95 target reported but not asserted
   Acceptance: the harness output names the new batch shape, and two consecutive independent release-harness invocations pass.
2. **Preserve commit and scope discipline** - amend the existing local commit again. Acceptance: `git rev-list --left-right --count origin/main...HEAD` remains `0 1`; `git show --stat --oneline --name-only HEAD` lists only `crates/editor-shell/src/lib.rs`, `crates/editor-shell/src/render_frame_e2e_perf.rs`, and `crates/editor-shell/src/render_path.rs`; no push is performed.

## Deferred Findings (NOT Approved for This Round)

1. **GPU completion wait semantics** - still deferred. This dispatch certifies CPU encode/submit minus surface acquire/present, not GPU completion.
2. **Non-perf branch tests for `render_frame` wrapper** - still deferred. The existing editor-shell lib suite remains green; this correction is only about recorder-host measurement robustness.
3. **Hard P95 threshold pinning** - still deferred. Do not add a hard P95 assertion in this correction. The future threshold should be chosen after the measurement harness is stable.

## Updated Acceptance Criteria

Updated from the prior correction packet:

- The Reviewer must be able to run the release harness twice consecutively after the correction and observe both runs pass the hard 30% variance gate.
- A pass that depends on ignoring a first failed invocation is not acceptable.
- The branch must remain one local commit ahead of `origin/main`; no push.

## Re-Verification Gates

The Executor MUST re-run these gates after the corrections:

- `git status --short --untracked-files=no` -> expected empty.
- `git rev-list --left-right --count origin/main...HEAD` -> expected `0 1`.
- `git show --stat --oneline --name-only HEAD` -> expected only the three allowed editor-shell source files.
- `cargo +nightly fmt --check -p rge-editor-shell` -> expected exit 0.
- `cargo check -p rge-editor-shell` -> expected exit 0.
- `cargo test -p rge-editor-shell --lib --no-fail-fast` -> expected 67 passed / 0 failed / 1 ignored.
- `cargo run -q -p rge-tool-architecture-lints -- all` -> expected 9 enforcement + 1 supplementary PASS, 0 violations.
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` -> expected exit 0, first consecutive run.
- Same release harness immediately again -> expected exit 0, second consecutive run.

## Halt Conditions (Updated if Any)

- If two consecutive release-harness passes cannot be achieved after increasing warmup/batching, STOP and report `HANDOFF_STATUS: BLOCKED`; do not weaken or delete the 30% variance gate.
- If correction requires editing any file other than `crates/editor-shell/src/render_frame_e2e_perf.rs`, STOP and report `HANDOFF_STATUS: BLOCKED`.
- If the branch would cease to be exactly one local commit ahead of `origin/main`, STOP and report `HANDOFF_STATUS: BLOCKED`.
- Do not push.

## Planner Notes

The hot rerun passing at 13.8% shows the extracted render path is not the problem. The failed independent first run at 54.6% shows the measurement unit is still too close to host timing noise for a closeout gate. This correction keeps the same metric and the same hard variance gate, but raises the timing window so the gate measures the render path instead of scheduler jitter.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
