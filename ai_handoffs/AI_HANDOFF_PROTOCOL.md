# AI Handoff Protocol

A lightweight, append-only governance protocol for AI-to-AI dispatch exchanges
in the RGE repository.

## Purpose

Multiple AI agents (Planner, Executor, Reviewer) cooperate on RGE work under
human arbitration. This protocol makes their exchanges visible, auditable, and
recoverable without any runtime tooling. It is a Markdown-based record-keeping
convention — NOT runtime automation, NOT a build-system hook, NOT a replacement
for ADRs / `Status.md` / `HANDOFF.md` / `change.md`.

Use this protocol when more than one AI agent is collaborating on a dispatch
that warrants peer review (commit-level work, multi-step chapters, design
decisions). For one-shot trivial tasks the existing repo-level docs are enough.

## Roles

### Planner AI
- Decomposes the user's intent into bounded dispatches.
- Issues `TASK_PACKET` files defining scope, acceptance criteria, halt
  conditions, and verification gates.
- Approves `CORRECTION_PACKET` content before any correction round runs.
- Issues `FINAL_CLOSEOUT` when the dispatch lands or is abandoned.
- Owns scope. The Executor never expands scope without a fresh Planner packet.

### Executor AI
- Reads the `TASK_PACKET`.
- Executes strictly within the stated `MAY edit` / `MUST NOT edit` envelope.
- Writes an `EXECUTION_REPORT` describing what shipped and verification
  results.
- Flags any uncertainty as `Open Questions for Reviewer`, never as silent
  scope expansion.

### Reviewer AI
- Reads the `TASK_PACKET` + `EXECUTION_REPORT`.
- Independently re-runs verification gates where feasible.
- Reads the actual changes (not just claims).
- Writes a `REVIEW_REPORT` with verdict
  (`APPROVED` / `NEEDS_CORRECTION` / `REJECTED`) and concrete findings.
- Recommends; does not directly order corrections.

### Human Arbiter
- The user is the final authority.
- Resolves disagreements among Planner / Executor / Reviewer.
- May override any protocol rule.
- Closes ambiguous scope decisions explicitly.

## Dispatch Lifecycle

1. **Planner → `TASK_PACKET`.** Scope, deliverables, acceptance criteria,
   verification gates, halt conditions.
2. **Executor → `EXECUTION_REPORT`.** What changed, per-file summary,
   verification results, deviations, open questions.
3. **Reviewer → `REVIEW_REPORT`.** Independently verified gates, findings
   (correct / needs-correction / latent-risks), test-coverage assessment,
   doc-accuracy check, recommended action.
4. **If `APPROVED`:** Planner writes `FINAL_CLOSEOUT`. Dispatch is `CLOSED`.
5. **If `NEEDS_CORRECTION`:** Planner writes a `CORRECTION_PACKET` enumerating
   the approved subset of review findings. Executor writes a new
   `EXECUTION_REPORT`. Reviewer writes a new `REVIEW_REPORT`. Loop until
   `APPROVED` → `CLOSEOUT`.
6. **If `REJECTED`:** Planner writes `FINAL_CLOSEOUT` with `STATUS: ABANDONED`
   and a written reason.

## Pre-Execution Review (Optional, Single-Reviewer)

A pre-execution review is OPTIONAL but recommended for substrate-impacting
dispatches (anything touching `crates/**`, `kernel/**`, `runtime/**`,
`editor/**`, ADRs, architecture lints, or any tracked production source).
The post-execution review at `Dispatch Lifecycle` step 3 above is the only
REQUIRED review.

If a pre-execution review is used:

1. **Exactly one reviewer.** Multiple pre-execution review packets are
   wasteful and explicitly discouraged. Choose a model that is NOT the
   Executor for the dispatch (cross-model second-opinion). For the
   current Planner=Codex / Executor=Claude working setup, the pre-exec
   reviewer is `Reviewer / OpenAI Codex` (same model as Planner, but a
   distinct role-packet).

