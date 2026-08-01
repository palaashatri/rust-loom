#!/usr/bin/env bash
# Tiny image-comparison helper for visual QA.
# Usage: img-compare.sh <baseline.png> <actual.png> <outdir>
# Prints metric=tolerance values; writes a diff image next to the actual image.
# Exit codes:
#   0  within tolerance (PASS)
#   1  beyond tolerance or size mismatch (FAIL)
#   2  comparison unavailable (no ImageMagick, no python3+PIL)
set -euo pipefail

BASE="${1:-}"
ACTUAL="${2:-}"
OUTDIR="${3:-}"
[ -f "$BASE" ] || { echo "baseline missing: $BASE"; exit 2; }
[ -f "$ACTUAL" ] || { echo "actual missing: $ACTUAL"; exit 2; }
[ -n "$OUTDIR" ] || OUTDIR="."
mkdir -p "$OUTDIR"

TOLERANCE="${VISUAL_QA_TOLERANCE:-0.02}"
DIFF_OUT="$OUTDIR/$(basename "$ACTUAL" .png).diff.png"

if command -v compare >/dev/null 2>&1; then
  METRIC=$(compare -metric RMSE "$BASE" "$ACTUAL" "$DIFF_OUT" 2>&1 | awk '{print $1}')
  METRIC="${METRIC:-1}"
  printf 'metric=%s tolerance=%s\n' "$METRIC" "$TOLERANCE"
  awk -v m="$METRIC" -v t="$TOLERANCE" 'BEGIN { exit !(m <= t) }'
  exit $?
fi

if command -v python3 >/dev/null 2>&1 && python3 -c 'import PIL' >/dev/null 2>&1; then
  VISUAL_QA_TOLERANCE="$TOLERANCE" python3 - "$BASE" "$ACTUAL" "$DIFF_OUT" <<'PYEOF'
import os
import sys
from PIL import Image, ImageChops

base, actual, diff_out = sys.argv[1:4]
tolerance = float(os.environ.get("VISUAL_QA_TOLERANCE", "0.02"))
b = Image.open(base).convert("RGB")
a = Image.open(actual).convert("RGB")
if a.size != b.size:
    print(f"size-mismatch baseline={b.size} actual={a.size}")
    sys.exit(1)
diff = ImageChops.difference(a, b)
hist = diff.histogram()
total = 0
mse = 0
for i, count in enumerate(hist):
    v = i % 256
    total += count
    mse += count * v * v
if total == 0:
    mse = 0.0
else:
    mse /= total
rmse = (mse ** 0.5) / 255.0
diff.save(diff_out)
print(f"metric={rmse:.6f} tolerance={tolerance}")
sys.exit(0 if rmse <= tolerance else 1)
PYEOF
  exit $?
fi

echo "compare unavailable (no ImageMagick 'compare' and no python3+PIL); actual image kept as-is"
exit 2
