# Correction Packet

DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-15_22-44-35+0300
RELATED_FILES:
- ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_TASK_2026-05-15_20-52-17+0300.md
- ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_EXEC_2026-05-15_21-10-04+0300.md
- ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_REVIEW_2026-05-15_21-19-56+0300.md
- .ai/handoff.schema.json
- new-handoff.ps1
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
STATUS: CORRECTION_OPEN

## References

- Task Packet: `ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_TASK_2026-05-15_20-52-17+0300.md`
- Latest Execution Report: `ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_EXEC_2026-05-15_21-10-04+0300.md`
- Latest Review Report: `ai_handoffs/POSTV0-HANDOFF-SIDECAR-RECONCILE-001_REVIEW_2026-05-15_21-19-56+0300.md`

## Approved Corrections (Planner Sign-Off)

The Executor MUST act on exactly these corrections, nothing more. This round
addresses the single P1 finding in the Review Report.

1. **Sidecar only on finalized packets** - Reviewer finding:
   "`new-handoff.ps1` writes a completed sidecar for an incomplete packet."
   Required change:
   - Do NOT add a `DRAFT` value to `handoff_status`; keep the existing
     protocol footer values unchanged.
   - Update `new-handoff.ps1` so ordinary scaffold mode creates only the
     canonical Markdown packet under `ai_handoffs/`. It MUST NOT create a
     `.meta.json` sidecar while the Markdown packet is still an unfilled
     template.
   - Add a finalize sidecar path to `new-handoff.ps1` (for example a
     `-Finalize -PacketPath <path>` mode, or an equivalent clearly named
     parameter set). Finalize mode MUST read an existing completed Markdown
     packet, parse its header/footer fields, reject unfilled placeholders, and
     write the matching `.meta.json` sidecar from the packet's actual values.
   - Update the `ai_handoffs/AI_HANDOFF_PROTOCOL.md` sidecar section so the
     lifecycle is coherent: sidecars are generated after packet finalization,
     not at initial scaffold time. The polling example may keep sidecar
     existence meaningful because sidecar existence now implies a finalized
     packet was parsed, but it should explicitly state that incomplete
     scaffolded packets have no sidecar.
   Acceptance:
   - A scaffold dry-run prints only the would-be `.md` packet path and no
     `.meta.json` path.
   - A finalize dry-run against a completed packet prints the would-be
     `.meta.json` path and derives values from the packet.
   - A finalize attempt against an unfilled template packet is rejected and
     creates no sidecar.
   - A generated or dry-run sidecar validates against `.ai/handoff.schema.json`
     and mirrors the packet's `DISPATCH_ID`, `AUTHOR`, `TIMESTAMP`,
     `STATUS`, `HANDOFF_STATUS`, `NEXT_ROLE`, and `EXIT_CODE`.

## Deferred Findings (NOT Approved for This Round)

None. The Review Report raised one actionable P1 finding, and this packet
approves that correction.

## Updated Acceptance Criteria

All original Task Packet acceptance criteria still apply, with this additional
acceptance bar:

- `new-handoff.ps1` must not allow a `.meta.json` sidecar to advertise
  `handoff_status: COMPLETE` for an unfilled Markdown template.

## Re-Verification Gates

The Executor MUST re-run and document these gates after the correction:

- `git status --short --branch` after correction.
- `git rev-list --left-right --count origin/main...HEAD`.
- JSON parse check for `.ai/handoff.schema.json`.
- Draft-07 schema validation of a valid sidecar sample against
  `.ai/handoff.schema.json`.
- Negative validation proving an invalid sidecar is rejected.
- `new-handoff.ps1` scaffold dry-run proving no `.meta.json` sidecar is
  created or advertised at scaffold time.
- `new-handoff.ps1` finalize dry-run against a completed packet proving the
  sidecar path and mirrored values are derived from that packet.
- `new-handoff.ps1` finalize attempt or dry-run against an unfilled template
  proving placeholders are rejected.
- `Select-String -Path ai_handoffs/AI_HANDOFF_PROTOCOL.md -Pattern "scaffold|finaliz|meta.json|handoff.schema.json|Future versions"` or equivalent evidence
  showing the sidecar lifecycle is defined and "Future versions" wording has
  not returned.
- `git diff --check`.

No cargo build or workspace test is required.

## Halt Conditions (Updated if Any)

Unchanged from the Task Packet, plus:

- Halt if implementing finalize mode would require changing committed review
  tooling from `8b4bea7`.
- Halt if `new-handoff.ps1` cannot reliably parse required header/footer
  fields from canonical packet Markdown without broad parser work.
- Halt if sidecar-on-finalize cannot be implemented without adding a parallel
  state machine outside `ai_handoffs/AI_HANDOFF_PROTOCOL.md`.

## Planner Notes

Planner chooses Fix B: sidecar on finalize. This preserves the existing
footer enum, keeps `.meta.json` existence meaningful, and avoids introducing a
`DRAFT` status that the Markdown footer does not have. The sidecar is a mirror
of a completed packet, not a draft-control surface.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-HANDOFF-SIDECAR-RECONCILE-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
