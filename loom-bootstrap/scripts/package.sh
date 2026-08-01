#!/usr/bin/env bash
# Package the whole loom suite (all loom-* repos in the parent directory) into
# ../Loom-Complete.zip with a deterministic file ordering, excluding build
# artifacts, VCS metadata and OS junk. Also writes ../Loom-Complete.zip.sha256.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

ZIP="$PARENT/Loom-Complete.zip"
LIST="$WORK/package-filelist.txt"

if ! command -v zip >/dev/null 2>&1; then
  log "FAIL zip not available (install via package manager)"
  exit 1
fi

cd "$PARENT"
if ! ls -d loom-* >/dev/null 2>&1; then
  log "FAIL no loom-* repos found in $PARENT"
  exit 1
fi

log "collecting files (excluding target/, .git/, .DS_Store, .work/, __pycache__)"
find loom-* \( -type f -o -type l \) 2>/dev/null \
  | grep -v -E '(^|/)\.git(/|$)' \
  | grep -v -E '(^|/)target(/|$)' \
  | grep -v -E '(^|/)\.work(/|$)' \
  | grep -v -E '\.DS_Store$' \
  | grep -v -E '__pycache__' \
  | sort > "$LIST"

COUNT="$(wc -l < "$LIST" | tr -d ' ')"
if [ "$COUNT" -eq 0 ]; then
  log "FAIL no files matched; nothing to package"
  exit 1
fi
log "packaging $COUNT files"

rm -f "$ZIP"
if zip -X -q -@ "$ZIP" < "$LIST"; then
  log "OK $ZIP ($(du -h "$ZIP" | awk '{print $1}'))"
else
  log "FAIL zip returned $?"
  exit 1
fi

if sha256_file "$ZIP"; then
  log "OK $(cat "$ZIP.sha256")"
fi
log "RESULT: PASS"
