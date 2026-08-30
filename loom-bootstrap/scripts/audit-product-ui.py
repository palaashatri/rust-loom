#!/usr/bin/env python3
from pathlib import Path
from dataclasses import dataclass
import json
import re
import sys
import tomllib
import hashlib

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
failures: list[str] = []
emoji = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27BF]")
slint_reference = re.compile(r'\bfrom\s+["\']([^"\']+\.slint)["\']')
GEOMETRY_MANIFEST_PATH = ROOT / "loom-design-bible/contracts/geometry-manifest.toml"
SHARED_UI_DIR = ROOT / "loom-core/crates/loom-ui/ui"


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


def balanced_slint_block(source: str, opening: int) -> str:
    """Return the balanced Slint object beginning at an opening brace."""

    if opening < 0 or opening >= len(source) or source[opening] != "{":
        return ""
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening : index + 1]
    return ""


def slint_instance_blocks(source: str, name: str) -> list[str]:
    """Extract instantiated (not declaration/import) Slint component blocks."""

    pattern = re.compile(
        rf"(?m)^\s*(?:(?:if|for)[^\n{{}}]*:\s*)?{re.escape(name)}\s*\{{"
    )
    instances: list[str] = []
    for match in pattern.finditer(source):
        opening = source.find("{", match.start(), match.end())
        block = balanced_slint_block(source, opening)
        if block:
            instances.append(source[match.start() : opening] + block)
    return instances


def slint_named_instance_blocks(source: str, names: tuple[str, ...]) -> list[tuple[int, str, str]]:
    """Return reachable named instances with source offsets and blocks."""

    matches: list[tuple[int, str, str]] = []
    for name in names:
        pattern = re.compile(
            rf"(?m)^\s*(?:(?:if|for)[^\n{{}}]*:\s*)?(?:\w+\s*:=\s*)?{re.escape(name)}\s*\{{"
        )
        for match in pattern.finditer(source):
            opening = source.find("{", match.start(), match.end())
            block = balanced_slint_block(source, opening)
            if block:
                matches.append((match.start(), name, source[match.start() : opening] + block))
    return sorted(matches)


def slint_instance_count(source: str, name: str) -> int:
    return len(slint_instance_blocks(source, name))


def literal_property(block: str, name: str) -> str:
    """Return the first literal/string value for a property in one object."""

    match = re.search(
        rf"(?m)(?:^|[{{;\n])\s*{re.escape(name)}\s*:\s*(?:\"([^\"]*)\"|([^;\n]+))\s*;",
        block,
    )
    if not match:
        return ""
    return match.group(1) if match.group(1) is not None else match.group(2).strip()


def simple_condition_value(condition: str, values: dict[str, bool]) -> bool:
    """Evaluate the boolean subset used by toolbar visibility branches."""

    expression = condition.strip()
    if not expression:
        return True
    for disjunction in expression.split("||"):
        conjunction_ok = True
        for term in disjunction.split("&&"):
            term = term.strip().strip("()")
            negated = term.startswith("!")
            name = term[1:] if negated else term
            name = name.removeprefix("root.").strip()
            if name not in values:
                conjunction_ok = False
                break
            result = values[name]
            if negated:
                result = not result
            conjunction_ok = conjunction_ok and result
        if conjunction_ok:
            return True
    return False


def max_visible_toolbar_groups(toolbar_source: str) -> tuple[int, int]:
    """Return (maximum visible groups, total declarations) for source branches."""

    group_blocks = slint_instance_blocks(toolbar_source, "ToolbarGroup")
    conditions: list[str] = []
    for block in group_blocks:
        prefix = block.split("{", 1)[0]
        match = re.search(r"\bif\s+(.+?)\s*:\s*ToolbarGroup", prefix)
        conditions.append(match.group(1) if match else "")
    names_set: set[str] = set()
    for condition in conditions:
        for term in re.split(r"&&|\|\|", condition):
            token = term.strip().strip("()")
            if token.startswith("!"):
                token = token[1:].strip()
            if token.startswith("root."):
                names_set.add(token.removeprefix("root."))
    names = sorted(names_set)
    max_visible = 0
    for bits in range(1 << len(names)):
        values = {name: bool(bits & (1 << index)) for index, name in enumerate(names)}
        # App controllers expose one canonical responsive state: a labeled
        # (wide) host is never combined with the compact overflow state. The
        # source often keeps both branches in one body, so counting every
        # independent boolean combination would report mutually exclusive
        # groups as simultaneously visible. Keep this constraint in the
        # source proof rather than hiding declarations or lowering the group
        # limit.
        if values.get("overflow", False) and any(
            values.get(flag, False) for flag in ("wide", "labeled")
        ):
            continue
        max_visible = max(
            max_visible,
            sum(simple_condition_value(condition, values) for condition in conditions),
        )
    return max_visible, len(group_blocks)


def source_geometry_signature(source: str) -> str:
    """Hash geometry-bearing declarations from the reachable app tree.

    The full source hashes below detect any stale entry point. This narrower
    signature is stored alongside the manifest metadata so the geometry
    proof visibly depends on the actual Slint object tree and not only on the
    five shell rectangles generated by the old audit.
    """

    geometry_lines = []
    for line in source.splitlines():
        if re.search(
            r"\b(?:x|y|width|height|min-width|min-height|max-width|max-height|preferred-width|preferred-height|spacing|padding(?:-left|-right|-top|-bottom)?)\s*:",
            line,
        ):
            geometry_lines.append(line.strip())
    return hashlib.sha256("\n".join(geometry_lines).encode("utf-8")).hexdigest()


