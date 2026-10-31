//! The text the page draws, as typed segments.
//!
//! A row of the page is the words of one laid-out line (or one table cell) and
//! the character style they carry. They reach the Slint page as segments, each
//! a plain string with its weight, slant and rules, so no markup language sits
//! between the document and the screen: a `*` or `<` or `1.` the user typed is
//! just that character, and nothing the user typed can change how the rest of
//! the line is drawn.
//!
//! The live canvas paints shaped glyph tiles, keeping contextual glyphs and
//! visual positions aligned with the layout's caret and selection geometry.
//! Logical Slint text remains available to accessibility and as a visible,
//! warned fallback if shaping or bounded rasterization fails.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;

use loom_fonts::raster::{
    rasterize_glyph_tile, RasterError, RasterLimits, RasterRun, RasterTileBounds,
    HARD_MAX_TILE_DIMENSION,
};
use loom_text::{Alignment, CharacterStyle, FontWeight};
use loom_writer_core::{PageStyle, RichBlock, WriterShapedVisualLine};
use slint::{ModelRc, SharedString, VecModel};

use crate::{
    writer_render_alignment, WriterGlyphTile, WriterRenderBlock, WriterTextRule, WriterTextSegment,
};

/// Weight of a bold run. Slint's font weight is a number; the page measures
/// weights below Bold as Regular, so a segment is only ever these two.
const BOLD: i32 = 700;
const REGULAR: i32 = 400;

/// Where the rules sit, in em from the top of a line of Inter, whose baseline
/// is about 0.97 em down. The thickness has a floor so a small size still draws
/// a visible line.
const UNDERLINE_TOP_EM: f32 = 1.07;
/// Where the baseline of a line of Inter sits, in em from the top of the line.
/// Glyphs of different sizes on one line are placed so their baselines match.
const BASELINE_EM: f32 = 0.97;
const STRIKETHROUGH_TOP_EM: f32 = 0.62;
const RULE_THICKNESS_EM: f32 = 0.07;
const RULE_THICKNESS_MIN_PT: f32 = 0.6;
/// A line this much wider than its row, in points, is wrapped; narrower
/// differences are rounding between two measurements of the same text.
const FIT_SLACK_PT: f32 = 0.5;
/// App tiles are smaller than the rasterizer's hard ceiling so very wide or
/// tall visible text is split into bounded images instead of being discarded.
const GLYPH_TILE_EDGE: u32 = 1024;
const MAX_GLYPH_TILE_REGIONS_PER_LINE: usize = 64;
const MAX_GLYPH_TILE_IMAGES_PER_LINE: usize = MAX_GLYPH_TILE_REGIONS_PER_LINE * 2;
const MAX_GLYPH_RUNS_PER_LINE: usize = 4096;

/// How the text of a row sits across the row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Align {
    /// Placed here, by the block's own alignment against the row width, and
    /// wrapped to it: table cells, chart labels and the fallback rows.
    Block,
    /// One line the page layout already broke, measured `width` points wide;
    /// aligned with exactly the shift the layout uses for its highlights.
    Line(f32),
    /// Left at the row's own edge: a stretch of a line, which the layout
    /// placed by measuring it.
    Start,
}

/// Where a rendered row sits, in points from the page content box.
#[derive(Clone, Copy)]
pub(super) struct Frame {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Extra paragraph geometry for a rendered line. Keeping these fields named
/// makes the relationship between expanded spaces, bidi fallback, and the
/// line's font size explicit at projection call sites.
pub(super) struct RowGeometry<'a> {
    pub(super) spaces: &'a [(Range<usize>, f32)],
    pub(super) unsupported: bool,
    pub(super) font_size: f32,
}

/// Visible page-content rectangle in page points. Projection clips glyph
/// images to the active viewport before allocating any tile buffers.
#[derive(Clone, Copy)]
pub(super) struct RasterClip {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) right: f32,
    pub(super) bottom: f32,
}

#[derive(Debug)]
pub(super) enum GlyphPaintError {
    Raster(RasterError),
    CellGeometry(loom_writer_core::TableGeometryError),
    RunLimitExceeded { count: usize, limit: usize },
    LineBoxTooShort { required: f32, available: f32 },
}

impl fmt::Display for GlyphPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raster(error) => fmt::Display::fmt(error, formatter),
            Self::CellGeometry(error) => fmt::Display::fmt(error, formatter),
            Self::RunLimitExceeded { count, limit } => write!(
                formatter,
                "line has {count} styled runs; the visible painter limit is {limit}"
            ),
            Self::LineBoxTooShort {
                required,
                available,
            } => write!(
                formatter,
                "cell text needs {required:.1} pt of height; its row provides {available:.1} pt"
            ),
        }
    }
}

