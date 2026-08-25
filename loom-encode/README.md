# Loom Encode

Local FFmpeg delivery queue: presets, progress, cancellation, deterministic plans.

![Loom Encode main window](docs/screenshot.png)

## Visual QA (macOS, software renderer, 1280×800, 2026-08-25)

- - Clean capture. Honest: local FFmpeg backend version shown; queue/editable paths stated.
- - Minor: large 0% readout dominates the center panel for queued jobs.

## Development

```sh
cargo test --workspace
cargo run -p loom-encode-app
# Headless QA capture (dev-only surface):
cargo build -p loom-encode-app --features visual-qa
```
