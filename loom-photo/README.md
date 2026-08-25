# Loom Photo

Layer-based raster editor: adjustments, masks, filters, non-destructive preview, PNG/JPEG export.

![Loom Photo main window](docs/screenshot.png)

## Visual QA (macOS, software renderer, 1280×800, 2026-08-25)

- - Minor: layer names truncate with ellipsis at panel width.
- - Minor: mode hint bar overlays canvas top edge.
- - Status bar honest: layers, pixel payloads, nondestructive preview.

## Development

```sh
cargo test --workspace
cargo run -p loom-photo-app
# Headless QA capture (dev-only surface):
cargo build -p loom-photo-app --features visual-qa
```