impl From<RasterError> for GlyphPaintError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

impl Frame {
    pub(super) fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Turn typed text into the segment models the page canvas draws.
pub(super) fn render_row(
    block: &RichBlock,
    frame: Frame,
    marker: &str,
    family: &str,
    align: Align,
    geometry: Option<RowGeometry<'_>>,
) -> WriterRenderBlock {
    let Frame {
        x,
        y,
        width,
        height,
    } = frame;
    let (spaces, unsupported, font_size) = geometry.map_or_else(
        || {
            (
                None,
                false,
                PageStyle::default().font_size_for_kind(block.kind.as_str()),
            )
        },
        |geometry| {
            (
                Some(geometry.spaces),
                geometry.unsupported,
                geometry.font_size,
            )
        },
    );
    let text = row_text_with_spaces(block, font_size, width, align, spaces);
    WriterRenderBlock {
        segments: ModelRc::new(VecModel::from(text.segments)),
        rules: ModelRc::new(VecModel::from(text.rules)),
        glyph_tiles: ModelRc::new(VecModel::from(Vec::<WriterGlyphTile>::new())),
        positioned: text.positioned,
        x,
        y,
        width,
        height,
        font_size,
        font_family: SharedString::from(family),
        alignment: writer_render_alignment(
            block
                .style
                .alignment
                .resolve_with_direction(block.text.as_str(), block.style.direction),
        ),
        marker: SharedString::from(marker),
        unsupported,
        unsupported_label: if unsupported {
            SharedString::from("Justify unavailable")
        } else {
            SharedString::default()
        },
        has_color: false,
        color: slint::Color::from_argb_encoded(0),
        table_cell: false,
    }
}

/// Rasterize the exact shaped glyphs already attached to a core line. Tile
/// positions are relative to the row frame and remain in page points; Slint
/// applies page zoom and the window's device scale when it displays them.
pub(super) fn set_shaped_glyphs(
    row: &mut WriterRenderBlock,
    shaped: &WriterShapedVisualLine,
    frame: Frame,
    pixels_per_point: f32,
    clip: RasterClip,
    default_ink_rgba: [u8; 4],
) -> Result<(), GlyphPaintError> {
    let tiles = rasterize_shaped_tiles(shaped, frame, pixels_per_point, clip, default_ink_rgba)?;
    row.glyph_tiles = ModelRc::new(VecModel::from(tiles));
    Ok(())
}

/// Shape and rasterize every wrapped visual line in a table cell using the
/// same core face resolution and per-run style pipeline as document lines.
pub(super) fn set_shaped_block_glyphs(
    row: &mut WriterRenderBlock,
    block: &RichBlock,
    frame: Frame,
    font_size: f32,
    pixels_per_point: f32,
    clip: RasterClip,
    default_ink_rgba: [u8; 4],
) -> Result<(), GlyphPaintError> {
    if frame.x >= clip.right
        || frame.x + frame.width <= clip.left
        || frame.y >= clip.bottom
        || frame.y + frame.height <= clip.top
    {
        return Ok(());
    }
    let geometry = loom_writer_core::shape_table_cell_geometry(block, font_size, frame.width)
        .map_err(GlyphPaintError::CellGeometry)?;
    let required_height = geometry.content_height_pt();
    if required_height > frame.height + f32::EPSILON {
        return Err(GlyphPaintError::LineBoxTooShort {
            required: required_height,
            available: frame.height,
        });
    }
    let mut tiles = Vec::new();
    let mut line_y = frame.y;
    for line in geometry.lines() {
        if let Some(shaped) = line.shaped() {
            let line_frame = Frame::new(frame.x, line_y, frame.width, line.pitch_pt());
            tiles.extend(rasterize_shaped_tiles(
                shaped,
                line_frame,
                pixels_per_point,
                clip,
                default_ink_rgba,
            )?);
        }
        line_y += line.pitch_pt();
    }
    row.glyph_tiles = ModelRc::new(VecModel::from(tiles));
    Ok(())
}

pub(super) struct FootnoteCellGlyphs<'a> {
    pub doc: &'a loom_writer_core::WriterDocument,
    pub table_block_id: u64,
    pub table: &'a loom_writer_core::loom_objects::table::Table,
    pub cell: (usize, usize),
    pub block: &'a RichBlock,
    pub frame: Frame,
    pub font_size: f32,
    pub pixels_per_point: f32,
    pub clip: RasterClip,
    pub default_ink_rgba: [u8; 4],
}

