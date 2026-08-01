# Run All

## Applications with binaries (writer, sheets)

```bash
# Headless smoke (writes a temp PNG, exits 0 on success)
loom-writer/target/debug/loom-writer --smoke
loom-sheets/target/debug/loom-sheets --smoke

# Screenshot (headless, deterministic software renderer)
loom-writer/target/debug/loom-writer --screenshot /tmp/writer.png --size 1280x800 --theme light
loom-sheets/target/debug/loom-sheets --screenshot /tmp/sheets.png --size 1280x800 --theme dark
# themes: light | dark | high-contrast

# GUI (requires a display; headless systems must use Xvfb/Wayland headless)
loom-writer/target/debug/loom-writer --open path/to/doc.loomdoc
loom-sheets/target/debug/loom-sheets --open path/to/sheet.loomtable
```

## CLI tools

```bash
loom-writer/target/debug/loom-writer-cli --help     # document creation/export CLI
loom-sheets/target/debug/loom-sheets-cli --help     # sheet/CSV CLI
```

## Orchestrated

```bash
cd loom-bootstrap
bash scripts/run-apps.sh        # smoke-run every implemented app (PASS = rc 0)
```

## Not yet runnable

loom-present, loom-photo, loom-motion, loom-video, loom-studio, loom-encode —
specification-only repos; `run-apps.sh` reports SKIP until binaries exist.

## Offline operation

All implemented workflows (create/edit/save/open/export/smoke/screenshot) run with no
network. Verified in a `--network none` Docker container (`scripts/docker-offline-test.sh`).
