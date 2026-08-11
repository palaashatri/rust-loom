#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
APPS = ["writer", "sheets", "present", "photo", "motion", "video", "studio", "encode"]
failures = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")
slint_reference = re.compile(r'(?:from\s+|export\s+\{[^}]+\}\s+from\s+)["\']([^"\']+\.slint)["\']')


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
    """Read only the active Slint module graph rooted at app.slint."""
    ui_dir = ROOT / f"loom-{app}/crates/loom-{app}-app/ui"
    entry = ui_dir / "app.slint"
    if not entry.is_file():
        failures.append(f"{app}: missing app.slint entry point")
        return ""

    visited: set[Path] = set()
    chunks: list[str] = []

    def visit(path: Path) -> None:
        resolved = path.resolve()
        if resolved in visited or not path.is_file():
            return
        visited.add(resolved)
        text = path.read_text(encoding="utf-8")
        chunks.append(text)
        for reference in slint_reference.findall(text):
            candidate = path.parent / reference
            try:
                candidate.resolve().relative_to(ui_dir.resolve())
            except ValueError:
                continue
            if candidate.is_file():
                visit(candidate)

    visit(entry)
    return "\n".join(chunks)


application_texts: dict[str, str] = {}
for app in APPS:
    main = ROOT / f"loom-{app}/crates/loom-{app}-app/src/main.rs"
    text = application_ui_text(app)
    application_texts[app] = text
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

for app, tokens in {
    "photo": ("canvas-pan := TouchArea", "pressed-x", "viewport-pan-x", "key-pressed(event)"),
    "motion": ("drag := TouchArea", "pressed-x", "transform-changed", "key-pressed(event)"),
    "video": ("ruler-scrub := TouchArea", "playhead-seconds", "root.seek(", "key-pressed(event)"),
}.items():
    text = application_texts[app]
    for token in tokens:
        if token not in text:
            failures.append(f"{app}: missing direct-manipulation contract {token}")

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
for token in ("png_dimensions", "find_sample", "generated-samples", "sample_open", "one or more theme/size captures are byte-identical"):
    if token not in native_matrix:
        failures.append(f"native UI matrix: missing evidence check {token}")

functional_matrix = (ROOT / "loom-bootstrap/scripts/native-functional-matrix.py").read_text(encoding="utf-8")
for token in ("validate_package", "export-md", "render-demo", "sine", "recover", "native-functional-matrix.json"):
    if token not in functional_matrix:
        failures.append(f"native functional matrix: missing journey evidence {token}")

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
