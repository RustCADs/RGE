#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Get-HaltClearEligibility in Invoke-AiDispatchAuto.ps1 --
    the fail-closed policy for default-OFF -AllowCodexClearHalt.

.DESCRIPTION
    Dot-sources the Auto driver through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam.
    Pure function; no side effects.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try { . $script:AutoScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-HaltClearEligibility' {
    It 'allows clearing self-resolved class <Class>' -ForEach @(
        @{ Class = 'seatbelt' }
        @{ Class = 'recovery' }
    ) {
        (Get-HaltClearEligibility -HaltClass $Class).Clearable | Should -BeTrue
    }

    It 'holds human-only class <Class>' -ForEach @(
        @{ Class = 'queue-exit' }
        @{ Class = 'seatbelt-corrupt' }
        @{ Class = 'consec-fail' }
        @{ Class = 'idle' }
        @{ Class = 'needs-human' }
        @{ Class = 'fault' }
        @{ Class = 'manual' }
    ) {
        (Get-HaltClearEligibility -HaltClass $Class).Clearable | Should -BeFalse
    }

    It 'fail-closed on an unknown class' {
        (Get-HaltClearEligibility -HaltClass 'something-new').Clearable | Should -BeFalse
    }

    It 'fail-closed on a blank class' {
        $d = Get-HaltClearEligibility -HaltClass ''
        $d.Clearable | Should -BeFalse
        $d.Reason | Should -Match 'no halt class'
    }

    It 'is case-insensitive and trims whitespace' {
        (Get-HaltClearEligibility -HaltClass '  SEATBELT ').Clearable | Should -BeTrue
        (Get-HaltClearEligibility -HaltClass 'Recovery').Clearable | Should -BeTrue
    }
}
