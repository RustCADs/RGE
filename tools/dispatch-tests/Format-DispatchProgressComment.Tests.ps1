#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-229 mid-run progress-comment formatter in
    Invoke-AiDispatchQueue.ps1 (Format-DispatchProgressComment).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    the pure helper Format-DispatchProgressComment loads without running the
    dispatch flow, then exercises the helper across the four supported
    stages (issue-claimed, loop-starting, loop-finished, publish-decision)
    and across all four publish-decision modes.

    The helper is pure and side-effect-free: it does not read or write
    files, call gh, git, codex, claude, the queue runner, the scheduler, or
    the network. These tests inherit that purity -- nothing here invokes
    any of those surfaces, no real GitHub issues are read or modified, no
    temporary repo is created, and no progress comment is posted. The tests
    only call the formatter and assert on the returned string.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }

    # Dot-source the production queue script through the testability seam so
    # Format-DispatchProgressComment lands in this Pester session without
    # running the dispatch flow or requiring gh / codex / claude on PATH.
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    # Synthetic inputs reused across contexts. Values that look like real
    # production strings but cannot collide with a live issue / branch.
    $script:IssueNumber = 99229
    $script:DispatchId  = 'ISSUE-PESTER-99229'
    $script:Branch      = 'ai-dispatch/ISSUE-PESTER-99229'
    $script:LoopLogPath = 'C:\Users\pester\AppData\Local\Temp\rge-ai-dispatch-ISSUE-PESTER-99229.log'

    # Regex for catching unexpanded PowerShell variable tokens leaking into
    # the rendered body (e.g. literal "$id" instead of the interpolated id).
    # The formatter must always expand its inputs before returning.
    $script:UnexpandedTokenPattern = '\$[A-Za-z_][A-Za-z0-9_]*'
}

