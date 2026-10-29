#!/usr/bin/env python3
"""Render original Loom SVG masters as cross-platform application icon assets.

Requires: pip install cairosvg pillow
Usage:
    python loom-bootstrap/scripts/generate-app-icons.py
    python loom-bootstrap/scripts/generate-app-icons.py --check
"""
from __future__ import annotations

import argparse
import io
import struct
from pathlib import Path

APPS = ("photo", "motion", "video", "studio", "encode")
ROOT = Path(__file__).resolve().parents[2]
ICONS = ROOT / "loom-core/crates/loom-ui/ui/app-icons"
SIZES = ((16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256))


def build_icon(app: str) -> None:
    import cairosvg
    from PIL import Image

    source = ICONS / f"loom-{app}.svg"
    image = Image.open(io.BytesIO(cairosvg.svg2png(
        url=str(source), output_width=1024, output_height=1024
    ))).convert("RGBA")
    if image.size != (1024, 1024) or image.getpixel((0, 0))[3] != 0:
        raise ValueError(f"{app}: expected transparent 1024x1024 master")
    image.save(ICONS / f"loom-{app}.png", optimize=True)
    image.resize((512, 512), Image.Resampling.LANCZOS).save(
        ICONS / f"loom-{app}.512.png", optimize=True
    )
    image.save(ICONS / f"loom-{app}.ico", format="ICO", sizes=SIZES)
    image.save(ICONS / f"loom-{app}.icns", format="ICNS")


def check_icon(app: str) -> None:
    from PIL import Image

    base = ICONS / f"loom-{app}"
    files = [Path(str(base) + ext) for ext in (".svg", ".png", ".512.png", ".ico", ".icns")]
    missing = [str(f) for f in files if not f.exists()]
    if missing:
        raise FileNotFoundError(f"{app}: missing icons: {', '.join(missing)}")
    for suffix, dimensions in ((".png", (1024, 1024)), (".512.png", (512, 512))):
        with Image.open(str(base) + suffix) as image:
            if image.size != dimensions:
                raise ValueError(f"{app}{suffix}: size {image.size}, expected {dimensions}")
            if image.convert("RGBA").getpixel((0, 0))[3] != 0:
                raise ValueError(f"{app}{suffix}: transparent margin was lost")
    with Image.open(str(base) + ".ico") as image:
        if image.format != "ICO" or image.size != (256, 256):
            raise ValueError(f"{app}: invalid ICO")
    with Image.open(str(base) + ".icns") as image:
        if image.format != "ICNS":
            raise ValueError(f"{app}: invalid ICNS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="validate existing exports")
    options = parser.parse_args()
    for app in APPS:
        if not options.check:
            build_icon(app)
        check_icon(app)
        print(f"PASS {app}")
    print("All five Loom application icons validated")


if __name__ == "__main__":
    main()
