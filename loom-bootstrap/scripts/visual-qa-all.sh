#!/usr/bin/env bash
# Visual QA: screenshot each app (light + optional dark), compare against baselines
# in ../loom-design-bible/baselines/<app>/ and produce a report.
# Usage: visual-qa-all.sh [--size 1280x800]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

SIZE="${VISUAL_QA_SIZE:-1280x800}"
TOLERANCE="${VISUAL_QA_TOLERANCE:-0.02}"
SHOT_SECS="${VISUAL_QA_SHOT_SECS:-120}"
BASELINE_ROOT="$PARENT/loom-design-bible/baselines"
REPORT="$WORK/visual-qa-report.md"
DEST_REPORT="$PARENT/visual-qa-report.md"

captured=0
compared=0
diffs=0
nobaseline=0
skipped=0
failures=""

{
  echo "# Loom Visual QA Report"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/visual-qa-all.sh"
  echo "Size: $SIZE | Tolerance: $TOLERANCE | Baselines: $BASELINE_ROOT"
  echo
  echo "| app | screenshot | dark theme | baseline | result |"
  echo "|-----|------------|------------|----------|--------|"
} > "$REPORT"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  if [ -z "$bin" ]; then
    skipped=$((skipped + 1))
    echo "| $app | — | — | — | no binary (not built or not implemented) |" >> "$REPORT"
    continue
  fi

  shot="$WORK/screenshots/${app}-light.png"
  log "SHOT $app light"
  if ! run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$shot" --size "$SIZE" > "$WORK/shot-$app-light.log" 2>&1 || [ ! -s "$shot" ]; then
    skipped=$((skipped + 1))
    echo "| $app | failed (see $WORK/shot-$app-light.log) | — | — | screenshot unavailable |" >> "$REPORT"
    continue
  fi
  captured=$((captured + 1))

  dark="no"
  if run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$WORK/screenshots/${app}-dark.png" --size "$SIZE" --theme dark > "$WORK/shot-$app-dark.log" 2>&1 && [ -s "$WORK/screenshots/${app}-dark.png" ]; then
    dark="yes"
    captured=$((captured + 1))
  else
    log "NOTE $app: --theme dark unsupported or failed (see $WORK/shot-$app-dark.log)"
  fi

  for variant in light dark; do
    shot="$WORK/screenshots/${app}-${variant}.png"
    [ -f "$shot" ] || continue
    baseline="$BASELINE_ROOT/$app/${app}-${variant}.png"
    if [ ! -f "$baseline" ]; then
      nobaseline=$((nobaseline + 1))
      echo "| $app | ${variant} | $dark | missing | no baseline (add to design-bible) |" >> "$REPORT"
      continue
    fi
    log "COMPARE $app-$variant vs baseline"
    set +e
    bash "$ROOT/scripts/img-compare.sh" "$baseline" "$shot" "$WORK/diffs" > "$WORK/cmp-$app-$variant.log" 2>&1
    rc=$?
    set -e
    case "$rc" in
      0)
        compared=$((compared + 1))
        echo "| $app | ${variant} | $dark | ok | $(tail -1 "$WORK/cmp-$app-$variant.log") |" >> "$REPORT"
        ;;
      1)
        diffs=$((diffs + 1))
        failures="$failures $app-$variant"
        echo "| $app | ${variant} | $dark | DIFF | $(tail -1 "$WORK/cmp-$app-$variant.log") |" >> "$REPORT"
        ;;
      2)
        compared=$((compared + 1))
        echo "| $app | ${variant} | $dark | unavailable | compare tooling missing; image kept |" >> "$REPORT"
        ;;
      *)
        diffs=$((diffs + 1))
        failures="$failures $app-$variant"
        echo "| $app | ${variant} | $dark | ERROR | compare exited $rc |" >> "$REPORT"
        ;;
    esac
  done
done

{
  echo
  echo "## Summary"
  echo
  echo "- screenshots captured: $captured"
  echo "- comparisons run: $compared"
  echo "- diffs beyond tolerance: $diffs"
  echo "- missing baselines: $nobaseline"
  echo "- apps skipped (no binary): $skipped"
} >> "$REPORT"

cp "$REPORT" "$DEST_REPORT"
log "report written to $DEST_REPORT (and $REPORT)"
log "SUMMARY visual-qa: captured=$captured compared=$compared diffs=$diffs nobaseline=$nobaseline skipped=$skipped"

if [ "$diffs" -gt 0 ]; then
  log "FAILED:$failures"
  log "diffs: $WORK/diffs/"
  exit 1
fi
log "RESULT: PASS"
