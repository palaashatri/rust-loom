#!/usr/bin/env bash
# Run cargo clippy -D warnings in every existing cargo workspace.
# Reports warning counts; fails only for issues in Loom's own crates, not in
# third-party dependency code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

PASS=0
SKIP=0
FAIL=0
TOTAL_LINES=0
OURS=0
FAILED_REPOS=""

for repo in $REPOS; do
  if ! has_cargo "$repo"; then
    log "SKIP $repo: no Cargo.toml (workspace not created yet)"
    SKIP=$((SKIP + 1))
    continue
  fi
  log "CLIPPY $repo"
  LOG="$WORK/clippy-$repo.log"
  if ( cd "$PARENT/$repo" && cargo clippy --all-targets --all-features --message-format short -- -D warnings ) > "$LOG" 2>&1; then
    W=$(grep -c -E 'warning|error' "$LOG" || true)
    TOTAL_LINES=$((TOTAL_LINES + W))
    PASS=$((PASS + 1))
    log "PASS $repo (diagnostic lines: $W)"
    continue
  fi
  if grep -q 'no such command' "$LOG"; then
    FAIL=$((FAIL + 1))
    FAILED_REPOS="$FAILED_REPOS $repo"
    log "FAIL $repo: clippy unavailable (rustup component clippy missing)"
    continue
  fi
  W=$(grep -c -E 'warning|error' "$LOG" || true)
  TOTAL_LINES=$((TOTAL_LINES + W))
  # Issues in Loom's own crates appear with repo-relative paths (src/, crates/, ./).
  O=$(grep -c -E '^(src/|crates/|\./|\.\./)' "$LOG" || true)
  OURS=$((OURS + O))
  if [ "$O" -gt 0 ]; then
    FAIL=$((FAIL + 1))
    FAILED_REPOS="$FAILED_REPOS $repo"
    log "FAIL $repo: $O clippy issue(s) in Loom crates (see $LOG)"
  else
    PASS=$((PASS + 1))
    log "PASS $repo: diagnostics only in third-party code ($W lines, see $LOG)"
  fi
done

log "SUMMARY clippy: pass=$PASS fail=$FAIL skip=$SKIP diagnostic_lines=$TOTAL_LINES loom_crate_issues=$OURS"
if [ "$FAIL" -gt 0 ] || [ "$SKIP" -gt 0 ]; then
  log "FAILED:$FAILED_REPOS"
  [ "$SKIP" -gt 0 ] && log "INCOMPLETE: $SKIP workspace(s) were not present"
  exit 1
fi
log "RESULT: PASS"
