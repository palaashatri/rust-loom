//! Bounded outline rasterization for already shaped glyph runs.
//!
//! This module consumes the glyph IDs and logical positions produced by
//! [`crate::Shaped`]. It does not reshape text. Callers supply the scale from
//! shaping units (normally points) to physical pixels and tile a viewport
//! into bounded images. The returned RGBA8 pixels are premultiplied.

use crate::{LoadedFace, Shaped, Variation};
use skrifa::instance::Size;
use skrifa::outline::{pen::OutlinePen, DrawSettings, Hinting};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider, Tag};
use tiny_skia::{FillRule, Paint, Path, PathBuilder, PixmapMut, Stroke};

/// Maximum tile edge length accepted by the rasterizer.
pub const HARD_MAX_TILE_DIMENSION: u32 = 4096;
/// Maximum premultiplied RGBA byte allocation for one raster tile.
pub const HARD_MAX_TILE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum glyphs whose outlines may be processed in one tile.
pub const HARD_MAX_GLYPHS_PER_TILE: usize = 8192;
/// Maximum shaped glyph entries scanned (including glyphs clipped outside a
/// tile) in one raster call.
pub const HARD_MAX_GLYPH_SCANS_PER_TILE: usize = 65536;
/// Maximum temporary outline buffer retained by one raster call.
pub const HARD_MAX_OUTLINE_SCRATCH_BYTES: usize = 8 * 1024 * 1024;
/// Maximum outline commands retained for one glyph path.
pub const HARD_MAX_PATH_COMMANDS_PER_GLYPH: usize = 16384;
/// Maximum total outline commands retained across a complete tile call.
pub const HARD_MAX_PATH_COMMANDS_PER_TILE: usize = 262144;
/// Maximum TrueType composite components visited for one glyph.
pub const HARD_MAX_GLYF_COMPONENT_VISITS_PER_GLYPH: usize = 65536;
/// Maximum TrueType composite components visited for one tile.
pub const HARD_MAX_GLYF_COMPONENT_VISITS_PER_TILE: usize = 262144;
const MAX_GLYF_COMPOSITE_DEPTH: usize = 32;
const MAX_GLYF_POINTS_PER_GLYPH: usize = u16::MAX as usize;
const MAX_EFFECTIVE_FONT_SIZE_PX: f32 = 4096.0;
const MAX_ABSOLUTE_ORIGIN_PX: f64 = 1.0e12;
const MAX_LOCAL_PATH_COORDINATE_PX: f64 = 1_000_000.0;
const SYNTHETIC_ITALIC_SHEAR: f64 = 0.207_911_69;

/// One already-shaped visual run to rasterize.
///
/// `font_size_units` is the size passed to the shaper that produced `shaped`.
/// `pixels_per_unit` converts both that size and every glyph advance/offset
/// from the same shaping units to physical pixels. The origin and baseline
/// are physical-pixel coordinates and may be fractional. `color` is straight
/// RGBA8; pixels in the output tile are premultiplied RGBA8.
#[derive(Clone, Copy, Debug)]
pub struct RasterRun<'a> {
    /// The face used to produce `shaped`.
    pub face: &'a LoadedFace,
    /// Visual-order glyph IDs, advances and offsets from the shaper.
    pub shaped: &'a Shaped,
    /// Font size in the units used by the shaper, normally points.
    pub font_size_units: f32,
    /// Physical pixels per shaping unit, including page zoom and device scale.
    pub pixels_per_unit: f32,
    /// Physical-pixel x coordinate of the run's visual left edge.
    pub origin_x_px: f64,
    /// Physical-pixel y coordinate of the run baseline.
    pub baseline_y_px: f64,
    /// Variable-font coordinates used for shaping this run.
    pub variations: &'a [Variation],
    /// Apply synthetic emboldening to the outlines without changing advances.
    pub synthetic_bold: bool,
    /// Apply a synthetic italic shear to the outlines without changing advances.
    pub synthetic_italic: bool,
    /// Straight-alpha RGBA8 color for this run.
    pub color: [u8; 4],
}

