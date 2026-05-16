# Review Report

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Reviewer / OpenAI Codex
TIMESTAMP: 2026-05-14_19-33-12+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md
- crates/editor-shell/src/lib.rs
- crates/editor-shell/src/render_path.rs
- crates/editor-shell/src/render_frame_e2e_perf.rs
STATUS: NEEDS_CORRECTION

## References

- Task Packet: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_TASK_2026-05-14_18-52-09+0300.md`
- Execution Report: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_19-19-40+0300.md`

## Independently Re-Run Gates

- `git status --short --untracked-files=no` -> empty output; tracked tree clean post-commit.
- `git rev-list --left-right --count origin/main...HEAD` -> `0 1`; exactly one local commit ahead, no push.
- `git log --oneline --decorate -5` -> HEAD `cd2ecd3 test(editor-shell): add render_frame encode-submit perf harness`.
- EXEC footer poll for `^HANDOFF_STATUS: COMPLETE$` -> `1`.
- TASK footer poll for `^HANDOFF_STATUS: COMPLETE$` -> `1`.
- `git show --stat --oneline --name-only HEAD` -> commit contains only `crates/editor-shell/src/lib.rs`, `crates/editor-shell/src/render_path.rs`, `crates/editor-shell/src/render_frame_e2e_perf.rs`.
- `cargo +nightly fmt --check -p rge-editor-shell` via `A:\RustCache\cargo\bin\cargo.exe` -> PASS.
- `cargo check -p rge-editor-shell` -> PASS.
- `cargo check -p rge-editor-shell --all-targets` -> PASS.
- `cargo test -p rge-editor-shell --lib --no-fail-fast` -> PASS, 67 passed / 0 failed / 1 ignored.
- `cargo run -q -p rge-tool-architecture-lints -- all` -> PASS, 9 enforcement + 1 supplementary, 0 violations.
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` first reviewer run -> FAIL: median P95 0.0515 ms, min P95 0.0267 ms, max P95 0.0573 ms, variance 59.4%, above 30% gate.
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` second reviewer run -> PASS: median P95 0.0379 ms, min P95 0.0332 ms, max P95 0.0388 ms, variance 14.8%.

## Findings

### Correct

- Scope control is correct. The local commit touches only the three allowed editor-shell source files and no handoff/root notes were committed.
- Public API discipline is correct. New production/test helpers are crate-local or private, and `Cargo.toml` / `Cargo.lock` are untouched.
- The production refactor shape is sound. `render_frame` still prepares depth before surface acquire, still acquires/presents through the surface path, still schedules redraw on success/recoverable skips, and still returns `false` for uninitialized state.
- The new harness does measure the intended path: `acquire_depth_view` plus `render_frame_to_target` against an offscreen color view, with no surface acquire/present.
- Non-perf gates pass independently: fmt, check, lib tests, and architecture lints.

### Needs Correction

- **Perf harness hard gate is flaky under reviewer re-run** -- `crates/editor-shell/src/render_frame_e2e_perf.rs:221-324`. The first independent invocation failed the required release harness gate with 59.4% P95 variance; the second invocation passed at 14.8%. Because the TASK acceptance criteria require the harness command to pass, a gate that fails on an immediate re-run is not closeout-safe. Recommended fix: stabilize the harness measurement by batching multiple frames per timing sample so the measured unit is above the Windows timer/scheduler noise floor, then prove it with two consecutive passing release-harness invocations.

### Latent Risks (Not Blocking After Correction)

- The harness currently measures CPU encode/submit without GPU completion wait. This matches the TASK wording, but future docs must keep saying "encode/submit minus surface acquire/present" and must not compare it directly to full GPU-completion Gate A numbers.
- Worst-frame spikes remain scheduler-sensitive. This is acceptable for measurement capture if the P95 variance gate becomes stable.
- Hard P95 threshold pinning remains deferred. The current dispatch should capture and recommend, not certify a final threshold.

## Test Coverage Assessment

- **Strong**: The crate-local ignored harness exercises actual render-path setup, transient depth acquisition, command encoding, bind-group setup, draw submission, and the new crate-local helper path.
- **Weak / Missing**: There is no dedicated non-perf unit test for the refactored production `render_frame` wrapper branches; existing lib tests plus the perf harness compile/run cover the broad shape, but not every branch.

## Doc Accuracy Check

- No tracked docs were edited, matching the TASK scope.
- `V0_RELEASE_CERTIFICATION.md` remains valid because this dispatch is post-v0 and the harness was already listed as a non-blocking deferral.

## Recommended Action

**ISSUE CORRECTION_PACKET addressing**:

1. Stabilize the release perf harness measurement so `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` is not a one-run coin flip around the 30% variance gate.

Do not close this dispatch until the correction EXEC lands and the reviewer re-runs the corrected harness.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Reviewer / OpenAI Codex
NEXT_ROLE: PLANNER_AI
EXIT_CODE: 1

---
