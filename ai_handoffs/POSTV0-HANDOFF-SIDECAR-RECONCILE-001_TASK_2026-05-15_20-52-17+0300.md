# Task Packet

DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-15_20-52-17+0300
RELATED_FILES:
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
- ai_handoffs/templates/TASK_PACKET.md
- ai_handoffs/templates/EXECUTION_REPORT.md
- ai_handoffs/templates/REVIEW_REPORT.md
- ai_handoffs/templates/CORRECTION_PACKET.md
- ai_handoffs/templates/FINAL_CLOSEOUT.md
- .ai/handoff.schema.json
- .ai/handoff.template.md
- new-handoff.ps1
STATUS: OPEN

## Goal

Reconcile the held-back JSON handoff files with the canonical `ai_handoffs/`
packet protocol. The current `.ai/handoff.schema.json`,
`.ai/handoff.template.md`, and `new-handoff.ps1` were created as a parallel
root-level `<SENDER>to<RECEIVER>_*.md` handoff surface. That competes with
`ai_handoffs/AI_HANDOFF_PROTOCOL.md`, which already defines the authoritative
dispatch lifecycle and already reserves a future structured JSON sidecar path.
This dispatch should remove that authority split by reshaping the JSON work
into a machine-readable `.meta.json` sidecar for canonical `ai_handoffs/`
Markdown packets.

## Scope

### MAY edit
- `.ai/handoff.schema.json` - reshape into the draft-07 schema for
  `ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.meta.json`.
- `.ai/handoff.template.md` - delete if it remains a competing root handoff
  template; canonical Markdown templates already live in
  `ai_handoffs/templates/`.
