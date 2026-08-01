#!/usr/bin/env bash
# Build the Docker images for the suite.
# Usage: docker-build.sh [service]  (default: build all services)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

command -v docker >/dev/null 2>&1 || { log "FAIL docker not available"; exit 1; }

if [ "$#" -eq 0 ]; then
  log "docker compose build (all services: ci, visual, offline)"
  docker compose -f "$ROOT/docker/compose.yaml" build
else
  log "docker compose build $*"
  docker compose -f "$ROOT/docker/compose.yaml" build "$@"
fi
