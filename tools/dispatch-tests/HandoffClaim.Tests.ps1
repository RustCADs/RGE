#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

BeforeAll {
    $script:TestsRoot = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:ClaimScriptPath = Join-Path $script:RepoRootForTest 'Invoke-HandoffClaim.ps1'
    if (-not (Test-Path -LiteralPath $script:ClaimScriptPath)) {
        throw "Invoke-HandoffClaim.ps1 not found at $script:ClaimScriptPath"
    }

    $env:RGE_HANDOFF_CLAIM_SKIP_MAIN = '1'
    try {
        . $script:ClaimScriptPath
    } finally {
        Remove-Item Env:RGE_HANDOFF_CLAIM_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    function Get-ClaimRecordPath {
        param(
            [Parameter(Mandatory)][string]$Root,
            [Parameter(Mandatory)][string]$DispatchId
        )
        return Join-Path $Root ".ai\handoff-claims\$DispatchId\claim.json"
    }

    function Get-ClaimEventFile {
        param([Parameter(Mandatory)][string]$Root)
        $dir = Join-Path $Root 'ai_handoffs\claims'
        if (-not (Test-Path -LiteralPath $dir)) { return @() }
        return @(Get-ChildItem -LiteralPath $dir -Filter '*.json' | Sort-Object Name)
    }

    function Read-JsonFile {
        param([Parameter(Mandatory)][string]$Path)
        return (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    }
}

Describe 'Invoke-HandoffClaim status and claim' {
    It 'reports AVAILABLE when no live claim exists' {
        $result = Invoke-HandoffClaim -ClaimAction Status -Id 'CLAIM-PESTER-AVAILABLE' -RepoRoot $TestDrive

        $result.status | Should -Be 'AVAILABLE'
        $result.message | Should -Be 'no live claim'
    }

    It 'creates a live lock record and append-only claim event' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')

        $result = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-CREATE' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'ai-dispatch/test' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now

        $result.status | Should -Be 'CLAIMED'
        $record = Read-JsonFile -Path (Get-ClaimRecordPath -Root $TestDrive -DispatchId 'CLAIM-PESTER-CREATE')
        $record.dispatch_id | Should -Be 'CLAIM-PESTER-CREATE'
        $record.actor | Should -Be 'Codex'
        $record.harness | Should -Be 'Pester'
        $record.branch | Should -Be 'ai-dispatch/test'
        $record.ttl_seconds | Should -Be 60

        $events = Get-ClaimEventFile -Root $TestDrive
        $events.Count | Should -Be 1
        $claimEvent = Read-JsonFile -Path $events[0].FullName
        $claimEvent.event | Should -Be 'claim'
        $claimEvent.dispatch_id | Should -Be 'CLAIM-PESTER-CREATE'
        $claimEvent.actor | Should -Be 'Codex'
    }

    It 'can share the live lock root while writing events under a worktree root' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        $primaryRoot = Join-Path $TestDrive 'primary'
        $worktreeRoot = Join-Path $TestDrive 'worktree-a'
        $otherWorktreeRoot = Join-Path $TestDrive 'worktree-b'
        New-Item -ItemType Directory -Path $primaryRoot, $worktreeRoot, $otherWorktreeRoot -Force |
            Out-Null

        $result = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-SPLIT-ROOT' `
            -ClaimActor 'Codex' -ClaimHarness 'Queue' -ClaimBranch 'ai-dispatch/split' `
            -Seconds 60 -RepoRoot $worktreeRoot -LiveRepoRoot $primaryRoot -Now $now

        $result.status | Should -Be 'CLAIMED'
        Test-Path -LiteralPath (Get-ClaimRecordPath -Root $primaryRoot -DispatchId 'CLAIM-PESTER-SPLIT-ROOT') |
            Should -BeTrue
        (Get-ClaimEventFile -Root $worktreeRoot).Count | Should -Be 1
        (Get-ClaimEventFile -Root $primaryRoot).Count | Should -Be 0

        $blocked = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-SPLIT-ROOT' `
            -ClaimActor 'Claude' -ClaimHarness 'Queue' -ClaimBranch 'ai-dispatch/split' `
            -Seconds 60 -RepoRoot $otherWorktreeRoot -LiveRepoRoot $primaryRoot -Now $now.AddSeconds(1)

        $blocked.status | Should -Be 'BLOCKED'
        $blocked.actor | Should -Be 'Codex'
        (Get-ClaimEventFile -Root $otherWorktreeRoot).Count | Should -Be 0
    }

    It 'blocks another actor while the claim is live' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        [void](Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-BLOCK' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now)

        $blocked = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-BLOCK' `
            -ClaimActor 'Claude' -ClaimHarness 'Pester' -ClaimBranch 'branch-b' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now.AddSeconds(1)

        $blocked.status | Should -Be 'BLOCKED'
        $blocked.actor | Should -Be 'Codex'
        $blocked.message | Should -Match 'another actor'
    }

    It 'reports OWNED for the same actor and harness while live' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        [void](Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-OWNED' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now)

        $owned = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-OWNED' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now.AddSeconds(1)

        $owned.status | Should -Be 'OWNED'
    }
}

