#!/usr/bin/env bash
# Compare two screenshots using the design-bible RGBA visual-QA contract.
# Usage: img-compare.sh <baseline.png> <actual.png> [outdir]
# A same-size comparison always writes a diff image to <outdir>.
# Exit codes:
#   0  both contract gates pass
#   1  valid comparison fails a gate, or the image sizes differ
#   2  invalid input or comparison tooling unavailable
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: img-compare.sh <baseline.png> <actual.png> [outdir]" >&2
  exit 2
fi

BASE="$1"
ACTUAL="$2"
OUTDIR="${3:-.}"

report_error() {
  printf 'mean_absolute_error=N/A differing_pixel_ratio=N/A result=ERROR reason=%s\n' "$1"
}

if [ ! -f "$BASE" ]; then
  echo "baseline missing: $BASE" >&2
  report_error baseline_missing
  exit 2
fi
if [ ! -f "$ACTUAL" ]; then
  echo "actual missing: $ACTUAL" >&2
  report_error actual_missing
  exit 2
fi
if ! mkdir -p "$OUTDIR"; then
  echo "cannot create comparison output directory: $OUTDIR" >&2
  report_error output_directory_error
  exit 2
fi

ACTUAL_NAME="$(basename "$ACTUAL")"
ACTUAL_NAME="${ACTUAL_NAME%.*}"
DIFF_OUT="$OUTDIR/${ACTUAL_NAME}.diff.png"

# ImageMagick's compare output and quantum-depth behavior vary by version. The
# design-bible gates require both RGBA metrics, so do not use it as a partial
# fallback that could turn an unevaluated comparison into PASS.
if ! command -v python3 >/dev/null 2>&1 || ! python3 -c 'from PIL import Image' >/dev/null 2>&1; then
  echo "comparison unavailable: python3 with PIL is required for RGBA MAE and eroded differing-pixel ratio" >&2
  report_error comparison_unavailable
  exit 2
fi

python3 - "$BASE" "$ACTUAL" "$DIFF_OUT" <<'PYEOF'
import sys

from PIL import Image, ImageChops, ImageFilter, ImageStat


MEAN_GATE = 1.0
RATIO_GATE = 0.01
CHANNEL_THRESHOLD = 8

base_path, actual_path, diff_path = sys.argv[1:4]


def error(reason: str, detail: str = "") -> None:
    if detail:
        print(f"comparison error: {detail}", file=sys.stderr)
    print(
        "mean_absolute_error=N/A "
        f"differing_pixel_ratio=N/A result=ERROR reason={reason}"
    )


try:
    with Image.open(base_path) as source:
        baseline = source.convert("RGBA")
    with Image.open(actual_path) as source:
        actual = source.convert("RGBA")
except (OSError, ValueError) as exc:
    error("image_read_error", str(exc))
    sys.exit(2)

if baseline.size != actual.size:
    print(
        "mean_absolute_error=N/A "
        "differing_pixel_ratio=N/A result=SIZE_MISMATCH "
        f"baseline_size={baseline.width}x{baseline.height} "
        f"actual_size={actual.width}x{actual.height}"
    )
    sys.exit(1)

try:
    total_pixels = baseline.width * baseline.height
    if total_pixels <= 0:
        raise ValueError("image has no pixels")

    difference = ImageChops.difference(actual, baseline)

    # ImageStat reports a mean for each RGBA channel. Averaging those channel
    # means gives the required 0..255 mean absolute error across pixels.
    channel_means = ImageStat.Stat(difference).mean
    mean_absolute_error = sum(channel_means) / len(channel_means)

    # A pixel differs when any channel exceeds the contract's absolute
    # threshold. MinFilter(3) is a one-pixel binary erosion; isolated one-pixel
    # shift outlines therefore disappear before the ratio is calculated.
    mask = Image.new("L", difference.size)
    difference_pixels = difference.load()
    mask.putdata(
        [
            255
            if max(difference_pixels[x, y]) > CHANNEL_THRESHOLD
            else 0
            for y in range(difference.height)
            for x in range(difference.width)
        ]
    )
    eroded = mask.filter(ImageFilter.MinFilter(3))
    eroded_pixels = eroded.load()
    differing_pixels = sum(
        eroded_pixels[x, y] != 0
        for y in range(eroded.height)
        for x in range(eroded.width)
    )
    differing_pixel_ratio = differing_pixels / total_pixels

    # Keep the artifact visible when RGB differs even though RGBA difference
    # images normally have a zero alpha channel when source alpha is equal.
    artifact = difference.copy()
    artifact.putalpha(Image.new("L", difference.size, 255))
    artifact.save(diff_path)
except (OSError, ValueError, TypeError) as exc:
    error("comparison_error", str(exc))
    sys.exit(2)

passes = mean_absolute_error < MEAN_GATE and differing_pixel_ratio < RATIO_GATE
result = "PASS" if passes else "FAIL"
print(
    f"mean_absolute_error={mean_absolute_error:.6f} "
    f"differing_pixel_ratio={differing_pixel_ratio:.6f} "
    f"mean_gate=<{MEAN_GATE:.1f} ratio_gate=<{RATIO_GATE:.2f} "
    f"result={result} diff={diff_path}"
)
sys.exit(0 if passes else 1)
PYEOF
