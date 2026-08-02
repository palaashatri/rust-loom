from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
THEME = ROOT / "loom-core/crates/loom-ui/ui/theme.slint"


def add_content_tokens() -> None:
    text = THEME.read_text()
    if "paper: color," not in text:
        text = text.replace(
            "    grid-minor: color,\n}",
            "    grid-minor: color,\n    paper: color,\n    paper-ink: color,\n    paper-muted: color,\n    paper-line: color,\n    media-stage: color,\n    content-accent: color,\n    content-accent-soft: color,\n    content-cool: color,\n    content-cool-soft: color,\n    meter-good: color,\n    meter-warn: color,\n}",
        )
        insertions = {
            "            grid-minor: #d9d9d5,": """            grid-minor: #d9d9d5,
            paper: #fbfaf6,
            paper-ink: #1d1d1f,
            paper-muted: #55565b,
            paper-line: #d6d4ce,
            media-stage: #050608,
            content-accent: #c66f43,
            content-accent-soft: #eeeae3,
            content-cool: #35576c,
            content-cool-soft: #e8edf0,
            meter-good: #58b478,
            meter-warn: #e0a24b,""",
            "            grid-minor: #24262b,": """            grid-minor: #24262b,
            paper: #fbfaf6,
            paper-ink: #1d1d1f,
            paper-muted: #55565b,
            paper-line: #d6d4ce,
            media-stage: #050608,
            content-accent: #c66f43,
            content-accent-soft: #eeeae3,
            content-cool: #35576c,
            content-cool-soft: #e8edf0,
            meter-good: #58b478,
            meter-warn: #e0a24b,""",
            "            grid-minor: #777777,": """            grid-minor: #777777,
            paper: #ffffff,
            paper-ink: #000000,
            paper-muted: #222222,
            paper-line: #000000,
            media-stage: #000000,
            content-accent: #ffd84d,
            content-accent-soft: #ffffff,
            content-cool: #003cff,
            content-cool-soft: #ffffff,
            meter-good: #00ff66,
            meter-warn: #ffd84d,""",
        }
        for marker, replacement in insertions.items():
            if marker not in text:
                raise RuntimeError(f"missing theme marker {marker}")
            text = text.replace(marker, replacement, 1)
    THEME.write_text(text.rstrip() + "\n")


def replace_header(path: Path, app_name: str, title_property: str, icon: str, context: str, comment_marker: str) -> None:
    text = path.read_text()
    import_line = re.search(r'import \{([^}]*)\} from "components\.slint";', text, re.S)
    if not import_line:
        raise RuntimeError(f"missing components import in {path}")
    imports = import_line.group(1)
    if "AppHeader" not in imports:
        imports = " AppHeader," + imports
        text = text[: import_line.start(1)] + imports + text[import_line.end(1) :]

    layout = text.index("    VerticalLayout {")
    start = text.index("        Rectangle {", layout)
    end = text.index(comment_marker, start)
    header = f'''        AppHeader {{
            app-name: "{app_name}";
            document-title: {title_property};
            icon: "{icon}";
            context: "{context}";
            state-text: "Saved locally";
            state-tone: 0;
        }}

'''
    text = text[:start] + header + text[end:]
    path.write_text(text.rstrip() + "\n")


def semantic_token(value: str) -> str:
    raw = value[1:]
    if len(raw) == 3:
        raw = "".join(ch * 2 for ch in raw)
    if len(raw) == 4:
        raw = "".join(ch * 2 for ch in raw)
    alpha = 255
    if len(raw) == 8:
        alpha = int(raw[6:8], 16)
        raw = raw[:6]
    r, g, b = int(raw[0:2], 16), int(raw[2:4], 16), int(raw[4:6], 16)
    if alpha < 210:
        return "Theme.palette().scrim" if alpha >= 80 else "Theme.palette().shadow"
    known = {
        (251, 250, 246): "Theme.palette().paper",
        (29, 29, 31): "Theme.palette().paper-ink",
        (85, 86, 91): "Theme.palette().paper-muted",
        (214, 212, 206): "Theme.palette().paper-line",
        (5, 6, 8): "Theme.palette().media-stage",
        (198, 111, 67): "Theme.palette().content-accent",
        (238, 234, 227): "Theme.palette().content-accent-soft",
        (53, 87, 108): "Theme.palette().content-cool",
        (232, 237, 240): "Theme.palette().content-cool-soft",
        (245, 245, 242): "Theme.palette().ink",
    }
    if (r, g, b) in known:
        return known[(r, g, b)]
    spread = max(r, g, b) - min(r, g, b)
    luminance = (r * 299 + g * 587 + b * 114) / 1000
    if spread < 18:
        if luminance > 235:
            return "Theme.palette().paper"
        if luminance > 178:
            return "Theme.palette().paper-line"
        if luminance > 78:
            return "Theme.palette().paper-muted"
        if luminance > 22:
            return "Theme.palette().paper-ink"
        return "Theme.palette().media-stage"
    if r > 170 and g > 125 and b < 120:
        return "Theme.palette().meter-warn"
    if r > g * 1.12 and g > b * 1.08:
        return "Theme.palette().content-accent"
    if b > r * 1.12 or (b > g * 1.08 and r < 150):
        return "Theme.palette().content-cool"
    if g > r * 1.08 and g > b * 1.08:
        return "Theme.palette().meter-good"
    if r > 150 and g < 115:
        return "Theme.palette().danger"
    return "Theme.palette().content-accent-soft"


def replace_hardcoded_colors() -> None:
    paths = [
        ROOT / "loom-present/crates/loom-present-app/ui/app.slint",
        ROOT / "loom-photo/crates/loom-photo-app/ui/app.slint",
        ROOT / "loom-video/crates/loom-video-app/ui/app.slint",
        ROOT / "loom-studio/crates/loom-studio-app/ui/app.slint",
    ]
    pattern = re.compile(r"#[0-9a-fA-F]{3,8}\b")
    for path in paths:
        text = path.read_text()
        text = pattern.sub(lambda match: semantic_token(match.group(0)), text)
        path.write_text(text.rstrip() + "\n")


def main() -> None:
    add_content_tokens()
    replace_header(
        ROOT / "loom-writer/crates/loom-writer-app/ui/app.slint",
        "Loom Writer",
        "root.doc-title",
        "document",
        "Document",
        "        // Standard Action Toolbar",
    )
    replace_header(
        ROOT / "loom-sheets/crates/loom-sheets-app/ui/app.slint",
        "Loom Sheets",
        "root.sheet-name",
        "table",
        "Spreadsheet",
        "        // Action Toolbar",
    )
    replace_hardcoded_colors()


if __name__ == "__main__":
    main()
