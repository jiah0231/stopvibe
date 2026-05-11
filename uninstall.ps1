#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

$dest = Join-Path $env:ProgramFiles "StopVibe"
$serviceExe = Join-Path $dest "stopvibe-service.exe"

$service = Get-Service -Name "StopVibeService" -ErrorAction SilentlyContinue
if ($service -and $service.Status -eq "Running") {
    Stop-Service -Name "StopVibeService" -ErrorAction SilentlyContinue
}

if (Test-Path -LiteralPath $serviceExe) {
    & $serviceExe --uninstall
}
else {
    sc.exe delete StopVibeService | Out-Null
}

schtasks.exe /Delete /TN StopVibeWatchdog /F 2>$null | Out-Null

if (Test-Path -LiteralPath $dest) {
    Remove-Item -LiteralPath $dest -Recurse -Force
}

Write-Host "StopVibe uninstalled"
