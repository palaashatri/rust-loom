#!/usr/bin/env bash
# Shared helpers for loom-bootstrap scripts. Source from scripts/, never execute directly.
set -euo pipefail

if [ -z "${ROOT:-}" ]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi
PARENT="$(cd "$ROOT/.." && pwd)"
WORK="$ROOT/.work"
mkdir -p "$WORK/screenshots" "$WORK/diffs"

# Cargo workspace repos that are part of the suite. Doc/spec/design/samples repos are
# intentionally excluded (they are not cargo workspaces).
REPOS="loom-core loom-writer loom-sheets loom-present loom-photo loom-motion loom-video loom-studio loom-encode loom-vision loom-plugin-sdk"
# Application repos expected to produce a binary named loom-<app>.
APPS="writer sheets present photo motion video studio encode"

NAME="$(basename "$0")"
log() { printf '[%s] %s\n' "$NAME" "$*"; }

has_cargo() { [ -f "$PARENT/$1/Cargo.toml" ]; }

# find_app_bin <app> -> prints path to a built loom-<app> binary, or empty string.
find_app_bin() {
  local app="$1" repo="$PARENT/loom-$app" bin=""
  if [ -x "$repo/target/release/loom-$app" ]; then
    bin="$repo/target/release/loom-$app"
  elif [ -x "$repo/target/debug/loom-$app" ]; then
    bin="$repo/target/debug/loom-$app"
  fi
  printf '%s' "$bin"
}

# run_with_timeout <secs> <cmd...> — uses coreutils timeout when available,
# otherwise backgrounds the command and kills it after <secs>.
# Exit status 124 means "killed by timeout".
run_with_timeout() {
  local secs="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout "$secs" "$@"
  else
    "$@" &
    local pid=$!
    (
      sleep "$secs"
      kill -9 "$pid" 2>/dev/null || true
    ) &
    local killer=$!
    local rc=0
    wait "$pid" || rc=$?
    kill "$killer" 2>/dev/null || true
    return "$rc"
  fi
}

# sha256_file <path> — writes <path>.sha256 using shasum or sha256sum.
sha256_file() {
  local f="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$f" > "$f.sha256"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$f" > "$f.sha256"
  else
    log "warning: no shasum/sha256sum tool available"
    return 1
  fi
}