/// Rasterize a table cell while retaining the inline advances reserved for its footnotes.
pub(super) fn set_shaped_block_glyphs_with_footnotes(
    row: &mut WriterRenderBlock,
    request: FootnoteCellGlyphs<'_>,
) -> Result<(), GlyphPaintError> {
    let FootnoteCellGlyphs {
        doc,
        table_block_id,
        table,
        cell,
        block,
        frame,
        font_size,
        pixels_per_point,
        clip,
        default_ink_rgba,
    } = request;
    if frame.x >= clip.right
        || frame.x + frame.width <= clip.left
        || frame.y >= clip.bottom
        || frame.y + frame.height <= clip.top
    {
        return Ok(());
    }
    let geometry = loom_writer_core::shape_table_cell_geometry_with_footnotes(
        doc,
        table_block_id,
        table,
        cell,
        block,
        font_size,
        frame.width,
    )
    .map_err(GlyphPaintError::CellGeometry)?;
    let required_height = geometry.content_height_pt();
    if required_height > frame.height + f32::EPSILON {
        return Err(GlyphPaintError::LineBoxTooShort {
            required: required_height,
            available: frame.height,
        });
    }
    let mut tiles = Vec::new();
    let mut line_y = frame.y;
    for line in geometry.lines() {
        if let Some(shaped) = line.shaped() {
            let line_frame = Frame::new(frame.x, line_y, frame.width, line.pitch_pt());
            tiles.extend(rasterize_shaped_tiles(
                shaped,
                line_frame,
                pixels_per_point,
                clip,
                default_ink_rgba,
            )?);
        }
        line_y += line.pitch_pt();
    }
    row.glyph_tiles = ModelRc::new(VecModel::from(tiles));
    Ok(())
}

fn rasterize_shaped_tiles(
    shaped: &WriterShapedVisualLine,
    frame: Frame,
    pixels_per_point: f32,
    clip: RasterClip,
    default_ink_rgba: [u8; 4],
) -> Result<Vec<WriterGlyphTile>, GlyphPaintError> {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return Err(RasterError::InvalidRunGeometry.into());
    }
    let left = frame.x.max(clip.left);
    let top = frame.y.max(clip.top);
    let right = (frame.x + frame.width).min(clip.right);
    let bottom = (frame.y + frame.height).min(clip.bottom);
    if !(left < right && top < bottom) {
        return Ok(Vec::new());
    }

    let scale = f64::from(pixels_per_point);
    let pixel_bounds = RasterTileBounds {
        origin_x_px: pixel_edge(f64::from(left) * scale, false)?,
        origin_y_px: pixel_edge(f64::from(top) * scale, false)?,
        width_px: u32::try_from(
            pixel_edge(f64::from(right) * scale, true)?
                .checked_sub(pixel_edge(f64::from(left) * scale, false)?)
                .ok_or(RasterError::InvalidRunGeometry)?,
        )
        .map_err(|_| RasterError::InvalidRunGeometry)?,
        height_px: u32::try_from(
            pixel_edge(f64::from(bottom) * scale, true)?
                .checked_sub(pixel_edge(f64::from(top) * scale, false)?)
                .ok_or(RasterError::InvalidRunGeometry)?,
        )
        .map_err(|_| RasterError::InvalidRunGeometry)?,
    };
    if pixel_bounds.width_px == 0 || pixel_bounds.height_px == 0 {
        return Err(RasterError::InvalidLimitsOrBounds.into());
    }
    let tile_edge = GLYPH_TILE_EDGE.min(HARD_MAX_TILE_DIMENSION);
    let tile_columns = pixel_bounds.width_px.div_ceil(tile_edge) as usize;
    let tile_rows = pixel_bounds.height_px.div_ceil(tile_edge) as usize;
    let tile_count = tile_columns
        .checked_mul(tile_rows)
        .ok_or(RasterError::TileLimitExceeded)?;
    if tile_count > MAX_GLYPH_TILE_REGIONS_PER_LINE {
        return Err(RasterError::TileLimitExceeded.into());
    }

    let painted_runs = shaped.painted_runs();
    if painted_runs.len() > MAX_GLYPH_RUNS_PER_LINE {
        return Err(GlyphPaintError::RunLimitExceeded {
            count: painted_runs.len(),
            limit: MAX_GLYPH_RUNS_PER_LINE,
        });
    }
    let mut raster_runs = Vec::new();
    raster_runs
        .try_reserve_exact(painted_runs.len())
        .map_err(|_| RasterError::TileAllocationFailed)?;
    for run in painted_runs {
        let raster_run = RasterRun {
            face: &run.face,
            shaped: &run.shaped,
            font_size_units: run.size_pt,
            pixels_per_unit: pixels_per_point,
            origin_x_px: f64::from(frame.x + run.x_pt) * scale,
            baseline_y_px: f64::from(frame.y + shaped.ascent_pt() - run.baseline_shift_pt) * scale,
            variations: &run.variations,
            synthetic_bold: run.synthetic_bold,
            synthetic_italic: run.synthetic_italic,
            color: if run.foreground_color_explicit {
                run.rgba
            } else {
                default_ink_rgba
            },
        };
        raster_runs.push(raster_run);
    }
    let tile_image_capacity = tile_count.min(MAX_GLYPH_TILE_IMAGES_PER_LINE);
    let mut tiles = Vec::new();
    tiles
        .try_reserve_exact(tile_image_capacity)
        .map_err(|_| RasterError::TileAllocationFailed)?;
    for row_index in 0..tile_rows {
        for column_index in 0..tile_columns {
            let origin_x_px = pixel_bounds.origin_x_px + i64::from(column_index as u32 * tile_edge);
            let origin_y_px = pixel_bounds.origin_y_px + i64::from(row_index as u32 * tile_edge);
            let bounds = RasterTileBounds {
                origin_x_px,
                origin_y_px,
                width_px: tile_edge.min(pixel_bounds.width_px - column_index as u32 * tile_edge),
                height_px: tile_edge.min(pixel_bounds.height_px - row_index as u32 * tile_edge),
            };
            let byte_len = tile_byte_len(bounds)?;
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(byte_len)
                .map_err(|_| RasterError::TileAllocationFailed)?;
            pixels.resize(byte_len, 0);
            paint_run_backgrounds(&mut pixels, bounds, shaped, frame, pixels_per_point);
            let glyphs = rasterize_glyph_tile(&raster_runs, bounds, RasterLimits::default())?;
            composite_premultiplied(&mut pixels, &glyphs.rgba_premultiplied)?;
            drop(glyphs);
            paint_run_decorations(
                &mut pixels,
                bounds,
                shaped,
                frame,
                pixels_per_point,
                default_ink_rgba,
            );
            push_glyph_tile(&mut tiles, bounds, frame, scale, pixels_per_point, pixels);
        }
    }
    Ok(tiles)
}

