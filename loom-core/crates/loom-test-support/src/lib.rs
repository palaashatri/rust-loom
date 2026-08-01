//! Deterministic headless UI capture and image comparison for Loom.
//!
//! This crate provides the visual-QA foundation used by every Loom
//! application:
//!
//! * [`capture`] — installs a headless Slint platform that renders real
//!   application components through the software renderer into an
//!   [`image::RgbaImage`]. Rendering is deterministic: the same component,
//!   size and theme produce byte-identical pixels on the same platform.
//! * [`image_diff`] — perceptual diffing with documented tolerances
//!   (mean absolute error and differing-pixel ratio).
//! * [`snapshot`] — golden-image harness that compares a rendered image
//!   against a committed baseline, writes the actual and diff images on
//!   failure, and never auto-approves baselines (explicit `LOOM_SNAPSHOT_UPDATE`
//!   opt-in only).
//! * [`png`] — PNG load/save helpers.
//!
//! Baselines are generated and compared inside the pinned Docker visual-QA
//! environment (see `loom-design-bible/VISUAL_QA.md`). This crate itself is
//! MIT OR Apache-2.0; it uses only the public Slint platform API and no
//! `unsafe` code.

pub mod capture;
pub mod image_diff;
pub mod png;
pub mod snapshot;

pub use capture::{snapshot_component, CaptureError};
pub use image_diff::{perceptual_diff, within_tolerance, DiffReport};
pub use snapshot::{assert_matches_baseline, Tolerance};
