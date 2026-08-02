#!/usr/bin/env bash
# Visual QA: screenshot each app's default light + dark themes, compare against
# baselines in ../loom-design-bible/baselines/<app>/, and produce a report.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: visual-qa-all.sh [--size WIDTHxHEIGHT]

Capture and compare each built app in its default light and dark themes. This
script does not execute the full design-bible matrix (high-contrast, text
scales, reduced motion, locales, component/state, or error-state captures).
Comparisons use the fixed RGBA gates: mean absolute error < 1.0 and
differing-pixel ratio < 0.01 after one-pixel erosion.

Options:
  --size WIDTHxHEIGHT  Override VISUAL_QA_SIZE for this run (default: 1280x800)
  -h, --help           Show this help text
EOF
}

SIZE="${VISUAL_QA_SIZE:-1280x800}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --size)
      [ "$#" -ge 2 ] || { echo "error: --size requires WIDTHxHEIGHT" >&2; usage >&2; exit 2; }
      SIZE="$2"
      shift 2
      ;;
    --size=*)
      SIZE="${1#--size=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      [ "$#" -eq 0 ] || { echo "error: unexpected argument: $1" >&2; usage >&2; exit 2; }
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! "$SIZE" =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]]; then
  echo "error: invalid size '$SIZE' (expected WIDTHxHEIGHT with positive integers)" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"

MEAN_GATE="1.0"
RATIO_GATE="0.01"
SHOT_SECS="${VISUAL_QA_SHOT_SECS:-120}"
BASELINE_ROOT="$PARENT/loom-design-bible/baselines"
REPORT="$WORK/visual-qa-report.md"
DEST_REPORT="${VISUAL_QA_DEST_REPORT:-$PARENT/visual-qa-report.md}"

# Keep cleanup explicit: these are the only eight application names and the
# only per-run artifact destinations this harness owns. Do not replace this
# with a wildcard or recursive removal; reports and historical evidence stay.
CLEANUP_APPS=(writer sheets present photo motion video studio encode)

cleanup_current_run_artifacts() {
  local app variant
  for app in "${CLEANUP_APPS[@]}"; do
    for variant in light dark; do
      rm -f -- \
        "$WORK/screenshots/${app}-${variant}.png" \
        "$WORK/cmp-${app}-${variant}.log" \
        "$WORK/diffs/${app}-${variant}.diff.png"
    done
  done
}

cleanup_current_run_artifacts
log "CLEAN current-run screenshot, comparison-log, and diff destinations for 8 apps"

field_from_log() {
  local key="$1" file="$2" value
  value="$(awk -v key="$key" '
    {
      for (i = 1; i <= NF; i++) {
        if (index($i, key "=") == 1) {
          print substr($i, length(key) + 2)
          exit
        }
      }
    }
  ' "$file")"
  if [ -n "$value" ]; then
    printf '%s\n' "$value"
  else
    printf 'N/A\n'
  fi
}

metric_is_numeric() {
  [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]]
}

captured=0
compared=0
valid_comparisons=0
diffs=0
size_mismatches=0
nobaseline=0
missing_binaries=0
shot_failures=0
compare_failures=0
failures=""

