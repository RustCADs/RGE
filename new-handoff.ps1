#Requires -Version 5.1
<#
.SYNOPSIS
    Scaffold a canonical ai_handoffs/ dispatch packet, or finalize a completed
    packet into its .meta.json sidecar.

.DESCRIPTION
    Two modes:

    Scaffold (default) - creates ONLY the canonical Markdown packet
      ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md
    copied from the matching ai_handoffs/templates/ file. A freshly
    scaffolded packet is an unfilled template and has NO sidecar.

    Finalize (-Finalize) - reads a COMPLETED Markdown packet, parses its
    header and completion footer, rejects any packet that still contains
    template placeholders, and writes the matching
      ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.meta.json
    sidecar from the packet's actual values. The sidecar conforms to
    .ai/handoff.schema.json (handoff-sidecar-v1).

    A sidecar therefore exists only for a finalized packet. This script
    does NOT scaffold root-level <SENDER>to<RECEIVER>_*.md files.

.PARAMETER DispatchId
    Scaffold mode. Stable dispatch identifier, e.g. POSTV0-FOO-001.

.PARAMETER PacketType
    Scaffold mode. One of: TASK, EXEC, REVIEW, CORRECT, CLOSEOUT.

.PARAMETER Author
    Scaffold mode. Role + AI identity, e.g. "Executor / Claude".

.PARAMETER Finalize
    Selects finalize mode.

.PARAMETER PacketPath
    Finalize mode. Path to the completed Markdown packet to finalize.

.PARAMETER DryRun
    Print what would be created (and, for finalize, the derived sidecar
    content) without writing anything.

.EXAMPLE
    .\new-handoff.ps1 -DispatchId POSTV0-FOO-001 -PacketType TASK -Author "Planner / OpenAI Codex"
    .\new-handoff.ps1 -Finalize -PacketPath ai_handoffs/POSTV0-FOO-001_TASK_2026-05-15_10-00-00+0300.md
    .\new-handoff.ps1 -Finalize -PacketPath <path> -DryRun
#>
[CmdletBinding(DefaultParameterSetName = 'Scaffold')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Scaffold')]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$DispatchId,

    [Parameter(Mandatory, ParameterSetName = 'Scaffold')]
    [ValidateSet('TASK', 'EXEC', 'REVIEW', 'CORRECT', 'CLOSEOUT')]
    [string]$PacketType,

    [Parameter(Mandatory, ParameterSetName = 'Scaffold')]
    [string]$Author,

    [Parameter(Mandatory, ParameterSetName = 'Finalize')]
    [switch]$Finalize,

    [Parameter(Mandatory, ParameterSetName = 'Finalize')]
    [string]$PacketPath,

    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

# Intentional validation failure: write to stderr and exit non-zero cleanly
# (no PowerShell terminating-error exception, so callers see a plain exit 1).
function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

$repoRoot   = Split-Path -Parent $MyInvocation.MyCommand.Path
$handoffDir = Join-Path $repoRoot 'ai_handoffs'

$validHandoffStatus = @('COMPLETE', 'FAILED', 'BLOCKED', 'NEEDS_HUMAN')
$validNextRole      = @('EXECUTOR_AI', 'REVIEWER_AI', 'PLANNER_AI', 'HUMAN_ARBITER', 'NONE')
$validStatus        = @('OPEN', 'AWAITING_REVIEW', 'BLOCKED', 'NEEDS_HUMAN', 'APPROVED',
                        'NEEDS_CORRECTION', 'REJECTED', 'CORRECTION_OPEN', 'CLOSED', 'ABANDONED')
$templateFor = @{
    TASK = 'TASK_PACKET.md'; EXEC = 'EXECUTION_REPORT.md'; REVIEW = 'REVIEW_REPORT.md'
    CORRECT = 'CORRECTION_PACKET.md'; CLOSEOUT = 'FINAL_CLOSEOUT.md'
}

function Get-PacketField {
    # First line-anchored "KEY: value" match. Header fields appear before the
    # footer; footer-only keys (HANDOFF_STATUS / NEXT_ROLE / EXIT_CODE) have a
    # single occurrence. STATUS is anchored, so it does not match HANDOFF_STATUS.
    param([string]$Text, [string]$Key)
    $m = [regex]::Match($Text, "(?m)^$([regex]::Escape($Key)):[ \t]*(.+?)[ \t]*$")
    if ($m.Success) { return $m.Groups[1].Value } else { return $null }
}

function Get-RelatedFiles {
    param([string]$Text)
    $items = @()
    $inBlock = $false
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^RELATED_FILES:[ \t]*$') { $inBlock = $true; continue }
        if ($inBlock) {
            if ($line -match '^[ \t]*-[ \t]+(.+?)[ \t]*$') { $items += $Matches[1] }
            elseif ($line.Trim() -ne '') { break }
        }
    }
    return , $items
}

# ---- scaffold mode -------------------------------------------------------

