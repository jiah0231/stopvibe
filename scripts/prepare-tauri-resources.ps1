param(
    [switch]$Dev
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$uiDir = Join-Path $repoRoot "ui"
$srcTauriDir = Join-Path $repoRoot "src-tauri"
$releaseDir = Join-Path $repoRoot "target\release"

Push-Location $repoRoot
try {
    cargo build --release -p stopvibe-service
}
finally {
    Pop-Location
}

Copy-Item -LiteralPath (Join-Path $releaseDir "stopvibe-service.exe") -Destination (Join-Path $srcTauriDir "stopvibe-service.exe") -Force
Remove-Item -LiteralPath (Join-Path $srcTauriDir "stopvibe-stub.exe") -Force -ErrorAction SilentlyContinue

if ($Dev) {
    Push-Location $uiDir
    try {
        npm run dev
    }
    finally {
        Pop-Location
    }
}
else {
    Push-Location $uiDir
    try {
        npm run build
    }
    finally {
        Pop-Location
    }
}
