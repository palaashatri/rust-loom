#!/usr/bin/env python3
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
failures: list[str] = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")


def require(condition: bool, message: str) -> None:
    if not condition:
        failures.append(message)


def read(path: str) -> str:
    target = ROOT / path
    if not target.is_file():
        failures.append(f"missing required file: {path}")
        return ""
    return target.read_text(encoding="utf-8")


def blocks(text: str, marker: str):
    cursor = 0
    while True:
        start = text.find(marker, cursor)
        if start < 0:
            return
        opening = text.find("{", start)
        if opening < 0:
            return
        depth = 0
        for idx in range(opening, len(text)):
            if text[idx] == "{":
                depth += 1
            elif text[idx] == "}":
                depth -= 1
                if depth == 0:
                    yield text[start:idx + 1]
                    cursor = idx + 1
                    break
        else:
            failures.append(f"unterminated Slint block: {marker}")
            return


def audit_design_contract() -> None:
    contract_path = ROOT / "loom-design-bible/contracts/desktop-ui.toml"
    tokens_path = ROOT / "loom-design-bible/tokens/loom.toml"
    standard_path = ROOT / "loom-design-bible/MECHANICAL_DESIGN_STANDARD.md"
    theme_path = ROOT / "loom-core/crates/loom-ui/ui/theme.slint"
    for path in (contract_path, tokens_path, standard_path, theme_path):
        require(path.is_file(), f"design system: missing {path.relative_to(ROOT)}")
    if not all(path.is_file() for path in (contract_path, tokens_path, theme_path)):
        return

    with contract_path.open("rb") as handle:
        contract = tomllib.load(handle)
    with tokens_path.open("rb") as handle:
        tokens = tomllib.load(handle)
    theme = theme_path.read_text(encoding="utf-8").lower()

    require(contract.get("format-version") == "1.0.0", "design system: unsupported desktop UI contract version")
    require(tokens.get("format-version") == "2.0.0", "design system: unsupported token version")

    validation = contract.get("validation", {})
    require(
        validation.get("required-viewports") == [[1024, 720], [1280, 800], [1440, 900], [1920, 1200]],
        "design system: required viewport matrix was weakened or reordered",
    )
    require(validation.get("required-text-scales") == [1.0, 1.25, 1.5], "design system: text-scale matrix was weakened")
    require(validation.get("max-unintentional-overlap-px") == 0, "design system: overlap tolerance must remain zero")
    require(validation.get("max-control-label-clipping-px") == 0, "design system: clipping tolerance must remain zero")
    require(contract.get("toolbar", {}).get("max-groups") == 3, "design system: toolbar group maximum must remain three")
    require(contract.get("controls", {}).get("minimum-pointer-target") == 28, "design system: pointer target must remain 28px")

    for theme_name in ("light", "dark", "high-contrast"):
        cp = contract.get("palette", {}).get(theme_name, {})
        tp = tokens.get("palette", {}).get(theme_name, {})
        require(cp == tp, f"design system: {theme_name} palette contract/token drift")
        for key, value in cp.items():
            require(f"{key}: {str(value).lower()}" in theme, f"runtime theme: missing {theme_name} {key}={value}")

    metric_pairs = {
        "control-height": ("controls", "standard-height"),
        "compact-control-height": ("controls", "compact-height"),
        "toolbar-height": ("chrome", "toolbar-height"),
        "header-height": ("chrome", "title-height"),
        "panel-header-height": ("chrome", "panel-header-height"),
        "status-height": ("chrome", "status-height"),
    }
    for runtime_name, (section, name) in metric_pairs.items():
        value = contract.get(section, {}).get(name)
        require(tokens.get("metrics", {}).get(runtime_name) == value, f"design system: metric drift for {runtime_name}")
        require(f"{runtime_name}: {value}px" in theme, f"runtime theme: missing {runtime_name}: {value}px")


def audit_toolkit() -> None:
    toolkit = read("loom-core/crates/loom-ui/ui/toolkit.slint")
    for component in (
        "DocumentChrome", "Toolbar", "ToolbarGroup", "ToolbarSpacer", "ToolbarButton",
        "ToolbarIconButton", "PanelHeader", "SidebarSurface", "InspectorSurface",
        "SectionHeader", "ToolkitStatusBar", "CanvasSurface", "ContentSurface",
    ):
        require(f"export component {component}" in toolkit, f"shared toolkit: missing {component}")

    for component in ("ToolbarButton", "ToolbarIconButton"):
        marker = f"export component {component}"
        start = toolkit.find(marker)
        end = toolkit.find("\nexport component ", start + len(marker))
        block = toolkit[start:] if end < 0 else toolkit[start:end]
        for token in ("accessible-role", "accessible-label", "accessible-action-default", "key-pressed(event)"):
            require(token in block, f"shared toolkit: {component} missing {token}")

    require(not re.search(r"#[0-9a-fA-F]{6,8}", toolkit), "shared toolkit: hard-coded palette color")


