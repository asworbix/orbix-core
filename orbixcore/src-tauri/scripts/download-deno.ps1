# Download the Deno binary for the host Tauri target triple and place it
# under src-tauri/binaries/deno-<target>.exe so the externalBin entry in
# tauri.conf.json picks it up.
#
# Windows counterpart of download-deno.sh (kept for CI / mac parity).
# Run before `npm run tauri build`. Output is .gitignored - the binary is
# ~40MB and re-downloaded at build time rather than committed.
$ErrorActionPreference = "Stop"

$DenoVersion = if ($env:DENO_VERSION) { $env:DENO_VERSION } else { "v2.5.4" }
$Here = Split-Path -Parent $PSScriptRoot   # src-tauri/
$BinDir = Join-Path $Here "binaries"
New-Item -ItemType Directory -Force $BinDir | Out-Null

# Host triple via rustc
$Triple = (& rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
if (-not $Triple) { $Triple = "x86_64-pc-windows-msvc" }
if ($Triple -notlike "*windows*") {
    Write-Error "This script targets Windows hosts; use download-deno.sh for $Triple"
}

$Asset = "deno-$Triple.zip"
$Exe = Join-Path $BinDir "deno-$Triple.exe"

# Tauri's build.rs drops a tiny placeholder so cargo's externalBin
# existence check passes on fresh checkouts. Re-download anything
# smaller than 1 MB so the real binary always wins.
if (Test-Path $Exe) {
    $sz = (Get-Item $Exe).Length
    if ($sz -gt 1000000) {
        Write-Host "[deno-download] $Exe already present ($sz bytes), skipping"
        exit 0
    }
    Write-Host "[deno-download] $Exe is only $sz bytes (placeholder); re-fetching"
    Remove-Item -Force $Exe
}

$Url = "https://github.com/denoland/deno/releases/download/$DenoVersion/$Asset"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("deno-dl-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $Tmp | Out-Null
try {
    Write-Host "[deno-download] fetching $DenoVersion for $Triple"
    $ZipPath = Join-Path $Tmp $Asset
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
    Expand-Archive -Path $ZipPath -DestinationPath $Tmp -Force
    Move-Item -Force (Join-Path $Tmp "deno.exe") $Exe
    Write-Host "[deno-download] wrote $Exe"
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
