#Requires -Version 5.1
<#
.SYNOPSIS
    Pull the next `ai-dispatch`-labelled GitHub issue and run it through the
    dispatch loop on a local branch, unattended.

.DESCRIPTION
    Work source : open GitHub issues labelled `ai-dispatch`, oldest first.
    Execution   : Invoke-AiDispatchLoop.ps1 on a per-issue branch
                  `ai-dispatch/ISSUE-<n>`, run as an isolated child process.
    Publish     : the dispatch's work is committed LOCALLY on that branch and
                  is never pushed. A human reviews and pushes/discards it.
    Bookkeeping : the issue is relabelled (running -> done, plus failed on a
                  non-zero loop exit) and a result comment is posted.

    Exactly one issue is processed per invocation. This is meant to be fired
    on a recurring schedule (e.g. Claude Code `/loop`). A temp-dir lock file
    prevents overlapping runs from colliding.

    Pre-existing untracked files (unrelated working-tree clutter) are parked
    with `git stash --include-untracked` for the duration of the run so the
    branch commit captures only the dispatch's own output, then restored.

.EXAMPLE
    .\Invoke-AiDispatchQueue.ps1 -DryRun
    # Report which issue would be picked next; mutate nothing.

.EXAMPLE
    .\Invoke-AiDispatchQueue.ps1
    # Process the oldest queued issue end to end.

.NOTES
    Requires local `git`, `gh` (authenticated), `codex`, `claude`,
    `powershell.exe`, and Invoke-AiDispatchLoop.ps1 in the repo root.
    Does not push. Human authorization remains required to publish a branch.
#>
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$QueueLabel = 'ai-dispatch',

    [ValidateRange(10, 1440)]
    [int]$StaleLockMinutes = 180,

    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

$script:LockPath = Join-Path $env:TEMP 'rge-ai-dispatch-queue.lock'
$script:LockHeld = $false

function Release-Lock {
    if ($script:LockHeld) {
        Remove-Item -LiteralPath $script:LockPath -Force -ErrorAction SilentlyContinue
        $script:LockHeld = $false
    }
}

function Fail {
    param([string]$Message)
    Release-Lock
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Finish {
    param([int]$Code = 0)
    Release-Lock
    exit $Code
}

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "Required command not found on PATH: $Name"
    }
}

function Write-Utf8 {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-Capture {
    # Run a native command with PS 5.1 EAP isolation (native stderr under
    # EAP=Stop becomes a terminating error). Merges stdout+stderr into $File.
    # Returns the process exit code.
    param([string]$File, [string]$Exe, [string[]]$CmdArgs)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @CmdArgs > $File 2>&1
    } finally {
        $ErrorActionPreference = $prev
    }
    return $LASTEXITCODE
}

function Invoke-Tool {
    # Invoke-Capture into a temp file, return [pscustomobject]@{ Code; Text }.
    param([string]$Exe, [string[]]$CmdArgs)
    $tmp = [System.IO.Path]::GetTempFileName()
    try {
        $code = Invoke-Capture -File $tmp -Exe $Exe -CmdArgs $CmdArgs
        $text = (Get-Content -Raw -LiteralPath $tmp -ErrorAction SilentlyContinue)
        # Get-Content -Raw yields $null for empty output; normalize to '' so
        # callers can safely call .Trim()/string ops on the result.
        if ($null -eq $text) { $text = '' }
        return [pscustomobject]@{ Code = $code; Text = $text }
    } finally {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    }
}

function Git-Step {
    # Run a git command; fail hard with captured output on non-zero exit.
    param([string[]]$CmdArgs)
    $r = Invoke-Tool -Exe 'git' -CmdArgs $CmdArgs
    if ($r.Code -ne 0) {
        Fail "git $($CmdArgs -join ' ') failed (exit $($r.Code)):`n$($r.Text)"
    }
    return $r.Text
}

# --- Environment -----------------------------------------------------------

$script:RepoRoot = $PSScriptRoot
Set-Location -LiteralPath $script:RepoRoot

Require-Command git
Require-Command gh
Require-Command codex
Require-Command claude
Require-Command powershell.exe

$loopScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchLoop.ps1'
if (-not (Test-Path -LiteralPath $loopScript)) {
    Fail "Dispatch loop script not found: $loopScript"
}

$auth = Invoke-Tool -Exe 'gh' -CmdArgs @('auth', 'status')
if ($auth.Code -ne 0) {
    Fail "gh is not authenticated. Run 'gh auth login' first.`n$($auth.Text)"
}

$originUrl = (Git-Step @('remote', 'get-url', 'origin')).Trim()
if ($originUrl -notmatch 'github\.com[:/](.+?)(?:\.git)?/?$') {
    Fail "Could not parse an owner/name slug from origin URL: $originUrl"
}
$repoSlug = $matches[1]

# --- Single-run lock -------------------------------------------------------

if (Test-Path -LiteralPath $script:LockPath) {
    $lockAge = (Get-Date) - (Get-Item -LiteralPath $script:LockPath).LastWriteTime
    if ($lockAge.TotalMinutes -lt $StaleLockMinutes) {
        Write-Output ("A dispatch-queue run is already in progress " +
            "(lock age {0:n0}m < {1}m). Skipping this tick." -f $lockAge.TotalMinutes, $StaleLockMinutes)
        exit 0
    }
    Write-Output ("Lock is stale ({0:n0}m old); overriding." -f $lockAge.TotalMinutes)
}
if (-not $DryRun) {
    Write-Utf8 $script:LockPath "pid=$PID started=$((Get-Date).ToString('o'))"
    $script:LockHeld = $true
}

