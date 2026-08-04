# Loom — Run All

Launch the eight desktop applications and their CLIs. Release binaries live
in `loom-<app>/target/release/`.

## Applications

| App | GUI binary | CLI binary |
|---|---|---|
| Writer | `loom-writer/target/release/loom-writer` | `loom-writer/target/release/loom-writer-cli` |
| Sheets | `loom-sheets/target/release/loom-sheets` | `loom-sheets/target/release/loom-sheets-cli` |
| Present | `loom-present/target/release/loom-present` | `loom-present/target/release/loom-present-cli` |
| Photo | `loom-photo/target/release/loom-photo` | `loom-photo/target/release/loom-photo-cli` |
| Motion | `loom-motion/target/release/loom-motion` | `loom-motion/target/release/loom-motion-cli` |
| Video | `loom-video/target/release/loom-video` | `loom-video/target/release/loom-video-cli` |
| Studio | `loom-studio/target/release/loom-studio` | `loom-studio/target/release/loom-studio-cli` |
| Encode | `loom-encode/target/release/loom-encode` | `loom-encode/target/release/loom-encode-cli` |

## Launch one app

```bash
loom-writer/target/release/loom-writer                # default document
loom-writer/target/release/loom-writer path/file.loomdoc   # open a Loom package
```

## Launch all apps

```bash
bash loom-bootstrap/scripts/run-apps.sh
```

## Headless / screenshot mode (used by visual QA)

All apps accept deterministic headless capture flags:

```bash
loom-writer/target/release/loom-writer \
  --screenshot /tmp/writer.png --size 1440x900 --theme dark
```

Flags: `--screenshot <png>`, `--size <WxH>`, `--theme light|dark|high-contrast`,
`--smoke`, `--palette` (open the command palette), `--journey <dir>`
(record a keyboard journey with per-step screenshots).

## CLI journeys (verified by the functional matrix)

```bash
loom-writer-cli create sample.loomdoc "Title" "Body"
loom-writer-cli validate sample.loomdoc
loom-writer-cli export-md sample.loomdoc out.md
loom-sheets-cli eval input.csv          # =SUM(...) etc.
loom-sheets-cli to-csv input.csv out.csv
loom-present-cli create deck.loomdeck "Title"
loom-present-cli pdf deck.loomdeck out.pdf
loom-encode-cli add sample.mov --preset mp4 && loom-encode-cli run
```

Every CLI prints structured output and returns non-zero on failure. All
workflows are local-first and off-network.