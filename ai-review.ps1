#Requires -Version 5.1
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$projectDir = (Get-Location).Path
$aiDir = Join-Path $projectDir ".ai"

if (-not (Test-Path $aiDir)) { New-Item -ItemType Directory -Path $aiDir -Force | Out-Null }

& git rev-parse --is-inside-work-tree *> $null
if ($LASTEXITCODE -ne 0) {
    [Console]::Error.WriteLine("Not inside a Git repository")
    exit 1
}

$briefSchema = Join-Path $aiDir "claude_brief.schema.json"
$reviewSchema = Join-Path $aiDir "codex_review.schema.json"

if (-not (Test-Path $briefSchema)) {
    [Console]::Error.WriteLine("Missing $briefSchema")
    exit 1
}
if (-not (Test-Path $reviewSchema)) {
    [Console]::Error.WriteLine("Missing $reviewSchema")
    exit 1
}

$diffPath = Join-Path $aiDir "current.diff"
& git diff | Out-File -FilePath $diffPath -Encoding utf8

if ((Get-Item $diffPath).Length -eq 0) {
    Write-Output "No uncommitted diff to review."
    exit 0
}

Write-Output "Creating Claude brief..."
$briefSchemaContent = Get-Content $briefSchema -Raw
$briefEnvelope = Join-Path $aiDir "claude_brief.envelope.json"
$briefOut = Join-Path $aiDir "claude_brief.json"

& claude -p --output-format json --json-schema $briefSchemaContent `
    "Analyze .ai/current.diff and produce a concise review brief for Codex. Focus on correctness, regressions, security, test gaps, and edge cases." `
| Out-File -FilePath $briefEnvelope -Encoding utf8

Get-Content $briefEnvelope -Raw | jq '.structured_output' | Out-File -FilePath $briefOut -Encoding utf8

Write-Output "Running Codex review..."
$reviewOut = Join-Path $aiDir "codex_review.json"

& codex exec --sandbox read-only --output-schema $reviewSchema --output-last-message $reviewOut `
    "Review the repository and .ai/current.diff. Use .ai/claude_brief.json as context. Return schema-compliant JSON only. Do not edit files."

Write-Output "Review result:"
Get-Content $reviewOut -Raw | jq .

$verdict = ((Get-Content $reviewOut -Raw | jq -r '.verdict') -replace '\s', '')

switch ($verdict) {
    "pass" { Write-Output "Codex verdict: pass"; exit 0 }
    "needs_changes" { Write-Output "Codex verdict: needs_changes"; exit 2 }
    "block" { Write-Output "Codex verdict: block"; exit 3 }
    default {
        [Console]::Error.WriteLine("Unknown Codex verdict: $verdict")
        exit 4
    }
}
