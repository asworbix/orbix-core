#!/usr/bin/env bash
# Build the orbix/bob CLI release binaries and stage them as Tauri sidecars
# (bundle.externalBin): src-tauri/binaries/orbix-<target-triple> and
# bob-<target-triple>. The bundler copies sidecars next to the app
# executable (Contents/MacOS on macOS) and code-signs them with the app;
# plain bundle resources are never signed, and notarization rejects an
# unsigned Mach-O. The desktop app's "Install orbix CLI" command copies
# the sidecar from there onto the user's PATH.
#
# Run as part of `npm run tauri:build` (through scripts/stage.mjs).
# ORBIX_TARGET overrides the target triple (defaults to the rustc host);
# it must match the `--target` given to `tauri build`.
set -euo pipefail

HERE="$(cd "$(dirname "$0")/../.." && pwd)"   # orbixcore/
REPO="$(cd "$HERE/.." && pwd)"                # repo root
BOB_SRC="$REPO/bob-cli"
DEST="$HERE/src-tauri/binaries"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
TRIPLE="${ORBIX_TARGET:-$HOST}"
mkdir -p "$DEST"

if [[ ! -d "$BOB_SRC" ]]; then
  echo "[bob:build] $BOB_SRC missing; cannot build bob CLI" >&2
  exit 1
fi

echo "[bob:build] building orbix + bob from $BOB_SRC for $TRIPLE"
CARGO_ARGS=(build --release --bin orbix --bin bob --quiet)
OUT_DIR="$BOB_SRC/target/release"
if [[ "$TRIPLE" != "$HOST" ]]; then
  CARGO_ARGS+=(--target "$TRIPLE")
  OUT_DIR="$BOB_SRC/target/$TRIPLE/release"
fi
( cd "$BOB_SRC" && cargo "${CARGO_ARGS[@]}" )

# orbix is the primary command; bob is a back-compat alias built from the
# same crate. Sidecar names carry the target triple; the bundler strips it.
cp -f "$OUT_DIR/orbix" "$DEST/orbix-$TRIPLE"
cp -f "$OUT_DIR/bob" "$DEST/bob-$TRIPLE"
chmod +x "$DEST/orbix-$TRIPLE" "$DEST/bob-$TRIPLE"
echo "[bob:build] staged $DEST/orbix-$TRIPLE ($(wc -c < "$DEST/orbix-$TRIPLE") bytes) + bob-$TRIPLE"
