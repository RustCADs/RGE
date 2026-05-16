# Task Packet

DISPATCH_ID: POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-16_10-31-15+0300
RELATED_FILES:
- crates/script-bench/src/script_host.rs
- crates/script-bench/BASELINE.md
- crates/script-bench/METHODOLOGY.md
- crates/script-bench/Cargo.toml
- Cargo.lock
- ai_handoffs/POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001_TASK_2026-05-16_10-31-15+0300.md
STATUS: OPEN

## Goal

Improve post-v0 Phase 3.4 script-bench memory-soak evidence by adding direct process-memory sampling to the existing `script_host` soak harness. The current documentation explicitly notes that the one-hour soak proved no panic, hang, or OOM, but did not capture `peak_rss` / `vss_delta` style process-memory metrics. This revision 0 task has no prior Claude gate. The Executor must make the smallest practical change so `MemorySoakReport` or a closely adjacent report type exposes peak resident/process working-set and start-to-end memory delta, and the formal soak stdout/docs no longer describe these metrics as missing.

## Scope

This is a source-editing task, not an audit-only task.

### MAY edit

- `crates/script-bench/src/script_host.rs`
- `crates/script-bench/BASELINE.md`
- `crates/script-bench/METHODOLOGY.md`
- `crates/script-bench/Cargo.toml`, only if a new script-bench-local dependency is genuinely required after attempting a standard-library or minimal platform-FFI implementation.
- `Cargo.lock`, only if `crates/script-bench/Cargo.toml` is changed for the dependency exception above.

### MUST NOT edit

- Any file outside the MAY edit list, except the single EXEC packet allowed below.
- Any other crate source or test file.
- Any `plans/**`, `docs/**`, `Status.md`, `HANDOFF.md`, `change.md`, release-certification file, protocol document, schema, script, template, `.gitignore`, generated import, or automation file.
- Any existing task, review, correction, closeout, state, or sidecar packet.
- The one-hour soak duration constant, unless the change is documentation-only and does not alter runtime behavior.

### MAY add new files

- Exactly one execution report matching `ai_handoffs/POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001_EXEC_*.md`.

### MUST NOT add new files

- New source modules, test files, docs outside `BASELINE.md` / `METHODOLOGY.md`, schemas, scripts, benchmarks, fixtures, sidecar `.meta.json` files, task/review/correction/closeout packets, or cleanup artifacts.
- New dependencies, except the tightly scoped script-bench-local dependency exception described under MAY edit.

### MAY commit

- If all required gates pass, the Executor MAY create exactly one local implementation commit.
- The commit MUST include only allowed source/doc/manifest/lock changes. It MUST NOT include `ai_handoffs/*`, automation logs, root-level cross-AI handoff files, unrelated tracked edits, or unrelated untracked files.
- Recommended subject: `test(script-bench): report memory soak process metrics`

## Required Implementation Shape

1. Inspect the existing `MemorySoakConfig`, `MemorySoakReport`, `ScriptHostBench::memory_soak`, and `script_host::tests::phase_3_memory_soak_one_hour` paths in `crates/script-bench/src/script_host.rs`.
2. Add the smallest process-memory sampler needed by `memory_soak`.
   - Preferred: standard library plus platform-gated code in `script_host.rs`.
   - Windows recorder-host support is required. On Windows, resident memory may be documented as process working set.
   - Linux `/proc/self/status` support is acceptable if it can be added simply.
   - Unsupported platforms must still compile. If a metric is unavailable on a platform, represent that honestly rather than fabricating a value.
3. Report, at minimum, peak resident/process working-set bytes and a start-to-end memory delta from the soak run.
   - Preferred field names are `peak_rss_bytes` and `vss_delta_bytes`, either directly on `MemorySoakReport` or on an adjacent report struct reachable from it.
   - If the platform reports process commit/private bytes instead of true virtual size, name or document that mapping clearly in Rust doc comments and markdown.
4. Sample memory at soak start, after each completed hot-reload cycle, and at soak end, so peak and delta are derived from observed samples.
5. Keep the existing one-hour soak ignored by default. Do not weaken the existing preservation assertions.
6. Update the one-hour soak `println!` so `--nocapture` output includes the new memory metrics when available.
7. Add or revise bounded unit coverage in `script_host.rs` only. The focused test must use a very short `MemorySoakConfig` duration and a small entity count, and must not require running the one-hour ignored soak.
8. Update `BASELINE.md` and `METHODOLOGY.md` to replace the "peak_rss / vss_delta missing" limitation with the new harness behavior and any platform caveat. Do not claim a new formal one-hour memory baseline unless the one-hour soak was actually run under a separately justified release-readiness invocation.
9. Avoid a new dependency. If the Executor concludes a dependency is required, keep it local to `crates/script-bench`, explain why standard library / platform FFI was insufficient in the EXEC packet, and keep the dependency surface minimal.

## Deliverables

