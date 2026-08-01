#!/usr/bin/env bash
# Run cargo fmt --check in every existing cargo workspace in the suite.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

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
  if ( cd "$PARENT/$repo" && cargo fmt --check ) > "$WORK/fmt-$repo.log" 2>&1; then
    PASS=$((PASS + 1))
    log "PASS fmt $repo"
  else
    FAIL=$((FAIL + 1))
    FAILED_REPOS="$FAILED_REPOS $repo"
    log "FAIL fmt $repo (run: cd $PARENT/$repo && cargo fmt)"
  fi
done

log "SUMMARY fmt: pass=$PASS skip=$SKIP fail=$FAIL"
if [ "$FAIL" -gt 0 ]; then
  log "UNFORMATTED:$FAILED_REPOS"
  exit 1
fi
log "RESULT: PASS"