/// Absolute physical-pixel rectangle covered by one output tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterTileBounds {
    /// Absolute x coordinate of the tile's left edge.
    pub origin_x_px: i64,
    /// Absolute y coordinate of the tile's top edge.
    pub origin_y_px: i64,
    /// Tile width in pixels.
    pub width_px: u32,
    /// Tile height in pixels.
    pub height_px: u32,
}

/// Tighter per-call resource limits. Values are capped by the immutable hard
/// ceilings above, even if a caller sets one of these fields higher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterLimits {
    /// Maximum edge length for this call's tile.
    pub max_tile_dimension: u32,
    /// Maximum output allocation for the tile.
    pub max_tile_bytes: usize,
    /// Maximum number of glyphs whose outlines intersect this tile.
    pub max_glyphs: usize,
    /// Maximum shaped glyph entries inspected, including clipped glyphs.
    pub max_glyph_scans: usize,
    /// Maximum reusable outline scratch buffer for the call.
    pub max_outline_scratch_bytes: usize,
    /// Maximum outline pen commands retained per glyph.
    pub max_path_commands_per_glyph: usize,
    /// Maximum outline pen commands retained across the tile.
    pub max_path_commands_per_tile: usize,
    /// Maximum TrueType composite components preflighted across the tile.
    pub max_glyf_component_visits_per_tile: usize,
}

impl Default for RasterLimits {
    fn default() -> Self {
        Self {
            max_tile_dimension: HARD_MAX_TILE_DIMENSION,
            max_tile_bytes: HARD_MAX_TILE_BYTES,
            max_glyphs: HARD_MAX_GLYPHS_PER_TILE,
            max_glyph_scans: HARD_MAX_GLYPH_SCANS_PER_TILE,
            max_outline_scratch_bytes: HARD_MAX_OUTLINE_SCRATCH_BYTES,
            max_path_commands_per_glyph: HARD_MAX_PATH_COMMANDS_PER_GLYPH,
            max_path_commands_per_tile: HARD_MAX_PATH_COMMANDS_PER_TILE,
            max_glyf_component_visits_per_tile: HARD_MAX_GLYF_COMPONENT_VISITS_PER_TILE,
        }
    }
}

