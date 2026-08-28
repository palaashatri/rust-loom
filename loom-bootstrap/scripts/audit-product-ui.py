#!/usr/bin/env python3
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
failures: list[str] = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")
slint_reference = re.compile(r'\bfrom\s+["\']([^"\']+\.slint)["\']')


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


def active_application_ui(app: str) -> str:
    """Return only app-local Slint modules reachable from ui/app.slint.

    Several applications intentionally keep app.slint as a stable export shim.
    Auditing just that file silently ignores the shipping workspace. Shared
    include-path modules (toolkit/theme/components) are validated separately and
    are not folded into app text, so legacy compatibility code cannot cause
    false positives in a migrated app.
    """
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
        try:
            resolved.relative_to(ui_dir.resolve())
        except ValueError:
            return
        visited.add(resolved)
        text = path.read_text(encoding="utf-8")
        chunks.append(text)
        for reference in slint_reference.findall(text):
            candidate = (path.parent / reference).resolve()
            if candidate.is_file():
                visit(candidate)

    visit(entry)
    return "\n".join(chunks)


def component_used(text: str, name: str) -> bool:
    # Components may be conditionally instantiated in Slint (`if expr :
    # Component { ... }`). Treat that as a real use while keeping the match
    # anchored to a component declaration line so prose/comments do not count.
    return (
        re.search(
            rf"(?m)^\s*(?:if\s+[^\n{{}}]+:\s*)?{re.escape(name)}\s*\{{",
            text,
        )
        is not None
    )


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

    labeled_toolbar = contract.get("component", {}).get("labeled-toolbar-item", {})
    require(
        labeled_toolbar.get("min-height") == 48 and labeled_toolbar.get("max-height") == 52,
        "design system: labeled toolbar item height must remain in the 48–52px range",
    )
    require(
        labeled_toolbar.get("label-size") == 10,
        "design system: labeled toolbar item text must remain 10px",
    )
    require(
        labeled_toolbar.get("allow-label-elision") is True,
        "design system: labeled toolbar item labels must explicitly allow elision",
    )

    compact_icon_button = contract.get("component", {}).get("icon-button", {})
    require(
        compact_icon_button.get("width") == 28
        and compact_icon_button.get("height") == 28
        and compact_icon_button.get("icon") == 16,
        "design system: compact icon-button geometry must remain 28px with a 16px icon",
    )

    for theme_name in ("light", "dark", "high-contrast"):
        contract_palette = contract.get("palette", {}).get(theme_name, {})
        token_palette = tokens.get("palette", {}).get(theme_name, {})
        require(contract_palette == token_palette, f"design system: {theme_name} palette contract/token drift")
        for key, value in contract_palette.items():
            require(f"{key}: {str(value).lower()}" in theme, f"runtime theme: missing {theme_name} {key}={value}")

    # runtime name -> (token key, contract section, contract key)
    metric_pairs = {
        "control-height": ("control-height", "controls", "standard-height"),
        "compact-control-height": ("compact-control-height", "controls", "compact-height"),
        "toolbar-height": ("toolbar-height", "chrome", "toolbar-height"),
        "header-height": ("header-height", "chrome", "title-height"),
        "panel-header-height": ("panel-header-height", "chrome", "panel-header-height"),
        "status-height": ("status-height", "chrome", "status-height"),
        "icon-size": ("icon-standard", "controls", "icon-standard"),
        "icon-small": ("icon-small", "controls", "icon-small"),
    }
    token_metrics = tokens.get("metrics", {})
    for runtime_name, (token_name, section, contract_name) in metric_pairs.items():
        value = contract.get(section, {}).get(contract_name)
        require(token_metrics.get(token_name) == value, f"design system: metric drift for {token_name}")
        require(f"{runtime_name}: {value}px" in theme, f"runtime theme: missing {runtime_name}: {value}px")


