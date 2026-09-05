#!/usr/bin/env bash
# Capture each application in light, dark, and high-contrast themes. This is a
# smoke matrix, not a baseline approval tool. It also rejects byte-identical
# theme captures, which previously allowed false light/dark claims.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib.sh"
SIZE="${VISUAL_QA_SIZE:-1280x800}"
SHOT_SECS="${VISUAL_QA_SHOT_SECS:-120}"
OUT="$WORK/theme-matrix"
REPORT="$WORK/theme-matrix-report.md"
mkdir -p "$OUT"
rm -f "$OUT"/*.png "$REPORT"

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

fail=0
{
  echo "# Loom theme smoke matrix"
  echo
  echo "| app | light | dark | high contrast | distinct |"
  echo "|---|---:|---:|---:|---:|"
} > "$REPORT"

for app in $APPS; do
  bin="$(find_app_bin "$app")"
  if [ -z "$bin" ]; then
    echo "| $app | no binary | no binary | no binary | FAIL |" >> "$REPORT"
    fail=1
    continue
  fi

  ok=1
  for theme in light dark high-contrast; do
    shot="$OUT/${app}-${theme}.png"
    if ! run_with_timeout "$SHOT_SECS" "$bin" --screenshot "$shot" --size "$SIZE" --theme "$theme" > "$WORK/theme-$app-$theme.log" 2>&1 || [ ! -s "$shot" ]; then
      ok=0
      fail=1
    fi
  done

  distinct="FAIL"
  if [ "$ok" -eq 1 ]; then
    light_hash="$(hash_file "$OUT/${app}-light.png")"
    dark_hash="$(hash_file "$OUT/${app}-dark.png")"
    hc_hash="$(hash_file "$OUT/${app}-high-contrast.png")"
    if [ "$light_hash" != "$dark_hash" ] && [ "$dark_hash" != "$hc_hash" ] && [ "$light_hash" != "$hc_hash" ]; then
      distinct="PASS"
    else
      fail=1
    fi
  fi
  echo "| $app | $([ -s "$OUT/${app}-light.png" ] && echo PASS || echo FAIL) | $([ -s "$OUT/${app}-dark.png" ] && echo PASS || echo FAIL) | $([ -s "$OUT/${app}-high-contrast.png" ] && echo PASS || echo FAIL) | $distinct |" >> "$REPORT"
done

cat "$REPORT"
[ "$fail" -eq 0 ]
