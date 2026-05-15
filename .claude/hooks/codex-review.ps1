#Requires -Version 5.1
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$projectDir = if ($env:CLAUDE_PROJECT_DIR) { $env:CLAUDE_PROJECT_DIR } else { (Get-Location).Path }
Set-Location $projectDir

$aiDir = Join-Path $projectDir ".ai"
if (-not (Test-Path $aiDir)) { New-Item -ItemType Directory -Path $aiDir -Force | Out-Null }

$hookEvent = [Console]::In.ReadToEnd()
$hookEvent | Out-File -FilePath (Join-Path $aiDir "last_claude_hook_event.json") -Encoding utf8

& git rev-parse --is-inside-work-tree *> $null
if ($LASTEXITCODE -ne 0) { exit 0 }

$diffPath = Join-Path $aiDir "latest.diff"
& git diff | Out-File -FilePath $diffPath -Encoding utf8

if ((Get-Item $diffPath).Length -eq 0) { exit 0 }

$schemaPath = Join-Path $aiDir "codex_review.schema.json"
if (-not (Test-Path $schemaPath)) {
    [Console]::Error.WriteLine("Missing $schemaPath")
    exit 0
}

$reviewPath = Join-Path $aiDir "codex_last_review.json"
$prompt = "Review .ai/latest.diff for correctness, regressions, security bugs, race conditions, data loss, and test gaps. Return schema-compliant JSON only. Do not edit files."

& codex exec --sandbox read-only --output-schema $schemaPath --output-last-message $reviewPath $prompt

if ($LASTEXITCODE -eq 0) {
    $summary = "Codex review completed."
    try {
        $reviewJson = Get-Content $reviewPath -Raw | ConvertFrom-Json
        if ($reviewJson.summary) { $summary = $reviewJson.summary }
    } catch {}
    $output = @{
        hookSpecificOutput = @{
            hookEventName     = "PostToolUse"
            additionalContext = "Codex review finished: $summary. Full review: .ai/codex_last_review.json"
        }
    }
    Write-Output ($output | ConvertTo-Json -Compress -Depth 4)
}
else {
    $output = @{
        systemMessage = "Codex review failed. Check .ai/ and Claude hook debug logs."
    }
    Write-Output ($output | ConvertTo-Json -Compress -Depth 4)
}
