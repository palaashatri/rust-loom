# Loom Motion

Keyframe composition editor: transform inspector, timeline, guides, SVG frame export.

![Loom Motion main window](docs/screenshot.png)

## Visual QA (macOS, software renderer, 1280×800, 2026-08-25)

- - DEFECT: sample composition opens with title layer at 0% opacity at t=0, so the canvas renders empty/black until scrubbed — reads as broken on first launch.
- - Minor: PAUSED/timecode chip overlaps canvas top edge.
- - Status bar honest: layer count, selection, Offline.

## Development

```sh
cargo test --workspace
cargo run -p loom-motion-app
# Headless QA capture (dev-only surface):
cargo build -p loom-motion-app --features visual-qa
```
