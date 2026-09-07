#!/usr/bin/env bash
# Run the full test suite inside the Docker 'ci' service (fmt, clippy, test).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

command -v docker >/dev/null 2>&1 || { log "FAIL docker not available"; exit 1; }

log "docker compose run --rm ci"
docker compose -f "$ROOT/docker/compose.yaml" run --rm ci \
  bash -lc "scripts/fmt-all.sh && scripts/clippy-all.sh && scripts/test-all.sh"
