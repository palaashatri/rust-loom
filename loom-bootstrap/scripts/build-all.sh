#!/usr/bin/env bash
# Build every existing cargo workspace in the suite.
# Usage: build-all.sh [--release|--debug]   (default: --release)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

MODE="${1:---release}"
case "$MODE" in
  --release) PROFILE="release" CARGO_ARGS="--release" ;;
  --debug)   PROFILE="debug"   CARGO_ARGS="" ;;
  *) log "unknown mode '$MODE' (use --release or --debug)"; exit 2 ;;
esac

PASS=0
SKIP=0
FAIL=0
FAILED_REPOS=""

for repo in $REPOS; do
  if ! has_cargo "$repo"; then
    log "SKIP $repo: no Cargo.toml (workspace not created yet)"
    SKIP=$((SKIP + 1))
    continue
  fi
  log "BUILD $repo ($PROFILE)"
  if ( cd "$PARENT/$repo" && cargo build $CARGO_ARGS ) > "$WORK/build-$repo.log" 2>&1; then
    PASS=$((PASS + 1))
    log "PASS $repo"
  else
    FAIL=$((FAIL + 1))
    FAILED_REPOS="$FAILED_REPOS $repo"
    log "FAIL $repo (see $WORK/build-$repo.log)"
  fi
done

log "SUMMARY build: pass=$PASS skip=$SKIP fail=$FAIL"
if [ "$FAIL" -gt 0 ]; then
  log "FAILED:$FAILED_REPOS"
  log "tail of failing build logs:"
  for f in "$WORK"/build-*.log; do
    [ -f "$f" ] || continue
    if grep -q "^error" "$f" || [ -s "$f" ]; then
      tail -5 "$f" || true
    fi
  done
  exit 1
fi
log "RESULT: PASS"