def audit_toolkit() -> None:
    toolkit = read("loom-core/crates/loom-ui/ui/toolkit.slint")
    required_components = (
        "DocumentChrome", "Toolbar", "ToolbarGroup", "ToolbarSpacer",
        "ToolbarButton", "ToolbarIconButton", "PanelHeader", "SidebarSurface",
        "InspectorSurface", "SectionHeader", "ToolkitStatusBar", "CanvasSurface",
        "ContentSurface", "TextField", "SearchField", "SegmentedControl",
        "Toggle", "RangeSlider", "PropertyRow", "TabStrip",
    )
    for component in required_components:
        require(f"export component {component}" in toolkit, f"shared toolkit: missing {component}")

    for component in ("ToolbarButton", "ToolbarIconButton", "Toggle"):
        marker = f"export component {component}"
        start = toolkit.find(marker)
        end = toolkit.find("\nexport component ", start + len(marker))
        block = toolkit[start:] if end < 0 else toolkit[start:end]
        for token in ("accessible-role", "accessible-label", "accessible-action-default", "key-pressed(event)"):
            require(token in block, f"shared toolkit: {component} missing {token}")

    for component in ("TextField", "SearchField", "SegmentedControl", "RangeSlider"):
        marker = f"export component {component}"
        start = toolkit.find(marker)
        end = toolkit.find("\nexport component ", start + len(marker))
        block = toolkit[start:] if end < 0 else toolkit[start:end]
        require("accessible-role" in block and "accessible-label" in block, f"shared toolkit: {component} lacks accessibility metadata")

    require(not re.search(r"#[0-9a-fA-F]{6,8}", toolkit), "shared toolkit: hard-coded palette color")

    toolbar_start = toolkit.find("export component Toolbar")
    toolbar_end = toolkit.find("\nexport component ", toolbar_start + 1)
    toolbar = toolkit[toolbar_start:] if toolbar_end < 0 else toolkit[toolbar_start:toolbar_end]
    require(
        "min-height: 48px;" in toolbar and "max-height: 52px;" in toolbar,
        "shared toolkit: Toolbar must reserve a 48–52px labeled-control slot",
    )

    group_start = toolkit.find("export component ToolbarGroup")
    group_end = toolkit.find("\nexport component ", group_start + 1)
    group = toolkit[group_start:] if group_end < 0 else toolkit[group_start:group_end]
    require(
        "min-height: 48px;" in group and "max-height: 52px;" in group,
        "shared toolkit: ToolbarGroup must reserve a 48–52px labeled-control slot",
    )

    item_start = toolkit.find("export component AppleToolbarItem")
    item_end = toolkit.find("\nexport component ", item_start + 1)
    item = toolkit[item_start:] if item_end < 0 else toolkit[item_start:item_end]
    require(
        "min-height: 48px;" in item and "max-height: 52px;" in item,
        "shared toolkit: AppleToolbarItem must reserve a 48–52px labeled-control slot",
    )
    require(
        "font-size: 10px;" in item,
        "shared toolkit: AppleToolbarItem labels must use 10px text",
    )
    require(
        "size: 18px;" in item,
        "shared toolkit: AppleToolbarItem icons must use the labeled-toolbar size",
    )
    require(
        "padding-top: 4px;" in item and "padding-bottom: 4px;" in item,
        "shared toolkit: AppleToolbarItem must preserve 4px vertical padding",
    )
    require(
        "overflow: elide;" in item,
        "shared toolkit: AppleToolbarItem labels must allow elision within their slot",
    )

    compact_start = toolkit.find("export component ToolbarIconButton")
    compact_end = toolkit.find("\nexport component ", compact_start + 1)
    compact = toolkit[compact_start:] if compact_end < 0 else toolkit[compact_start:compact_end]
    require(
        "width: Theme.tokens.metrics.control-height;" in compact
        and "height: Theme.tokens.metrics.control-height;" in compact
        and "size: 16px;" in compact,
        "shared toolkit: ToolbarIconButton must retain tokenized 28px/16px compact geometry",
    )


def audit_icons() -> None:
    icons = read("loom-core/crates/loom-ui/ui/icons.slint")
    path = next(blocks(icons, "Path {"), "")
    require(
        "width: parent.width;" in path and "height: parent.height;" in path,
        "shared icons: Icon Path must scale to the requested parent dimensions",
    )
    require(
        not re.search(r"(?m)^\s*(?:width|height):\s*20px;", path),
        "shared icons: Icon Path must not use a fixed 20px viewport",
    )


def audit_app(app: str) -> str:
    main_path = f"loom-{app}/crates/loom-{app}-app/src/main.rs"
    text = active_application_ui(app)
    main = read(main_path)
    migrated = 'from "toolkit.slint"' in text

    if migrated:
        for name in ("DocumentChrome", "Toolbar", "ToolbarGroup", "ToolkitStatusBar"):
            require(component_used(text, name), f"{app}: toolkit migration missing {name}")
        for name in ("AppHeader", "WorkspaceToolbar", "StatusBar"):
            require(not component_used(text, name), f"{app}: toolkit migration still contains legacy {name}")
    else:
        for name in ("AppHeader", "WorkspaceToolbar", "StatusBar"):
            require(component_used(text, name), f"{app}: missing legacy {name} before migration")
        require("compact-layout" in text, f"{app}: missing compact-layout policy before migration")

    require("Theme.palette()" in text, f"{app}: missing semantic palette use")
    require("min-width:" in text, f"{app}: missing minimum responsive width")
    require("min-height:" in text, f"{app}: missing minimum responsive height")
    require("horizontal-stretch" in text or "CanvasSurface" in text, f"{app}: missing horizontal adaptive layout")
    require("vertical-stretch" in text or "CanvasSurface" in text, f"{app}: missing vertical adaptive layout")

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
audit_icons()
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
