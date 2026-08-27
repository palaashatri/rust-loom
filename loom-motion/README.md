# Loom Motion

Loom Motion is a motion graphics and keyframe animation studio engineered for high-precision timeline pacing, transform curves, and vector animation.

![Loom Motion main window](docs/screenshot.png)

## Core Capabilities

- **Keyframe Motion Composition**: Transform parameter animation (Position, Scale, Rotation, Opacity) with easing curves.
- **Timeline & Transport**: Real composition clock with frame-accurate transport controls (`Play/Pause`, `Loop`, `Timecode display`).
- **Layers & Hierarchy**: Multi-layer motion stack with parenting, transform inheritance, and solo/mute toggles.
- **Vector Frame Export**: Deterministic high-fidelity SVG frame and vector sequence renderer.
- **Global Menu Bar**: Native macOS NSMenu and Linux DBusMenu global desktop menus (`Cmd/Ctrl+E` SVG export, `Cmd/Ctrl+Shift+N` new layer).
- **Command Palette**: `Cmd/Ctrl+K` searchable command palette with zero-latency response.

## Visual QA Status

- **Status**: **PASS** (Toolkit & Design System Compliant).
- **Layout**: Professional motion workspace with stage canvas backdrop, collapsible keyframe drawer, and transform inspector.
- **Chrome**: Unified `DocumentChrome`, `ToolbarGroup`, and `ToolkitStatusBar`.

## Development

```sh
cargo test --manifest-path loom-motion/Cargo.toml
cargo run --manifest-path loom-motion/Cargo.toml -p loom-motion-app
# Headless QA capture:
cargo build --manifest-path loom-motion/Cargo.toml --features visual-qa
```
