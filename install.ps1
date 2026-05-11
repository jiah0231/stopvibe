#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:ProgramFiles "StopVibe"
$releaseDir = Join-Path $repoRoot "target\release"

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\prepare-tauri-resources.ps1")

Push-Location $repoRoot
try {
    cargo build --release
}
finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item -LiteralPath (Join-Path $releaseDir "stopvibe-service.exe") -Destination $dest -Force

$guiExe = Join-Path $releaseDir "stopvibe-tauri.exe"
if (Test-Path -LiteralPath $guiExe) {
    Copy-Item -LiteralPath $guiExe -Destination (Join-Path $dest "StopVibe.exe") -Force
}

$service = Get-Service -Name "StopVibeService" -ErrorAction SilentlyContinue
if (-not $service) {
    & (Join-Path $dest "stopvibe-service.exe") --install
}

$service = Get-Service -Name "StopVibeService" -ErrorAction SilentlyContinue
if ($service -and $service.Status -ne "Running") {
    Start-Service -Name "StopVibeService"
}

Write-Host "StopVibe installed to $dest"
