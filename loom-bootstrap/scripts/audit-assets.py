#!/usr/bin/env python3
"""Require explicit provenance for repository visual/font/audio assets."""
from __future__ import annotations

import fnmatch
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "loom-bootstrap/contracts/assets.toml"
TRACKED_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".webp", ".svg",
    ".ttf", ".otf", ".woff", ".woff2",
    ".wav", ".mp3", ".flac", ".ogg",
}
SKIP_PARTS = {".git", ".work", "target", "dist", "__pycache__"}
errors: list[str] = []

config = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
allow = config.get("allow", [])

for entry in allow:
    if entry.get("commercial") is not True or entry.get("redistributable") is not True:
        errors.append(f"asset allow rule is not commercially redistributable: {entry.get('glob', '<missing>')}")
    for field in ("glob", "origin", "license"):
        if not str(entry.get(field, "")).strip():
            errors.append(f"asset allow rule missing {field}: {entry}")

for path in ROOT.rglob("*"):
    if not path.is_file() or any(part in SKIP_PARTS for part in path.parts):
        continue
    if path.suffix.lower() not in TRACKED_EXTENSIONS:
        continue
    rel = path.relative_to(ROOT).as_posix()
    if not any(fnmatch.fnmatch(rel, entry["glob"]) for entry in allow):
        errors.append(f"unregistered asset provenance: {rel}")

if errors:
    print("Loom asset audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print("Loom asset audit: PASS")
