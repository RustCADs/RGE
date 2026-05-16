# Execution Report

DISPATCH_ID: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_15-24-57+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_TASK_2026-05-14_03-37-07+0300.md — TASK consumed.
- ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_CLOSEOUT_2026-05-14_15-19-49+0300.md — Job 6 dependency, STATUS: CLOSED.
- ai_handoffs/MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP_CLOSEOUT_2026-05-14_15-19-50+0300.md — Job 7 formally SKIPPED/CLOSED.
- ai_handoffs/MAIN-ORDERED-SERIAL_STATE_2026-05-14_15-19-51+0300.md — release signal: Job 8 RELEASED NOW; Jobs 9-10 HELD; stop after EXEC with `NEXT_ROLE: REVIEWER_AI`.
- crates/script-bench/src/script_host.rs:845 — ignored Phase 3.4 memory-soak test located.
- crates/script-bench/BASELINE.md — Phase 3.3 / 3.4 formal-gate sections + 1-hour-soak-run-2026-05-12 section.
- plans/IMPLEMENTATION.md — lines 308 (Phase 3 release-readiness criteria) + 316-318 (exit-criteria with CLOSED markers).
- ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_EXEC_2026-05-14_15-24-57+0300.md — this packet.
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_TASK_2026-05-14_03-37-07+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. The Planner's TASK packet's footer `NEXT_ROLE: EXECUTOR_AI` plus the serial-state marker `MAIN-ORDERED-SERIAL_STATE_2026-05-14_15-19-51+0300.md` plus `OPENAItoCLAUDE_2026-05-14_15-19-52+0300.md` ("Execute Job 8 only. It is read-only. [...] do not run the one-hour soak") route directly to the Executor under v2 Rule 7 from `d017a35`. Executor proceeded to execution.

## Prior-Jobs Closure Verification (TASK Halt Condition)

