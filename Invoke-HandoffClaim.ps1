#Requires -Version 5.1
<#
.SYNOPSIS
    Standalone ADR-121 claim/lock helper for AI handoff dispatches.

.DESCRIPTION
    Advisory tooling for the ADR-121 claim primitive. It acquires a live
    dispatch lock by atomically creating `.ai/handoff-claims/<DISPATCH_ID>/`
    and records durable append-only events under `ai_handoffs/claims/`.
    Use `-LiveRoot` when the live lock must be shared from a primary checkout
    while append-only events are written under an isolated worktree root.

    This script is deliberately not wired into Invoke-AiDispatchLoop.ps1,
    Invoke-AiDispatchQueue.ps1, Invoke-AiDispatchAuto.ps1, or
    .ai/dispatch.verify.ps1.
#>
[CmdletBinding()]
param(
    [ValidateSet('Status', 'Claim', 'Renew', 'Release', 'Reclaim')]
    [string]$Action = 'Status',

    [string]$DispatchId,

    [string]$Actor = '',

    [string]$Harness = 'manual',

    [string]$Branch = '',

    [ValidateRange(1, 604800)]
    [int]$TtlSeconds = 3600,

    [string]$Root = (Get-Location).Path,

    [string]$LiveRoot = '',

    [switch]$JsonOnly,

    [switch]$Blocking
)

$ErrorActionPreference = 'Stop'

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Test-HandoffClaimDispatchId {
    param([Parameter(Mandatory)][string]$Id)
    return ($Id -match '^[A-Za-z0-9][A-Za-z0-9._-]*$')
}

function ConvertTo-HandoffClaimTimestamp {
    param([Parameter(Mandatory)][DateTimeOffset]$Time)
    return $Time.ToString('o')
}

function ConvertTo-HandoffClaimStampForFile {
    param([Parameter(Mandatory)][DateTimeOffset]$Time)
    return ($Time.ToString('yyyy-MM-dd_HH-mm-ss-fffffffzzz') -replace ':', '')
}

function Resolve-HandoffClaimRoot {
    param([Parameter(Mandatory)][string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    return $full.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
}

function Get-HandoffClaimPaths {
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [string]$LiveRepoRoot = '',
        [Parameter(Mandatory)][string]$Id
    )
    if (-not (Test-HandoffClaimDispatchId -Id $Id)) {
        throw "invalid dispatch id for claim path: $Id"
    }
    $rootPath = Resolve-HandoffClaimRoot -Path $RepoRoot
    $liveBase = if ([string]::IsNullOrWhiteSpace($LiveRepoRoot)) {
        $rootPath
    } else {
        Resolve-HandoffClaimRoot -Path $LiveRepoRoot
    }
    $liveRoot = Join-Path $liveBase '.ai\handoff-claims'
    $eventRoot = Join-Path $rootPath 'ai_handoffs\claims'
    $lockDir = Join-Path $liveRoot $Id
    $recordPath = Join-Path $lockDir 'claim.json'
    return [pscustomobject][ordered]@{
        root = $rootPath
        live_root_base = $liveBase
        live_root = $liveRoot
        event_root = $eventRoot
        lock_dir = $lockDir
        record_path = $recordPath
    }
}

function New-HandoffClaimRecord {
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$ClaimActor,
        [Parameter(Mandatory)][string]$ClaimHarness,
        [Parameter(Mandatory)][string]$ClaimBranch,
        [Parameter(Mandatory)][DateTimeOffset]$Now,
        [Parameter(Mandatory)][int]$Seconds
    )
    return [pscustomobject][ordered]@{
        dispatch_id = $Id
        actor = $ClaimActor
        harness = $ClaimHarness
        branch = $ClaimBranch
        timestamp = ConvertTo-HandoffClaimTimestamp -Time $Now
        ttl_seconds = $Seconds
        pid = $PID
    }
}

