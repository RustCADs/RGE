#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-230 CORRECT-round PR-metadata gate in
    Invoke-AiDispatchQueue.ps1 (Resolve-DispatchPrViewMetadata).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    Resolve-DispatchPrViewMetadata loads without running the dispatch flow,
    then exercises every relevant `gh pr view --json number,url` outcome.

    The correction packet requires that PR-mode publish only marks success
    when both a PR number and a PR URL are available; missing or partial
    metadata must be publish-pipeline failure so the final issue comment
    cannot claim PR success without the required reference.

    The helper is pure: no gh / git / network / file I/O. The tests inherit
    that purity -- no live GitHub calls, no real PR creation, no branch push.
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
}

Describe 'Resolve-DispatchPrViewMetadata (PR metadata success gate)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Resolve-DispatchPrViewMetadata -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Success path: complete metadata' {
        It 'returns Success=true with both PrNumber and PrUrl when JSON has number and url' {
            $json = '{"number":42,"url":"https://github.com/octo/repo/pull/42"}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success       | Should -BeTrue
            $r.PrNumber      | Should -Be 42
            $r.PrUrl         | Should -Be 'https://github.com/octo/repo/pull/42'
            $r.FailureReason | Should -Be ''
        }

        It 'returns a System.Boolean for Success' {
            $json = '{"number":7,"url":"https://github.com/o/r/pull/7"}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success.GetType().FullName | Should -Be 'System.Boolean'
        }

        It 'is deterministic for repeated calls with the same input' {
            $json = '{"number":230,"url":"https://github.com/halil/rge/pull/230"}'
            $a = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $b = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $a.Success  | Should -BeExactly $b.Success
            $a.PrNumber | Should -BeExactly $b.PrNumber
            $a.PrUrl    | Should -BeExactly $b.PrUrl
        }
    }

    Context 'Non-zero exit code: publish-pipeline failure' {
        It 'returns Success=false when ExitCode is non-zero, even with valid JSON' {
            $json = '{"number":42,"url":"https://github.com/octo/repo/pull/42"}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 1 -Text $json
            $r.Success       | Should -BeFalse
            $r.PrNumber      | Should -Be 0
            $r.PrUrl         | Should -Be ''
            $r.FailureReason | Should -Match 'gh pr view'
            $r.FailureReason | Should -Match 'exit 1'
        }

        It 'carries the stderr/stdout snippet in FailureReason on non-zero exit' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 2 -Text 'no pull requests found'
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'no pull requests found'
        }
    }

    Context 'Empty or whitespace output: publish-pipeline failure' {
        It 'returns Success=false on empty Text' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text ''
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'empty output'
        }

        It 'returns Success=false on whitespace-only Text' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text "   `n  `t"
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'empty output'
        }

        It 'returns Success=false on $null Text (treated as empty)' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $null
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'empty output'
        }
    }

    Context 'Unparseable JSON: publish-pipeline failure' {
        It 'returns Success=false with a JSON parse explanation on garbage input' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text 'not json {'
            $r.Success       | Should -BeFalse
            $r.PrNumber      | Should -Be 0
            $r.PrUrl         | Should -Be ''
            $r.FailureReason | Should -Match 'unparseable JSON'
        }
    }

    Context 'Incomplete metadata: publish-pipeline failure' {
        It 'returns Success=false when number is missing' {
            $json = '{"url":"https://github.com/o/r/pull/9"}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'incomplete PR metadata'
        }

        It 'returns Success=false when url is missing' {
            $json = '{"number":9}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'incomplete PR metadata'
        }

        It 'returns Success=false when number is 0' {
            $json = '{"number":0,"url":"https://github.com/o/r/pull/0"}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'incomplete PR metadata'
        }

        It 'returns Success=false when url is the empty string' {
            $json = '{"number":7,"url":""}'
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text $json
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'incomplete PR metadata'
        }

        It 'returns Success=false when both fields are missing' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text '{}'
            $r.Success       | Should -BeFalse
            $r.FailureReason | Should -Match 'incomplete PR metadata'
        }
    }

    Context 'Failure result shape' {
        It 'always emits the four documented properties' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text '{}'
            $names = $r.PSObject.Properties.Name | Sort-Object
            ($names -join ',') | Should -Be 'FailureReason,PrNumber,PrUrl,Success'
        }

        It 'never leaks the success-path detail into a failure FailureReason' {
            $r = Resolve-DispatchPrViewMetadata -ExitCode 0 -Text '{}'
            $r.FailureReason | Should -Not -Match 'Pushed .* to origin'
            $r.FailureReason | Should -Not -Match '^Pushed'
        }
    }
}
