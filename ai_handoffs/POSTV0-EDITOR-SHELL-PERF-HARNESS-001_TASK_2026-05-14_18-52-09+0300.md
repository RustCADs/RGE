# Task Packet

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_18-52-09+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_TASK_2026-05-14_18-26-15+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_EXEC_2026-05-14_18-31-54+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_REVIEW_2026-05-14_18-38-01+0300.md
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_CLOSEOUT_2026-05-14_18-38-02+0300.md
- V0_RELEASE_CERTIFICATION.md
- plans/BASELINE.md
- plans/IMPLEMENTATION.md
- crates/editor-shell/src/render_path.rs
- crates/editor-shell/src/lib.rs
- crates/editor-shell/tests/editor_frame_idle.rs
STATUS: OPEN

## Goal

Implement the first post-v0 editor-shell render-frame performance harness. The harness must measure the editor-shell frame encode/submit path without winit surface acquire/present, using the smallest source refactor that preserves the existing production `render_frame` behavior. This dispatch is source-editing but tightly bounded: it should add a crate-local ignored timing harness and the minimum helper extraction needed to drive it.

This dispatch is measurement-capture, not final certification. Do not hard-code a new v0 release gate threshold yet. Capture the recorder-host P50/P95/min/max/worst-frame/variance result, enforce stability/finite measurements, and recommend a future threshold from the observed value in the EXEC packet.

## Scope

### MAY edit

- `crates/editor-shell/src/render_path.rs`
- `crates/editor-shell/src/lib.rs`

### MAY add new files

- `crates/editor-shell/src/render_frame_e2e_perf.rs`
- Exactly one execution report matching `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_*.md`

### MUST NOT edit

- `crates/editor-shell/src/lifecycle.rs`
- `crates/editor-shell/src/render_input.rs`
- `crates/editor-shell/tests/**`
- `crates/gfx/**`
- `crates/brep-render/**`
- `crates/cad-core/**`
- `crates/cad-projection/**`
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

### MAY commit

- If and only if all required gates pass, the Executor MAY create exactly one local commit.
- The commit MUST include only the allowed source files above.
- The commit MUST NOT include `ai_handoffs/*`, root-level `OPENAItoCLAUDE_*`, root-level `CLAUDEtoOPENAI_*`, logs, or unrelated untracked files.
- Recommended subject: `test(editor-shell): add render_frame encode-submit perf harness`

### MUST NOT

- Push.
- Add a dependency.
- Add public API.
- Retarget v0 certification.
- Modify protocol docs/templates.
- Start a follow-on dispatch.

## Required Implementation Shape

Implement the preflight-approved crate-local shape unless it proves impossible:

1. Add a crate-local test module declaration in `crates/editor-shell/src/lib.rs`, for example:

   ```rust
   #[cfg(test)]
   mod render_frame_e2e_perf;
   ```

2. In `crates/editor-shell/src/render_path.rs`, factor the render path so a test can drive the editor-shell encode/submit body against an offscreen `wgpu::TextureView` without needing `SurfaceContext` or `Window`.

3. Keep new helpers private or `pub(crate)`. Do not add public perf-only methods.

4. Preserve production behavior of `EditorShell::render_frame(&mut self) -> bool`:
   - It still acquires the current surface texture.
   - It still presents.
   - It still schedules redraw through the existing window path.
   - It still returns `false` when render state is uninitialized.
   - It still returns `true` for recoverable skip paths that should continue the event loop.

5. Avoid setup drift. Extract shared setup for the common post-surface/headless work where practical. The production `init_render_state(&ActiveEventLoop)` and the new headless test setup should share the material/light/camera/pipeline/mesh/pool/frame-graph initialization path rather than duplicating it wholesale.

6. Add `crates/editor-shell/src/render_frame_e2e_perf.rs` with one ignored release-only test. The test should:
   - Build the existing single-cuboid editor scene through existing editor-shell/cad-projection/cad-core paths.
   - Initialize render state headlessly with a concrete target format such as `wgpu::TextureFormat::Bgra8UnormSrgb`.
   - Allocate one offscreen color target texture and view.
   - Run 3 measurements.
   - For each measurement, run 60 warmup frames and 600 sample frames.
   - Print P50, P95, min, max, worst-frame, and variance across run P95s.
   - Assert all measurements are finite and non-negative.
   - Assert variance across the 3 run P95s is <= 30%.
   - Do not assert a hard P95 threshold yet; report whether P95 is under the proposed soft target of 1.0 ms in the test output and EXEC.

