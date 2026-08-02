from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "loom-photo/crates/loom-photo-core/src/lib.rs",
    """        for channel in 0..3 {\n            let value = pixel[channel] as f32 * alpha + 255.0 * (1.0 - alpha);\n            rgb.push(value.round().clamp(0.0, 255.0) as u8);\n        }""",
    """        for source in pixel.iter().take(3) {\n            let value = *source as f32 * alpha + 255.0 * (1.0 - alpha);\n            rgb.push(value.round().clamp(0.0, 255.0) as u8);\n        }""",
)

for path in (
    "loom-studio/crates/loom-studio-app/ui/app.slint",
    "loom-video/crates/loom-video-app/ui/app.slint",
):
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    text = text.replace("index % 2", "Math.mod(index, 2)")
    text = text.replace("Math.floor(seconds) % 60", "Math.mod(Math.floor(seconds), 60)")
    if " % " in text:
        raise SystemExit(f"unsupported modulo expression remains in {path}")
    file.write_text(text, encoding="utf-8")

replace_once(
    "loom-core/crates/loom-ui/ui/smoke.slint",
    'import { Theme } from "theme.slint";\n',
    'import { Theme } from "theme.slint";\nexport { Theme } from "theme.slint";\n',
)