def app_shell_sequence(source: str) -> list[str]:
    """Extract shell owners in source order from the exported Window root."""

    root = re.search(r"(?m)^\s*export component (\w+) inherits Window\s*\{", source)
    if root is None:
        return []
    block = balanced_slint_block(source, source.find("{", root.start(), root.end()))
    if not block:
        return []
    # Keep this deliberately tied to the actual root's reachable block. A
    # prose/comment occurrence or an unrelated component declaration cannot
    # satisfy the shell proof.
    candidates = (
        "TitleChrome",
        "DocumentChrome",
        "ContextToolbar",
        "LabeledToolbar",
        "Toolbar",
        "DirectionalLayout",
        "CanvasSurface",
        "ContentSurface",
        "SidebarSurface",
        "InspectorSurface",
        "StatusBar",
        "ToolkitStatusBar",
    )
    # Domain-owned wrappers keep state and callbacks beside their surface
    # (for example ``VideoToolbar`` or ``PhotoInspector``). Follow their
    # inheritance chain so the geometry manifest records the canonical shell
    # owner even when the Window intentionally instantiates the wrapper.
    inheritance = component_inheritance_map(source)

    def canonical_names(component: str) -> set[str]:
        names: set[str] = set()
        current = component
        seen: set[str] = set()
        while current and current not in seen:
            seen.add(current)
            if current in candidates:
                names.add(current)
            current = inheritance.get(current, "")
        return names

    found: list[tuple[int, str]] = []
    for name in candidates:
        for match in re.finditer(
            rf"(?m)^\s*(?:if\s+[^\n{{}}]+:\s*)?(?:\w+\s*:=\s*)?{re.escape(name)}\s*\{{",
            block,
        ):
            found.append((match.start(), name))
    for component in inheritance:
        names = canonical_names(component)
        if not names:
            continue
        for match in re.finditer(
            rf"(?m)^\s*(?:if\s+[^\n{{}}]+:\s*)?(?:\w+\s*:=\s*)?{re.escape(component)}\s*\{{",
            block,
        ):
            # A wrapper can only occupy one place in the root tree. Record
            # every canonical owner in its chain at that source position; the
            # metadata consumers deduplicate where a surface has aliases.
            found.extend((match.start(), name) for name in sorted(names))
    return [name for _, name in sorted(found)]


def numeric_widths(source: str, component: str) -> list[float]:
    """Read literal width declarations from actual component instances."""

    widths: list[float] = []
    for block in slint_instance_blocks(source, component):
        depth = 0
        opening = block.find("{")
        closing = block.rfind("}")
        for line in block[opening + 1 : closing].splitlines():
            if depth == 0:
                match = re.match(r"\s*width:\s*([0-9]+(?:\.[0-9]+)?)px\s*;", line)
                if match:
                    widths.append(float(match.group(1)))
            depth += line.count("{") - line.count("}")
    return widths


def root_component_metadata(source: str) -> dict:
    match = re.search(r"(?m)^\s*export component (\w+) inherits Window\s*\{", source)
    if match is None:
        return {}
    block = balanced_slint_block(source, source.find("{", match.start(), match.end()))
    metadata: dict[str, object] = {"root-component": match.group(1)}
    for prefix in ("min", "preferred"):
        for axis in ("width", "height"):
            value = re.search(rf"(?m)^\s*{prefix}-{axis}:\s*([0-9]+(?:\.[0-9]+)?)px\s*;", block)
            if value:
                metadata[f"root-{prefix}-{axis}"] = float(value.group(1))
    return metadata


def app_source_files(app: str) -> list[Path]:
    """Resolve the complete checked-in Slint include graph for one app.

    Slint's build include path lets app files import ``toolkit.slint`` and
    ``components.slint`` by basename even though those files live outside the
    app's ``ui/`` directory. The previous resolver silently dropped those
    modules and made a manifest hash insensitive to shared primitive edits.
    Restrict resolution to the app UI tree plus Loom's shared UI include root;
    unresolved compiler-provided modules remain external by design.
    """

    ui_dir = ROOT / f"loom-{app}/crates/loom-{app}-app/ui"
    entry = ui_dir / "app.slint"
    visited: set[Path] = set()

    roots = (ui_dir.resolve(), SHARED_UI_DIR.resolve())

    def visit(path: Path) -> None:
        resolved = path.resolve()
        if resolved in visited or not path.is_file():
            return
        if not any(_is_relative_to(resolved, root) for root in roots):
            return
        visited.add(resolved)
        source = path.read_text(encoding="utf-8")
        for reference in slint_reference.findall(source):
            candidates = [(path.parent / reference).resolve()]
            # Imports resolved through the compiler include path (the common
            # ``toolkit.slint`` spelling) have no app-relative file.
            candidates.append((SHARED_UI_DIR / reference).resolve())
            for candidate in candidates:
                if candidate.is_file():
                    visit(candidate)

    visit(entry)
    return sorted(visited)


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def relative_source_path(path: Path) -> str:
    return path.resolve().relative_to(ROOT.resolve()).as_posix()


def source_hashes(paths: list[Path]) -> dict[str, str]:
    return {
        relative_source_path(path): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in paths
    }


def toolbar_source_metadata(source: str) -> dict[str, object]:
    toolbar_blocks: list[str] = []
    toolbar_name = ""
    inheritance = component_inheritance_map(source)

    def declaration_block(component: str) -> str:
        match = re.search(
            rf"(?m)^\s*(?:export\s+)?component\s+{re.escape(component)}\s+inherits\s+[^\{{]+\{{",
            source,
        )
        if match is None:
            return ""
        opening = source.find("{", match.start(), match.end())
        block = balanced_slint_block(source, opening)
        return source[match.start() : opening] + block if block else ""

    def host_for(component: str) -> str:
        current = component
        seen: set[str] = set()
        while current and current not in seen:
            seen.add(current)
            if current in {"LabeledToolbar", "ContextToolbar", "Toolbar"}:
                return current
            current = inheritance.get(current, "")
        return ""

    # Prefer the concrete labeled host when an app has both conditional
    # variants. This makes the manifest describe the full responsive toolbar
    # contract rather than whichever branch happens to appear first in source.
    for name in ("LabeledToolbar", "ContextToolbar", "Toolbar"):
        blocks_for_name = slint_instance_blocks(source, name)
        if blocks_for_name:
            toolbar_name = name
            toolbar_blocks.extend(blocks_for_name)
            if name != "Toolbar":
                break
    # Componentized app toolbars keep their command groups in a named wrapper
    # (``PhotoToolbar``) or body (``PresentToolbarBody``). Include each
    # declaration once. Wrapper inheritance identifies the canonical slot;
    # the suffix fallback covers body components that own a DirectionalLayout
    # rather than inheriting the host directly.
    for component in inheritance:
        canonical_host = host_for(component)
        if not canonical_host and not re.search(r"(?:ToolbarBody|ActionToolbar)$", component):
            continue
        block = declaration_block(component)
        if not block:
            continue
        toolbar_blocks.append(block)
        if not toolbar_name:
            toolbar_name = canonical_host or "Toolbar"
    toolbar_source = "\n".join(toolbar_blocks)
    visible_groups, group_declarations = max_visible_toolbar_groups(toolbar_source)
    item_names = (
        "IconOnlyToolbarItem",
        "ToolbarIconButton",
        "IconOverLabelToolbarItem",
        "AppleToolbarItem",
    )
    toolbar_items = []
    for _, component, block in slint_named_instance_blocks(toolbar_source, item_names):
        toolbar_items.append(
            {
                "component": component,
                "icon": literal_property(block, "icon"),
                "label": literal_property(block, "label"),
                "accessible-label": literal_property(block, "accessible-label"),
            }
        )
    labels = [item["label"] for item in toolbar_items if item["label"]]
    return {
        "toolbar-component": toolbar_name,
        "toolbar-groups": visible_groups,
        "toolbar-group-declarations": group_declarations,
        "toolbar-icon-only-items": slint_instance_count(toolbar_source, "IconOnlyToolbarItem")
        + slint_instance_count(toolbar_source, "ToolbarIconButton"),
        "toolbar-icon-over-label-items": slint_instance_count(toolbar_source, "IconOverLabelToolbarItem")
        + slint_instance_count(toolbar_source, "AppleToolbarItem"),
        "toolbar-overflow-items": slint_instance_count(toolbar_source, "Overflow")
        + slint_instance_count(toolbar_source, "ToolbarOverflowButton")
        + slint_instance_count(toolbar_source, "OverflowMenuButton"),
        "toolbar-labels": labels,
        "toolbar-items": toolbar_items,
        "labeled-slot-bindings": len(re.findall(r"\blabeled-slot\s*:", toolbar_source)),
    }


