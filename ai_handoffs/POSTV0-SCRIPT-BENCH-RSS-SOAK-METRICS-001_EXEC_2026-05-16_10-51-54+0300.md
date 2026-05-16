# Execution Report

DISPATCH_ID: POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001
AUTHOR: Executor / Claude
TIMESTAMP: 2026-05-16_10-51-54+0300
RELATED_FILES:
- crates/script-bench/src/script_host.rs — process-memory sampler + `ProcessMemoryMetrics`, integrated into `memory_soak`; one-hour soak `println!` extended; bounded test added; SPLIT-EXEMPTION annotation
- crates/script-bench/Cargo.toml — added `memory-stats = "1"` dependency with rationale comment
- Cargo.lock — `memory-stats 1.2.0` locked (reuses existing `libc` + `windows-sys 0.52.0`)
- crates/script-bench/BASELINE.md — replaced stale "no `peak_rss`/`vss_delta`" limitation with the new harness behavior section
- crates/script-bench/METHODOLOGY.md — W5 soak gate description + change-log entry for the process-memory metrics
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001_TASK_2026-05-16_10-31-15+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. (The Task Packet states
this revision 0 task has no prior Claude gate.)

## What I Changed

### Source

- `crates/script-bench/src/script_host.rs`:
  - New public struct `ProcessMemoryMetrics` (adjacent to `MemorySoakReport`)
    exposing `peak_rss_bytes`, `start_rss_bytes` / `end_rss_bytes`,
    `start_vss_bytes` / `end_vss_bytes`, `vss_delta_bytes` (signed, saturating),
    and `samples`. Rust doc comments spell out the Windows / Linux / unsupported
    platform mapping.
  - New field `MemorySoakReport::process_memory: Option<ProcessMemoryMetrics>` —
    `None` honestly represents platforms with no supported sampler; it does not
    weaken the preservation invariants.
  - New private free functions `sample_process_memory()` (returns
    `Option<(resident, virtual)>` bytes; `None` on unsupported platforms) and
    `memory_delta_bytes()` (signed end-minus-start, saturating into `i64`).
  - `ScriptHostBench::memory_soak` now samples process memory at soak start,
    after each completed hot-reload cycle, and at soak end; `peak_rss` and the
    end footprint are derived purely from observed samples. The one-hour soak
    duration constant and the swap loop semantics are unchanged.
  - `phase_3_memory_soak_one_hour` `println!` extended with a
    `phase3_memory_soak_memory: …` line (or an explicit "sampling unavailable"
    line) so `--nocapture` surfaces the new metrics. The test stays
    `#[ignore]`'d; its preservation assertions (`cycles > 0`,
    `restored_components == cycles * entity_count`) are untouched.
  - Added a `// SPLIT-EXEMPTION:` annotation at the top of the file: the
    additions push it from 881 to ~1082 lines, past the 1000-line hard cap of
    the `split-exemption` architecture lint. Splitting is barred by this
    dispatch's scope (no new source modules) and would scatter the shared
    Counter fixtures/helpers; the annotation is the lint's sanctioned escape
    hatch and matches the `loft.rs` precedent. See "Deviations" below.

### Tests

