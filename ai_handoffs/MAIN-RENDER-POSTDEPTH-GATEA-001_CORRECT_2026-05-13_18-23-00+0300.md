# Correction Packet

DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-13_18-23-00+0300
RELATED_FILES:
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-12-01+0300.md
- plans/IMPLEMENTATION.md
- plans/BASELINE.md
- Status.md
- HANDOFF.md
- change.md
STATUS: CORRECTION_OPEN

## References

- Task Packet: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md`
- Latest Execution Report: none yet; this is a pre-execution scope correction.
- Latest Review Report: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-12-01+0300.md`

## Approved Corrections (Planner Sign-Off)

The Executor MUST act on exactly this correction in addition to the original Task Packet:

1. **Allow implementation-plan status update** - Manual packet audit found that the Task Packet correctly targets the post-depth Gate A measurement gap, but accidentally places `plans/IMPLEMENTATION.md` in `MUST NOT edit` even though the stale deferred marker lives there. Required change: treat `plans/IMPLEMENTATION.md` as MAY edit for status-marker/result wording only. Acceptance: if the post-depth Gate A measurement succeeds, `plans/IMPLEMENTATION.md` no longer leaves line 468's post-depth measurement gap sounding deferred/unrun; if the measurement is blocked or fails, leave the deferral wording honest and record the blocked/failed result in the EXEC packet.

## Deferred Findings (NOT Approved for This Round)

1. **Editor-shell end-to-end harness** - Deferred because the original task intentionally chooses the smaller gfx-level synthetic depth-attached harness. Future trigger: separate dispatch if the user asks for non-winit editor-shell perf infrastructure or production-path end-to-end measurement.
2. **Visual-fidelity broadening** - Deferred because `MAIN-RENDER-VISUAL-FIDELITY-002` is already the queued Turn 2 dispatch.

## Updated Acceptance Criteria

- Original Task Packet acceptance criteria remain in force.
- Add: `plans/IMPLEMENTATION.md` may be edited only to replace or qualify the stale post-depth Gate A measurement-gap marker with the real Turn 1 outcome.
- Add: no PLAN target retargeting; only status/result wording may change in `plans/IMPLEMENTATION.md`.

## Re-Verification Gates

The Executor MUST run the same gates from the Task Packet after applying this correction:

- `cargo +nightly fmt --check -p rge-gfx` -> expected exit 0
- `cargo test -p rge-gfx --release --test gate_a_simple_scene_depth_60fps -- --ignored --nocapture` -> expected exit 0 on GPU host, or documented BLOCKED if no GPU adapter
- `cargo test -p rge-gfx --test lit_mesh_depth_overlay_smoke` -> expected exit 0 or GPU-skip success
- `cargo test -p rge-gfx --test frame_graph_umbrella_smoke` -> expected exit 0
- `cargo test --workspace --no-fail-fast` -> expected exit 0; test count may gain one ignored test
- `cargo run -q -p rge-tool-architecture-lints -- all` -> expected exit 0 (9 enforcement + 1 supplementary PASS)
- `git diff --check` -> expected exit 0

## Halt Conditions (Updated if Any)

Unchanged from Task Packet, with one clarification: editing `plans/IMPLEMENTATION.md` for the specific status-marker/result wording above is allowed and must not be treated as a halt condition.

## Planner Notes

This correction exists because the manual check found a scope contradiction before execution. The original task is otherwise valid. Claude Reviewer2 should read this correction packet together with the original TASK and OpenAI Reviewer1 packet before deciding whether to approve execution.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