2. **The Executor consumes the pre-exec review** — it does NOT write a
   duplicate "Reviewer2" / second-opinion approval packet when
   concurring. Empirical lesson from the ROLEFLOW + MAIN-RENDER
   dispatch series (2026-05-13 / 2026-05-14): duplicate pre-execution
   approval packets added no signal beyond the first reviewer's
   `APPROVE`.

3. **Concurring path**: If the Executor concurs with the pre-exec review
   and has no critique, the `EXECUTION_REPORT` notes in its
   `Pre-Execution Review Consumed` section: "Pre-execution review
   consumed; no additional pre-exec critique. Proceeded to execution."
   No duplicate `REVIEW_REPORT` is written.

4. **Critique path**: If the Executor finds a real issue before
   executing, it MUST halt. Halting means writing an `EXECUTION_REPORT`
   with `STATUS: BLOCKED`, `HANDOFF_STATUS: BLOCKED`, and
   `NEXT_ROLE: PLANNER_AI` (or `STATUS: NEEDS_HUMAN`,
   `HANDOFF_STATUS: NEEDS_HUMAN`, and `NEXT_ROLE: HUMAN_ARBITER` if
   explicit arbitration is required). The critique goes in the `Open
   Questions for Reviewer` and `Deviations from Task Packet` sections of
   the `EXECUTION_REPORT`. The Planner then issues a `CORRECTION_PACKET`
   (or re-plans via a new `TASK_PACKET`). The Executor does NOT write a
   duplicate `REVIEW_REPORT` to express disagreement; the
   `EXECUTION_REPORT`'s footer-status carries the signal.

5. **Watcher behaviour**: filesystem watchers polling for dispatch
   readiness MUST wait for the `EXECUTION_REPORT` after the single
   pre-exec review, NOT for an additional duplicate approval packet
   from the Executor.

## Correction Loop

Corrections require Planner approval. Reviewer findings are recommendations,
not direct execution orders. The Planner:

- Decides which review findings to act on this round.
- Decides which findings to defer (record as latent risks).
- Writes the `CORRECTION_PACKET` with the approved subset enumerated.
- The Executor acts ONLY on the corrections enumerated in that packet — not
  on the raw `REVIEW_REPORT`.

This protects against reviewer-loop runaway, prevents scope creep, and keeps
the Planner accountable for what ships.

## File Naming Convention

```
ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md
```

Where:

- `<DISPATCH_ID>`: a stable identifier for the dispatch, chosen by the
  Planner. Recommended forms:
  - Date-letter: `2026-05-13-A`, `2026-05-13-B` (multiple dispatches per day).
  - Chapter-suffix: `phase7-fillet-sub-eta`,
    `phase8-loft-curvature`.
- `<PACKET_TYPE>`: one of `TASK`, `EXEC`, `REVIEW`, `CORRECT`, `CLOSEOUT`.
- `<TIMESTAMP>`: ISO-8601 local form `YYYY-MM-DD_HH-MM-SS+TZTZ`
  (e.g. `2026-05-13_13-00-00+0300`).

### Examples

A clean single-round dispatch:

```
2026-05-13-A_TASK_2026-05-13_13-00-00+0300.md
2026-05-13-A_EXEC_2026-05-13_14-30-00+0300.md
2026-05-13-A_REVIEW_2026-05-13_14-45-00+0300.md
2026-05-13-A_CLOSEOUT_2026-05-13_15-00-00+0300.md
```

A dispatch with one correction round:

```
2026-05-13-B_TASK_2026-05-13_13-00-00+0300.md
2026-05-13-B_EXEC_2026-05-13_14-30-00+0300.md
2026-05-13-B_REVIEW_2026-05-13_14-45-00+0300.md
2026-05-13-B_CORRECT_2026-05-13_15-00-00+0300.md
2026-05-13-B_EXEC_2026-05-13_16-00-00+0300.md
2026-05-13-B_REVIEW_2026-05-13_16-15-00+0300.md
2026-05-13-B_CLOSEOUT_2026-05-13_16-30-00+0300.md
```

Multiple correction rounds are allowed; each `CORRECT` packet gets its own
unique timestamp, and the `EXEC` / `REVIEW` packets follow naturally.

## Required Markdown Sections

Every packet — regardless of type — MUST contain at minimum:

