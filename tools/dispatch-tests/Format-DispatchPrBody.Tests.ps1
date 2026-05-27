#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-230 PR body / title formatters in
    Invoke-AiDispatchQueue.ps1 (Format-DispatchPrTitle, Format-DispatchPrBody).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    the pure helpers load without running the dispatch flow, then exercises
    them across a handful of synthetic dispatch fixtures.

    The helpers are pure and side-effect-free: they do not read or write
    files, call gh, git, codex, claude, the queue runner, the scheduler, or
    the network -- they only return a markdown string assembled from their
    arguments. The tests inherit that purity: nothing here invokes any of
    those surfaces, no real GitHub PR is opened, no branch is pushed, no
    issue is mutated.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }

    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    # Synthetic inputs reused across contexts. Values that look like real
    # production strings but cannot collide with a live dispatch.
    $script:IssueNumber     = 99230
    $script:IssueTitle      = 'Add opt-in PR publish mode to AI dispatch queue'
    $script:DispatchId      = 'ISSUE-PESTER-99230'
    $script:Branch          = 'ai-dispatch/ISSUE-PESTER-99230'
    $script:CommitSha       = 'abc1234'
    $script:DispatchLogPath = 'ai_dispatch_logs/log_2026-05-27_14-30-00+0300.md'
    $script:Verdict         = 'pass'

    # Pattern guard against an unexpanded `$varName` token leaking into the
    # rendered body or title.
    $script:UnexpandedTokenPattern = '\$[A-Za-z_][A-Za-z0-9_]*'
}

Describe 'Format-DispatchPrTitle (deterministic PR title formatter)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchPrTitle -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    It 'includes the dispatch id and the issue title' {
        $t = Format-DispatchPrTitle -DispatchId $script:DispatchId -IssueTitle $script:IssueTitle
        $t | Should -Match ([regex]::Escape($script:DispatchId))
        $t | Should -Match ([regex]::Escape($script:IssueTitle))
    }

    It 'falls back to a placeholder when the issue title is empty' {
        $t = Format-DispatchPrTitle -DispatchId $script:DispatchId -IssueTitle ''
        $t | Should -Match '\(no title\)'
        $t | Should -Not -Match $script:UnexpandedTokenPattern
    }

    It 'is deterministic across repeated calls' {
        $first  = Format-DispatchPrTitle -DispatchId $script:DispatchId -IssueTitle $script:IssueTitle
        $second = Format-DispatchPrTitle -DispatchId $script:DispatchId -IssueTitle $script:IssueTitle
        $first | Should -BeExactly $second
    }
}

Describe 'Format-DispatchPrBody (deterministic PR body formatter)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchPrBody -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'A typical successful PR-mode body' {

        BeforeAll {
            $script:Body = Format-DispatchPrBody `
                -IssueNumber     $script:IssueNumber `
                -IssueTitle      $script:IssueTitle `
                -DispatchId      $script:DispatchId `
                -Branch          $script:Branch `
                -CommitSha       $script:CommitSha `
                -DispatchLogPath $script:DispatchLogPath `
                -Verdict         $script:Verdict
        }

        It 'returns a non-empty string' {
            $script:Body | Should -Not -BeNullOrEmpty
        }

        It 'includes the source issue number and title' {
            $script:Body | Should -Match ('#' + [regex]::Escape($script:IssueNumber.ToString()))
            $script:Body | Should -Match ([regex]::Escape($script:IssueTitle))
        }

        It 'includes the dispatch id, branch, commit SHA, log path, and verdict' {
            $script:Body | Should -Match ([regex]::Escape($script:DispatchId))
            $script:Body | Should -Match ([regex]::Escape($script:Branch))
            $script:Body | Should -Match ([regex]::Escape($script:CommitSha))
            $script:Body | Should -Match ([regex]::Escape($script:DispatchLogPath))
            $script:Body | Should -Match ([regex]::Escape($script:Verdict))
        }

        It 'links to the issue with Refs #<n> and not Closes #<n>' {
            $script:Body | Should -Match ('(?m)^Refs #' + $script:IssueNumber + '\s*$')
            $script:Body | Should -Not -Match 'Closes #'
            $script:Body | Should -Not -Match 'closes #'
            $script:Body | Should -Not -Match 'CLOSES #'
            $script:Body | Should -Not -Match 'Fixes #'
            $script:Body | Should -Not -Match 'Resolves #'
        }

        It 'explicitly disclaims merging, origin/main pushing, and issue close' {
            $script:Body | Should -Match 'not merge'
            $script:Body | Should -Match 'origin/main'
            $script:Body | Should -Match 'not.*close.*source issue'
        }

        It 'has no unexpanded PowerShell variable tokens' {
            $script:Body | Should -Not -Match $script:UnexpandedTokenPattern
        }
    }

    Context 'Issue-number generalization (not hard-coded to #230)' {
        It 'uses the supplied issue number, not a baked-in value' {
            $body = Format-DispatchPrBody `
                -IssueNumber     17 `
                -IssueTitle      'Some other issue' `
                -DispatchId      'ISSUE-17' `
                -Branch          'ai-dispatch/ISSUE-17' `
                -CommitSha       'deadbee' `
                -DispatchLogPath 'ai_dispatch_logs/log_x.md' `
                -Verdict         'pass'
            $body | Should -Match '(?m)^Refs #17\s*$'
            $body | Should -Not -Match '(?m)^Refs #230\s*$'
        }

        It 'still emits Refs #230 when the source issue is #230' {
            $body = Format-DispatchPrBody `
                -IssueNumber     230 `
                -IssueTitle      'PR mode' `
                -DispatchId      'ISSUE-230' `
                -Branch          'ai-dispatch/ISSUE-230' `
                -CommitSha       '1234567' `
                -DispatchLogPath 'ai_dispatch_logs/log_y.md' `
                -Verdict         'pass'
            $body | Should -Match '(?m)^Refs #230\s*$'
        }
    }

    Context 'Empty-title fallback' {
        It 'renders a placeholder title without crashing or leaking tokens' {
            $body = Format-DispatchPrBody `
                -IssueNumber     7 `
                -IssueTitle      '' `
                -DispatchId      'ISSUE-7' `
                -Branch          'ai-dispatch/ISSUE-7' `
                -CommitSha       'cafef00' `
                -DispatchLogPath 'ai_dispatch_logs/log_z.md' `
                -Verdict         'pass'
            $body | Should -Match '\(no title\)'
            $body | Should -Not -Match $script:UnexpandedTokenPattern
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical bodies for repeated calls with the same inputs' {
            $args = @{
                IssueNumber     = $script:IssueNumber
                IssueTitle      = $script:IssueTitle
                DispatchId      = $script:DispatchId
                Branch          = $script:Branch
                CommitSha       = $script:CommitSha
                DispatchLogPath = $script:DispatchLogPath
                Verdict         = $script:Verdict
            }
            $first  = Format-DispatchPrBody @args
            $second = Format-DispatchPrBody @args
            $first | Should -BeExactly $second
        }

        It 'is a function, not an alias or external command' {
            (Get-Command -Name Format-DispatchPrBody).CommandType | Should -Be 'Function'
        }
    }
}
