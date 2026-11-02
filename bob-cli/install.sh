#!/usr/bin/env bash
# Build the orbix CLI in release mode and drop it into ~/.local/bin so
# it's on PATH. `bob` is installed as a symlink to the same binary.
# Run from the bob-cli/ directory.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

echo "[orbix-cli] building release binaries…"
cargo build --release --bin orbix --bin bob

DEST="${BOB_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$DEST"
cp -f target/release/orbix "$DEST/orbix"
chmod +x "$DEST/orbix"
# bob is the back-compat alias: symlink when possible, copy as fallback.
ln -sf "$DEST/orbix" "$DEST/bob" 2>/dev/null || {
  cp -f target/release/bob "$DEST/bob"
  chmod +x "$DEST/bob"
}

echo "[orbix-cli] installed: $DEST/orbix (and bob -> orbix)"
if ! echo ":$PATH:" | grep -q ":$DEST:"; then
  echo
  echo "[orbix-cli] note: $DEST is not on PATH."
  echo "            add this to your shell rc:"
  echo "            export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo
echo "Try:   orbix \"what's in this directory\""
echo "       (the model server starts itself if Orbix Core is installed)"
