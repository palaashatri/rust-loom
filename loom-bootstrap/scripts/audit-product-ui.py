#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
APPS = ["writer", "sheets", "present", "photo", "motion", "video", "studio", "encode"]
failures = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")


def slider_blocks(text: str):
    cursor = 0
    while True:
        start = text.find("Slider {", cursor)
        if start < 0:
            return
        opening = text.find("{", start)
        depth = 0
        for index in range(opening, len(text)):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    yield text[start:index + 1]
                    cursor = index + 1
                    break
        else:
            failures.append("shared UI: unterminated Slider block")
            return


def application_ui_text(app: str) -> str:
    """Read the complete Slint module graph owned by an application."""
    ui_dir = ROOT / f"loom-{app}/crates/loom-{app}-app/ui"
    files = sorted(ui_dir.glob("*.slint"))
    if not files:
        failures.append(f"{app}: no Slint UI modules found")
        return ""
    return "\n".join(file.read_text(encoding="utf-8") for file in files)


for app in APPS:
    main = ROOT / f"loom-{app}/crates/loom-{app}-app/src/main.rs"
    text = application_ui_text(app)
    main_text = main.read_text(encoding="utf-8")
    for token, message in (
        ("AppHeader {", "missing shared AppHeader"),
        ("StatusBar {", "missing shared StatusBar"),
        ("Theme.palette()", "bypasses semantic palette"),
        ("min-width:", "missing minimum responsive width"),
        ("min-height:", "missing minimum responsive height"),
        ("horizontal-stretch", "missing horizontal adaptive layout"),
        ("vertical-stretch", "missing vertical adaptive layout"),
        ("compact-layout", "missing compact desktop layout policy"),
    ):
        if token not in text:
            failures.append(f"{app}: {message}")
    if emoji.search(text):
        failures.append(f"{app}: emoji/icon-font glyphs remain in professional UI")
    if re.search(r"#[0-9a-fA-F]{6,8}", text):
        failures.append(f"{app}: hard-coded color outside the shared theme")
    if any(
        token in text.lower()
        for token in (
            "coming soon",
            "placeholder ui",
            "placeholder control",
            "fake progress",
            "model preview",
        )
    ):
        failures.append(f"{app}: prototype or fabricated-state language remains")
    if "!other.starts_with('-') && args.open.is_none()" not in main_text:
        failures.append(f"{app}: native shell positional document opening is not supported")
    for slider in slider_blocks(text):
        if "label:" not in slider:
            failures.append(f"{app}: slider lacks a semantic accessibility label")

shared = (ROOT / "loom-core/crates/loom-ui/ui/components.slint").read_text(encoding="utf-8")
for component in [
    "WorkspaceToolbar",
    "SidebarSurface",
    "InspectorSurface",
    "PaneTabs",
    "CanvasBackdrop",
    "TransportButton",
]:
    if f"export component {component}" not in shared:
        failures.append(f"shared UI: missing {component}")
for component in (
    "ToolButton",
    "IconButton",
    "PrimaryButton",
    "SegmentedControl",
    "Slider",
    "WorkspaceRow",
    "PaneTabs",
    "TransportButton",
):
    start = shared.find(f"export component {component}")
    if start < 0:
        continue
    end = shared.find("\nexport component ", start + 1)
    block = shared[start:] if end < 0 else shared[start:end]
    if "accessible-role" not in block or "accessible-label" not in block:
        failures.append(f"shared UI: {component} lacks accessible role or label")
    if component != "Slider" and "accessible-action-default" not in block:
        failures.append(f"shared UI: {component} lacks an accessible default action")
    if "key-pressed(event)" not in block:
        failures.append(f"shared UI: {component} lacks keyboard interaction")

theme = (ROOT / "loom-core/crates/loom-ui/ui/theme.slint").read_text(encoding="utf-8")
for token in [
    "surface-raised",
    "chrome",
    "panel",
    "shadow",
    "grid-major",
    "control-height",
    "header-height",
    "reduced-motion",
]:
    if token not in theme:
        failures.append(f"theme: missing product token {token}")

native = (ROOT / ".github/workflows/cross-platform.yml").read_text(encoding="utf-8")
for token in (
    "windows-2025",
    "macos-15",
    "macos-15-intel",
    "native-ui-matrix.py",
    "1024x720",
    "1440x900",
    "1920x1200",
    "upload-artifact",
):
    if token not in native:
        failures.append(f"native UI validation: missing {token}")

native_matrix = (ROOT / "loom-bootstrap/scripts/native-ui-matrix.py").read_text(encoding="utf-8")
for token in ("png_dimensions", "find_sample", "sample_open", "one or more theme/size captures are byte-identical"):
    if token not in native_matrix:
        failures.append(f"native UI matrix: missing evidence check {token}")

packaging = (ROOT / "loom-bootstrap/packaging/release.py").read_text(encoding="utf-8")
for token in ("DOCUMENT_TYPES", "MimeType=", "RegistryValue", "CFBundleDocumentTypes"):
    if token not in packaging:
        failures.append(f"native packaging: missing {token}")

if failures:
    print("Loom UI productisation audit failed:")
    for failure in failures:
        print(f"- {failure}")
    sys.exit(1)
print("Loom UI productisation audit passed for all eight applications and native targets")
