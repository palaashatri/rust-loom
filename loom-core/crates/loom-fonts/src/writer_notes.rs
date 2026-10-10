//! Design note: how Writer uses this crate, and what remains.
//!
//! Nothing here is code. `loom-writer-core::text_metrics` now measures the
//! bundled Inter faces through this crate: each distinct word (with the space
//! after it, which Inter Bold kerns a full stop and a comma against) is shaped
//! once per face with kerning, its per-grapheme advances are cached, and
//! carets, selection rectangles, hit-testing, line breaking and the PDF export
//! all read the same positions. The PDF writer spaces its runs by the same
//! advances (`loom_pdf::RunShaper`). What is *not* done, and what the sections
//! below still describe, is measuring with the family a style names (the page
//! markup draws every run in Inter, so there is nothing to measure it with),
//! the system font scan and the font picker.
//!
//! # 1. Measuring with the selected font
//!
//! Once the page can draw a family other than Inter, replace the bundled-only
//! measurement with the catalogue:
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
//! * Describe a paragraph once with [`Paragraph::new`]: its text, one
//!   [`StyledSpan`] per character-style run, and the paragraph direction. This
//!   resolves bidi levels and scripts over the whole paragraph, which a
//!   wrapped line needs (a line that starts mid-way through a Hebrew
//!   paragraph is not laid out like the same words on their own). Then call
//!   [`FontCatalog::layout_paragraph_line`] for each line's byte range. For
//!   a single line of styled runs, [`FontCatalog::layout_line`] takes
//!   [`StyledRun`]s directly; [`FontCatalog::layout_text`] is the one-style
//!   convenience, and [`FontCatalog::text_width`] gives just the width.
//!   Shaping is per piece (one style, one face, one script, one direction),
//!   so kerning does not cross a bold/regular boundary; that matches what the
//!   page draws, because Slint also shapes each styled span separately.
//! * Line breaking: take the candidate break offsets from
//!   [`line_break_opportunities`] (UAX #14), measure with the layout, and cut
//!   at the last opportunity that fits. Do not sum per-character widths;
//!   kerning and ligatures make that wrong. Cache the [`LineLayout`] per
//!   `(block text hash, style, size, line range)`; the catalogue itself does
//!   not cache shaping.
//! * Line height: [`LineLayout::ascent`] and [`LineLayout::descent`] are the
//!   largest over the runs on the line (and are set for an empty line, so an
//!   empty paragraph has a caret height); `line_height()` adds the largest
//!   gap. Keep Writer's explicit line-spacing multiplier on top.
//! * Draw each [`LaidOutRun`] with its own face, size and
//!   [`LaidOutRun::variations`]; embolden or slant only when that run's
//!   `synthetic_bold` / `synthetic_italic` say so (fallback faces are judged
//!   separately). [`LaidOutRun::underline`] and `strikeout` give the line
//!   positions. A variable font's named instances are matched during
//!   [`FontCatalog::resolve`], so "bold" is a real bold when the font has one.
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
//! * Caret x for a byte offset: [`LineLayout::x_at_offset`] with an
//!   [`Affinity`]. Pointer to caret: [`LineLayout::offset_at_x`], which
//!   returns the offset and the affinity. Both work on grapheme boundaries,
//!   split ligature clusters evenly, and are `O(log n)` on tables built once
//!   with the layout; keep the [`LineLayout`], not the stops.
//! * At a bidi boundary one logical offset has two carets on screen. Keep the
//!   affinity with the caret: `Leading` is the leading edge of the character
//!   that starts at the offset, `Trailing` the trailing edge of the character
//!   that ends there. Typing at a boundary inherits the side the caret is on.
//! * Selection rectangles: [`LineLayout::selection_rects`] returns the
//!   horizontal extents of a logical range, merged where they touch; a
//!   selection that crosses a bidi boundary can be several.
//! * Left/Right arrow moves in visual order: step through
//!   [`LineLayout::caret_positions`], which is sorted by `x`. Home/End use
//!   the first and last position.
//! * Writer's document model stays logical (byte offsets); only the view
//!   converts. Right-to-left paragraphs need the paragraph direction passed to
//!   [`Paragraph::new`].
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
//! * A run with non-empty [`LaidOutRun::variations`] was shaped at a named
//!   instance of a variable font; the font file alone describes the default
//!   instance. Embedding it needs the instance's outlines (an instancer or a
//!   static-instance subset), otherwise draw that run with a base-14 font.

#[cfg(doc)]
use crate::{line_break_opportunities, Availability, CaretStop};
#[cfg(doc)]
use crate::{
    Affinity, EmbeddingPermission, FaceId, FaceInfo, FontCatalog, FontRef, LaidOutRun, LineLayout,
    LoadedFace, Paragraph, ScanCache, Shaped, ShapedGlyph, StyledRun, StyledSpan,
};
