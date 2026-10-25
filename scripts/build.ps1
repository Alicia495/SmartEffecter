param([switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\dev-env.ps1"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    if ($DebugBuild) { cargo build --locked } else { cargo rustc --locked --bin guitar-smart-effecter -- -C opt-level=3 }
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    $profile = 'debug'
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item "target\$profile\guitar-smart-effecter.exe" dist\GuitarSmartEffecter.exe
    Copy-Item README.md dist\README.md
    if (Test-Path TEST_REPORT.md) { Copy-Item TEST_REPORT.md dist\TEST_REPORT.md }
    python scripts\package-notices.py
    if ($LASTEXITCODE -ne 0) { throw 'Packaging notices failed' }
    Write-Host 'Ready: dist\GuitarSmartEffecter.exe'
} finally { Pop-Location }
