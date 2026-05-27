#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-227 terminal label-plan helper in
    Invoke-AiDispatchQueue.ps1.

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    the pure helper Get-DispatchTerminalLabelPlan loads without running the
    dispatch flow, then exercises the helper across the three terminal
    states it reconciles:

      * Terminal success (RunFailed=$false, WillRetry=$false).
      * Terminal failure (RunFailed=$true, WillRetry=$false) -- both single
        and multi taxonomy selections.
      * Retry (WillRetry=$true).

    The helper consumes only already-computed queue state and a known
    failure-taxonomy label set; it does not read or write files, call gh,
    git, codex, claude, the queue runner, the scheduler, or the network.
    The tests inherit that purity: nothing here invokes any of those
    surfaces, no real GitHub issues are created or modified, and no
    temporary repo or run-dir is built.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }

    # Dot-source the production queue script through the testability seam so
    # Get-DispatchTerminalLabelPlan lands in this Pester session without
    # running the dispatch flow or requiring gh / codex / claude on PATH.
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    # Synthetic label inputs. Names mirror the production defaults so the
    # tests fail loudly if the helper accidentally hard-codes any of them.
    $script:QueueLabel = 'ai-dispatch'
    $script:RunLabel   = 'ai-dispatch-running'
    $script:DoneLabel  = 'ai-dispatch-done'
    $script:FailLabel  = 'ai-dispatch-failed'
    $script:RetryLabel = 'ai-dispatch-retry'
    $script:KnownTaxonomy = @(
        'ai-dispatch-failure-stall',
        'ai-dispatch-failure-timeout',
        'ai-dispatch-failure-blocked',
        'ai-dispatch-failure-verification',
        'ai-dispatch-failure-control',
        'ai-dispatch-failure-publish',
        'ai-dispatch-failure-unknown'
    )
}

