#!/usr/bin/env python3
from pathlib import Path
from dataclasses import dataclass
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
failures: list[str] = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")
slint_reference = re.compile(r'\bfrom\s+["\']([^"\']+\.slint)["\']')


@dataclass(frozen=True)
class GeometryRect:
    """A logical-pixel rectangle in the shared shell manifest."""

    name: str
    x: float
    y: float
    width: float
    height: float


def rect_overlap(left: GeometryRect, right: GeometryRect) -> tuple[float, float]:
    """Return the positive overlap width/height for two rectangles."""

    width = max(0.0, min(left.x + left.width, right.x + right.width) - max(left.x, right.x))
    height = max(0.0, min(left.y + left.height, right.y + right.height) - max(left.y, right.y))
    return width, height


def geometry_manifest(
    width: int,
    height: int,
    text_scale: float,
    direction: str,
    app_contract: dict,
    responsive: dict,
    geometry_contract: dict,
) -> dict:
    """Build a deterministic shell manifest independent of rendered pixels.

    Optional sidebars collapse before the primary surface can fall below the
    shared minimum. The toolbar fixture models three groups and the largest
    icon-over-label captions at the requested text scale; hosts can use the
    same data to explain why a command moved to overflow.
    """

    title_height = float(geometry_contract["title-height"])
    status_height = float(geometry_contract["status-height"])
    context_toolbar_height = float(geometry_contract["context-toolbar-height"])
    labeled_min = float(geometry_contract["labeled-toolbar-min-height"])
    labeled_max = float(geometry_contract["labeled-toolbar-max-height"])
    primary_min_width = float(geometry_contract["primary-surface-min-width"])
    primary_min_height = float(geometry_contract["primary-surface-min-height"])
    p1 = int(responsive["priority-1-icon-only-below"])
    p2 = int(responsive["priority-2-overflow-below"])
    icon_only = width < p1
    overflow = width < p2
    labeled = width >= p2
    toolbar_height = labeled_min if labeled else context_toolbar_height

    # Keep fixed chrome in source order. Adjacent edges are intentional and
    # do not count as overlap; any positive area overlap is a defect.
    rects = [
        GeometryRect("title", 0.0, 0.0, float(width), title_height),
        GeometryRect("toolbar", 0.0, title_height, float(width), toolbar_height),
    ]
    work_y = title_height + toolbar_height
    work_height = max(0.0, float(height) - work_y - status_height)
    rects.append(GeometryRect("primary-work-surface", 0.0, work_y, float(width), work_height))
    rects.append(GeometryRect("status", 0.0, work_y + work_height, float(width), status_height))

    left = float(app_contract.get("left-sidebar-default", 0))
    right = float(app_contract.get("right-inspector-default", 0))
    primary_width = float(width) - left - right
    # Collapse optional panels in contract order until the primary surface is
    # usable. A manifest records the resulting geometry, not an aspirational
    # preferred layout that would starve the work area.
    if primary_width < primary_min_width and right > 0:
        right = 0.0
        primary_width = float(width) - left
    if primary_width < primary_min_width and left > 0:
        left = 0.0
        primary_width = float(width)

    # Three logical groups: P0 icon controls, P1 captions/icons, P2 overflow.
    # Caption width is intentionally conservative so a 150% text-scale probe
    # catches a host that forgot to move or elide a lower-priority command.
    caption_widths = [
        max(48.0, 10.0 * len(label) * 0.62 * text_scale + 16.0)
        for label in ("Transform", "Insert", "Export")
    ]
    visible_item_widths = [28.0, 28.0, 28.0]
    if labeled:
        visible_item_widths.extend(caption_widths)
    elif not overflow:
        visible_item_widths.extend([28.0, 28.0])
    else:
        # P2 actions are represented by one canonical 28px overflow button.
        visible_item_widths.append(28.0)
    toolbar_content_width = sum(visible_item_widths) + 4.0 * (len(visible_item_widths) - 1) + 16.0

    return {
        "viewport": [width, height],
        "text-scale": text_scale,
        "direction": direction,
        "rects": [rect.__dict__ for rect in rects],
        "optional-panels": {"left": left, "right": right},
        "primary-surface": {"width": primary_width, "height": work_height},
        "toolbar": {
            "height": toolbar_height,
            "lines": 1,
            "groups": 3,
            "content-width": toolbar_content_width,
            "fits": toolbar_content_width <= float(width),
            "overflow": overflow,
            "icon-only": icon_only,
            "labeled": labeled,
            "slot": "labeled" if labeled else "context",
            "slot-range": [labeled_min, labeled_max] if labeled else [context_toolbar_height, context_toolbar_height],
        },
    }


