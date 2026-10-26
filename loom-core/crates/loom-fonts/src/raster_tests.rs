use super::*;
use crate::{FontCatalog, ShapeSettings, TextDirection};

const ARABIC: &[u8] = include_bytes!("../assets/fonts/NotoNaskhArabic-Regular.ttf");
const HEBREW: &[u8] = include_bytes!("../assets/fonts/NotoSansHebrew-Regular.ttf");
const INTER: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Regular.ttf");

fn loaded(bytes: &'static [u8]) -> LoadedFace {
    LoadedFace::from_static(bytes, 0).expect("the pinned Noto fixture is a valid TTF")
}

fn shape_rtl(face: &LoadedFace, text: &str, script: [u8; 4]) -> Shaped {
    face.shape(
        text,
        18.0,
        &ShapeSettings {
            direction: TextDirection::RightToLeft,
            script: Some(script),
            ..ShapeSettings::default()
        },
    )
}

fn run<'a>(face: &'a LoadedFace, shaped: &'a Shaped) -> RasterRun<'a> {
    RasterRun {
        face,
        shaped,
        font_size_units: 18.0,
        pixels_per_unit: 1.5,
        origin_x_px: 7.25,
        baseline_y_px: 52.5,
        variations: &[],
        synthetic_bold: false,
        synthetic_italic: false,
        color: [210, 96, 32, 160],
    }
}

fn bounds(origin_x_px: i64, origin_y_px: i64, width_px: u32, height_px: u32) -> RasterTileBounds {
    RasterTileBounds {
        origin_x_px,
        origin_y_px,
        width_px,
        height_px,
    }
}

fn pixels(tile: &RasterTile) -> impl Iterator<Item = [u8; 4]> + '_ {
    tile.rgba_premultiplied
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
}

fn crop(tile: &RasterTile, inner: RasterTileBounds) -> Vec<u8> {
    assert!(inner.origin_x_px >= tile.bounds.origin_x_px);
    assert!(inner.origin_y_px >= tile.bounds.origin_y_px);
    assert!(
        inner.origin_x_px + i64::from(inner.width_px)
            <= tile.bounds.origin_x_px + i64::from(tile.bounds.width_px)
    );
    assert!(
        inner.origin_y_px + i64::from(inner.height_px)
            <= tile.bounds.origin_y_px + i64::from(tile.bounds.height_px)
    );
    let dx = (inner.origin_x_px - tile.bounds.origin_x_px) as usize;
    let dy = (inner.origin_y_px - tile.bounds.origin_y_px) as usize;
    let source_stride = tile.bounds.width_px as usize * 4;
    let target_stride = inner.width_px as usize * 4;
    let mut out = Vec::with_capacity(target_stride * inner.height_px as usize);
    for y in 0..inner.height_px as usize {
        let start = (dy + y) * source_stride + dx * 4;
        out.extend_from_slice(&tile.rgba_premultiplied[start..start + target_stride]);
    }
    out
}

#[test]
fn shaped_arabic_glyphs_rasterize_at_scaled_fractional_positions_and_clip_to_tiles() {
    let face = loaded(ARABIC);
    assert!(face.covers('س'));
    let shaped = shape_rtl(&face, "سلام  \u{200d}", *b"Arab");
    assert!(!shaped.glyphs.is_empty());
    let original_geometry = shaped.clone();
    let run = run(&face, &shaped);
    let full_bounds = bounds(0, 0, 128, 80);
    let full = rasterize_glyph_tile(&[run], full_bounds, RasterLimits::default())
        .expect("shaped glyph outlines should produce a tile");

    assert!(
        pixels(&full).any(|pixel| pixel[3] > 0),
        "Arabic outline pixels should be present"
    );
    assert!(
        pixels(&full)
            .all(|pixel| { pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3] }),
        "the tile must be premultiplied RGBA"
    );

    let inner_bounds = bounds(4, 8, 88, 60);
    let inner = rasterize_glyph_tile(&[run], inner_bounds, RasterLimits::default())
        .expect("a viewport sub-tile should rasterize");
    assert_eq!(inner.rgba_premultiplied, crop(&full, inner_bounds));
    assert_eq!(
        shaped, original_geometry,
        "rasterization must not alter shaping geometry"
    );

    let unit_scale = RasterRun {
        pixels_per_unit: 1.0,
        ..run
    };
    let unit_tile = rasterize_glyph_tile(&[unit_scale], full_bounds, RasterLimits::default())
        .expect("unit scale should rasterize");
    assert_ne!(full.rgba_premultiplied, unit_tile.rgba_premultiplied);

    let integral_origin = RasterRun {
        origin_x_px: 7.0,
        baseline_y_px: 52.0,
        ..run
    };
    let integral_tile =
        rasterize_glyph_tile(&[integral_origin], full_bounds, RasterLimits::default())
            .expect("integral origin should rasterize");
    assert_ne!(full.rgba_premultiplied, integral_tile.rgba_premultiplied);
}

