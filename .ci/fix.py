from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
path = ROOT / "loom-sheets/crates/loom-sheets-app/ui/app.slint"
text = path.read_text()
old = "                    font-style: italic;\n"
if text.count(old) != 1:
    raise RuntimeError(f"expected one Sheets font-style declaration, found {text.count(old)}")
path.write_text(text.replace(old, "", 1).rstrip() + "\n")
