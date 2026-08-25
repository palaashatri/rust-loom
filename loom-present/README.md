# Loom Present

Deck editor with masters, transitions, presenter session, PDF export, `.loomdeck` packages.

![Loom Present main window](docs/screenshot.png)

## Visual QA (macOS, software renderer, 1280×800, 2026-08-25)

- - DEFECT: Inspector bottom-right clips/overlaps alignment controls at 800px height.
- - NOTE: sample deck copy references Linux while running on macOS; make platform-neutral.
- - Status bar honest: slides, validation issues, undo state.

## Development

```sh
cargo test --workspace
cargo run -p loom-present-app
# Headless QA capture (dev-only surface):
cargo build -p loom-present-app --features visual-qa
```
