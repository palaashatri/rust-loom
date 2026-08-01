//! Loom UI — the shared Slint component library and design-token set used
//! by every Loom application.
//!
//! The `.slint` sources live in the `ui/` directory of this crate:
//!
//! * `ui/theme.slint` — semantic design tokens (light, dark, high-contrast
//!   palettes; typography, spacing, motion), exposed through the `Theme`,
//!   `ThemeDark` and `ThemeHighContrast` globals.
//! * `ui/icons.slint` — the original Loom line-icon family (`Icon`
//!   component; see `Icon::icon` for the icon name table).
//! * `ui/components.slint` — `LoomWindow` application chrome,
//!   `StatusBar`, `ToolButton`, `IconButton`, `PrimaryButton`,
//!   `SearchField`, `Field`, `SegmentedControl`, `EmptyState`, `Panel`,
//!   `InspectorSection`.
//!
//! Applications import from these files directly, for example:
//!
//! ```slint
//! import { LoomWindow, ToolButton } from "@loom/ui/ui/components.slint";
//! ```
//!
//! The library itself is backend-agnostic. Desktop applications enable the
//! `default` features for the winit + femtovg backends; tests and headless
//! visual QA use `default-features = false` together with
//! `loom-test-support`'s capture platform.
//!
//! ## Design rules enforced here
//!
//! * Every interactive component has an `accessible-role` and
//!   `accessible-label`, a `FocusScope`, and keyboard activation
//!   (Space/Enter) built in.
//! * Components never animate by default; hosts add motion explicitly and
//!   must honor `Theme.tokens.motion.reduced-motion`.
//! * All colors come from the token palettes — no hard-coded colors inside
//!   app `.slint` files.

slint::include_modules!();

/// The smoke fixture (`ui/smoke.slint`) is only compiled into test builds;
/// it exercises every component in one window and is captured against a
/// committed golden baseline.
#[cfg(test)]
mod smoke_window {
    include!(concat!(env!("OUT_DIR"), "/smoke.rs"));
}

/// The icon names available on the [`Icon`] component, in alphabetical
/// order. Used by tooling and documentation; the authoritative drawing of
/// each icon lives in `ui/icons.slint`.
pub const ICON_NAMES: &[&str] = &[
    "audio",
    "brush",
    "camera",
    "check",
    "chevron-left",
    "chevron-right",
    "clock",
    "close",
    "cursor",
    "document",
    "export",
    "eye",
    "film",
    "folder",
    "grid",
    "image",
    "import",
    "keyboard",
    "layers",
    "link",
    "mask",
    "minus",
    "new",
    "open",
    "package",
    "pause",
    "play",
    "plus",
    "redo",
    "save",
    "scale",
    "scissors",
    "search",
    "settings",
    "slide",
    "stop",
    "table",
    "terminal",
    "text",
    "timeline",
    "trash",
    "undo",
    "video",
    "wand",
    "waveform",
    "waves",
    "zoom-in",
];

#[cfg(test)]
mod visual_tests {
    use super::smoke_window::*;

    fn baseline_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("baselines")
            .join("light")
            .join("smoke-window.png")
    }

    #[test]
    fn smoke_window_renders_and_matches_baseline() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().unwrap();
        let img =
            loom_test_support::capture::snapshot_component(&window, 900.0, 600.0, 1.0).unwrap();
        assert_eq!((img.width(), img.height()), (900, 600));

        let canvas = image::Rgba([250, 249, 247, 255]);
        let non_canvas = img.pixels().filter(|p| **p != canvas).count();
        assert!(
            non_canvas > 2000,
            "window looks blank: only {non_canvas} non-canvas pixels"
        );

        loom_test_support::snapshot::assert_matches_baseline(
            &img,
            &baseline_path(),
            loom_test_support::snapshot::Tolerance::default(),
        )
        .expect("visual baseline mismatch");
    }
}
