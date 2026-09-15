import json
import os
from pathlib import Path
import subprocess

root = Path('/home/patri/code/rust-loom')
out = root / '.work/uiux-audit-2026-09-14'
env = os.environ.copy()
env.update(PATH='/tmp/loom-cargo/bin:' + env['PATH'], RUSTUP_HOME='/tmp/loom-rustup',
           CARGO_HOME='/tmp/loom-cargo', CARGO_BUILD_JOBS='2',
           CARGO_TARGET_DIR=str(root / 'loom-sheets/target'),
           PKG_CONFIG_PATH='/tmp/loom-pkgconfig', LIBRARY_PATH='/tmp/loom-libs')
cmd = ['cargo', 'build', '--manifest-path', 'loom-sheets/Cargo.toml', '--locked', '--offline', '-p', 'loom-sheets-app']
result = subprocess.run(cmd, cwd=root, env=env)
records = [{'build_command': cmd, 'build_exit': result.returncode}]
if result.returncode == 0:
    binary = root / 'loom-sheets/target/debug/loom-sheets'
    states = [
        ('01-start-light-1440.png', '1440x900', []),
        ('02-compact-light-1024.png', '1024x720', []),
        ('03-palette-light.png', '1440x900', ['--palette']),
        ('04-template-chooser.png', '1024x720', ['--template-chooser']),
        ('05-chart.png', '1440x900', ['--chart']),
        ('06-objects.png', '1024x720', ['--objects']),
        ('07-dark.png', '1440x900', ['--theme', 'dark']),
        ('08-high-contrast.png', '1024x720', ['--theme', 'high-contrast']),
    ]
    for name, size, flags in states:
        path = out / 'sheets' / name
        if path.exists():
            old = path.parent / 'initial-build' / name
            old.parent.mkdir(exist_ok=True)
            path.rename(old)
        cmd = [str(binary), '--screenshot', str(path), '--size', size] + flags
        result = subprocess.run(cmd, cwd=root, env=env, timeout=60)
        records.append({'capture': name, 'command': cmd, 'exit': result.returncode})
        print(name, result.returncode, flush=True)
(out / 'sheets-capture-manifest.json').write_text(json.dumps(records, indent=2) + '\n')