fn push_glyph_tile(
    tiles: &mut Vec<WriterGlyphTile>,
    bounds: RasterTileBounds,
    frame: Frame,
    scale: f64,
    pixels_per_point: f32,
    pixels: Vec<u8>,
) {
    if !pixels.iter().any(|channel| *channel != 0) {
        return;
    }
    let buffer: slint::SharedPixelBuffer<slint::Rgba8Pixel> =
        slint::SharedPixelBuffer::clone_from_slice(&pixels, bounds.width_px, bounds.height_px);
    let source = slint::Image::from_rgba8_premultiplied(buffer);
    tiles.push(WriterGlyphTile {
        x: (bounds.origin_x_px as f64 / scale - f64::from(frame.x)) as f32,
        y: (bounds.origin_y_px as f64 / scale - f64::from(frame.y)) as f32,
        width: bounds.width_px as f32 / pixels_per_point,
        height: bounds.height_px as f32 / pixels_per_point,
        source,
    });
}

fn pixel_edge(value: f64, round_up: bool) -> Result<i64, RasterError> {
    if !value.is_finite() || value.abs() > 1.0e12 {
        return Err(RasterError::InvalidRunGeometry);
    }
    Ok(if round_up {
        value.ceil() as i64
    } else {
        value.floor() as i64
    })
}

