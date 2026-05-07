# RECOVERY_MODEL

| Companion to | PLAN.md §1.13 (failure-class taxonomy + per-subsystem table); ADR-102 (failure containment model with 5 classes — formal ADR pending; PLAN §1.13 + this doc are canonical until trigger fires, mirroring the ADR-099/§0.3.1 + ADR-101/§1.14 pattern) |
|---|---|
| Status | Active v0; 23 of original 81 rollout-debt exemptions cleared / 58 remain (per Status.md 2026-05-09 line "23 of original 81 rollout-debt exemptions cleared cumulatively"); enforcement via `failure-class` architecture lint that scans every Tier-1 + Tier-2 `src/lib.rs` for `//! Failure class: <kind>` |
| Audience | Subsystem authors landing first real implementation (declaration triggers exemption removal); reviewers verifying recovery-path coverage; orchestrator authors mapping `Diagnostic::failure_class` onto recovery actions |
| Sibling doc | `KERNEL_DIAGNOSTICS.md` — `FailureClass` enum carried by every `Diagnostic`; `KERNEL_PLUGIN_HOST_LIFECYCLE.md` — plugin-fatal isolation enforcement (catch_unwind shield + leak-detection); `EXECUTION_DOMAINS.md` — per-domain failure-class implications |
| Reference impls | `tools/architecture-lints/src/failure_class.rs` (the lint itself; 240L) · `tools/architecture-lints/exemptions.toml` (60 entries; 58 failure-class rollout-debt + 1 graph-foundation false-positive + 1 reserved) · 23 `src/lib.rs` files carrying `//! Failure class: <kind>` declarations · `kernel/diagnostics/src/failure_class.rs` (the closed-set enum) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the recovery model: the five failure classes, where each is declared, how the lint enforces it, and how each class maps onto orchestrator recovery actions. Per-subsystem rationale lives in each subsystem's sibling §18 doc.

## 1. Why a substrate

Without a fixed taxonomy, every subsystem invents its own "what to do when it fails" story. Plugin A's `RuntimeFault` would mean "log and continue"; plugin B's `RuntimeFault` would mean "tear down the editor". The orchestrator would have to encode N per-subsystem recovery policies. CI couldn't reason about "is this a recoverable error" without a registry of subsystem-specific judgements.

PLAN §1.13 commits to **five canonical failure classes** with two enforcement points:

- **Per-crate declaration via `//! Failure class: <kind>` in `src/lib.rs`.** Documents the crate's worst-case recovery story; lint-enforced.
- **Per-event tagging via `Diagnostic::failure_class: Option<FailureClass>`.** Carried by emitted diagnostics; consumers branch on the variant.

The classes are ordered by escalation: a `Recoverable` failure leaves the engine fully online; a `KernelFatal` failure tears it down. Recovery paths get richer (and more disruptive) as the class escalates.

## 2. The five classes

Lives at `kernel/diagnostics/src/failure_class.rs`:

```rust
pub enum FailureClass {
    Recoverable,
    SnapshotRecoverable,
    PluginFatal,
    SessionFatal,
    KernelFatal,
}
```

Per PLAN §1.13, each class maps to a fixed response:

| Class | Examples | Response |
|---|---|---|
| **Recoverable** | tessellation crash on bad input, shader compile timeout, plugin panic during init, expr-wasm parse error | Isolate; surface diagnostic; subsystem stays online with reduced capability |
| **Snapshot-recoverable** | cad-core rebuild partial failure, hot-reload migration failure, projection invalidation cycle, schema-divergence-on-load | Rollback to last known-good checkpoint; user gets undo entry surface |
| **Plugin-fatal** | sandbox escape attempt, repeated panic, persistent resource quota breach, manifest tamper | Unload plugin; revoke trust if Verified-Community; surface diagnostic; editor continues |
| **Session-fatal** | asset DB corruption, schema version unrecoverable, cad-core graph corruption, render device lost beyond recovery | Save recovery dump; restart editor; offer to reload from autosave |
| **Kernel-fatal** | OOM, deadlock in core scheduler, kernel ABI mismatch detected, audit-ledger corruption | Crash report; terminate; user restarts from clean state |

