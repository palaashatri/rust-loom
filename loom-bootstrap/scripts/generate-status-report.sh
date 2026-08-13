#!/usr/bin/env bash
# Generate a verification report from current command evidence.
# Reports are build artifacts under .work, never committed root-level truth.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

REPORT="$WORK/VERIFICATION_REPORT.md"

repo_app() {
  case "$1" in
    loom-writer) printf '%s' writer ;;
    loom-sheets) printf '%s' sheets ;;
    loom-present) printf '%s' present ;;
    loom-photo) printf '%s' photo ;;
    loom-motion) printf '%s' motion ;;
    loom-video) printf '%s' video ;;
    loom-studio) printf '%s' studio ;;
    loom-encode) printf '%s' encode ;;
    *) printf '%s' '' ;;
  esac
}

metadata_status() {
  local repo="$1"
  if ! has_cargo "$repo"; then
    printf '%s' '—'
  elif ( cd "$PARENT/$repo" && cargo metadata --no-deps --offline >/dev/null 2>&1 ) \
       || ( cd "$PARENT/$repo" && cargo metadata --no-deps >/dev/null 2>&1 ); then
    printf '%s' 'PASS'
  else
    printf '%s' 'FAIL'
  fi
}

build_status() {
  local repo="$1" log_file="$WORK/build-$1.log"
  if [ ! -f "$log_file" ]; then
    printf '%s' 'NOT_RUN'
  elif grep -qE 'Finished .*profile' "$log_file"; then
    printf '%s' 'PASS'
  else
    printf '%s' 'FAIL'
  fi
}

test_status() {
  local repo="$1" log_file="$WORK/test-$1.log"
  if [ ! -f "$log_file" ]; then
    printf '%s' 'NOT_RUN'
  elif grep -qE '^test result: FAILED' "$log_file"; then
    printf '%s' 'FAIL'
  elif grep -qE '^test result: ok' "$log_file"; then
    printf '%s' 'PASS'
  else
    printf '%s' 'FAIL'
  fi
}

test_count() {
  local log_file="$WORK/test-$1.log"
  if [ ! -f "$log_file" ]; then
    printf '%s' '—'
  else
    awk '/^test result: ok/ { total += $4 } END { print total + 0 }' "$log_file"
  fi
}

binary_status() {
  local app="$1" bin
  if [ -z "$app" ]; then
    printf '%s' '—'
    return
  fi
  bin="$(find_app_bin "$app")"
  if [ -n "$bin" ]; then
    printf '%s' 'PASS'
  else
    printf '%s' 'MISSING'
  fi
}

mkdir -p "$WORK"
{
  echo "# Loom Verification Report"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/generate-status-report.sh"
  echo
  echo "This report is a disposable CI artifact. It is not the project truth statement or a product-completion score."
  echo
  echo "## Cargo workspaces"
  echo
  echo "| repo | cargo | metadata | build log | test log | test cases | app binary |"
  echo "|------|-------|----------|-----------|----------|------------|-------------|"
} > "$REPORT"

for repo in $REPOS; do
  cargo='no'
  if has_cargo "$repo"; then
    cargo='yes'
  fi
  app="$(repo_app "$repo")"
  echo "| $repo | $cargo | $(metadata_status "$repo") | $(build_status "$repo") | $(test_status "$repo") | $(test_count "$repo") | $(binary_status "$app") |" >> "$REPORT"
done

{
  echo
  echo "## Application smoke evidence"
  echo
  echo "| app | binary | smoke log |"
  echo "|-----|---------|------------|"
} >> "$REPORT"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  binary='MISSING'
  [ -n "$bin" ] && binary='PASS'
  smoke='NOT_RUN'
  if [ -f "$WORK/smoke-summary.log" ]; then
    if grep -q "^$app|pass|" "$WORK/smoke-summary.log"; then
      smoke='PASS'
    elif grep -q "^$app|" "$WORK/smoke-summary.log"; then
      smoke='FAIL'
    fi
  fi
  echo "| $app | $binary | $smoke |" >> "$REPORT"
done

visual='NOT_RUN'
ui_matrix="$ROOT/.work/evidence/ui/native-ui-matrix.json"
if [ -f "$ui_matrix" ] && grep -q '"passed": true' "$ui_matrix"; then
  visual='PASS'
elif [ -f "$ui_matrix" ]; then
  visual='INCOMPLETE/FAIL'
fi

{
  echo
  echo "## Visual QA evidence"
  echo
  echo "- report: $visual"
  echo "- source: native-ui-matrix.json"
  echo "- palette screenshots prove overlay rendering only; keyboard journeys are separate artifacts"
  echo
  echo "## Evidence sources"
  echo
  echo "- build logs: loom-bootstrap/.work/build-<repo>.log"
  echo "- test logs: loom-bootstrap/.work/test-<repo>.log"
  echo "- smoke summary: loom-bootstrap/.work/smoke-summary.log"
  echo "- native UI matrix: loom-bootstrap/.work/evidence/ui/native-ui-matrix.json"
  echo "- keyboard journeys: native evidence journeys/<app>/<app>.json"
} >> "$REPORT"

log "report written to $REPORT"