- `script_host::tests::memory_soak_reports_process_memory_metrics` (new,
  non-`#[ignore]`'d): runs a bounded soak (8 entities, 20 ms duration floor),
  asserts the preservation invariants still hold, asserts
  `samples == cycles + 2` and `peak_rss >= start/end` when metrics are present,
  and — under `#[cfg(windows)]` — asserts the recorder host produces non-zero
  numeric resident / peak / end working-set values. Completes in ~0.04 s.

### Docs

- `crates/script-bench/BASELINE.md`: rewrote the stale "It does NOT certify …
  NO `peak_rss` / `vss_delta`" bullet to scope it to the 2026-05-12 run, and
  replaced the "Harness limitation flagged for future improvement" paragraph
  with a new "Memory-soak process-memory metrics — harness revision 2026-05-16"
  section describing the captured fields, the platform mapping, the dependency
  rationale, and the explicit statement that no new one-hour baseline row is
  published (the 2026-05-12 RUN rows stand unchanged).
- `crates/script-bench/METHODOLOGY.md`: expanded the W5 "Script-host soak gate"
  / "What is measured" paragraphs to describe the process-memory sampling, and
  added a dated change-log entry (no methodology version bump — no workload
  constant changed).

## Per-File Summary

Dependency decision: a standard-library / minimal platform-FFI sampler was
attempted first and rejected. Reading the Windows process working set requires
an `unsafe extern` FFI call (`GetProcessMemoryInfo`); the workspace lint
`unsafe_code = "forbid"` (inherited by `rge-script-bench` via
`[lints] workspace = true`) cannot be lowered by an inner
`#[allow(unsafe_code)]`, and editing the lint configuration is outside this
dispatch's MAY-edit envelope. The Task Packet's MAY-edit clause explicitly
permits a script-bench-local dependency in exactly this situation. `memory-stats`
keeps the required `unsafe` FFI inside the dependency and exposes a safe API, so
`rge-script-bench` stays unsafe-free. It is tightly scoped and localised to
`crates/script-bench`; `cargo` locked exactly one new package (`memory-stats
1.2.0`) — its only transitive dep, `windows-sys 0.52.0`, plus `libc`, were
already in the workspace lock. The runtime crates are unaffected.

## Verification Results

- `git status --short --untracked-files=no` → 6 tracked files modified:
  `.gitignore` (pre-existing, unrelated — not touched by this dispatch) plus the
  5 in-scope files (`Cargo.lock`, `crates/script-bench/{BASELINE.md,Cargo.toml,
  METHODOLOGY.md,src/script_host.rs}`).
- `cargo +nightly fmt --check -p rge-script-bench` → exit 0
- `cargo check -p rge-script-bench` → exit 0
- `cargo test -p rge-script-bench --lib --no-fail-fast script_host::tests` →
  9 passed / 0 failed / 1 ignored (19 filtered out). The ignored test is
  `phase_3_memory_soak_one_hour` — it remains `#[ignore]`'d and was NOT run.
- `cargo test -p rge-script-bench --lib script_host::tests::memory_soak_reports_process_memory_metrics -- --nocapture`
  → 1 passed / 0 failed; captured stdout:
  `memory_soak_metrics: cycles=49 samples=51 start_rss_bytes=17256448 end_rss_bytes=18247680 peak_rss_bytes=18247680 start_vss_bytes=1638400 end_vss_bytes=1638400 vss_delta_bytes=0`
  (Windows recorder host — numeric resident / peak working-set and a
  start-to-end delta were produced.)
- `cargo run -q -p rge-tool-architecture-lints -- all` → exit 0
  (10 enforcement PASS including `split-exemption`; snapshot supplementary PASS)
- `git diff --check -- crates/script-bench/src/script_host.rs crates/script-bench/BASELINE.md crates/script-bench/METHODOLOGY.md crates/script-bench/Cargo.toml Cargo.lock`
  → exit 0 (no whitespace errors; only informational CRLF "will be replaced"
  notices, which `git diff --check` does not treat as errors).
- `git diff --stat` (same paths) → `Cargo.lock` +11, `BASELINE.md` +41/-5
  region, `Cargo.toml` +12, `METHODOLOGY.md` +24, `script_host.rs` +221;
  5 files changed, 304 insertions(+), 5 deletions(-).
- The one-hour ignored soak (`phase_3_memory_soak_one_hour -- --ignored`) was
  NOT run — there is no release-readiness justification for it under this
  dispatch, per the Task Packet's Acceptance Criteria.
- Supplementary (not a required gate): `cargo clippy -p rge-script-bench
  --lib --tests` surfaces only pre-existing warnings; the new code adds none.

## Deviations from Task Packet

- **SPLIT-EXEMPTION annotation added to `script_host.rs`.** The required
  additions grow the file past the `split-exemption` lint's 1000-line hard cap.
  The lint is a required verification gate and failed without remediation.
  Remediation options within the single allowed source file were (a) compress
  ~100+ lines of the deliverable, which would gut the documentation and the
  bounded test, or (b) add the lint's own sanctioned `// SPLIT-EXEMPTION:`
  annotation. Splitting the file is forbidden by the Task Packet (no new source
  modules). Option (b) was chosen — it is a comment in an already-MAY-edit file,
  is the smallest practical change, and matches established repo precedent
  (`loft.rs`, `host.rs`). My-side verbosity was still trimmed where readability-
  neutral (multi-line `assert!` calls collapsed). Flagged here for Reviewer
  visibility; otherwise execution stayed within the MAY / MUST NOT envelope.
- **No commit created.** The Task Packet's MAY-commit clause permits one local
  commit, but the dispatch invocation's Protocol rules explicitly instruct "Do
  not commit. Do not push." The direct invocation instruction was honored; all
  changes are left uncommitted in the working tree.

## Open Questions for Reviewer

- `vss_delta_bytes` is reported on the *virtual* footprint (Windows: commit
  charge), matching the preferred field name `vss_delta_bytes`; `peak_rss_bytes`
  is the *resident* peak. Both are documented. Confirm this resident-vs-virtual
  split matches Planner intent (the Task Packet asked for "peak resident/
  process-working-set bytes and a start-to-end memory delta" plus the preferred
  names — this implementation provides both peak RSS and a virtual delta, and
  additionally start/end RSS so a resident delta is derivable).
- On the recorder host `memory-stats` reports Windows `virtual_mem` as
  `PROCESS_MEMORY_COUNTERS::PagefileUsage` (commit charge). The observed values
  are small and stable (`vss_delta_bytes=0` across a bounded run); this is
  honest passthrough of the OS counter, documented as a commit-charge delta —
  not a fabricated or massaged number.

## Worktree State

- Tracked files (modified, uncommitted): `.gitignore` (pre-existing, unrelated),
  `Cargo.lock`, `crates/script-bench/BASELINE.md`,
  `crates/script-bench/Cargo.toml`, `crates/script-bench/METHODOLOGY.md`,
  `crates/script-bench/src/script_host.rs`.
- Untracked items: this EXEC packet
  (`ai_handoffs/POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001_EXEC_2026-05-16_10-51-54+0300.md`)
  is the only file added by this dispatch. Numerous pre-existing untracked
  root-level cross-AI handoff files and automation artifacts remain present and
  were not touched.
- Branch: main
- Last commit: `6e15c5f` chore: ignore .claude/worktrees/ (Claude Code local state)

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-SCRIPT-BENCH-RSS-SOAK-METRICS-001
AUTHOR: Executor / Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
