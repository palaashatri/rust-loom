#!/usr/bin/env bash
# Launch each existing app binary with --smoke for 5 seconds to verify it opens a window.
# Reports which apps succeeded; apps with no built binary are reported as missing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

SMOKE_SECS="${SMOKE_SECS:-5}"
PASS=0
SKIP=0
FAIL=0
FAILED_APPS=""
SUMMARY_LOG="$WORK/smoke-summary.log"
: > "$SUMMARY_LOG"

log "smoke test: each app runs 'loom-<app> --smoke' for ${SMOKE_SECS}s"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  if [ -z "$bin" ]; then
    log "SKIP $app: no built binary found (run scripts/build-all.sh; binary may not be implemented yet)"
    echo "$app|missing-binary" >> "$SUMMARY_LOG"
    SKIP=$((SKIP + 1))
    continue
  fi
  log "SMOKE $app: $bin --smoke"
  set +e
  run_with_timeout "$SMOKE_SECS" "$bin" --smoke > "$WORK/smoke-$app.log" 2>&1
  rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then
    PASS=$((PASS + 1))
    echo "$app|pass|exit=0" >> "$SUMMARY_LOG"
    log "PASS $app (exit=0: clean smoke exit)"
  elif [ "$rc" -eq 124 ]; then
    FAIL=$((FAIL + 1))
    FAILED_APPS="$FAILED_APPS $app"
    echo "$app|timeout|exit=124" >> "$SUMMARY_LOG"
    log "FAIL $app (timed out after ${SMOKE_SECS}s; --smoke must exit cleanly, see $WORK/smoke-$app.log)"
  else
    FAIL=$((FAIL + 1))
    FAILED_APPS="$FAILED_APPS $app"
    echo "$app|fail|exit=$rc" >> "$SUMMARY_LOG"
    log "FAIL $app (exit=$rc, see $WORK/smoke-$app.log)"
  fi
done

log "SUMMARY smoke: pass=$PASS skip=$SKIP fail=$FAIL"
if [ "$FAIL" -gt 0 ] || [ "$SKIP" -gt 0 ]; then
  log "FAILED:$FAILED_APPS"
  [ "$SKIP" -gt 0 ] && log "INCOMPLETE: $SKIP app(s) had no executable binary"
  exit 1
fi
log "RESULT: PASS"