def audit_app(app: str) -> str:
    ui_path = f"loom-{app}/crates/loom-{app}-app/ui/app.slint"
    main_path = f"loom-{app}/crates/loom-{app}-app/src/main.rs"
    text = read(ui_path)
    main = read(main_path)
    migrated = 'from "toolkit.slint"' in text

    if migrated:
        for token, description in (
            ("DocumentChrome {", "DocumentChrome"),
            ("Toolbar {", "Toolbar"),
            ("ToolbarGroup {", "ToolbarGroup"),
            ("ToolkitStatusBar {", "ToolkitStatusBar"),
        ):
            require(token in text, f"{app}: toolkit migration missing {description}")
        for token, description in (
            ("AppHeader {", "legacy AppHeader"),
            ("WorkspaceToolbar {", "legacy WorkspaceToolbar"),
            ("StatusBar {", "legacy StatusBar"),
        ):
            require(token not in text, f"{app}: toolkit migration still contains {description}")
    else:
        require("AppHeader {" in text, f"{app}: missing legacy AppHeader before migration")
        require("WorkspaceToolbar {" in text, f"{app}: missing legacy WorkspaceToolbar before migration")
        require("StatusBar {" in text, f"{app}: missing legacy StatusBar before migration")
        require("compact-layout" in text, f"{app}: missing compact-layout policy before migration")

    for token, description in (
        ("Theme.palette()", "semantic palette use"),
        ("min-width:", "minimum responsive width"),
        ("min-height:", "minimum responsive height"),
        ("horizontal-stretch", "horizontal adaptive layout"),
        ("vertical-stretch", "vertical adaptive layout"),
    ):
        require(token in text, f"{app}: missing {description}")

    require(not emoji.search(text), f"{app}: emoji/icon-font glyphs remain")
    require(not re.search(r"#[0-9a-fA-F]{6,8}", text), f"{app}: hard-coded color outside theme")
    lowered = text.lower()
    for token in ("coming soon", "placeholder ui", "placeholder control", "fake progress", "model preview"):
        require(token not in lowered, f"{app}: prototype/fabricated-state language remains: {token}")

    require("!other.starts_with('-') && args.open.is_none()" in main, f"{app}: positional document opening unsupported")
    for slider in blocks(text, "Slider {"):
        require("label:" in slider, f"{app}: slider lacks accessibility label")
    return text


audit_design_contract()
audit_toolkit()
app_text = {app: audit_app(app) for app in APPS}

for app, required in {
    "photo": ("canvas-pan := TouchArea", "pressed-x", "viewport-pan-x", "key-pressed(event)"),
    "motion": ("drag := TouchArea", "pressed-x", "transform-changed", "key-pressed(event)"),
    "video": ("ruler-scrub := TouchArea", "playhead-seconds", "root.seek(", "key-pressed(event)"),
}.items():
    for token in required:
        require(token in app_text[app], f"{app}: missing direct-manipulation contract {token}")

legacy = read("loom-core/crates/loom-ui/ui/components.slint")
for component in ("WorkspaceToolbar", "SidebarSurface", "InspectorSurface", "PaneTabs", "CanvasBackdrop", "TransportButton"):
    require(f"export component {component}" in legacy, f"legacy compatibility UI: missing {component}")

native = read(".github/workflows/cross-platform.yml")
for token in ("windows-2025", "macos-15", "macos-15-intel", "native-ui-matrix.py", "1024x720", "1440x900", "1920x1200", "upload-artifact"):
    require(token in native, f"native UI validation: missing {token}")

native_matrix = read("loom-bootstrap/scripts/native-ui-matrix.py")
for token in ("png_dimensions", "find_sample", "generated-samples", "sample_open", "one or more theme/size captures are byte-identical"):
    require(token in native_matrix, f"native UI matrix: missing evidence check {token}")

functional_matrix = read("loom-bootstrap/scripts/native-functional-matrix.py")
for token in ("validate_package", "export-md", "render-demo", "sine", "recover", "native-functional-matrix.json"):
    require(token in functional_matrix, f"native functional matrix: missing journey evidence {token}")

packaging = read("loom-bootstrap/packaging/release.py")
for token in ("DOCUMENT_TYPES", "MimeType=", "RegistryValue", "CFBundleDocumentTypes"):
    require(token in packaging, f"native packaging: missing {token}")

if failures:
    print("Loom UI productisation audit failed:")
    for failure in failures:
        print(f"- {failure}")
    sys.exit(1)

migrated = [app for app in APPS if 'from "toolkit.slint"' in app_text[app]]
print(f"Loom UI productisation audit passed; toolkit-migrated apps: {', '.join(migrated) or 'none'}")