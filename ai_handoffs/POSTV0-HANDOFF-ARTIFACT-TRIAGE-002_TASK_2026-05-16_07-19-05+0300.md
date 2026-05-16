# Task Packet

DISPATCH_ID: POSTV0-HANDOFF-ARTIFACT-TRIAGE-002
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-16_07-19-05+0300
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

Audit the repository's current untracked handoff-related artifacts and recommend a cleanup plan without changing those artifacts. This dispatch is revision 0 for `POSTV0-HANDOFF-ARTIFACT-TRIAGE-002`; there is no prior Claude gate. The purpose is to separate handoff/protocol artifacts that should be kept, finalized, tracked, archived, ignored, or deleted from unrelated untracked work before any cleanup is attempted.

## Scope

This is an audit-only task. The Executor MUST NOT modify, delete, move, rename, format, finalize, or create cleanup sidecars for any audited artifact.

### MAY edit
- None.

### MUST NOT edit
- Any tracked repository file.
- Any existing file under `ai_handoffs/`.
- Any existing file under `.ai/`.
- Any root-level `OPENAItoCLAUDE_*.md`, `CLAUDEtoOPENAI_*.md`, `JobsDone_*.md`, or `CLAUDE_SUB_EPSILON_REVIEW.md` file.
- `.gitignore`.
- Source files, tests, docs, schemas, scripts, lockfiles, generated imports, archives, or project metadata.

### MAY add new files
- Exactly one execution report matching `ai_handoffs/POSTV0-HANDOFF-ARTIFACT-TRIAGE-002_EXEC_*.md`.

### MUST NOT add new files
- Cleanup scripts.
- Sidecar `.meta.json` files.
- New task, review, correction, or closeout packets.
- New docs, ADRs, schemas, source files, test files, `.gitignore` entries, or archive files.
- Any file outside the single allowed execution report.

## Deliverables

- Produce one EXEC packet for this dispatch under `ai_handoffs/`.
- Record the exact commands used to enumerate untracked files and their raw or summarized results.
- List every untracked handoff-related artifact found, grouped at minimum into `ai_handoffs/` packets, root cross-AI handoff notes, `.ai/` protocol or dispatch artifacts, and other handoff-adjacent files.
- Explicitly list notable untracked files that were excluded from handoff cleanup analysis because they are not handoff artifacts.
- For each handoff-related group, recommend one cleanup action: keep as local scratch, finalize later, add to tracking later, archive later, delete later, ignore later, or needs human decision.
- Record any pre-existing tracked modification visible during the audit, especially `.gitignore`, without editing it.

## Acceptance Criteria

- The task remains audit-only: no existing file is modified, moved, deleted, finalized, or reformatted.
- The only new file, if any, is the single EXEC packet allowed above.
- The EXEC packet references this TASK packet by filename.
- The EXEC packet includes the start and end `git status --short --untracked-files=all` observations.
- The EXEC packet distinguishes handoff artifacts from unrelated untracked project imports, archives, scripts, and other local files.
- Cleanup recommendations are concrete enough for a later Planner to create a separate cleanup task without redoing the inventory.
- The EXEC footer has exactly one line-anchored `HANDOFF_STATUS: COMPLETE` if the audit succeeds.

## Constraints / Non-Goals

- Do not perform cleanup.
- Do not edit `.gitignore`.
- Do not create, finalize, or repair handoff sidecars.
- Do not classify unrelated source imports or project research as cleanup targets unless they directly function as handoff artifacts.
- Do not run Rust builds, tests, formatters, architecture lints, or other expensive validation.
- Do not commit or push.
- Do not treat this packet as authorization to modify any artifact after recommending a cleanup action.

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
- The Executor determines that the audit cannot be documented without editing an existing file other than the allowed EXEC packet.
- The Executor would need to delete, move, rename, ignore, finalize, or otherwise change an artifact to answer the task.
- The observed state suggests repository corruption, unreadable git metadata, or path access failures that make the inventory unreliable.
- More than one new execution report for this dispatch already exists, making the correct report target ambiguous.

## Planner Notes

This packet intentionally authorizes inventory and recommendations only. It does not authorize cleanup. A later Planner packet should decide whether to delete, archive, track, ignore, or finalize any artifacts based on the Executor's report. The starting workspace is known to contain many untracked files and at least one tracked `.gitignore` modification; those facts should be recorded, not corrected, by this dispatch.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-HANDOFF-ARTIFACT-TRIAGE-002
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