def assert_geometry_manifest(manifest: dict, geometry_contract: dict) -> list[str]:
    """Return actionable geometry defects found in one shell manifest."""

    width, height = manifest["viewport"]
    max_overlap = float(geometry_contract["max-overlap-px"])
    max_clipping = float(geometry_contract["max-clipping-px"])
    issues: list[str] = []
    rects = [GeometryRect(**rect) for rect in manifest["rects"]]
    for index, rect in enumerate(rects):
        if rect.x < -max_clipping or rect.y < -max_clipping:
            issues.append(f"{rect.name} starts outside viewport")
        if rect.x + rect.width > width + max_clipping or rect.y + rect.height > height + max_clipping:
            issues.append(f"{rect.name} clips viewport bounds")
        for other in rects[index + 1 :]:
            overlap_width, overlap_height = rect_overlap(rect, other)
            if overlap_width > max_overlap and overlap_height > max_overlap:
                issues.append(f"{rect.name} overlaps {other.name}")

    toolbar = manifest["toolbar"]
    if toolbar["lines"] > 1:
        issues.append("toolbar wraps to more than one line")
    if not toolbar["fits"]:
        issues.append("toolbar content exceeds viewport width")
    if toolbar["slot"] == "labeled":
        low, high = toolbar["slot-range"]
        if not low <= toolbar["height"] <= high:
            issues.append("labeled toolbar leaves its 48-52px slot")
    else:
        low, high = toolbar["slot-range"]
        if toolbar["height"] != low or toolbar["height"] != high:
            issues.append("context toolbar leaves its 40px slot")

    primary = manifest["primary-surface"]
    if primary["width"] < float(geometry_contract["primary-surface-min-width"]):
        issues.append(f"primary surface starved horizontally ({primary['width']:.1f}px)")
    if primary["height"] < float(geometry_contract["primary-surface-min-height"]):
        issues.append(f"primary surface starved vertically ({primary['height']:.1f}px)")
    return issues


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


def exported_component_count(text: str, name: str) -> int:
    """Count real exported declarations, excluding imports/re-exports."""
    return len(re.findall(rf"(?m)^\s*export component {re.escape(name)}\b", text))


def exported_component_block(text: str, name: str) -> str:
    """Return one exported component's balanced Slint block."""
    marker = re.search(rf"(?m)^\s*export component {re.escape(name)}\b", text)
    if marker is None:
        return ""
    opening = text.find("{", marker.end())
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[marker.start() : index + 1]
    return ""


def slint_object_fields(block: str) -> tuple[dict[str, str], set[str]]:
    """Parse scalar ``key: value`` entries from one balanced Slint object.

    Palette objects contain several assignments per line, so line-oriented
    substring checks are not sufficient. Returning duplicate keys lets the
    audit reject a malformed object instead of silently accepting the last
    occurrence.
    """
    opening = block.find("{")
    if opening < 0 or not block.endswith("}"):
        return {}, set()
    body = block[opening + 1 : -1]
    fields: dict[str, str] = {}
    duplicates: set[str] = set()
    for match in re.finditer(r"([A-Za-z][A-Za-z0-9_-]*)\s*:\s*([^,{}\n]+)", body):
        key = match.group(1).lower()
        value = match.group(2).strip().lower()
        if key in fields:
            duplicates.add(key)
        fields[key] = value
    return fields, duplicates