#[test]
fn shaped_hebrew_combining_marks_and_synthetic_styles_keep_shaped_geometry() {
    let face = loaded(HEBREW);
    let shaped = shape_rtl(&face, "שָׁלוֹם", *b"Hebr");
    assert!(!shaped.glyphs.is_empty());
    let original_geometry = shaped.clone();
    let base = run(&face, &shaped);
    let tile_bounds = bounds(0, 0, 120, 80);
    let regular = rasterize_glyph_tile(&[base], tile_bounds, RasterLimits::default())
        .expect("Hebrew outline and combining glyphs should rasterize");
    let synthetic = RasterRun {
        synthetic_bold: true,
        synthetic_italic: true,
        ..base
    };
    let styled = rasterize_glyph_tile(&[synthetic], tile_bounds, RasterLimits::default())
        .expect("synthetic styles should transform glyph paint");
    assert!(pixels(&regular).any(|pixel| pixel[3] != 0));
    assert_ne!(regular.rgba_premultiplied, styled.rgba_premultiplied);
    assert_eq!(
        shaped, original_geometry,
        "synthetic styles must not change advances or offsets"
    );
}

#[test]
fn zero_ink_spaces_and_default_ignorables_do_not_fail_outline_rasterization() {
    let face = loaded(ARABIC);
    let shaped = shape_rtl(&face, "   \u{200d}", *b"Arab");
    assert!(
        !shaped.glyphs.is_empty(),
        "the shaped spaces should exercise empty outlines"
    );
    let tile = rasterize_glyph_tile(
        &[run(&face, &shaped)],
        bounds(0, 0, 64, 64),
        RasterLimits::default(),
    )
    .expect("valid zero-ink glyphs must be harmless in a text line");
    assert!(pixels(&tile).all(|pixel| pixel[3] == 0));
}

#[test]
fn outlines_outside_the_tile_are_culled_before_outline_work() {
    let face = loaded(ARABIC);
    let shaped = shape_rtl(&face, "م", *b"Arab");
    let offscreen = RasterRun {
        origin_x_px: 10_000.0,
        ..run(&face, &shaped)
    };
    let tile = rasterize_glyph_tile(
        &[offscreen],
        bounds(0, 0, 64, 64),
        RasterLimits {
            max_path_commands_per_glyph: 1,
            ..RasterLimits::default()
        },
    )
    .expect("an off-tile glyph requires no outline path work");
    assert!(pixels(&tile).all(|pixel| pixel[3] == 0));
}

#[test]
fn rasterizer_bounds_glyph_scans_before_clipping_work() {
    let face = loaded(ARABIC);
    let shaped = shape_rtl(&face, "سلام", *b"Arab");
    let run = run(&face, &shaped);
    let result = rasterize_glyph_tile(
        &[run],
        bounds(0, 0, 128, 80),
        RasterLimits {
            max_glyph_scans: 1,
            ..RasterLimits::default()
        },
    );
    assert!(matches!(
        result,
        Err(RasterError::GlyphScanLimitExceeded { count, limit: 1 }) if count > 1
    ));
}

#[test]
fn rasterizer_rejects_tile_and_path_work_over_limits() {
    let face = loaded(ARABIC);
    let shaped = shape_rtl(&face, "م", *b"Arab");
    let run = run(&face, &shaped);

    assert_eq!(
        rasterize_glyph_tile(
            &[],
            bounds(0, 0, HARD_MAX_TILE_DIMENSION + 1, 1),
            RasterLimits::default(),
        ),
        Err(RasterError::TileLimitExceeded)
    );
    assert_eq!(
        rasterize_glyph_tile(
            &[],
            bounds(0, 0, 2048, 2048),
            RasterLimits {
                max_tile_bytes: 8,
                ..RasterLimits::default()
            },
        ),
        Err(RasterError::TileLimitExceeded)
    );

    let low_path_limit = RasterLimits {
        max_path_commands_per_glyph: 1,
        ..RasterLimits::default()
    };
    assert!(matches!(
        rasterize_glyph_tile(&[run], bounds(0, 0, 64, 64), low_path_limit),
        Err(RasterError::PathCommandLimitExceeded { limit: 1 })
    ));
    let low_tile_path_limit = RasterLimits {
        max_path_commands_per_tile: 1,
        ..RasterLimits::default()
    };
    assert!(matches!(
        rasterize_glyph_tile(&[run], bounds(0, 0, 64, 64), low_tile_path_limit),
        Err(RasterError::TilePathCommandLimitExceeded { limit: 1 })
    ));

    let low_scratch_limit = RasterLimits {
        max_outline_scratch_bytes: 1,
        ..RasterLimits::default()
    };
    assert!(matches!(
        rasterize_glyph_tile(&[run], bounds(0, 0, 64, 64), low_scratch_limit),
        Err(RasterError::OutlineScratchLimitExceeded { required, limit: 1 }) if required > 1
    ));

    let too_many_glyphs = Shaped {
        glyphs: vec![
            crate::ShapedGlyph {
                glyph_id: 1,
                cluster: 0,
                advance: 0.0,
                x_offset: 0.0,
                y_offset: 0.0,
            };
            2
        ],
        width: 0.0,
        rtl: false,
    };
    let too_many_run = RasterRun {
        shaped: &too_many_glyphs,
        ..run
    };
    assert_eq!(
        rasterize_glyph_tile(
            &[too_many_run],
            bounds(0, 0, 64, 64),
            RasterLimits {
                max_glyphs: 1,
                ..RasterLimits::default()
            },
        ),
        Err(RasterError::GlyphLimitExceeded)
    );

    let invalid_scale = RasterRun {
        pixels_per_unit: f32::INFINITY,
        ..run
    };
    assert_eq!(
        rasterize_glyph_tile(
            &[invalid_scale],
            bounds(0, 0, 64, 64),
            RasterLimits::default()
        ),
        Err(RasterError::InvalidRunGeometry)
    );

    let underflowing_size = RasterRun {
        font_size_units: f32::MIN_POSITIVE,
        pixels_per_unit: f32::MIN_POSITIVE,
        ..run
    };
    assert_eq!(
        rasterize_glyph_tile(
            &[underflowing_size],
            bounds(0, 0, 64, 64),
            RasterLimits::default()
        ),
        Err(RasterError::InvalidRunGeometry),
        "positive f32 inputs whose product rounds to zero must be refused"
    );
}