The `label()` method returns the canonical lower-case kebab string (`recoverable` / `snapshot-recoverable` / `plugin-fatal` / `session-fatal` / `kernel-fatal`) that the lint matches against the `//! Failure class:` doc-comment text.

## 3. Per-crate declaration via lib.rs doc-comment

Every Tier-1 (`kernel/*`) and Tier-2 (`crates/*`) crate's `src/lib.rs` MUST contain at least one `//! Failure class: <kind>` line. Multiple values may appear comma-separated:

```rust
//! Failure class: recoverable, snapshot-recoverable
```

The declaration documents the **crate's worst-case recovery story** — the highest-escalation class any of its operations might raise. Individual `Diagnostic`s within the crate may carry any class via `Diagnostic::with_failure_class(...)`; the lib-level declaration is the bound on what the crate as a whole can produce.

Convention: pick the class that matches the most-disruptive recovery path. `kernel/diagnostics` is `recoverable` because its emit operations can't fail catastrophically (it's a routing layer); `kernel/audit-ledger` is `kernel-fatal` because checksum-fail + integrity-violation paths route there per PLAN §1.13 line 573.

## 4. The 23 declared crates (post-2026-05-09)

The 23 `src/lib.rs` files currently carrying `//! Failure class: <kind>`:

| Crate | Class | Rationale |
|---|---|---|
| `kernel/types` | recoverable | Typed-IO substrate; serde failures surface as `Result`. See `KERNEL_TYPES.md` §11. |
| `kernel/ecs` | recoverable | World state is part of PIE; failures isolate per system. |
| `kernel/schedule` | kernel-fatal | Scheduler-detected deadlock = kernel-fatal per PLAN §1.13 line 572. See `KERNEL_SCHEDULE.md` §10. |
| `kernel/diagnostics` | recoverable | Routing layer; sink emit infallible. See `KERNEL_DIAGNOSTICS.md` §11. |
| `kernel/plugin-host` | plugin-fatal | Plugin-fatal isolation; catch_unwind shield. See `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §13. |
| `kernel/asset` | snapshot-recoverable | Registry corruption recoverable via re-load + dep-graph replay. See `KERNEL_ASSET.md`. |
| `kernel/audit-ledger` | kernel-fatal | Checksum failure = kernel-fatal per PLAN §1.13 line 573. See `KERNEL_AUDIT_LEDGER.md` §9. |
| `kernel/app` | recoverable | Frame-loop driver; per-phase failures isolate. |
| `kernel/graph-foundation` | snapshot-recoverable | Graph corruption recoverable via snapshot restore. |
| `kernel/events` | recoverable | Channel-overflow = warning; advance_frame infallible. |
| `crates/cad-core` | snapshot-recoverable | Operator failures rollback via `CadGraph::rollback`. See `CAD_CORE_MODEL.md` §11. |
| `crates/cad-projection` | snapshot-recoverable | Projection invalidation cycle (>1000 iterations) per PLAN §1.13. |
| `crates/editor-actions` | snapshot-recoverable | Action-failure rolls back via Command Bus undo path. |
| `crates/editor-state` | recoverable | Coordination state (selection / hover / modal); failures surface as warnings. |
| `crates/gfx` | recoverable | GPU init / pipeline-build failure; software fallback. |
| `crates/physics` | snapshot-recoverable | Physics state participates in PIE; snapshot rollback canonical. |
| `crates/audio` | recoverable | Audio failures transient (`ManagerError::UnknownClip` etc.); no PIE participation. |
| `crates/script-host` | plugin-fatal | WASM trap = plugin-fatal Tier-3 / recoverable Tier-2 (PLAN §1.13 row). |
| `crates/components-render` | recoverable | Component impls; ECS-isolated. |
| `crates/components-audio` | recoverable | Component impls; ECS-isolated. |
| `crates/components-animation` | recoverable | Component impls; ECS-isolated. |
| `crates/components-identity` | recoverable | Component impls; ECS-isolated. |
| `crates/ui-theme` | recoverable | Theme-load diagnostics; non-fatal. |

## 5. The `failure-class` architecture lint

Lives at `tools/architecture-lints/src/failure_class.rs` (240L). Run via `cargo run -p rge-tool-architecture-lints -- failure-class` (or `... -- all` to run all 9 lints).

### Algorithm

1. Walk every workspace member via `cargo metadata`.
2. Skip non-Tier-1 / non-Tier-2 crates (the lint's `classify` helper returns Tier::One / Tier::Two for `kernel/*` and `crates/*`).
3. For each in-scope crate, locate `src/lib.rs` (prefers the `lib` cargo target's `src_path`; falls back to manifest sibling).
4. Check `tools/architecture-lints/exemptions.toml` for an `[[exemption]]` whose `lint = "failure-class"` and `file = "<manifest-rel-path>"`.
5. If exempt — skip. Otherwise scan every `//!` line for the prefix `Failure class:` and validate each comma-separated value against the closed set.
6. Surface violations as `Violation { file, line, message }`. Exit non-zero if any violation found.

### Closed-set values (case-sensitive)

```rust
const VALID_CLASSES: &[&str] = &[
    "recoverable",
    "snapshot-recoverable",
    "plugin-fatal",
    "session-fatal",
    "kernel-fatal",
];
```

The keyword `Failure class` matches case-sensitively; the lint's `parses_extra_whitespace` unit test pins permissive whitespace handling around the colon and around values, while `wrong_case_keyword_not_parsed` rejects lowercase `failure class`.

## 6. The rollout-debt frame (Phase 1.x)

The lint was introduced 2026-05-05 as part of Phase 0.2 architecture enforcement. At rollout time, all 81 Tier-1 + Tier-2 crates lacked the declaration. Rather than block landing the lint behind 81 simultaneous edits, the orchestrator added per-crate exemptions to `exemptions.toml`:

```toml
[[exemption]]
lint = "failure-class"
file = "crates/<name>/Cargo.toml"
reason = "Phase 1.x rollout debt - declaration added when crate gets first real implementation per IMPLEMENTATION.md."
```

The deal: when a crate gets its first real implementation (per IMPLEMENTATION.md phase order), the orchestrator adds the `//! Failure class: <kind>` line and removes the exemption in the same dispatch. The lint then enforces the declaration on that crate immediately.

**Cumulative progress** (per Status.md 2026-05-09): 23 of original 81 cleared / 58 remain. Cleared crates carry their declaration; the remaining 58 crates are stubs / tier-2 placeholders waiting for Phase 4-Foundation (Phase 4-Geometry / Phase 4-Authoring) implementations.

The exemption removal is mechanical: each cleared crate landed its first plugin-canary, gltf-importer, animation-clip-loader, etc. — the implementation closure that satisfies the audit-1 "declaration added when crate gets first real implementation" condition. See Status.md "Physics + Audio failure-class exemptions cleared" (2026-05-08) for the canonical recipe.

## 7. Per-event tagging via `Diagnostic::failure_class`

Every emitted `Diagnostic` may carry an `Option<FailureClass>`:

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub failure_class: Option<FailureClass>,
    pub span: Span,
    pub message: String,
    pub suggestion: Option<Suggestion>,
}
```

The orchestrator branches on the class:

- `None` — informational; no recovery action.
- `Some(Recoverable)` — log + continue; isolate the failing operation.
- `Some(SnapshotRecoverable)` — invoke snapshot-restore (cad-core `rollback`, PIE snapshot replay, etc.).
- `Some(PluginFatal)` — unload the named plugin; revoke trust if Tier-3 and Verified-Community.
- `Some(SessionFatal)` — save recovery dump; restart editor; offer reload-from-autosave.
- `Some(KernelFatal)` — crash report; terminate; user restarts from clean state.

The substrate (`kernel/diagnostics`) does NOT enforce recovery semantics — it only carries the tag. Routing is the orchestrator's responsibility (see `KERNEL_DIAGNOSTICS.md` §4).

## 8. Mapping `PluginError` variants to failure classes

Per ADR-114 §"PluginError variant policy" and `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §7, the host's auto-emit policy maps `PluginError` variants onto severity + an implicit failure-class story:

| `PluginError` variant | Severity | Implicit class | Notes |
|---|---|---|---|
| `InitFailed { reason }` | Error | plugin-fatal | Plugin is marked `Failed`; host isolates per PLAN §1.13. |
| `RuntimeFault { reason }` | Error | plugin-fatal | Genuine plugin-side failure; isolation only. |
| `Panic { phase, payload }` | Error | plugin-fatal | Host-classified, host-recovered via catch_unwind; resources held by the panicker may be leaked but the engine survives. |
| `ContractViolation { resource_type }` | Warning | (caller bug — no class) | Caller misconfiguration, not plugin failure; not a recovery event. |
| `ShutdownFailed { reason }` (lifecycle-driven) | Error | plugin-fatal | Real failure during teardown. |
| `ShutdownFailed { reason }` (host-initiated unregister) | Warning | (host-driven — no class) | Host explicitly asked for unregister; teardown imperfection is non-fatal. |

The host's `catch_unwind` shield + leak-detection diff are the **mechanical enforcement of plugin-fatal isolation**: a plugin failure marks the plugin's record `Failed` but does NOT propagate to other plugins. Cross-ref `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §10.

## 9. Concrete subsystem examples

### snapshot-recoverable: `crates/physics`

Physics state participates in PIE per `crates/physics/src/lib.rs` doc-comment. Forces, impulses, joint motors are recorded into `PhysicsInputLedger` per-tick; on snapshot restore the ledger replays into a fresh Rapier3D world. Failure class is snapshot-recoverable because:

- Rapier internal panics surface as recoverable per PLAN §1.13 row "physics | Rapier internal panic | recoverable (entity quarantined)".
- Replay-Stable v1.0 (PLAN §1.6.8 same-machine gameplay) implies snapshot-based recovery.
- Physics state is part of PIE so snapshot rollback is the canonical recovery path.

### recoverable: `crates/audio`

Audio failures are transient — `ManagerError::UnknownClip` is exercised by the canary's `RuntimeFault` path; audio device loss surfaces as a recoverable warning. Audio state does NOT participate in PIE (it's transient: a paused-then-resumed editor restarts the audio mixer from scratch). Failure class matches `crates/gfx` / `kernel/diagnostics` / `kernel/ecs`.

### kernel-fatal: `kernel/audit-ledger`

PLAN §1.13 line 573 specifically promotes "audit-ledger checksum fail" to kernel-fatal. The lib.rs module-doc explains the nuance: ledger corruption (hash collision detected, cursor out of range) at the API level is `LedgerError` and caller-recoverable, but **integrity-violation paths** route to kernel-fatal. Recovery is snapshot restore. The class scopes the *integrity-violation worst case*; benign operational errors stay caller-recoverable. See `KERNEL_AUDIT_LEDGER.md` §9.

### snapshot-recoverable: `crates/cad-core`

Operator-evaluation failures (`OpError::WrongArity`, `OpError::EmptyResult`, csgrs-wrapped panic via `OpError::InvalidParameter`) surface inside a `begin_operation` / `commit | rollback` bracket. The transactional discipline ensures the canonical recovery: a caller running `cad.begin_operation(); evaluate(); commit()` who hits an `OpError` calls `cad.rollback()` instead, restoring the pre-operation graph state. See `CAD_CORE_MODEL.md` §11.

### kernel-fatal: `kernel/schedule`

PLAN §1.13 line 572: "scheduler deadlock = kernel-fatal". The class is **scoped**: build-time errors (`DuplicateSystem`, `Cycle`, `MissingDependency`, `NotBuilt`) are caller-recoverable — fix the registration and rebuild. The kernel-fatal escalation applies specifically to `run()`-time invariants (deadlock; system panic the supervisor cannot quarantine). See `KERNEL_SCHEDULE.md` §10.

## 10. Cross-ref auto-emit policy

The auto-emit policy that routes `PluginError` variants onto `Diagnostic::Severity` (and implicitly onto failure-class via the variant's class) lives at `kernel/plugin-host/src/host.rs` lines 666-678. The policy is documented in `KERNEL_DIAGNOSTICS.md` §9 (severity table) + `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §7 (host-side mechanics) + `PLUGIN_API.md` §3 (plugin-author surface).

The discrimination is regression-pinned by `tick_all_emits_warning_for_contract_violation` (host.rs line 1675) and `unregister_emits_warning_on_shutdown_failure` (host.rs line 1726). The auto-emit policy is the orchestrator's first hop: the host emits a structured diagnostic; downstream consumers (editor UI, CI, replay logs) branch on `Diagnostic::failure_class` to invoke the right recovery action.

## 11. CI gate + fault injection

PLAN §1.13 last line: "CI verifies failure-class declarations are present for every Tier-1 + Tier-2 crate. Recovery paths tested via fault injection on golden test projects."

The first half is implemented today: the `failure-class` architecture lint runs in CI's `architecture.yml` workflow and exits non-zero on any missing declaration not matched by an exemption. The second half (fault injection on golden test projects) is Phase 4-Polish work — the W21 golden-project fixtures are landed but the fault-injection test harness has not been built yet. Tracked in Status.md "remaining audit-debt" under future-CI-work.

## 12. Source / spec inconsistencies

- **Brief stated "ADR-102" exists as a file**; source-truth: `docs/adr/` contains only `ADR-098`, `ADR-104`, `ADR-112`, `ADR-114`. ADR-102 is referenced in PLAN §1.13 line 577 ("ADR-102. Companion: `RGE/RECOVERY_MODEL.md`") but has not been created as a formal ADR file. This doc adopts the same pattern as `GRAPH_FOUNDATION.md` (companion to ADR-101 — also pending; superseded by §18 doc until trigger fires) and `KERNEL_TYPES.md` (companion to PLAN §1.2.4 with no ADR file). The header table reflects this.
- **Brief stated "23 of 81 cleared / 58 remain"**; source-truth: `tools/architecture-lints/exemptions.toml` carries 58 `lint = "failure-class"` entries (verified via `grep -c 'lint = "failure-class"'`). The 23-of-81 figure is therefore consistent (81 total - 58 remain = 23 cleared). Status.md confirms the cumulative count.
- **Brief stated "23 of 81 rollout-debt exemptions cleared"**; source-truth grep on lib.rs files found exactly 23 `//! Failure class:` declarations across the workspace. The two numbers are consistent (declarations added = exemptions cleared).
- **Brief stated `kernel/audit-ledger` is "kernel-fatal-on-integrity-violation but recoverable-on-benign-corruption per its docs"**; source-truth: `kernel/audit-ledger/src/lib.rs` line 1 declares only `//! Failure class: kernel-fatal` (not a comma-separated multi-value). The lib-level recovery-model paragraph captures the nuance via prose (cursor out of range / benign hash-collision paths are caller-recoverable via `LedgerError`), but the doc-comment declaration is single-class. See `KERNEL_AUDIT_LEDGER.md` §9 for the full rationale.

## 13. References

- **PLAN.md §1.13** — failure containment model; the per-subsystem table (line 555-573) is the canonical mapping.
- **PLAN.md §1.6.8** — Replay-Stable v1.0 determinism mode; snapshot-recoverable is the canonical recovery for PIE-participating subsystems.
- **ADR-102** (deferred) — failure containment model formal ADR; pending until trigger fires (mirrors ADR-099 / ADR-101 deferred-pattern).
- **ADR-114** — `PluginError` variant policy; auto-emit Severity mapping; host-side classification rationale.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; `FailureClass` enum, severity table, auto-emit consumer surface.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; plugin-fatal isolation enforcement (catch_unwind shield, leak-detection diff).
- **`EXECUTION_DOMAINS.md`** — sibling §18 doc; per-domain failure-class implications (CPU gameplay / GPU shading / GPU compute / Expression).
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; kernel-fatal-on-integrity-violation rationale.
- **`KERNEL_SCHEDULE.md`** — sibling §18 doc; scoped kernel-fatal (build-time errors recoverable; run-time deadlock kernel-fatal).
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; snapshot-recoverable transactional rollback recipe.
- **`KERNEL_TYPES.md`** — sibling §18 doc; recoverable substrate; `SchemaMismatch` is the snapshot-recoverable specific path.
- **`tools/architecture-lints/src/failure_class.rs`** — the lint itself; algorithm + closed-set validation + parse helpers.
- **`tools/architecture-lints/exemptions.toml`** — the 58 remaining failure-class rollout-debt entries; canonical removal-recipe per Status.md "Physics + Audio failure-class exemptions cleared" (2026-05-08).
- **`kernel/diagnostics/src/failure_class.rs`** — the closed-set `FailureClass` enum + `label()` method that matches the lint's parse format.
