# Loom Sheets

Local-first spreadsheet: multi-sheet workbooks, formulas, validation, CSV import/export, `.loomtable` packages.

![Loom Sheets main window](docs/screenshot.png)

## Visual QA (macOS, software renderer, 1280×800, 2026-08-25)

- - Clean capture. Cosmetic: fixed 8-column grid leaves dead canvas at wide sizes.
- - Status bar honest: cell and formula counts.

## Development

```sh
cargo test --workspace
cargo run -p loom-sheets-app
# Headless QA capture (dev-only surface):
cargo build -p loom-sheets-app --features visual-qa
```