#[test]
fn invalid_glyph_ids_fail_visibly_instead_of_silently_dropping_content() {
    let face = loaded(ARABIC);
    let invalid = Shaped {
        glyphs: vec![crate::ShapedGlyph {
            glyph_id: u16::MAX,
            cluster: 0,
            advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        width: 10.0,
        rtl: false,
    };
    assert_eq!(
        rasterize_glyph_tile(
            &[run(&face, &invalid)],
            bounds(0, 0, 64, 64),
            RasterLimits::default(),
        ),
        Err(RasterError::UnsupportedGlyph { glyph_id: u16::MAX })
    );
}

#[test]
fn composite_glyph_preflight_refuses_work_over_the_tile_visit_budget() {
    let face = loaded(INTER);
    let font = FontRef::from_index(face.bytes(), face.collection_index()).unwrap();
    let glyph_count = font.maxp().unwrap().num_glyphs();
    let composite = (1..glyph_count)
        .find(|glyph_id| {
            preflight_glyf_glyph(
                &font,
                GlyphId::new(u32::from(*glyph_id)),
                HARD_MAX_GLYF_COMPONENT_VISITS_PER_GLYPH,
            )
            .is_ok_and(|visits| visits > 1)
        })
        .expect("Inter contains a composite glyph for the preflight regression");
    let shaped = Shaped {
        glyphs: vec![crate::ShapedGlyph {
            glyph_id: composite,
            cluster: 0,
            advance: face.glyph_advance(composite, 18.0),
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        width: face.glyph_advance(composite, 18.0),
        rtl: false,
    };
    let run = run(&face, &shaped);
    assert_eq!(
        rasterize_glyph_tile(
            &[run],
            bounds(0, 0, 64, 64),
            RasterLimits {
                max_glyf_component_visits_per_tile: 1,
                ..RasterLimits::default()
            },
        ),
        Err(RasterError::OutlineComplexityLimitExceeded { limit: 1 })
    );
}

#[cfg(feature = "bundled-script-fallback")]
#[test]
fn bundled_script_fallback_registers_named_faces_with_real_coverage_and_shaping() {
    let catalog = FontCatalog::bundled_only();
    assert!(catalog
        .families()
        .iter()
        .any(|name| name == "Noto Naskh Arabic"));
    assert!(catalog
        .families()
        .iter()
        .any(|name| name == "Noto Sans Hebrew"));

    let arabic_ref = catalog.resolve("Noto Naskh Arabic", 400, false).unwrap();
    let arabic = catalog.load(arabic_ref.primary()).unwrap();
    assert!(arabic.covers('ع'));
    assert!(!shape_rtl(&arabic, "العربية", *b"Arab").glyphs.is_empty());

    let hebrew_ref = catalog.resolve("Noto Sans Hebrew", 400, false).unwrap();
    let hebrew = catalog.load(hebrew_ref.primary()).unwrap();
    assert!(hebrew.covers('ש'));
    assert!(!shape_rtl(&hebrew, "עברית", *b"Hebr").glyphs.is_empty());

    let file_names: Vec<_> = crate::bundled_faces()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert!(file_names.contains(&"NotoNaskhArabic-Regular.ttf"));
    assert!(file_names.contains(&"NotoSansHebrew-Regular.ttf"));
}
