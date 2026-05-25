# AI Dispatch Automation — Improvements Plan

> Source: empirical observations from 2026-05-25 watch session (5 dispatches: #166, #168 (BLOCKED), #169, #171, #173, #175). This file is a queue of dispatch-ready improvement tasks intended for the Codex planner. Each section is shaped to become one GitHub issue with the `ai-dispatch` + `ai-auto` labels.

## Background — what was observed

In 5 sequential dispatches over ~3.5 hours:

- Total wall-clock: ~3h25m for 4 successful publishes (+ 1 BLOCKED reroute).
- Sustained throughput: ~1.2 tasks/hour.
- **Single biggest waste: 15-21 min "publish lag"** between Codex `pass` verdict and `git push origin main`. Across the 5 dispatches that's ~85 minutes of pure dead time — ~40% of total wall-clock.
- **No parallel execution**: per [`AI_DISPATCH_PARALLEL.md`](AI_DISPATCH_PARALLEL.md) the design is documented but the FanOut runner script does not exist on disk.
- **Scheduled tasks `RGE-AiDispatch` and `RGE-AiDispatchQueue` are both Disabled** in Windows Task Scheduler — automation only runs when a human is actively driving an interactive shell.
- **BLOCKED tasks require human routing**: ISSUE-168 surfaced a kernel/ecs sparse-column gap, halted cleanly, but the user had to manually close it, queue the kernel fix (#169), then re-queue the loader retry (#171). The queue has no `depends-on:` awareness.
- **No early-failure prediction**: ISSUE-168 burned ~30 minutes of Codex plan + Claude execute before halting. A cheap preflight could have flagged the kernel assumption mismatch in seconds.

## Sequencing

Do **Task 1** first (root-cause publish lag — informs Task 2). Then Task 2, then Task 3 in parallel with Task 5 (independent). Task 4 (FanOut) is the multiplier — do it after 1+2 ship so the parallel pipeline benefits from the publish fix from day one.

| Order | Task | Est wall-clock |
|---|---|---|
| 1 | Diagnose publish lag (instrument + report) | ~45 min |
| 2 | Fast-publish path (apply the fix) | ~45 min |
| 3 | Re-enable Windows scheduled tasks | ~15 min |
| 4 | FanOut runner (per AI_DISPATCH_PARALLEL.md) | 5-7 hours, 3-4 dispatches |
| 5 | Pre-flight substrate-readiness check | ~45 min |
| 6 | Dependency-aware queue (`depends-on:`) | ~45 min |
| 7 | Fix spurious TaskCreate reminder hook | ~15 min |

---

## Task 1 — Diagnose the publish lag (instrumentation pass)

**Issue title**: `Instrument queue publish path to root-cause the 15-21min ff-merge lag`

**Background**:
After Codex emits `verdict: pass`, `commit_readiness: ready_for_publish`, the queue still takes 15-21 minutes to perform `git merge --ff-only ai-dispatch/ISSUE-N` + `git push origin main` + close the GitHub issue. Empirical data from 2026-05-25:

- ISSUE-166: ~17 min from verdict to main advance
- ISSUE-169: ~21 min  
- ISSUE-171: ~20 min
- ISSUE-173: ~19 min

Root cause unknown. Suspected: a `Wait-GitHubActions`-style poll loop with a long interval, or a `Start-Sleep` between publish phases, or the queue runner's interactive-confirmation guard firing without user input.

**Scope**: Read-only diagnosis + targeted instrumentation. No behavioral change.

**MAY edit**:
- `Invoke-AiDispatchQueue.ps1` — add timestamped `Write-Host "[stage] $(Get-Date -Format 'HH:mm:ss.fff') start: <phase>"` log lines around the publish path (after Codex pass verdict, before/after CI poll, before/after ff-merge, before/after push, before/after issue close).
- `Wait-GitHubActions.ps1` if it has its own internal sleep loop — add the same timestamped trace.

**MUST NOT edit**:
- `Invoke-AiDispatchLoop.ps1` (orchestrator — outside scope of this dispatch)
- `Invoke-AiDispatchAuto.ps1`
- `Register-AiDispatchSchedule.ps1`
- Any Rust workspace file (`crates/**`, `kernel/**`, `tools/**`)
- `Cargo.toml`, `Cargo.lock`
- `exemptions.toml`

**Acceptance**:
- After this dispatch ships, the next real dispatch's queue-runner stdout/log shows phase-by-phase timestamps that account for the entire 15-21 min publish window.
- Include a short report (in the EXEC packet) of what each phase actually does so the followup Task 2 can choose the right fix surgically.

**Halt conditions**:
- If the instrumentation reveals the lag is on the GitHub API side (rate-limiting, webhook delay) rather than local — halt and surface that finding; Task 2's design will differ.
- If adding trace lines requires changing the orchestrator's contract with Codex/Claude — halt.

**Verification**:
- `cargo run -q -p rge-tool-architecture-lints -- all` — exit 0 (should be unaffected; PS scripts aren't in scope).
- `cargo +nightly fmt --check` — exit 0.
- `.ai/dispatch.verify.ps1` — exit 0.
- Manual: run a no-op dispatch with the instrumented queue against a throwaway issue, confirm timestamps land in the log.

---

## Task 2 — Fast-publish path (apply the fix)

**Issue title**: `Implement fast-publish path: cut publish lag from ~17min to <2min`

**Depends on**: Task 1's diagnostic report.

**Background**:
Today's queue waits ~17 min between Codex `pass` and `git push origin main`. Since `.ai/dispatch.verify.ps1` already ran the canonical gate locally (all 7 verification steps green) **before** Codex even sees the EXEC packet, the publish path's CI-poll is redundant — it's verifying the same gate twice.

Two viable approaches; Task 1's report should make the choice obvious:

**Approach A — drop the CI gate** (if Task 1 confirms the lag is a `Wait-GitHubActions` poll): trust the local verify, ff-merge + push immediately on Codex pass.

**Approach B — shorten the poll interval** (if Task 1 finds the lag is in a `Start-Sleep -Seconds 60` loop that polls CI): drop poll interval to 5-10s, since CI typically finishes within seconds-to-minutes of branch push.

**Both approaches must preserve**: ability to roll back (`git reset --hard` of main + force-push) if publish fails or CI later goes red after we've already merged.

**MAY edit**:
- `Invoke-AiDispatchQueue.ps1` (the publish-step body only)
- `Wait-GitHubActions.ps1` if Approach B

**MUST NOT edit**:
- `Invoke-AiDispatchLoop.ps1`
- Anything outside the queue runner's publish path
- Architecture lint configs, exemptions, kernel code

**Acceptance**:
- Two real dispatches after this one publish in <3 min from `verdict: pass`.
- Rollback path tested manually: induce a failure mid-publish, verify the queue cleans up correctly (branch kept local, issue labelled `ai-dispatch-failed`).

**Halt conditions**:
- If the redundant CI gate turns out to catch something `dispatch.verify.ps1` doesn't (e.g., a workflow that runs only in GitHub Actions environment) — halt and document the difference; do not silently drop a real check.

**Verification**: same gates as Task 1.

---

## Task 3 — Re-enable Windows scheduled tasks for unattended dispatch

**Issue title**: `Re-enable RGE-AiDispatch* scheduled tasks + verify unattended dispatch works`

**Background**:
Today both `RGE-AiDispatch` and `RGE-AiDispatchQueue` Windows Scheduled Tasks are in state `Disabled`. Last run was 2026-05-23 / 2026-05-18 respectively. Today's 5 dispatches all ran from an interactive shell session driven by a human. Once Tasks 1+2 ship, re-enabling the scheduler should give ~25-30 free dispatches per overnight window.

**Scope**:
1. Confirm `Register-AiDispatchSchedule.ps1` still produces a valid task definition against today's automation scripts (no drift since the last successful unattended run).
2. Verify dispatch can complete end-to-end **without an interactive shell** (no prompts, no `Read-Host`, no `Get-Credential`). Today's interactive sessions may have papered over a missing piece.
3. Re-enable both tasks via `Enable-ScheduledTask`.
4. Trigger a manual one-shot run of `RGE-AiDispatchQueue` against a trivial canary issue to confirm clean end-to-end behavior.

**MAY edit**:
- `Register-AiDispatchSchedule.ps1` (only if drift requires it)
- `Invoke-AiDispatchAuto.ps1` (only if it has interactive guards that block unattended)

**MUST NOT edit**:
- Rust workspace
- The dispatch loop's core protocol contract

**Acceptance**:
- Both scheduled tasks state = `Ready`.
- One canary task completes end-to-end while no interactive shell is attached (run via `Start-ScheduledTask -TaskName RGE-AiDispatchQueue` and observe).
- `gh issue view <canary>` shows CLOSED + `ai-dispatch-done`.
- New entry in `ai_dispatch_logs/`.

**Halt conditions**:
- If the scheduler runs in a context that can't auth `gh` (different user / session) — halt; this is a real prerequisite gap requiring a separate dispatch.
- If the dispatch loop hangs on stdin / a credential prompt — halt and surface the specific prompt.

**Verification**: queue-side verification gates (no Rust gates needed).

---

## Task 4 — FanOut runner (parallel-dispatch fan-out)

**Issue title**: `Implement Invoke-AiDispatchFanOut.ps1 per AI_DISPATCH_PARALLEL.md`

**Depends on**: Tasks 1+2 (so the parallel pipeline isn't multiplying a 17-min serial bottleneck).

**Background**: full design lives in [`AI_DISPATCH_PARALLEL.md`](AI_DISPATCH_PARALLEL.md) §3-§7. The doc is comprehensive — read it first. Summary: each parallel dispatch gets its own `git worktree` so independent runs don't clobber each other's working trees. Infrastructure (orchestrator scripts + ai_handoffs templates) is copied into each worktree per §4 Option B.

**Decomposition** (split across 3-4 dispatches):

### Task 4.1 — FanOut v0 (sequential mode behind a flag)

**Scope**: write `Invoke-AiDispatchFanOut.ps1` that:
- Takes a list of task issue numbers.
- Creates one worktree per task under `..\dispatch-worktrees\FANOUT-<N>` off current `HEAD`.
- Copies infrastructure scripts + `ai_handoffs/templates/` into each worktree (Option B).
- Launches `Invoke-AiDispatchLoop.ps1` once per worktree, **sequentially** in v0 (concurrency comes in 4.2).
- On completion, leaves the worktree intact for human review (no merge, no push).

**MAY edit**:
- New file `Invoke-AiDispatchFanOut.ps1`
- Minor flag addition to `Invoke-AiDispatchLoop.ps1` to accept an explicit `-WorkingDir` parameter (currently uses `git rev-parse --show-toplevel`).

**MUST NOT edit**:
- The dispatch protocol contract
- Queue runner publish path (separate concern)

### Task 4.2 — Concurrency throttle

**Scope**: add `-MaxConcurrency N` to FanOut. Use a PowerShell job/runspace pool. Throttle launches so at most N `Invoke-AiDispatchLoop.ps1` instances run at once. Recommended default N=2 for initial validation.

### Task 4.3 — Worktree cleanup + failure handling

**Scope**: 
- On `Ctrl+C` or fatal error, run `git worktree remove` for each spawned worktree (preserving any with uncommitted human work — detect via `git status` non-empty).
- Aggregate per-worktree exit codes into a single summary report.
- Surface any worktree that hit STATUS=BLOCKED with a 1-line summary + EXEC packet path for human inspection.

### Task 4.4 — Canary validation

**Scope**: queue 2-3 trivial-disjoint canary tasks (e.g., "add doctest to crate A" + "add doctest to crate B" + "bump trailing newline in crate C"). Run them through FanOut with `-MaxConcurrency 2`. Verify all complete with worktrees intact, no cross-talk on `ai_handoffs/` packet writes, no shared-target-dir cargo races.

**Halt conditions across 4.1-4.4**:
- If a parallel run surfaces a substrate race (e.g., `ai_handoffs/` two-writer interleave) — halt and route to a substrate dispatch.
- If `git worktree add` produces unexpected output on Windows that breaks the script — halt and document.

---

## Task 5 — Pre-flight substrate-readiness check

**Issue title**: `Add preflight that catches BLOCKED-by-substrate-gap dispatches before Codex plan runs`

**Background**:
ISSUE-168 today wasted ~30 minutes (Codex plan + Claude execute + halt) before halting with "kernel/ecs catch-all archetype assumes dense columns; my heterogeneous insert pattern can't proceed without kernel changes that are out of scope". A 30-second preflight could have caught this.

**Scope**: 
- Pre-Codex-plan step that reads the TASK packet's `MAY edit` + `MUST NOT edit` lists.
- Runs `cargo check -p <each crate that owns a touched file>` against current main.
- Checks a small hand-maintained list of "known substrate gaps" (initially: kernel/ecs sparse-column assumption; expandable as more gaps surface) and flags if the TASK touches a code path that hits one.
- If flagged: halts the dispatch with `STATUS: PREFLIGHT_BLOCKED` and a route-to-substrate hint, before burning any Codex/Claude cycles.

**MAY edit**:
- New file `Invoke-AiDispatchPreflight.ps1`
- Hook into `Invoke-AiDispatchLoop.ps1` (call preflight before Codex plan)
- New file `docs/dispatch/known-substrate-gaps.md` (the hand-maintained list)

**MUST NOT edit**:
- Rust workspace (this is pure tooling)
- Codex/Claude prompts
- The TASK packet schema

**Acceptance**:
- A test issue with a TASK explicitly touching the `Component` insertion path with `kernel/**` MUST-NOT — preflight halts within 30s with the route-to-substrate hint.
- A normal dispatch (no flagged path) — preflight passes through transparently in <5s.

---

## Task 6 — Dependency-aware queue (`depends-on:`)

**Issue title**: `Queue should honor depends-on: #N labels and skip tasks with unmet prerequisites`

**Background**:
ISSUE-171 (loader retry) depended on ISSUE-169 (kernel sparse-column fix). Today the user manually re-queued #171 after #169 closed. If the queue had run them blindly in parallel — or even sequentially without dependency awareness — #171 would have hit the same kernel gap as #168 and BLOCKED again, wasting another ~30 minutes.

**Scope**:
- Extend `Invoke-AiDispatchQueue.ps1` issue-selection logic.
- When selecting the next ai-dispatch issue, parse the issue body for a `depends-on: #N` line (also accept `depends-on: #N, #M, #P` comma-separated).
- Skip an issue if any of its `depends-on:` targets is not yet `CLOSED`.
- If all candidate issues are blocked by unmet dependencies, queue idles (instead of failing).

**MAY edit**:
- `Invoke-AiDispatchQueue.ps1`
- `Invoke-AiDispatchAuto.ps1` (the autonomous task selector — same logic)

**MUST NOT edit**:
- The orchestrator
- Issue templates (this should be opt-in; if no `depends-on:` line, current behavior preserved)

**Acceptance**:
- Two test issues: A and B with `depends-on: #<A>`. Queue runs A first, only proceeds to B after A closes.
- An issue with `depends-on:` pointing to a CLOSED issue — proceeds normally.
- An issue with `depends-on:` pointing to a nonexistent issue — halts with a clear error.

---

## Task 7 — Disable spurious TaskCreate reminder hook in watch-only sessions

**Issue title**: `Suppress TaskCreate reminders in read-only/polling Claude Code sessions`

**Background**: Today's watch loop (every 3 minutes for ~3.5 hours) received the `<system-reminder>The task tools haven't been used recently...` message ~12 times. The reminder is irrelevant for polling sessions and adds cognitive overhead. The hook firing it is likely in `~/.claude/settings.json`.

**Scope**:
- Read user's Claude settings.json (path varies; likely `C:\Users\halil\.claude\settings.json`).
- Locate the hook that emits the TaskCreate reminder.
- Add a condition: skip reminder when the most recent N tool calls are all `Bash` / `PowerShell` polling commands against `.ai/dispatch-*` or `ai_dispatch_logs/`.
- Test by running a short watch loop and confirming the reminder no longer fires.

**MAY edit**:
- `~/.claude/settings.json` (and possibly `~/.claude/settings.local.json`)

**MUST NOT edit**:
- Project files
- Other developers' settings

**Acceptance**:
- Run a 6-tick watch loop on the dispatch automation. Verify zero TaskCreate reminders fire.
- Run a 2-tool-call code-edit session (different work pattern). Verify the reminder still fires when genuinely useful.

---

## How to enqueue these

Each Task N above is shaped to become one GitHub issue:

```powershell
# Example for Task 1:
gh -R RustCADs/RGE issue create `
  --title "Instrument queue publish path to root-cause the 15-21min ff-merge lag" `
  --body-file <body-file-from-Task-1-section> `
  --label "ai-dispatch,ai-auto"
```

Queue will pick them up in oldest-first order. Once Task 6 (dependency-aware) ships, later issues can use `depends-on: #N` to gate sequencing. Until then, queue them in the sequencing order from the table above and only one at a time has the `ai-dispatch` label.
