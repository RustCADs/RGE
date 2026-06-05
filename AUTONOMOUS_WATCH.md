# Autonomous dispatch with a Claude watch/safety-monitor

Status: **scaffold for review — NOT armed.** Build + dry-run only. The unattended
scheduler (`Register-AiDispatchSchedule.ps1`) is intentionally NOT registered by
this change; nothing runs live against RGE until an explicit operator go.

## Goal (owner-directed reconfiguration)

Run the dispatch automation **fully autonomously with Codex as the decision +
execution layer**, and put **Claude in a dedicated watch / safety-monitor role**
that records everything and can halt the run with a report. Concretely:

- **Codex** = plans + selects the next task + **executes** + control-reviews +
  **authorizes the merge** (auto-publish to `origin/main` on its own `pass`
  verdict). Codex is "the human" — no per-PR human review in the loop.
- **Claude** = **watch only**. It does not execute. It observes the run, records
  everything to a log, assesses for anomalies, and on a problem it **terminates
  the automation and writes a report file**. Claude is the independent
  cross-AI safety backstop (since Codex now reviews its own work, the Claude
  veto is the only non-Codex check).

This **reverses the human-only merge gate** the project used previously. The
safety net for that reversal is three-layered: (1) the §18 *delegated-human
auto-publish* bounds in `AI_DISPATCH_AUTOMATION.md` (capped autonomous-issue
count, etc.); (2) Codex's `pass` control verdict; (3) the **Claude watch veto**
defined here. Any one tripping blocks/halts the publish.

## What already exists (reused, not rebuilt)

- **Codex selects the next task** — `Invoke-AiDispatchAuto.ps1` (Codex picks from
  `.ai/dispatch.tasks.md` and runs it through the queue).
- **Auto-merge on pass** — `Invoke-AiDispatchQueue.ps1 -PublishMode main`
  (fast-forward + push `origin/main` + close the issue), bounded by §18.
- **A per-phase event log** — `.ai/dispatch-trace/*.jsonl`, emitted by the queue
  + autonomous drivers, aggregated by `Get-AiDispatchTrends.ps1`. This is a
  primary signal the watch layer consumes ("it may already exist" — it does).
- **A passive read-only run dashboard** — the existing `Watch-AiDispatch.ps1`, a
  human-facing terminal watcher that summarizes packets, run-dir JSON, the Codex
  control verdict, and a progress bar. It explicitly does NOT call claude/codex,
  git-write, or kill anything. This is the other half of the "watch … log file
  (it may already exist)" the owner referred to. The NEW active guard below is
  its **action-taking sibling, not a replacement** — the passive watcher keeps
  its read-only, non-participating contract intact.
- **A Codex stall watchdog + abort-on-`block`** inside `Invoke-AiDispatchLoop.ps1`.

## What this change adds

### 1. `Invoke-AiDispatchGuard.ps1` — the Claude active safety-guard (BUILT here)

The action-taking sibling of the passive `Watch-AiDispatch.ps1` dashboard: where
the dashboard only *displays*, the guard *records + decides + halts*. A supervisor
that wraps an autonomous dispatch run:

1. **Launch + capture.** Starts the autonomous driver as a child process, tee-ing
   its stdout/stderr to a watch log (`.ai/dispatch-watch/<run-id>/watch.log`) and
   mirroring structured events to `.ai/dispatch-watch/<run-id>/events.jsonl`. In
   `-DryRun` it drives a mock child instead, so the full watch path is exercised
   without touching the real loop.
2. **Record everything.** Every phase boundary (from the `.ai/dispatch-trace`
   JSONL + the child's stdout) is appended to the watch log with a timestamp and
   a monotonically-increasing sequence number.
3. **Assess.** At each checkpoint (phase boundary or `-AssessIntervalSec`), it
   invokes `claude -p` with the recent activity and a fixed rubric, requesting a
   structured verdict: `{ "verdict": "ok" | "abort", "reason": "..." }`. Every
   assessment (prompt digest + verdict + reason) is recorded.
4. **Hard rules (deterministic, no LLM).** Independently of the LLM, trip an
   immediate abort on: a git push to a protected ref outside the sanctioned
   publish flow; a verification-gate hard failure; a run exceeding
   `-MaxRunMinutes`; correction rounds exceeding `-MaxCorrectionRounds`; or a
   `block` / `needs_human` verdict in the trace. Hard rules are the floor; the
   LLM assessment is the ceiling.
5. **Terminate + report.** On any abort (LLM `abort` or a hard-rule trip), kill
   the child process **tree** (`taskkill /T /F`) and write
   `.ai/dispatch-watch/<run-id>/abort-report.md` — the trigger (rule id or LLM
   reason), the captured tail of the log, the last few trace events, and the
   recommended human follow-up. The watch log records the final disposition
   (`completed` | `aborted`) either way.

`-DryRun` (default for this scaffold) never launches the real driver, never
invokes `-PublishMode main`, and exercises the watch→record→assess→terminate→
report path against a scripted mock so the logic is verifiable offline.

### 2. `-Executor codex` for the loop (SPECCED here — NOT yet coded)

`Invoke-AiDispatchLoop.ps1`'s execute phase is hardcoded to Claude
(`Invoke-ClaudeExecute`, "You are Executor / Claude", Claude `--permission-mode`).
To make Codex the executor without disturbing the battle-tested default:

- Add an additive param `[ValidateSet('claude','codex')] [string]$Executor =
  'claude'` (default unchanged → existing behaviour byte-identical).
- Add `Invoke-CodexExecute` mirroring `Invoke-ClaudeExecute` but Codex-voiced
  ("You are Executor / Codex") and routed through the existing `Invoke-CodexPrompt`
  helper (so it reuses the Codex stall watchdog + run-dir capture). The execute
  call site branches on `$Executor`.
- `Invoke-AiDispatchAuto.ps1` passes `-Executor codex` when driving the fully-
  autonomous mode.

This is deferred to a follow-up so this PR stays additive + the loop's existing
tests stay green; the watch layer is independent of which AI executes (it
supervises the run either way).

## Dry-run (this change)

```
# Exercises the full watch→record→assess→terminate→report path against a mock
# child, writing the watch log + (on a simulated anomaly) an abort report. No
# real dispatch, no publish, no scheduler.
.\Invoke-AiDispatchGuard.ps1 -DryRun -DispatchId GUARD-DRYRUN                 # benign run completes
.\Invoke-AiDispatchGuard.ps1 -DryRun -DryRunOutcome hardrule -DispatchId GUARD-HR   # hard-rule abort + report
.\Invoke-AiDispatchGuard.ps1 -DryRun -DryRunOutcome llm-abort -DispatchId GUARD-LLM  # Claude-veto abort + report
```

## NOT done (explicit)

- The scheduler is NOT registered (`Register-AiDispatchSchedule.ps1` not run).
- `-PublishMode main` is NOT invoked by this change.
- The `-Executor codex` loop swap is specced, not coded.
- No live autonomous run against RGE.

Arming any of the above is a separate, explicit operator decision.