- `new-handoff.ps1` - rewrite as a canonical `ai_handoffs/` packet and sidecar
  scaffolder, or delete if it cannot be made canonical in this dispatch.
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md` - promote the reserved JSON sidecar
  wording into a defined v1 sidecar section.

### MUST NOT edit
- `crates/**`
- `kernel/**`
- `runtime/**`
- `editor/**`
- `plans/**`
- `docs/**` outside `ai_handoffs/AI_HANDOFF_PROTOCOL.md`
- `Status.md`
- `HANDOFF.md`
- `change.md`
- `V0_RELEASE_CERTIFICATION.md`
- `Cargo.toml`
- `Cargo.lock`
- `.mcp.json`
- `.claude/**`
- `.gitignore`
- `ai-review.ps1`
- `ai-review.sh`
- `.ai/claude_brief.schema.json`
- `.ai/codex_review.schema.json`
- any existing `ai_handoffs/*_TASK_*.md`, `*_EXEC_*.md`, `*_REVIEW_*.md`,
  `*_CORRECT_*.md`, or `*_CLOSEOUT_*.md` packet file
- root-level `OPENAItoCLAUDE_*.md`, `CLAUDEtoOPENAI_*.md`, or
  `CLAUDE_SUB_EPSILON_REVIEW.md`

### MAY add new files
- Exactly one `EXECUTION_REPORT` packet for this dispatch under
  `ai_handoffs/`.
- Temporary local validation samples under `.ai/` if needed, but they MUST
  remain ignored/untracked generated artifacts and MUST NOT be committed.

### MUST NOT add new files
- New root-level handoff Markdown files.
- New `ai_handoffs/templates/*.md` templates.
- New ADRs, architecture lints, doctrine docs, Cargo entries, crates, tests, or
  source files.
- Committed sample `.meta.json` sidecars, unless the Executor halts for Planner
  approval first and explains why a permanent sample is required.

## Deliverables

- `.ai/handoff.schema.json` defines the canonical sidecar shape for
  `ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.meta.json`.
- The sidecar schema mirrors the canonical Markdown packet fields rather than
  the old root sender/receiver convention. Required fields should include at
  least:
  - `schema_version`
  - `dispatch_id`
  - `packet_type` with values `TASK`, `EXEC`, `REVIEW`, `CORRECT`, `CLOSEOUT`
  - `author`
  - `timestamp`
  - `related_files`
  - `status`
  - footer mirror fields `handoff_status`, `next_role`, `exit_code`
- Optional fields may include `source_packet`, `findings`, `verification`,
  `remaining_risks`, `open_questions`, and packet-type-specific arrays or
  objects when they directly mirror sections in the canonical Markdown
  packets.
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md` documents the `.meta.json` sidecar as
  the defined machine-readable mirror for canonical packets and removes or
  replaces the "Future versions may add structured JSON sidecars" wording.
- `.ai/handoff.template.md` no longer describes an active parallel root
  handoff convention. Prefer deletion unless it is rewritten as a sidecar-only
  example and the existing `ai_handoffs/templates/` remain the only Markdown
  packet templates.
- `new-handoff.ps1`, if retained, emits canonical `ai_handoffs/` packet names
  plus matching `.meta.json` sidecars. It must not scaffold
  `CLAUDEtoOPENAI_*` or `OPENAItoCLAUDE_*` root files as the active path.
- One `EXECUTION_REPORT` packet documenting the exact files changed, validation
  commands, whether a commit was created, and any residual risks.

## Acceptance Criteria

- No tracked source, Cargo, plan, status, release-certification, or review
  tooling files are modified.
- No existing dispatch packet is edited; append-only protocol rules remain
  intact.
- `.ai/handoff.schema.json` is valid JSON Schema draft-07.
- A sample sidecar object for one existing canonical packet validates against
  `.ai/handoff.schema.json`. The sample may be generated temporarily and does
  not need to be committed.
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md` names `.ai/handoff.schema.json` as the
  schema for packet sidecars and documents the sidecar filename convention:
  `ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.meta.json`.
- No document describes `ai-handoff-v1` or root `<SENDER>to<RECEIVER>_*.md`
  files as the active canonical convention going forward. Historical root files
  may still be acknowledged as historical precedent.
- `new-handoff.ps1` either scaffolds canonical `ai_handoffs/` packet and
  sidecar pairs, or is removed with the reason documented in the execution
  report.
- If a commit is created, it contains only the allowed reconciliation files and
  excludes generated `.ai/*.json`, `.ai/*.diff`, `.claude/worktrees/`, existing
  handoff packets, and root precedent handoff files.

## Constraints / Non-Goals

- Do not bless the current `ai-handoff-v1` root sender/receiver schema as-is.
- Do not create a second active handoff bus.
- Do not rewrite historical root `OPENAItoCLAUDE_*` / `CLAUDEtoOPENAI_*`
  precedent files.
- Do not migrate old packets to sidecars in this dispatch.
- Do not reconcile stale `HANDOFF.md` counts in this dispatch.
- Do not clean up or ignore `.claude/worktrees/` in this dispatch; that is a
  separate local-state cleanup.
- Do not change the review automation committed in `8b4bea7`.
- Do not push unless the Human Arbiter explicitly authorizes it after review.

## Verification Gates

The Executor MUST run and document the result of each of these in their
`EXECUTION_REPORT`:

- `git status --short --branch` before and after the dispatch.
- `git rev-list --left-right --count origin/main...HEAD` before and after the
  dispatch.
- A JSON parse check for `.ai/handoff.schema.json`.
- A draft-07 schema validation of one sample `.meta.json` sidecar object
  against `.ai/handoff.schema.json`.
- `Select-String -Path ai_handoffs/AI_HANDOFF_PROTOCOL.md -Pattern "meta.json|handoff.schema.json|Future versions"` or equivalent evidence showing
  the reserved-sidecar wording is now a defined sidecar section, not a vague
  future note.
- If `new-handoff.ps1` is retained: a dry-run or safe test invocation proving
  it emits an `ai_handoffs/` packet path and matching `.meta.json` path without
  creating root `OPENAItoCLAUDE_*` / `CLAUDEtoOPENAI_*` files. If a dry-run mode
  does not exist, add one within this file or document why manual inspection is
  sufficient.
- `git diff --check`.

No cargo build or workspace test is required for this governance/tooling-only
dispatch. If the Executor chooses to run cargo gates, document them as extra
evidence, not as required acceptance gates.

## Halt Conditions

The Executor MUST halt without committing and set `HANDOFF_STATUS: BLOCKED`
with `NEXT_ROLE: PLANNER_AI` if any of the following occur:

- The root `<SENDER>to<RECEIVER>_*.md` convention turns out to serve a live
  purpose that the canonical `ai_handoffs/` packet lifecycle cannot absorb.
- A correct solution requires changing the already-committed review tooling in
  `8b4bea7`.
- A correct solution requires editing any existing packet despite append-only
  rules.
- A correct solution requires broad automation, CI integration, a new runtime
  daemon, or non-local orchestration.
- The Executor cannot validate the schema with locally available tools.
- The working tree contains tracked modifications outside the allowed files.

## Planner Notes

The chosen direction is option (a): conform to the canonical `.meta.json`
sidecar path. Option (b), blessing `ai-handoff-v1` as-is, was rejected because
it would preserve a parallel sender/receiver handoff convention and legitimize
the fragmentation this dispatch is meant to remove.

This dispatch intentionally dogfoods `ai_handoffs/AI_HANDOFF_PROTOCOL.md`
because it changes governance. Planner authored this `TASK_PACKET`; Executor
should proceed only within the scope above and route the result to Reviewer via
the standard footer.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