try {
    # --- Preflight: clean main, in sync with origin ------------------------

    $currentBranch = (Git-Step @('symbolic-ref', '--short', 'HEAD')).Trim()
    if ($currentBranch -ne 'main') {
        Fail ("Repository is on branch '$currentBranch', not 'main'. A previous run " +
            "may have been interrupted. Return to a clean 'main' before queueing.")
    }

    $porcelain = @((Git-Step @('status', '--porcelain=v1')) -split "`r?`n" | Where-Object { $_ })
    $trackedDirty = @($porcelain | Where-Object { $_ -notmatch '^\?\? ' })
    if ($trackedDirty.Count -gt 0) {
        Fail ("Tracked files are dirty on main; refusing to queue a dispatch:`n" +
            ($trackedDirty -join "`n"))
    }
    $hasUntracked = (@($porcelain | Where-Object { $_ -match '^\?\? ' }).Count -gt 0)

    Git-Step @('fetch', '--quiet', 'origin', 'main') | Out-Null
    $headSha = (Git-Step @('rev-parse', 'HEAD')).Trim()
    $originSha = (Git-Step @('rev-parse', 'origin/main')).Trim()
    if ($headSha -ne $originSha) {
        Fail ("Local main ($($headSha.Substring(0,8))) is not in sync with " +
            "origin/main ($($originSha.Substring(0,8))). Resolve before queueing.")
    }

    # --- Select the oldest unprocessed queued issue ------------------------

    $runLabel = "${QueueLabel}-running"
    $doneLabel = "${QueueLabel}-done"
    $failLabel = "${QueueLabel}-failed"

    $list = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'list', '--repo', $repoSlug, '--label', $QueueLabel,
        '--state', 'open', '--limit', '100',
        '--json', 'number,title,body,labels,url')
    if ($list.Code -ne 0) {
        Fail "gh issue list failed (exit $($list.Code)):`n$($list.Text)"
    }

    # PS 5.1 ConvertFrom-Json emits a JSON array as a single non-enumerated
    # object; assign first, then wrap, so an empty [] yields zero items.
    $issues = @()
    if ($list.Text -and $list.Text.Trim()) {
        try { $parsed = $list.Text | ConvertFrom-Json }
        catch { Fail "Could not parse gh issue list JSON: $($_.Exception.Message)" }
        if ($null -ne $parsed) { $issues = @($parsed) }
    }

    $pending = @($issues | Where-Object {
        $names = @($_.labels | ForEach-Object { $_.name })
        ($names -notcontains $runLabel) -and ($names -notcontains $doneLabel)
    } | Sort-Object number)

    if ($pending.Count -eq 0) {
        Write-Output "No queued '$QueueLabel' issues to process in $repoSlug."
        Finish 0
    }

    $issue = $pending[0]
    $id = "ISSUE-$($issue.number)"
    $branch = "ai-dispatch/$id"
    $title = if ($issue.title) { [string]$issue.title } else { '(no title)' }

    Write-Output "Repo:     $repoSlug"
    Write-Output "Queued:   $($pending.Count) issue(s)"
    Write-Output "Next:     #$($issue.number) - $title"
    Write-Output "Dispatch: $id  ->  branch $branch"

    if ($DryRun) {
        Write-Output ""
        Write-Output "DryRun: no labels changed, no branch created, loop not run."
        Finish 0
    }

    # A branch with no terminal label means an earlier run was interrupted;
    # do not silently clobber it.
    if ((Git-Step @('branch', '--list', $branch)).Trim()) {
        Fail ("Branch '$branch' already exists but issue #$($issue.number) is not " +
            "labelled '$runLabel'/'$doneLabel'. Inconsistent state - resolve by hand.")
    }

    # --- Ensure bookkeeping labels exist (idempotent) ----------------------

    $labelSpec = @(
        @{ Name = $QueueLabel; Color = '0e8a16'; Desc = 'Queued for the AI dispatch loop' },
        @{ Name = $runLabel;   Color = 'fbca04'; Desc = 'AI dispatch in progress' },
        @{ Name = $doneLabel;  Color = '5319e7'; Desc = 'AI dispatch processed' },
        @{ Name = $failLabel;  Color = 'd93f0b'; Desc = 'AI dispatch run failed' }
    )
    foreach ($l in $labelSpec) {
        Invoke-Tool -Exe 'gh' -CmdArgs @(
            'label', 'create', $l.Name, '--repo', $repoSlug,
            '--color', $l.Color, '--description', $l.Desc, '--force') | Out-Null
    }

    # --- Mark running, build the goal --------------------------------------

    $edit = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'edit', "$($issue.number)", '--repo', $repoSlug, '--add-label', $runLabel)
    if ($edit.Code -ne 0) {
        Fail "Could not label issue #$($issue.number) '$runLabel' (exit $($edit.Code)):`n$($edit.Text)"
    }

    $goalBody = if ($issue.body -and $issue.body.Trim()) { [string]$issue.body } else { $title }
    $goalText = "GitHub issue #$($issue.number): $title`r`n`r`n$goalBody"
    $goalFile = Join-Path $env:TEMP "rge-ai-dispatch-goal-$id.txt"
    Write-Utf8 $goalFile $goalText

    # --- Park unrelated untracked clutter, branch, run the loop ------------

    $stashed = $false
    if ($hasUntracked) {
        Git-Step @('stash', 'push', '--include-untracked', '--message', "ai-dispatch-queue park: $id") | Out-Null
        $stashed = $true
    }

    Git-Step @('checkout', '-b', $branch) | Out-Null

    $loopLog = Join-Path $env:TEMP "rge-ai-dispatch-$id.log"
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $loopScript `
            -DispatchId $id -GoalFile $goalFile > $loopLog 2>&1
    } finally {
        $ErrorActionPreference = $prevEap
    }
    $loopExit = $LASTEXITCODE

    $loopText = (Get-Content -Raw -LiteralPath $loopLog -ErrorAction SilentlyContinue)
    $verdict = 'unknown'
    $vm = [regex]::Matches([string]$loopText, '(?im)^Codex control verdict:\s*(\S+)\s*$')
    if ($vm.Count -gt 0) { $verdict = $vm[$vm.Count - 1].Groups[1].Value }

    # --- Commit the dispatch's work locally on the branch ------------------

    Git-Step @('add', '-A') | Out-Null
    $staged = Invoke-Tool -Exe 'git' -CmdArgs @('diff', '--cached', '--quiet')
    $committed = $false
    $commitSha = ''
    if ($staged.Code -ne 0) {
        $outcome = if ($loopExit -eq 0) { 'ok' } else { "failed (exit $loopExit)" }
        $msg = @"
ai-dispatch $id`: $title