All prior jobs are closed or formally skipped: Jobs 1-4 + 6 CLOSED; Jobs 5 + 7 SKIPPED/CLOSED (both followed Claude's audit recommendation; Planner accepted). The "Prior jobs are not closed or formally skipped" halt condition is NOT TRIPPED.

## What I Changed

### Source
- None.

### Tests
- None.

### Docs
- `ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_EXEC_2026-05-14_15-24-57+0300.md`: created this readiness execution packet (the only filesystem change).

## Per-File Summary

Read-only audit. Zero tracked-file edits. **No one-hour soak run** (TASK explicit constraint). Single filesystem effect: this new untracked EXEC packet under `ai_handoffs/`.

## Verification Results

Per the TASK's `Verification Gates` section:

### Gate 1: Search for ignored Phase 3 soak tests

```
grep -rnE "#\[ignore" crates/script-host crates/script-bench
```

→ **Exactly one match**:

```
crates/script-bench/src/script_host.rs:845:
    #[ignore = "Phase 3.4 memory-soak gate runs for one hour; run explicitly when validating release readiness"]
    fn phase_3_memory_soak_one_hour() {
        let bench = ScriptHostBench::new().expect("compile fixtures");
        let report = bench
            .memory_soak(MemorySoakConfig::formal())
            .expect("one-hour memory soak");
        [...]
        assert!(report.elapsed >= FORMAL_MEMORY_SOAK_DURATION);
        assert!(report.cycles > 0);
        assert_eq!(report.restored_components, report.cycles as usize * report.entity_count);
    }
```

The test is the ONLY `#[ignore]` in `crates/script-host` + `crates/script-bench`. Its annotation explicitly says: "Phase 3.4 memory-soak gate runs for one hour; run explicitly when validating release readiness."

### Gate 2: Search for Phase 3 gate status in Status / HANDOFF / BASELINE / docs

**`plans/IMPLEMENTATION.md` exit-criteria (4 of 4 CLOSED)**:

| # | Criterion | Status |
|---|---|---|
| 1 | Hot-reload p95 < 100ms on a 1000-entity scene | **CLOSED 2026-05-11 + re-validated 2026-05-12** on recorder host (Windows 11 / x86_64 / cargo 1.94.1 / wasmtime 44.0.1; min-of-3 p95 = 0.796 ms 2026-05-11; single-run p95 = 0.818 ms 2026-05-12; both << 100 ms gate). Test: `phase3_hot_reload_1000_entities_100_cycles` via `formal_100_cycle_preservation_gate_uses_1000_entities` |
| 2 | ECS iteration via WASM ≤ 1.5× native Rust | **CLOSED 2026-05-11 + re-validated 2026-05-12** on recorder host (bulk-path substrate; 2026-05-11 ratio = 1.21× native ~81 µs / wasm ~98 µs; 2026-05-12 ratio = 1.34× native ~67.93 µs / wasm ~90.82 µs; both ≤ 1.5× gate; +10.7% drift flagged but in-gate; substrate UNCHANGED). Test: `script_host::tests::phase_3_4_ecs_via_wasm_ratio_meets_gate` |
| 3 | **1-hour session without memory leak** | **CLOSED 2026-05-12** on recorder host: `cargo test -p rge-script-bench --release --lib script_host::tests::phase_3_memory_soak_one_hour -- --ignored --nocapture` exits 0 in **exactly 3600.00 s wall-clock**; assertions `report.elapsed >= 1 hour` + `report.cycles > 0` + `report.restored_components == cycles * entity_count` ALL HELD; estimated ~4.4M cycles at re-validated 0.818 ms/cycle Phase 3.3 p95; no panic / no OOM / no hang. CONSTRAINED-CERTIFIED on recorder host only. Implicit "no memory leak" claim (no explicit `peak_rss` / `vss_delta` capture — flagged as future harness improvement) |
| 4 | Component data preserved across 100 hot-reload cycles | **CLOSED 2026-05-11 + re-validated 2026-05-12 + re-asserted 2026-05-12-soak** on recorder host: assertion `restored_components == cycles * entity_count` HELD in (a) the 100-cycle formal gate at 1000 entities, (b) the re-validation re-run, AND (c) the 1-hour soak's ~4.4M cycle run. Test: `crates/script-bench/src/script_host.rs::hot_reload_preservation` |

### Gate 3: `git status --short --untracked-files=no`

→ empty output (tracked tree clean). No in-flight edits.

## Critical Finding — The Soak Has Already Been Run

The 1-hour soak that Job 8 is staged to consider is **ALREADY RUN AND PASSING** as of **2026-05-12**:

- Test: `crates/script-bench/src/script_host.rs::phase_3_memory_soak_one_hour` (the `#[ignore]` test at L845 — annotation says "run explicitly when validating release readiness")
- Invocation: `cargo test -p rge-script-bench --release --lib script_host::tests::phase_3_memory_soak_one_hour -- --ignored --nocapture`
- Wall-clock: exactly 3600.00 s
- Cycles: ~4.4M (estimated at the re-validated 0.818 ms/cycle Phase 3.3 p95)
- Assertions: all three HELD (`elapsed >= 1 hour` + `cycles > 0` + `restored_components == cycles * entity_count`)
- Failure modes: none observed (no panic / no OOM / no hang)
- Result recorded in: `plans/IMPLEMENTATION.md` exit-criterion #3 + `crates/script-bench/BASELINE.md` "Formal 1-hour memory soak (Phase 3.4 exit criterion #3) — RUN 2026-05-12" section

The Phase 3 release-readiness scoreboard is **fully closed**:

```
Phase 3 exit criteria:
  [✓] 1. Hot-reload p95 < 100ms                     CLOSED 2026-05-11 + 2026-05-12
  [✓] 2. ECS iteration via WASM ≤ 1.5× native        CLOSED 2026-05-11 + 2026-05-12
  [✓] 3. 1-hour session without memory leak          CLOSED 2026-05-12 (soak run)
  [✓] 4. Component data preserved 100 hot-reload     CLOSED 2026-05-11 + 2x re-asserted
```

The `#[ignore]` annotation on the soak test is the standard "don't run this in every `cargo test` invocation" guard — it does NOT mean the soak is pending or unrun. The annotation directs the operator to "run explicitly when validating release readiness," and that explicit run HAS happened.

## Halt-Condition Checks

| Halt condition | Status |
|---|---|
| Prior jobs are not closed or formally skipped | NOT TRIPPED — Jobs 1-4 + 6 CLOSED; Jobs 5 + 7 SKIPPED/CLOSED |
| The Phase 3 docs and code disagree in a way that needs Planner correction | NOT TRIPPED — docs (`plans/IMPLEMENTATION.md` exit criteria 1-4 all marked CLOSED with specific harness names + run timestamps) and code (the `#[ignore]` test exists at `crates/script-bench/src/script_host.rs:845` with annotations + assertions consistent with the recorded run) are aligned. The annotation says "run explicitly when validating release readiness"; the docs record that the explicit run happened on 2026-05-12 |
| Running tests would exceed the intended read-only readiness scope | NOT TRIPPED — no tests were re-run by this audit; the audit reads the docs-recorded prior gate-passing state plus the test definition for shape verification |

## Deliverables

### Deliverable 1: Current Phase 3.3/3.4 gate status from code and docs

**All four Phase 3 exit criteria are CLOSED.** See the table in Gate 2 above:
- Phase 3.3 hot-reload p95 < 100ms: PASS (min-of-3 p95 = 0.796 ms; ~125× under gate)
- Phase 3.4 ECS-via-WASM ratio ≤ 1.5×: PASS (1.21× and 1.34× on the two runs; both in-gate; +10.7% drift flagged but well-within gate)
- Phase 3.4 1-hour memory soak: PASS (3600.00 s wall-clock; ~4.4M cycles; no leak)
- Phase 3 component preservation: PASS (validated 3× including inside the 4.4M-cycle soak)

### Deliverable 2: Locate the ignored one-hour soak test or confirm it is absent

**Located.** `crates/script-bench/src/script_host.rs:845`:

```rust
#[test]
#[ignore = "Phase 3.4 memory-soak gate runs for one hour; run explicitly when validating release readiness"]
fn phase_3_memory_soak_one_hour() {
    let bench = ScriptHostBench::new().expect("compile fixtures");
    let report = bench
        .memory_soak(MemorySoakConfig::formal())
        .expect("one-hour memory soak");
    /* ... */
    assert!(report.elapsed >= FORMAL_MEMORY_SOAK_DURATION);
    assert!(report.cycles > 0);
    assert_eq!(report.restored_components, report.cycles as usize * report.entity_count);
}
```

The test is the only `#[ignore]` across `crates/script-host` + `crates/script-bench`. It has been **run explicitly** on 2026-05-12 with PASS result; see Critical Finding above.

### Deliverable 3: Recommendation — run / defer / convert

**Recommendation: DEFER the soak indefinitely. Do NOT re-run it now.**

Reasoning:

1. **The soak has already been run with PASS result**. Re-running it would not produce new signal — the 2026-05-12 result is recorded in both `plans/IMPLEMENTATION.md` and `crates/script-bench/BASELINE.md`.
2. **The bulk-path substrate is UNCHANGED since 2026-05-12** (per `IMPLEMENTATION.md` Phase 3.4 line: "substrate UNCHANGED"). The 1-hour soak's load-bearing claim (no memory leak across ~4.4M cycles) remains valid for the current substrate.
3. **No substrate-affecting change has landed in `crates/script-host` or `crates/script-bench` in this session**. The 4 local-only commits (`03d3f05`, `2b64241`, `d017a35`, `bcfe902`) touch `crates/gfx/tests/`, `ai_handoffs/`, and project docs only — none touches the script substrate.
4. **The 1-hour cost** (3600 s wall-clock + recorder-host occupancy) is not justified for a re-run that would only re-confirm the existing result.

**However**, the soak SHOULD be re-run explicitly under the following triggers:

| Trigger | When |
|---|---|
| **Material change to `crates/script-host/**` or `crates/script-bench/**`** | If any future dispatch modifies the WASM ABI, the host-bridge surface (`rge.ecs::*`), the bulk-path substrate, or the memory-soak harness itself — the soak should be re-run before the change is considered "release-ready" |
| **Release-readiness review** | If/when a formal v0 release is cut, the soak should be re-run on the release-candidate substrate as a fresh certification (not just leaning on the 2026-05-12 cached result) |
| **Wasmtime version bump** | The current run was at wasmtime 44.0.1; a major-version bump should re-run the soak |
| **`peak_rss` / `vss_delta` harness improvement** | Per the BASELINE.md note, the current soak's "no memory leak" claim is implicit (no explicit RSS/VSS capture). If a future dispatch adds explicit memory-pressure assertions to the harness, the soak should be re-run with the improved instrumentation to convert the implicit claim into an explicit one |

**Format**: explicit human-approved command, NOT a `#[cfg(test)]` default-run. The current `#[ignore]` annotation correctly reflects the "run on demand" posture. **Do NOT convert to CI nightly** unless and until (a) explicit RSS/VSS capture is added AND (b) a sustained-CI infrastructure for RGE is established (none currently exists). Adding it to CI prematurely would gate every push on a 1-hour run.

### Deliverable 4: Job 9 recommendation

**Job 9 can proceed unchanged.**

Job 9 (`MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT`) is described in the queue manifest as "Read-only kernel cavity/stub state audit." It is **orthogonal to Phase 3 / script-host concerns**:
- Job 9 audits `kernel/**` (the kernel cavity / stub state)
- Job 8's findings (Phase 3 fully closed) do not invalidate any assumption Job 9 would need
- Job 9 is read-only and short-running; consistent with the queue's serial-rule cadence

**No reconsideration required.** Job 9 can be released by the next serial state marker after this Job 8's CLOSEOUT.

## Deviations from Task Packet

None. Execution stayed strictly within the TASK scope:
- Exactly one new file produced (this EXEC packet).
- Zero tracked-file edits.
- Zero edits to Status.md / HANDOFF.md / change.md / source / test / Cargo / ADR / lint / protocol-doc / template (TASK MUST NOT envelope honored).
- **No one-hour soak run** (TASK Constraints / Non-Goals: "Do not run a one-hour test" — honored absolutely).
- No commit, no push, no expensive-test re-run.

## Open Questions for Reviewer / Planner

- **`peak_rss` / `vss_delta` harness improvement**: `crates/script-bench/BASELINE.md` flags the lack of explicit RSS/VSS capture as a "future harness improvement." This is the genuine outstanding pressure point for Phase 3 release-readiness — the 1-hour soak's "no memory leak" claim is implicit (assertion-survival over 4.4M cycles + no OOM/hang) rather than explicit (peak RSS bounded; VSS delta bounded). Reviewer/Planner may consider whether to spawn a fresh substrate dispatch to add this instrumentation. **NOT recommended as a queue follow-up via Job 9 or 10** — that would expand scope outside the queue manifest's "kernel cavity audit" + "roadmap consolidate" remits. If genuinely needed, it's a separate `script-bench` improvement dispatch.
- **Wasmtime version drift watch**: the current soak certified on wasmtime 44.0.1. Any future `Cargo.lock` update bumping wasmtime should trigger a re-soak.
- **Drift in the 3.4 ECS-via-WASM ratio**: `IMPLEMENTATION.md:317` records a +10.7% drift between the 2026-05-11 run (1.21×) and 2026-05-12 re-validation (1.34×) — both in-gate but the drift sits "outside ±5% noise band, in-gate." Reviewer/Planner may consider whether this is single-run noise (most likely) or substrate-drift (worth investigation). NOT a Job 8 deliverable; just observed.

## Job 9 Proceed Recommendation

**Recommended: PROCEED unchanged.**

Job 9 (`MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT`) is read-only, orthogonal to Phase 3, and unaffected by Job 8's findings. Per the queue's serial rule, Job 9 stays HELD inside this Job 8 dispatch — the controller will release Job 9 after Codex's CLOSEOUT on this Job 8 EXEC.

Executor will NOT start Job 9 from inside this Job 8. Per the serial state marker:

> Do not start Job 9 from inside Job 8.

## Worktree State

- Tracked files: clean (verified by empty `git status --short --untracked-files=no`).
- New untracked items from this turn: 1 (this EXEC packet).
- Branch: `main`.
- HEAD: `bcfe902 docs(status): MAIN-ORDERED-JOB-003 - queue status reconciliation`.
- Local main is **4 commits ahead** of `origin/main` (`03d3f05` + `2b64241` + `d017a35` + `bcfe902`); none pushed. Unchanged by this Job 8 read-only audit.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