function Get-HandoffClaimRecord {
    param([Parameter(Mandatory)][string]$RecordPath)
    if (-not (Test-Path -LiteralPath $RecordPath)) { return $null }
    try {
        return (Get-Content -Raw -LiteralPath $RecordPath | ConvertFrom-Json)
    } catch {
        throw "claim record is not valid JSON: $RecordPath"
    }
}

function Test-HandoffClaimExpired {
    param(
        [Parameter(Mandatory)]$Record,
        [Parameter(Mandatory)][DateTimeOffset]$Now
    )
    $stamp = [DateTimeOffset]::Parse([string]$Record.timestamp)
    $ttl = [int]$Record.ttl_seconds
    return ($stamp.AddSeconds($ttl) -le $Now)
}

function Write-HandoffClaimJson {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)]$Value
    )
    $json = $Value | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))
}

function Write-HandoffClaimRecord {
    param(
        [Parameter(Mandatory)][string]$RecordPath,
        [Parameter(Mandatory)]$Record
    )
    Write-HandoffClaimJson -Path $RecordPath -Value $Record
}

function Write-HandoffClaimEvent {
    param(
        [Parameter(Mandatory)]$Paths,
        [Parameter(Mandatory)][string]$Event,
        [Parameter(Mandatory)]$Record,
        [Parameter(Mandatory)][DateTimeOffset]$Now,
        [hashtable]$Extra = @{}
    )
    if (-not (Test-Path -LiteralPath $Paths.event_root)) {
        New-Item -ItemType Directory -Path $Paths.event_root -Force | Out-Null
    }
    $stamp = ConvertTo-HandoffClaimStampForFile -Time $Now
    $leaf = "$($Record.dispatch_id)_$($stamp)_$Event.json"
    $path = Join-Path $Paths.event_root $leaf
    $eventRecord = [ordered]@{
        dispatch_id = $Record.dispatch_id
        event = $Event
        actor = $Record.actor
        harness = $Record.harness
        branch = $Record.branch
        timestamp = ConvertTo-HandoffClaimTimestamp -Time $Now
        ttl_seconds = [int]$Record.ttl_seconds
    }
    foreach ($key in $Extra.Keys) {
        $eventRecord[$key] = $Extra[$key]
    }
    $attempt = 0
    while (Test-Path -LiteralPath $path) {
        $attempt++
        $path = Join-Path $Paths.event_root "$($Record.dispatch_id)_$($stamp)_$Event.$attempt.json"
    }
    Write-HandoffClaimJson -Path $path -Value ([pscustomobject]$eventRecord)
    return $path
}

function Test-HandoffClaimOwnedBy {
    param(
        [Parameter(Mandatory)]$Record,
        [Parameter(Mandatory)][string]$ClaimActor,
        [Parameter(Mandatory)][string]$ClaimHarness
    )
    return ([string]$Record.actor -eq $ClaimActor -and [string]$Record.harness -eq $ClaimHarness)
}

function Remove-HandoffClaimLockDirectory {
    param([Parameter(Mandatory)]$Paths)
    if (-not (Test-Path -LiteralPath $Paths.lock_dir)) { return }

    $liveRoot = Resolve-HandoffClaimRoot -Path $Paths.live_root
    $target = Resolve-HandoffClaimRoot -Path (Resolve-Path -LiteralPath $Paths.lock_dir).Path
    $prefix = $liveRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $target.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to remove claim lock outside live root: $target"
    }
    Remove-Item -LiteralPath $target -Recurse -Force
}

function New-HandoffClaimResult {
    param(
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)]$Paths,
        [AllowNull()]$Record,
        [string]$Message = '',
        [string]$EventPath = ''
    )
    return [pscustomobject][ordered]@{
        status = $Status
        dispatch_id = if ($Record) { [string]$Record.dispatch_id } else { '' }
        actor = if ($Record) { [string]$Record.actor } else { '' }
        harness = if ($Record) { [string]$Record.harness } else { '' }
        branch = if ($Record) { [string]$Record.branch } else { '' }
        ttl_seconds = if ($Record) { [int]$Record.ttl_seconds } else { 0 }
        lock_path = $Paths.lock_dir
        event_path = $EventPath
        message = $Message
    }
}

