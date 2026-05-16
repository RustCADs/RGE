# Task Packet

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_18-26-15+0300
RELATED_FILES:
- V0_RELEASE_CERTIFICATION.md
- Status.md
- HANDOFF.md
- plans/BASELINE.md
- plans/IMPLEMENTATION.md
- crates/editor-shell/src/render_path.rs
- crates/editor-shell/src/lifecycle.rs
- crates/editor-shell/src/render_input.rs
- crates/editor-shell/tests/editor_frame_idle.rs
- crates/editor-shell/tests/render_input_boundary.rs
STATUS: OPEN

## Goal

Run the first post-v0 text-based automation dispatch after v0 certification. The goal is to determine whether the editor-shell mock-event-loop perf harness should proceed, and if so to define the smallest honest implementation dispatch. This is a read-only preflight/design dispatch: inspect the current editor-shell render path, the v0 certification record, and the documented deferral in the baseline/implementation docs; do not change source code.

## Scope

### MAY edit
- None.

### MUST NOT edit
- `crates/**`
- `kernel/**`
- `runtime/**`
- `editor/**`
- `plans/**`
- `docs/**`
- `Status.md`
- `HANDOFF.md`
- `change.md`
- `V0_RELEASE_CERTIFICATION.md`
- `Cargo.toml`
- `Cargo.lock`
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md`
- `ai_handoffs/templates/**`

### MAY add new files
- Exactly one execution report matching `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_EXEC_*.md`.

### MUST NOT add new files
- New source files.
- New tests.
- New docs outside the single EXEC packet.
- New TASK, REVIEW, CORRECT, or CLOSEOUT packets.
- New ADRs, architecture lints, doctrine docs, Cargo entries, scripts, or automation files.

## Deliverables

- One `EXECUTION_REPORT` packet under `ai_handoffs/` with:
  - A clear feasibility verdict: `PROCEED`, `DEFER`, or `NEEDS_HUMAN`.
  - A concise design map for the smallest honest editor-shell end-to-end perf harness, if `PROCEED`.
  - The exact tracked files likely to require edits in a future implementation dispatch.
  - The likely test/harness shape and measurement gate, including what would and would not be certified.
  - A risk list focused on where `EditorShell::render_frame` coupling could balloon.
  - A recommended next dispatch packet shape, but do not create that packet.
  - Confirmation that v0 certification remains valid regardless of this post-v0 preflight.

## Acceptance Criteria

- Exactly one new EXEC packet is produced for this dispatch.
- The EXEC packet footer contains exactly one line-anchored `HANDOFF_STATUS: COMPLETE` marker and routes `NEXT_ROLE: REVIEWER_AI`.
- No tracked files are modified.
- No source, test, Cargo, plan, status, handoff, change, protocol, template, or cert docs are edited.
- No commit and no push are performed.
- The EXEC cites concrete file/line evidence for:
  - The current `EditorShell::render_frame` implementation.
  - The documented `EditorShell::render_frame` end-to-end measurement deferral.
  - The v0 certification state.
- The EXEC explicitly states whether the next implementation dispatch should be started now or deferred.

## Constraints / Non-Goals

- This dispatch does not implement the perf harness.
- This dispatch does not refactor `EditorShell::render_frame`.
- This dispatch does not introduce a mock `ActiveEventLoop`, winit bypass, headless surface wrapper, new trait, new public API, new cargo dependency, or new architecture.
- This dispatch does not rerun expensive gates.
- This dispatch does not alter v0 certification docs.
- This dispatch does not issue a new ordered queue.

## Verification Gates

The Executor MUST run and document the result of each of these in their `EXECUTION_REPORT`:

- `git status --short --untracked-files=no` -> expected no tracked modifications before and after.
- `git rev-list --left-right --count origin/main...HEAD` -> expected `0 0`.
- `git log --oneline --decorate -3` -> expected HEAD at or descended from `b13c176 docs(cert): v0 release certification at 6aaf7f1`.
- `rg -n "pub\\(crate\\) fn render_frame|EditorShell::render_frame|mock event|mock-event|end-to-end|Gate A|v0 release" crates/editor-shell/src crates/editor-shell/tests plans/BASELINE.md plans/IMPLEMENTATION.md V0_RELEASE_CERTIFICATION.md Status.md HANDOFF.md` -> use the hits as evidence.

No cargo build/test is required for this read-only preflight. If the Executor chooses to run any cargo command, document it, but do not treat it as required by this TASK.

## Halt Conditions

The Executor MUST halt without creating an implementation plan beyond the EXEC packet, and set `HANDOFF_STATUS: BLOCKED` if any of the following occur:

- Any tracked file is modified before or during the dispatch.
- `origin/main...HEAD` is not `0 0` and the reason is not understood.
- The v0 certification record is missing or does not show `CERTIFIED v0`.
- The current render path cannot be inspected locally.
- The Executor concludes that even feasibility analysis requires source edits.
- The natural next step would require broad editor architecture, a real winit bypass, a public API redesign, or a new dependency before a small harness can be honestly scoped.

## Planner Notes

This dispatch follows the v0 release certification at commit `b13c176`, which certified commit `6aaf7f1` and pushed the certification docs to `origin/main`. It intentionally uses the repository's Markdown handoff protocol rather than plugin automation. Under protocol v2 Rule 7, no duplicate pre-execution Reviewer2 packet is expected. The Executor should write "No pre-execution review issued for this dispatch." in the EXEC packet's `Pre-Execution Review Consumed` section.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
