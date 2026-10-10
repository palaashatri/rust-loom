//! Design note: how Writer should adopt this crate.
//!
//! Nothing here is code; it records the intended integration so the first
//! Writer change can follow it. Writer is untouched by this crate's
//! introduction: `loom-writer-core::text_metrics` still measures with two
//! hard-coded Inter advance tables.
//!
//! # 1. Measuring with the selected font
//!
//! Replace `text_metrics::glyph_units` (per-character table lookups) with the
//! catalogue:
//!
//! * Hold one `Arc<FontCatalog>` for the process. Start with
//!   [`FontCatalog::bundled_only`] so the window is usable immediately, scan
//!   the system on a worker thread with a persisted [`ScanCache`], and swap the
//!   `Arc` in when the scan finishes. Re-resolve every `FontRef` after a swap.
//! * A character style stores `(family: String, weight: u16, italic: bool)`.
//!   Never store a [`FaceId`] or the resolved substitute in the document: both
//!   are local to one machine and one scan.
//! * For each style run call [`FontCatalog::resolve`] once and keep the
//!   [`FontRef`] in a small map keyed by `(family, weight, italic)`.
//! * Measure a style run with [`FontCatalog::layout_line`] (or
//!   [`FontCatalog::text_width`]). Shaping is per style run, so kerning does
//!   not cross a bold/regular boundary; that matches what the page draws,
//!   because Slint also shapes each styled span separately.
//! * Line breaking: shape the whole paragraph once per style run and cut at
//!   grapheme boundaries using [`Shaped::caret_stops`], whose `x` values are
//!   exact cumulative advances. Do not sum per-character widths; kerning and
//!   ligatures make that wrong. Cache `Shaped` per `(block text hash, style,
//!   size)`; the catalogue itself does not cache shaping.
//! * Line height comes from [`LoadedFace::line_metrics`] of the largest run on
//!   the line (`line_height` is ascent + descent + gap); keep Writer's
//!   explicit line-spacing multiplier on top.
//! * Text size is linear: shape once at a reference size and scale if zoom
//!   changes. Do not round advances; PDF export and the page must agree.
//!
//! Open risk: Slint renders with its own font selection by family name. The
//! face this crate resolves must be the one Slint picks, or measurement and
//! drawing diverge. For bundled Inter they are the same bytes. For system
//! fonts, pass Slint the resolved face's `family`, `weight` and `italic`, and
//! verify with a rendered comparison before enabling the picker.
//!
//! # 2. Caret placement and hit-testing
//!
//! * Caret x for a byte offset: [`LineLayout::x_at_offset`]. Pointer to
//!   offset: [`LineLayout::offset_at_x`]. Both work on grapheme boundaries
//!   and split ligature clusters evenly.
//! * Ask for a layout once per visible line and keep the [`CaretStop`] list;
//!   hit-testing a click is then a nearest-stop search.
//! * Selection rectangles are the x-range between two stops of the same run.
//!   A selection that crosses a bidi boundary is several rectangles, one per
//!   [`LaidOutRun`] it touches.
//! * Left/Right arrow moves in visual order: step through the sorted stops by
//!   `x`, not by offset. Home/End use the line's first and last stop.
//! * Writer's document model stays logical (byte offsets); only the view
//!   converts. Right-to-left paragraphs need the paragraph direction passed to
//!   [`FontCatalog::layout_line`] and the caret's affinity stored at a run
//!   boundary (leading edge of the next run versus trailing edge of the
//!   previous one).
//!
//! # 3. The font picker model
//!
//! The picker is a list of rows built from four sources, in this order:
//!
//! 1. *Theme fonts*: the document theme's heading and body families, shown
//!    first so a user can return to them.
//! 2. *Recent*: the last eight families the user applied (persisted in
//!    settings, not in the document).
//! 3. *In this document*: families used by the document but not installed.
//!    Each row is built from [`FontCatalog::availability`]:
//!    [`Availability::Missing`] shows "Not installed" and the substitute's
//!    name; [`Availability::MetricCompatible`] shows the stand-in without a
//!    warning because line breaks will not move.
//! 4. *All fonts*: [`FontCatalog::families`], filtered by a case-insensitive
//!    substring search.
//!
//! Choosing a missing family keeps its name in the document and draws with
//! the substitute, so the file renders correctly on a machine that has it.
//! The row preview is drawn by the shell with the family name; for a missing
//! family it is drawn in the substitute and muted. The catalogue scan runs off
//! the UI thread; the picker shows bundled and recent rows at once and fills
//! the rest as the scan reports.
//!
//! # 4. PDF embedding
//!
//! * Use the face the text was shaped with: [`LoadedFace::bytes`] and
//!   [`LoadedFace::collection_index`] give the file; [`ShapedGlyph::glyph_id`]
//!   gives the glyphs to keep.
//! * Check [`FaceInfo::embedding`]: [`EmbeddingPermission::Restricted`]
//!   faces must not be embedded (draw with a base-14 font or refuse), and
//!   [`FaceInfo::subsetting_allowed`] says whether a subset is permitted.
//! * Write a Type 0 font with an Identity-H encoding: a CIDFontType2 for
//!   TrueType outlines (FontFile2) or CIDFontType0 with FontFile3
//!   `OpenType` for CFF. Emit glyph ids directly, a `W` array from the shaped
//!   advances scaled to 1000 units per em, and a `ToUnicode` CMap built from
//!   the clusters so copy and text search work.
//! * Subsetting needs a subsetter (for example `fontations`' `skera`); that
//!   is a separate dependency decision. Until then embed the whole font only
//!   when it is small, otherwise fall back to the current base-14 path.
//! * Widths in the PDF must come from the same shaping as the page layout so
//!   line breaks in the PDF equal those on screen.