- `DISPATCH_ID`
- `AUTHOR` (role + AI identity, e.g. `Planner / Claude`,
  `Executor / Codex`, `Reviewer / Claude`)
- `TIMESTAMP`
- `RELATED_FILES`
- `STATUS`

Plus packet-type-specific structured sections — see the templates in
`ai_handoffs/templates/`. Every packet MUST also end with the
machine-readable completion footer — see `Machine-Readable Completion
Footer` below.

## Machine-Readable Completion Footer

Every packet MUST end with a deterministic completion footer block, so the
next role (a polling watcher, a CI hook, or another AI agent) can detect
packet completion without inspecting chat-UI state or guessing from the
document body. Orchestration is filesystem-state-based, not chat-UI-based.

### Format

The footer is plain Markdown — two horizontal rules (`---`) wrapping five
key-value lines:

```text
---

HANDOFF_STATUS: <COMPLETE | FAILED | BLOCKED | NEEDS_HUMAN>
DISPATCH_ID: <same as the header>
AUTHOR: <same as the header>
NEXT_ROLE: <role the recipient should fulfill next>
EXIT_CODE: <0 for success; non-zero for failure>

---
```

The footer MUST be the last non-empty content in the file. No prose, no
additional sections, and no signature blocks may follow it. The Markdown
horizontal rules surrounding the block are part of the standard.

### `HANDOFF_STATUS` Values

- `COMPLETE` — packet is fully written and ready to consume.
- `FAILED` — the author attempted the role and failed; the next role
  should read the body for the failure mode before retrying or routing.
- `BLOCKED` — the author is waiting on input from the next role (e.g.
  clarification, missing context, environment issue) before the dispatch
  can advance.
- `NEEDS_HUMAN` — explicit human arbitration is required; no AI role
  should proceed until the Human Arbiter records a decision (typically in
  a follow-up packet authored by the Planner).

### `NEXT_ROLE` Values

- `EXECUTOR_AI` — Executor should act on this packet next.
- `REVIEWER_AI` — Reviewer should act on this packet next.
- `PLANNER_AI` — Planner should decide the next step.
- `HUMAN_ARBITER` — User must intervene before any AI proceeds.
- `NONE` — terminal; no further packet is expected in this dispatch
  (typical for `FINAL_CLOSEOUT` with `STATUS: CLOSED` or `ABANDONED`).

### Polling Pattern

A downstream watcher detects packet completion with a line-anchored grep:

```bash
while ! grep -q "^HANDOFF_STATUS: COMPLETE$" ai_handoffs/<packet>.md; do
  sleep 2
done
```

