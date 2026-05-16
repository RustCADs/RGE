# Task Packet

DISPATCH_ID: POSTV0-HANDOFF-ARTIFACT-TRIAGE-004
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-16_09-27-22+0300
RELATED_FILES:
- ai_handoffs/
- .ai/
- OPENAItoCLAUDE_*.md
- CLAUDEtoOPENAI_*.md
- JobsDone_*.md
- CLAUDE_SUB_EPSILON_REVIEW.md
- new-handoff.ps1
STATUS: OPEN

## Goal

Audit the repository's current untracked handoff-related artifacts and recommend cleanup actions without performing cleanup or editing any audited artifact. This is revision 0 for `POSTV0-HANDOFF-ARTIFACT-TRIAGE-004`; there is no prior Claude gate. The output should give a later Planner enough evidence to decide which handoff artifacts should be kept, tracked, archived, ignored, finalized, deleted, or escalated for human decision in a separate cleanup dispatch.

## Scope

This is an audit-only task. The Executor MUST NOT modify, delete, move, rename, format, finalize, repair, archive, ignore, or otherwise clean up any audited artifact.

### MAY edit
- None.

### MUST NOT edit
- Any tracked repository file.
- Any existing file under `ai_handoffs/`, including task, execution, review, correction, closeout, state, and sidecar artifacts.
- Any existing file under `.ai/`.
- Any root-level `OPENAItoCLAUDE_*.md`, `CLAUDEtoOPENAI_*.md`, `JobsDone_*.md`, or `CLAUDE_SUB_EPSILON_REVIEW.md` file.
- `.gitignore`.
- Source files, tests, docs, schemas, scripts, lockfiles, generated imports, archives, project metadata, protocol templates, or local tooling.
- Any other task packet.

### MAY add new files
- Exactly one execution report matching `ai_handoffs/POSTV0-HANDOFF-ARTIFACT-TRIAGE-004_EXEC_*.md`.

### MUST NOT add new files
- Cleanup scripts.
- Sidecar `.meta.json` files.
- New task, review, correction, closeout, or state packets.
- New docs, ADRs, schemas, source files, test files, `.gitignore` entries, archive files, generated metadata, or local tooling.
- Any file outside the single allowed execution report.

## Deliverables

- Produce exactly one EXEC packet for this dispatch under `ai_handoffs/`.
- Reference this TASK packet by filename in the EXEC packet.
- Record the exact commands used to enumerate untracked files and either raw results or concise summaries with enough detail for review.
- List every untracked handoff-related artifact found, grouped at minimum into `ai_handoffs/` packets and sidecars, root cross-AI handoff notes, `.ai/` protocol or dispatch artifacts, and other handoff-adjacent files.
- Explicitly list notable untracked files that were excluded from handoff cleanup analysis because they are not handoff artifacts.
- For each handoff-related artifact or group, recommend one cleanup action: keep as local scratch, finalize later, add to tracking later, archive later, delete later, ignore later, or needs human decision.
- Record any pre-existing tracked modification visible during the audit, especially `.gitignore`, without editing it.
- Record the final workspace observation after writing the EXEC packet and explain any difference from the starting observation.

## Acceptance Criteria

- The task remains audit-only: no existing file is modified, moved, deleted, finalized, repaired, archived, ignored, or reformatted.
- The only new file, if any, is the single EXEC packet allowed above.
- The EXEC packet distinguishes handoff artifacts from unrelated untracked project imports, archives, scripts, generated files, local scratch files, and other non-handoff files.
- Cleanup recommendations are concrete enough for a later Planner to create a separate cleanup task without redoing the inventory.
- The EXEC packet includes start and end `git status --short --untracked-files=all` observations.
- The EXEC packet documents every verification gate result, including non-zero exits if any gate fails.
- The EXEC footer has exactly one line-anchored `HANDOFF_STATUS: COMPLETE` if the audit succeeds.

## Constraints / Non-Goals

- Do not perform cleanup.
- Do not edit `.gitignore`.
- Do not create, finalize, repair, delete, or regenerate handoff sidecars.
- Do not classify unrelated source imports, project research, generated artifacts, archives, or local scratch files as cleanup targets unless they directly function as handoff artifacts.
- Do not run Rust builds, tests, formatters, architecture lints, package managers, or other expensive validation.
- Do not commit or push.
- Do not treat this packet as authorization to modify any artifact after recommending a cleanup action.
- Do not start a follow-up cleanup, correction, review, or closeout packet.

## Verification Gates

The Executor MUST run and document the result of each gate in the EXEC packet:

- `git status --short --untracked-files=all`
- `git ls-files --others --exclude-standard -- ai_handoffs`
- `git ls-files --others --exclude-standard -- .ai`
- `git ls-files --others --exclude-standard -- 'OPENAItoCLAUDE_*.md' 'CLAUDEtoOPENAI_*.md' 'JobsDone_*.md' 'CLAUDE_SUB_EPSILON_REVIEW.md'`
- `git diff --stat -- .gitignore`
- A final `git status --short --untracked-files=all` after writing the EXEC packet, with any difference from the starting state explained.

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if any of the following occur:

- Any command needed to enumerate untracked files cannot run or returns output that cannot be interpreted.
- The audit cannot be documented without editing an existing file other than the allowed EXEC packet.
- Answering the task would require deleting, moving, renaming, ignoring, archiving, finalizing, repairing, or otherwise changing an artifact.
- The observed state suggests repository corruption, unreadable git metadata, path access failures, or inconsistent git output that makes the inventory unreliable.
- Any `ai_handoffs/POSTV0-HANDOFF-ARTIFACT-TRIAGE-004_EXEC_*.md` file already exists before the Executor starts, making the correct report target ambiguous.
- The Executor finds that a requested cleanup recommendation would affect source, docs, schemas, scripts, `.gitignore`, or project metadata rather than handoff artifacts.

## Planner Notes

This packet intentionally authorizes inventory and recommendations only. It does not authorize cleanup. A later Planner packet should decide whether to delete, archive, track, ignore, or finalize any artifacts based on the Executor's report. The workspace may already contain many untracked files and tracked modifications; those facts should be recorded, not corrected, by this dispatch.

---

HANDOFF_STATUS: COMPLETE
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
