# Build the Orbix CLI (orbix.exe + bob.exe) in release mode and put it on your
# PATH, so you can run `orbix connect` from any terminal. Run from the bob-cli
# directory:  powershell -ExecutionPolicy Bypass -File .\install.ps1
$ErrorActionPreference = 'Stop'

Set-Location -Path $PSScriptRoot

# ── Prerequisite: the Rust toolchain ──────────────────────────────────────
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host ''
    Write-Host '[orbix-cli] Rust is not installed.' -ForegroundColor Yellow
    Write-Host '            Install it from https://rustup.rs (accept the defaults),'
    Write-Host '            open a fresh terminal, and run this script again.'
    exit 1
}

Write-Host '[orbix-cli] building release binaries (orbix + bob)...'
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host '[orbix-cli] build failed. See the cargo output above.' -ForegroundColor Red
    exit $LASTEXITCODE
}

# ── Install location (override with $env:ORBIX_INSTALL_DIR) ────────────────
$dest = if ($env:ORBIX_INSTALL_DIR) { $env:ORBIX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Orbix\bin' }
New-Item -ItemType Directory -Force -Path $dest | Out-Null

foreach ($bin in @('orbix.exe', 'bob.exe')) {
    $src = Join-Path 'target\release' $bin
    if (Test-Path $src) {
        Copy-Item -Force -Path $src -Destination (Join-Path $dest $bin)
        Write-Host "[orbix-cli] installed: $(Join-Path $dest $bin)"
    }
}

# ── Ensure the install dir is on the user PATH ────────────────────────────
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$onPath = ($userPath -split ';') -contains $dest
if (-not $onPath) {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $dest } else { "$userPath;$dest" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host ''
    Write-Host "[orbix-cli] added $dest to your user PATH." -ForegroundColor Green
    Write-Host '            Open a NEW terminal for it to take effect.'
}

Write-Host ''
Write-Host 'Try:   orbix connect' -ForegroundColor Cyan
Write-Host '       (on the host device, turn on Settings > Models > Expose local model endpoint,'
Write-Host '        and make sure both devices are on the same network)'
