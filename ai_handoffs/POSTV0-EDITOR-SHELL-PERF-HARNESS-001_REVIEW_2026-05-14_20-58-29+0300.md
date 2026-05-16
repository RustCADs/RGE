# Review Report

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Reviewer / OpenAI Codex
TIMESTAMP: 2026-05-14_20-58-29+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_19-33-13+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_20-28-09+0300.md
- crates/editor-shell/src/lib.rs
- crates/editor-shell/src/render_path.rs
- crates/editor-shell/src/render_frame_e2e_perf.rs
STATUS: NEEDS_CORRECTION

## References

- Task Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`
- Superseded Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md`
- Prior Review Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_REVIEW_2026-05-14_19-33-12+0300.md`
- Prior Correction Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_CORRECT_2026-05-14_19-33-13+0300.md`
- Latest Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_20-28-09+0300.md`

## Independently Re-Run Gates

- `git status --short --untracked-files=no` -> empty; tracked tree clean.
- `git rev-list --left-right --count origin/main...HEAD` -> `0 1`; one local commit ahead, no push.
- `git show --stat --oneline --name-only HEAD` -> `1f4876c test(editor-shell): add render_frame encode-submit perf harness`; files are `crates/editor-shell/src/lib.rs`, `crates/editor-shell/src/render_frame_e2e_perf.rs`, `crates/editor-shell/src/render_path.rs`.
- `git diff cd2ecd3..HEAD -- crates/editor-shell/src/render_frame_e2e_perf.rs` -> correction-round diff is confined to `crates/editor-shell/src/render_frame_e2e_perf.rs`.
- `cargo +nightly fmt --check -p rge-editor-shell` -> PASS exit 0.
- `cargo check -p rge-editor-shell` -> PASS exit 0.
- `cargo test -p rge-editor-shell --lib --no-fail-fast` -> PASS exit 0; 67 passed / 0 failed / 1 ignored.
- `cargo run -q -p rge-tool-architecture-lints -- all` -> PASS exit 0; 9 enforcement lints + 1 supplementary PASS, 0 violations.
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` first independent run -> FAIL; variance 54.6% exceeds 30.0% hard gate.
- Same release harness hot rerun -> PASS; variance 13.8%.

## Findings

### Correct

- The amended commit is still one local commit ahead of `origin/main` and has not been pushed.
- The amended commit touches only the original allowed source set: `lib.rs`, `render_path.rs`, and `render_frame_e2e_perf.rs`.
- The correction-round source edit is confined to `crates/editor-shell/src/render_frame_e2e_perf.rs`.
- Non-perf gates are green: fmt, editor-shell check, editor-shell lib tests, and architecture lints all pass.
- The batched sample shape is implemented as requested: `SAMPLE_BATCHES = 600`, `FRAMES_PER_SAMPLE = 10`, one `Instant::elapsed` per batch, and stored samples are per-frame batch means.

### Needs Correction

- **Variance gate still flakes on an independent cold run** - `crates/editor-shell/src/render_frame_e2e_perf.rs:86-97` and `crates/editor-shell/src/render_frame_e2e_perf.rs:254-261`. The latest EXEC reported two consecutive passes (29.4% and 4.3%), but the Reviewer re-run immediately failed the same hard gate with variance 54.6%:

```text
run 0: P95 = 0.023610 ms
run 1: P95 = 0.026860 ms
run 2: P95 = 0.038280 ms
cross-run: median P95 = 0.026860 ms; min P95 = 0.023610 ms; max P95 = 0.038280 ms;
variance across run P95s = 54.6%
panic: variance across 3 run P95s = 54.6% exceeds 30.0% gate
```

Recommended fix: increase measurement stability rather than weakening the gate by raising the warmup and per-sample batch size, then amend the same local commit and prove two consecutive release-harness passes again.

### Latent Risks (Not Blocking)

- The hot rerun passed at 13.8%, so the production path is not implicated; this is measurement robustness, not a render regression.
- The harness still measures CPU encode/submit minus surface acquire/present, not GPU completion. This remains correctly documented and outside this correction.
- Hard P95 threshold pinning remains deferred. The only current hard perf gate is variance stability.

## Test Coverage Assessment

- **Strong**: the ignored release harness exercises the extracted production encode/submit path through `EditorShell::acquire_depth_view` and `EditorShell::render_frame_to_target` on the recorder host.
- **Weak / Missing**: the current sample timing unit is still too close to host scheduling noise on at least one independent run. The test needs a larger warmup and/or larger batch unit before it can be trusted as a closeout gate.

## Doc Accuracy Check

- The commit message and EXEC packet accurately state that v0 certification remains valid and that this is a post-v0 measurement-capture dispatch.
- No docs update is required before the corrected harness itself becomes stable.

## Recommended Action

**ISSUE CORRECTION_PACKET addressing**:

1. Increase recorder-host measurement stability while preserving the 30% hard variance gate.
2. Amend the existing local commit again, keep the branch at `0 1`, do not push.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Reviewer / OpenAI Codex
NEXT_ROLE: PLANNER_AI
EXIT_CODE: 1

---
