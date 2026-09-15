import json
import os
from pathlib import Path
import subprocess
import time

root = Path('/home/patri/code/rust-loom')
out = root / '.work/uiux-audit-2026-09-14'
env = os.environ.copy()
env.update(PATH='/tmp/loom-cargo/bin:' + env['PATH'], RUSTUP_HOME='/tmp/loom-rustup',
           CARGO_HOME='/tmp/loom-cargo', CARGO_BUILD_JOBS='2',
           CARGO_TARGET_DIR=str(root / 'loom-sheets/target'),
           PKG_CONFIG_PATH='/tmp/loom-pkgconfig', LIBRARY_PATH='/tmp/loom-libs',
           XDG_STATE_HOME=str(out / 'native-state'))
records = []
for app in ('writer', 'present', 'photo', 'motion', 'video', 'studio', 'encode'):
    (out / app).mkdir(exist_ok=True)
    command = ['cargo', 'build', '--manifest-path', f'loom-{app}/Cargo.toml',
               '--locked', '--offline', '-p', f'loom-{app}-app']
    print('Building ' + app, flush=True)
    with (out / 'builds' / (app + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=log)
    records.append({'app': app, 'build_command': command, 'build_exit': result.returncode})
    if result.returncode == 0:
        binary = root / 'loom-sheets/target/debug' / ('loom-' + app)
        for number, size, theme, flags in [(1, '1440x900', 'light', []),
                                            (2, '1024x720', 'light', []),
                                            (3, '1440x900', 'dark', []),
                                            (4, '1440x900', 'light', ['--palette'])]:
            filename = f'{number:02d}-{size}-{theme}' + ('-palette' if flags else '') + '.png'
            command = [str(binary), '--screenshot', str(out / app / filename),
                       '--size', size, '--theme', theme] + flags
            with (out / app / (filename + '.log')).open('w') as log:
                try:
                    result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=log, timeout=45)
                    code = result.returncode
                except subprocess.TimeoutExpired:
                    code = 'timeout'
            records.append({'app': app, 'capture': filename, 'command': command, 'exit': code})
            print(f'{app} {filename}: {code}', flush=True)
    else:
        print(app + ' build failed; inspect log', flush=True)
    (out / 'capture-manifest.json').write_text(json.dumps(records, indent=2) + '\n')
