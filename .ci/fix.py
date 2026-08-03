import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
subprocess.run(
    ["rustup", "toolchain", "install", "stable", "--profile", "minimal", "--component", "rustfmt"],
    cwd=root,
    check=True,
)
for workspace in ("loom-motion", "loom-encode"):
    subprocess.run(
        ["cargo", "+stable", "fmt", "--all"],
        cwd=root / workspace,
        check=True,
    )
