#!/usr/bin/env bash
# Run visual QA inside the Docker 'visual' service (headless X via xvfb).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

command -v docker >/dev/null 2>&1 || { log "FAIL docker not available"; exit 1; }

log "docker compose run --rm visual"
docker compose -f "$ROOT/docker/compose.yaml" run --rm visual \
  bash -lc "xvfb-run -a -s '-screen 0 1280x800x24' bash scripts/visual-qa-all.sh"