def slint_palette_fields(theme_block: str) -> tuple[dict[str, str], set[str]]:
    palette_block = next(blocks(theme_block, "palette:"), "")
    return slint_object_fields(palette_block)


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
    theme_source = theme_path.read_text(encoding="utf-8")
    theme = theme_source.lower()
    theme_blocks = {
        "light": next(blocks(theme_source, "export global Theme {"), ""),
        "dark": next(blocks(theme_source, "export global ThemeDark {"), ""),
        "high-contrast": next(blocks(theme_source, "export global ThemeHighContrast {"), ""),
    }

    require(contract.get("format-version") == "1.0.0", "design system: unsupported desktop UI contract version")
    require(tokens.get("format-version") == "2.0.0", "design system: unsupported token version")

    validation = contract.get("validation", {})
    require(
        validation.get("required-viewports") == [[1024, 720], [1280, 800], [1440, 900], [1920, 1200]],
        "design system: required viewport matrix was weakened or reordered",
    )
    require(validation.get("required-text-scales") == [1.0, 1.25, 1.5], "design system: text-scale matrix was weakened")
    require(validation.get("required-directions") == ["ltr", "rtl"], "design system: direction matrix must include LTR and RTL")
    require(validation.get("max-unintentional-overlap-px") == 0, "design system: overlap tolerance must remain zero")
    require(validation.get("max-control-label-clipping-px") == 0, "design system: clipping tolerance must remain zero")
    require(contract.get("toolbar", {}).get("max-groups") == 3, "design system: toolbar group maximum must remain three")
    require(contract.get("controls", {}).get("minimum-pointer-target") == 28, "design system: pointer target must remain 28px")

    responsive = contract.get("responsive", {})
    require(
        responsive.get("priority-1-icon-only-below") == 1180
        and responsive.get("priority-2-overflow-below") == 1320,
        "design system: responsive breakpoints must remain centrally owned at 1180/1320",
    )
    require(
        responsive.get("transition-probes") == [1179, 1180, 1279, 1280, 1319, 1320],
        "design system: responsive transition probe matrix was weakened or reordered",
    )
    require(
        responsive.get("text-scale-probes") == [1.0, 1.5],
        "design system: responsive text-scale probes must include 150%",
    )
    require(
        responsive.get("direction-probes") == ["ltr", "rtl"],
        "design system: responsive direction probes must include LTR and RTL",
    )

    geometry = contract.get("geometry-manifest", {})
    require(geometry.get("version") == "1.0.0", "design system: geometry manifest version is missing or unsupported")
    for key, expected in {
        "primary-surface-min-width": 480,
        "primary-surface-min-height": 320,
        "title-height": 40,
        "context-toolbar-height": 40,
        "labeled-toolbar-min-height": 48,
        "labeled-toolbar-max-height": 52,
        "status-height": 28,
        "max-overlap-px": 0,
        "max-clipping-px": 0,
        "max-toolbar-lines": 1,
    }.items():
        require(geometry.get(key) == expected, f"design system: geometry manifest drift for {key}")

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

    ownership = contract.get("primitive-ownership", {})
    expected_ownership = {
        "canonical-module": "loom-core/crates/loom-ui/ui/toolkit.slint",
        "icon-module": "loom-core/crates/loom-ui/ui/icons.slint",
        "compatibility-module": "loom-core/crates/loom-ui/ui/components.slint",
        "toolbar": "Toolbar",
        "toolbar-group": "ToolbarGroup",
        "toolbar-button": "ToolbarButton",
        "toolbar-icon-button": "ToolbarIconButton",
        "toolbar-overflow-button": "ToolbarOverflowButton",
        "panel-header": "PanelHeader",
        "sidebar": "SidebarSurface",
        "inspector": "InspectorSurface",
        "status": "ToolkitStatusBar",
        "icon": "Icon",
        "title-chrome": "TitleChrome",
        "icon-only-toolbar-item": "IconOnlyToolbarItem",
        "icon-over-label-toolbar-item": "IconOverLabelToolbarItem",
        "sheet-tab-strip": "SheetTabStrip",
        "formula-bar": "FormulaBar",
        "inspector-section": "InspectorSection",
        "property-row": "PropertyRow",
        "field": "Field",
        "segmented-control": "SegmentedControl",
        "status-bar": "StatusBar",
        "overflow": "Overflow",
    }
    for key, expected in expected_ownership.items():
        require(ownership.get(key) == expected, f"design system: primitive ownership drift for {key}")

    expected_wrappers = {
        "ToolButton": "ToolbarButton",
        "IconButton": "ToolbarIconButton",
        "WorkspaceToolbar": "Toolbar",
        "PaneTabs": "TabStrip",
        "Slider": "RangeSlider",
        "Switch": "Toggle",
    }
    require(
        ownership.get("legacy-wrappers") == expected_wrappers,
        "design system: legacy wrapper map was weakened or changed",
    )
    expected_reexports = {
        "Toolbar": "Toolbar",
        "ToolbarGroup": "ToolbarGroup",
        "ToolbarSpacer": "ToolbarSpacer",
        "ToolbarButton": "ToolbarButton",
        "ToolbarIconButton": "ToolbarIconButton",
        "AppleToolbarItem": "AppleToolbarItem",
        "ToolbarOverflowButton": "ToolbarOverflowButton",
        "OverflowMenuButton": "OverflowMenuButton",
        "PanelHeader": "PanelHeader",
        "SidebarSurface": "SidebarSurface",
        "InspectorSurface": "InspectorSurface",
        "SearchField": "SearchField",
        "SegmentedControl": "SegmentedControl",
        "Toggle": "Toggle",
        "RangeSlider": "RangeSlider",
        "TabStrip": "TabStrip",
        "ToolkitStatusBar": "ToolkitStatusBar",
        "TitleChrome": "TitleChrome",
        "IconOnlyToolbarItem": "IconOnlyToolbarItem",
        "IconOverLabelToolbarItem": "IconOverLabelToolbarItem",
        "SheetTabStrip": "SheetTabStrip",
        "FormulaBar": "FormulaBar",
        "InspectorSection": "InspectorSection",
        "PropertyRow": "PropertyRow",
        "Field": "Field",
        "StatusBar": "StatusBar",
        "Overflow": "Overflow",
        "ContextToolbar": "ContextToolbar",
        "LabeledToolbar": "LabeledToolbar",
    }
    require(
        ownership.get("legacy-reexports") == expected_reexports,
        "design system: legacy re-export map was weakened or changed",
    )

    overflow = contract.get("overflow-policy", {})
    expected_overflow = {
        "canonical-button": "ToolbarOverflowButton",
        "compatibility-button": "OverflowMenuButton",
        "icon": "more",
        "accessible-role": "button",
        "accessible-description": "Opens the toolbar overflow menu",
        "keyboard-activation": ["Space", "Enter"],
        "priority-1-mode": "icon-only",
        "priority-2-mode": "overflow",
        "priority-1-breakpoint": 1180,
        "priority-2-breakpoint": 1320,
        "primary-workspace-min-width": 480,
        "preserve-primary-workspace": True,
    }
    for key, expected in expected_overflow.items():
        require(overflow.get(key) == expected, f"design system: overflow policy drift for {key}")
    require(
        contract.get("toolbar", {}).get("priority-1-icon-only-below")
        == overflow.get("priority-1-breakpoint"),
        "design system: P1 overflow breakpoint must have one owner",
    )
    require(
        contract.get("toolbar", {}).get("priority-2-overflow-below")
        == overflow.get("priority-2-breakpoint"),
        "design system: P2 overflow breakpoint must have one owner",
    )

    for theme_name in ("light", "dark", "high-contrast"):
        contract_palette = contract.get("palette", {}).get(theme_name, {})
        token_palette = tokens.get("palette", {}).get(theme_name, {})
        require(contract_palette == token_palette, f"design system: {theme_name} palette contract/token drift")
        runtime_palette, duplicate_keys = slint_palette_fields(theme_blocks[theme_name])
        for key in sorted(duplicate_keys):
            require(False, f"runtime theme: duplicate {theme_name} palette key={key}")
        for key, value in contract_palette.items():
            require(
                runtime_palette.get(key.lower()) == str(value).lower(),
                f"runtime theme: missing {theme_name} {key}={value}",
            )

    # runtime name -> (token key, contract section, contract key)
    metric_pairs = {
        "control-height": ("control-height", "controls", "standard-height"),
        "compact-control-height": ("compact-control-height", "controls", "compact-height"),
        "toolbar-height": ("toolbar-height", "chrome", "toolbar-height"),
        "context-toolbar-height": ("context-toolbar-height", "chrome", "context-toolbar-height"),
        "labeled-toolbar-min-height": ("labeled-toolbar-min-height", "chrome", "labeled-toolbar-min-height"),
        "labeled-toolbar-max-height": ("labeled-toolbar-max-height", "chrome", "labeled-toolbar-max-height"),
        "header-height": ("header-height", "chrome", "title-height"),
        "panel-header-height": ("panel-header-height", "chrome", "panel-header-height"),
        "status-height": ("status-height", "chrome", "status-height"),
        "sheet-tab-height": ("sheet-tab-height", "chrome", "sheet-tab-height"),
        "formula-bar-height": ("formula-bar-height", "chrome", "formula-bar-height"),
        "icon-size": ("icon-standard", "controls", "icon-standard"),
        "icon-small": ("icon-small", "controls", "icon-small"),
    }
    token_metrics = tokens.get("metrics", {})
    for runtime_name, (token_name, section, contract_name) in metric_pairs.items():
        value = contract.get(section, {}).get(contract_name)
        require(token_metrics.get(token_name) == value, f"design system: metric drift for {token_name}")
        require(f"{runtime_name}: {value}px" in theme, f"runtime theme: missing {runtime_name}: {value}px")


