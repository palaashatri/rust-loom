#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
APPS = ["writer", "sheets", "present", "photo", "motion", "video", "studio", "encode"]
failures = []
emoji = re.compile("[\\U0001F000-\\U0001FAFF\\u2600-\\u27BF]")
for app in APPS:
    file = ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint"
    text = file.read_text()
    if "AppHeader {" not in text:
        failures.append(f"{app}: missing shared AppHeader")
    if "StatusBar {" not in text:
        failures.append(f"{app}: missing shared StatusBar")
    if "Theme.palette()" not in text:
        failures.append(f"{app}: bypasses semantic palette")
    if emoji.search(text):
        failures.append(f"{app}: emoji/icon-font glyphs remain in professional UI")
    if 'state-text: "Model preview"' in text:
        failures.append(f"{app}: legacy prototype status remains")
    if re.search(r"#[0-9a-fA-F]{6,8}", text):
        failures.append(f"{app}: hard-coded color outside the shared theme")

shared = (ROOT / "loom-core/crates/loom-ui/ui/components.slint").read_text()
for component in ["WorkspaceToolbar", "SidebarSurface", "InspectorSurface", "PaneTabs", "CanvasBackdrop", "TransportButton"]:
    if f"export component {component}" not in shared:
        failures.append(f"shared UI: missing {component}")

theme = (ROOT / "loom-core/crates/loom-ui/ui/theme.slint").read_text()
for token in ["surface-raised", "chrome", "panel", "shadow", "grid-major", "control-height", "header-height"]:
    if token not in theme:
        failures.append(f"theme: missing product token {token}")

if failures:
    print("Loom UI productisation audit failed:")
    for failure in failures:
        print(f"- {failure}")
    sys.exit(1)
print("Loom UI productisation audit passed for all eight applications")