7. Name the scope honestly in the test and output: this measures editor-shell encode/submit minus surface acquire/present. It does not certify winit event-loop scheduling, surface acquire, present/vsync, universal hardware, cold-start, sustained thermal behavior, or loaded-scene complexity beyond the current single-cuboid render path.

## Deliverables

- Updated `crates/editor-shell/src/render_path.rs` with a narrow helper extraction and a headless render-state setup path.
- Updated `crates/editor-shell/src/lib.rs` with the `#[cfg(test)]` module declaration.
- New `crates/editor-shell/src/render_frame_e2e_perf.rs` ignored release-only timing harness.
- One `EXECUTION_REPORT` packet under `ai_handoffs/` with:
  - Exact files changed.
  - Whether a local commit was created, including hash if so.
  - Exact test/gate commands and output summaries.
  - Captured P50/P95/min/max/worst-frame/variance values from the harness.
  - A recommendation for the future hard threshold, if the measurement is stable.
  - Explicit confirmation that v0 certification remains valid.

## Acceptance Criteria

- No public API additions.
- No new dependencies.
- No changes outside the allowed files.
- Production `render_frame` behavior remains equivalent for surface acquire/present/redraw.
- The ignored release harness compiles and runs on the recorder host.
- The harness output includes P50, P95, min, max, worst-frame, and variance across 3 run P95s.
- Variance across run P95s is <= 30%.
- `cargo +nightly fmt --check -p rge-editor-shell` passes.
- `cargo check -p rge-editor-shell` passes.
- `cargo test -p rge-editor-shell --lib --no-fail-fast` passes.
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture` passes on the recorder host.
- `cargo run -q -p rge-tool-architecture-lints -- all` passes.
- `git status --short --untracked-files=no` is clean after the source commit or, if no commit is made, contains only the allowed tracked edits.
- No push is performed.

## Verification Gates

The Executor MUST run and document the result of each command:

- `git status --short --untracked-files=no`
- `git rev-list --left-right --count origin/main...HEAD`
- `git log --oneline --decorate -3`
- `cargo +nightly fmt --check -p rge-editor-shell`
- `cargo check -p rge-editor-shell`
- `cargo test -p rge-editor-shell --lib --no-fail-fast`
- `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture`
- `cargo run -q -p rge-tool-architecture-lints -- all`
- `git diff --stat HEAD -- crates/editor-shell/src/render_path.rs crates/editor-shell/src/lib.rs crates/editor-shell/src/render_frame_e2e_perf.rs`
- If a commit is created: `git show --stat --oneline --name-only HEAD`

## Halt Conditions

The Executor MUST halt and write the EXEC packet with `HANDOFF_STATUS: BLOCKED` or `NEEDS_HUMAN` if any of these occur:

- Implementing the harness requires editing outside the allowed file set.
- A public API addition appears necessary.
- A new dependency appears necessary.
- `lifecycle.rs`, `render_input.rs`, `Cargo.toml`, `Cargo.lock`, plans, status docs, handoff docs, cert docs, or protocol files need changes.
- Production `render_frame` behavior cannot be preserved.
- The headless path cannot initialize without a real `ActiveEventLoop`.
- The release harness cannot obtain a GPU adapter on the recorder host.
- The release harness repeatedly exceeds 30% variance.
- Any required non-perf compile/test/lint gate fails and cannot be fixed within the allowed file set.
- The natural fix expands into broad editor architecture, real winit mocking, a surface abstraction rewrite, multi-scene rendering, or threading.

## Planner Notes

- Current clean anchor before this TASK: HEAD `b13c176 (HEAD -> main, origin/main) docs(cert): v0 release certification at 6aaf7f1`; `origin/main...HEAD` is `0 0`; tracked tree clean.
- The preflight dispatch `POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001` is CLOSED and approved this implementation direction.
- Under protocol v2 Rule 7, no duplicate Reviewer2 packet is expected. Write `No pre-execution review issued for this dispatch.` in the EXEC packet's `Pre-Execution Review Consumed` section.
- This is not a v0 blocker. v0 certification remains valid whether this implementation succeeds, blocks, or needs follow-up.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-HARNESS-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
