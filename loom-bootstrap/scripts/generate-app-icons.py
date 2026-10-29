#!/usr/bin/env python3
"""Export approved Loom PNG masters without redrawing any artwork.

Requires: pip install pillow
Usage:
    python loom-bootstrap/scripts/generate-app-icons.py
    python loom-bootstrap/scripts/generate-app-icons.py --check
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

APPS = ("photo", "motion", "video", "studio", "encode")
ROOT = Path(__file__).resolve().parents[2]
ICONS = ROOT / "loom-core/crates/loom-ui/ui/app-icons"
SIZES = ((16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256))


def validate_source(app: str) -> None:
    """Reject any changed source image, including metadata or byte-level edits."""
    from PIL import Image

    source = ICONS / f"loom-{app}.source.png"
    if not source.is_file():
        raise FileNotFoundError(f"Missing approved PNG master: {source}")
    manifest = json.loads((ICONS / "approved-source-sha256.json").read_text())
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != manifest[app]:
        raise ValueError(f"{app}: source is not byte-identical to approved PNG")
    with Image.open(source) as image:
        if image.size != (1254, 1254) or image.mode != "RGBA":
            raise ValueError(f"{app}: unexpected master size or color mode")


def build_icon(app: str) -> None:
    from PIL import Image

    validate_source(app)
    source = ICONS / f"loom-{app}.source.png"
    image = Image.open(source).convert("RGBA")
    image = image.resize((1024, 1024), Image.Resampling.LANCZOS)
    image.save(ICONS / f"loom-{app}.png", optimize=True)
    image.resize((512, 512), Image.Resampling.LANCZOS).save(
        ICONS / f"loom-{app}.512.png", optimize=True
    )
    image.save(ICONS / f"loom-{app}.ico", format="ICO", sizes=SIZES)
    image.save(ICONS / f"loom-{app}.icns", format="ICNS")


def check_icon(app: str) -> None:
    from PIL import Image

    validate_source(app)

    base = ICONS / f"loom-{app}"
    files = [Path(str(base) + ext) for ext in (".source.png", ".png", ".512.png", ".ico", ".icns")]
    missing = [str(f) for f in files if not f.exists()]
    if missing:
        raise FileNotFoundError(f"{app}: missing icons: {', '.join(missing)}")
    for suffix, dimensions in ((".png", (1024, 1024)), (".512.png", (512, 512))):
        with Image.open(str(base) + suffix) as image:
            if image.size != dimensions:
                raise ValueError(f"{app}{suffix}: size {image.size}, expected {dimensions}")
            if image.convert("RGBA").getpixel((0, 0))[3] > 1:
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