Match the marker line-anchored so partial writes or earlier mentions in
the document body (e.g. quoting another packet's footer in prose) do not
produce false positives. The footer's placement at end-of-file also makes
detection robust against truncated reads.

### Why a Filesystem-Level Marker

Orchestrating multi-agent work through chat-UI state causes hanging loops,
partial reads, race conditions, duplicate reviews, and semantic drift —
these are classical distributed-system coordination problems. The
filesystem (specifically, the dispatch's Markdown packets in
`ai_handoffs/`) is the right protocol bus in v1. Future versions may add
structured JSON sidecars (e.g. `<packet>.meta.json` carrying the same
fields), but the Markdown footer is sufficient and authoritative for v1.

## Rules

### 1. Append-only

Each packet is a NEW file. Packets are never modified or replaced. The
dispatch's audit trail is the ordered concatenation of its packets.

### 2. Prior files must not be rewritten

Once a packet is written, it is immutable. Errors are corrected by writing a
NEW packet (e.g. a `CORRECTION_PACKET` or a follow-up `REVIEW_REPORT`), never
by editing the original. This preserves the chain-of-custody for every
decision.

### 3. No scope expansion without explicit approval

The Executor MUST stay within the `TASK_PACKET` `MAY edit` / `MUST NOT edit`
envelope. If new work is required, the Executor writes an `EXECUTION_REPORT`
noting the gap in `Deviations from Task Packet` or `Open Questions for
Reviewer`, and the Planner issues a new `TASK_PACKET` or
`CORRECTION_PACKET`. Silent scope expansion violates the protocol.

### 4. Correction packets require Planner approval

Reviewer findings flow through the Planner. The Reviewer recommends; the
Planner decides. `CORRECTION_PACKET` files explicitly enumerate which review
findings are approved for execution and which are deferred as latent risks.
Without an approved `CORRECTION_PACKET`, no correction is performed.

### 5. Final closeout must include tests/gates and remaining risks

`FINAL_CLOSEOUT` is the only packet that closes a dispatch. It MUST include:

- All verification gates run and their results.
- Test-count delta (workspace + per-crate).
- Final commit hash(es).
- Remaining risks, explicitly enumerated (or `none known` if truly none).
- Suggested follow-on tasks (or `none` if truly none).

A `FINAL_CLOSEOUT` that omits any of these is invalid and the dispatch is
not considered closed.

### 6. Every packet must end with the machine-readable completion footer

See `Machine-Readable Completion Footer` above. The footer is mandatory.
A packet missing the footer is invalid, regardless of how complete its
body looks. Downstream polling MUST gate on the footer line
(`^HANDOFF_STATUS: <value>$`), not on the body.

### 7. Pre-execution review is single-reviewer; no duplicate rubber-stamp

If a dispatch has a pre-execution review (optional but recommended for
substrate-impacting work — see `Pre-Execution Review (Optional,
Single-Reviewer)` above), exactly one reviewer writes it, typically the
model that is NOT the Executor. The Executor does NOT write a duplicate
second-opinion approval packet. If the Executor disagrees with the
pre-execution review, it halts via the `EXECUTION_REPORT`'s header and
footer (`STATUS: BLOCKED`, `HANDOFF_STATUS: BLOCKED`,
`NEXT_ROLE: PLANNER_AI`; or `NEEDS_HUMAN` / `HUMAN_ARBITER` when human
arbitration is required) and the `Open Questions for Reviewer` body
section — never via a duplicate `REVIEW_REPORT` packet.

This rule encodes the v2 simplification adopted 2026-05-14 after the
ROLEFLOW + MAIN-RENDER dispatch series demonstrated that duplicate
pre-execution `REVIEW_REPORT` packets from the Executor added no signal
beyond the first reviewer's `APPROVE`.

## Repository Layout

```
ai_handoffs/
  AI_HANDOFF_PROTOCOL.md   # this file
  templates/
    TASK_PACKET.md
    EXECUTION_REPORT.md
    REVIEW_REPORT.md
    CORRECTION_PACKET.md
    FINAL_CLOSEOUT.md
  <DISPATCH_ID>_TASK_<TIMESTAMP>.md
  <DISPATCH_ID>_EXEC_<TIMESTAMP>.md
  ...
```

Handoff packet files are typically left untracked (consistent with the
existing precedent at the repo root) unless the Planner explicitly stages
them for audit-trail commits. Either choice is acceptable; the protocol
does not mandate one.

## Relationship to Existing Precedent

This protocol formalizes the practice established by these files at the
repository root (created 2026-05-13):

- `OPENAItoCLAUDE_2026-05-13_12-15-18+0300.md` — Codex → Claude review packet
  after the sub-epsilon hardening commit.
- `CLAUDEtoOPENAI_2026-05-13_12-22-26+0300.md` — Claude → Codex review-of-
  review.
- `CLAUDE_SUB_EPSILON_REVIEW.md` — Codex-prepared review packet for
  Claude's sub-epsilon review.

Those files remain valid historical records and are NOT migrated into
`ai_handoffs/`. Going forward, new dispatches use the structured packet
format in `ai_handoffs/`.

## Non-Goals

This is a GOVERNANCE protocol. It is NOT:

- CI integration.
- Automated packet routing or scheduling.
- A machine-readable dispatch state-machine.
- A replacement for ADRs, `Status.md`, `HANDOFF.md`, or `change.md`.
- Enforcement beyond Reviewer + Planner discipline + human arbitration.

The protocol exists to make multi-AI dispatch auditable and recoverable.
It does not replace any existing governance surface and it does not encode
any architectural doctrine beyond the rules above.