{
  echo "# Loom Visual QA Report"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/visual-qa-all.sh"
  echo "Size: $SIZE | Gates: mean absolute error < $MEAN_GATE (0..255), differing-pixel ratio < $RATIO_GATE after 1px erosion | Baselines: $BASELINE_ROOT"
  echo
  echo "## Coverage"
  echo
  echo "- executed: default application light/dark theme captures only"
  echo "- full design-bible matrix: NOT RUN by this harness (high-contrast, text-scale, reduced-motion, locale, component/state, and error-state captures are outside this script)"
  echo
  echo "| app | screenshot | dark theme | baseline | mean absolute error | differing-pixel ratio | result | artifacts |"
  echo "|-----|------------|------------|----------|---------------------|------------------------|--------|-----------|"
} > "$REPORT"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  if [ -z "$bin" ]; then
    missing_binaries=$((missing_binaries + 1))
    failures="$failures $app-no-binary"
    echo "| $app | — | — | N/A | N/A | N/A | NO BINARY | actual=N/A; diff=N/A |" >> "$REPORT"
    continue
  fi

  shot="$WORK/screenshots/${app}-light.png"
  log "SHOT $app light"
  if ! run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$shot" --size "$SIZE" --theme light > "$WORK/shot-$app-light.log" 2>&1 || [ ! -s "$shot" ]; then
    shot_failures=$((shot_failures + 1))
    failures="$failures $app-light-screenshot"
    echo "| $app | light | — | N/A | N/A | N/A | FAILED | actual=$shot; diff=N/A; log=$WORK/shot-$app-light.log |" >> "$REPORT"
    continue
  fi
  captured=$((captured + 1))

  dark="no"
  dark_shot="$WORK/screenshots/${app}-dark.png"
  if run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$dark_shot" --size "$SIZE" --theme dark > "$WORK/shot-$app-dark.log" 2>&1 && [ -s "$dark_shot" ]; then
    dark="yes"
    captured=$((captured + 1))
  else
    shot_failures=$((shot_failures + 1))
    failures="$failures $app-dark-screenshot"
    log "FAIL $app: --theme dark unsupported or failed (see $WORK/shot-$app-dark.log)"
    echo "| $app | dark | no | N/A | N/A | N/A | FAILED | actual=$dark_shot; diff=N/A; log=$WORK/shot-$app-dark.log |" >> "$REPORT"
  fi

  for variant in light dark; do
    shot="$WORK/screenshots/${app}-${variant}.png"
    [ -f "$shot" ] || continue
    baseline="$BASELINE_ROOT/$app/${app}-${variant}.png"
    if [ ! -f "$baseline" ]; then
      nobaseline=$((nobaseline + 1))
      failures="$failures $app-$variant-missing-baseline"
      echo "| $app | ${variant} | $dark | MISSING | N/A | N/A | INCOMPLETE | actual=$shot; diff=N/A |" >> "$REPORT"
      continue
    fi

    log "COMPARE $app-$variant vs baseline"
    compared=$((compared + 1))
    cmp_log="$WORK/cmp-$app-$variant.log"
    set +e
    bash "$ROOT/scripts/img-compare.sh" "$baseline" "$shot" "$WORK/diffs" > "$cmp_log" 2>&1
    rc=$?
    set -e

    mean_metric="$(field_from_log mean_absolute_error "$cmp_log")"
    ratio_metric="$(field_from_log differing_pixel_ratio "$cmp_log")"
    comparison_result="$(field_from_log result "$cmp_log")"
    diff_artifact="$WORK/diffs/$(basename "$shot" .png).diff.png"

    case "$rc" in
      0)
        if metric_is_numeric "$mean_metric" && metric_is_numeric "$ratio_metric" && [ "$comparison_result" = "PASS" ] && [ -s "$diff_artifact" ]; then
          valid_comparisons=$((valid_comparisons + 1))
          echo "| $app | ${variant} | $dark | ok | $mean_metric | $ratio_metric | PASS | actual=$shot; diff=$diff_artifact |" >> "$REPORT"
        else
          compare_failures=$((compare_failures + 1))
          failures="$failures $app-$variant-comparison-output"
          echo "| $app | ${variant} | $dark | N/A | N/A | N/A | ERROR | actual=$shot; diff=N/A; log=$cmp_log |" >> "$REPORT"
        fi
        ;;
      1)
        if [ "$comparison_result" = "SIZE_MISMATCH" ]; then
          size_mismatches=$((size_mismatches + 1))
          failures="$failures $app-$variant-size-mismatch"
          echo "| $app | ${variant} | $dark | size mismatch | N/A | N/A | SIZE MISMATCH | actual=$shot; diff=N/A; log=$cmp_log |" >> "$REPORT"
        elif metric_is_numeric "$mean_metric" && metric_is_numeric "$ratio_metric" && [ "$comparison_result" = "FAIL" ] && [ -s "$diff_artifact" ]; then
          valid_comparisons=$((valid_comparisons + 1))
          diffs=$((diffs + 1))
          failures="$failures $app-$variant"
          echo "| $app | ${variant} | $dark | DIFF | $mean_metric | $ratio_metric | FAIL | actual=$shot; diff=$diff_artifact |" >> "$REPORT"
        else
          compare_failures=$((compare_failures + 1))
          failures="$failures $app-$variant-comparison-output"
          echo "| $app | ${variant} | $dark | N/A | N/A | N/A | ERROR | actual=$shot; diff=N/A; log=$cmp_log |" >> "$REPORT"
        fi
        ;;
      2)
        compare_failures=$((compare_failures + 1))
        failures="$failures $app-$variant-compare-tool"
        echo "| $app | ${variant} | $dark | N/A | N/A | N/A | ERROR | actual=$shot; diff=N/A; log=$cmp_log |" >> "$REPORT"
        ;;
      *)
        compare_failures=$((compare_failures + 1))
        failures="$failures $app-$variant"
        echo "| $app | ${variant} | $dark | N/A | N/A | N/A | ERROR | actual=$shot; diff=N/A; log=$cmp_log (exit=$rc) |" >> "$REPORT"
        ;;
    esac
  done
done

{
  echo
  echo "## Summary"
  echo
  echo "- screenshots captured: $captured"
  echo "- fixed comparison gates: mean absolute error < $MEAN_GATE (0..255) AND differing-pixel ratio < $RATIO_GATE after 1px erosion"
  echo "- comparisons run: $compared"
  echo "- valid comparisons: $valid_comparisons"
  echo "- diffs beyond gates: $diffs"
  echo "- size mismatches: $size_mismatches"
  echo "- missing baselines: $nobaseline (INCOMPLETE when non-zero)"
  echo "- screenshot failures: $shot_failures"
  echo "- apps missing binaries: $missing_binaries"
  echo "- comparison/input failures: $compare_failures"
  echo "- coverage run: default application light/dark themes only"
  echo "- full design-bible matrix: NOT RUN by this harness"
  echo "- provenance caveat: this gate proves capture/baseline consistency; it is not an independent regression result when baselines come from the same reviewed capture set"
  if [ "$diffs" -eq 0 ] && [ "$size_mismatches" -eq 0 ] && [ "$nobaseline" -eq 0 ] && [ "$shot_failures" -eq 0 ] && [ "$missing_binaries" -eq 0 ] && [ "$compare_failures" -eq 0 ]; then
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
log "SUMMARY visual-qa: captured=$captured compared=$compared valid_comparisons=$valid_comparisons diffs=$diffs size_mismatches=$size_mismatches nobaseline=$nobaseline screenshot_failures=$shot_failures missing_binaries=$missing_binaries compare_failures=$compare_failures"

if [ "$diffs" -gt 0 ] || [ "$size_mismatches" -gt 0 ] || [ "$nobaseline" -gt 0 ] || [ "$shot_failures" -gt 0 ] || [ "$missing_binaries" -gt 0 ] || [ "$compare_failures" -gt 0 ]; then
  log "FAILED_OR_INCOMPLETE:$failures"
  log "diffs: $WORK/diffs/"
  exit 1
fi
log "RESULT: PASS"
