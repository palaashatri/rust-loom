# macOS Native UI Polish, Design Bible & Logo Audit — 2026-11-28

Dated audit record for branch `macos_native_ui_polish` at commit `771c140b` on macOS 27.2.1 (26B131). Application quality and gate scheduling remain solely in [current.md](../../workflow/current.md).

## 1. Executive Verdict

- **Design Bible compliance:** Core principles, light/dark/high-contrast palettes, reduced-motion wiring, responsive toolbar breakpoints (`1180`/`1320`), and visual QA contracts are implemented. Key gaps: `tokens/loom.toml:250` specifies `Noto Sans` while `theme.slint:9-15,164-168` bundles and defaults to `Inter`; light-mode `surface` and `surface-raised` are both `#FFFFFF`; apps still import legacy `toolkit.slint`/`components.slint` and app-local `font_picker.slint` rather than adopting all 27 `ui-foundation.toml` primitives; 3 pointer handle targets `< 24 px` remain under `GOV-06`.
- **Application logo visibility:** `.icns` (1024×1024) + `Info.plist` (`CFBundleIconFile`) + `.dmg` packaging in `loom-bootstrap/packaging/release.py` and `.github/workflows/release.yml` are implemented. However, **raw/unbundled binaries** show the generic macOS exec icon in Dock/Cmd+Tab because `macos_window.rs` never calls `NSApplication::setApplicationIconImage` (and `NSImage` is not enabled in `loom-desktop/Cargo.toml:15`). Additionally, `LoomPresent.app` triggers the macOS 27.2.1 IconServices squircle-jail (~30% smaller inside a dark glass bezel) due to an oversized/asymmetric `contentBox` (`[73, 78, 878, 843]`). Start Hub / Welcome Guide do not consistently render app icons (and `audit-ui-foundation.py` does not block them).
- **macOS native polish & performance:** `NSMenu` menubar, `NSAppearance` + opaque titlebar, and `.AppleSystemUIFont` UI font are implemented. Major native gaps: stacked titlebar + Slint toolbar (`~76–84 px` vs Apple's unified `~52 px`), `*` dirty suffix instead of `setDocumentEdited:` / `setRepresentedURL:`, `Ctrl+Alt+...` shortcut hints in Writer Slint popovers, collapsed overlapping `B/I/U/S` glyphs and duplicate `A` / slide-count in Present, unreachable Slide Sorter in Present, and OpenGL (`renderer-femtovg`) instead of Metal (`renderer-skia` / `renderer-femtovg-wgpu`) per `docs/rules/platforms.md` §18.

## 2. Design Bible Compliance Matrix

| Chapter / Contract | Status | Evidence (`file:line`) | Action Needed |
|---|---|---|---|
| Principles & Anti-Patterns (`GOV-05`) | IMPLEMENTED | `loom-design-bible/README.md:7-10`, `desktop-ui.toml:14-22`, `ui-foundation.toml:66-71` | None |
| Tokens & Color (`tokens/loom.toml`, `COLOR.md`) | PARTIAL | `tokens/loom.toml:20-140,316-347`, `theme.slint:20-32,90-103,194-201` | Differentiate light `surface` vs `surface-raised` (`#FFFFFF`); reconcile archived prose (`GOV-02`) |
| Typography (`TYPOGRAPHY.md`) | DRIFT/CONTRADICTION | `tokens/loom.toml:250-253`, `theme.slint:9-15,164-168` | Align `tokens/loom.toml` (`Noto Sans`) vs `theme.slint` (`Inter`) |
| Motion & Reduced Motion (`MOTION.md`) | IMPLEMENTED | `desktop-ui.toml:167-174`, `theme.slint:83,127,252,294`, `components.slint:96,141,347` | None |
| Components & UI Foundation (`ui-foundation.toml`) | PARTIAL | `ui-foundation.toml:12-40`, `loom-ui/ui/foundation/`, `writer/present font_picker.slint` | Migrate remaining `toolkit.slint`/`components.slint` & app-local `font_picker.slint` |
| Windows, Toolbars, Menus, Palette, Dialogs | IMPLEMENTED | `desktop-ui.toml:49-71,119-145`, `macos_window.rs:34-92`, `macos_menu.rs:21,209` | Unify macOS titlebar/toolbar & native dirty dot/proxy icon |
| Accessibility (`ACCESSIBILITY.md`, `GOV-06`) | PARTIAL | `loom-design-bible/README.md:89-95`, `desktop-ui.toml:103-104,144,151,165` | Resolve 3 `< 24 px` handle targets (`GOV-06`) |
| Visual QA & Performance (`VISUAL_QA.md`) | IMPLEMENTED | `desktop-ui.toml:27-41`, `ui-foundation.toml:1-10,53-71`, `geometry-manifest.toml` | Complete `PERF-01` native GPU/RSS review |

## 3. Application Logo Visibility Audit

| Surface | Status | Evidence | Action Needed |
|---|---|---|---|
| macOS runtime Dock / Cmd+Tab (unbundled) | MISSING | `macos_window.rs:1-120`, `loom-desktop/Cargo.toml:15`, `app.slint` (`Window { icon }`) | Enable `NSImage` in `loom-desktop/Cargo.toml` and call `NSApplication::setApplicationIconImage` |
| `.app` bundle `Info.plist` & `.icns` | IMPLEMENTED (1 squircle defect) | `loom-ui/ui/app-icons/loom-{writer,sheets,present}.icns` (1024×1024), `release.py:553-641` | Re-center/pad `loom-present.png` (`contentBox [73,78,878,843]`) to avoid macOS 27.2.1 squircle-jail |
| `.github/workflows/release.yml` vs `release.py` | PARTIAL | `.github/workflows/release.yml:92-100,311-333`, `release.py:553-641` | `.dmg` artifacts are built in CI (7-day upload only); `publish_nightly` / `publish_version_tag` only publish the 6 bare-binary portable archives (`loom-nightly-...-macos-arm64.tar.gz`), omitting the macOS `.dmg` with `.app` bundles from GitHub Releases |
| Start Hub / Welcome Guide / About | MISSING | `start_hub.slint`, `welcome_guide.slint`, `audit-ui-foundation.py:200,369,381` | Add app-icon branding where missing (`audit-ui-foundation.py` allows it) |
| `README.md` | PARTIAL | `README.md:7-17` | Optional: add app icon badges alongside screenshots |

## 4. Element-by-Element Comparison Matrices

### Apple Pages 14.5 vs Loom Writer
- **Titlebar & Traffic Lights (Major):** Stacked `~76 px` chrome with `*` dirty suffix vs Pages' unified `~52 px` titlebar+toolbar with native dirty dot & proxy icon.
- **Toolbar & Popovers (Major):** Slint Insert/Style popovers show `Ctrl+Alt+...` instead of `Cmd+Opt+...` (while Command Palette and `NSMenu` use `Cmd`).
- **Inspector & Font Picker (Minor):** Table + Text controls stack vertically in one pane; inline accordion Font Picker pushes lower controls off-screen. `1st Col` toggle lacks pill chrome matching `Header`/`Banded`, and stacked Table+Text clips `Left | Center | Right` at 900px height.
- **Canvas, Status Bar, Command Palette (None):** Crisp Retina typography, clean table rendering, Light/Dark themes, and Command Palette are strong.

### Apple Numbers 14.5 vs Loom Sheets
- **Titlebar & Traffic Lights (High):** Stacked `32 px` titlebar + `52 px` Slint toolbar (`~84 px`) with `*` suffix vs Numbers' unified `52 px` header.
- **Charts & Object Chrome (Medium):** Floating chart card overlay with keyboard callout box vs Numbers' borderless canvas chart; Dark Mode chart card header (`Bar X`) renders as a dark box inside white chart card.
- **Welcome Guide (Medium):** Sheets Welcome Guide highlights box overflows right border of 680px modal.
- **Menu Bar & Dock (Medium):** Raw binary menu bar shows lowercase `loom-sheets` and generic exec icon.
- **Formula Bar, Grid, Inspector, Themes (Low):** Formula Bar, `Cmd+Alt+R` popover shortcuts, Dark Mode, Start Hub, and Template Chooser are clean and well-aligned.

### Apple Keynote 14.5 vs Loom Present
- **Right Inspector (Critical):**
  1. Collapsed `B/I/U/S` `LoomIconButton` glyphs overlap at `x=0` (`format_section.slint:149-185`).
  2. Duplicate `A` glyph reused on unrelated controls (Automatic, No fill, Theme paper, New from Template).
  3. Duplicate `3 slide(s)` + `3 slides` readout (`document_section.slint:59-64`).
  4. Unlabeled Geometry inputs.
- **Slide Sorter Reachability (Major):** `slide_sorter.slint` exists but is unreachable from toolbar, View menu, or Command Palette (`> Slide Sorter` → `No matching commands`).
- **Titlebar & Start Hub (Major):** Stacked titlebar with `*`; Start Hub header buttons overlap divider; empty `bundleIdentifier` in AX.

## 5. macOS Native Polish & Performance Measurements

| App | Cold Launch (ms) | Initial RSS (MB) | Active RSS (MB) | Scroll 30/25 steps (avg ms) | Type Bench (ms) | `LOOM_FRAME_BENCH=enforce` |
|---|---:|---:|---:|---:|---:|---|
| **Pages 14.5** | 1647.3 | 151.2 | 331.3 | 18.66 | 593.42 (40 ch) | — |
| **Loom Writer** | 1754.1 | 131.1 | 150.6 | 19.08 | 572.16 (40 ch) | 7 passed; 100p keystroke p95=15.77 ms |
| **Numbers 14.5** | 1603.7 | 145.2 | 707.7 | 18.87 | 269.22 (11 ch) | — |
| **Loom Sheets** | 767.8 | 137.0 | 183.9 | 18.46 | 247.28 (11 ch) | Passes software/scroll (`PERF-01` open) |
| **Keynote 14.5** | 1991.0 | 153.3 | 534.8 | 19.22 | 397.83 (28 ch) | — |
| **Loom Present** | 788.2 | 107.2 | 139.4 | 18.53 | 423.75 (28 ch) | Tracked in `frame_bench_tests.rs` |

- **Graphics backend:** All 3 apps use Slint 1.17.1 `renderer-femtovg` (OpenGL) + `renderer-software`; `docs/rules/platforms.md` §18 targets Metal (`renderer-skia` or `renderer-femtovg-wgpu`).
- **macOS UX parity gaps:** Unified titlebar (`FullSizeContentView`/`NSToolbar`), `setDocumentEdited:`/`setRepresentedURL:`, native `NSMenu` context menus, overlay scrollbars, trackpad inertia/pinch-to-zoom, and Cocoa text bindings (`Option/Cmd+Arrow/Backspace`, `Ctrl+A/E/K/D/T`).

## 6. Remediation Backlog & `< 100`-File PR Plan

- [ ] **PR Wave 1 — Critical UI & Logo Fixes:** Fix Present `format_section.slint:149-185` collapsed `B/I/U/S` glyphs, duplicate `A` glyph, duplicate slide count, and unreachable Slide Sorter; add `NSApplication::setApplicationIconImage` in `macos_window.rs` + `NSImage` in `loom-desktop/Cargo.toml`; fix `loom-present.png` squircle padding.
- [ ] **PR Wave 2 — macOS Native Chrome & Shortcuts:** Unify titlebar/toolbar, wire `setDocumentEdited:`/`setRepresentedURL:`, replace `Ctrl+Alt+...` with `Cmd+Opt+...` in Writer Slint popovers, and add Cocoa text navigation bindings in `caret_keys.rs`.
- [ ] **PR Wave 3 — Design Bible & Foundation Alignment:** Reconcile `Noto Sans` vs `Inter` (`tokens/loom.toml` vs `theme.slint`), differentiate light `surface-raised`, and migrate remaining legacy `toolkit.slint`/`font_picker.slint` usages.
- [ ] **PR Wave 4 — Metal Backend & `PERF-01`:** Evaluate/enable `renderer-skia` (Metal) on macOS and close `PERF-01` native GPU/RSS review. (PR Wave 8 offline probe: `loom-ui` now exposes opt-in `renderer-skia` and `renderer-femtovg-wgpu` features, but `cargo check -p loom-ui --features renderer-skia --offline` fails because `i-slint-backend-android-activity` / Skia dependencies are not in `Cargo.lock` offline cache; `default` remains on `renderer-femtovg` + `renderer-software`.)

## 7. Resumption State Tracker

| Wave | Branch | Base | Status | Files Changed | PR URL |
|---|---|---|---|---:|---|
| PR Wave 1 | `macos-polish/1-foundation-logos-native` | `main` | `[x]` | — | PR #35 |
| PR Wave 2 | `macos-polish/2-writer-sheets-parity` | `main` | `[x]` | — | PR #36 |
| PR Wave 3 | `macos-polish/3-present-parity` | `main` | `[x]` | — | PR #37 |
| PR Wave 4 | `macos-polish/4-metal-perf` | `main` | `[x]` | — | PR #38 |
| PR Wave 5 | `macos-polish/5-themes-high-contrast-core` | `main` | `[x]` | — | PR #39 |
| PR Wave 6 | `macos-polish/6-writer-sheets-themes-polish` | `macos-polish/5-themes-high-contrast-core` | `[x]` | — | PR #40 |
| PR Wave 7 | `macos-polish/7-present-themes-polish` | `macos-polish/5-themes-high-contrast-core` | `[x]` | — | PR #41 |
| PR Wave 8 | `macos-polish/8-macos-native-context-metal-dmg` | `macos-polish/5-themes-high-contrast-core` | `[ ]` | — | Pending |
