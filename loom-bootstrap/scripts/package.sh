#!/usr/bin/env bash
# Package the whole loom suite (all loom-* repos in the parent directory) into
# ../Loom-Complete.zip with a deterministic file ordering, excluding build
# artifacts, VCS metadata and OS junk. Also writes ../Loom-Complete.zip.sha256.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
WORK="$ROOT/.work"
if [ -L "$WORK" ]; then
  printf 'package.sh: refusing symlinked work directory: %s\n' "$WORK" >&2
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
LIST="$WORK/package-filelist.txt"
TMP_ZIP="$ZIP.tmp.$$"

# These are the suite directories documented by BOOTSTRAP.md. Keep the
# explicit list so a partially checked-out suite cannot produce a plausible
# but incomplete archive.
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

cleanup_tmp() {
  rm -f -- "$TMP_ZIP" 2>/dev/null || true
}
trap cleanup_tmp EXIT

if ! command -v zip >/dev/null 2>&1; then
  log "FAIL zip not available (install via package manager)"
  exit 1
fi

cd "$PARENT"
for repo in "${SUITE_DIRS[@]}"; do
  if [ ! -d "$repo" ] || [ -L "$repo" ]; then
    log "FAIL required suite directory missing or symlinked: $repo"
    exit 1
  fi
done
for repo in $REPOS; do
  if [ ! -f "$repo/Cargo.toml" ]; then
    log "FAIL required Cargo workspace missing Cargo.toml: $repo"
    exit 1
  fi
done

if ! rm -f -- "$ZIP" "$CHECKSUM" "$TMP_ZIP"; then
  log "FAIL unable to remove previous package or checksum sidecar"
  exit 1
fi

# Package only regular files. -P makes the no-follow policy explicit; symbolic
# links are deliberately excluded so untrusted link targets are never read.
log "collecting regular files (excluding symlinks, target/, .git/, .DS_Store, .work/, __pycache__)"
find -P loom-* -type f 2>/dev/null \
  | grep -v -E '(^|/)\.git(/|$)' \
  | grep -v -E '(^|/)target(/|$)' \
  | grep -v -E '(^|/)\.work(/|$)' \
  | grep -v -E '\.DS_Store$' \
  | grep -v -E '__pycache__' \
  | LC_ALL=C sort > "$LIST"

COUNT="$(wc -l < "$LIST" | tr -d ' ')"
if [ "$COUNT" -eq 0 ]; then
  log "FAIL no files matched; nothing to package"
  exit 1
fi
log "packaging $COUNT files"

if zip -X -q -@ "$TMP_ZIP" < "$LIST" && [ -s "$TMP_ZIP" ]; then
  if ! mv -f -- "$TMP_ZIP" "$ZIP"; then
    log "FAIL unable to publish completed archive: $ZIP"
    rm -f -- "$ZIP" "$CHECKSUM" || true
    exit 1
  fi
  log "OK $ZIP ($(du -h "$ZIP" | awk '{print $1}'))"
else
  ZIP_STATUS=$?
  log "FAIL zip returned $ZIP_STATUS"
  rm -f -- "$ZIP" "$CHECKSUM" || true
  exit 1
fi

if ! sha256_file "$ZIP" || [ ! -s "$CHECKSUM" ]; then
  log "FAIL checksum generation for $ZIP"
  rm -f -- "$ZIP" "$CHECKSUM" || true
  exit 1
fi
log "OK $CHECKSUM"
log "RESULT: PASS"