def audit_geometry_manifest() -> None:
    """Exercise shared geometry at every boundary, scale, and direction.

    This is intentionally a numeric assertion over the contract rather than a
    PNG comparison. It catches regressions that can leave a stable screenshot
    hash while controls overlap, labels clip, a toolbar wraps, or the primary
    work surface becomes unusably narrow.
    """

    contract_path = ROOT / "loom-design-bible/contracts/desktop-ui.toml"
    if not contract_path.is_file():
        return
    with contract_path.open("rb") as handle:
        contract = tomllib.load(handle)

    responsive = contract.get("responsive", {})
    geometry = contract.get("geometry-manifest", {})
    probes = responsive.get("transition-probes", [])
    required_probes = [1179, 1180, 1279, 1280, 1319, 1320]
    require(probes == required_probes, "geometry manifest: transition probes must cover both sides of 1180/1320")

    # Assert the policy at each exact boundary.  1279/1280 are intentional
    # stability probes: no undocumented third breakpoint may appear there.
    expected = {
        1179: {"icon-only": True, "overflow": True, "labeled": False},
        1180: {"icon-only": False, "overflow": True, "labeled": False},
        1279: {"icon-only": False, "overflow": True, "labeled": False},
        1280: {"icon-only": False, "overflow": True, "labeled": False},
        1319: {"icon-only": False, "overflow": True, "labeled": False},
        1320: {"icon-only": False, "overflow": False, "labeled": True},
    }
    for width in required_probes:
        manifest = geometry_manifest(
            width,
            800,
            1.0,
            "ltr",
            {"left-sidebar-default": 0, "right-inspector-default": 0},
            responsive,
            geometry,
        )
        for key, value in expected[width].items():
            require(
                manifest["toolbar"][key] == value,
                f"geometry manifest: {width}px responsive state drift for {key}",
            )

    viewports = [(width, 800) for width in required_probes]
    viewports.extend(tuple(pair) for pair in contract.get("validation", {}).get("required-viewports", []))
    seen_viewports: set[tuple[int, int]] = set()
    for app in APPS:
        app_contract = contract.get("app", {}).get(app, {})
        for width, height in viewports:
            for text_scale in responsive.get("text-scale-probes", [1.0, 1.5]):
                for direction in responsive.get("direction-probes", ["ltr", "rtl"]):
                    key = (int(width), int(height), float(text_scale), direction, app)
                    if key in seen_viewports:
                        continue
                    seen_viewports.add(key)
                    manifest = geometry_manifest(
                        int(width),
                        int(height),
                        float(text_scale),
                        direction,
                        app_contract,
                        responsive,
                        geometry,
                    )
                    issues = assert_geometry_manifest(manifest, geometry)
                    for issue in issues:
                        require(
                            False,
                            f"geometry manifest: {app} {width}x{height} scale={text_scale} direction={direction}: {issue}",
                        )


