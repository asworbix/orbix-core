#!/usr/bin/env bash
# Compile the orbix-speak AVSpeechSynthesizer helper and stage it as a Tauri
# sidecar (bundle.externalBin): src-tauri/binaries/orbix-speak-<target-triple>.
# Sidecars land next to the app executable (Contents/MacOS) and are
# code-signed with the app, which a plain bundle resource is not. The
# compiled binary is gitignored, so a fresh checkout (CI included) has only
# the .swift source; without this step `tauri build` fails sidecar
# validation. Run as part of `npm run tauri:build` (through scripts/stage.mjs).
# ORBIX_TARGET overrides the target triple (defaults to the rustc host).
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"   # src-tauri/
SRC="$HERE/swift/orbix-speak.swift"
TRIPLE="${ORBIX_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
OUT="$HERE/binaries/orbix-speak-$TRIPLE"
mkdir -p "$HERE/binaries"

if [[ ! -f "$SRC" ]]; then
  echo "[swift:build] $SRC missing; cannot build orbix-speak" >&2
  exit 1
fi

if ! command -v swiftc >/dev/null 2>&1; then
  echo "[swift:build] swiftc not found (install Xcode command line tools)" >&2
  exit 1
fi

# Match the app's minimum deployment target (tauri.macos.conf.json
# minimumSystemVersion = 13.0) so the helper loads on the same range of macOS
# versions the .app supports. The source uses macOS 13 voice APIs (.premium),
# so 13.0 is also the floor the code itself requires.
case "$TRIPLE" in
  aarch64-apple-darwin) TARGET_FLAGS=(-target arm64-apple-macos13) ;;
  x86_64-apple-darwin)  TARGET_FLAGS=(-target x86_64-apple-macos13) ;;
  *) echo "[swift:build] unsupported target $TRIPLE" >&2; exit 1 ;;
esac

echo "[swift:build] compiling orbix-speak (${TARGET_FLAGS[*]})"
swiftc -O "${TARGET_FLAGS[@]}" -framework AVFoundation -framework Foundation \
  -o "$OUT" "$SRC"
chmod +x "$OUT"
echo "[swift:build] staged $OUT ($(wc -c < "$OUT") bytes)"