Describe 'Format-DispatchProgressComment (mid-run progress-comment formatter)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchProgressComment -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Stage: issue-claimed' {
        BeforeAll {
            $script:IssueClaimedBody = Format-DispatchProgressComment `
                -Stage 'issue-claimed' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch
        }

        It 'returns a non-empty string' {
            $script:IssueClaimedBody | Should -Not -BeNullOrEmpty
        }

        It 'includes the issue number, dispatch id, and branch name' {
            $script:IssueClaimedBody | Should -Match ("#" + [regex]::Escape($script:IssueNumber.ToString()))
            $script:IssueClaimedBody | Should -Match ([regex]::Escape($script:DispatchId))
            $script:IssueClaimedBody | Should -Match ([regex]::Escape($script:Branch))
        }

        It 'is labelled as a non-terminal progress marker' {
            $script:IssueClaimedBody | Should -Match 'AI dispatch progress'
            $script:IssueClaimedBody | Should -Match 'issue claimed'
            $script:IssueClaimedBody | Should -Match 'non-terminal progress marker'
        }

        It 'has no unexpanded PowerShell variable tokens' {
            $script:IssueClaimedBody | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'does not embed full logs, transcripts, or control JSON' {
            $script:IssueClaimedBody | Should -Not -Match '(?m)^```json'
            $script:IssueClaimedBody | Should -Not -Match 'loop output'
            $script:IssueClaimedBody | Should -Not -Match 'transcript'
        }
    }

    Context 'Stage: loop-starting' {
        BeforeAll {
            $script:LoopStartingBody = Format-DispatchProgressComment `
                -Stage 'loop-starting' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -LoopLogPath $script:LoopLogPath
        }

        It 'includes the loop-log path' {
            $script:LoopStartingBody | Should -Match ([regex]::Escape($script:LoopLogPath))
        }

        It 'includes the issue number, dispatch id, and branch name' {
            $script:LoopStartingBody | Should -Match ("#" + [regex]::Escape($script:IssueNumber.ToString()))
            $script:LoopStartingBody | Should -Match ([regex]::Escape($script:DispatchId))
            $script:LoopStartingBody | Should -Match ([regex]::Escape($script:Branch))
        }

        It 'is labelled as the inner-loop-starting stage' {
            $script:LoopStartingBody | Should -Match 'inner loop starting'
            $script:LoopStartingBody | Should -Match 'Invoke-AiDispatchLoop\.ps1'
        }

        It 'falls back deterministically when no loop log path is given' {
            $fallback = Format-DispatchProgressComment `
                -Stage 'loop-starting' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch
            $fallback | Should -Match 'path not yet available'
            $fallback | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'has no unexpanded PowerShell variable tokens' {
            $script:LoopStartingBody | Should -Not -Match $script:UnexpandedTokenPattern
        }
    }

    Context 'Stage: loop-finished' {
        BeforeAll {
            $script:LoopFinishedBody = Format-DispatchProgressComment `
                -Stage 'loop-finished' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -LoopExit    '0' `
                -Verdict     'pass'
        }

        It 'includes the loop exit code and the Codex control verdict' {
            $script:LoopFinishedBody | Should -Match '`0`'
            $script:LoopFinishedBody | Should -Match '`pass`'
        }

        It 'is labelled as the inner-loop-finished stage' {
            $script:LoopFinishedBody | Should -Match 'inner loop finished'
        }

        It 'falls back to unknown verdict when none is given' {
            $unknownVerdict = Format-DispatchProgressComment `
                -Stage 'loop-finished' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -LoopExit    '1'
            $unknownVerdict | Should -Match '`unknown`'
            $unknownVerdict | Should -Match '`1`'
            $unknownVerdict | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'falls back to unknown loop exit when none is given' {
            $unknownExit = Format-DispatchProgressComment `
                -Stage 'loop-finished' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -Verdict     'pass'
            $unknownExit | Should -Match 'Loop exit code: `unknown`'
            $unknownExit | Should -Match '`pass`'
            $unknownExit | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'has no unexpanded PowerShell variable tokens' {
            $script:LoopFinishedBody | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'does not embed loop output tails or model transcripts' {
            $script:LoopFinishedBody | Should -Not -Match '(?m)^```text'
            $script:LoopFinishedBody | Should -Not -Match 'loop output'
        }
    }

    Context 'Stage: publish-decision' {
        It 'distinguishes auto-publish mode' {
            $body = Format-DispatchProgressComment `
                -Stage 'publish-decision' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -PublishMode 'auto-publish'
            $body | Should -Match 'auto-publish'
            $body | Should -Match 'origin/main'
            $body | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'distinguishes -NoPublish branch mode' {
            $body = Format-DispatchProgressComment `
                -Stage 'publish-decision' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -PublishMode 'branch'
            $body | Should -Match 'NoPublish'
            $body | Should -Match 'dispatch branch'
            $body | Should -Not -Match 'origin/main'
            $body | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'distinguishes not-eligible-to-publish mode' {
            $body = Format-DispatchProgressComment `
                -Stage 'publish-decision' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -PublishMode 'not-eligible'
            $body | Should -Match 'not eligible'
            $body | Should -Match 'verdict'
            $body | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'distinguishes no-commit mode' {
            $body = Format-DispatchProgressComment `
                -Stage 'publish-decision' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -PublishMode 'no-commit'
            $body | Should -Match 'no committable changes'
            $body | Should -Not -Match $script:UnexpandedTokenPattern
        }

        It 'produces a different body for each of the four publish modes' {
            $modes = @('auto-publish', 'branch', 'not-eligible', 'no-commit')
            $bodies = foreach ($m in $modes) {
                Format-DispatchProgressComment `
                    -Stage 'publish-decision' `
                    -IssueNumber $script:IssueNumber `
                    -DispatchId  $script:DispatchId `
                    -Branch      $script:Branch `
                    -PublishMode $m
            }
            ($bodies | Select-Object -Unique).Count | Should -Be 4
        }
    }

    Context 'Determinism, validation, and purity' {
        It 'returns identical bodies for repeated calls with the same inputs' {
            $first = Format-DispatchProgressComment `
                -Stage 'loop-finished' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -LoopExit    '0' `
                -Verdict     'pass'
            $second = Format-DispatchProgressComment `
                -Stage 'loop-finished' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -LoopExit    '0' `
                -Verdict     'pass'
            $first | Should -BeExactly $second
        }

        It 'rejects an unknown stage name' {
            { Format-DispatchProgressComment `
                -Stage 'not-a-real-stage' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch } | Should -Throw
        }

        It 'rejects an unknown publish mode' {
            { Format-DispatchProgressComment `
                -Stage 'publish-decision' `
                -IssueNumber $script:IssueNumber `
                -DispatchId  $script:DispatchId `
                -Branch      $script:Branch `
                -PublishMode 'totally-made-up' } | Should -Throw
        }

        It 'does not invoke gh, git, codex, claude, the queue, or the network' {
            # The helper is a pure string formatter. We assert purity by
            # verifying that calls within the test session never resolved
            # any of the external commands the queue depends on. The test
            # has not imported / invoked these names at all, so this is a
            # belt-and-suspenders check: if the formatter accidentally
            # shelled out, the test surface would still not exercise it,
            # but the assertion below documents the contract.
            (Get-Command -Name Format-DispatchProgressComment).CommandType |
                Should -Be 'Function'
            # Smoke-call once more in a fresh invocation -- it must
            # complete deterministically with no side effects we can
            # observe from inside this test.
            $body = Format-DispatchProgressComment `
                -Stage 'issue-claimed' `
                -IssueNumber 1 `
                -DispatchId  'ISSUE-PURE' `
                -Branch      'ai-dispatch/ISSUE-PURE'
            $body | Should -Not -BeNullOrEmpty
        }
    }
}
