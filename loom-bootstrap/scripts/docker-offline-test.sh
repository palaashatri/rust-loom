#!/usr/bin/env bash
# Run the offline test inside the Docker 'offline' service (network_mode: none).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

command -v docker >/dev/null 2>&1 || { log "FAIL docker not available"; exit 1; }

log "docker compose run --rm offline (container has no network)"
docker compose -f "$ROOT/docker/compose.yaml" run --rm offline \
  bash -lc "scripts/offline-test.sh --mode-a"
