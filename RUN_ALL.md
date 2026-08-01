# Run all

All eight application binaries support the same headless QA contract:

```bash
loom-writer/target/release/loom-writer --smoke
loom-sheets/target/release/loom-sheets --smoke
loom-present/target/release/loom-present --smoke
loom-photo/target/release/loom-photo --smoke
loom-motion/target/release/loom-motion --smoke
loom-video/target/release/loom-video --smoke
loom-studio/target/release/loom-studio --smoke
loom-encode/target/release/loom-encode --smoke
```

Capture a deterministic software-rendered image with, for example:

```bash
loom-present/target/release/loom-present --screenshot /tmp/present.png --size 1280x800 --theme light
loom-encode/target/release/loom-encode --screenshot /tmp/encode.png --size 1280x800 --theme dark
```

`--open <path>` loads and validates the corresponding package in both
headless screenshots and the initial GUI state. Missing files and malformed
archives exit nonzero with an app-specific error. Package extensions are
`.loomdoc`, `.loomtable`, `.loomdeck`, `.loomphoto`, `.loommotion`,
`.loomvideo`, `.loomstudio`, and `.loomencode`.

The orchestrated smoke command is:

```bash
cd loom-bootstrap
bash scripts/run-apps.sh
```

It exits nonzero if an expected binary is missing, a process times out, or a
smoke command fails. Fresh evidence has all 8 applications exiting `0`.

CLI binaries remain available in the writer, sheets, and each newer app
workspace for package creation/inspection. See each repository README for the
app-specific subcommands.
