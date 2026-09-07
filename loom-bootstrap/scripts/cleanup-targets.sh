#!/usr/bin/env bash
# Remove only explicitly allowlisted Loom build and bootstrap-temporary paths.
# Source, logs, screenshots, reports, and arbitrary CARGO_TARGET_DIR paths are
# deliberately outside this script's cleanup scope.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: cleanup-targets.sh [--dry-run] [--visual-diffs]

Remove known generated Cargo targets and bootstrap-owned temporary outputs.

Options:
  --dry-run       List eligible paths without removing anything.
  --visual-diffs  Also remove the exact loom-bootstrap/.work/diffs directory.
  -h, --help      Show this help and exit.

Default cleanup is limited to:
  - target/ under the Cargo repositories listed by scripts/lib.sh;
  - loom-plugin-sdk/crates/loom-plugin-cli/target/ (fixture output);
  - .work/verify-extract/, .work/verify-target/, and .work/package-filelist.txt.

The script never removes source files, logs, screenshots, reports, historical
run directories, or a CARGO_TARGET_DIR supplied by the environment. Visual
diffs are preserved unless --visual-diffs is explicitly supplied.
EOF
}

DRY_RUN=0
CLEAN_VISUAL_DIFFS=0

for arg in "$@"; do
  case "$arg" in
    --dry-run)
      DRY_RUN=1
      ;;
    --visual-diffs)
      CLEAN_VISUAL_DIFFS=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'cleanup-targets.sh: unknown option: %s\n\n' "$arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

# Resolve the repository from this script, not from the caller's cwd or an
# environment-provided root. This keeps every deletion below known roots.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
PARENT="$(cd "$ROOT/.." && pwd -P)"
WORK="$ROOT/.work"

if [ -z "$ROOT" ] || [ "$ROOT" = "/" ] || [ "$PARENT" = "/" ]; then
  printf 'cleanup-targets.sh: refusing unsafe repository root\n' >&2
  exit 1
fi
if [ -L "$WORK" ]; then
  printf 'cleanup-targets.sh: refusing symlinked work directory: %s\n' "$WORK" >&2
  exit 1
fi

# lib.sh is the source of the suite's repository list and logging convention.
# Pin LOOM_WORK before sourcing it so an arbitrary caller override cannot move
# this script's cleanup scope outside this bootstrap repository. Skip lib.sh's
# normal work-directory initialization so even a dry-run stays side-effect-free.
LOOM_WORK="$WORK"
LOOM_SKIP_WORK_INIT=1
source "$ROOT/scripts/lib.sh"

if [ "$WORK" != "$ROOT/.work" ]; then
  log "FAIL lib.sh changed WORK outside the bootstrap repository: $WORK"
  exit 1
fi

failures=0
planned=0
removed=0

is_allowed_path() {
  case "$1" in
    "$PARENT/loom-core/target"|\
    "$PARENT/loom-writer/target"|\
    "$PARENT/loom-sheets/target"|\
    "$PARENT/loom-present/target"|\
    "$PARENT/loom-photo/target"|\
    "$PARENT/loom-motion/target"|\
    "$PARENT/loom-video/target"|\
    "$PARENT/loom-studio/target"|\
    "$PARENT/loom-encode/target"|\
    "$PARENT/loom-vision/target"|\
    "$PARENT/loom-plugin-sdk/target"|\
    "$PARENT/loom-plugin-sdk/crates/loom-plugin-cli/target"|\
    "$WORK/verify-extract"|\
    "$WORK/verify-target"|\
    "$WORK/package-filelist.txt"|\
    "$WORK/diffs")
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

