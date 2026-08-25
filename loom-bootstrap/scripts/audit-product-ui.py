#!/usr/bin/env python3
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
APPS = ["writer", "sheets", "present", "photo", "motion", "video", "studio", "encode"]
TOOLKIT_MIGRATED_APPS = {"writer"}
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


def audit_mechanical_design_contract() -> None:
    """Enforce the source-of-truth chain before inspecting application UI."""
    contract_path = ROOT / "loom-design-bible/contracts/desktop-ui.toml"
    tokens_path = ROOT / "loom-design-bible/tokens/loom.toml"
    standard_path = ROOT / "loom-design-bible/MECHANICAL_DESIGN_STANDARD.md"
    theme_path = ROOT / "loom-core/crates/loom-ui/ui/theme.slint"

    for path in (contract_path, tokens_path, standard_path, theme_path):
        if not path.is_file():
            failures.append(f"design system: missing {path.relative_to(ROOT)}")
            return

    with contract_path.open("rb") as handle:
        contract = tomllib.load(handle)
    with tokens_path.open("rb") as handle:
        tokens = tomllib.load(handle)
    theme = theme_path.read_text(encoding="utf-8").lower()

    if contract.get("format-version") != "1.0.0":
        failures.append("design system: unsupported desktop UI contract version")
    if tokens.get("format-version") != "2.0.0":
        failures.append("design system: unsupported design token version")

    for theme_name in ("light", "dark", "high-contrast"):
        contract_palette = contract.get("palette", {}).get(theme_name, {})
        token_palette = tokens.get("palette", {}).get(theme_name, {})
        if contract_palette != token_palette:
            failures.append(f"design system: {theme_name} palette contract/token drift")
            continue
        for key, value in contract_palette.items():
            expected = f"{key}: {str(value).lower()}"
            if expected not in theme:
                failures.append(
                    f"design system: runtime theme missing {theme_name} {key}={value}"
                )

    metric_pairs = {
        "control-height": ("controls", "standard-height"),
        "compact-control-height": ("controls", "compact-height"),
        "toolbar-height": ("chrome", "toolbar-height"),
        "header-height": ("chrome", "title-height"),
        "panel-header-height": ("chrome", "panel-header-height"),
    }
    token_metrics = tokens.get("metrics", {})
    for runtime_name, (section, contract_name) in metric_pairs.items():
        contract_value = contract.get(section, {}).get(contract_name)
        token_value = token_metrics.get(runtime_name)
        if contract_value != token_value:
            failures.append(
                f"design system: metric drift {runtime_name}: contract={contract_value}, token={token_value}"
            )
        expected = f"{runtime_name}: {contract_value}px"
        if expected not in theme:
            failures.append(f"design system: runtime theme missing {expected}")

    validation = contract.get("validation", {})
    expected_viewports = [[1024, 720], [1280, 800], [1440, 900], [1920, 1200]]
    if validation.get("required-viewports") != expected_viewports:
        failures.append("design system: required viewport matrix was weakened or reordered")
    if validation.get("max-unintentional-overlap-px") != 0:
        failures.append("design system: overlap tolerance must remain zero")
    if validation.get("max-control-label-clipping-px") != 0:
        failures.append("design system: control-label clipping tolerance must remain zero")
    if contract.get("toolbar", {}).get("max-groups") != 3:
        failures.append("design system: toolbar group maximum must remain three")
    if contract.get("controls", {}).get("minimum-pointer-target") != 28:
        failures.append("design system: default desktop pointer target must remain 28px")


def audit_toolkit_source() -> None:
    path = ROOT / "loom-core/crates/loom-ui/ui/toolkit.slint"
    if not path.is_file():
        failures.append("shared UI: missing toolkit.slint migration target")
        return
    text = path.read_text(encoding="utf-8")
    required = (
        "DocumentChrome",
        "Toolbar",
        "ToolbarGroup",
        "ToolbarButton",
        "ToolbarIconButton",
        "SidebarSurface",
        "InspectorSurface",
        "SectionHeader",
        "ToolkitStatusBar",
        "CanvasSurface",
    )
    for component in required:
        if f"export component {component}" not in text:
            failures.append(f"shared toolkit: missing {component}")
    for component in ("ToolbarButton", "ToolbarIconButton"):
        start = text.find(f"export component {component}")
        end = text.find("\nexport component ", start + 1)
        block = text[start:] if end < 0 else text[start:end]
        for token in ("accessible-role", "accessible-label", "accessible-action-default", "key-pressed(event)"):
            if token not in block:
                failures.append(f"shared toolkit: {component} missing {token}")
    if re.search(r"#[0-9a-fA-F]{6,8}", text):
        failures.append("shared toolkit: hard-coded color outside semantic theme")


audit_mechanical_design_contract()
audit_toolkit_source()

application_texts: dict[str, str] = {}
for app in APPS:
    main = ROOT / f"loom-{app}/crates/loom-{app}-app/src/main.rs"
    text = application_ui_text(app)
    application_texts[app] = text
    if not main.is_file():
        failures.append(f"{app}: missing Rust application entry point")
        continue
    main_text = main.read_text(encoding="utf-8")

    if app in TOOLKIT_MIGRATED_APPS:
        for token, message in (
            ('from "toolkit.slint"', "does not import the desktop toolkit"),
            ("DocumentChrome {", "missing toolkit DocumentChrome"),
            ("Toolbar {", "missing toolkit Toolbar"),
            ("ToolbarGroup {", "missing deterministic toolbar grouping"),
            ("ToolkitStatusBar {", "missing toolkit status bar"),
        ):
            if token not in text:
                failures.append(f"{app}: {message}")
        for forbidden, message in (
            ("AppHeader {", "still uses legacy AppHeader after toolkit migration"),
            ("WorkspaceToolbar {", "still uses legacy WorkspaceToolbar after toolkit migration"),
            ("StatusBar {", "still uses legacy StatusBar after toolkit migration"),
        ):
            if forbidden in text:
                failures.append(f"{app}: {message}")
    else:
        for token, message in (
            ("AppHeader {", "missing shared AppHeader"),
            ("StatusBar {", "missing shared StatusBar"),
            ("compact-layout", "missing compact desktop layout policy"),
        ):
            if token not in text:
                failures.append(f"{app}: {message}")

    for token, message in (
        ("Theme.palette()", "bypasses semantic palette"),
        ("min-width:", "missing minimum responsive width"),
        ("min-height:", "missing minimum responsive height"),
        ("horizontal-stretch", "missing horizontal adaptive layout"),
        ("vertical-stretch", "missing vertical adaptive layout"),
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
for token in (
    "png_dimensions",
    "find_sample",
    "generated-samples",
    "sample_open",
    "one or more theme/size captures are byte-identical",
):
    if token not in native_matrix:
        failures.append(f"native UI matrix: missing evidence check {token}")

functional_matrix = (ROOT / "loom-bootstrap/scripts/native-functional-matrix.py").read_text(encoding="utf-8")
for token in (
    "validate_package",
    "export-md",
    "render-demo",
    "sine",
    "recover",
    "native-functional-matrix.json",
):
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