def audit_toolkit() -> None:
    toolkit = read("loom-core/crates/loom-ui/ui/toolkit.slint")
    required_components = (
        "DocumentChrome", "TitleChrome", "Toolbar", "ContextToolbar", "LabeledToolbar", "ToolbarGroup", "ToolbarSpacer",
        "ToolbarButton", "ToolbarIconButton", "IconOnlyToolbarItem", "ToolbarOverflowButton", "Overflow",
        "AppleToolbarItem", "IconOverLabelToolbarItem", "PanelHeader", "SidebarSurface",
        "InspectorSurface", "SectionHeader", "ToolkitStatusBar", "CanvasSurface",
        "ContentSurface", "TextField", "SearchField", "SegmentedControl",
        "Toggle", "RangeSlider", "Field", "FormulaBar", "InspectorSection", "PropertyRow", "TabStrip", "SheetTabStrip",
        "StatusBar",
    )
    for component in required_components:
        require(f"export component {component}" in toolkit, f"shared toolkit: missing {component}")

    for component in ("ToolbarButton", "ToolbarIconButton", "Toggle"):
        block = exported_component_block(toolkit, component)
        for token in ("accessible-role", "accessible-label", "accessible-action-default", "key-pressed(event)"):
            require(token in block, f"shared toolkit: {component} missing {token}")

    for component in ("TextField", "Field", "FormulaBar", "SearchField", "SegmentedControl", "RangeSlider"):
        block = exported_component_block(toolkit, component)
        require("accessible-role" in block and "accessible-label" in block, f"shared toolkit: {component} lacks accessibility metadata")

    require(not re.search(r"#[0-9a-fA-F]{6,8}", toolkit), "shared toolkit: hard-coded palette color")

    toolbar = exported_component_block(toolkit, "Toolbar")
    require(
        "labeled-slot" in toolbar and "ResponsivePolicy.toolbar-height" in toolbar,
        "shared toolkit: Toolbar must explicitly select context vs labeled slot",
    )

    context = exported_component_block(toolkit, "ContextToolbar")
    require(
        "labeled-slot: false" in context
        and "export component ContextToolbar" in context,
        "shared toolkit: ContextToolbar must opt into the 40px context slot",
    )
    labeled_toolbar = exported_component_block(toolkit, "LabeledToolbar")
    require(
        "labeled-slot: true" in labeled_toolbar
        and "export component LabeledToolbar" in labeled_toolbar,
        "shared toolkit: LabeledToolbar must opt into the 48–52px slot",
    )

    group = exported_component_block(toolkit, "ToolbarGroup")
    require(
        "labeled-slot" in group
        and "ResponsivePolicy.labeled-toolbar-min-height" in group
        and "ResponsivePolicy.context-toolbar-height" in group,
        "shared toolkit: ToolbarGroup must expose explicit slot geometry",
    )

    item = exported_component_block(toolkit, "IconOverLabelToolbarItem")
    require(
        "ResponsivePolicy.labeled-toolbar-min-height" in item
        and "ResponsivePolicy.labeled-toolbar-max-height" in item,
        "shared toolkit: IconOverLabelToolbarItem must reserve a 48–52px labeled-control slot",
    )
    require(
        "font-size: 10px;" in item,
        "shared toolkit: IconOverLabelToolbarItem labels must use 10px text",
    )
    require(
        "size: 18px;" in item,
        "shared toolkit: IconOverLabelToolbarItem icons must use the labeled-toolbar size",
    )
    require(
        "padding-top: 4px;" in item and "padding-bottom: 4px;" in item,
        "shared toolkit: IconOverLabelToolbarItem must preserve 4px vertical padding",
    )
    require(
        "overflow: elide;" in item,
        "shared toolkit: IconOverLabelToolbarItem labels must allow elision within their slot",
    )

    icon_only = exported_component_block(toolkit, "IconOnlyToolbarItem")
    require(
        "width: Theme.tokens.metrics.control-height;" in icon_only
        and "height: Theme.tokens.metrics.control-height;" in icon_only,
        "shared toolkit: IconOnlyToolbarItem must retain compact token geometry",
    )
    icon_over_label = exported_component_block(toolkit, "IconOverLabelToolbarItem")
    require(
        "requires-labeled-slot" in icon_over_label
        and "ResponsivePolicy.labeled-toolbar-min-height" in icon_over_label,
        "shared toolkit: IconOverLabelToolbarItem must declare its labeled slot",
    )

    compact = exported_component_block(toolkit, "ToolbarIconButton")
    require(
        "width: Theme.tokens.metrics.control-height;" in compact
        and "height: Theme.tokens.metrics.control-height;" in compact
        and "size: 16px;" in compact,
        "shared toolkit: ToolbarIconButton must retain tokenized 28px/16px compact geometry",
    )

    overflow = exported_component_block(toolkit, "ToolbarOverflowButton")
    for token in (
        "accessible-role: button",
        "accessible-label:",
        "accessible-description: \"Opens the toolbar overflow menu\"",
        "accessible-action-default",
        "key-pressed(event)",
        "Key.Space",
        "Key.Return",
        'icon: "more"',
        "Theme.tokens.metrics.control-height",
    ):
        require(token in overflow, f"shared toolkit: ToolbarOverflowButton missing {token}")

    for component in ("PanelHeader", "ToolkitStatusBar"):
        block = exported_component_block(toolkit, component)
        require("Theme.tokens" in block and "Theme.palette()" in block, f"shared toolkit: {component} is not tokenized")
    for component in ("SidebarSurface", "InspectorSurface"):
        block = exported_component_block(toolkit, component)
        # Panel surfaces use fixed min/max widths from the machine contract;
        # their colors and nested header remain semantic/tokenized.
        require("Theme.palette()" in block and "PanelHeader" in block, f"shared toolkit: {component} is not tokenized")


