#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

# Unit coverage for Invoke-AiDispatchGuard.ps1 safety logic: source classification
# (review #3) and the broadened protected-ref / force-push + signal hard rules
# (review #1 / #2). Dot-sources the guard via its RGE_AI_DISPATCH_GUARD_SKIP_MAIN
# seam so Main never launches a run -- no child process, no claude, no git, no gh.

BeforeAll {
    $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = '1'
    $guard = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
    . $guard -DispatchId 'PESTER' -WatchRoot $TestDrive
    $ErrorActionPreference = 'Continue'
}

AfterAll {
    Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
}

Describe 'Get-RecordSource' {
    It 'classifies loop/gate status lines as signal: <Line>' -ForEach @(
        @{ Line = 'VERIFY OK: all 7 verification step(s) passed.' }
        @{ Line = 'VERIFY FAILED: ratio too high' }
        @{ Line = 'GATE_EXIT=2' }
        @{ Line = 'Codex execution round 1: executed' }
        @{ Line = 'control verdict=pass' }
        @{ Line = 'HANDOFF_STATUS: BLOCKED' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'signal'
    }

    It 'classifies an echoed command as command: <Line>' -ForEach @(
        @{ Line = '+ git push origin main' }
        @{ Line = 'git push origin HEAD:main' }
        @{ Line = 'PS A:\rge> git status' }
        @{ Line = 'cargo test --workspace' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'command'
    }

    It 'classifies free prose as prose -- a mention is not an action (review #3): <Line>' -ForEach @(
        @{ Line = 'The TASK says: do not run git push origin main under any circumstances.' }
        @{ Line = 'Codex selected task DEMO-1 from the queue' }
        @{ Line = 'Reasoning about whether to publish to main later' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'prose'
    }
}

Describe 'Test-HardRule command patterns (review #1/#2: broadened protected-ref + force)' {
    It 'trips on protected-ref push form: <Cmd>' -ForEach @(
        @{ Cmd = 'git push origin main' }
        @{ Cmd = 'git push origin master' }
        @{ Cmd = 'git push origin HEAD:main' }
        @{ Cmd = 'git push origin refs/heads/main' }
        @{ Cmd = 'git push --set-upstream origin main' }
        @{ Cmd = 'git push origin +main:main' }
        @{ Cmd = 'git push --force origin main' }
        @{ Cmd = 'git push -f origin master' }
    ) {
        Test-HardRule -CommandText $Cmd -SignalText '' -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -Match 'forbidden-command'
    }

    It 'does NOT trip on a non-protected push: <Cmd>' -ForEach @(
        @{ Cmd = 'git push origin feature/widget' }
        @{ Cmd = 'git push origin ai-dispatch/ISSUE-42' }
    ) {
        Test-HardRule -CommandText $Cmd -SignalText '' -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -BeNullOrEmpty
    }

    It 'does NOT trip when the dangerous push appears only in non-command text (review #3)' {
        Test-HardRule -CommandText '' -SignalText 'note: do not git push origin main' `
            -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 | Should -BeNullOrEmpty
    }
}

Describe 'Test-HardRule signal patterns' {
    It 'trips on failure signal: <Sig>' -ForEach @(
        @{ Sig = 'VERIFY FAILED: gate' }
        @{ Sig = 'GATE_EXIT=101' }
        @{ Sig = 'HANDOFF_STATUS: NEEDS_HUMAN' }
        @{ Sig = 'control verdict=block' }
    ) {
        Test-HardRule -CommandText '' -SignalText $Sig -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -Match 'forbidden-signal'
    }

    It 'does NOT trip on a passing gate / pass verdict: <Sig>' -ForEach @(
        @{ Sig = 'VERIFY OK: all 7 verification step(s) passed.' }
        @{ Sig = 'GATE_EXIT=0' }
        @{ Sig = 'control verdict=pass' }
    ) {
        Test-HardRule -CommandText '' -SignalText $Sig -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -BeNullOrEmpty
    }
}

Describe 'Test-HardRule stall limit' {
    It 'trips when the no-progress duration reaches the stall limit' {
        Test-HardRule -CommandText '' -SignalText '' -ElapsedMinutes 1 -StallElapsed 999 -CorrectionRounds 0 |
            Should -Match 'stalled'
    }
}