/// A rasterization refusal or invalid input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterError {
    /// A limit was zero or bounds were empty.
    InvalidLimitsOrBounds,
    /// Run size, scale, origin, or shaped positions were non-finite or invalid.
    InvalidRunGeometry,
    /// The tile exceeds a caller limit or a hard ceiling.
    TileLimitExceeded,
    /// The tile byte size overflowed or could not be allocated.
    TileAllocationFailed,
    /// The glyph-entry count exceeds the per-call limit.
    GlyphLimitExceeded,
    /// The tile would scan too many glyph entries before clipping.
    GlyphScanLimitExceeded { count: usize, limit: usize },
    /// A font face that was previously parsed could not be reopened.
    FontParseFailed,
    /// The glyph has no scalable outline and is not a known zero-ink glyph.
    UnsupportedGlyph { glyph_id: u16 },
    /// The outline renderer needed more temporary memory than allowed.
    OutlineScratchLimitExceeded { required: usize, limit: usize },
    /// A glyph emitted more path commands than allowed.
    PathCommandLimitExceeded { limit: usize },
    /// The tile's aggregate glyph paths exceed the per-call command limit.
    TilePathCommandLimitExceeded { limit: usize },
    /// An outline exceeds the preflight depth, point, or visit bound.
    OutlineComplexityLimitExceeded { limit: usize },
    /// The required TrueType glyph tables could not be read.
    InvalidOutlineTables,
    /// The font's outline data could not be drawn.
    OutlineDrawFailed,
    /// A path contained invalid geometry or could not be finalized.
    InvalidOutlinePath,
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimitsOrBounds => {
                f.write_str("raster limits and tile bounds must be nonzero")
            }
            Self::InvalidRunGeometry => f.write_str("raster run contains invalid geometry"),
            Self::TileLimitExceeded => f.write_str("raster tile exceeds the configured limit"),
            Self::TileAllocationFailed => {
                f.write_str("raster tile allocation failed or overflowed")
            }
            Self::GlyphLimitExceeded => f.write_str("raster tile contains too many glyphs"),
            Self::GlyphScanLimitExceeded { count, limit } => write!(
                f,
                "raster tile scans {count} glyph entries; limit is {limit}"
            ),
            Self::FontParseFailed => f.write_str("raster font data could not be parsed"),
            Self::UnsupportedGlyph { glyph_id } => {
                write!(f, "glyph {glyph_id} has no supported outline")
            }
            Self::OutlineScratchLimitExceeded { required, limit } => write!(
                f,
                "glyph outline needs {required} scratch bytes; limit is {limit}"
            ),
            Self::PathCommandLimitExceeded { limit } => {
                write!(f, "glyph outline exceeds the {limit}-command limit")
            }
            Self::TilePathCommandLimitExceeded { limit } => {
                write!(f, "tile outlines exceed the {limit}-command limit")
            }
            Self::OutlineComplexityLimitExceeded { limit } => write!(
                f,
                "outline preflight exceeds its complexity limit of {limit}"
            ),
            Self::InvalidOutlineTables => f.write_str("TrueType outline tables are invalid"),
            Self::OutlineDrawFailed => f.write_str("font outline could not be drawn"),
            Self::InvalidOutlinePath => f.write_str("font outline contains invalid path geometry"),
        }
    }
}

impl std::error::Error for RasterError {}

/// One transparent or painted premultiplied-RGBA tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterTile {
    /// Absolute physical-pixel location and dimensions of the pixel buffer.
    pub bounds: RasterTileBounds,
    /// Row-major RGBA8 pixels with premultiplied color channels.
    pub rgba_premultiplied: Vec<u8>,
}

