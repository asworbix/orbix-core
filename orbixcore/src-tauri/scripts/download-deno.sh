#!/usr/bin/env bash
# Download the Deno binary for one or more Tauri target triples and
# place it under src-tauri/binaries/deno-<target> so the externalBin
# entry in tauri.conf.json picks it up.
#
# Why: connector_runtime.rs spawns one Deno subprocess per active
# module. We ship Deno as a sidecar (§12 decision #3) so users do not
# need it on PATH and the binary signs alongside the app bundle.
#
# Run before `npm run tauri build`. CI calls this with --all to fetch
# every target. Local devs typically run it with no args and get only
# their host triple.
#
# Output is .gitignored - these are ~40MB per arch and we re-download
# at build time rather than committing them.
set -euo pipefail

DENO_VERSION="${DENO_VERSION:-v2.5.4}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$HERE/binaries"
mkdir -p "$BIN_DIR"

# Map Tauri target triple → Deno release asset name.
download_one() {
  local triple="$1"
  local asset
  case "$triple" in
    aarch64-apple-darwin)   asset="deno-aarch64-apple-darwin.zip" ;;
    x86_64-apple-darwin)    asset="deno-x86_64-apple-darwin.zip" ;;
    x86_64-unknown-linux-gnu) asset="deno-x86_64-unknown-linux-gnu.zip" ;;
    aarch64-unknown-linux-gnu) asset="deno-aarch64-unknown-linux-gnu.zip" ;;
    x86_64-pc-windows-msvc) asset="deno-x86_64-pc-windows-msvc.zip" ;;
    *) echo "unsupported target triple: $triple" >&2; return 1 ;;
  esac

  local out="$BIN_DIR/deno-$triple"
  local exe="$out"
  [[ "$triple" == *windows* ]] && exe="$out.exe"

  # Tauri's build.rs drops a tiny placeholder so cargo's externalBin
  # existence check passes on fresh checkouts. Re-download anything
  # smaller than 1 MB so the real binary always wins.
  if [[ -f "$exe" ]]; then
    local sz
    sz=$(wc -c < "$exe" | tr -d ' ')
    if [[ "$sz" -gt 1000000 ]]; then
      echo "[deno-download] $exe already present (${sz} bytes), skipping"
      return 0
    fi
    echo "[deno-download] $exe is only ${sz} bytes (placeholder); re-fetching"
    rm -f "$exe"
  fi

  local url="https://github.com/denoland/deno/releases/download/$DENO_VERSION/$asset"
  local tmp
  tmp="$(mktemp -d)"
  echo "[deno-download] fetching $DENO_VERSION for $triple"
  curl -fL --retry 3 -o "$tmp/$asset" "$url"
  unzip -q "$tmp/$asset" -d "$tmp"
  if [[ "$triple" == *windows* ]]; then
    mv "$tmp/deno.exe" "$exe"
  else
    mv "$tmp/deno" "$exe"
    chmod +x "$exe"
  fi
  rm -rf "$tmp"
  echo "[deno-download] wrote $exe"
}

if [[ "${1:-}" == "--all" ]]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin \
           x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
           x86_64-pc-windows-msvc; do
    download_one "$t"
  done
else
  # Detect host triple via rustc, fallback to a sensible default.
  HOST="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
  HOST="${HOST:-aarch64-apple-darwin}"
  download_one "$HOST"
fi