fn tile_byte_len(bounds: RasterTileBounds) -> Result<usize, RasterError> {
    usize::try_from(bounds.width_px)
        .ok()
        .and_then(|width| {
            usize::try_from(bounds.height_px)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(RasterError::TileAllocationFailed)
}

fn paint_run_backgrounds(
    pixels: &mut [u8],
    bounds: RasterTileBounds,
    shaped: &WriterShapedVisualLine,
    frame: Frame,
    pixels_per_point: f32,
) {
    for run in shaped.painted_runs() {
        let Some(color) = run.background_rgba else {
            continue;
        };
        let baseline = frame.y + shaped.ascent_pt() - run.baseline_shift_pt;
        fill_rect(
            pixels,
            bounds,
            f64::from(frame.x + run.x_pt) * f64::from(pixels_per_point),
            f64::from(baseline - run.ascent_pt) * f64::from(pixels_per_point),
            f64::from(frame.x + run.x_pt + run.shaped.width) * f64::from(pixels_per_point),
            f64::from(baseline + run.descent_pt) * f64::from(pixels_per_point),
            color,
        );
    }
}

fn paint_run_decorations(
    pixels: &mut [u8],
    bounds: RasterTileBounds,
    shaped: &WriterShapedVisualLine,
    frame: Frame,
    pixels_per_point: f32,
    default_ink_rgba: [u8; 4],
) {
    for run in shaped.painted_runs() {
        let baseline = frame.y + shaped.ascent_pt() - run.baseline_shift_pt;
        for (enabled, metric, fallback_offset, fallback_thickness) in [
            (
                run.underline,
                run.underline_metric,
                -run.size_pt * 0.12,
                run.size_pt * 0.06,
            ),
            (
                run.strikethrough,
                run.strikeout_metric,
                run.size_pt * 0.30,
                run.size_pt * 0.06,
            ),
        ] {
            if !enabled {
                continue;
            }
            let offset = metric.map_or(fallback_offset, |value| value.offset);
            let thickness = metric
                .map_or(fallback_thickness, |value| value.thickness)
                .max(0.25);
            let top = baseline - offset;
            fill_rect(
                pixels,
                bounds,
                f64::from(frame.x + run.x_pt) * f64::from(pixels_per_point),
                f64::from(top) * f64::from(pixels_per_point),
                f64::from(frame.x + run.x_pt + run.shaped.width) * f64::from(pixels_per_point),
                f64::from(top + thickness) * f64::from(pixels_per_point),
                if run.foreground_color_explicit {
                    run.rgba
                } else {
                    default_ink_rgba
                },
            );
        }
    }
}

fn fill_rect(
    pixels: &mut [u8],
    bounds: RasterTileBounds,
    left_px: f64,
    top_px: f64,
    right_px: f64,
    bottom_px: f64,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    let left =
        (left_px.floor() as i64 - bounds.origin_x_px).clamp(0, i64::from(bounds.width_px)) as u32;
    let top =
        (top_px.floor() as i64 - bounds.origin_y_px).clamp(0, i64::from(bounds.height_px)) as u32;
    let right =
        (right_px.ceil() as i64 - bounds.origin_x_px).clamp(0, i64::from(bounds.width_px)) as u32;
    let bottom =
        (bottom_px.ceil() as i64 - bounds.origin_y_px).clamp(0, i64::from(bounds.height_px)) as u32;
    for y in top..bottom {
        for x in left..right {
            let index = ((y as usize * bounds.width_px as usize) + x as usize) * 4;
            blend_straight_over(&mut pixels[index..index + 4], color);
        }
    }
}

fn blend_straight_over(destination: &mut [u8], source: [u8; 4]) {
    let alpha = u16::from(source[3]);
    let inverse = 255 - alpha;
    for channel in 0..3 {
        let premultiplied = (u16::from(source[channel]) * alpha + 127) / 255;
        destination[channel] = (premultiplied
            + (u16::from(destination[channel]) * inverse + 127) / 255)
            .min(255) as u8;
    }
    destination[3] = (alpha + (u16::from(destination[3]) * inverse + 127) / 255).min(255) as u8;
}

fn composite_premultiplied(destination: &mut [u8], source: &[u8]) -> Result<(), RasterError> {
    if destination.len() != source.len() || destination.len() % 4 != 0 {
        return Err(RasterError::TileAllocationFailed);
    }
    for (target, glyph) in destination.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
        let inverse = 255 - u16::from(glyph[3]);
        for channel in 0..3 {
            target[channel] = (u16::from(glyph[channel])
                + (u16::from(target[channel]) * inverse + 127) / 255)
                .min(255) as u8;
        }
        target[3] =
            (u16::from(glyph[3]) + (u16::from(target[3]) * inverse + 127) / 255).min(255) as u8;
    }
    Ok(())
}

/// What a row draws.
pub(crate) struct RowText {
    pub segments: Vec<WriterTextSegment>,
    pub rules: Vec<WriterTextRule>,
    /// False: at most one segment, aligned and wrapped by the text item.
    pub positioned: bool,
}

/// Clip block-relative expanded gaps to one rendered fragment and translate
/// them to the fragment's byte offsets.
pub(crate) fn fragment_justification_spaces(
    spaces: &[(Range<usize>, f32)],
    fragment: Range<usize>,
) -> Vec<(Range<usize>, f32)> {
    spaces
        .iter()
        .filter_map(|(gap, extra)| {
            let start = gap.start.max(fragment.start);
            let end = gap.end.min(fragment.end);
            (start < end).then_some((
                start - fragment.start..end - fragment.start,
                *extra * (end - start) as f32 / (gap.end - gap.start) as f32,
            ))
        })
        .collect()
}

fn run_color(color: Option<&str>) -> Option<u32> {
    let hex = color?.strip_prefix('#')?;
    if !hex.is_ascii() || !matches!(hex.len(), 6 | 8) {
        return None;
    }
    let channel = |start| u8::from_str_radix(&hex[start..start + 2], 16).ok();
    let (red, green, blue, alpha) = (
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if hex.len() == 8 { channel(6)? } else { u8::MAX },
    );
    Some(u32::from_be_bytes([alpha, red, green, blue]))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    size: f32,
    baseline_shift_pt: f32,
    color: Option<u32>,
}

impl Look {
    fn of(style: Option<&CharacterStyle>, block_size: f32, heading: bool) -> Self {
        match style {
            Some(style) => {
                let requested_size = loom_writer_core::run_size(style, block_size);
                let superscript = style.superscript && !style.subscript;
                let subscript = style.subscript && !style.superscript;
                Self {
                    bold: heading || style.weight.numeric() >= FontWeight::Bold.numeric(),
                    italic: style.italic,
                    underline: style.underline,
                    strikethrough: style.strikethrough,
                    size: if superscript || subscript {
                        requested_size * 0.7
                    } else {
                        requested_size
                    },
                    baseline_shift_pt: if superscript {
                        requested_size * 0.35
                    } else if subscript {
                        -(requested_size * 0.35)
                    } else {
                        0.0
                    },
                    color: run_color(style.color.as_deref()),
                }
            }
            None => Self {
                bold: heading,
                italic: false,
                underline: false,
                strikethrough: false,
                size: block_size,
                baseline_shift_pt: 0.0,
                color: None,
            },
        }
    }
}

/// A stretch of a block's bytes drawn in one look.
struct Piece {
    start: usize,
    end: usize,
    look: Look,
}

/// Characters that end a line instead of being drawn.
fn is_line_end(ch: char) -> bool {
    matches!(ch, '\n' | '\u{b}' | '\u{c}')
}

fn drawn(text: &str) -> String {
    text.chars().filter(|ch| !is_line_end(*ch)).collect()
}

fn has_drawn(text: &str) -> bool {
    text.chars().any(|ch| !is_line_end(ch))
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// A block's text with its runs, measured at one size the way the page layout
/// measures it, so what is placed here is where the layout puts its highlight.
struct Measure<'a> {
    block: &'a RichBlock,
    size: f32,
}

impl Measure<'_> {
    /// Width in points of bytes `from..to` of the block, shaped on their own.
    fn width(&self, from: usize, to: usize) -> f32 {
        self.block.measured_advance(from, to, self.size)
    }

    /// The same, without the white space that hangs off the end of a line.
    fn ink_width(&self, from: usize, to: usize) -> f32 {
        let text = self.block.text.as_str();
        self.width(from, from + text[from..to].trim_end().len())
    }
}

