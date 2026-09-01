#!/usr/bin/env python3
"""Validate the unaccepted Loom UI foundation without pretending to judge beauty."""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "loom-design-bible/contracts/ui-foundation.toml"
FACADE = ROOT / "loom-core/crates/loom-ui/ui/foundation.slint"
FOUNDATION = ROOT / "loom-core/crates/loom-ui/ui/foundation"
GALLERY = FOUNDATION / "gallery.slint"
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
errors: list[str] = []

config = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
if config.get("status") != "ACCEPTANCE_BLOCKED":
    errors.append("foundation status changed without explicit acceptance")
if config.get("consumer_imports_allowed") is not False:
    errors.append("application consumption must remain locked before acceptance")
if config.get("approved_baselines") is not False:
    errors.append("an unaccepted foundation cannot have approved baselines")

if not FACADE.is_file() or not GALLERY.is_file():
    errors.append("foundation facade or gallery is missing")
else:
    facade_text = FACADE.read_text(encoding="utf-8")
    gallery_text = GALLERY.read_text(encoding="utf-8")
    source_text = "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted(FOUNDATION.rglob("*.slint"))
    )
    for component in config.get("required_components", []):
        if not re.search(rf"export component\s+{re.escape(component)}\b", source_text):
            errors.append(f"required foundation component is not implemented: {component}")
        if component not in facade_text:
            errors.append(f"required foundation component is not exported: {component}")
        if component not in gallery_text:
            errors.append(f"required foundation component is not demonstrated: {component}")

    for path in sorted(FOUNDATION.rglob("*.slint")):
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(ROOT)
        if re.search(r"#[0-9a-fA-F]{3,8}\b", text):
            errors.append(f"foundation contains a hard-coded color instead of Theme tokens: {rel}")
        for token in ("TODO", "TBD", "HACK", "AppleToolbarItem"):
            if token in text:
                errors.append(f"foundation contains forbidden token {token}: {rel}")

    for theme in config.get("required_themes", []):
        if theme not in gallery_text:
            errors.append(f"gallery does not expose required theme: {theme}")

# The gate is intentionally human-blocked. Static code can prove mechanics,
# not aesthetic excellence, so no image file under a foundation baseline path
# is allowed before acceptance.
baseline_root = ROOT / "loom-core/crates/loom-ui/baselines/foundation"
if baseline_root.exists() and any(path.is_file() for path in baseline_root.rglob("*")):
    errors.append("foundation baselines exist before human visual acceptance")

for app in APPS:
    ui_root = ROOT / f"loom-{app}" / "crates" / f"loom-{app}-app" / "ui"
    if not ui_root.exists():
        continue
    for path in ui_root.rglob("*.slint"):
        text = path.read_text(encoding="utf-8")
        if "foundation.slint" in text or "/foundation/" in text:
            errors.append(f"locked app imports unaccepted foundation: {path.relative_to(ROOT)}")

if errors:
    print("Loom UI foundation audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print("Loom UI foundation audit: PASS (mechanical/source contract only; visual acceptance still human-blocked)")
