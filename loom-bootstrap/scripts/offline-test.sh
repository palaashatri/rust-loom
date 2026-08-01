#!/usr/bin/env bash
# Offline test: run the suite's core workflows with network disabled.
#
# Mode A (default, no docker): unset proxy variables, then run the test suite
# with cargo --offline. Note: if a repo has no Cargo.lock or the registry cache
# is cold, --offline will fail to resolve — those repos are reported, not hidden.
#
# Mode B (docker): runs mode A inside a container started with --network none.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

MODE="${1:---mode-a}"

if [ "$MODE" = "--mode-b" ]; then
  if ! command -v docker >/dev/null 2>&1; then
    log "WARN docker not available; falling back to mode A"
    exec bash "$ROOT/scripts/offline-test.sh" --mode-a
  fi
  IMAGE="loom-bootstrap-ci"
  if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    log "image $IMAGE not found; building it (first run only, may take a while)"
    docker compose -f "$ROOT/docker/compose.yaml" build ci
  fi
  log "docker run --network none (mode B)"
  exec docker run --rm --network none -v "$PARENT:/workspace" \
    -w /workspace/loom-bootstrap -e CARGO_TERM_COLOR=never \
    "$IMAGE" bash scripts/offline-test.sh --mode-a
fi

if [ "$MODE" != "--mode-a" ]; then
  log "unknown mode '$MODE' (use --mode-a or --mode-b)"
  exit 2
fi

log "mode A: disabling proxy variables (network must not be needed)"
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY 2>/dev/null || true
if env | grep -qi proxy; then
  log "WARN proxy variables still present after unset:"
  env | grep -i proxy || true
else
  log "OK no proxy variables set"
fi

LOCKED=0
log "repos with a Cargo.lock (required for cargo --offline):"
for repo in $REPOS; do
  if has_cargo "$repo" && [ -f "$PARENT/$repo/Cargo.lock" ]; then
    log "  LOCKED  $repo"
    LOCKED=$((LOCKED + 1))
  elif has_cargo "$repo"; then
    log "  UNLOCKED $repo (no Cargo.lock yet)"
  fi
done
if [ "$LOCKED" -eq 0 ]; then
  log "WARNING: no repo has a Cargo.lock — a cold registry will make --offline fail."
fi

log "running test suite with --offline (network-dependent resolution will fail loudly)"
bash "$ROOT/scripts/test-all.sh" --offline
