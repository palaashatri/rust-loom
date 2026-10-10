//! Font catalogue, fallback resolution, shaping and text metrics for Loom.
//!
//! This crate is the foundation for a real font picker and for font-aware
//! text measurement in Writer. It has no UI dependency and does no drawing.
//!
//! * [`FontCatalog`] enumerates installed and bundled faces (reading only the
//!   small name and style tables), removes duplicates, and answers
//!   [`FontCatalog::resolve`] with a [`FontRef`] that carries a fallback chain
//!   and a `substituted` flag.
//! * [`LoadedFace`] shapes text (kerning on) and reports advances, clusters,
//!   glyph ids and line metrics, using `harfrust` over `skrifa`/`read-fonts`.
//! * [`segment`] and [`FontCatalog::layout_line`] split text by bidi level and
//!   script, choose a face per character from the fallback chain, and place
//!   the runs visually, with caret and hit-testing helpers.
//!
//! See [`writer_notes`] for how Writer is expected to use it.
//!
//! Bundled faces are the Inter files `loom-ui` already ships, so a width
//! measured here is a width the shell draws.

mod bundled;
mod catalog;
mod face;
mod info;
mod layout;
mod resolve;
mod scan;
mod segment;
mod sfnt;
mod shaped;
pub mod writer_notes;

pub use bundled::{bundled_faces, BundledFace};
pub use catalog::{Availability, FontCatalog};
pub use face::{FontError, LineMetrics, LoadedFace, ShapeSettings, TextDirection};
pub use info::{EmbeddingPermission, FaceId, FaceInfo, FaceSource};
pub use layout::{LaidOutRun, LineLayout};
pub use resolve::{FallbackPolicy, FontRef};
pub use scan::{
    system_font_dirs, Platform, ScanCache, ScanConfig, ScanIssue, ScanLimits, ScanReport,
};
pub use segment::{segment, visual_order, TextRun};
pub use sfnt::SfntError;
pub use shaped::{CaretStop, Shaped, ShapedGlyph};
