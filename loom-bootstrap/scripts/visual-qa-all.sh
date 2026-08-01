#!/usr/bin/env bash
# Visual QA: screenshot each app (light + dark), compare against baselines
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
DEST_REPORT="${VISUAL_QA_DEST_REPORT:-$PARENT/visual-qa-report.md}"

captured=0
compared=0
diffs=0
nobaseline=0
missing_binaries=0
shot_failures=0
compare_failures=0
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
    missing_binaries=$((missing_binaries + 1))
    failures="$failures $app-no-binary"
    echo "| $app | — | — | NO BINARY | application binary is required |" >> "$REPORT"
    continue
  fi

  shot="$WORK/screenshots/${app}-light.png"
  log "SHOT $app light"
  if ! run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$shot" --size "$SIZE" --theme light > "$WORK/shot-$app-light.log" 2>&1 || [ ! -s "$shot" ]; then
    shot_failures=$((shot_failures + 1))
    failures="$failures $app-light-screenshot"
    echo "| $app | failed (see $WORK/shot-$app-light.log) | — | FAILED | screenshot unavailable |" >> "$REPORT"
    continue
  fi
  captured=$((captured + 1))

  dark="no"
  if run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$WORK/screenshots/${app}-dark.png" --size "$SIZE" --theme dark > "$WORK/shot-$app-dark.log" 2>&1 && [ -s "$WORK/screenshots/${app}-dark.png" ]; then
    dark="yes"
    captured=$((captured + 1))
  else
    shot_failures=$((shot_failures + 1))
    failures="$failures $app-dark-screenshot"
    log "FAIL $app: --theme dark unsupported or failed (see $WORK/shot-$app-dark.log)"
  fi

  for variant in light dark; do
    shot="$WORK/screenshots/${app}-${variant}.png"
    [ -f "$shot" ] || continue
    baseline="$BASELINE_ROOT/$app/${app}-${variant}.png"
    if [ ! -f "$baseline" ]; then
      nobaseline=$((nobaseline + 1))
      failures="$failures $app-$variant-missing-baseline"
      echo "| $app | ${variant} | $dark | MISSING | baseline required in design-bible |" >> "$REPORT"
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
        compare_failures=$((compare_failures + 1))
        failures="$failures $app-$variant-compare-tool"
        echo "| $app | ${variant} | $dark | ERROR | compare tooling missing; image kept |" >> "$REPORT"
        ;;
      *)
        compare_failures=$((compare_failures + 1))
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
  echo "- screenshot failures: $shot_failures"
  echo "- apps missing binaries: $missing_binaries"
  echo "- comparison-tool failures: $compare_failures"
  if [ "$diffs" -eq 0 ] && [ "$nobaseline" -eq 0 ] && [ "$shot_failures" -eq 0 ] && [ "$missing_binaries" -eq 0 ] && [ "$compare_failures" -eq 0 ]; then
    echo "- result: PASS"
  else
    echo "- result: INCOMPLETE/FAIL"
  fi
} >> "$REPORT"

if [ "$REPORT" != "$DEST_REPORT" ]; then
  cp "$REPORT" "$DEST_REPORT"
  log "report written to $DEST_REPORT (and $REPORT)"
else
  log "report written to $REPORT"
fi
log "SUMMARY visual-qa: captured=$captured compared=$compared diffs=$diffs nobaseline=$nobaseline screenshot_failures=$shot_failures missing_binaries=$missing_binaries compare_failures=$compare_failures"

if [ "$diffs" -gt 0 ] || [ "$nobaseline" -gt 0 ] || [ "$shot_failures" -gt 0 ] || [ "$missing_binaries" -gt 0 ] || [ "$compare_failures" -gt 0 ]; then
  log "FAILED_OR_INCOMPLETE:$failures"
  log "diffs: $WORK/diffs/"
  exit 1
fi
log "RESULT: PASS"