/// The block's text cut wherever a visible run attribute changes.
fn pieces(block: &RichBlock, block_size: f32) -> Vec<Piece> {
    let text = block.text.as_str();
    let mut cuts = BTreeSet::from([0, text.len()]);
    for run in &block.runs {
        cuts.insert(floor_boundary(text, run.start));
        cuts.insert(floor_boundary(text, run.end));
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();
    let heading = loom_writer_core::is_heading_kind(block.kind.as_str());
    let mut pieces: Vec<Piece> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let look = Look::of(
            block
                .runs
                .iter()
                .find(|run| run.start <= start && start < run.end)
                .map(|run| &run.style),
            block_size,
            heading,
        );
        match pieces.last_mut() {
            Some(last) if last.look == look => last.end = end,
            _ => pieces.push(Piece { start, end, look }),
        }
    }
    pieces
}

fn segment(text: String, look: Look, x: f32, y: f32) -> WriterTextSegment {
    WriterTextSegment {
        text: text.into(),
        x,
        y,
        size: look.size,
        weight: if look.bold { BOLD } else { REGULAR },
        italic: look.italic,
        underline: look.underline,
        strikethrough: look.strikethrough,
        has_color: look.color.is_some(),
        color: slint::Color::from_argb_encoded(look.color.unwrap_or_default()),
    }
}

/// What the page draws for `block` in a row `width` points wide, set at
/// `font_size`.
#[cfg(test)]
pub(crate) fn row_text(block: &RichBlock, font_size: f32, width: f32, align: Align) -> RowText {
    row_text_with_spaces(block, font_size, width, align, None)
}