Describe 'Invoke-HandoffClaim renew and release' {
    It 'renews only the owning actor claim' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        [void](Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-RENEW' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now)

        $blocked = Invoke-HandoffClaim -ClaimAction Renew -Id 'CLAIM-PESTER-RENEW' `
            -ClaimActor 'Claude' -ClaimHarness 'Pester' -ClaimBranch 'branch-b' `
            -Seconds 120 -RepoRoot $TestDrive -Now $now.AddSeconds(10)
        $blocked.status | Should -Be 'BLOCKED'

        $renewed = Invoke-HandoffClaim -ClaimAction Renew -Id 'CLAIM-PESTER-RENEW' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 120 -RepoRoot $TestDrive -Now $now.AddSeconds(10)
        $renewed.status | Should -Be 'RENEWED'

        $record = Read-JsonFile -Path (Get-ClaimRecordPath -Root $TestDrive -DispatchId 'CLAIM-PESTER-RENEW')
        $record.ttl_seconds | Should -Be 120
        $record.timestamp | Should -Be '2026-06-06T13:00:10.0000000+03:00'
        (Get-ClaimEventFile -Root $TestDrive | Where-Object { $_.Name -match '_renew\.json$' }).Count |
            Should -Be 1
    }

    It 'releases only the owning actor claim and removes live lock state' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        [void](Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-RELEASE' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now)

        $blocked = Invoke-HandoffClaim -ClaimAction Release -Id 'CLAIM-PESTER-RELEASE' `
            -ClaimActor 'Claude' -ClaimHarness 'Pester' -ClaimBranch 'branch-b' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now.AddSeconds(5)
        $blocked.status | Should -Be 'BLOCKED'
        Test-Path -LiteralPath (Split-Path -Parent (Get-ClaimRecordPath -Root $TestDrive -DispatchId 'CLAIM-PESTER-RELEASE')) |
            Should -BeTrue

        $released = Invoke-HandoffClaim -ClaimAction Release -Id 'CLAIM-PESTER-RELEASE' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 60 -RepoRoot $TestDrive -Now $now.AddSeconds(10)
        $released.status | Should -Be 'RELEASED'
        Test-Path -LiteralPath (Split-Path -Parent (Get-ClaimRecordPath -Root $TestDrive -DispatchId 'CLAIM-PESTER-RELEASE')) |
            Should -BeFalse

        $status = Invoke-HandoffClaim -ClaimAction Status -Id 'CLAIM-PESTER-RELEASE' -RepoRoot $TestDrive
        $status.status | Should -Be 'AVAILABLE'
        (Get-ClaimEventFile -Root $TestDrive | Where-Object { $_.Name -match '_release\.json$' }).Count |
            Should -Be 1
    }
}

Describe 'Invoke-HandoffClaim stale and safety semantics' {
    It 'requires explicit Reclaim for expired claims and records expire plus reclaim events' {
        $now = [DateTimeOffset]::Parse('2026-06-06T13:00:00+03:00')
        [void](Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-STALE' `
            -ClaimActor 'Codex' -ClaimHarness 'Pester' -ClaimBranch 'branch-a' `
            -Seconds 1 -RepoRoot $TestDrive -Now $now)
        $later = $now.AddSeconds(2)

        $status = Invoke-HandoffClaim -ClaimAction Status -Id 'CLAIM-PESTER-STALE' -RepoRoot $TestDrive -Now $later
        $status.status | Should -Be 'STALE'

        $plainClaim = Invoke-HandoffClaim -ClaimAction Claim -Id 'CLAIM-PESTER-STALE' `
            -ClaimActor 'Claude' -ClaimHarness 'Pester' -ClaimBranch 'branch-b' `
            -Seconds 60 -RepoRoot $TestDrive -Now $later
        $plainClaim.status | Should -Be 'STALE'

        $reclaimed = Invoke-HandoffClaim -ClaimAction Reclaim -Id 'CLAIM-PESTER-STALE' `
            -ClaimActor 'Claude' -ClaimHarness 'Pester' -ClaimBranch 'branch-b' `
            -Seconds 60 -RepoRoot $TestDrive -Now $later
        $reclaimed.status | Should -Be 'RECLAIMED'

        $record = Read-JsonFile -Path (Get-ClaimRecordPath -Root $TestDrive -DispatchId 'CLAIM-PESTER-STALE')
        $record.actor | Should -Be 'Claude'
        (Get-ClaimEventFile -Root $TestDrive | Where-Object { $_.Name -match '_expire\.json$' }).Count |
            Should -Be 1
        (Get-ClaimEventFile -Root $TestDrive | Where-Object { $_.Name -match '_reclaim\.json$' }).Count |
            Should -Be 1
    }

    It 'rejects dispatch ids that could escape the claim directories' {
        { Invoke-HandoffClaim -ClaimAction Status -Id '..\CLAIM-ESCAPE' -RepoRoot $TestDrive } |
            Should -Throw -ExpectedMessage '*invalid dispatch id*'
    }
}