remove_path() {
  local label="$1"
  local path="$2"
  local kind="$3"
  local attempt

  if ! is_allowed_path "$path"; then
    log "FAIL refusing path outside allowlist: $path"
    failures=$((failures + 1))
    return 0
  fi
  if [ -z "$path" ] || [ "$path" = "/" ] || [ "$path" = "$ROOT" ] || \
     [ "$path" = "$PARENT" ] || [ "$path" = "$WORK" ]; then
    log "FAIL refusing broad cleanup path: $path"
    failures=$((failures + 1))
    return 0
  fi
  if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    log "SKIP $label (absent): $path"
    return 0
  fi
  if [ -L "$path" ]; then
    log "FAIL refusing symlink: $path"
    failures=$((failures + 1))
    return 0
  fi
  case "$kind" in
    dir)
      if [ ! -d "$path" ]; then
        log "FAIL expected directory, found another file type: $path"
        failures=$((failures + 1))
        return 0
      fi
      ;;
    file)
      if [ ! -f "$path" ]; then
        log "FAIL expected regular file, found another file type: $path"
        failures=$((failures + 1))
        return 0
      fi
      ;;
    *)
      log "FAIL internal cleanup type error for $path"
      failures=$((failures + 1))
      return 0
      ;;
  esac

  if [ "$DRY_RUN" -eq 1 ]; then
    planned=$((planned + 1))
    log "DRY-RUN would remove $label: $path"
    return 0
  fi

  if [ "$kind" = "dir" ]; then
    # Finder can recreate .DS_Store while macOS is viewing a target tree. A
    # bounded retry removes only that metadata inside this already-allowlisted
    # path, then verifies that the requested target really disappeared.
    for attempt in 1 2 3; do
      rm -rf -- "$path" 2>/dev/null || true
      if [ ! -e "$path" ] && [ ! -L "$path" ]; then
        break
      fi
      find "$path" -depth -type f -name '.DS_Store' -delete 2>/dev/null || true
      find "$path" -depth -type d -empty -delete 2>/dev/null || true
      [ "$attempt" -lt 3 ] && sleep 0.1
    done
  else
    rm -f -- "$path" 2>/dev/null || true
  fi

  if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    removed=$((removed + 1))
    log "REMOVED $label: $path"
  else
    log "FAIL removing $label: $path"
    failures=$((failures + 1))
  fi
}

cleanup_repo_target() {
  local repo="$1"
  local repo_dir="$PARENT/$repo"

  case "$repo" in
    loom-core|loom-writer|loom-sheets|loom-present|loom-photo|loom-motion|\
    loom-video|loom-studio|loom-encode|loom-vision|loom-plugin-sdk)
      ;;
    *)
      log "FAIL refusing unknown repository from REPOS: $repo"
      failures=$((failures + 1))
      return 0
      ;;
  esac

  if [ ! -d "$repo_dir" ]; then
    log "SKIP $repo (sibling repository absent): $repo_dir"
    return 0
  fi
  if [ -L "$repo_dir" ]; then
    log "SKIP $repo (sibling repository is a symlink): $repo_dir"
    return 0
  fi
  if [ ! -f "$repo_dir/Cargo.toml" ]; then
    log "SKIP $repo (no Cargo.toml): $repo_dir"
    return 0
  fi
  remove_path "Cargo target ($repo)" "$repo_dir/target" dir
}

if [ -n "${CARGO_TARGET_DIR:-}" ]; then
  log "NOTE CARGO_TARGET_DIR is set; ignoring it because cleanup is allowlist-only"
fi

for repo in $REPOS; do
  cleanup_repo_target "$repo"
done

# This nested target is documented fixture output, not an arbitrary recursive
# search for target directories. Check every relevant container before removal.
PLUGIN_ROOT="$PARENT/loom-plugin-sdk"
PLUGIN_CRATES="$PLUGIN_ROOT/crates"
PLUGIN_CLI="$PLUGIN_CRATES/loom-plugin-cli"
if [ -d "$PLUGIN_ROOT" ] && [ -f "$PLUGIN_CLI/Cargo.toml" ]; then
  if [ -L "$PLUGIN_ROOT" ] || [ -L "$PLUGIN_CRATES" ] || [ -L "$PLUGIN_CLI" ]; then
    log "SKIP plugin fixture target (a container is a symlink): $PLUGIN_CLI"
  else
    remove_path "plugin fixture target" "$PLUGIN_CLI/target" dir
  fi
else
  log "SKIP plugin fixture target (fixture crate absent): $PLUGIN_CLI"
fi

remove_path "package verification extract" "$WORK/verify-extract" dir
remove_path "package verification target" "$WORK/verify-target" dir
remove_path "package file list" "$WORK/package-filelist.txt" file

if [ "$CLEAN_VISUAL_DIFFS" -eq 1 ]; then
  remove_path "visual diffs" "$WORK/diffs" dir
else
  log "PRESERVE visual diffs (pass --visual-diffs to remove only $WORK/diffs)"
fi

if [ "$failures" -ne 0 ]; then
  log "RESULT: FAIL ($failures refusal/error(s))"
  exit 1
fi
if [ "$DRY_RUN" -eq 1 ]; then
  log "RESULT: PASS (dry-run; $planned eligible path(s), nothing removed)"
else
  log "RESULT: PASS ($removed path(s) removed)"
fi
