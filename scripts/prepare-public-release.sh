#!/usr/bin/env bash
# Assemble the sanitized public snapshot of Orbix Core.
#
# The public repository starts from a clean tree, not from this repository's
# history: private components and internal documents stay behind
# (scripts/public-exclude.txt), the result is scanned for secrets, em dashes,
# and personal paths, and with --commit it becomes a fresh git repository with
# one "Initial open-source release" commit, ready to push.
#
#   scripts/prepare-public-release.sh [output-dir] [--commit]
#
# Defaults to ../orbix-core-public next to this repository. Only tracked files
# are copied, so untracked local state never ships.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
EXCLUDE="$ROOT/scripts/public-exclude.txt"
OUT=""
COMMIT=0
for arg in "$@"; do
  case "$arg" in
    --commit) COMMIT=1 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) OUT="$arg" ;;
  esac
done
OUT=$(cd "$(dirname "${OUT:-$ROOT/../orbix-core-public}")" && pwd)/$(basename "${OUT:-$ROOT/../orbix-core-public}")

echo "Source  $ROOT"
echo "Output  $OUT"
rm -rf "$OUT"
mkdir -p "$OUT"

# ── Copy tracked files minus the exclude list ───────────────────────────
git -C "$ROOT" ls-files -z \
  | python3 -c '
import sys, fnmatch
rules = [l.strip() for l in open(sys.argv[1]) if l.strip() and not l.startswith("#")]
def excluded(p):
    for r in rules:
        if r.endswith("/") and p.startswith(r): return True
        if "*" in r and fnmatch.fnmatch(p, r): return True
        if p == r: return True
    return False
paths = [p for p in sys.stdin.buffer.read().split(b"\0") if p]
kept = [p for p in paths if not excluded(p.decode())]
sys.stdout.buffer.write(b"\0".join(kept) + (b"\0" if kept else b""))
print(f"{len(kept)} files kept, {len(paths) - len(kept)} excluded", file=sys.stderr)
' "$EXCLUDE" \
  | rsync -a --from0 --files-from=- "$ROOT/" "$OUT/"

# ── Checks ──────────────────────────────────────────────────────────────
fail=0

echo "== secrets =="
if command -v gitleaks >/dev/null 2>&1; then
  if gitleaks detect --no-git --source "$OUT" --redact --exit-code 1 >/dev/null 2>&1; then
    echo "clean"
  else
    echo "FOUND: gitleaks detect --no-git --source \"$OUT\" --redact"
    fail=1
  fi
else
  echo "gitleaks not installed (brew install gitleaks); skipped"
fi

echo "== em dashes (U+2014) =="
if hits=$(grep -rl $'\xe2\x80\x94' "$OUT" --exclude-dir=node_modules --exclude-dir=target \
    --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.md' --include='*.json' --include='*.yml' --include='*.js' --include='*.html' 2>/dev/null) && [ -n "$hits" ]; then
  echo "$hits"; fail=1
else
  echo "clean"
fi

echo "== personal paths =="
if hits=$(grep -rnE "/Users/[a-z]+/" "$OUT" --exclude-dir=node_modules --exclude-dir=target 2>/dev/null | grep -vE "/Users/(you|me|x|username|<[a-z]+>)/") && [ -n "$hits" ]; then
  echo "$hits"; fail=1
else
  echo "clean"
fi

echo "== private names =="
if hits=$(grep -rniE "auth-proxy/|orbixcore-ios/|research/tool-call-eval|research/replay-eval" "$OUT" --exclude-dir=node_modules --include='*.md' --include='*.yml' 2>/dev/null) && [ -n "$hits" ]; then
  echo "$hits"; fail=1
else
  echo "clean"
fi

if [ "$fail" -ne 0 ]; then
  echo
  echo "Snapshot has problems (see above). Nothing was committed."
  exit 1
fi

# ── Fresh history ───────────────────────────────────────────────────────
if [ "$COMMIT" -eq 1 ]; then
  git -C "$OUT" init -q -b main
  git -C "$OUT" add -A
  git -C "$OUT" commit -q -s -m "Initial open-source release"
  echo
  echo "Committed. Next:"
  echo "  gh repo create orbix-core --public --source \"$OUT\" --push"
  echo "  git -C \"$OUT\" tag v0.1.0 && git -C \"$OUT\" push origin v0.1.0"
else
  echo
  echo "Snapshot ready (no git history). Re-run with --commit to create the initial commit."
fi