function Invoke-HandoffClaim {
    param(
        [Parameter(Mandatory)][ValidateSet('Status', 'Claim', 'Renew', 'Release', 'Reclaim')]
        [string]$ClaimAction,
        [Parameter(Mandatory)][string]$Id,
        [string]$ClaimActor = '',
        [string]$ClaimHarness = 'manual',
        [string]$ClaimBranch = '',
        [int]$Seconds = 3600,
        [string]$RepoRoot = (Get-Location).Path,
        [string]$LiveRepoRoot = '',
        [DateTimeOffset]$Now = [DateTimeOffset]::Now
    )

    if (-not (Test-HandoffClaimDispatchId -Id $Id)) {
        throw "invalid dispatch id: $Id"
    }
    if ($ClaimAction -ne 'Status' -and [string]::IsNullOrWhiteSpace($ClaimActor)) {
        throw "Actor is required for $ClaimAction"
    }

    $paths = Get-HandoffClaimPaths -RepoRoot $RepoRoot -LiveRepoRoot $LiveRepoRoot -Id $Id
    if (-not (Test-Path -LiteralPath $paths.live_root)) {
        New-Item -ItemType Directory -Path $paths.live_root -Force | Out-Null
    }
    if ([string]::IsNullOrWhiteSpace($ClaimBranch)) {
        $ClaimBranch = 'unknown'
        if (Test-Path -LiteralPath (Join-Path $paths.root '.git')) {
            $oldErrorActionPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $detectedBranch = (& git -C $paths.root branch --show-current 2>$null)
                if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($detectedBranch)) {
                    $ClaimBranch = [string]$detectedBranch
                }
            } finally {
                $ErrorActionPreference = $oldErrorActionPreference
            }
        }
    }

    $record = Get-HandoffClaimRecord -RecordPath $paths.record_path
    if ($ClaimAction -eq 'Status') {
        if (-not $record) {
            return New-HandoffClaimResult -Status 'AVAILABLE' -Paths $paths -Record $null -Message 'no live claim'
        }
        if (Test-HandoffClaimExpired -Record $record -Now $Now) {
            return New-HandoffClaimResult -Status 'STALE' -Paths $paths -Record $record -Message 'live claim expired'
        }
        return New-HandoffClaimResult -Status 'LIVE' -Paths $paths -Record $record -Message 'live claim active'
    }

    if ($ClaimAction -eq 'Claim') {
        if (-not $record) {
            try {
                New-Item -ItemType Directory -Path $paths.lock_dir -ErrorAction Stop | Out-Null
            } catch {
                $record = Get-HandoffClaimRecord -RecordPath $paths.record_path
                if ($record) {
                    return New-HandoffClaimResult -Status 'BLOCKED' -Paths $paths -Record $record -Message 'claim appeared during acquire'
                }
                throw
            }
            $newRecord = New-HandoffClaimRecord -Id $Id -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness `
                -ClaimBranch $ClaimBranch -Now $Now -Seconds $Seconds
            Write-HandoffClaimRecord -RecordPath $paths.record_path -Record $newRecord
            $eventPath = Write-HandoffClaimEvent -Paths $paths -Event 'claim' -Record $newRecord -Now $Now
            return New-HandoffClaimResult -Status 'CLAIMED' -Paths $paths -Record $newRecord -EventPath $eventPath
        }
        if (Test-HandoffClaimExpired -Record $record -Now $Now) {
            return New-HandoffClaimResult -Status 'STALE' -Paths $paths -Record $record -Message 'existing claim expired; use Reclaim'
        }
        if (Test-HandoffClaimOwnedBy -Record $record -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness) {
            return New-HandoffClaimResult -Status 'OWNED' -Paths $paths -Record $record -Message 'claim already owned by actor'
        }
        return New-HandoffClaimResult -Status 'BLOCKED' -Paths $paths -Record $record -Message 'live claim owned by another actor'
    }

    if (-not $record) {
        return New-HandoffClaimResult -Status 'AVAILABLE' -Paths $paths -Record $null -Message 'no live claim'
    }

    $expired = Test-HandoffClaimExpired -Record $record -Now $Now
    if ($ClaimAction -eq 'Renew') {
        if ($expired) {
            return New-HandoffClaimResult -Status 'STALE' -Paths $paths -Record $record -Message 'cannot renew expired claim'
        }
        if (-not (Test-HandoffClaimOwnedBy -Record $record -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness)) {
            return New-HandoffClaimResult -Status 'BLOCKED' -Paths $paths -Record $record -Message 'cannot renew another actor claim'
        }
        $newRecord = New-HandoffClaimRecord -Id $Id -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness `
            -ClaimBranch $ClaimBranch -Now $Now -Seconds $Seconds
        Write-HandoffClaimRecord -RecordPath $paths.record_path -Record $newRecord
        $eventPath = Write-HandoffClaimEvent -Paths $paths -Event 'renew' -Record $newRecord -Now $Now
        return New-HandoffClaimResult -Status 'RENEWED' -Paths $paths -Record $newRecord -EventPath $eventPath
    }

    if ($ClaimAction -eq 'Release') {
        if (-not (Test-HandoffClaimOwnedBy -Record $record -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness)) {
            return New-HandoffClaimResult -Status 'BLOCKED' -Paths $paths -Record $record -Message 'cannot release another actor claim'
        }
        $eventPath = Write-HandoffClaimEvent -Paths $paths -Event 'release' -Record $record -Now $Now
        Remove-HandoffClaimLockDirectory -Paths $paths
        return New-HandoffClaimResult -Status 'RELEASED' -Paths $paths -Record $record -EventPath $eventPath
    }

    if ($ClaimAction -eq 'Reclaim') {
        if (-not $expired) {
            if (Test-HandoffClaimOwnedBy -Record $record -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness) {
                return New-HandoffClaimResult -Status 'OWNED' -Paths $paths -Record $record -Message 'claim already owned by actor'
            }
            return New-HandoffClaimResult -Status 'BLOCKED' -Paths $paths -Record $record -Message 'cannot reclaim live claim'
        }
        [void](Write-HandoffClaimEvent -Paths $paths -Event 'expire' -Record $record -Now $Now `
            -Extra @{ previous_actor = [string]$record.actor; previous_harness = [string]$record.harness })
        Remove-HandoffClaimLockDirectory -Paths $paths
        New-Item -ItemType Directory -Path $paths.lock_dir -ErrorAction Stop | Out-Null
        $newRecord = New-HandoffClaimRecord -Id $Id -ClaimActor $ClaimActor -ClaimHarness $ClaimHarness `
            -ClaimBranch $ClaimBranch -Now $Now -Seconds $Seconds
        Write-HandoffClaimRecord -RecordPath $paths.record_path -Record $newRecord
        $eventPath = Write-HandoffClaimEvent -Paths $paths -Event 'reclaim' -Record $newRecord -Now $Now
        return New-HandoffClaimResult -Status 'RECLAIMED' -Paths $paths -Record $newRecord -EventPath $eventPath
    }

    throw "unsupported claim action: $ClaimAction"
}

if ($env:RGE_HANDOFF_CLAIM_SKIP_MAIN -eq '1') { return }

if ([string]::IsNullOrWhiteSpace($DispatchId)) {
    Fail 'DispatchId is required unless RGE_HANDOFF_CLAIM_SKIP_MAIN=1 is set.'
}

try {
    $result = Invoke-HandoffClaim -ClaimAction $Action -Id $DispatchId -ClaimActor $Actor `
        -ClaimHarness $Harness -ClaimBranch $Branch -Seconds $TtlSeconds -RepoRoot $Root `
        -LiveRepoRoot $LiveRoot
} catch {
    Fail $_.Exception.Message
}

if (-not $JsonOnly) {
    Write-Output "CLAIM_STATUS: $($result.status)"
}
Write-Output ($result | ConvertTo-Json -Depth 8)

if ($Blocking -and $result.status -in @('BLOCKED', 'STALE')) { exit 2 }
exit 0
