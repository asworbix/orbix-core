# Build the orbix/bob CLI release binaries and stage them as Tauri sidecars
# (bundle.externalBin): binaries\orbix-<target-triple>.exe and
# bob-<target-triple>.exe. The bundler installs sidecars next to the app
# executable and Authenticode-signs them with it (bundle resources are not
# signed). The desktop app's "Install orbix CLI" command copies the sidecar
# from there onto the user's PATH.
#
# Windows counterpart of build-bob.sh. Run as part of `npm run tauri:build`.
# ORBIX_TARGET overrides the target triple (defaults to the rustc host).
$ErrorActionPreference = "Stop"

$Here = Split-Path -Parent $PSScriptRoot          # src-tauri/
$Orbixcore = Split-Path -Parent $Here             # orbixcore/
$Repo = Split-Path -Parent $Orbixcore             # repo root
$BobSrc = Join-Path $Repo "bob-cli"
$Dest = Join-Path $Here "binaries"
New-Item -ItemType Directory -Force $Dest | Out-Null

$Host_ = ((rustc -vV | Select-String '^host: ').Line) -replace '^host: ', ''
$Triple = if ($env:ORBIX_TARGET) { $env:ORBIX_TARGET } else { $Host_ }

if (-not (Test-Path $BobSrc)) {
    Write-Error "[bob:build] $BobSrc missing; cannot build bob CLI"
}

Write-Host "[bob:build] building orbix + bob from $BobSrc for $Triple"
$CargoArgs = @("build", "--release", "--bin", "orbix", "--bin", "bob", "--quiet")
$OutDir = Join-Path $BobSrc "target\release"
if ($Triple -ne $Host_) {
    $CargoArgs += @("--target", $Triple)
    $OutDir = Join-Path $BobSrc "target\$Triple\release"
}
Push-Location $BobSrc
try {
    cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}

# orbix is the primary command; bob is a back-compat alias built from the
# same crate. Sidecar names carry the target triple; the bundler strips it.
Copy-Item -Force (Join-Path $OutDir "orbix.exe") (Join-Path $Dest "orbix-$Triple.exe")
Copy-Item -Force (Join-Path $OutDir "bob.exe") (Join-Path $Dest "bob-$Triple.exe")
$sz = (Get-Item (Join-Path $Dest "orbix-$Triple.exe")).Length
Write-Host "[bob:build] staged $Dest\orbix-$Triple.exe ($sz bytes) + bob-$Triple.exe"