Unattended dispatch run via Invoke-AiDispatchQueue.ps1.
Loop exit code: $loopExit. Control verdict: $verdict. Outcome: $outcome.
Source: $($issue.url)

Local commit only - not pushed. Review the $branch branch,
then push or discard it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"@
        $msgFile = Join-Path $env:TEMP "rge-ai-dispatch-msg-$id.txt"
        Write-Utf8 $msgFile $msg
        Git-Step @('commit', '-F', $msgFile) | Out-Null
        Remove-Item -LiteralPath $msgFile -Force -ErrorAction SilentlyContinue
        $commitSha = (Git-Step @('rev-parse', '--short', 'HEAD')).Trim()
        $committed = $true
    }

    Git-Step @('checkout', 'main') | Out-Null
    if (-not $committed) {
        Git-Step @('branch', '-D', $branch) | Out-Null
    }

    # --- Restore the parked untracked clutter ------------------------------

    $stashWarning = ''
    if ($stashed) {
        $pop = Invoke-Tool -Exe 'git' -CmdArgs @('stash', 'pop')
        if ($pop.Code -ne 0) {
            $stashWarning = "WARNING: 'git stash pop' failed; parked untracked files " +
                "are still stashed. Run 'git stash pop' by hand.`n$($pop.Text)"
        }
    }

    # --- Post the result comment and finalize labels -----------------------

    $branchLine = if ($committed) {
        "Committed locally as ``$commitSha`` on branch ``$branch`` (not pushed)."
    } else {
        "The loop produced no committable changes; branch ``$branch`` was not kept."
    }
    $logTail = if ($loopText) {
        ($loopText -split "`r?`n" | Select-Object -Last 30) -join "`n"
    } else { '(no loop output captured)' }
    $statusIcon = if ($loopExit -eq 0) { 'succeeded' } else { 'FAILED' }
    $commentBody = @"
**AI dispatch run $statusIcon** - dispatch ``$id``

- Loop exit code: ``$loopExit``
- Codex control verdict: ``$verdict``
- $branchLine

<details><summary>Dispatch loop output (tail)</summary>

``````
$logTail
``````
</details>

_Posted by Invoke-AiDispatchQueue.ps1. Code stays on the local branch until a human reviews and pushes it._
"@
    $commentFile = Join-Path $env:TEMP "rge-ai-dispatch-comment-$id.txt"
    Write-Utf8 $commentFile $commentBody
    $comment = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'comment', "$($issue.number)", '--repo', $repoSlug, '--body-file', $commentFile)
    Remove-Item -LiteralPath $commentFile -Force -ErrorAction SilentlyContinue
    if ($comment.Code -ne 0) {
        Write-Output "WARNING: could not post result comment (exit $($comment.Code)): $($comment.Text)"
    }

    $relabel = @('issue', 'edit', "$($issue.number)", '--repo', $repoSlug,
        '--remove-label', $runLabel, '--remove-label', $QueueLabel, '--add-label', $doneLabel)
    if ($loopExit -ne 0) { $relabel += @('--add-label', $failLabel) }
    $rl = Invoke-Tool -Exe 'gh' -CmdArgs $relabel
    if ($rl.Code -ne 0) {
        Write-Output "WARNING: could not finalize labels on issue #$($issue.number) (exit $($rl.Code)): $($rl.Text)"
    }

    Remove-Item -LiteralPath $goalFile -Force -ErrorAction SilentlyContinue

    # --- Report ------------------------------------------------------------

    Write-Output ""
    Write-Output "Dispatch $id $statusIcon (loop exit $loopExit, verdict $verdict)."
    Write-Output $branchLine.Replace('`', '')
    Write-Output "Issue #$($issue.number) relabelled; result comment posted."
    Write-Output "Loop log: $loopLog"
    if ($stashWarning) {
        Write-Output ""
        Write-Output $stashWarning
    }

    Finish 0
} catch {
    Fail "Unexpected error: $($_.Exception.Message)"
}
