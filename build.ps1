# build.ps1 - Standardized Build & Packaging Entrypoint for ClipShrinker
[CmdletBinding()]
param(
    [string]$BuildNum = "1"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = $PSScriptRoot }
if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = (Get-Location).Path }

$packScript = Join-Path $ScriptDir "pack.ps1"
if (-not (Test-Path $packScript)) {
    Write-Error "Underlying pack.ps1 script not found at: $packScript"
}

Write-Host "Delegating build execution to pack.ps1 (BuildNum: $BuildNum)..." -ForegroundColor Cyan
& $packScript -BuildNum $BuildNum