def audit_shared_primitive_ownership() -> None:
    toolkit = read("loom-core/crates/loom-ui/ui/toolkit.slint")
    icons = read("loom-core/crates/loom-ui/ui/icons.slint")
    components = read("loom-core/crates/loom-ui/ui/components.slint")

    canonical = (
        "TitleChrome",
        "Toolbar",
        "ContextToolbar",
        "LabeledToolbar",
        "ToolbarGroup",
        "ToolbarButton",
        "ToolbarIconButton",
        "IconOnlyToolbarItem",
        "IconOverLabelToolbarItem",
        "ToolbarOverflowButton",
        "Overflow",
        "PanelHeader",
        "SidebarSurface",
        "InspectorSurface",
        "ToolkitStatusBar",
        "StatusBar",
        "FormulaBar",
        "InspectorSection",
        "PropertyRow",
        "Field",
        "SegmentedControl",
        "SheetTabStrip",
    )
    for name in canonical:
        require(exported_component_count(toolkit, name) == 1, f"shared ownership: toolkit must define exactly one {name}")
        require(
            exported_component_count(components, name) == 0,
            f"shared ownership: components.slint must not define a second {name}",
        )

    require(exported_component_count(icons, "Icon") == 1, "shared ownership: icons.slint must define exactly one Icon")
    require(exported_component_count(toolkit, "Icon") == 0, "shared ownership: toolkit must not define Icon")
    require(exported_component_count(components, "Icon") == 0, "shared ownership: components.slint must not define Icon")

    wrappers = {
        "ToolButton": "ToolbarButton",
        "IconButton": "ToolbarIconButton",
        "WorkspaceToolbar": "Toolbar",
        "PaneTabs": "TabStrip",
        "Slider": "RangeSlider",
        "Switch": "Toggle",
    }
    for legacy, target in wrappers.items():
        require(
            re.search(rf"(?m)^\s*export component {legacy}\s+inherits\s+{target}\b", components)
            is not None,
            f"shared ownership: {legacy} must wrap {target}",
        )

    reexport_start = components.find("// Re-export the canonical shared primitives")
    reexports = components[reexport_start:] if reexport_start >= 0 else ""
    require('} from "toolkit.slint";' in reexports, "shared ownership: compatibility module must re-export toolkit primitives")
    for name in (
        "Toolbar",
        "ToolbarGroup",
        "ToolbarSpacer",
        "ToolbarButton",
        "ToolbarIconButton",
        "ToolbarOverflowButton",
        "OverflowMenuButton",
        "PanelHeader",
        "SidebarSurface",
        "InspectorSurface",
        "SearchField",
        "SegmentedControl",
        "Toggle",
        "RangeSlider",
        "TabStrip",
        "ToolkitStatusBar",
        "TitleChrome",
        "IconOnlyToolbarItem",
        "IconOverLabelToolbarItem",
        "SheetTabStrip",
        "FormulaBar",
        "InspectorSection",
        "PropertyRow",
        "Field",
        "StatusBar",
        "Overflow",
        "ContextToolbar",
        "LabeledToolbar",
    ):
        require(re.search(rf"\b{re.escape(name)}\b", reexports) is not None, f"shared ownership: missing {name} re-export")

    shared = "\n".join((toolkit, icons, components)).lower()
    require(
        not re.search(r"traffic[\s_-]*lights?|macos[\s_-]*(?:traffic|close|minimize|zoom)|window[\s_-]*control", shared),
        "shared ownership: simulated macOS traffic-light/window controls remain",
    )

    segmented = exported_component_block(toolkit, "SegmentedControl")
    require(
        re.search(r"(?m)^\s*(?:width|min-width|preferred-width|max-width):[^;\n]*root\.width", segmented)
        is None,
        "shared ownership: SegmentedControl segment sizing must not depend on parent width",
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
audit_geometry_manifest()
audit_toolkit()
audit_icons()
audit_shared_primitive_ownership()
app_text = {app: audit_app(app) for app in APPS}

for app, required in {
    "photo": ("canvas-pan := TouchArea", "pressed-x", "viewport-pan-x", "key-pressed(event)"),
    "motion": ("drag := TouchArea", "pressed-x", "transform-changed", "key-pressed(event)"),
    "video": ("ruler-scrub := TouchArea", "playhead-seconds", "root.seek(", "key-pressed(event)"),
}.items():
    for token in required:
        require(token in app_text[app], f"{app}: missing direct-manipulation contract {token}")

legacy = read("loom-core/crates/loom-ui/ui/components.slint")
for component in ("WorkspaceToolbar", "PaneTabs", "CanvasBackdrop", "TransportButton"):
    require(f"export component {component}" in legacy, f"legacy compatibility UI: missing {component}")
for component in ("SidebarSurface", "InspectorSurface"):
    require(
        re.search(rf"\b{re.escape(component)}\b", legacy[legacy.find("// Re-export the canonical shared primitives") :])
        is not None,
        f"legacy compatibility UI: missing {component} re-export",
    )

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