Describe 'Get-DispatchTerminalLabelPlan (terminal label-plan helper)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Get-DispatchTerminalLabelPlan -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Terminal success (RunFailed=$false, WillRetry=$false)' {

        BeforeAll {
            $script:SuccessPlan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $false `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @() `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
        }

        It 'adds only the done label' {
            $script:SuccessPlan.Add        | Should -Contain $script:DoneLabel
            ,$script:SuccessPlan.Add       | Should -HaveCount 1
        }

        It 'removes queue, running, retry, and failed labels' {
            $script:SuccessPlan.Remove | Should -Contain $script:QueueLabel
            $script:SuccessPlan.Remove | Should -Contain $script:RunLabel
            $script:SuccessPlan.Remove | Should -Contain $script:RetryLabel
            $script:SuccessPlan.Remove | Should -Contain $script:FailLabel
        }

        It 'removes every known failure-taxonomy label' {
            foreach ($t in $script:KnownTaxonomy) {
                $script:SuccessPlan.Remove | Should -Contain $t
            }
        }

        It 'does not list the done label in the remove plan' {
            $script:SuccessPlan.Remove | Should -Not -Contain $script:DoneLabel
        }
    }

    Context 'Terminal failure (RunFailed=$true, WillRetry=$false), single taxonomy' {

        BeforeAll {
            $script:SelectedTaxonomy = @('ai-dispatch-failure-control')
            $script:FailurePlan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels $script:SelectedTaxonomy `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
        }

        It 'adds done, failed, and the selected taxonomy label' {
            $script:FailurePlan.Add | Should -Contain $script:DoneLabel
            $script:FailurePlan.Add | Should -Contain $script:FailLabel
            $script:FailurePlan.Add | Should -Contain 'ai-dispatch-failure-control'
        }

        It 'removes queue, running, and retry labels' {
            $script:FailurePlan.Remove | Should -Contain $script:QueueLabel
            $script:FailurePlan.Remove | Should -Contain $script:RunLabel
            $script:FailurePlan.Remove | Should -Contain $script:RetryLabel
        }

        It 'removes every non-selected failure-taxonomy label' {
            foreach ($t in $script:KnownTaxonomy) {
                if ($t -eq 'ai-dispatch-failure-control') {
                    $script:FailurePlan.Remove | Should -Not -Contain $t
                } else {
                    $script:FailurePlan.Remove | Should -Contain $t
                }
            }
        }

        It 'never lists the failed label or the selected taxonomy in remove' {
            $script:FailurePlan.Remove | Should -Not -Contain $script:FailLabel
            $script:FailurePlan.Remove | Should -Not -Contain $script:DoneLabel
            $script:FailurePlan.Remove | Should -Not -Contain 'ai-dispatch-failure-control'
        }
    }

    Context 'Terminal failure with multiple selected taxonomy labels' {

        BeforeAll {
            $script:MultiTaxonomy = @(
                'ai-dispatch-failure-verification',
                'ai-dispatch-failure-publish'
            )
            $script:MultiPlan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels $script:MultiTaxonomy `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
        }

        It 'adds every selected taxonomy label alongside done and failed' {
            $script:MultiPlan.Add | Should -Contain $script:DoneLabel
            $script:MultiPlan.Add | Should -Contain $script:FailLabel
            foreach ($t in $script:MultiTaxonomy) {
                $script:MultiPlan.Add | Should -Contain $t
            }
        }

        It 'partitions taxonomy labels cleanly between add and remove' {
            $remove = @($script:MultiPlan.Remove)
            foreach ($t in $script:MultiTaxonomy) {
                $remove | Should -Not -Contain $t
            }
            $unselected = $script:KnownTaxonomy | Where-Object { $script:MultiTaxonomy -notcontains $_ }
            foreach ($t in $unselected) {
                $remove | Should -Contain $t
            }
        }
    }

    Context 'Retry (WillRetry=$true)' {

        BeforeAll {
            $script:RetryPlan = Get-DispatchTerminalLabelPlan `
                -WillRetry $true `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @('ai-dispatch-failure-timeout') `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
        }

        It 'keeps or adds the queue and retry labels' {
            $script:RetryPlan.Add | Should -Contain $script:QueueLabel
            $script:RetryPlan.Add | Should -Contain $script:RetryLabel
        }

        It 'removes running, done, and failed labels' {
            $script:RetryPlan.Remove | Should -Contain $script:RunLabel
            $script:RetryPlan.Remove | Should -Contain $script:DoneLabel
            $script:RetryPlan.Remove | Should -Contain $script:FailLabel
        }

        It 'removes every failure-taxonomy label, including any selected one' {
            foreach ($t in $script:KnownTaxonomy) {
                $script:RetryPlan.Remove | Should -Contain $t
            }
        }

        It 'never lists queue or retry in the remove plan' {
            $script:RetryPlan.Remove | Should -Not -Contain $script:QueueLabel
            $script:RetryPlan.Remove | Should -Not -Contain $script:RetryLabel
        }
    }

    Context 'Stale / incompatible taxonomy label removal' {

        It 'plans to scrub every known taxonomy label from a passing run' {
            $plan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $false `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @() `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
            foreach ($t in $script:KnownTaxonomy) {
                $plan.Remove | Should -Contain $t
            }
        }

        It 'plans to scrub stale taxonomy labels when switching classifications' {
            # An issue carries 'ai-dispatch-failure-timeout' from a prior
            # attempt. The new terminal failure classifies the run as
            # 'ai-dispatch-failure-verification'. The helper must plan to
            # remove the stale 'timeout' label so the issue cannot end up
            # carrying two contradictory taxonomy classifications.
            $plan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @('ai-dispatch-failure-verification') `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
            $plan.Add    | Should -Contain 'ai-dispatch-failure-verification'
            $plan.Remove | Should -Contain 'ai-dispatch-failure-timeout'
            $plan.Remove | Should -Not -Contain 'ai-dispatch-failure-verification'
        }

        It 'never overlaps add and remove for any state' {
            $cases = @(
                @{ WillRetry = $false; RunFailed = $false; Tax = @() },
                @{ WillRetry = $false; RunFailed = $true;  Tax = @('ai-dispatch-failure-stall') },
                @{ WillRetry = $false; RunFailed = $true;  Tax = @('ai-dispatch-failure-control', 'ai-dispatch-failure-publish') },
                @{ WillRetry = $true;  RunFailed = $true;  Tax = @() },
                @{ WillRetry = $true;  RunFailed = $true;  Tax = @('ai-dispatch-failure-timeout') }
            )
            foreach ($c in $cases) {
                $plan = Get-DispatchTerminalLabelPlan `
                    -WillRetry $c.WillRetry `
                    -RunFailed $c.RunFailed `
                    -QueueLabel $script:QueueLabel `
                    -RunLabel   $script:RunLabel `
                    -DoneLabel  $script:DoneLabel `
                    -FailLabel  $script:FailLabel `
                    -RetryLabel $script:RetryLabel `
                    -TaxonomyLabels $c.Tax `
                    -KnownFailureTaxonomyLabels $script:KnownTaxonomy
                $addSet = [System.Collections.Generic.HashSet[string]]::new()
                foreach ($a in $plan.Add) { [void]$addSet.Add($a) }
                foreach ($r in $plan.Remove) {
                    $addSet.Contains($r) | Should -BeFalse `
                        -Because "Add and Remove must not overlap for state WillRetry=$($c.WillRetry), RunFailed=$($c.RunFailed), Tax=[$($c.Tax -join ',')], but '$r' appears in both"
                }
            }
        }
    }

    Context 'Helper purity and determinism' {

        It 'returns identical plans across repeated calls with the same inputs' {
            $first = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @('ai-dispatch-failure-publish') `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
            $second = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @('ai-dispatch-failure-publish') `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
            (@($first.Add)    -join '|') | Should -BeExactly (@($second.Add)    -join '|')
            (@($first.Remove) -join '|') | Should -BeExactly (@($second.Remove) -join '|')
        }

        It 'deduplicates repeated taxonomy labels in the selected set' {
            $plan = Get-DispatchTerminalLabelPlan `
                -WillRetry $false `
                -RunFailed $true `
                -QueueLabel $script:QueueLabel `
                -RunLabel   $script:RunLabel `
                -DoneLabel  $script:DoneLabel `
                -FailLabel  $script:FailLabel `
                -RetryLabel $script:RetryLabel `
                -TaxonomyLabels @('ai-dispatch-failure-stall', 'ai-dispatch-failure-stall') `
                -KnownFailureTaxonomyLabels $script:KnownTaxonomy
            @($plan.Add | Where-Object { $_ -eq 'ai-dispatch-failure-stall' }).Count | Should -Be 1
        }
    }
}
