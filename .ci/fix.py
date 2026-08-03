import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]

studio_path = root / "loom-studio/crates/loom-studio-app/src/audio_io.rs"
studio = studio_path.read_text(encoding="utf-8")
replacements = (
    (
        '''        let output_device_name = output_device
            .name()
            .unwrap_or_else(|_| "Default output".into());''',
        '''        let output_device_name = output_device.to_string();''',
        "output device name",
    ),
    (
        "                input_rate = config.sample_rate.0;",
        "                input_rate = config.sample_rate;",
        "input sample rate",
    ),
    (
        "                input_device_name = input_device.name().ok();",
        "                input_device_name = Some(input_device.to_string());",
        "input device name",
    ),
    (
        "    let device_rate = config.sample_rate.0;",
        "    let device_rate = config.sample_rate;",
        "output sample rate",
    ),
)
for old, new, label in replacements:
    count = studio.count(old)
    if count != 1:
        raise RuntimeError(f"Studio {label}: expected one match, found {count}")
    studio = studio.replace(old, new, 1)
studio_path.write_text(studio, encoding="utf-8", newline="\n")

plugin_path = root / "loom-plugin-sdk/crates/loom-plugin-host/src/lifecycle.rs"
plugin = plugin_path.read_text(encoding="utf-8")
old = "    let mut entry = archive\n"
new = "    let entry = archive\n"
if plugin.count(old) != 1:
    raise RuntimeError(f"Plugin archive entry: expected one match, found {plugin.count(old)}")
plugin_path.write_text(plugin.replace(old, new, 1), encoding="utf-8", newline="\n")

subprocess.run(
    ["rustup", "toolchain", "install", "stable", "--profile", "minimal", "--component", "rustfmt"],
    cwd=root,
    check=True,
)
for workspace in ("loom-studio", "loom-plugin-sdk"):
    subprocess.run(["cargo", "+stable", "fmt", "--all"], cwd=root / workspace, check=True)
