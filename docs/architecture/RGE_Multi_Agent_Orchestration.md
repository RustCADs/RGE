# RGE Multi-Agent Orchestration Discussion

## Context

A workflow split emerged between two AI roles:

1. Decision / Governance AI
2. Execution AI

The workflow stalled after both agents entered a passive waiting state.

---

# Observed Sequence

## Decision AI

The Decision AI concluded:

- next practical step:
  - Phase 3.3–3.4 bench gate inspection
- repo state:
  - pushed
  - clean
  - lints green
  - tests passing
- recommendation:
  - perform read-only inspection of benchmark gates

But then ended with:

```text
wait for explicit go
```

---

## Execution AI

Execution AI inherited the state and responded with:

```text
Holding until explicit go.
```

---

# Architectural Observation

This was not a coding failure.

It was a:

- governance failure
- orchestration failure
- transition-authority failure

The system lacked a component responsible for:

- advancing workflow state
- authorizing bounded continuation
- converting recommendations into execution contracts

---

# Root Cause

Both agents optimized for:

- safety
- non-overstepping
- bounded authority

Result:

```text
Decision AI:
"I recommend X."

Execution AI:
"I will wait."

Outcome:
No actor possessed continuation authority.
```

---

# Systems Interpretation

This resembles:

- leader election without commit
- orchestration without scheduler tick
- FSM transition missing trigger edge
- authority graph with no active executor

---

# Important Insight

The workflow accidentally exposed a hidden architectural requirement:

## RGE needs an orchestration layer

Not merely:

```text
Decision AI ↔ Execution AI
```

But:

```text
Governance Layer
        ↓
Scheduler / Orchestrator
        ↓
Execution Operators
```

---

# Correct Architectural Separation

## 1. Decision AI

Responsibilities:

- doctrine
- bounded planning
- architecture safety
- semantic authority
- state evaluation

Does NOT:

- directly execute
- own transition authority

---

## 2. Orchestrator

Responsibilities:

- transition authority
- workflow advancement
- execution triggering
- blocked-state handling
- retry/escalation
- contract dispatching

This is the missing layer.

---

## 3. Execution AI

Responsibilities:

- perform bounded work
- obey execution contract
- avoid scope expansion
- return deterministic output

Does NOT:

- redesign doctrine
- self-expand mission
- stall indefinitely

---

# Proposed Protocol

## Mandatory State Labels

Every Decision AI output must end with:

```text
NEXT_ACTION: EXECUTE
NEXT_ACTION: WAIT_FOR_USER
NEXT_ACTION: ASK_CLARIFICATION
NEXT_ACTION: STOP
NEXT_ACTION: BLOCKED
```

---

# Example Correct Dispatch

```text
NEXT_ACTION: EXECUTE

TASK:
Read-only inspect Phase 3.3–3.4.

SCOPE:
- find stated gates in IMPLEMENTATION.md
- enumerate current bench coverage
- locate last recorded numbers
- decide whether next step is:
  A) run + record
  B) add missing gate + record

FORBIDDEN:
- no code edits
- no benchmark execution
- no git mutation
- no cleanup
- no doctrine rewrite

OUTPUT:
Inspection report only.
```

---

# Key Rule

## Read-only bounded tasks auto-execute

They do NOT require explicit user approval.

---

# Explicit GO Required Only For

Tasks that:

- mutate code
- change repo state
- alter doctrine
- run destructive operations
- perform expensive runtime operations
- modify benchmarks/results

---

# Replacement For Passive Waiting

Instead of:

```text
Holding until explicit go.
```

Agents must emit explicit operational states:

```text
EXECUTING
WAITING
BLOCKED
DONE
```

---

# Minimal Viable Orchestration Doctrine

## Suggested File

```text
RGE_AGENT_ORCHESTRATION.md
```

---

# Initial Doctrine

```text
Decision AI produces:
- EXECUTE
- WAIT_FOR_USER
- BLOCKED
- STOP

Read-only bounded tasks auto-execute.

Mutating tasks require explicit GO.

Execution AI follows only the contract.

Orchestrator owns transition authority.
```

---

# Suggested Orchestrator Prompt

```text
You are the RGE Orchestrator.

Your job is not to design or implement.
Your job is to move work between Decision AI and Execution AI.

Rules:
1. If Decision AI emits NEXT_ACTION: EXECUTE,
   convert it into an execution contract and start Execution AI.

2. If Decision AI emits NEXT_ACTION: WAIT_FOR_USER,
   stop and ask the user for authorization.

3. If task is read-only, bounded, and non-destructive,
   do not wait for user confirmation.

4. If task mutates:
   - code
   - schema
   - repo state
   - doctrine
   - benchmark records
   require explicit authorization unless already granted.

5. Execution AI may only do the contract.

6. After execution finishes,
   return result to Decision AI for validation.
```

---

# Suggested Execution AI Prompt

```text
You are the RGE Execution Operator.

Execute only the given contract.

You may:
- inspect files
- summarize evidence
- run commands only if explicitly allowed
- produce requested outputs

You may not:
- expand scope
- mutate doctrine
- mutate repo state unless allowed
- invent missing state
- wait if contract says EXECUTE

If blocked:
output BLOCKED with exact reason.
```

---

# Final Assessment