def app_source_metadata(app: str, source: str) -> dict[str, object]:
    metadata = root_component_metadata(source)
    metadata.update(toolbar_source_metadata(source))
    shell = app_shell_sequence(source)
    metadata.update(
        {
            "root-shell": shell,
            "source-rectangles": slint_instance_count(source, "Rectangle"),
            "source-horizontal-layouts": slint_instance_count(source, "HorizontalLayout"),
            "source-vertical-layouts": slint_instance_count(source, "VerticalLayout"),
            "source-grid-layouts": slint_instance_count(source, "GridLayout"),
            "source-property-rows": slint_instance_count(source, "PropertyRow"),
            "source-geometry-signature": source_geometry_signature(source),
        }
    )
    metadata.update(
        {
            "title-component": "TitleChrome" if component_used(source, "TitleChrome") else "DocumentChrome",
            "status-component": "StatusBar" if component_used(source, "StatusBar") else "ToolkitStatusBar",
            "sidebar-count": slint_instance_count(source, "SidebarSurface"),
            "inspector-count": slint_instance_count(source, "InspectorSurface"),
            "sidebar-width": max(numeric_widths(source, "SidebarSurface"), default=0.0),
            "inspector-width": max(numeric_widths(source, "InspectorSurface"), default=0.0),
            "directional-layout-count": slint_instance_count(source, "DirectionalLayout"),
            "rtl-binding": "Theme.rtl" in source,
            # DirectionalLayout swaps logical edge insets, but it cannot
            # reorder arbitrary children. Record whether this app's source
            # adds an explicit root.rtl branch for distinct panel placement.
            "rtl-layout-branches": bool(re.search(r"\bif\s+root\.rtl\b", source)),
        }
    )
    return metadata


def runtime_metric(source: str, metric: str) -> float:
    match = re.search(rf"\b{re.escape(metric)}:\s*([0-9]+(?:\.[0-9]+)?)px\b", source)
    if match is None:
        raise ValueError(f"missing runtime metric {metric}")
    return float(match.group(1))