pub(crate) fn row_text_with_spaces(
    block: &RichBlock,
    font_size: f32,
    width: f32,
    align: Align,
    supplied_spaces: Option<&[(Range<usize>, f32)]>,
) -> RowText {
    let text = block.text.as_str();
    let all = pieces(block, font_size);
    let shown: Vec<&Piece> = all
        .iter()
        .filter(|piece| has_drawn(&text[piece.start..piece.end]))
        .collect();
    let decorated = shown
        .iter()
        .any(|piece| piece.look.underline || piece.look.strikethrough);
    let broken_inside = text.trim_end_matches(is_line_end).contains(is_line_end);
    let resolved_alignment = block
        .style
        .alignment
        .resolve_with_direction(text, block.style.direction);
    let paragraph_start = Alignment::Start.resolve_with_direction(text, block.style.direction);
    let justified_block_row = align == Align::Block && resolved_alignment == Alignment::Justify;
    let positioned_spaces = supplied_spaces.is_some_and(|spaces| !spaces.is_empty());
    let shifted = shown
        .iter()
        .any(|piece| piece.look.baseline_shift_pt != 0.0);
    if shown.len() <= 1
        && !decorated
        && !broken_inside
        && !justified_block_row
        && !positioned_spaces
        && !shifted
    {
        let segments = shown
            .first()
            .map(|piece| vec![segment(drawn(text), piece.look, 0.0, 0.0)])
            .unwrap_or_default();
        return RowText {
            segments,
            rules: Vec::new(),
            positioned: false,
        };
    }

    let measure = Measure {
        block,
        size: font_size,
    };
    let mut segments = Vec::new();
    let mut rules = Vec::new();
    let lines = line_ranges(&measure, width, align);
    for (index, &(line_start, line_end)) in lines.iter().enumerate() {
        let ink_end = line_start + text[line_start..line_end].trim_end().len();
        let spaces = supplied_spaces.map_or_else(
            || {
                if align == Align::Block {
                    block
                        .justification_spaces_for_line(line_start..line_end, font_size, width)
                        .0
                } else {
                    Vec::new()
                }
            },
            |spaces| spaces.to_vec(),
        );
        let line_width = match align {
            Align::Line(measured) => measured,
            _ => measure.width(line_start, ink_end),
        };
        let shift = match (align, resolved_alignment) {
            (Align::Start, _) => 0.0,
            (_, Alignment::Center) => ((width - line_width) / 2.0).max(0.0),
            (_, Alignment::Right) => (width - line_width).max(0.0),
            (Align::Block | Align::Line(_), Alignment::Justify)
                if spaces.is_empty() && paragraph_start == Alignment::Right =>
            {
                (width - line_width).max(0.0)
            }
            _ => 0.0,
        };
        // The tallest glyph on the line sets its height; smaller glyphs sit on
        // the same baseline, so each one's top is lowered by its size gap.
        let line_size = all
            .iter()
            .filter(|piece| piece.start.max(line_start) < piece.end.min(line_end))
            .map(|piece| piece.look.size)
            .fold(font_size, f32::max);
        let line_top = index as f32
            * loom_writer_core::line_pitch(&block.runs, font_size)
            * loom_writer_core::TABLE_CELL_LINE_PITCH_EM;
        let draw_line = LineDraw {
            measure: &measure,
            text,
            line_start,
            ink_end,
            shift,
            line_top,
            line_size,
            spaces: &spaces,
        };
        for piece in &all {
            let (from, to) = (piece.start.max(line_start), piece.end.min(line_end));
            if from >= to {
                continue;
            }
            draw_piece(&draw_line, piece, from, to, &mut segments, &mut rules);
        }
    }
    RowText {
        segments,
        rules,
        positioned: true,
    }
}

struct LineDraw<'a> {
    measure: &'a Measure<'a>,
    text: &'a str,
    line_start: usize,
    ink_end: usize,
    shift: f32,
    line_top: f32,
    line_size: f32,
    spaces: &'a [(Range<usize>, f32)],
}

fn draw_piece(
    line: &LineDraw<'_>,
    piece: &Piece,
    from: usize,
    to: usize,
    segments: &mut Vec<WriterTextSegment>,
    rules: &mut Vec<WriterTextRule>,
) {
    let mut cuts: Vec<usize> = line
        .spaces
        .iter()
        .map(|(gap, _)| gap.end)
        .filter(|cut| from < *cut && *cut < to)
        .collect();
    cuts.push(to);
    cuts.sort_unstable();
    cuts.dedup();
    let mut start = from;
    for end in cuts {
        let words = drawn(&line.text[start..end]);
        let x = line.shift
            + line.measure.width(line.line_start, start)
            + extra_before(line.spaces, start);
        let y = line.line_top + (line.line_size - piece.look.size) * BASELINE_EM
            - piece.look.baseline_shift_pt;
        if !words.is_empty() {
            segments.push(segment(words, piece.look, x, y));
        }
        let inked = end.min(line.ink_end);
        if inked > start && (piece.look.underline || piece.look.strikethrough) {
            let run_width =
                line.measure.width(start, inked) + extra_between(line.spaces, start, inked);
            let thickness = (piece.look.size * RULE_THICKNESS_EM).max(RULE_THICKNESS_MIN_PT);
            for (wanted, top_em) in [
                (piece.look.underline, UNDERLINE_TOP_EM),
                (piece.look.strikethrough, STRIKETHROUGH_TOP_EM),
            ] {
                if wanted {
                    rules.push(WriterTextRule {
                        x,
                        y: y + piece.look.size * top_em,
                        width: run_width,
                        height: thickness,
                    });
                }
            }
        }
        start = end;
    }
}