- Updated `crates/script-bench/src/script_host.rs` with process-memory sampling integrated into `ScriptHostBench::memory_soak`.
- A `MemorySoakReport` or adjacent report type that exposes observed peak resident/process working-set and start-to-end memory delta.
- Focused bounded tests in `script_host.rs` proving the new fields are populated on the Windows recorder host and that unsupported platforms remain explicit if applicable.
- Updated `BASELINE.md` and `METHODOLOGY.md` language describing the new memory-soak metrics and removing the stale "direct RSS/VSS metrics are missing" claim.
- One EXEC packet under `ai_handoffs/` documenting changed files, dependency decision, exact gates run, whether a commit was created, and captured output from the bounded memory-metric check.

## Acceptance Criteria

- The formal one-hour soak test remains `#[ignore]` and is not required for this dispatch.
- The Executor does not run `script_host::tests::phase_3_memory_soak_one_hour -- --ignored` unless the EXEC packet first explains an explicit release-readiness reason; the expected path for this task is not to run it.
- A bounded test or short-duration harness check exercises `memory_soak` and prints or asserts the new memory metrics without taking more than a few seconds on the recorder host.
- On Windows, the bounded check reports numeric process-memory values for peak resident/working-set and start-to-end delta.
- On unsupported non-Windows platforms, the crate still compiles and the report/docs clearly indicate unsupported or unavailable memory metrics.
- Existing preservation invariants remain intact: `cycles > 0` and `restored_components == cycles * entity_count`.
- Documentation no longer says `MemorySoakReport` lacks `peak_rss` / `vss_delta` evidence; it describes what is now measured and what remains out of scope.
- No dependency is added unless justified in the EXEC packet and localized to `crates/script-bench`.
- No unrelated protocol, automation, release-certification, roadmap, status, or source files are changed.

## Constraints / Non-Goals

- Do not retarget PLAN thresholds, v0 certification, or Phase 3.4 pass/fail criteria.
- Do not rerun or overwrite the 2026-05-12 formal one-hour baseline rows as if this dispatch produced a new one-hour release soak.
- Do not add CI ingestion, Criterion JSON aggregation, a new benchmark binary, or broad reporting schema changes.
- Do not introduce unsafe code beyond a narrowly documented platform FFI boundary if Windows process-memory APIs require it.
- Do not change the script-host WASM ABI, Counter fixtures, ECS bridge, hot-reload semantics, or wasmtime strategy behavior.
- Do not mask memory sampler failures as zeros. Use `Option`, an explicit availability enum, or an error-bearing adjacent report shape if needed.
- Do not push.

## Verification Gates

The Executor MUST run and document the result of each command in the EXEC packet:

- `git status --short --untracked-files=no`
- `cargo +nightly fmt --check -p rge-script-bench`
- `cargo check -p rge-script-bench`
- `cargo test -p rge-script-bench --lib --no-fail-fast script_host::tests`
- A focused bounded memory-metric test command, for example `cargo test -p rge-script-bench --lib script_host::tests::memory_soak_reports_process_memory_metrics -- --nocapture`, adjusted to the actual test name.
- `cargo run -q -p rge-tool-architecture-lints -- all`
- `git diff --check -- crates/script-bench/src/script_host.rs crates/script-bench/BASELINE.md crates/script-bench/METHODOLOGY.md crates/script-bench/Cargo.toml Cargo.lock`
- `git diff --stat -- crates/script-bench/src/script_host.rs crates/script-bench/BASELINE.md crates/script-bench/METHODOLOGY.md crates/script-bench/Cargo.toml Cargo.lock`
- If a commit is created: `git show --stat --oneline --name-only HEAD`

The Executor MUST NOT use the one-hour ignored soak as a required gate for this dispatch.

## Halt Conditions

The Executor MUST halt and write the EXEC packet with `HANDOFF_STATUS: BLOCKED` or `NEEDS_HUMAN` if any of the following occur:

- Implementing process-memory sampling requires edits outside the allowed file set.
- The only viable implementation requires a broad dependency, workspace-wide dependency movement, or a dependency outside `crates/script-bench`.
- Windows process-memory sampling cannot produce numeric resident/working-set and delta metrics on the recorder host.
- The implementation would require changing the WASM fixtures, script-host ABI, ECS bridge, wasmtime runtime crates, or formal soak duration.
- The bounded memory-metric test is flaky, takes unexpectedly long, or requires the one-hour ignored soak to prove the change.
- Existing `script_host` tests or architecture lints fail and cannot be fixed within the allowed files.
- Any required change would alter v0 certification, PLAN thresholds, release status files, protocol docs, automation scripts, or unrelated handoff artifacts.
- The workspace shows unrelated tracked modifications in a file the Executor needs to edit and the safe merge path is unclear.

## Planner Notes

- Current limitation is documented in `BASELINE.md`: the formal one-hour soak currently records elapsed time, preservation assertions, and no OOM/hang/panic, but explicitly says `MemorySoakReport` has no `peak_rss` / `vss_delta` fields.
- The one-hour soak `println!` was already improved to show existing report fields, but not direct process-memory metrics. This task should extend that line with new memory fields rather than adding a second long-running harness.
- The recorder environment for this dispatch is Windows-oriented. A Windows-only sampler is acceptable if it is cfg-gated and honest in docs, but a simple Linux `/proc/self/status` path is welcome if it does not expand scope.
- No pre-execution Claude gate exists for this revision.

---

HANDOFF_STATUS: COMPLETE
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
