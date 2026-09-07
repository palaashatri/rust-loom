#!/usr/bin/env bash
# Verify the packaged Loom-Complete.zip: extract into a clean directory, run the
# environment check, parse every Cargo workspace, and test every Cargo
# workspace from the extracted tree. Build targets stay in .work and are
# removed on exit so package verification does not consume the workspace disk.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
WORK="$ROOT/.work"
if [ -L "$WORK" ]; then
  printf 'verify-package.sh: refusing symlinked work directory: %s\n' "$WORK" >&2
  exit 1
fi
LOOM_WORK="$WORK"
source "$ROOT/scripts/lib.sh"

if [ "$WORK" != "$ROOT/.work" ]; then
  log "FAIL lib.sh changed WORK outside the bootstrap repository: $WORK"
  exit 1
fi

ZIP="$PARENT/Loom-Complete.zip"
CHECKSUM="$ZIP.sha256"
EXTRACT="$WORK/verify-extract"
VERIFY_TARGET="$WORK/verify-target"
FAIL=0
PASS=0

SUITE_DIRS=(
  loom-bootstrap
  loom-core
  loom-writer
  loom-sheets
  loom-present
  loom-photo
  loom-motion
  loom-video
  loom-studio
  loom-encode
  loom-vision
  loom-plugin-sdk
  loom-design-bible
  loom-spec
  loom-samples
)

cleanup_path() {
  local path="$1"
  local attempt

  case "$path" in
    "$WORK/verify-extract"|"$WORK/verify-target")
      ;;
    *)
      log "FAIL refusing verification cleanup path outside allowlist: $path"
      return 1
      ;;
  esac
  if [ -L "$path" ]; then
    log "FAIL refusing symlinked verification cleanup path: $path"
    return 1
  fi
  if [ ! -e "$path" ]; then
    return 0
  fi
  if [ ! -d "$path" ]; then
    log "FAIL refusing non-directory verification cleanup path: $path"
    return 1
  fi
  # Finder can recreate .DS_Store while a verification tree is visible. Keep
  # retries bounded and remove only metadata inside this exact allowlisted
  # path before checking that the tree is gone.
  for attempt in 1 2 3; do
    rm -rf -- "$path" 2>/dev/null || true
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      return 0
    fi
    find "$path" -depth -type f -name '.DS_Store' -delete 2>/dev/null || true
    find "$path" -depth -type d -empty -delete 2>/dev/null || true
    [ "$attempt" -lt 3 ] && sleep 0.1
  done
  log "FAIL unable to remove verification path: $path"
  return 1
}

cleanup() {
  local status=0
  cleanup_path "$EXTRACT" || status=1
  cleanup_path "$VERIFY_TARGET" || status=1
  return "$status"
}

on_exit() {
  local status="$1"
  if ! cleanup; then
    log "WARN package-verification cleanup incomplete: $EXTRACT or $VERIFY_TARGET remains"
    status=1
  fi
  exit "$status"
}

trap 'on_exit "$?"' EXIT
if ! cleanup; then
  log "FAIL unable to clear prior package-verification temporary paths"
  exit 1
fi

[ -f "$ZIP" ] || { log "FAIL $ZIP not found (run scripts/package.sh first)"; exit 1; }
[ ! -L "$ZIP" ] || { log "FAIL refusing symlinked archive: $ZIP"; exit 1; }
[ -s "$CHECKSUM" ] || { log "FAIL $CHECKSUM not found or empty (run scripts/package.sh first)"; exit 1; }
[ ! -L "$CHECKSUM" ] || { log "FAIL refusing symlinked checksum sidecar: $CHECKSUM"; exit 1; }
command -v unzip >/dev/null 2>&1 || { log "FAIL unzip not available"; exit 1; }

if command -v shasum >/dev/null 2>&1; then
  if ! shasum -a 256 -c "$CHECKSUM"; then
    log "FAIL checksum verification for $ZIP"
    exit 1
  fi
elif command -v sha256sum >/dev/null 2>&1; then
  if ! sha256sum -c "$CHECKSUM"; then
    log "FAIL checksum verification for $ZIP"
    exit 1
  fi
else
  log "FAIL no shasum or sha256sum available for checksum verification"
  exit 1
fi
log "OK checksum $CHECKSUM"

ARCHIVE_ENTRIES=""
if ! ARCHIVE_ENTRIES="$(unzip -Z1 "$ZIP")"; then
  log "FAIL unable to list archive entries"
  exit 1
fi
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  case "$entry" in
    /*|../*|*/../*|*/..|./*|*/./*)
      log "FAIL unsafe archive entry: $entry"
      exit 1
      ;;
  esac
done <<< "$ARCHIVE_ENTRIES"

if ! unzip -tq "$ZIP" >/dev/null; then
  log "FAIL archive integrity check: $ZIP"
  exit 1
fi
log "OK archive integrity $ZIP"

mkdir -p "$EXTRACT"
log "extracting $ZIP into $EXTRACT"
unzip -q "$ZIP" -d "$EXTRACT"
BOOTSTRAP="$EXTRACT/loom-bootstrap"
[ -d "$BOOTSTRAP" ] || { log "FAIL extracted archive has no loom-bootstrap/"; exit 1; }

for repo in "${SUITE_DIRS[@]}"; do
  repo_dir="$EXTRACT/$repo"
  if [ ! -d "$repo_dir" ] || [ -L "$repo_dir" ]; then
    log "FAIL required suite directory missing or symlinked from archive: $repo"
    exit 1
  fi
done
for repo in $REPOS; do
  if [ ! -f "$EXTRACT/$repo/Cargo.toml" ]; then
    log "FAIL $repo: expected Cargo.toml missing from archive"
    exit 1
  fi
done
if [ -n "$(find -P "$EXTRACT" -type l -print -quit)" ]; then
  log "FAIL extracted archive contains a symbolic link"
  exit 1
fi

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
