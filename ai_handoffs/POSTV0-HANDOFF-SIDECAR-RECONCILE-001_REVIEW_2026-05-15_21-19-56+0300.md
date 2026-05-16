# Review Report

DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Reviewer / OpenAI Codex
TIMESTAMP: 2026-05-15_21-19-56+0300
RELATED_FILES:
- ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_TASK_2026-05-15_20-52-17+0300.md
- ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_EXEC_2026-05-15_21-10-04+0300.md
- .ai/handoff.schema.json
- new-handoff.ps1
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
STATUS: NEEDS_CORRECTION

## Verdict

NEEDS_CORRECTION.

## Findings

### [P1] `new-handoff.ps1` writes a completed sidecar for an incomplete packet

- Category: needs_correction
- Severity: high
- File: `new-handoff.ps1`
- Lines: 79-90

`new-handoff.ps1` copies the canonical Markdown template verbatim at line 79, but then writes a sidecar with `handoff_status = 'COMPLETE'` at line 90. The copied templates still contain placeholders such as `DISPATCH_ID: <same as the TASK_PACKET>` and `HANDOFF_STATUS: <COMPLETE | FAILED | BLOCKED | NEEDS_HUMAN>`, so the generated Markdown packet is not actually complete or consumable. The sidecar would therefore falsely advertise readiness to any watcher that follows the newly documented sidecar polling path.

This violates the dispatch goal that the sidecar is a machine-readable mirror of the authoritative packet, and it creates the same kind of coordination false-positive the protocol footer was designed to avoid.

Recommended fix: make the scaffolder either:

- fully instantiate the Markdown packet header/footer so the `.md` and `.meta.json` agree, or
- do not create the sidecar until the packet is complete, or
- add an explicit draft mode to the schema/protocol and ensure watchers do not treat draft sidecars as ready.

The smallest correction is probably to replace the template placeholders for `DISPATCH_ID`, `AUTHOR`, `TIMESTAMP`, `STATUS`, `HANDOFF_STATUS`, `NEXT_ROLE`, and `EXIT_CODE` when creating the `.md`, then emit the sidecar with the same values.

## Correctness Notes

- The protocol doc change is directionally correct: it promotes `.meta.json` from vague future wording into an optional sidecar, keeps Markdown authoritative, and marks root `OPENAItoCLAUDE_*` / `CLAUDEtoOPENAI_*` files as historical only.
- `.ai/handoff.schema.json` parses and validates the reviewer sample shape. A deliberately invalid sample missing `exit_code` and using `packet_type: "FOO"` is rejected.
- Deleting `.ai/handoff.template.md` is correct; it was the competing root-template surface.
- Retaining `new-handoff.ps1` is acceptable if the correction above makes it safe.
- A strict `additionalProperties: false` sidecar schema is acceptable for this v1 governance surface; schema revision is the right mechanism for new packet sections.
- Not creating `.meta.json` sidecars for existing packets is acceptable. The task explicitly made migration/population a non-goal.

## Verification Performed

- Read the TASK packet and EXEC packet in full.
- `git status --short --branch` -> branch is `main...origin/main`; tracked modification is `ai_handoffs/AI_HANDOFF_PROTOCOL.md`; `.ai/handoff.schema.json`, `new-handoff.ps1`, and this dispatch's packets are untracked as expected.
- `git rev-list --left-right --count origin/main...HEAD` -> `0 0`.
- `git diff --check` -> exit 0, only LF/CRLF warning for `ai_handoffs/AI_HANDOFF_PROTOCOL.md`.
- `.ai/handoff.schema.json` parse via `ConvertFrom-Json` -> OK.
- Draft-07 validation via `npx --yes ajv-cli validate --spec=draft7 --strict=false`:
  - valid sample sidecar -> valid, exit 0.
  - invalid sample missing `exit_code` and with `packet_type: "FOO"` -> invalid, exit 1.
- `new-handoff.ps1 -DryRun` -> emits only `ai_handoffs/...md` and `ai_handoffs/...meta.json` paths, no root handoff paths.
- Inspected `new-handoff.ps1` implementation lines 79-90 and canonical template footer placeholders.

## Test Coverage Assessment

No cargo tests are required for this governance/tooling-only dispatch. The schema validation and dry-run checks are the right gate class, but the dry-run did not catch the sidecar/Markdown readiness mismatch because it only prints paths. The correction should include a dry-run or non-destructive check that proves generated packet and sidecar readiness fields agree.

## Recommended Action

Planner should issue a `CORRECTION_PACKET` approving the P1 scaffolder fix above. After correction, Reviewer should re-check `new-handoff.ps1` against the generated Markdown/sidecar consistency invariant.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Reviewer / OpenAI Codex
NEXT_ROLE: PLANNER_AI
EXIT_CODE: 0

---
