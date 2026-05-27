#Requires -Version 5.1
<#
.SYNOPSIS
    Static parameter-introspection tests proving Invoke-AiDispatchAuto.ps1
    and Register-AiDispatchSchedule.ps1 accept `-PublishMode pr` without
    running a live dispatch, hitting GitHub, or registering a Scheduled Task.

.DESCRIPTION
    The two outer-layer scripts do not have a dot-source seam like the queue
    (no test seam env var, no pure helpers that load without side effects),
    so direct end-to-end invocation in a unit-test would need `gh` auth and
    would file real issues / open real PRs / register real scheduled tasks.

    Instead, parse the scripts with the PowerShell AST and assert that the
    `-PublishMode` parameter exists and its ValidateSet attribute contains
    'pr'. This proves the parameter contract without any I/O.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)

    function script:Get-ValidateSetValues {
        param([string]$ScriptPath, [string]$ParameterName)
        $tokens = $null
        $errors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            $ScriptPath, [ref]$tokens, [ref]$errors)
        if ($errors -and $errors.Count -gt 0) {
            throw "Parser errors in $($ScriptPath): " + ($errors | ForEach-Object { $_.Message }) -join '; '
        }
        $paramAsts = $ast.FindAll({
            param($n)
            $n -is [System.Management.Automation.Language.ParameterAst] -and
            $n.Name.VariablePath.UserPath -eq $ParameterName
        }, $true)
        foreach ($p in $paramAsts) {
            foreach ($attr in $p.Attributes) {
                if ($attr -is [System.Management.Automation.Language.AttributeAst] -and
                    $attr.TypeName.Name -eq 'ValidateSet') {
                    return @($attr.PositionalArguments | ForEach-Object { $_.Value })
                }
            }
        }
        return $null
    }
}

Describe 'ISSUE-230: -PublishMode pr surfaces in outer-layer scripts' {

    It 'Invoke-AiDispatchQueue.ps1 -PublishMode accepts pr' {
        $values = Get-ValidateSetValues `
            -ScriptPath (Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1') `
            -ParameterName 'PublishMode'
        $values | Should -Not -BeNullOrEmpty
        $values | Should -Contain 'pr'
        $values | Should -Contain 'main'
        $values | Should -Contain 'branch'
        # The empty-string member is the queue's "use the default" sentinel
        # that lets Resolve-DispatchPublishMode honour -NoPublish for legacy
        # callers; keep it asserted so a future cleanup does not silently
        # drop the back-compat default path.
        $values | Should -Contain ''
    }

    It 'Invoke-AiDispatchAuto.ps1 -PublishMode accepts pr (parser-level)' {
        $values = Get-ValidateSetValues `
            -ScriptPath (Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1') `
            -ParameterName 'PublishMode'
        $values | Should -Not -BeNullOrEmpty
        $values | Should -Contain 'pr'
        $values | Should -Contain 'main'
        $values | Should -Contain 'branch'
    }

    It 'Register-AiDispatchSchedule.ps1 -PublishMode accepts pr (parser-level)' {
        $values = Get-ValidateSetValues `
            -ScriptPath (Join-Path $script:RepoRootForTest 'Register-AiDispatchSchedule.ps1') `
            -ParameterName 'PublishMode'
        $values | Should -Not -BeNullOrEmpty
        $values | Should -Contain 'pr'
        $values | Should -Contain 'main'
        $values | Should -Contain 'branch'
    }
}
