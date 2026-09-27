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
        "ink-disabled",
        "canvas",
        "canvas-alt",
        "surface",
        "surface-raised",
        "surface-sunken",
        "chrome",
        "panel",
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


tokens = tomllib.loads(TOKEN_FILE.read_text(encoding="utf-8"))
desktop_ui = tomllib.loads(DESKTOP_UI.read_text(encoding="utf-8"))
theme_source = THEME_SOURCE.read_text(encoding="utf-8")
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
    for foreground_key in ("accent-ink", "ink-disabled"):
        foreground = palette[foreground_key].lower()
        if contract_palette[foreground_key].lower() != foreground:
            errors.append(f"{theme} {foreground_key} differs between loom.toml and desktop-ui.toml")
        if runtime_palette.get(foreground_key) != foreground:
            actual = runtime_palette.get(foreground_key, "<missing>")
            errors.append(
                f"runtime theme.slint {theme} {foreground_key} value {actual} does not match token {foreground}"
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
