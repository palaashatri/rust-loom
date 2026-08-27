# Loom Video

Loom Video is a local-first non-linear video editor (NLE) providing multi-track timeline editing, waveform previews, clip trimming, and local FFmpeg delivery pipelines.

![Loom Video main window](docs/screenshot.png)

## Core Capabilities

- **Multi-Track NLE Timeline**: Video and audio tracks with ripple, roll, slip, and slide editing tools.
- **Clip Trimming & Splitting**: Split at playhead (`Cmd/Ctrl+B`), precision In/Out trimming, and snap-to-grid (`Snap` mode).
- **Media Bin & Inspector**: Media bin browser with duration/codec probing, and structured timeline inspector.
- **Local Engine & Export**: Direct FFmpeg pipeline integration for hardware-accelerated timeline concatenation and H.264 exports.
- **Global Menu Bar**: Native macOS NSMenu and Linux DBusMenu global desktop menus (`Cmd/Ctrl+E` export, `Cmd/Ctrl+B` blade/split).
- **Command Palette**: `Cmd/Ctrl+K` instant command dispatch.

## Visual QA Status

- **Status**: **PASS** (Toolkit & Design System Compliant).
- **Layout**: Dark preview monitor surface, multi-track timeline lanes, and media sidebar.
- **Chrome**: Unified `DocumentChrome`, `ToolbarGroup`, and `ToolkitStatusBar`.

## Development

```sh
cargo test --manifest-path loom-video/Cargo.toml
cargo run --manifest-path loom-video/Cargo.toml -p loom-video-app
# Headless QA capture:
cargo build --manifest-path loom-video/Cargo.toml --features visual-qa
```
