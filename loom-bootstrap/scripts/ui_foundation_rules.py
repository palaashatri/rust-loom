#!/usr/bin/env python3
"""Shared palette, source and accent validation helpers for the UI foundation audit."""
from __future__ import annotations

import re

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


