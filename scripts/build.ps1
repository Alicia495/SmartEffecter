param([switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\dev-env.ps1"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    if ($DebugBuild) { cargo build --locked } else { cargo rustc --locked --bin guitar-smart-effecter -- -C opt-level=3 }
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    $profile = 'debug'
    New-Item -ItemType Directory -Force dist | Out-Null
    # Stage and rename instead of overwriting an executable used by a running app.
    $packageDir = (Resolve-Path dist).Path
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
    $destination = Join-Path $packageDir 'GuitarSmartEffecter.exe'
    $staged = Join-Path $packageDir "GuitarSmartEffecter.new-$stamp.exe"
    $previous = Join-Path $packageDir "GuitarSmartEffecter.previous-$stamp.exe"
    Copy-Item "target\$profile\guitar-smart-effecter.exe" -Destination $staged
    if (Test-Path -LiteralPath $destination) { Move-Item -LiteralPath $destination -Destination $previous }
    try { Move-Item -LiteralPath $staged -Destination $destination }
    catch {
        if (!(Test-Path -LiteralPath $destination) -and (Test-Path -LiteralPath $previous)) {
            Move-Item -LiteralPath $previous -Destination $destination
        }
        throw
    }
    Copy-Item README.md dist\README.md
    Copy-Item LICENSE dist\LICENSE
    Copy-Item CHANGELOG.md dist\CHANGELOG.md
    New-Item -ItemType Directory -Force dist\docs | Out-Null
    Copy-Item docs\* dist\docs -Recurse -Force
    if (Test-Path TEST_REPORT.md) { Copy-Item TEST_REPORT.md dist\TEST_REPORT.md }
    python scripts\package-notices.py
    if ($LASTEXITCODE -ne 0) { throw 'Packaging notices failed' }
    Write-Host 'Ready: dist\GuitarSmartEffecter.exe'
} finally { Pop-Location }