def geometry_manifest(
    width: int,
    height: int,
    text_scale: float,
    direction: str,
    app_contract: dict,
    responsive: dict,
    geometry_contract: dict,
    source_metadata: dict[str, object],
    runtime_metrics: dict[str, float],
) -> dict:
    """Build a numeric shell manifest from the reachable Slint source tree.

    The manifest is intentionally a logical geometry proof (the compiler's
    runtime tree is not available to this Python audit), but every shell owner,
    toolbar item, panel width, and geometry signature comes from the actual
    exported Window source metadata. A fixed fixture cannot satisfy this
    function anymore.
    """

    title_height = runtime_metrics["title-height"]
    status_height = runtime_metrics["status-height"]
    context_toolbar_height = runtime_metrics["context-toolbar-height"]
    labeled_min = runtime_metrics["labeled-toolbar-min-height"]
    labeled_max = runtime_metrics["labeled-toolbar-max-height"]
    primary_min_width = float(geometry_contract["primary-surface-min-width"])
    primary_min_height = float(geometry_contract["primary-surface-min-height"])
    p1 = int(responsive["priority-1-icon-only-below"])
    p2 = int(responsive["priority-2-overflow-below"])
    icon_only = width < p1
    overflow = width < p2
    labeled = width >= p2
    toolbar_height = labeled_min if labeled else context_toolbar_height

    root_shell = list(source_metadata.get("root-shell", []))
    title_name = next((name for name in root_shell if name in {"TitleChrome", "DocumentChrome"}), "")
    toolbar_name = next(
        (name for name in root_shell if name in {"ContextToolbar", "LabeledToolbar", "Toolbar"}),
        "",
    )
    work_name = next(
        (name for name in root_shell if name in {"DirectionalLayout", "CanvasSurface", "ContentSurface"}),
        "",
    )
    status_name = next((name for name in root_shell if name in {"StatusBar", "ToolkitStatusBar"}), "")

    # Keep the actual root's shell owners in source order. The audit rejects a
    # missing owner before this list is used, so these are not synthetic
    # placeholders masquerading as rendered app geometry.
    rects = []
    if title_name:
        rects.append(GeometryRect("title", 0.0, 0.0, float(width), title_height))
    if toolbar_name:
        rects.append(GeometryRect("toolbar", 0.0, title_height, float(width), toolbar_height))
    work_y = title_height + toolbar_height
    work_height = max(0.0, float(height) - work_y - status_height)

    left = float(source_metadata.get("sidebar-width", 0.0))
    right = float(source_metadata.get("inspector-width", 0.0))
    # Source metadata may omit a literal width for a responsive expression;
    # the contract remains the durable fallback for that explicit case.
    if left <= 0:
        left = float(app_contract.get("left-sidebar-default", 0))
    if right <= 0:
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

    mirror_panels = bool(source_metadata.get("rtl-layout-branches", False))
    if direction == "ltr" or not mirror_panels:
        left_x, right_x, primary_x = 0.0, float(width) - right, left
    else:
        left_x, right_x, primary_x = float(width) - left, 0.0, right
    if left > 0 and work_name:
        rects.append(GeometryRect("left-sidebar", left_x, work_y, left, work_height))
    if right > 0 and work_name:
        rects.append(GeometryRect("right-inspector", right_x, work_y, right, work_height))
    if work_name:
        rects.append(GeometryRect("primary-work-surface", primary_x, work_y, primary_width, work_height))
    if status_name:
        rects.append(GeometryRect("status", 0.0, work_y + work_height, float(width), status_height))

    # Labels and item counts come from the app's real toolbar source. Caption
    # width is intentionally conservative so a 150% probe catches a host that
    # forgot to move or elide a lower-priority command.
    toolbar_items = list(source_metadata.get("toolbar-items", []))
    caption_widths = []
    for item in toolbar_items:
        label = str(item.get("label", ""))
        # Dynamic labels (for example a zoom percentage or transport state)
        # are represented by their source expression. Measure the longest
        # literal branch rather than charging the expression's source-code
        # length; otherwise `root.is-playing ? "Pause" : "Play"` would make
        # the geometry proof report a collision that cannot occur at runtime.
        literal_lengths = [len(value) for value in re.findall(r'"([^"]*)"', label)]
        label_length = max(4, max(literal_lengths, default=len(label)))
        caption_widths.append(max(48.0, 10.0 * label_length * 0.62 * text_scale + 16.0))
    icon_count = sum(
        1 for item in toolbar_items if item.get("component") in {"IconOnlyToolbarItem", "ToolbarIconButton"}
    )
    labeled_count = sum(
        1
        for item in toolbar_items
        if item.get("component") in {"IconOverLabelToolbarItem", "AppleToolbarItem"}
    )
    overflow_count = int(source_metadata.get("toolbar-overflow-items", 0))
    if labeled:
        # Alternate compact branches are not present in the wide tree; only
        # labels attached to icon-over-label instances consume the labeled
        # slot. This avoids counting hidden icon-only fallbacks twice.
        visible_item_widths = caption_widths[:labeled_count] or [28.0] * max(1, labeled_count)
    elif not overflow:
        visible_item_widths = [28.0] * max(1, icon_count)
    else:
        # P2 actions are represented by one canonical overflow target even if
        # the source currently has no explicit Overflow instance.
        visible_item_widths = [28.0] * max(1, icon_count)
        visible_item_widths.append(28.0 if overflow_count == 0 else 28.0 * overflow_count)
    toolbar_content_width = sum(visible_item_widths) + 4.0 * (len(visible_item_widths) - 1) + 16.0
    fits = toolbar_content_width <= float(width)

    return {
        "viewport": [width, height],
        "text-scale": text_scale,
        "direction": direction,
        "rtl-layout-mode": "mirrored-panels" if mirror_panels else "logical-insets-only",
        "source-shell": root_shell,
        "source-geometry-signature": source_metadata.get("source-geometry-signature", ""),
        "source-object-counts": {
            "rectangles": int(source_metadata.get("source-rectangles", 0)),
            "horizontal-layouts": int(source_metadata.get("source-horizontal-layouts", 0)),
            "vertical-layouts": int(source_metadata.get("source-vertical-layouts", 0)),
            "grid-layouts": int(source_metadata.get("source-grid-layouts", 0)),
            "property-rows": int(source_metadata.get("source-property-rows", 0)),
        },
        "rects": [rect.__dict__ for rect in rects],
        "optional-panels": {"left": left, "right": right},
        "primary-surface": {"width": primary_width, "height": work_height},
        "toolbar": {
            "height": toolbar_height,
            "lines": 1 if fits else 2,
            "groups": int(source_metadata.get("toolbar-groups", 0)),
            "content-width": toolbar_content_width,
            "fits": fits,
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
    if toolbar["groups"] > 3:
        issues.append(f"toolbar exposes more than three visible groups ({toolbar['groups']})")
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


def component_inheritance_map(text: str) -> dict[str, str]:
    """Return app-local component inheritance without treating declarations as uses."""

    return {
        child: parent
        for child, parent in re.findall(
            r"(?m)^\s*(?:export\s+)?component\s+(\w+)\s+inherits\s+(\w+)\s*\{",
            text,
        )
    }


def component_used_or_inherits(text: str, name: str) -> bool:
    """Recognize a canonical primitive consumed through a local wrapper.

    Applications keep domain-owned components such as ``PhotoToolbar`` and
    ``PhotoStatusBar`` so the state boundary travels with the workspace. The
    source-backed audit must follow those wrappers to ``LabeledToolbar`` or
    ``ToolkitStatusBar`` instead of requiring every app to duplicate the
    shared primitive at the Window call site.
    """

    if component_used(text, name):
        return True
    inheritance = component_inheritance_map(text)
    used_components = {
        component
        for component in inheritance
        if component_used(text, component)
    }
    for component in used_components:
        current = component
        seen: set[str] = set()
        while current in inheritance and current not in seen:
            seen.add(current)
            current = inheritance[current]
            if current == name:
                return True
    return False


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
        "toolbar-icon-button": "IconOnlyToolbarItem",
        "toolbar-overflow-button": "Overflow",
        "panel-header": "PanelHeader",
        "sidebar": "SidebarSurface",
        "inspector": "InspectorSurface",
        "status": "StatusBar",
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
        "IconButton": "IconOnlyToolbarItem",
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
        "DirectionalLayout": "DirectionalLayout",
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
    expected_compatibility = {
        "DocumentChrome": "TitleChrome",
        "ToolbarIconButton": "IconOnlyToolbarItem",
        "AppleToolbarItem": "IconOverLabelToolbarItem",
        "ToolbarOverflowButton": "Overflow",
        "OverflowMenuButton": "Overflow",
        "ToolkitStatusBar": "StatusBar",
        "TextField": "Field",
        "TabStrip": "SheetTabStrip",
    }
    require(
        ownership.get("compatibility-inherits") == expected_compatibility,
        "design system: compatibility inheritance map was weakened or changed",
    )

    overflow = contract.get("overflow-policy", {})
    expected_overflow = {
        "canonical-button": "Overflow",
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


def write_geometry_manifest() -> None:
    """Generate the checked-in source manifest used by the geometry audit."""

    def toml_inline_table(value: dict[str, object]) -> str:
        """Serialize one toolbar item using TOML, not JSON object syntax."""

        fields = ", ".join(
            f"{json.dumps(str(key))} = {json.dumps(str(item_value))}"
            for key, item_value in value.items()
        )
        return "{ " + fields + " }"

    toolkit_path = ROOT / "loom-core/crates/loom-ui/ui/toolkit.slint"
    theme_path = ROOT / "loom-core/crates/loom-ui/ui/theme.slint"
    theme_source = theme_path.read_text(encoding="utf-8")
    metrics = {
        "title-height": runtime_metric(theme_source, "header-height"),
        "context-toolbar-height": runtime_metric(theme_source, "context-toolbar-height"),
        "labeled-toolbar-min-height": runtime_metric(theme_source, "labeled-toolbar-min-height"),
        "labeled-toolbar-max-height": runtime_metric(theme_source, "labeled-toolbar-max-height"),
        "status-height": runtime_metric(theme_source, "status-height"),
    }
    lines = [
        "# Generated from the checked-in Slint entry points by",
        "# audit-product-ui.py --write-geometry-manifest.",
        "# Do not edit by hand: source hashes make stale geometry evidence fail.",
        "",
        "[meta]",
        'version = "1.0.0"',
        'generator = "loom-bootstrap/scripts/audit-product-ui.py"',
        "",
    ]
    for app in APPS:
        source = active_application_ui(app)
        metadata = app_source_metadata(app, source)
        paths = app_source_files(app)
        hashes = source_hashes(paths)
        lines.append(f"[apps.{app}]")
        entry_path = ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint"
        lines.append(f"entry = {json.dumps(relative_source_path(entry_path))}")
        lines.append(f"source-files = {json.dumps(sorted(hashes))}")
        inline_hashes = ", ".join(f"{json.dumps(path)} = {json.dumps(value)}" for path, value in sorted(hashes.items()))
        lines.append(f"source-hashes = {{ {inline_hashes} }}")
        for key in (
            "root-component",
            "title-component",
            "toolbar-component",
            "status-component",
        ):
            lines.append(f"{key} = {json.dumps(metadata.get(key, ''))}")
        for key in (
            "root-min-width",
            "root-min-height",
            "root-preferred-width",
            "root-preferred-height",
            "sidebar-count",
            "inspector-count",
            "sidebar-width",
            "inspector-width",
            "toolbar-groups",
            "toolbar-group-declarations",
            "toolbar-icon-only-items",
            "toolbar-icon-over-label-items",
            "toolbar-overflow-items",
            "labeled-slot-bindings",
            "directional-layout-count",
            "source-rectangles",
            "source-horizontal-layouts",
            "source-vertical-layouts",
            "source-grid-layouts",
            "source-property-rows",
        ):
            lines.append(f"{key} = {json.dumps(metadata.get(key, 0))}")
        lines.append(f"rtl-binding = {str(bool(metadata.get('rtl-binding', False))).lower()}")
        lines.append(f"rtl-layout-branches = {str(bool(metadata.get('rtl-layout-branches', False))).lower()}")
        lines.append(f"toolbar-labels = {json.dumps(metadata.get('toolbar-labels', []))}")
        toolbar_items = metadata.get("toolbar-items", [])
        toolbar_items_toml = "[" + ", ".join(
            toml_inline_table(item) for item in toolbar_items if isinstance(item, dict)
        ) + "]"
        lines.append(f"toolbar-items = {toolbar_items_toml}")
        lines.append(f"root-shell = {json.dumps(metadata.get('root-shell', []))}")
        lines.append(f"source-geometry-signature = {json.dumps(metadata.get('source-geometry-signature', ''))}")
        lines.append("")
    GEOMETRY_MANIFEST_PATH.write_text("\n".join(lines), encoding="utf-8")


def audit_source_manifest(manifest: dict) -> dict[str, dict[str, object]]:
    """Validate manifest hashes and return source-derived app metadata."""

    require(manifest.get("meta", {}).get("version") == "1.0.0", "geometry manifest: unsupported source manifest version")
    apps = manifest.get("apps", {})
    result: dict[str, dict[str, object]] = {}
    for app in APPS:
        entry = apps.get(app)
        if not isinstance(entry, dict):
            require(False, f"geometry manifest: missing source entry for {app}")
            continue
        source = active_application_ui(app)
        actual = app_source_metadata(app, source)
        expected_hashes = entry.get("source-hashes", {})
        actual_paths = app_source_files(app)
        actual_hashes = source_hashes(actual_paths)
        expected_entry = relative_source_path(ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint")
        require(entry.get("entry") == expected_entry, f"geometry manifest: {app} entry path is stale")
        require(
            entry.get("source-files") == sorted(actual_hashes),
            f"geometry manifest: {app} source file list is stale",
        )
        require(
            expected_hashes == actual_hashes,
            f"geometry manifest: {app} source hash is stale; regenerate from current Slint",
        )
        for key, value in actual.items():
            if isinstance(value, float) and value.is_integer():
                value = int(value)
            require(entry.get(key) == value, f"geometry manifest: {app} metadata drift for {key}")
        require(
            int(actual.get("toolbar-groups", 0)) <= 3,
            f"geometry manifest: {app} source exposes more than three visible toolbar groups",
        )
        result[app] = actual
    return result


def audit_geometry_manifest() -> None:
    """Exercise source-derived geometry at every boundary, scale, and direction."""

    contract_path = ROOT / "loom-design-bible/contracts/desktop-ui.toml"
    if not contract_path.is_file() or not GEOMETRY_MANIFEST_PATH.is_file():
        require(GEOMETRY_MANIFEST_PATH.is_file(), "geometry manifest: checked-in source manifest is missing")
        return
    with contract_path.open("rb") as handle:
        contract = tomllib.load(handle)
    with GEOMETRY_MANIFEST_PATH.open("rb") as handle:
        source_manifest = tomllib.load(handle)

    responsive = contract.get("responsive", {})
    geometry = contract.get("geometry-manifest", {})
    theme_source = read("loom-core/crates/loom-ui/ui/theme.slint")
    runtime_metrics = {
        "title-height": runtime_metric(theme_source, "header-height"),
        "context-toolbar-height": runtime_metric(theme_source, "context-toolbar-height"),
        "labeled-toolbar-min-height": runtime_metric(theme_source, "labeled-toolbar-min-height"),
        "labeled-toolbar-max-height": runtime_metric(theme_source, "labeled-toolbar-max-height"),
        "status-height": runtime_metric(theme_source, "status-height"),
    }
    source_metadata = audit_source_manifest(source_manifest)
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
    reference_metadata = source_metadata.get("writer", {})
    reference_contract = contract.get("app", {}).get("writer", {})
    for width in required_probes:
        manifest = geometry_manifest(
            width,
            800,
            1.0,
            "ltr",
            reference_contract,
            responsive,
            geometry,
            reference_metadata,
            runtime_metrics,
        )
        for key, value in expected[width].items():
            require(
                manifest["toolbar"][key] == value,
                f"geometry manifest: {width}px responsive state drift for {key}",
            )

    viewports = [(width, 800) for width in required_probes]
    viewports.extend(tuple(pair) for pair in contract.get("validation", {}).get("required-viewports", []))
    for app in APPS:
        app_contract = contract.get("app", {}).get(app, {})
        app_metadata = source_metadata.get(app, {})
        for width, height in viewports:
            for text_scale in responsive.get("text-scale-probes", [1.0, 1.5]):
                for direction in responsive.get("direction-probes", ["ltr", "rtl"]):
                    manifest = geometry_manifest(
                        int(width),
                        int(height),
                        float(text_scale),
                        direction,
                        app_contract,
                        responsive,
                        geometry,
                        app_metadata,
                        runtime_metrics,
                    )
                    issues = assert_geometry_manifest(manifest, geometry)
                    for issue in issues:
                        require(
                            False,
                            f"geometry manifest: {app} {width}x{height} scale={text_scale} direction={direction}: {issue}",
                        )

        # Direction is a real source/runtime input. Only apps with explicit
        # root.rtl panel branches may claim mirrored panel geometry; the
        # current stable DirectionalLayout contract otherwise proves logical
        # edge insets without inventing child-order mirroring.
        ltr = geometry_manifest(1280, 800, 1.0, "ltr", app_contract, responsive, geometry, app_metadata, runtime_metrics)
        rtl = geometry_manifest(1280, 800, 1.0, "rtl", app_contract, responsive, geometry, app_metadata, runtime_metrics)
        require(ltr["direction"] != rtl["direction"], f"geometry manifest: {app} direction state is not preserved")
        if app_metadata.get("rtl-layout-branches", False):
            require(ltr["rects"] != rtl["rects"], f"geometry manifest: {app} RTL panel branch does not mirror source panels")
        else:
            require(ltr["rects"] == rtl["rects"], f"geometry manifest: {app} invented RTL panel mirroring without a source branch")


def audit_toolkit() -> None:
    toolkit = read("loom-core/crates/loom-ui/ui/toolkit.slint")
    required_components = (
        "DocumentChrome", "TitleChrome", "Toolbar", "ContextToolbar", "LabeledToolbar", "ToolbarGroup", "ToolbarSpacer",
        "DirectionalLayout", "ToolbarButton", "ToolbarIconButton", "IconOnlyToolbarItem", "ToolbarOverflowButton", "Overflow",
        "AppleToolbarItem", "IconOverLabelToolbarItem", "PanelHeader", "SidebarSurface",
        "InspectorSurface", "SectionHeader", "ToolkitStatusBar", "CanvasSurface",
        "ContentSurface", "TextField", "SearchField", "SegmentedControl",
        "Toggle", "RangeSlider", "Field", "FormulaBar", "InspectorSection", "PropertyRow", "TabStrip", "SheetTabStrip",
        "StatusBar",
    )
    for component in required_components:
        require(f"export component {component}" in toolkit, f"shared toolkit: missing {component}")

    for component in ("ToolbarButton", "IconOnlyToolbarItem", "Toggle"):
        block = exported_component_block(toolkit, component)
        for token in ("accessible-role", "accessible-label", "accessible-action-default", "key-pressed(event)"):
            require(token in block, f"shared toolkit: {component} missing {token}")

    for component in ("Field", "FormulaBar", "SearchField", "SegmentedControl", "RangeSlider"):
        block = exported_component_block(toolkit, component)
        require("accessible-role" in block and "accessible-label" in block, f"shared toolkit: {component} lacks accessibility metadata")

    require(not re.search(r"#[0-9a-fA-F]{6,8}", toolkit), "shared toolkit: hard-coded palette color")

    toolbar = exported_component_block(toolkit, "Toolbar")
    require(
        "labeled-slot" in toolbar and "ResponsivePolicy.toolbar-height" in toolbar,
        "shared toolkit: Toolbar must explicitly select context vs labeled slot",
    )
    require(
        re.search(r"(?m)^\s*in property <bool> labeled-slot:\s*true;", toolbar) is not None,
        "shared toolkit: Toolbar must preserve the legacy labeled-slot default",
    )

    directional = exported_component_block(toolkit, "DirectionalLayout")
    require(
        "export component DirectionalLayout inherits HorizontalLayout" in directional
        and "in property <bool> rtl: Theme.rtl;" in directional
        and "alignment: stretch;" in directional
        and "padding-left: root.rtl ? root.trailing-padding : root.leading-padding;" in directional
        and "padding-right: root.rtl ? root.leading-padding : root.trailing-padding;" in directional,
        "shared toolkit: DirectionalLayout must bind RTL to stable row geometry",
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
    require(
        re.search(r"(?m)^\s*in property <bool> labeled-slot:\s*false;", group) is not None,
        "shared toolkit: ToolbarGroup must default to the 40px context slot",
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
        "ResponsivePolicy.labeled-toolbar-min-height" in icon_over_label
        and "ResponsivePolicy.labeled-toolbar-max-height" in icon_over_label,
        "shared toolkit: IconOverLabelToolbarItem must reserve its labeled slot",
    )

    compact = exported_component_block(toolkit, "IconOnlyToolbarItem")
    require(
        "width: Theme.tokens.metrics.control-height;" in compact
        and "height: Theme.tokens.metrics.control-height;" in compact
        and "size: 16px;" in compact,
        "shared toolkit: IconOnlyToolbarItem must retain tokenized 28px/16px compact geometry",
    )

    overflow = exported_component_block(toolkit, "Overflow")
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
        require(token in overflow, f"shared toolkit: Overflow missing {token}")

    for component in ("PanelHeader", "StatusBar"):
        block = exported_component_block(toolkit, component)
        require("Theme.tokens" in block and "Theme.palette()" in block, f"shared toolkit: {component} is not tokenized")
    for component in ("SidebarSurface", "InspectorSurface"):
        block = exported_component_block(toolkit, component)
        # Panel surfaces use fixed min/max widths from the machine contract;
        # their colors and nested header remain semantic/tokenized.
        require("Theme.palette()" in block and "PanelHeader" in block, f"shared toolkit: {component} is not tokenized")

    property_row = exported_component_block(toolkit, "PropertyRow")
    require(
        "in property <bool> stacked" in property_row
        and "GridLayout" in property_row
        and "row: root.stacked-layout ? 1 : 0;" in property_row
        and "wrap: word-wrap;" in property_row
        and "private property <bool> auto-stacked" in property_row
        and "accessible-label: root.label;" in property_row
        and "overflow: clip;" not in property_row,
        "shared toolkit: PropertyRow must support non-eliding automatic stacked localization layout",
    )


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
        "IconButton": "IconOnlyToolbarItem",
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

    # Canonical declarations own the implementation. Compatibility spellings
    # are empty one-way inheritance shims; checking the direction prevents a
    # second geometry/state owner from quietly returning through an old name.
    canonical_bases = {
        "TitleChrome": "Rectangle",
        "IconOnlyToolbarItem": "Rectangle",
        "IconOverLabelToolbarItem": "Rectangle",
        "Overflow": "Rectangle",
        "StatusBar": "Rectangle",
        "Field": "Rectangle",
        "SheetTabStrip": "Rectangle",
    }
    for name, base in canonical_bases.items():
        require(
            re.search(rf"(?m)^\s*export component {name}\s+inherits\s+{base}\b", toolkit)
            is not None,
            f"shared ownership: canonical {name} must own its {base} implementation",
        )

    compatibility = {
        "DocumentChrome": "TitleChrome",
        "ToolbarIconButton": "IconOnlyToolbarItem",
        "AppleToolbarItem": "IconOverLabelToolbarItem",
        "ToolbarOverflowButton": "Overflow",
        "OverflowMenuButton": "Overflow",
        "ToolkitStatusBar": "StatusBar",
        "TextField": "Field",
        "TabStrip": "SheetTabStrip",
    }
    for legacy, target in compatibility.items():
        block = exported_component_block(toolkit, legacy)
        require(
            re.search(rf"(?m)^\s*export component {legacy}\s+inherits\s+{target}\b", block)
            is not None,
            f"shared ownership: compatibility {legacy} must inherit canonical {target}",
        )
        body = block[block.find("{") + 1 : block.rfind("}")]
        body_without_comments = re.sub(r"//[^\n]*|/\*[\s\S]*?\*/", "", body).strip()
        require(
            not body_without_comments,
            f"shared ownership: compatibility {legacy} contains a second implementation",
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
        "DirectionalLayout",
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
        require(
            component_used(text, "TitleChrome") or component_used(text, "DocumentChrome"),
            f"{app}: toolkit migration missing title chrome",
        )
        require(
            component_used_or_inherits(text, "Toolbar")
            or component_used_or_inherits(text, "ContextToolbar")
            or component_used_or_inherits(text, "LabeledToolbar"),
            f"{app}: toolkit migration missing toolbar",
        )
        require(component_used(text, "ToolbarGroup"), f"{app}: toolkit migration missing ToolbarGroup")
        require(
            component_used_or_inherits(text, "StatusBar")
            or component_used_or_inherits(text, "ToolkitStatusBar"),
            f"{app}: toolkit migration missing status bar",
        )
        for name in ("AppHeader", "WorkspaceToolbar"):
            require(not component_used(text, name), f"{app}: toolkit migration still contains legacy {name}")
        require("Theme.rtl" in text, f"{app}: RTL root state is not wired to Theme.rtl")
        require("DirectionalLayout" in text, f"{app}: workspace does not consume DirectionalLayout")
        audit_toolbar_slot_hosts(app, text)
    else:
        for name in ("AppHeader", "WorkspaceToolbar", "StatusBar"):
            require(component_used(text, name), f"{app}: missing legacy {name} before migration")
        require("compact-layout" in text, f"{app}: missing compact-layout policy before migration")

    require("Theme.palette()" in text, f"{app}: missing semantic palette use")
    require("min-width:" in text, f"{app}: missing minimum responsive width")
    require("min-height:" in text, f"{app}: missing minimum responsive height")
    require("horizontal-stretch" in text or "CanvasSurface" in text, f"{app}: missing horizontal adaptive layout")
    require("vertical-stretch" in text or "CanvasSurface" in text, f"{app}: missing vertical adaptive layout")

    if app in {"writer", "sheets", "present"}:
        require("ResponsivePolicy::get" in main, f"{app}: responsive breakpoints are not read from ResponsivePolicy")
        require(
            not any(
                re.search(r"(?:<|>|==|>=|<=)\s*(?:1180|1320)\b", line)
                and not line.lstrip().startswith(("assert", "for width"))
                for line in main.splitlines()
            ),
            f"{app}: production breakpoint logic contains a host-local 1180/1320 threshold",
        )

    require(not emoji.search(text), f"{app}: emoji/icon-font glyphs remain")
    require(not re.search(r"#[0-9a-fA-F]{6,8}", text), f"{app}: hard-coded color outside theme")
    lowered = text.lower()
    for token in ("coming soon", "placeholder ui", "placeholder control", "fake progress", "model preview"):
        require(token not in lowered, f"{app}: prototype/fabricated-state language remains: {token}")

    require("!other.starts_with('-') && args.open.is_none()" in main, f"{app}: positional document opening unsupported")
    for slider in blocks(text, "Slider {"):
        require("label:" in slider, f"{app}: slider lacks accessibility label")
    return text


def toolbar_component_declarations(source: str) -> dict[str, str]:
    """Return named toolbar-body declarations and their balanced source blocks."""

    declarations: dict[str, str] = {}
    pattern = re.compile(
        r"(?m)^\s*(?:export\s+)?component\s+(\w+(?:ToolbarBody|ActionToolbar))\s+inherits\s+[^\{]+\{"
    )
    for match in pattern.finditer(source):
        opening = source.find("{", match.start(), match.end())
        block = balanced_slint_block(source, opening)
        if block:
            declarations[match.group(1)] = source[match.start() : opening] + block
    return declarations


def toolbar_condition(block: str) -> str:
    """Return the conditional expression attached to a ToolbarGroup block."""

    prefix = block.split("{", 1)[0]
    match = re.search(r"\bif\s+(.+?)\s*:\s*ToolbarGroup", prefix)
    return match.group(1) if match else ""


def condition_names(condition: str) -> set[str]:
    names: set[str] = set()
    for term in re.split(r"&&|\|\|", condition):
        token = term.strip().strip("()")
        token = token.removeprefix("!").strip().removeprefix("root.")
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", token):
            names.add(token)
    return names


def condition_can_be_true(condition: str, known: dict[str, bool]) -> bool:
    """Evaluate a condition while enumerating only its unresolved flags."""

    names = sorted(condition_names(condition))
    unknown = [name for name in names if name not in known]
    for bits in range(1 << len(unknown)):
        values = dict(known)
        values.update({name: bool(bits & (1 << index)) for index, name in enumerate(unknown)})
        if simple_condition_value(condition, values):
            return True
    return simple_condition_value(condition, known) if not unknown else False


def toolbar_body_instances(block: str, declarations: dict[str, str]) -> list[tuple[str, str]]:
    """Find body instances nested in a toolbar host, preserving bindings."""

    if not declarations:
        return []
    names = tuple(declarations)
    return [
        (name, instance)
        for _, name, instance in slint_named_instance_blocks(block, names)
    ]


def audit_toolbar_body_slot(
    app: str,
    host_kind: str,
    host_block: str,
    body_name: str,
    body_instance: str,
    body_source: str,
) -> None:
    """Prove a named body selects controls compatible with its host slot."""

    expected_labeled = host_kind == "labeled"
    bindings: dict[str, bool] = {}
    for property_name in ("wide", "labeled", "overflow"):
        match = re.search(
            rf"(?m)^\s*{re.escape(property_name)}\s*:\s*(true|false)\s*;",
            body_instance,
        )
        if match:
            bindings[property_name] = match.group(1) == "true"

    selector_values = [
        bindings[name]
        for name in ("wide", "labeled")
        if name in bindings
    ]
    require(
        bool(selector_values) and all(value == expected_labeled for value in selector_values),
        f"{app}: {host_kind} toolbar body {body_name} must bind wide/labeled explicitly",
    )

    # Responsive controllers pair a labeled host with the non-overflow state
    # and a context host with its compact/overflow state. This keeps mutually
    # exclusive branches from being interpreted as simultaneous children.
    if "overflow" not in bindings and selector_values:
        bindings["overflow"] = not expected_labeled

    groups = slint_instance_blocks(body_source, "ToolbarGroup")
    selected_groups: list[tuple[str, str]] = []
    for group in groups:
        condition = toolbar_condition(group)
        if not condition_can_be_true(condition, bindings):
            continue
        slot = literal_property(group, "labeled-slot").lower()
        selected_groups.append((slot, group))

    wanted_slot = "true" if expected_labeled else "false"
    require(
        any(slot == wanted_slot for slot, _ in selected_groups),
        f"{app}: {host_kind} toolbar body {body_name} has no selected {wanted_slot} ToolbarGroup",
    )
    require(
        all(slot == wanted_slot for slot, _ in selected_groups),
        f"{app}: {host_kind} toolbar body {body_name} can expose the opposite toolbar slot",
    )
    selected_source = "\n".join(group for slot, group in selected_groups if slot == wanted_slot)
    if expected_labeled:
        require(
            re.search(r"\b(?:AppleToolbarItem|IconOverLabelToolbarItem)\s*\{", selected_source)
            is not None,
            f"{app}: labeled toolbar body {body_name} must select icon-over-label controls",
        )
    else:
        require(
            re.search(r"\b(?:IconOnlyToolbarItem|ToolbarIconButton|Overflow)\s*\{", selected_source)
            is not None,
            f"{app}: context toolbar body {body_name} must select icon-only/overflow controls",
        )


def audit_toolbar_slot_hosts(app: str, source: str) -> None:
    """Require hosts, groups, and named bodies to choose one explicit slot."""

    declarations = toolbar_component_declarations(source)
    host_specs = (
        ("context", slint_instance_blocks(source, "ContextToolbar")),
        ("labeled", slint_instance_blocks(source, "LabeledToolbar")),
    )
    for host_kind, hosts in host_specs:
        expected = "false" if host_kind == "context" else "true"
        for block in hosts:
            require(
                re.search(r"(?m)^\s*labeled-slot\s*:\s*root\.", block) is None,
                f"{app}: {host_kind.title()}Toolbar must not bind labeled-slot to responsive state",
            )
            direct_groups = slint_instance_blocks(block, "ToolbarGroup")
            for group in direct_groups:
                require(
                    literal_property(group, "labeled-slot").lower() == expected,
                    f"{app}: {host_kind.title()}Toolbar direct ToolbarGroup must use labeled-slot: {expected}",
                )
            body_instances = toolbar_body_instances(block, declarations)
            require(
                bool(body_instances) or bool(direct_groups),
                f"{app}: {host_kind.title()}Toolbar must expose a concrete body or direct ToolbarGroup",
            )
            for body_name, body_instance in body_instances:
                audit_toolbar_body_slot(
                    app,
                    host_kind,
                    block,
                    body_name,
                    body_instance,
                    declarations[body_name],
                )

    for block in slint_instance_blocks(source, "Toolbar"):
        require(
            not re.search(r"\b(?:AppleToolbarItem|IconOverLabelToolbarItem)\s*\{", block),
            f"{app}: generic Toolbar cannot host icon-over-label controls",
        )
        for body_name, body_instance in toolbar_body_instances(block, declarations):
            require(
                not re.search(r"\b(?:AppleToolbarItem|IconOverLabelToolbarItem)\s*\{", declarations[body_name]),
                f"{app}: generic Toolbar body {body_name} cannot host icon-over-label controls",
            )

    for group in slint_instance_blocks(source, "ToolbarGroup"):
        require(
            re.search(r"\blabeled-slot\s*:\s*(?:true|false)\s*;", group) is not None,
            f"{app}: ToolbarGroup must declare its context/labeled slot",
        )


if "--write-geometry-manifest" in sys.argv:
    write_geometry_manifest()
    print(f"wrote {GEOMETRY_MANIFEST_PATH.relative_to(ROOT)}")
    sys.exit(0)

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