## Is the problem solvable?

Yes.

The issue is not model capability.

The issue is missing workflow protocol.

---

# Recommended Immediate Action

Introduce orchestration doctrine now while the repo is:

- clean
- coherent
- checkpointed

Do NOT overbuild yet.

Start as:

- markdown doctrine only
- manual orchestration
- bounded execution contracts

Then test the protocol during:

```text
Phase 3.3–3.4 read-only inspect
```

---

# Important Architectural Insight

This event strongly suggests RGE is evolving beyond a simple codebase and toward:

```text
semantic substrate
+
governance system
+
execution topology
```

The deadlock was effectively an early manifestation of:

- transition-authority gaps
- semantic workflow orchestration requirements
- distributed governance behavior

This is likely a foundational future concern for:

- benchmark systems
- runtime orchestration
- semantic operators
- validation pipelines
- finance/ERP substrate layers
- geometry execution graphs
- deterministic AI workflows
- distributed semantic authority systems

---

# Protocol

The sections above are the discussion that led to this protocol. This
section is the codified version: a settled reference, not a proposal.
When the discussion and the protocol disagree, the protocol wins.

## Roles

Three roles, sharply separated:

1. **Decision AI (Governance)** — owns doctrine, bounded planning,
   architecture safety, semantic authority, state evaluation. Does NOT
   directly execute. Does NOT own transition authority.
2. **Orchestrator** — owns transition authority, workflow advancement,
   execution triggering, blocked-state handling, contract dispatching.
   Does NOT design or implement. May be the user, an AI in the role,
   or a manual operator.
3. **Execution AI** — performs bounded work, obeys the contract,
   returns deterministic output. Does NOT redesign doctrine, self-
   expand mission, or stall indefinitely.

## Transition Authority

The Orchestrator alone advances workflow state. Decision AI emits
recommendations; Execution AI emits results. Neither role advances
state on its own.

## NEXT_ACTION Labels

Decision AI output ends with exactly one of:

| Label                          | Meaning                                                              |
|--------------------------------|----------------------------------------------------------------------|
| `NEXT_ACTION: EXECUTE`         | Recommend immediate execution; Orchestrator dispatches               |
| `NEXT_ACTION: WAIT_FOR_USER`   | Recommend execution but require explicit user authorization          |
| `NEXT_ACTION: ASK_CLARIFICATION` | Cannot recommend without resolving a question                      |
| `NEXT_ACTION: STOP`            | Reached a coherent stopping point; no further action recommended     |
| `NEXT_ACTION: BLOCKED`         | Cannot proceed; state the blocker                                    |

## Auto-Execute Rule

Read-only bounded tasks auto-execute. The Orchestrator does NOT
require explicit user authorization for tasks that:

- Read files, run queries, summarize evidence
- Cannot mutate code, repo state, doctrine, or recorded benchmark numbers
- Cannot trigger destructive or expensive operations

## Explicit Authorization Required For

Tasks that:

- Mutate code, configs, or schemas
- Change repo state (commits, pushes, merges)
- Alter doctrine (ADRs, PLAN, IMPLEMENTATION, governance docs)
- Run destructive operations (force-push, rebase, history rewrite,
  file deletion outside tooling)
- Perform expensive runtime operations (full bench runs, soak tests,
  long-running validations)
- Modify benchmark records or recorded gate numbers

## Bounded Execution Contract

The Orchestrator dispatches Execution AI with a contract of this shape:

```text
NEXT_ACTION: EXECUTE

TASK:
<one-sentence statement of the goal>

COMMAND BUDGET:
<numbered list of allowed commands / files / actions>

ALLOWED FILES:
<explicit list — only relevant for mutating tasks>

FORBIDDEN:
<explicit list — e.g. source changes, Cargo changes,
untracked items, scope expansion>

STOP IF:
<conditions that must abort execution before completion>

OUTPUT:
<the exact shape of the deterministic output the Orchestrator expects>
```

The contract is the entire mandate. Execution AI executes the contract
verbatim — no more, no less.

## Execution Continuation Rule

When a contract says EXECUTE, Execution AI proceeds immediately. It
does NOT wait for additional confirmation. Waiting in the presence of
explicit bounded authority is a protocol violation — the original
deadlock that motivated this protocol.

If a STOP IF condition fires mid-execution, Execution AI halts and
reports the condition. It does NOT silently proceed.

## Agent States

Replace passive "Holding until explicit go" with explicit operational
states:

| State        | Meaning                                                  |
|--------------|----------------------------------------------------------|
| `EXECUTING`  | Actively performing contract work                        |
| `WAITING`    | Contract returned to Orchestrator; awaiting next contract |
| `BLOCKED`    | STOP IF triggered or unresolvable condition reached      |
| `DONE`       | Contract complete; deterministic output returned         |

`BLOCKED` outputs the exact reason. Silent waiting is not an option.

## Scope Discipline

This protocol is markdown-only. It does NOT introduce:

- Code automation, enforcement, or parsing
- New lints, CI gates, or runtime checks
- Workflow engines, schedulers, or executors

The protocol is manual. The Orchestrator is a human or an AI in the
role; contracts are written in conversation; states are observed in
conversation. Future automation may layer on top, but this document
fixes only the manual operating contract.
