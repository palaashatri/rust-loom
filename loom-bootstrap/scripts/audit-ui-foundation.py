#!/usr/bin/env python3
"""Validate the unaccepted Loom UI foundation without pretending to judge beauty."""
from __future__ import annotations

import math
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "loom-design-bible/contracts/ui-foundation.toml"
TOKEN_FILE = ROOT / "loom-design-bible/tokens/loom.toml"
DESKTOP_UI = ROOT / "loom-design-bible/contracts/desktop-ui.toml"
THEME_SOURCE = ROOT / "loom-core/crates/loom-ui/ui/theme.slint"
SHEETS_OBJECTS_SOURCE = ROOT / "loom-sheets/crates/loom-sheets-app/ui/objects.slint"
TOOLBAR_ITEM_SOURCE = ROOT / "loom-core/crates/loom-ui/ui/toolbar_item.slint"
TOOLKIT_SOURCE = ROOT / "loom-core/crates/loom-ui/ui/toolkit.slint"
FOUNDATION_CONTROLS_SOURCE = ROOT / "loom-core/crates/loom-ui/ui/foundation/controls.slint"
FOUNDATION_CHROME_SOURCE = ROOT / "loom-core/crates/loom-ui/ui/foundation/chrome.slint"
FACADE = ROOT / "loom-core/crates/loom-ui/ui/foundation.slint"
FOUNDATION = ROOT / "loom-core/crates/loom-ui/ui/foundation"
GALLERY = FOUNDATION / "gallery.slint"
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
errors: list[str] = []


def relative_luminance(hex_color: str) -> float:
    channels = [int(hex_color[index : index + 2], 16) / 255 for index in (1, 3, 5)]
    linear = [value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4 for value in channels]
    return sum(value * weight for value, weight in zip(linear, (0.2126, 0.7152, 0.0722)))


