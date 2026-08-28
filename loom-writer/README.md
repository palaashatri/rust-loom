# Loom Writer

Loom Writer is a calm, professional local-first word processor engineered for private, high-clarity document composition with Apple Pages-class interface refinement.

![Loom Writer main window](docs/screenshot.png)

## Core Capabilities

- **Document Chrome & Toolbars**: Clean, distraction-free document header, native global menu bar (macOS / Linux DBusMenu), and Pages-style action toolbar with centered insert group (`Table`, `Chart`, `Text`, `Shape`, `Media`, `Comment`).
- **Typography & Formatting Inspector**: Structured right-hand inspector with Paragraph Styles (`Body`, `H1`, `H2`, `Title`), font metrics, inline `[B][I][U][S]` controls, alignment segments, indentations, and list spacing.
- **Visual Template Chooser**: Categorized template selection modal with true A4/Letter portrait previews (`Blank`, `Report`, `Letter`, `CV`).
- **Open Package Format**: Inspectable versioned `.loomdoc` storage with zero telemetry or cloud dependency.
- **Export & Recovery**: Deterministic PDF export and atomic snapshot journal crash recovery.

## Visual QA Status

- **Status**: **PASS** (Toolkit & Design System Compliant).
- **Layout**: Centered paper canvas with realistic elevation drop shadow against the dark backdrop.
- **Controls**: Full bidirectional binding between selection state and inspector controls.

## Development

```sh
cargo test --manifest-path loom-writer/Cargo.toml
cargo run --manifest-path loom-writer/Cargo.toml -p loom-writer-app
# Headless QA capture:
cargo build --manifest-path loom-writer/Cargo.toml
```
