//! Font catalogue, fallback resolution, shaping and text metrics for Loom.
//!
//! This crate is the foundation for a real font picker and for font-aware
//! text measurement in Writer. It has no UI dependency and does no drawing.
//!
//! * [`FontCatalog`] enumerates installed and bundled faces (reading only the
//!   small name and style tables), removes duplicates, and answers
//!   [`FontCatalog::resolve`] with a [`FontRef`] that carries a fallback chain
//!   and a `substituted` flag. Variable fonts contribute their named
//!   instances, so a bold request is met by a real Bold instance.
//! * [`LoadedFace`] shapes text (kerning on) and reports advances, clusters,
//!   glyph ids and line metrics, using `harfrust` over `skrifa`/`read-fonts`.
//! * [`Paragraph`] resolves bidi levels and scripts once for a paragraph;
//!   [`FontCatalog::layout_paragraph_line`] and [`FontCatalog::layout_line`]
//!   lay out one line of styled runs: grapheme clusters choose their face
//!   from the fallback chain, runs are reordered per line, and the resulting
//!   [`LineLayout`] answers caret, hit-test and selection queries from tables
//!   built once (with [`Affinity`] at bidi boundaries).
//! * [`line_break_opportunities`] gives UAX #14 break positions for wrapping.
//!
//! See [`writer_notes`] for how Writer is expected to use it.
//!
//! # Bundled faces
//!
//! The `bundled-inter` feature (on by default) embeds the Inter files
//! `loom-ui` already ships, so the catalogue is never empty and a width
//! measured here is a width the shell draws. The bytes must be embedded
//! **once** per binary: an application whose shell registers Inter itself
//! (through Slint's `import` of the same font files) should depend on this
//! crate with `default-features = false`, and one that instead registers the
//! faces from [`bundled_faces`] must make sure the shell does not also embed
//! them. Either way the binary then carries one copy rather than two.

mod bundled;
mod catalog;
mod face;
mod info;
mod layout;
mod linebreak;
mod paragraph;
mod resolve;
mod scan;
mod segment;
mod sfnt;
mod shaped;
#[doc(hidden)]
pub mod work;
pub mod writer_notes;

pub use bundled::{bundled_faces, BundledFace};
pub use catalog::{Availability, FontCatalog};
pub use face::{
    Decoration, FontError, LineMetrics, LoadedFace, ShapeSettings, TextDirection, Variation,
    MAX_FONT_FILE_BYTES,
};
pub use info::{
    EmbeddingPermission, FaceId, FaceInfo, FaceSource, NamedInstance, VariationAxis, MAX_ALIASES,
};
pub use layout::{Affinity, CaretPosition, LaidOutRun, LineLayout, SelectionRect};
pub use linebreak::{line_break_opportunities, LineBreak};
pub use paragraph::{Paragraph, StyledRun, StyledSpan};
pub use resolve::{FaceSetup, FallbackPolicy, FontRef};
pub use scan::{
    system_font_dirs, Platform, ScanCache, ScanConfig, ScanIssue, ScanLimits, ScanReport,
};
pub use segment::{segment, visual_order, TextRun};
pub use sfnt::SfntError;
pub use shaped::{CaretStop, Shaped, ShapedGlyph};
