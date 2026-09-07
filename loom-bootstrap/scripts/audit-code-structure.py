#!/usr/bin/env python3
"""Enforce small-source budgets without breaking registered legacy debt."""
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "loom-bootstrap/contracts/code-quality.toml"
SKIP_PARTS = {".git", ".work", "target", "dist", "__pycache__"}
errors: list[str] = []

config = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
limits = config["limits"]
legacy = {str(path): int(cap) for path, cap in config.get("legacy_byte_caps", {}).items()}

suffix_caps = {
    ".rs": int(limits["rust_max_bytes"]),
    ".slint": int(limits["slint_max_bytes"]),
    ".py": int(limits["python_max_bytes"]),
    ".sh": int(limits["shell_max_bytes"]),
}

for path in ROOT.rglob("*"):
    if not path.is_file() or any(part in SKIP_PARTS for part in path.parts):
        continue
    cap = suffix_caps.get(path.suffix.lower())
    if cap is None:
        continue
    rel = path.relative_to(ROOT).as_posix()
    size = path.stat().st_size
    if rel in legacy:
        if size > legacy[rel]:
            errors.append(f"legacy source grew: {rel}: {size} > {legacy[rel]} bytes")
        continue
    if size > cap:
        errors.append(f"source exceeds {path.suffix} budget: {rel}: {size} > {cap} bytes")

# The new foundation is held to stricter architecture rules than legacy UI.
foundation = ROOT / "loom-core/crates/loom-ui/ui/foundation"
if foundation.exists():
    for path in foundation.rglob("*.slint"):
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(ROOT).as_posix()
        for forbidden in ("TODO", "TBD", "HACK", "AppleToolbarItem", "ToolbarButton"):
            if forbidden in text:
                errors.append(f"foundation contains forbidden legacy/placeholder token {forbidden}: {rel}")

if errors:
    print("Loom code-structure audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"Loom code-structure audit: PASS ({len(legacy)} legacy ceilings enforced)")