fn extra_before(spaces: &[(Range<usize>, f32)], offset: usize) -> f32 {
    spaces
        .iter()
        .map(|(range, extra)| {
            if offset <= range.start {
                0.0
            } else if offset >= range.end {
                *extra
            } else {
                *extra * (offset - range.start) as f32 / (range.end - range.start) as f32
            }
        })
        .sum()
}

fn extra_between(spaces: &[(Range<usize>, f32)], start: usize, end: usize) -> f32 {
    (extra_before(spaces, end) - extra_before(spaces, start)).max(0.0)
}

/// The byte ranges of the lines of `text`: a line ends after each line-end
/// character, and for [`Align::Block`] a longer line is wrapped to `width`.
/// Ranges cover the text; the line-end characters stay at the end of theirs.
fn line_ranges(measure: &Measure, width: f32, align: Align) -> Vec<(usize, usize)> {
    let text = measure.block.text.as_str();
    let mut hard = Vec::new();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if is_line_end(ch) {
            hard.push((start, at + ch.len_utf8()));
            start = at + ch.len_utf8();
        }
    }
    if start < text.len() || hard.is_empty() {
        hard.push((start, text.len()));
    }
    if align != Align::Block {
        return hard;
    }
    hard.into_iter()
        .flat_map(|(start, end)| wrap(measure, width, start, end))
        .collect()
}

/// Greedy word wrap of `text[start..end]` to `width`: a line breaks before the
/// first word that no longer fits, and a word wider than the row stays whole on
/// a line of its own.
fn wrap(measure: &Measure, width: f32, start: usize, end: usize) -> Vec<(usize, usize)> {
    let text = measure.block.text.as_str();
    let fits = |from: usize, to: usize| measure.ink_width(from, to) <= width + FIT_SLACK_PT;
    let mut lines = Vec::new();
    let mut from = start;
    while from < end {
        if fits(from, end) {
            lines.push((from, end));
            break;
        }
        let mut previous = None;
        let mut cut = None;
        for (at, ch) in text[from..end].char_indices() {
            let at = from + at;
            let starts_word = at > from
                && !ch.is_whitespace()
                && previous.is_some_and(|before: char| before.is_whitespace());
            previous = Some(ch);
            if !starts_word {
                continue;
            }
            if fits(from, at) {
                cut = Some(at);
            } else {
                // The first break after `from` is taken even when it does not
                // fit, so a long word advances instead of looping.
                cut = cut.or(Some(at));
                break;
            }
        }
        let cut = cut.unwrap_or(end);
        lines.push((from, cut));
        from = cut;
    }
    if lines.is_empty() {
        lines.push((start, end));
    }
    lines
}

#[cfg(test)]
mod shaped_raster_tests {
    use super::*;

    #[test]
    fn wrapped_cell_text_reports_a_visible_error_when_its_row_is_too_short() {
        let _serial = loom_writer_core::test_fonts::serial();
        let block = RichBlock::new(
            1,
            "paragraph",
            "alpha beta gamma delta epsilon zeta eta theta",
        );
        let available_width = 42.0;
        let geometry = loom_writer_core::shape_table_cell_geometry(&block, 12.0, available_width)
            .expect("the bundled Writer fonts shape the cell text");
        assert!(
            geometry.lines().len() > 1,
            "the narrow cell width forces wrapping"
        );
        let required_height = geometry.content_height_pt();
        let frame_height = required_height - 0.5;
        let frame = Frame::new(0.0, 0.0, available_width, frame_height);
        let mut row = render_row(&block, frame, "", "", Align::Block, None);
        let error = set_shaped_block_glyphs(
            &mut row,
            &block,
            frame,
            12.0,
            1.0,
            RasterClip {
                left: 0.0,
                top: 0.0,
                right: 100.0,
                bottom: 100.0,
            },
            loom_writer_core::PageAppearance::White.auto_ink_rgba(),
        )
        .expect_err("the cell must not silently clip lines that do not fit");
        assert!(matches!(
            error,
            GlyphPaintError::LineBoxTooShort {
                required,
                available
            } if required > available
        ));
    }
}
