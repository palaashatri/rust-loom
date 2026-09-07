#!/usr/bin/env bash
# Verify the host toolchain against the suite compatibility manifest.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

MSRV_MAJOR=1
MSRV_MINOR=80
FAIL=0

log "toolchain check (MSRV $MSRV_MAJOR.$MSRV_MINOR)"

check_version() {
  local tool="$1" got="$2" major minor
  major="$(printf '%s' "$got" | cut -d. -f1)"
  minor="$(printf '%s' "$got" | cut -d. -f2)"
  if [ "$major" -gt "$MSRV_MAJOR" ] || { [ "$major" -eq "$MSRV_MAJOR" ] && [ "$minor" -ge "$MSRV_MINOR" ]; }; then
    log "OK   $tool $got (>= $MSRV_MAJOR.$MSRV_MINOR)"
  else
    log "FAIL $tool $got (< $MSRV_MAJOR.$MSRV_MINOR, update via rustup)"
    FAIL=1
  fi
}

if command -v rustc >/dev/null 2>&1; then
  check_version rustc "$(rustc --version | awk '{print $2}')"
else
  log "FAIL rustc not found (install via https://rustup.rs)"
  FAIL=1
fi

if command -v cargo >/dev/null 2>&1; then
  check_version cargo "$(cargo --version | awk '{print $2}')"
else
  log "FAIL cargo not found (install via https://rustup.rs)"
  FAIL=1
fi

log "optional tool availability:"
for tool in docker zip unzip just timeout; do
  if command -v "$tool" >/dev/null 2>&1; then
    log "  OK   $tool: $(command -v "$tool")"
  else
    log "  WARN $tool: not found"
    if [ "$tool" = "docker" ]; then
      log "  WARN docker is only required for the Docker visual-QA / offline-test paths"
    elif [ "$tool" = "just" ]; then
      log "  WARN just is optional; scripts/ can be invoked directly"
    fi
  fi
done

if command -v python3 >/dev/null 2>&1 && python3 -c 'import PIL' >/dev/null 2>&1; then
  log "  OK   python3 + PIL: $(python3 -c 'import PIL; print(PIL.__version__)')"
else
  log "  WARN python3+PIL not available (image comparison fallback disabled)"
fi

if [ "$FAIL" -eq 1 ]; then
  log "RESULT: FAIL — toolchain below MSRV or missing; fix before building."
  exit 1
fi
log "RESULT: PASS"