/// Rasterizes shaped glyph IDs from existing visual-order runs into one
/// clipped, transparent RGBA tile.
///
/// All glyph advances and offsets are multiplied by each run's
/// `pixels_per_unit`. Outlines use `font_size_units * pixels_per_unit`; the
/// function never maps text back through the font's character map or shapes
/// it again. Calls that need a larger image must split it into viewport tiles.
///
/// Tile memory, outline scratch, retained path commands, visible glyph work,
/// scanned glyph entries, and TrueType composite visits have hard ceilings.
/// Glyph bounds are checked before outline decomposition, so off-tile glyphs
/// do not consume outline work. `skrifa::OutlinePen` has no
/// cancellation callback, so the callback traversal of one already-parsed
/// glyph cannot be interrupted; the path buffer stops growing at its cap and
/// returns a visible error after `skrifa` finishes that glyph. CFF charstring
/// work remains bounded by the loaded font-table bytes and its subroutine-depth
/// limit rather than a per-operator budget.
pub fn rasterize_glyph_tile(
    runs: &[RasterRun<'_>],
    bounds: RasterTileBounds,
    limits: RasterLimits,
) -> Result<RasterTile, RasterError> {
    let limits = EffectiveLimits::try_from(limits)?;
    if bounds.width_px == 0 || bounds.height_px == 0 {
        return Err(RasterError::InvalidLimitsOrBounds);
    }
    if bounds.width_px > limits.max_tile_dimension || bounds.height_px > limits.max_tile_dimension {
        return Err(RasterError::TileLimitExceeded);
    }
    let bytes = usize::try_from(bounds.width_px)
        .ok()
        .and_then(|width| {
            usize::try_from(bounds.height_px)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(RasterError::TileAllocationFailed)?;
    if bytes > limits.max_tile_bytes {
        return Err(RasterError::TileLimitExceeded);
    }

    let glyph_count = runs.iter().try_fold(0usize, |total, run| {
        total.checked_add(run.shaped.glyphs.len())
    });
    if glyph_count.map_or(true, |count| count > limits.max_glyph_scans) {
        return Err(RasterError::GlyphScanLimitExceeded {
            count: glyph_count.unwrap_or(usize::MAX),
            limit: limits.max_glyph_scans,
        });
    }
    for run in runs {
        validate_run(run)?;
    }

    let mut rgba_premultiplied = Vec::new();
    rgba_premultiplied
        .try_reserve_exact(bytes)
        .map_err(|_| RasterError::TileAllocationFailed)?;
    rgba_premultiplied.resize(bytes, 0);
    {
        let pixmap =
            PixmapMut::from_bytes(&mut rgba_premultiplied, bounds.width_px, bounds.height_px)
                .ok_or(RasterError::TileAllocationFailed)?;
        let mut context = RasterContext {
            bounds,
            limits,
            pixmap,
            scratch: Vec::new(),
            path_commands: 0,
            glyf_component_visits: 0,
            glyphs_drawn: 0,
        };
        for run in runs {
            context.draw_run(run)?;
        }
    }
    Ok(RasterTile {
        bounds,
        rgba_premultiplied,
    })
}

#[derive(Clone, Copy)]
struct EffectiveLimits {
    max_tile_dimension: u32,
    max_tile_bytes: usize,
    max_glyphs: usize,
    max_glyph_scans: usize,
    max_outline_scratch_bytes: usize,
    max_path_commands_per_glyph: usize,
    max_path_commands_per_tile: usize,
    max_glyf_component_visits_per_tile: usize,
}

impl TryFrom<RasterLimits> for EffectiveLimits {
    type Error = RasterError;

    fn try_from(limits: RasterLimits) -> Result<Self, Self::Error> {
        let effective = Self {
            max_tile_dimension: limits.max_tile_dimension.min(HARD_MAX_TILE_DIMENSION),
            max_tile_bytes: limits.max_tile_bytes.min(HARD_MAX_TILE_BYTES),
            max_glyphs: limits.max_glyphs.min(HARD_MAX_GLYPHS_PER_TILE),
            max_glyph_scans: limits.max_glyph_scans.min(HARD_MAX_GLYPH_SCANS_PER_TILE),
            max_outline_scratch_bytes: limits
                .max_outline_scratch_bytes
                .min(HARD_MAX_OUTLINE_SCRATCH_BYTES),
            max_path_commands_per_glyph: limits
                .max_path_commands_per_glyph
                .min(HARD_MAX_PATH_COMMANDS_PER_GLYPH),
            max_path_commands_per_tile: limits
                .max_path_commands_per_tile
                .min(HARD_MAX_PATH_COMMANDS_PER_TILE),
            max_glyf_component_visits_per_tile: limits
                .max_glyf_component_visits_per_tile
                .min(HARD_MAX_GLYF_COMPONENT_VISITS_PER_TILE),
        };
        if effective.max_tile_dimension == 0
            || effective.max_tile_bytes == 0
            || effective.max_glyphs == 0
            || effective.max_glyph_scans == 0
            || effective.max_outline_scratch_bytes == 0
            || effective.max_path_commands_per_glyph == 0
            || effective.max_path_commands_per_tile == 0
            || effective.max_glyf_component_visits_per_tile == 0
        {
            return Err(RasterError::InvalidLimitsOrBounds);
        }
        Ok(effective)
    }
}

fn validate_run(run: &RasterRun<'_>) -> Result<(), RasterError> {
    let effective_size_px = run.font_size_units * run.pixels_per_unit;
    let effective_size = f64::from(run.font_size_units) * f64::from(run.pixels_per_unit);
    if !run.font_size_units.is_finite()
        || run.font_size_units <= 0.0
        || !run.pixels_per_unit.is_finite()
        || run.pixels_per_unit <= 0.0
        || !effective_size_px.is_finite()
        || effective_size_px <= 0.0
        || !effective_size.is_finite()
        || effective_size > f64::from(MAX_EFFECTIVE_FONT_SIZE_PX)
        || !run.origin_x_px.is_finite()
        || !run.baseline_y_px.is_finite()
        || run.origin_x_px.abs() > MAX_ABSOLUTE_ORIGIN_PX
        || run.baseline_y_px.abs() > MAX_ABSOLUTE_ORIGIN_PX
        || !run.shaped.width.is_finite()
        || run.variations.iter().any(|axis| !axis.value.is_finite())
        || run.shaped.glyphs.iter().any(|glyph| {
            !glyph.advance.is_finite() || !glyph.x_offset.is_finite() || !glyph.y_offset.is_finite()
        })
    {
        return Err(RasterError::InvalidRunGeometry);
    }
    Ok(())
}

struct RasterContext<'a> {
    bounds: RasterTileBounds,
    limits: EffectiveLimits,
    pixmap: PixmapMut<'a>,
    scratch: Vec<u8>,
    path_commands: usize,
    glyf_component_visits: usize,
    glyphs_drawn: usize,
}

impl RasterContext<'_> {
    fn draw_run(&mut self, run: &RasterRun<'_>) -> Result<(), RasterError> {
        if run.color[3] == 0 || run.shaped.glyphs.is_empty() {
            return Ok(());
        }
        let font = FontRef::from_index(run.face.bytes(), run.face.collection_index())
            .map_err(|_| RasterError::FontParseFailed)?;
        let font_glyph_count = font
            .maxp()
            .map_err(|_| RasterError::InvalidOutlineTables)?
            .num_glyphs();
        let location = font.axes().location(
            run.variations
                .iter()
                .map(|axis| (Tag::new(&axis.tag), axis.value)),
        );
        let font_size_px = run.font_size_units * run.pixels_per_unit;
        let scale = f64::from(run.pixels_per_unit);
        let tile_x = self.bounds.origin_x_px as f64;
        let tile_y = self.bounds.origin_y_px as f64;
        let mut pen_x_units = 0.0_f64;
        let mut paint = Paint::default();
        paint.set_color_rgba8(run.color[0], run.color[1], run.color[2], run.color[3]);

        let outlines = font.outline_glyphs();
        let metrics = font.glyph_metrics(Size::new(font_size_px), &location);
        for glyph in &run.shaped.glyphs {
            let glyph_id = GlyphId::new(u32::from(glyph.glyph_id));
            if glyph.glyph_id >= font_glyph_count {
                return Err(RasterError::UnsupportedGlyph {
                    glyph_id: glyph.glyph_id,
                });
            }
            let glyph_x_px =
                run.origin_x_px + pen_x_units * scale + f64::from(glyph.x_offset) * scale - tile_x;
            let glyph_baseline_px = run.baseline_y_px - f64::from(glyph.y_offset) * scale - tile_y;
            if !glyph_x_px.is_finite()
                || !glyph_baseline_px.is_finite()
                || glyph_x_px.abs() > MAX_LOCAL_PATH_COORDINATE_PX
                || glyph_baseline_px.abs() > MAX_LOCAL_PATH_COORDINATE_PX
            {
                return Err(RasterError::InvalidRunGeometry);
            }

            let ink_bounds = metrics
                .bounds(glyph_id)
                .ok_or(RasterError::UnsupportedGlyph {
                    glyph_id: glyph.glyph_id,
                })?;
            let (x_min, y_min, x_max, y_max) = (
                f64::from(ink_bounds.x_min),
                f64::from(ink_bounds.y_min),
                f64::from(ink_bounds.x_max),
                f64::from(ink_bounds.y_max),
            );
            if [x_min, y_min, x_max, y_max]
                .iter()
                .any(|coordinate| !coordinate.is_finite())
            {
                return Err(RasterError::InvalidOutlineTables);
            }
            if x_min == x_max && y_min == y_max {
                pen_x_units += f64::from(glyph.advance);
                continue;
            }
            let shear_min = if run.synthetic_italic {
                (y_min * SYNTHETIC_ITALIC_SHEAR).min(y_max * SYNTHETIC_ITALIC_SHEAR)
            } else {
                0.0
            };
            let shear_max = if run.synthetic_italic {
                (y_min * SYNTHETIC_ITALIC_SHEAR).max(y_max * SYNTHETIC_ITALIC_SHEAR)
            } else {
                0.0
            };
            let stroke_pad = if run.synthetic_bold {
                (f64::from(font_size_px) / 48.0).max(0.125)
            } else {
                0.0
            };
            let left = glyph_x_px + x_min + shear_min - stroke_pad - 1.0;
            let right = glyph_x_px + x_max + shear_max + stroke_pad + 1.0;
            let top = glyph_baseline_px - y_max - 1.0;
            let bottom = glyph_baseline_px - y_min + 1.0;
            if [left, right, top, bottom]
                .iter()
                .any(|coordinate| !coordinate.is_finite())
            {
                return Err(RasterError::InvalidRunGeometry);
            }
            if right <= 0.0
                || left >= f64::from(self.bounds.width_px)
                || bottom <= 0.0
                || top >= f64::from(self.bounds.height_px)
            {
                pen_x_units += f64::from(glyph.advance);
                continue;
            }
            self.glyphs_drawn = self
                .glyphs_drawn
                .checked_add(1)
                .ok_or(RasterError::GlyphLimitExceeded)?;
            if self.glyphs_drawn > self.limits.max_glyphs {
                return Err(RasterError::GlyphLimitExceeded);
            }

            if font.color_glyphs().get(glyph_id).is_some() {
                return Err(RasterError::UnsupportedGlyph {
                    glyph_id: glyph.glyph_id,
                });
            }

            if outlines.format() == Some(skrifa::outline::OutlineGlyphFormat::Glyf) {
                let remaining = self
                    .limits
                    .max_glyf_component_visits_per_tile
                    .saturating_sub(self.glyf_component_visits);
                let visits = preflight_glyf_glyph(&font, glyph_id, remaining)?;
                self.glyf_component_visits = self.glyf_component_visits.checked_add(visits).ok_or(
                    RasterError::OutlineComplexityLimitExceeded {
                        limit: self.limits.max_glyf_component_visits_per_tile,
                    },
                )?;
            }

            let Some(outline) = outlines.get(glyph_id) else {
                if bitmap_glyph_exists(&font, glyph_id) {
                    return Err(RasterError::UnsupportedGlyph {
                        glyph_id: glyph.glyph_id,
                    });
                }
                return Err(RasterError::UnsupportedGlyph {
                    glyph_id: glyph.glyph_id,
                });
            };

            let scratch_required = outline.draw_memory_size(Hinting::None);
            if scratch_required > self.limits.max_outline_scratch_bytes {
                return Err(RasterError::OutlineScratchLimitExceeded {
                    required: scratch_required,
                    limit: self.limits.max_outline_scratch_bytes,
                });
            }
            if self.scratch.len() < scratch_required {
                let additional = scratch_required - self.scratch.len();
                self.scratch.try_reserve_exact(additional).map_err(|_| {
                    RasterError::OutlineScratchLimitExceeded {
                        required: scratch_required,
                        limit: self.limits.max_outline_scratch_bytes,
                    }
                })?;
                self.scratch.resize(scratch_required, 0);
            }

            let tile_remaining = self
                .limits
                .max_path_commands_per_tile
                .saturating_sub(self.path_commands);
            if tile_remaining == 0 {
                return Err(RasterError::TilePathCommandLimitExceeded {
                    limit: self.limits.max_path_commands_per_tile,
                });
            }
            let path_command_limit = self.limits.max_path_commands_per_glyph.min(tile_remaining);
            let mut path_pen = GlyphPathPen::new(
                glyph_x_px,
                glyph_baseline_px,
                run.synthetic_italic,
                path_command_limit,
            );
            let settings = DrawSettings::unhinted(Size::new(font_size_px), &location)
                .with_memory(Some(&mut self.scratch[..scratch_required]));
            outline
                .draw(settings, &mut path_pen)
                .map_err(|_| RasterError::OutlineDrawFailed)?;
            let (path, commands) = match path_pen.finish() {
                Ok(path) => path,
                Err(RasterError::PathCommandLimitExceeded { .. })
                    if tile_remaining <= self.limits.max_path_commands_per_glyph =>
                {
                    return Err(RasterError::TilePathCommandLimitExceeded {
                        limit: self.limits.max_path_commands_per_tile,
                    });
                }
                Err(error) => return Err(error),
            };
            self.path_commands = self.path_commands.checked_add(commands).ok_or(
                RasterError::TilePathCommandLimitExceeded {
                    limit: self.limits.max_path_commands_per_tile,
                },
            )?;
            if let Some(path) = path {
                self.pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
                if run.synthetic_bold {
                    let stroke = Stroke {
                        width: (font_size_px / 24.0).max(0.25),
                        miter_limit: 2.0,
                        line_join: tiny_skia::LineJoin::Round,
                        ..Stroke::default()
                    };
                    self.pixmap.stroke_path(
                        &path,
                        &paint,
                        &stroke,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
            pen_x_units += f64::from(glyph.advance);
        }
        Ok(())
    }
}

fn preflight_glyf_glyph(
    font: &FontRef<'_>,
    glyph_id: GlyphId,
    visit_limit: usize,
) -> Result<usize, RasterError> {
    if visit_limit == 0 {
        return Err(RasterError::OutlineComplexityLimitExceeded { limit: 0 });
    }
    let is_long = font
        .head()
        .map_err(|_| RasterError::InvalidOutlineTables)?
        .index_to_loc_format()
        == 1;
    let loca = font
        .loca(is_long)
        .map_err(|_| RasterError::InvalidOutlineTables)?;
    let glyf = font.glyf().map_err(|_| RasterError::InvalidOutlineTables)?;
    let mut budget = GlyfPreflightBudget {
        visit_limit: visit_limit.min(HARD_MAX_GLYF_COMPONENT_VISITS_PER_GLYPH),
        visits: 0,
        points: 0,
    };
    preflight_glyf_component(glyph_id, 0, &loca, &glyf, &mut budget)?;
    Ok(budget.visits)
}

struct GlyfPreflightBudget {
    visit_limit: usize,
    visits: usize,
    points: usize,
}

fn preflight_glyf_component(
    glyph_id: GlyphId,
    depth: usize,
    loca: &skrifa::raw::tables::loca::Loca<'_>,
    glyf: &skrifa::raw::tables::glyf::Glyf<'_>,
    budget: &mut GlyfPreflightBudget,
) -> Result<(), RasterError> {
    if depth > MAX_GLYF_COMPOSITE_DEPTH {
        return Err(RasterError::OutlineComplexityLimitExceeded {
            limit: MAX_GLYF_COMPOSITE_DEPTH,
        });
    }
    budget.visits =
        budget
            .visits
            .checked_add(1)
            .ok_or(RasterError::OutlineComplexityLimitExceeded {
                limit: budget.visit_limit,
            })?;
    if budget.visits > budget.visit_limit {
        return Err(RasterError::OutlineComplexityLimitExceeded {
            limit: budget.visit_limit,
        });
    }
    let glyph = loca
        .get_glyf(glyph_id, glyf)
        .map_err(|_| RasterError::InvalidOutlineTables)?;
    let Some(glyph) = glyph else {
        return Ok(());
    };
    match glyph {
        skrifa::raw::tables::glyf::Glyph::Simple(simple) => {
            budget.points = budget.points.checked_add(simple.num_points()).ok_or(
                RasterError::OutlineComplexityLimitExceeded {
                    limit: MAX_GLYF_POINTS_PER_GLYPH,
                },
            )?;
            if budget.points > MAX_GLYF_POINTS_PER_GLYPH {
                return Err(RasterError::OutlineComplexityLimitExceeded {
                    limit: MAX_GLYF_POINTS_PER_GLYPH,
                });
            }
        }
        skrifa::raw::tables::glyf::Glyph::Composite(composite) => {
            for (component, _) in composite.component_glyphs_and_flags() {
                preflight_glyf_component(component.into(), depth + 1, loca, glyf, budget)?;
            }
        }
    }
    Ok(())
}

fn bitmap_glyph_exists(font: &FontRef<'_>, glyph_id: GlyphId) -> bool {
    let strikes = font.bitmap_strikes();
    (0..strikes.len()).any(|index| {
        strikes
            .get(index)
            .is_some_and(|strike| strike.get(glyph_id).is_some())
    })
}

struct GlyphPathPen {
    path: PathBuilder,
    origin_x: f64,
    baseline_y: f64,
    italic: bool,
    max_commands: usize,
    commands: usize,
    limit_exceeded: bool,
    invalid_point: bool,
}

impl GlyphPathPen {
    fn new(origin_x: f64, baseline_y: f64, italic: bool, max_commands: usize) -> Self {
        Self {
            path: PathBuilder::new(),
            origin_x,
            baseline_y,
            italic,
            max_commands,
            commands: 0,
            limit_exceeded: false,
            invalid_point: false,
        }
    }

    fn begin_command(&mut self) -> bool {
        if self.commands >= self.max_commands {
            self.limit_exceeded = true;
            return false;
        }
        self.commands += 1;
        true
    }

    fn point(&mut self, x: f32, y: f32) -> Option<(f32, f32)> {
        let y = f64::from(y);
        let x = self.origin_x
            + f64::from(x)
            + if self.italic {
                y * SYNTHETIC_ITALIC_SHEAR
            } else {
                0.0
            };
        let y = self.baseline_y - y;
        if !x.is_finite()
            || !y.is_finite()
            || x.abs() > MAX_LOCAL_PATH_COORDINATE_PX
            || y.abs() > MAX_LOCAL_PATH_COORDINATE_PX
        {
            self.invalid_point = true;
            return None;
        }
        let x = x as f32;
        let y = y as f32;
        if !x.is_finite() || !y.is_finite() {
            self.invalid_point = true;
            None
        } else {
            Some((x, y))
        }
    }

    fn finish(self) -> Result<(Option<Path>, usize), RasterError> {
        if self.limit_exceeded {
            return Err(RasterError::PathCommandLimitExceeded {
                limit: self.max_commands,
            });
        }
        if self.invalid_point {
            return Err(RasterError::InvalidOutlinePath);
        }
        Ok((self.path.finish(), self.commands))
    }
}

impl OutlinePen for GlyphPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.begin_command() {
            return;
        }
        if let Some((x, y)) = self.point(x, y) {
            self.path.move_to(x, y);
        }
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if !self.begin_command() {
            return;
        }
        if let Some((x, y)) = self.point(x, y) {
            self.path.line_to(x, y);
        }
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if !self.begin_command() {
            return;
        }
        let Some((cx, cy)) = self.point(cx, cy) else {
            return;
        };
        if let Some((x, y)) = self.point(x, y) {
            self.path.quad_to(cx, cy, x, y);
        }
    }

    fn curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        if !self.begin_command() {
            return;
        }
        let Some((cx1, cy1)) = self.point(cx1, cy1) else {
            return;
        };
        let Some((cx2, cy2)) = self.point(cx2, cy2) else {
            return;
        };
        if let Some((x, y)) = self.point(x, y) {
            self.path.cubic_to(cx1, cy1, cx2, cy2, x, y);
        }
    }

    fn close(&mut self) {
        if self.begin_command() {
            self.path.close();
        }
    }
}

#[cfg(test)]
#[path = "raster_tests.rs"]
mod tests;