if ($PSCmdlet.ParameterSetName -eq 'Scaffold') {
    $now    = [DateTimeOffset]::Now
    $tsFile = $now.ToString('yyyy-MM-dd_HH-mm-ss') + $now.ToString('zzz').Replace(':', '')
    $packetPath   = Join-Path $handoffDir "${DispatchId}_${PacketType}_${tsFile}.md"
    $templatePath = Join-Path $handoffDir (Join-Path 'templates' $templateFor[$PacketType])

    if ($DryRun) {
        Write-Output "DRY RUN (scaffold) - would create the Markdown packet ONLY:"
        Write-Output "  packet : $packetPath"
        Write-Output "  (from template: $templatePath)"
        Write-Output "  sidecar: none - a scaffolded packet is an unfilled template and has no .meta.json sidecar."
        Write-Output "           Run -Finalize on the completed packet to generate its sidecar."
        return
    }

    if (-not (Test-Path $templatePath)) { Fail "Template not found: $templatePath" }
    if (Test-Path $packetPath) { Fail "Packet already exists: $packetPath" }

    Copy-Item -Path $templatePath -Destination $packetPath
    Write-Output $packetPath
    Write-Output "Scaffolded an unfilled packet (no sidecar). Fill it in, then run:"
    Write-Output "  new-handoff.ps1 -Finalize -PacketPath `"$packetPath`""
    return
}

# ---- finalize mode -------------------------------------------------------

if (-not (Test-Path -LiteralPath $PacketPath)) { Fail "Packet not found: $PacketPath" }
$packetItem = Get-Item -LiteralPath $PacketPath
$packetName = $packetItem.Name

# Canonical packet filename: <DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md
$nameRx = '^(?<id>.+)_(?<type>TASK|EXEC|REVIEW|CORRECT|CLOSEOUT)_(?<ts>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}[+-]\d{4})\.md$'
if ($packetName -notmatch $nameRx) {
    Fail "Not a canonical packet filename (expected <DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md): $packetName"
}
$packetType = $Matches['type']

$text = [System.IO.File]::ReadAllText($packetItem.FullName)

$fields = [ordered]@{
    dispatch_id    = Get-PacketField $text 'DISPATCH_ID'
    author         = Get-PacketField $text 'AUTHOR'
    timestamp      = Get-PacketField $text 'TIMESTAMP'
    status         = Get-PacketField $text 'STATUS'
    handoff_status = Get-PacketField $text 'HANDOFF_STATUS'
    next_role      = Get-PacketField $text 'NEXT_ROLE'
    exit_code      = Get-PacketField $text 'EXIT_CODE'
}
$relatedFiles = Get-RelatedFiles $text

# Reject unfilled templates and malformed packets: any field that is missing,
# still carries a <...> placeholder, or fails its enum / integer shape.
$reasons = @()
foreach ($k in $fields.Keys) {
    if ([string]::IsNullOrWhiteSpace($fields[$k])) { $reasons += "missing field: $k" }
    elseif ($fields[$k] -match '[<>]')             { $reasons += "unfilled placeholder in $k ('$($fields[$k])')" }
}
if ($fields['handoff_status'] -and $fields['handoff_status'] -notmatch '[<>]' -and $fields['handoff_status'] -notin $validHandoffStatus) {
    $reasons += "invalid handoff_status: '$($fields['handoff_status'])'"
}
if ($fields['next_role'] -and $fields['next_role'] -notmatch '[<>]' -and $fields['next_role'] -notin $validNextRole) {
    $reasons += "invalid next_role: '$($fields['next_role'])'"
}
if ($fields['status'] -and $fields['status'] -notmatch '[<>]' -and $fields['status'] -notin $validStatus) {
    $reasons += "invalid status: '$($fields['status'])'"
}
if ($fields['exit_code'] -and $fields['exit_code'] -notmatch '^-?\d+$') {
    $reasons += "non-integer exit_code: '$($fields['exit_code'])'"
}
if ($reasons.Count -gt 0) {
    Fail ("Refusing to finalize - packet is an unfilled template or malformed:`n  - " + ($reasons -join "`n  - "))
}

$sidecar = [ordered]@{
    schema_version = 'handoff-sidecar-v1'
    dispatch_id    = $fields['dispatch_id']
    packet_type    = $packetType
    author         = $fields['author']
    timestamp      = $fields['timestamp']
    related_files  = $relatedFiles
    status         = $fields['status']
    handoff_status = $fields['handoff_status']
    next_role      = $fields['next_role']
    exit_code      = [int]$fields['exit_code']
}
# ConvertTo-Json in Windows PowerShell 5.1 emits double-spaces after colons
# and multi-line empty arrays; clean both for readability.
$json = ($sidecar | ConvertTo-Json -Depth 8) `
    -replace '(?ms)\[\s+\]', '[]' `
    -replace '":  ', '": '
$sidecarPath = $packetItem.FullName -replace '\.md$', '.meta.json'

if ($DryRun) {
    Write-Output "DRY RUN (finalize) - would derive this sidecar from the completed packet:"
    Write-Output "  packet : $($packetItem.FullName)"
    Write-Output "  sidecar: $sidecarPath"
    Write-Output "  --- derived sidecar content ---"
    Write-Output $json
    return
}

if (Test-Path -LiteralPath $sidecarPath) { Fail "Sidecar already exists: $sidecarPath" }
# BOM-less UTF-8 so parsers and grep do not choke on the BOM.
[System.IO.File]::WriteAllText($sidecarPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Output $sidecarPath
