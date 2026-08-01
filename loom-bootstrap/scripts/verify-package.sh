#!/usr/bin/env bash
# Verify the packaged Loom-Complete.zip: extract into a clean directory, run the
# environment check, parse every Cargo workspace, and test every Cargo
# workspace from the extracted tree. Build targets stay in .work and are
# removed on exit so package verification does not consume the workspace disk.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

ZIP="$PARENT/Loom-Complete.zip"
EXTRACT="$WORK/verify-extract"
VERIFY_TARGET="$WORK/verify-target"
FAIL=0
PASS=0

[ -f "$ZIP" ] || { log "FAIL $ZIP not found (run scripts/package.sh first)"; exit 1; }
command -v unzip >/dev/null 2>&1 || { log "FAIL unzip not available"; exit 1; }

rm -rf "$EXTRACT" "$VERIFY_TARGET"
cleanup() {
  local attempt
  for attempt in 1 2 3; do
    rm -rf "$EXTRACT" "$VERIFY_TARGET" 2>/dev/null || true
    if [ ! -e "$EXTRACT" ] && [ ! -e "$VERIFY_TARGET" ]; then
      return 0
    fi
    sleep 1
  done
  log "WARN package-verification cleanup incomplete: $EXTRACT or $VERIFY_TARGET remains"
  return 1
}
trap 'status=$?; cleanup || status=1; exit "$status"' EXIT
mkdir -p "$EXTRACT"
log "extracting $ZIP into $EXTRACT"
unzip -q "$ZIP" -d "$EXTRACT"
TREE="$(ls -d "$EXTRACT"/loom-* 2>/dev/null | head -1)"
if [ -z "$TREE" ]; then
  log "FAIL no loom-* directories found inside the archive"
  exit 1
fi
BOOTSTRAP="$EXTRACT/loom-bootstrap"
[ -d "$BOOTSTRAP" ] || { log "FAIL extracted archive has no loom-bootstrap/"; exit 1; }

step() { log "verify step: $1"; }

step "env check (from extracted tree)"
if bash "$BOOTSTRAP/scripts/env-check.sh"; then
  log "OK env check"
else
  log "FAIL env check"
  FAIL=1
fi

for repo in $REPOS; do
  repo_dir="$EXTRACT/$repo"
  if [ ! -f "$repo_dir/Cargo.toml" ]; then
    log "FAIL $repo: expected Cargo.toml missing from archive"
    FAIL=$((FAIL + 1))
    continue
  fi

  step "cargo metadata ($repo)"
  if ( cd "$repo_dir" && cargo metadata --no-deps --offline >/dev/null 2>&1 ) \
     || ( cd "$repo_dir" && cargo metadata --no-deps >/dev/null 2>&1 ); then
    log "OK $repo metadata parses"
  else
    log "FAIL $repo metadata (see $WORK/verify-metadata-$repo.log)"
    ( cd "$repo_dir" && cargo metadata --no-deps --offline ) > "$WORK/verify-metadata-$repo.log" 2>&1 || true
    FAIL=$((FAIL + 1))
    continue
  fi

  step "cargo test --workspace ($repo)"
  if ( cd "$repo_dir" && CARGO_TARGET_DIR="$VERIFY_TARGET" cargo test --locked --offline --workspace ) > "$WORK/verify-test-$repo.log" 2>&1; then
    log "OK $repo tests (see $WORK/verify-test-$repo.log)"
    PASS=$((PASS + 1))
  else
    log "FAIL $repo tests (see $WORK/verify-test-$repo.log)"
    FAIL=$((FAIL + 1))
  fi
done

log "SUMMARY extracted workspaces: pass=$PASS fail=$FAIL"

if [ "$FAIL" -eq 0 ]; then
  log "RESULT: PASS — package verified"
else
  log "RESULT: FAIL — see steps above"
  exit 1
fi