def contrast_ratio(first: str, second: str) -> float:
    lighter, darker = sorted((relative_luminance(first), relative_luminance(second)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def application_accent_errors(source: str, tokens: dict, contract: dict) -> list[str]:
    """Validate the shared brand colours without recolouring document roles."""
    result: list[str] = []
    roles = ("accent", "accent-hover", "accent-pressed", "accent-ink",
             "focus", "selection", "selection-soft")
    for app in ("writer", "sheets", "present"):
        for theme in ("light", "dark"):
            label = f"{app} {theme}"
            palette = tokens.get("application-accent", {}).get(app, {}).get(theme, {})
            expected = contract.get("application-accent", {}).get(app, {}).get(theme, {})
            try:
                block = braced_block(
                    source, rf"property\s*<ApplicationAccent>\s*{app}-{theme}\s*:\s*\{{", label,
                )
                runtime = dict(re.findall(r"([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*,", block))
            except ValueError as error:
                result.append(str(error))
                runtime = {}
            valid = True
            for role in roles:
                value = palette.get(role)
                if not isinstance(value, str) or not re.fullmatch(r"#[0-9a-fA-F]{6}", value):
                    result.append(f"{label} {role} must be an opaque colour")
                    valid = False
                    continue
                if expected.get(role, "").lower() != value.lower():
                    result.append(f"{label} {role} differs between token and desktop contract")
                if runtime.get(role, "").lower() != value.lower():
                    result.append(f"{label} {role} differs between runtime and token")
            if not valid:
                continue
            for role in ("accent", "accent-hover", "accent-pressed", "selection"):
                if contrast_ratio(palette[role], palette["accent-ink"]) < 4.5:
                    result.append(f"{label} {role}/accent-ink contrast requires 4.5:1")
            chrome = tokens["palette"][theme]
            for role in ("focus", "selection"):
                for background in ("canvas", "chrome", "panel", "surface", "paper"):
                    if contrast_ratio(palette[role], chrome[background]) < 3.0:
                        result.append(f"{label} {role}/{background} contrast requires 3:1")
            foreground = palette["accent-pressed" if theme == "light" else "accent"]
            if contrast_ratio(foreground, palette["selection-soft"]) < 4.5:
                result.append(f"{label} selected control label contrast requires 4.5:1")
    return result


def slint_code_without_comments_or_strings(source: str) -> str:
    visible = list(source)
    index = 0

    def mask(start: int, end: int) -> None:
        for position in range(start, end):
            if source[position] != "\n":
                visible[position] = " "

    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            if end < 0:
                end = len(source)
            mask(index, end)
            index = end
        elif source.startswith("/*", index):
            start = index
            index += 2
            depth = 1
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            mask(start, index)
        elif source[index] in ('"', "'"):
            start = index
            quote = source[index]
            index += 1
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == quote:
                    index += 1
                    break
                else:
                    index += 1
            mask(start, min(index, len(source)))
        else:
            index += 1
    return "".join(visible)


def braced_block(source: str, declaration: str, label: str) -> str:
    match = re.search(declaration, source)
    if not match:
        raise ValueError(f"missing Slint palette scope: {label}")
    opening = source.rfind("{", match.start(), match.end())
    if opening < 0:
        raise ValueError(f"missing opening brace in Slint scope: {label}")
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : index]
    raise ValueError(f"unclosed Slint scope: {label}")


def braced_block_bounds(source: str, declaration: str, label: str) -> tuple[int, int]:
    """Return a braced scope's body offsets using the comment/string-masked source."""
    match = re.search(declaration, source)
    if not match:
        raise ValueError(f"missing Slint palette scope: {label}")
    opening = source.rfind("{", match.start(), match.end())
    if opening < 0:
        raise ValueError(f"missing opening brace in Slint scope: {label}")
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return opening + 1, index
    raise ValueError(f"unclosed Slint scope: {label}")


def direct_property_assignments(source: str, property_name: str) -> list[re.Match]:
    """Find property assignments at the current component scope, excluding children."""
    assignment = re.compile(
        rf"(?<![\w-]){re.escape(property_name)}\s*:\s*([^;]+);"
    )
    matches: list[re.Match] = []
    depth = 0
    index = 0
    while index < len(source):
        if depth == 0:
            match = assignment.match(source, index)
            if match:
                matches.append(match)
                index = match.end()
                continue
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
        index += 1
    return matches


def runtime_palette_values(source: str, theme: str) -> dict[str, str]:
    theme_scopes = {
        "light": r"(?<![\w-])export\s+global\s+Theme\b\s*\{",
        "dark": r"(?<![\w-])export\s+global\s+ThemeDark\b\s*\{",
        "high-contrast": r"(?<![\w-])export\s+global\s+ThemeHighContrast\b\s*\{",
    }
    if theme not in theme_scopes:
        raise ValueError(f"unknown runtime theme: {theme}")
    source = slint_code_without_comments_or_strings(source)
    if theme == "light":
        theme_block = braced_block(source, theme_scopes[theme], theme)
        token_block = braced_block(
            theme_block,
            r"(?<![\w-])private\s+property\s*<\s*ThemeTokens\s*>\s*light-tokens\s*:\s*\{",
            f"{theme} ThemeTokens.light-tokens",
        )
    else:
        theme_block = braced_block(source, theme_scopes[theme], theme)
        token_block = braced_block(
            theme_block,
            r"(?<![\w-])(?:in-out\s+)?property\s*<\s*ThemeTokens\s*>\s*tokens\s*:\s*\{",
            f"{theme} ThemeTokens.tokens",
        )
    palette_block = braced_block(
        token_block,
        r"(?<![\w-])palette\s*:\s*\{",
        f"{theme} ThemeTokens.palette",
    )
    roles = (
        "accent",
        "accent-hover",
        "accent-pressed",
        "accent-ink",
        "ink",
        "ink-disabled",
        "canvas",
        "canvas-alt",
        "surface",
        "surface-raised",
        "surface-sunken",
        "chrome",
        "panel",
        "paper-ink",
    )
    role_pattern = "|".join(sorted(roles, key=len, reverse=True))
    assignments = list(
        re.finditer(
            rf"(?<![a-z0-9_-])({role_pattern})\s*:", palette_block, re.IGNORECASE
        )
    )
    names = [match.group(1).lower() for match in assignments]
    values = {}
    for role in roles:
        count = names.count(role)
        if count > 1:
            raise ValueError(f"runtime theme.slint {theme} palette role {role} occurs more than once")
        if count == 0:
            raise ValueError(f"runtime theme.slint {theme} palette is missing required roles")
    for assignment in assignments:
        role = assignment.group(1).lower()
        value_match = re.match(
            r"\s*(#[0-9a-f]{6})(?=\s*(?:,|}))",
            palette_block[assignment.end() :],
            re.IGNORECASE,
        )
        if not value_match:
            raise ValueError(
                f"runtime theme.slint {theme} palette role {role} must be a six-digit opaque color literal"
            )
        values[role] = value_match.group(1).lower()
    return values


def sheets_shape_label_errors(source: str, palettes: dict, minimum: float) -> list[str]:
    """Check shape text roles against every fixed pastel fill and theme."""
    code = slint_code_without_comments_or_strings(source)
    result: list[str] = []
    component_declaration = (
        r"export\s+component\s+SheetObjectLayer\s+inherits\s+Rectangle\s*\{"
    )
    component_matches = list(re.finditer(component_declaration, code))
    if len(component_matches) != 1:
        return ["Sheets object source must define exactly one SheetObjectLayer component"]
    try:
        component_start, component_end = braced_block_bounds(
            code,
            component_declaration,
            "Sheets SheetObjectLayer",
        )
        component = code[component_start:component_end]
        repeater_declaration = (
            r"for\s+idx\s+in\s+root\.objects\.length\s*:\s*Rectangle\s*\{"
        )
        repeater_matches = list(re.finditer(repeater_declaration, component))
        if len(repeater_matches) != 1:
            return ["Sheets object layer must define exactly one object Rectangle repeater"]
        shape_rectangle_start, shape_rectangle_end = braced_block_bounds(
            component,
            repeater_declaration,
            "Sheets shape object Rectangle",
        )
    except ValueError as error:
        return [str(error)]

    shape_rectangle_code = component[shape_rectangle_start:shape_rectangle_end]
    shape_rectangle_source = source[
        component_start + shape_rectangle_start : component_start + shape_rectangle_end
    ]
    text_nodes = list(re.finditer(r"\bText\s*\{", shape_rectangle_code))
    if len(text_nodes) != 1:
        result.append("Sheets shape object must define exactly one Text child")
        return result
    shape_text_declaration = r"if\s+root\.objects\[idx\]\.kind.{0,40}:\s*Text\s*\{"
    shape_text_matches = list(re.finditer(shape_text_declaration, shape_rectangle_code))
    if len(shape_text_matches) != 1 or shape_text_matches[0].end() != text_nodes[0].end():
        result.append("Sheets shape object must define exactly one shape label Text")
        return result
    raw_text_declaration = shape_rectangle_source[
        shape_text_matches[0].start() : shape_text_matches[0].end()
    ]
    declaration_without_comments = re.sub(
        r"/\*.*?\*/|//[^\n]*",
        lambda match: "".join("\n" if character == "\n" else " " for character in match.group()),
        raw_text_declaration,
        flags=re.DOTALL,
    )
    if not re.fullmatch(
        r'\s*if\s+root\.objects\[idx\]\.kind\s*==\s*"shape"\s*:\s*Text\s*\{\s*',
        declaration_without_comments,
    ):
        result.append("Sheets shape label Text must be conditioned on kind == shape")
    shape_text = braced_block(
        shape_rectangle_code,
        shape_text_declaration,
        "Sheets shape label Text",
    )
    color_assignments = list(re.finditer(r"\bcolor\s*:\s*([^;]+);", shape_text))
    if len(color_assignments) != 1:
        result.append("Sheets shape label must define exactly one foreground color")
    else:
        expression = re.sub(r"\s+", " ", color_assignments[0].group(1)).strip()
        expected = re.compile(
            r"root\.objects\[idx\]\.fill >= 0 && root\.objects\[idx\]\.fill <= 6 "
            r"\? Theme\.palette\(\)\.paper-ink : Theme\.palette\(\)\.ink"
        )
        if not expected.fullmatch(expression):
            result.append(
                "Sheets shape label foreground must use paper-ink for colored fills and ink for unfilled shapes"
            )

    backgrounds = direct_property_assignments(shape_rectangle_code, "background")
    if len(backgrounds) != 1:
        result.append("Sheets shape object must define exactly one background expression")
        return result

    background = backgrounds[0]
    background_expression = shape_rectangle_source[background.start(1) : background.end(1)]
    fill_branches = re.findall(
        r"root\.objects\[idx\]\.fill\s*==\s*(\d+)\s*\?\s*(#[0-9a-fA-F]{6})",
        background.group(1),
    )
    fill_indices = [int(index) for index, _ in fill_branches]
    valid_fill_counts = True
    for index in range(7):
        count = fill_indices.count(index)
        if count != 1:
            valid_fill_counts = False
            result.append(f"Sheets shape background fill index {index} occurs {count} times; requires exactly once")

    expected_fills = (
        "#FECACA",
        "#FED7AA",
        "#FEF08A",
        "#BBF7D0",
        "#BFDBFE",
        "#DDD6FE",
        "#E5E7EB",
    )
    expected_mapping = "root.objects[idx].kind==\"shape\"?("
    expected_mapping += ":".join(
        f"root.objects[idx].fill=={index}?{color}"
        for index, color in enumerate(expected_fills)
    )
    expected_mapping += ":Theme.palette().surface-raised):Theme.palette().surface"
    if "".join(background_expression.split()) != expected_mapping:
        result.append("Sheets shape background must preserve its approved fill mapping and fallbacks")
    if not valid_fill_counts or fill_indices != list(range(7)):
        return result

    fills = {int(index): color.lower() for index, color in fill_branches}

    for theme in ("light", "dark", "high-contrast"):
        palette = palettes[theme]
        foreground = palette["paper-ink"].lower()
        for index, background in sorted(fills.items()):
            ratio = contrast_ratio(foreground, background)
            if ratio < minimum:
                result.append(
                    f"Sheets shape fill {index} in {theme} paper-ink contrast is {ratio:.3f}:1; "
                    f"requires {minimum:.1f}:1"
                )
        fallback_ratio = contrast_ratio(
            palette["ink"].lower(), palette["surface-raised"].lower()
        )
        if fallback_ratio < minimum:
            result.append(
                f"Sheets unfilled shape in {theme} ink contrast is {fallback_ratio:.3f}:1; "
                f"requires {minimum:.1f}:1"
            )
    return result


def toolbar_caption_errors(desktop_ui: dict, tokens: dict, theme_source: str,
                           toolbar_item_source: str) -> list[str]:
    """Keep shared toolbar action captions tied to full shared typography."""
    result: list[str] = []
    typography = desktop_ui.get("type", {})
    type_tokens = tokens.get("type", {})
    type_roles = type_tokens.get("role", {})
    leading_tokens = type_tokens.get("leading", {})
    try:
        caption_size = float(typography["caption"])
        caption_leading = float(typography["line-height-caption"])
        token_caption_size = float(type_roles["caption"])
        token_caption_leading = float(leading_tokens["caption"])
    except (KeyError, TypeError, ValueError):
        result.append("toolbar caption size and line height must be declared in shared tokens")
        return result

    if caption_size != token_caption_size:
        result.append("desktop caption size differs from the shared type caption token")
    if caption_leading != token_caption_leading:
        result.append("desktop caption line height differs from the shared leading token")

    components = desktop_ui.get("component", {})
    for name in ("labeled-toolbar-item", "icon-over-label-toolbar-item"):
        policy = components.get(name, {})
        if policy.get("label-size") != caption_size:
            result.append(f"{name} label-size must match type.caption")
        if policy.get("label-line-height") != caption_leading:
            result.append(f"{name} label-line-height must match type.line-height-caption")
        if policy.get("allow-label-elision") is not False:
            result.append(f"{name} must forbid action-caption elision")

    if desktop_ui.get("toolbar", {}).get("allow-labeled-item-elision") is not False:
        result.append("toolbar policy must forbid labeled action-caption elision")
    if desktop_ui.get("truncation", {}).get("allow-labeled-toolbar-label") is not False:
        result.append("truncation policy must forbid labeled action-caption ellipsis")
    if desktop_ui.get("forbidden", {}).get("labeled-toolbar-label-ellipsis") is not True:
        result.append("forbidden policy must reject labeled toolbar caption ellipsis")

    code = slint_code_without_comments_or_strings(theme_source)
    runtime_themes = (
        ("light", r"private\s+property\s*<ThemeTokens>\s*light-tokens\s*:\s*\{", False),
        ("dark", r"export\s+global\s+ThemeDark\s*\{", True),
        ("high-contrast", r"export\s+global\s+ThemeHighContrast\s*\{", True),
    )
    for name, declaration, nested_tokens in runtime_themes:
        try:
            theme_block = braced_block(code, declaration, f"{name} theme tokens")
            token_block = (
                braced_block(
                    theme_block,
                    r"in-out\s+property\s*<ThemeTokens>\s*tokens\s*:\s*\{",
                    f"{name} ThemeTokens",
                )
                if nested_tokens
                else theme_block
            )
            type_block = braced_block(token_block, r"typography\s*:\s*\{", f"{name} typography")
            leading_block = braced_block(token_block, r"leading\s*:\s*\{", f"{name} leading")
        except ValueError as error:
            result.append(str(error))
            continue
        for label, block, key, expected in (
            ("caption size", type_block, "caption", token_caption_size),
            ("caption leading", leading_block, "caption", token_caption_leading),
        ):
            values = re.findall(rf"\b{key}\s*:\s*([0-9]+(?:\.[0-9]+)?)px\b", block)
            if len(values) != 1 or float(values[0]) != expected:
                result.append(f"runtime theme {name} {label} must match its shared token")

    for label, declaration, expected in (
        ("scaled caption size", r"in-out\s+property\s*<TypeScale>\s*type\s*:\s*\{",
         r"caption\s*:\s*tokens\.typography\.caption\s*\*\s*max\(1\.0,\s*root\.text-scale\)"),
        ("scaled caption leading", r"out\s+property\s*<Leading>\s*leading\s*:\s*\{",
         r"caption\s*:\s*tokens\.leading\.caption\s*\*\s*max\(1\.0,\s*root\.text-scale\)"),
    ):
        try:
            block = braced_block(code, declaration, label)
        except ValueError as error:
            result.append(str(error))
            continue
        if not re.search(expected, block):
            result.append(f"Theme must scale {label} from its shared token")

    component_code = slint_code_without_comments_or_strings(toolbar_item_source)
    component_declaration = (
        r"export\s+component\s+IconOverLabelToolbarItem\s+inherits\s+Rectangle\s*\{"
    )
    try:
        component = braced_block(component_code, component_declaration, "toolbar action item")
        label_text = braced_block(component, r"label-text\s*:=\s*Text\s*\{", "toolbar caption")
    except ValueError as error:
        result.append(str(error))
    else:
        if not re.search(r"font-size\s*:\s*Theme\.type\.caption\s*;", label_text):
            result.append("toolbar action caption must use Theme.type.caption")
        if not re.search(r"min-height\s*:\s*Theme\.leading\.caption\s*;", label_text):
            result.append("toolbar action caption must use Theme.leading.caption")
        if not re.search(r"wrap\s*:\s*no-wrap\s*;", label_text):
            result.append("toolbar action captions must remain on one line")
        if re.search(r"overflow\s*:\s*elide\b", label_text):
            result.append("toolbar action captions must not elide")
        if not re.search(
            r"min-width\s*:\s*max\(48px,\s*label-text\.preferred-width\s*\+\s*16px\)",
            component,
        ):
            result.append("toolbar action width must preserve the full measured caption")

    return result


def toolbar_focus_outline_errors(desktop_ui: dict, tokens: dict, theme_source: str,
                                 sources: dict[str, str]) -> list[str]:
    """Keep toolbar state outlines inside targets using the shared geometry."""
    result: list[str] = []
    try:
        focus_ring = float(desktop_ui["controls"]["focus-ring"])
        focus_offset = float(desktop_ui["controls"]["focus-offset"])
        token_focus_ring = float(tokens["metrics"]["focus-ring"])
        token_focus_offset = float(tokens["metrics"]["focus-offset"])
    except (KeyError, TypeError, ValueError):
        result.append("toolbar focus ring and offset must be declared in shared metrics")
        return result

    if focus_ring != token_focus_ring:
        result.append("desktop focus-ring size differs from the shared focus-ring token")
    if focus_offset != token_focus_offset:
        result.append("desktop focus-offset differs from the shared focus-offset token")

    theme_code = slint_code_without_comments_or_strings(theme_source)
    themes = (
        ("light", r"private\s+property\s*<ThemeTokens>\s*light-tokens\s*:\s*\{", False),
        ("dark", r"export\s+global\s+ThemeDark\s*\{", True),
        ("high-contrast", r"export\s+global\s+ThemeHighContrast\s*\{", True),
    )
    for name, declaration, nested_tokens in themes:
        try:
            theme_block = braced_block(theme_code, declaration, f"{name} theme tokens")
            token_block = (
                braced_block(
                    theme_block,
                    r"in-out\s+property\s*<ThemeTokens>\s*tokens\s*:\s*\{",
                    f"{name} ThemeTokens",
                )
                if nested_tokens
                else theme_block
            )
            metrics = braced_block(token_block, r"metrics\s*:\s*\{", f"{name} metrics")
        except ValueError as error:
            result.append(str(error))
            continue
        for label, key, expected in (
            ("focus-ring", "focus-ring", token_focus_ring),
            ("focus-offset", "focus-offset", token_focus_offset),
        ):
            values = re.findall(rf"\b{re.escape(key)}\s*:\s*([0-9]+(?:\.[0-9]+)?)px\b", metrics)
            if len(values) != 1 or float(values[0]) != expected:
                result.append(f"runtime theme {name} {label} must match its shared token")

    def check_outline(source: str, declaration: str, name: str, *, selected: bool,
                      selection_property: str = "checked") -> None:
        code = slint_code_without_comments_or_strings(source)
        try:
            component = braced_block(code, declaration, name)
            outline = braced_block(component, r"state-outline\s*:=\s*Rectangle\s*\{", f"{name} outline")
        except ValueError as error:
            result.append(str(error))
            return
        expected_properties = (
            (r"x\s*:\s*Theme\.tokens\.metrics\.focus-offset\s*;", "x inset"),
            (r"y\s*:\s*Theme\.tokens\.metrics\.focus-offset\s*;", "y inset"),
            (r"width\s*:\s*max\(0px,\s*root\.width\s*-\s*2\s*\*\s*Theme\.tokens\.metrics\.focus-offset\s*\)", "horizontal inset"),
            (r"height\s*:\s*max\(0px,\s*root\.height\s*-\s*2\s*\*\s*Theme\.tokens\.metrics\.focus-offset\s*\)", "vertical inset"),
        )
        for pattern, label in expected_properties:
            if not re.search(pattern, outline):
                result.append(f"{name} focus/selection outline must preserve the contract {label}")
        if not re.search(r"focus\.has-focus\s*\?\s*Theme\.tokens\.metrics\.focus-ring", outline):
            result.append(f"{name} focused outline must use the shared focus-ring width")
        if selected and not re.search(rf"root\.{selection_property}\s*\?\s*1px", outline):
            result.append(f"{name} selected outline must retain its 1px contract border")

    check_outline(
        sources["toolbar_item"],
        r"export\s+component\s+IconOverLabelToolbarItem\s+inherits\s+Rectangle\s*\{",
        "toolbar action",
        selected=True,
    )
    toolkit = slint_code_without_comments_or_strings(sources["toolkit"])
    check_outline(
        toolkit,
        r"export\s+component\s+IconOnlyToolbarItem\s+inherits\s+Rectangle\s*\{",
        "icon-only toolbar action",
        selected=True,
    )
    check_outline(
        toolkit,
        r"export\s+component\s+Overflow\s+inherits\s+Rectangle\s*\{",
        "toolkit overflow action",
        selected=False,
    )
    check_outline(
        sources["foundation_controls"],
        r"export\s+component\s+LoomIconButton\s+inherits\s+Rectangle\s*\{",
        "foundation icon button",
        selected=True,
        selection_property="selected",
    )
    chrome = slint_code_without_comments_or_strings(sources["foundation_chrome"])
    try:
        overflow = braced_block(
            chrome,
            r"export\s+component\s+LoomOverflowButton\s+inherits\s+LoomIconButton\s*\{",
            "foundation overflow action",
        )
    except ValueError as error:
        result.append(str(error))
    else:
        if not re.search(r"inset-state-outline\s*:\s*true\s*;", overflow):
            result.append("foundation overflow action must enable the inset state outline")

    return result


tokens = tomllib.loads(TOKEN_FILE.read_text(encoding="utf-8"))
desktop_ui = tomllib.loads(DESKTOP_UI.read_text(encoding="utf-8"))
theme_source = THEME_SOURCE.read_text(encoding="utf-8")
try:
    toolbar_item_source = TOOLBAR_ITEM_SOURCE.read_text(encoding="utf-8")
    toolbar_focus_sources = {
        "toolbar_item": toolbar_item_source,
        "toolkit": TOOLKIT_SOURCE.read_text(encoding="utf-8"),
        "foundation_controls": FOUNDATION_CONTROLS_SOURCE.read_text(encoding="utf-8"),
        "foundation_chrome": FOUNDATION_CHROME_SOURCE.read_text(encoding="utf-8"),
    }
except OSError as error:
    errors.append(f"shared toolbar item source is missing or unreadable: {error}")
else:
    errors.extend(toolbar_caption_errors(desktop_ui, tokens, theme_source, toolbar_item_source))
    errors.extend(toolbar_focus_outline_errors(desktop_ui, tokens, theme_source, toolbar_focus_sources))
try:
    accents = THEME_SOURCE.with_name("app-accents.slint").read_text(encoding="utf-8")
except OSError as error:
    errors.append(f"application accent source is missing: {error}")
else:
    errors.extend(application_accent_errors(accents, tokens, desktop_ui))
minimum_contrast = 4.5
try:
    configured_contrast = float(tokens.get("a11y", {}).get("contrast-body"))
except (TypeError, ValueError):
    configured_contrast = minimum_contrast
    errors.append("loom.toml a11y.contrast-body must be a finite number at least 4.5:1")
else:
    if not math.isfinite(configured_contrast) or configured_contrast < minimum_contrast:
        errors.append("loom.toml a11y.contrast-body must be a finite number at least 4.5:1")
        configured_contrast = minimum_contrast
contrast_floor = max(minimum_contrast, configured_contrast)
for theme in ("light", "dark", "high-contrast"):
    palette = tokens["palette"][theme]
    contract_palette = desktop_ui["palette"][theme]
    try:
        runtime_palette = runtime_palette_values(theme_source, theme)
    except ValueError as error:
        errors.append(str(error))
        runtime_palette = {}
    disabled_backgrounds = (
        "canvas",
        "canvas-alt",
        "surface",
        "surface-raised",
        "surface-sunken",
        "chrome",
        "panel",
    )
    contrast_pairs = (
        ("accent", "accent-ink"),
        ("accent-hover", "accent-ink"),
        ("accent-pressed", "accent-ink"),
    ) + tuple((background_key, "ink-disabled") for background_key in disabled_backgrounds)
    for background_key, foreground_key in contrast_pairs:
        background = palette[background_key].lower()
        foreground = palette[foreground_key].lower()
        ratio = contrast_ratio(foreground, background)
        if ratio < contrast_floor:
            errors.append(
                f"{theme} {background_key}/{foreground_key} contrast is {ratio:.3f}:1; "
                f"requires {contrast_floor:.1f}:1"
            )
        if contract_palette[background_key].lower() != background:
            errors.append(f"{theme} {background_key} differs between loom.toml and desktop-ui.toml")
        if runtime_palette.get(background_key) != background:
            actual = runtime_palette.get(background_key, "<missing>")
            errors.append(
                f"runtime theme.slint {theme} {background_key} value {actual} does not match token {background}"
            )
    for foreground_key in ("accent-ink", "ink", "ink-disabled", "paper-ink"):
        foreground = palette[foreground_key].lower()
        if contract_palette[foreground_key].lower() != foreground:
            errors.append(f"{theme} {foreground_key} differs between loom.toml and desktop-ui.toml")
        if runtime_palette.get(foreground_key) != foreground:
            actual = runtime_palette.get(foreground_key, "<missing>")
            errors.append(
                f"runtime theme.slint {theme} {foreground_key} value {actual} does not match token {foreground}"
            )

try:
    sheets_objects_source = SHEETS_OBJECTS_SOURCE.read_text(encoding="utf-8")
except OSError as error:
    errors.append(f"Sheets object source is missing or unreadable: {error}")
else:
    errors.extend(
        sheets_shape_label_errors(sheets_objects_source, tokens["palette"], contrast_floor)
    )

config = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
status = config.get("status")
if status not in ("ACCEPTANCE_BLOCKED", "ACCEPTED"):
    errors.append(f"invalid foundation status: {status}")
if status == "ACCEPTED":
    if config.get("consumer_imports_allowed") is not True:
        errors.append("accepted foundation must allow consumer imports")
    if config.get("approved_baselines") is not True:
        errors.append("accepted foundation must have approved_baselines = true")
else:
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

baseline_root = ROOT / "loom-core/crates/loom-ui/baselines/foundation"
if status == "ACCEPTED":
    if not baseline_root.exists() or not any(path.is_file() for path in baseline_root.rglob("*.png")):
        errors.append("accepted foundation requires approved baselines under baselines/foundation")
else:
    if baseline_root.exists() and any(path.is_file() for path in baseline_root.rglob("*")):
        errors.append("foundation baselines exist before human visual acceptance")

for app in APPS:
    if app in ("sheets", "writer", "present", "photo", "motion") and status == "ACCEPTED":
        continue
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

if status == "ACCEPTED":
    print("Loom UI foundation audit: PASS (foundation ACCEPTED; approved baselines present)")
else:
    print("Loom UI foundation audit: PASS (mechanical/source contract only; visual acceptance still human-blocked)")
