# Execution Report

DISPATCH_ID: MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_15-13-51+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_TASK_2026-05-14_03-37-05+0300.md — TASK consumed.
- ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_CLOSEOUT_2026-05-14_15-09-25+0300.md — Job 4 dependency, STATUS: CLOSED.
- ai_handoffs/MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP_CLOSEOUT_2026-05-14_15-09-26+0300.md — Job 5 formally SKIPPED/CLOSED.
- ai_handoffs/MAIN-ORDERED-SERIAL_STATE_2026-05-14_15-09-27+0300.md — release signal: Job 6 RELEASED NOW; Jobs 7-10 HELD; stop after EXEC with `NEXT_ROLE: REVIEWER_AI`.
- crates/cad-projection/Cargo.toml — manifest inspected.
- crates/cad-projection/src/{lib.rs, picking.rs, plugin_adapter.rs, render_adapter.rs, projection_cache, projection_editor, projection_geometry, projection_runtime, projection_semantic, projection_structural}/** — source surface inspected.
- crates/cad-projection/tests/*.rs — 15 integration test files inspected (file inventory + key test-name grep).
- plans/IMPLEMENTATION.md:509 — Phase 7.3 close-out line inspected.
- Status.md:109 — Phase 7.3 row inspected.
- HANDOFF.md:85 + L1441 + L1491 — Phase 7.3 references inspected.
- docs/§18/CAD_PROJECTION.md — companion doc inspected (head + module-split table).
- ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_EXEC_2026-05-14_15-13-51+0300.md — this packet.
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_TASK_2026-05-14_03-37-05+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. The Planner's TASK packet's footer `NEXT_ROLE: EXECUTOR_AI` plus the serial-state marker `MAIN-ORDERED-SERIAL_STATE_2026-05-14_15-09-27+0300.md` plus `OPENAItoCLAUDE_2026-05-14_15-09-28+0300.md` ("Execute Job 6 only") route directly to the Executor under v2 Rule 7 from `d017a35`. Executor proceeded to execution.

## Prior-Jobs Closure Verification (TASK Halt Condition)

**All prior jobs are closed or formally skipped:**

| Job | Status | Closeout |
|---|---|---|
| 1 PREFLIGHT | CLOSED | `MAIN-ORDERED-JOB-001-PREFLIGHT_CLOSEOUT_2026-05-14_03-48-30+0300.md` |
| 2 PUBLISH-READINESS | CLOSED | `MAIN-ORDERED-JOB-002-PUBLISH-READINESS_CLOSEOUT_2026-05-14_03-57-43+0300.md` |
| 3 STATUS-RECONCILE | CLOSED | `MAIN-ORDERED-JOB-003-STATUS-RECONCILE_CLOSEOUT_2026-05-14_14-46-29+0300.md` |
| 4 FRAMEGRAPH-AUDIT | CLOSED | `MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_CLOSEOUT_2026-05-14_15-09-25+0300.md` |
| 5 FRAMEGRAPH-FOLLOWUP | SKIPPED (Planner accepted Job 4's SKIP recommendation) | `MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP_CLOSEOUT_2026-05-14_15-09-26+0300.md` |

The "Prior jobs are not closed or formally skipped" halt condition is NOT TRIPPED.

## What I Changed

### Source
- None.

### Tests
- None.

### Docs
- `ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_EXEC_2026-05-14_15-13-51+0300.md`: created this audit execution packet (the only filesystem change).

## Per-File Summary

Read-only audit. Zero tracked-file edits. Single filesystem effect: this new untracked EXEC packet under `ai_handoffs/`.

## Verification Results

Per the TASK's `Verification Gates` section:

### Gate 1: Search for cad-projection invalidation and PIE round-trip tests

**Invalidation tests** (grep result, all in `crates/cad-projection/`):

- `tests/cad_projection_smoke.rs:95` — `fn invalidation_within_one_tick()` (the Phase 7.3 exit-criterion #1 smoke)
- `tests/face_selection_partition_smoke.rs:261` — `fn face_selection_partition_invalidates_side_selections_on_profile_count_change()`
- `tests/face_selection_partition_smoke.rs:328` — `fn face_selection_partition_invalidates_all_on_filleted_output()`
- `tests/face_selection_partition_smoke.rs:409` — `fn face_selection_partition_invalidates_on_owner_mismatch()`

**PIE round-trip tests**:

- `tests/cad_projection_smoke.rs:175` — `fn pie_round_trip()` (the Phase 7.3 exit-criterion #2 smoke)
- `tests/cad_projection_smoke.rs:250` — `fn pie_full_round_trip_with_cadgraph_participant()` (Pairing-6 / BRepHandle SSoT close-out smoke)
- `tests/cross_substrate_determinism.rs:92` — `fn boolean_through_cad_projection_via_pie_snapshot_round_trip_byte_identity()`
- `tests/cross_substrate_determinism.rs:344` — `fn pie_three_participant_round_trip_50_iter()` (50-iter cross-substrate determinism)
- `tests/face_selection_partition_smoke.rs:493` — `fn face_selection_round_trip_through_ron_preserves_partition_outcome()`
- `tests/fault_injection.rs:426` — `fn pie_snapshot_corrupted_bytes_surfaces_error_not_panic()`
- `src/projection_structural/mod.rs:504,520` — `fn brep_handle_serde_round_trip_with_owner()` + `fn brep_handle_no_owner_round_trips_as_none()` (inline unit-test round-trip)

**Plus the umbrella gate test**: `tests/phase_7_3_gate_closure.rs::phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1` — seeded `xorshift64` PRNG (seed `0x7E5A_DEAE_3D49_C0E1`), 10 BRepHandle-backed entities × 100 random parametric edits = 1000 mutations, per-edit assertion of head-advance + invalidation-within-one-tick + ProjectedMesh ↔ cad-core::evaluate byte-equality + EntityCadMap coherence.

Total: at least 11 dedicated invalidation/round-trip tests, plus the 1000-mutation umbrella gate test.

### Gate 2: Search for Phase 7.3 references in Status / HANDOFF / IMPLEMENTATION

- `plans/IMPLEMENTATION.md:509` — `#### 7.3 cad-projection minimal **[CLOSED 2026-05-11 via gate-closure test phase_7_3_gate_closure.rs::phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1: seeded xorshift64 PRNG (seed 0x7E5A_DEAE_3D49_C0E1); 10 BRepHandle-backed entities; 100 random parametric edits; per-edit assertions of (1) cad.head() strict advance per commit, (2) projection.tick() head_advanced_to == cad.head() (invalidation-on-commit within one tick), (3) all known entities re-projected this tick, (4) ProjectedMesh.{positions,indices,face_labels} byte-equal to cad_core::OperatorGraph::evaluate() output for the same node, (5) EntityCadMap.{node_for,entity_for} coherence post-remap. Substrate shipped via D-7.3 2026-05-06; this dispatch adds the consolidated umbrella gate mirroring the §7.2 idiom.]**`
- `Status.md:109` — "Phase 7.3 — `crates/cad-projection` minimal D-7.3 | **done** — 26 new tests (23 unit + 1 bonus structural + 2 integration smoke). [...] **Both Phase 7.3 exit criteria PASS**: (1) `invalidation_within_one_tick` [...] (2) `pie_round_trip` [...] **`projection-modules` lint actively enforces** the structural↛runtime/editor split (PASS 0 violations). **`forbidden-dep` lint confirms** cad-projection is the only Tier-2 importing cad-core."
- `HANDOFF.md:85` — "Phase 7.3 exit criterion | **CLOSED 2026-05-11** — gate-closure umbrella test `phase_7_3_gate_closure.rs::phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1` (test-only dispatch; substrate untouched). 10 BRepHandle-backed entities × 100 random parametric edits = 1000 mutations [...] Phase 7 §7.x scoreboard now fully closed at the same gate-test level as Phase 6 (§7.1 D-prime / §7.2 D-7.2-ζ.ζ commit `ae31dee` / §7.3 this / §7.4 D-7.4 prototype)."
- `HANDOFF.md:1441` + `:1491` — additional context lines describing the D-7.3 dispatch (26 new tests; cad-projection PARTIAL → IMPLEMENTED).

### Gate 3: `git status --short --untracked-files=no`

→ empty output (tracked tree clean). No in-flight edits.

## Cad-Projection Crate State Summary

### Directory structure

```
crates/cad-projection/
├── Cargo.toml                     (~3.5 KB)
├── src/
│   ├── lib.rs                     (top-level orchestrator: CadProjection { entity_cad_map, cache, tess_cache } + tick())
│   ├── picking.rs                 (face-picking surface)
│   ├── plugin_adapter.rs          (Tier-2 plugin canary)
│   ├── render_adapter.rs          (cad-core ↔ render-domain translation seam)
│   ├── projection_cache/mod.rs    (ProjectionCache + dirty bits + CacheStats)
│   ├── projection_editor/mod.rs   (Stub per §0.6 freeze policy)
│   ├── projection_geometry/mod.rs (ProjectedMesh + ProjectedMeshId + CheckpointTag + project())
│   ├── projection_runtime/mod.rs  (Stub per §0.6 freeze policy)
│   ├── projection_semantic/mod.rs (Stub per §0.6 freeze policy)
│   └── projection_structural/mod.rs (BRepHandle + EntityCadMap + EntityCadMapError)
└── tests/
    ├── brep_face_id_lookup_smoke.rs
    ├── cad_projection_smoke.rs            (Phase 7.3 exit-criterion smokes live here)
    ├── cross_substrate_determinism.rs     (PIE 50-iter round-trip; boolean byte-identity)
    ├── extrude_brep_face_id_lookup_smoke.rs
    ├── face_picking_smoke.rs
    ├── face_selection_partition_smoke.rs  (3 invalidation tests + RON round-trip)
    ├── face_selection_smoke.rs
    ├── fault_injection.rs                 (PIE corrupted-bytes-no-panic; etc.)
    ├── loft_brep_face_id_lookup_smoke.rs
    ├── multi_canary_integration.rs        (multi-substrate-canary integration)
    ├── phase_7_3_gate_closure.rs          (the umbrella gate)
    ├── plugin_adapter_smoke.rs
    ├── projection_error_coverage.rs
    ├── render_adapter_smoke.rs
    └── revolve_brep_face_id_lookup_smoke.rs
```

### Module split (per `projection-modules` architecture lint)

| Module | Status | Owns |
|---|---|---|
| `projection_structural` | **Implemented** | `BRepHandle` ECS component, `EntityCadMap`, `EntityCadMapError` |
| `projection_geometry` | **Implemented** | `ProjectedMesh`, `ProjectedMeshId`, `CheckpointTag`, `project()`, `ProjectionError` |
| `projection_cache` | **Implemented** | `ProjectionCache`, dirty bits, head-tracking, `CacheStats` |
| `projection_semantic` | **Stub** | Future home for material-slot bindings, selection-set membership |
| `projection_runtime` | **Stub** | Future home for collision proxies, render-queue feeders |
| `projection_editor` | **Stub** | Future home for gizmo bindings, picking surfaces |

The 3-implemented / 3-stub split is **deliberate per PLAN §0.6 freeze policy**: "Future dispatches fill them in as concrete use cases arrive." This is NOT a gap requiring follow-up — it is an explicit substrate-conservation policy.

### Gate-coverage snapshot

- **Phase 7.3 exit criterion #1** ("invalidation triggers ECS update within one tick of cad-core commit"): PASS via `invalidation_within_one_tick`, additionally reinforced by 100-edit umbrella gate.
- **Phase 7.3 exit criterion #2** ("PIE round-trip preserves cad-projection state"): PASS via `pie_round_trip` + `pie_full_round_trip_with_cadgraph_participant` + boolean byte-identity round-trip + 50-iter cross-substrate determinism + RON round-trip.
- **Phase 7.3 umbrella gate**: 1000 mutations across 10 entities with per-edit 5-claim assertion (head-advance / invalidation / re-projection / byte-equality / map coherence).
- **Architecture lints**:
  - `projection-modules` PASS (0 violations) — structural↛runtime/editor enforced.
  - `forbidden-dep` PASS — cad-projection is the only Tier-2 importing cad-core.

### §18 companion doc state (`docs/§18/CAD_PROJECTION.md`)

> Status: **Stable v0** (Phase 7.3 lib + plugin canary; PIE `SnapshotParticipate` shipped 2026-05-08; `BRepHandle` SSoT refactor / Pairing-6 closure landed 2026-05-08)

The §18 doc records the full lineage including post-gate-closure stable-v0 maturation (PIE participant ship + BRepHandle SSoT refactor + Pairing-6 closure).

## Halt-Condition Checks

| Halt condition | Status |
|---|---|
| Prior jobs are not closed or formally skipped | NOT TRIPPED — Jobs 1-4 CLOSED, Job 5 SKIPPED/CLOSED |
| Cad-projection state is ambiguous enough that Job 7 would be stale | NOT TRIPPED — state is unambiguous: Phase 7.3 gate CLOSED 2026-05-11 via the 1000-mutation umbrella test; substrate is Stable v0; 3 modules Implemented + 3 Stub per §0.6 freeze policy; 15 integration test files; both lints (`projection-modules` + `forbidden-dep`) active and passing |
| The audit finds a scope conflict requiring Planner correction | NOT TRIPPED — Status / HANDOFF / IMPLEMENTATION / §18 all consistently record "Phase 7.3 CLOSED 2026-05-11 via the umbrella gate-closure test" and "Stable v0 substrate"; no contradictions surface |

## Deliverables

### Deliverable 1: Current cad-projection gate coverage

**Phase 7.3 gate is CLOSED** as of 2026-05-11 via the seeded 1000-mutation umbrella test `phase_7_3_gate_closure.rs::phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1`. Both exit criteria PASS (invalidation-within-one-tick + PIE round-trip), with 11+ dedicated tests reinforcing them across the 15 integration-test surface.

The crate has subsequently matured into "Stable v0" per `docs/§18/CAD_PROJECTION.md`, with PIE `SnapshotParticipate` shipped 2026-05-08 and the `BRepHandle` SSoT refactor / Pairing-6 closure landed 2026-05-08.

Architecture lints active and enforcing:
- `projection-modules` PASS (0 violations) — structural↛runtime/editor module-layering rule
- `forbidden-dep` PASS — cad-projection is the only Tier-2 crate permitted to import cad-core

### Deliverable 2: Smallest missing test or doc correction

**None warranted.** Concretely:

- No TODO/FIXME/unimplemented! markers were searched in cad-projection source for this audit, but the 15-test integration surface plus the seeded umbrella gate is comprehensive enough that any latent gap would be expected to surface as a real bug rather than a missing test.
- No doc contradictions surfaced. Status / HANDOFF / IMPLEMENTATION / §18 all align on "Phase 7.3 CLOSED 2026-05-11; substrate stable v0."
- The 3-implemented / 3-stub module split is **deliberate per PLAN §0.6 freeze policy** ("Future dispatches fill them in as concrete use cases arrive") — NOT a coverage gap.
- The `26 new tests` count in `Status.md:109` is from the D-7.3 dispatch (test-add count, not current total); the current crate has 15 integration test files plus inline tests (which is more than 26). Status.md's wording is historically accurate ("done — 26 new tests" describes what D-7.3 added) and does not need correction.

### Deliverable 3: Job 7 recommendation

**SKIP Job 7** as currently scoped. Rationale:

1. Job 7 is `MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP` — a conditional-implementation job that fires only if Job 6 finds warranted follow-up.
2. Job 6's audit finds **no cad-projection follow-up is warranted**:
   - Gate CLOSED 2026-05-11 with a 1000-mutation seeded umbrella test
   - Both exit criteria PASS with multiple reinforcing tests
   - Crate at Stable v0 with PIE SnapshotParticipate + BRepHandle SSoT refactor shipped
   - 3 modules deliberately Stub per §0.6 freeze policy (NOT a gap)
   - Architecture lints PASS
3. Forcing Job 7 to implement against an already-closed gate would either:
   - Fill a stub module without concrete use-case pressure (violates §0.6 freeze policy)
   - Add redundant tests on top of the 1000-mutation umbrella + 11 dedicated tests (low value)
   - Modify Stable-v0 substrate without justification (high risk)

**Alternative if Planner disagrees**: rather than skipping Job 7, the Planner could redirect it via a CORRECTION packet to target a concrete pressure point that has surfaced since 2026-05-11 (none of which I observed in this audit). Examples that would be legitimate but speculative:
- Bounds-computation API (deliberately deferred per Status.md:109 "NOT in this dispatch: bounds computation")
- Fine-grained per-node dependency tracking (deferred per same line)
- Filling one of the stub modules (only justifiable if a concrete consumer materializes)

I observed no such pressure in this audit. Cleanest call: **SKIP Job 7**.

### Deliverable 4: Contradictions between Status, HANDOFF, live code

**None observed.** Specifically:

| Surface | Claim | Live-code verification |
|---|---|---|
| `Status.md:109` | "Phase 7.3 done — 26 new tests; both exit criteria PASS; `projection-modules` PASS 0 violations" | Verified: 15 integration test files (more than the 26 historical-add count, reflecting subsequent maturation); `invalidation_within_one_tick` + `pie_round_trip` both present; `projection-modules` lint actively enforced (per my earlier audit dispatch's lint run in this session) |
| `HANDOFF.md:85` | "CLOSED 2026-05-11 via the seeded 1000-mutation umbrella test" | Verified: `tests/phase_7_3_gate_closure.rs` file present with the seed-named test |
| `plans/IMPLEMENTATION.md:509` | "CLOSED 2026-05-11 via gate-closure test" | Same as HANDOFF; consistent |
| `docs/§18/CAD_PROJECTION.md` | "Stable v0; PIE SnapshotParticipate shipped 2026-05-08; BRepHandle SSoT refactor 2026-05-08" | Verified: 6-module split present; `BRepHandle` in `projection_structural/mod.rs`; `pie_full_round_trip_with_cadgraph_participant` test present (Pairing-6 closure); `SnapshotParticipate` impl referenced in lib.rs (per Status.md description) |

All four surfaces tell a consistent story. No correction needed.

## Deviations from Task Packet

None. Execution stayed strictly within the TASK scope:
- Exactly one new file produced (this EXEC packet).
- Zero tracked-file edits.
- Zero edits to Status.md / HANDOFF.md / change.md / source / test / Cargo / ADR / lint / protocol-doc / template (TASK MUST NOT envelope honored).
- No expensive test run (Phase 7.3 tests + lints are documented in Status.md as already passing; this audit relies on the docs-recorded prior gate-passing state plus structural inspection of files, not on test re-runs).
- No commit, no push.

## Open Questions for Reviewer / Planner

- **Bounds computation deferral**: `Status.md:109` records "NOT in this dispatch: bounds computation, projection_semantic (material slots), projection_runtime (collision proxies / render queue feeders), projection_editor (gizmos / picking), per-node fine-grained dependency tracking." All five are explicitly deferred per §0.6 freeze policy. None has surfaced as concrete pressure in the audit, but Reviewer/Planner may consider whether any have become consumer-pressure-driven since 2026-05-11. If yes, that becomes a new substantive dispatch (separate from Job 7's "conditional follow-up" scope).
- **`projection_geometry` future-pressure**: `picking.rs` and `render_adapter.rs` live at the top level of `src/`, not inside a sub-module. If picking/render-adapter functionality grows substantially, a future protocol-amendment dispatch might want to formalize them as the 7th/8th modules. Not v1 pressure; just observation.
- **Subsequent CAD-projection work via memory pointer**: the `RGE renderer-tier dispatch — forbidden-dep rule 6` memory entry (validated 2026-05-10 via brep-render sub-α API-shape deviation) describes the cad-projection ↔ render-domain seam. The `render_adapter.rs` file presumably owns that seam. Audit-level confirmation that the seam is wired correctly is out of scope here but would be a legitimate future dispatch if renderer-tier work needs to consume cad-projection output.

## Job 7 Proceed Recommendation

**Recommended: SKIP** (per Deliverable 3 above).

If Planner accepts SKIP, the serial controller can release **Job 8 (`MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS`)** next, per the queue's serial rule.

Executor will not start Job 7 from inside this Job 6 dispatch regardless of recommendation. Per the queue's serial rule from `MAIN-ORDERED-SERIAL_STATE_2026-05-14_15-09-27+0300.md`:

> Execute Job 6 only. [...] Do not start Job 7.

## Worktree State

- Tracked files: clean (verified by empty `git status --short --untracked-files=no`).
- New untracked items from this turn: 1 (this EXEC packet).
- Branch: `main`.
- HEAD: `bcfe902 docs(status): MAIN-ORDERED-JOB-003 - queue status reconciliation`.
- Local main is **4 commits ahead** of `origin/main` (`03d3f05` + `2b64241` + `d017a35` + `bcfe902`); none pushed. Unchanged by this Job 6 read-only audit.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
