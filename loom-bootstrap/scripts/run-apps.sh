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

log "smoke test: each app runs 'loom-<app> --smoke' for ${SMOKE_SECS}s"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  if [ -z "$bin" ]; then
    log "SKIP $app: no built binary found (run scripts/build-all.sh; binary may not be implemented yet)"
    SKIP=$((SKIP + 1))
    continue
  fi
  log "SMOKE $app: $bin --smoke"
  set +e
  run_with_timeout "$SMOKE_SECS" "$bin" --smoke > "$WORK/smoke-$app.log" 2>&1
  rc=$?
  set -e
  if [ "$rc" -eq 0 ] || [ "$rc" -eq 124 ]; then
    PASS=$((PASS + 1))
    log "PASS $app (exit=$rc: clean exit or alive for ${SMOKE_SECS}s)"
  else
    FAIL=$((FAIL + 1))
    FAILED_APPS="$FAILED_APPS $app"
    log "FAIL $app (exit=$rc, see $WORK/smoke-$app.log)"
  fi
done

log "SUMMARY smoke: pass=$PASS skip=$SKIP fail=$FAIL"
if [ "$FAIL" -gt 0 ]; then
  log "FAILED:$FAILED_APPS"
  exit 1
fi
log "RESULT: PASS"
