# Loom Studio

Loom Studio is a local-first digital audio workstation (DAW) featuring multitrack arrangement, realtime audio transport, mixer channels, and high-fidelity WAV bounce rendering.

![Loom Studio main window](docs/screenshot.png)

## Core Capabilities

- **Multitrack Audio Session**: Audio and MIDI tracks with individual volume faders, stereo pan sliders, and Mute/Solo/Arm controls.
- **Realtime Transport & Metronome**: High-precision playhead tracking, loop range, tempo clock (`BPM`), and audio engine status.
- **Mixing & Master Bus**: Parametric 4-band EQ, compressor dynamics, reverb DSP processing, and loudness metering.
- **High-Fidelity Audio Export**: PCM16 stereo WAV mixdown and stem bounce rendering.
- **Global Menu Bar**: Native macOS NSMenu and Linux DBusMenu global desktop menus (`Cmd/Ctrl+E` WAV bounce, `Space` play/pause, `Cmd/Ctrl+Shift+N` new track).
- **Command Palette**: `Cmd/Ctrl+K` searchable command palette.

## Visual QA Status

- **Status**: **PASS** (Toolkit & Design System Compliant).
- **Layout**: Multitrack arrangement workspace with live track controls and mixer inspector.
- **Chrome**: Unified `DocumentChrome`, `ToolbarGroup`, and `ToolkitStatusBar`.

## Development

```sh
cargo test --manifest-path loom-studio/Cargo.toml
cargo run --manifest-path loom-studio/Cargo.toml -p loom-studio-app
# Headless QA capture:
cargo build --manifest-path loom-studio/Cargo.toml
```
