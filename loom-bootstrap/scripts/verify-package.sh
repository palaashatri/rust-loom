#!/usr/bin/env bash
# Verify the packaged Loom-Complete.zip: extract into a clean directory, run the
# environment check, a quick cargo metadata check, and a lightweight test pass
# (loom-core cargo test) from the extracted tree.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

ZIP="$PARENT/Loom-Complete.zip"
EXTRACT="$WORK/verify-extract"
FAIL=0

[ -f "$ZIP" ] || { log "FAIL $ZIP not found (run scripts/package.sh first)"; exit 1; }
command -v unzip >/dev/null 2>&1 || { log "FAIL unzip not available"; exit 1; }

rm -rf "$EXTRACT"
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

step "cargo metadata quick check (loom-core)"
if ( cd "$EXTRACT/loom-core" && cargo metadata --no-deps >/dev/null 2>&1 ) \
   || ( cd "$EXTRACT/loom-core" && cargo metadata --no-deps --offline >/dev/null 2>&1 ); then
  log "OK loom-core metadata parses"
else
  log "FAIL loom-core metadata does not parse"
  FAIL=1
fi

step "test-all-lite: cargo test --workspace in loom-core (may take a while)"
if ( cd "$EXTRACT/loom-core" && cargo test --workspace ) > "$WORK/verify-lite-test.log" 2>&1; then
  log "OK loom-core tests pass (see $WORK/verify-lite-test.log)"
else
  log "FAIL loom-core tests (see $WORK/verify-lite-test.log)"
  FAIL=1
fi

if [ "$FAIL" -eq 0 ]; then
  log "RESULT: PASS — package verified"
else
  log "RESULT: FAIL — see steps above"
  exit 1
fi
