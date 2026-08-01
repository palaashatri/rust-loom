#!/usr/bin/env bash
# Run cargo test --workspace in every existing cargo workspace in the suite.
# Usage: test-all.sh [--offline]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

OFFLINE=0
if [ "${1:-}" = "--offline" ]; then
  OFFLINE=1
fi

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
  log "TEST $repo (offline=$OFFLINE)"
  args="test --workspace"
  [ "$OFFLINE" -eq 1 ] && args="test --offline --workspace"
  if ( cd "$PARENT/$repo" && cargo $args ) > "$WORK/test-$repo.log" 2>&1; then
    TESTS="$(awk '/^test result: ok\./ { passed += $4 } END { print passed + 0 }' "$WORK/test-$repo.log")"
    if [ "$TESTS" -gt 0 ]; then
      PASS=$((PASS + 1))
      log "PASS $repo ($TESTS tests passed)"
    else
      FAIL=$((FAIL + 1))
      FAILED_REPOS="$FAILED_REPOS $repo"
      log "FAIL $repo (cargo returned 0 but no tests passed; see $WORK/test-$repo.log)"
    fi
  else
    FAIL=$((FAIL + 1))
    FAILED_REPOS="$FAILED_REPOS $repo"
    log "FAIL $repo (see $WORK/test-$repo.log)"
  fi
done

log "SUMMARY test: pass=$PASS skip=$SKIP fail=$FAIL"
if [ "$FAIL" -gt 0 ] || [ "$SKIP" -gt 0 ]; then
  log "FAILED:$FAILED_REPOS"
  [ "$SKIP" -gt 0 ] && log "INCOMPLETE: $SKIP workspace(s) were not present"
  exit 1
fi
log "RESULT: PASS"
