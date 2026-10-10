//! What a scan reads out of a font file: names (and which language wins),
//! aliases, style flags, and variable-font axes and instances.

mod common;

use common::{fvar_table, inter, name_table, patch_table, rename_family, with_table, TempDir};
use loom_fonts::{
    EmbeddingPermission, FallbackPolicy, FontCatalog, ScanCache, ScanConfig, ShapeSettings,
    Variation, VariationAxis,
};
use std::fs;

fn catalog_of(fonts: &[(&str, Vec<u8>)]) -> (FontCatalog, TempDir) {
    let dir = TempDir::new("faces");
    for (name, bytes) in fonts {
        fs::write(dir.path().join(name), bytes).unwrap();
    }
    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.include_bundled = false;
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    (FontCatalog::scan(&config), dir)
}

/// Inter Regular with its name table replaced by `records` plus the PostScript
/// and style names a scan expects.
fn named(records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
    let mut all = records.to_vec();
    all.extend([
        (3, 0x0409, 2, "Regular"),
        (3, 0x0409, 4, "Probe Regular"),
        (3, 0x0409, 6, "Probe-Regular"),
    ]);
    with_table(&inter("Inter-Regular.ttf"), b"name", name_table(&all))
}

fn only_face(catalog: &FontCatalog) -> loom_fonts::FaceInfo {
    assert_eq!(catalog.face_count(), 1, "{:?}", catalog.report());
    catalog.faces_in_order().next().unwrap().1.clone()
}

#[test]
fn english_names_outrank_other_languages_and_platforms() {
    // Rank order: Windows en-US, other Windows English, Macintosh English,
    // Unicode platform, other Windows, anything else.
    type Records<'a> = &'a [(u16, u16, u16, &'a str)];
    let cases: &[(Records<'_>, &str)] = &[
        (
            &[(3, 0x0407, 1, "Deutsch"), (3, 0x0409, 1, "English")],
            "English",
        ),
        (
            &[(3, 0x0409, 1, "English"), (3, 0x0407, 1, "Deutsch")],
            "English",
        ),
        (
            &[(3, 0x0409, 1, "English"), (3, 0x0809, 1, "British")],
            "English",
        ),
        (
            &[(3, 0x0407, 1, "Deutsch"), (3, 0x0809, 1, "British")],
            "British",
        ),
        (
            &[(3, 0x0809, 1, "British"), (1, 0, 1, "MacEnglish")],
            "British",
        ),
        (
            &[(3, 0x0407, 1, "Deutsch"), (1, 0, 1, "MacEnglish")],
            "MacEnglish",
        ),
        (
            &[(1, 0, 1, "MacEnglish"), (0, 0, 1, "Unicode")],
            "MacEnglish",
        ),
        (
            &[(3, 0x0407, 1, "Deutsch"), (0, 0, 1, "Unicode")],
            "Unicode",
        ),
        (
            &[(3, 0x0407, 1, "Deutsch"), (1, 1, 1, "Francais")],
            "Deutsch",
        ),
        // en-US beats a generic "en" that sorts before it in the table.
        (
            &[(3, 0x0009, 1, "Generic"), (3, 0x0409, 1, "English")],
            "English",
        ),
        // Equal ranks keep the first record in table order.
        (
            &[(3, 0x040C, 1, "Francais"), (3, 0x0407, 1, "Deutsch")],
            "Deutsch",
        ),
    ];
    for (records, expected) in cases {
        let (catalog, _dir) = catalog_of(&[("probe.ttf", named(records))]);
        let face = only_face(&catalog);
        assert_eq!(face.family, *expected, "records {records:?}");
        assert_eq!(face.postscript_name, "Probe-Regular");
    }
}

#[test]
fn a_typographic_family_name_wins_over_the_legacy_one() {
    let (catalog, _dir) = catalog_of(&[(
        "probe.ttf",
        named(&[
            (3, 0x0409, 1, "Probe Light"),
            (3, 0x0409, 16, "Probe"),
            (3, 0x0409, 17, "Light"),
        ]),
    )]);
    let face = only_face(&catalog);
    assert_eq!(face.family, "Probe");
    assert_eq!(face.legacy_family, "Probe Light");
    assert_eq!(face.style, "Light");
}

#[test]
fn family_names_in_other_languages_are_aliases_that_find_the_face() {
    let (catalog, _dir) = catalog_of(&[(
        "probe.ttf",
        named(&[
            (3, 0x0409, 1, "Probe Light"),
            (3, 0x0407, 1, "Probe Leicht"),
            (3, 0x0409, 16, "Probe"),
            (3, 0x0411, 16, "\u{30d7}\u{30ed}\u{30fc}\u{30d6}"),
            (1, 0, 16, "Probe"),
        ]),
    )]);
    let face = only_face(&catalog);
    assert_eq!(face.family, "Probe");
    assert_eq!(
        face.aliases,
        ["Probe Leicht", "\u{30d7}\u{30ed}\u{30fc}\u{30d6}"],
        "other-language names, without the primary names or repeats"
    );
    assert!(catalog.has_family("\u{30d7}\u{30ed}\u{30fc}\u{30d6}"));
    assert!(catalog.has_family("  probe   LEICHT "));
    assert_eq!(catalog.faces("Probe Leicht").len(), 1);
    // The picker still lists the one primary name.
    assert_eq!(catalog.families(), ["Probe"]);
    // Resolving an alias is a hit, not a substitution.
    let font = catalog.resolve("Probe Leicht", 400, false).unwrap();
    assert!(!font.substituted);
    assert_eq!(catalog.face(font.primary()).unwrap().family, "Probe");
}

#[test]
fn alias_lists_are_bounded() {
    let many: Vec<String> = (0..200).map(|n| format!("Alias number {n}")).collect();
    let mut records: Vec<(u16, u16, u16, &str)> = vec![(3, 0x0409, 1, "Probe")];
    for (n, name) in many.iter().enumerate() {
        records.push((3, 0x1000 + n as u16, 1, name));
    }
    let (catalog, _dir) = catalog_of(&[("probe.ttf", named(&records))]);
    let face = only_face(&catalog);
    assert_eq!(face.aliases.len(), loom_fonts::MAX_ALIASES);
}

#[test]
fn italic_comes_from_fs_selection_mac_style_or_the_italic_angle() {
    let regular = inter("Inter-Regular.ttf");
    let base = patch_table(&regular, b"OS/2", 62, &0x0040u16.to_be_bytes());
    let base = patch_table(&base, b"head", 44, &0u16.to_be_bytes());
    let base = patch_table(&base, b"post", 4, &0u32.to_be_bytes());
    let upright = |font: &[u8]| {
        let (catalog, _dir) = catalog_of(&[("probe.ttf", font.to_vec())]);
        only_face(&catalog).italic
    };
    assert!(!upright(&base), "the fixture itself is upright");
    // fsSelection bit 0 (italic) and bit 9 (oblique).
    assert!(upright(&patch_table(
        &base,
        b"OS/2",
        62,
        &0x0001u16.to_be_bytes()
    )));
    assert!(upright(&patch_table(
        &base,
        b"OS/2",
        62,
        &0x0200u16.to_be_bytes()
    )));
    // Other fsSelection bits (bold, regular, use typo metrics) are not italic.
    assert!(!upright(&patch_table(
        &base,
        b"OS/2",
        62,
        &0x0020u16.to_be_bytes()
    )));
    assert!(!upright(&patch_table(
        &base,
        b"OS/2",
        62,
        &0x0180u16.to_be_bytes()
    )));
    // head.macStyle: bit 1 is italic, bit 0 (bold) is not.
    assert!(upright(&patch_table(
        &base,
        b"head",
        44,
        &2u16.to_be_bytes()
    )));
    assert!(!upright(&patch_table(
        &base,
        b"head",
        44,
        &1u16.to_be_bytes()
    )));
    // post.italicAngle: any non-zero angle.
    let minus_twelve = (-12i32 * 65536).to_be_bytes();
    assert!(upright(&patch_table(&base, b"post", 4, &minus_twelve)));
}

#[test]
fn weight_width_and_embedding_come_from_os2() {
    let regular = inter("Inter-Regular.ttf");
    let probe = |tag_offset: usize, value: u16, mac_bold: bool| {
        let font = patch_table(&regular, b"OS/2", tag_offset, &value.to_be_bytes());
        let font = patch_table(&font, b"head", 44, &u16::from(mac_bold).to_be_bytes());
        let (catalog, _dir) = catalog_of(&[("probe.ttf", font)]);
        only_face(&catalog)
    };
    // usWeightClass: 1..=9 are hundreds, 10..=1000 literal, else macStyle.
    assert_eq!(probe(4, 700, false).weight, 700);
    assert_eq!(probe(4, 3, false).weight, 300);
    assert_eq!(probe(4, 1000, false).weight, 1000);
    assert_eq!(probe(4, 0, true).weight, 700);
    assert_eq!(probe(4, 0, false).weight, 400);
    assert_eq!(probe(4, 1500, false).weight, 400);
    // usWidthClass 1..=9, anything else is normal.
    assert_eq!(probe(6, 3, false).width, 3);
    assert_eq!(probe(6, 9, false).width, 9);
    assert_eq!(probe(6, 0, false).width, 5);
    assert_eq!(probe(6, 10, false).width, 5);
    // fsType.
    assert_eq!(
        probe(8, 0, false).embedding,
        EmbeddingPermission::Installable
    );
    assert_eq!(
        probe(8, 2, false).embedding,
        EmbeddingPermission::Restricted
    );
    assert_eq!(
        probe(8, 4, false).embedding,
        EmbeddingPermission::PreviewAndPrint
    );
    assert_eq!(probe(8, 8, false).embedding, EmbeddingPermission::Editable);
    assert!(probe(8, 0, false).subsetting_allowed);
    assert!(!probe(8, 0x0100, false).subsetting_allowed);
}

fn variable_font(instances: &[(u16, Vec<f32>)]) -> Vec<u8> {
    let font = with_table(
        &inter("Inter-Regular.ttf"),
        b"name",
        name_table(&[
            (3, 0x0409, 1, "Vario"),
            (3, 0x0409, 2, "Regular"),
            (3, 0x0409, 4, "Vario Regular"),
            (3, 0x0409, 6, "Vario-Regular"),
            (3, 0x0409, 256, "Weight"),
            (3, 0x0409, 257, "Light"),
            (3, 0x0409, 258, "Regular"),
            (3, 0x0409, 259, "Bold"),
        ]),
    );
    with_table(
        &font,
        b"fvar",
        fvar_table(&[(*b"wght", 100.0, 400.0, 900.0)], instances),
    )
}

fn wght(value: f32) -> Variation {
    Variation {
        tag: *b"wght",
        value,
    }
}

#[test]
fn a_variable_font_reports_its_axes_instances_and_default_weight() {
    let font = variable_font(&[(257, vec![300.0]), (258, vec![400.0]), (259, vec![700.0])]);
    let (catalog, _dir) = catalog_of(&[("vario.ttf", font)]);
    let face = only_face(&catalog);
    assert!(face.variable);
    assert_eq!(
        face.axes,
        [VariationAxis {
            tag: *b"wght",
            min: 100.0,
            default: 400.0,
            max: 900.0
        }]
    );
    let instances: Vec<(&str, &[f32])> = face
        .instances
        .iter()
        .map(|i| (i.style.as_str(), i.coords.as_slice()))
        .collect();
    assert_eq!(
        instances,
        [
            ("Light", &[300.0][..]),
            ("Regular", &[400.0][..]),
            ("Bold", &[700.0][..])
        ]
    );
    // The face's own weight is the default instance's, not the axis maximum.
    assert_eq!(face.weight, 400);
    assert_eq!(face.axis(*b"wght").map(|a| a.max), Some(900.0));
    assert!(face.axis(*b"wdth").is_none());
}

#[test]
fn a_named_bold_instance_means_no_synthetic_bold_and_carries_its_coordinates() {
    let font = variable_font(&[(257, vec![300.0]), (258, vec![400.0]), (259, vec![700.0])]);
    let (catalog, _dir) = catalog_of(&[("vario.ttf", font)]);

    let bold = catalog.resolve("Vario", 700, false).unwrap();
    assert!(!bold.synthetic_bold, "the Bold instance supplies real bold");
    assert_eq!(bold.chain_setup[0].variations, [wght(700.0)]);
    assert!(!bold.chain_setup[0].synthetic_bold);

    let light = catalog.resolve("Vario", 300, false).unwrap();
    assert_eq!(light.chain_setup[0].variations, [wght(300.0)]);

    // Regular is the face's default instance: no coordinates to apply.
    let regular = catalog.resolve("Vario", 400, false).unwrap();
    assert!(regular.chain_setup[0].variations.is_empty());
    assert!(!regular.synthetic_bold);

    // Heavier than any instance: the nearest (Bold) is a real bold too.
    let black = catalog.resolve("Vario", 900, false).unwrap();
    assert_eq!(black.chain_setup[0].variations, [wght(700.0)]);
    assert!(!black.synthetic_bold);
}

#[test]
fn bold_is_synthetic_when_no_instance_is_heavy_enough() {
    let font = variable_font(&[(257, vec![300.0]), (258, vec![400.0])]);
    let static_font = rename_family(&inter("Inter-Regular.ttf"), "Plain");
    let (catalog, _dir) = catalog_of(&[("vario.ttf", font), ("plain.ttf", static_font)]);
    let bold = catalog.resolve("Vario", 700, false).unwrap();
    assert!(bold.synthetic_bold);
    assert!(bold.chain_setup[0].synthetic_bold);
    assert!(bold.chain_setup[0]
        .variations
        .iter()
        .all(|v| v.value < 600.0));
    // A static family behaves the same way.
    let plain = catalog.resolve("Plain", 700, false).unwrap();
    assert!(plain.synthetic_bold);
    assert!(plain.chain_setup[0].variations.is_empty());
}

#[test]
fn variations_are_accepted_by_shaping_and_metrics() {
    // The fixture has an fvar but no gvar, so coordinates cannot change the
    // outlines or advances; this checks the plumbing accepts them.
    let font = variable_font(&[(259, vec![700.0])]);
    let (catalog, _dir) = catalog_of(&[("vario.ttf", font)]);
    let id = catalog.faces("Vario")[0].0;
    let face = catalog.load(id).unwrap();
    let plain = face.shape("Hello", 16.0, &ShapeSettings::default());
    let varied = face.shape(
        "Hello",
        16.0,
        &ShapeSettings {
            variations: vec![wght(700.0)],
            ..ShapeSettings::default()
        },
    );
    assert_eq!(plain.glyphs.len(), varied.glyphs.len());
    assert!((plain.width - varied.width).abs() < 1e-3);
    let metrics = face.line_metrics_at(16.0, &[wght(700.0)]);
    assert!((metrics.ascent - face.line_metrics(16.0).ascent).abs() < 1e-3);

    // A layout carries the coordinates of the matched instance on its runs,
    // so a renderer draws the glyphs at the same coordinates.
    let bold = catalog.resolve("Vario", 700, false).unwrap();
    let layout = catalog.layout_text("Hello", &bold, 16.0, loom_fonts::TextDirection::Auto);
    assert_eq!(layout.runs[0].variations, [wght(700.0)]);
    let regular = catalog.resolve("Vario", 400, false).unwrap();
    let layout = catalog.layout_text("Hello", &regular, 16.0, loom_fonts::TextDirection::Auto);
    assert!(layout.runs[0].variations.is_empty());
}

/// Optional check against a real variable font: set
/// `LOOM_FONTS_VARIABLE_FONT` to the path of a variable TrueType file with a
/// `wght` axis (for example the Inter variable font Slint ships).
#[test]
fn a_real_variable_font_gets_wider_when_the_weight_axis_rises() {
    let Some(path) = std::env::var_os("LOOM_FONTS_VARIABLE_FONT") else {
        eprintln!("skipped: LOOM_FONTS_VARIABLE_FONT is not set");
        return;
    };
    let bytes = fs::read(path).unwrap();
    let (catalog, _dir) = catalog_of(&[("real.ttf", bytes)]);
    let info = only_face(&catalog);
    assert!(info.variable, "the file must be a variable font");
    let axis = *info.axis(*b"wght").expect("a wght axis");
    let id = catalog.faces_in_order().next().unwrap().0;
    let face = catalog.load(id).unwrap();
    let text = "Wonderful typography";
    let at = |value: f32| {
        face.shape(
            text,
            20.0,
            &ShapeSettings {
                variations: vec![wght(value)],
                ..ShapeSettings::default()
            },
        )
        .width
    };
    let (light, heavy) = (at(axis.min), at(axis.max));
    println!(
        "wght {} -> {light:.2}, wght {} -> {heavy:.2}",
        axis.min, axis.max
    );
    assert!(heavy > light + 0.5, "{light} vs {heavy}");
    let default = face.text_width(text, 20.0);
    assert!((default - at(axis.default)).abs() < 0.01);

    // Through resolve and layout: a request for bold selects the Bold named
    // instance and the layout is shaped at its coordinates.
    let family = info.family.clone();
    let regular = catalog.resolve(&family, 400, false).unwrap();
    let bold = catalog.resolve(&family, 700, false).unwrap();
    let wide = catalog.layout_text(text, &bold, 20.0, loom_fonts::TextDirection::Auto);
    let normal = catalog.layout_text(text, &regular, 20.0, loom_fonts::TextDirection::Auto);
    println!(
        "bold instance variations {:?}, widths regular {:.2} bold {:.2}",
        bold.chain_setup[0].variations, normal.width, wide.width
    );
    if !bold.chain_setup[0].variations.is_empty() {
        assert!(!bold.synthetic_bold);
        assert_eq!(wide.runs[0].variations, bold.chain_setup[0].variations);
        assert!(wide.width > normal.width + 0.5, "bold must be wider");
    }
}

#[test]
fn aliases_axes_and_instances_survive_the_cache_text() {
    let font = variable_font(&[(257, vec![300.0]), (259, vec![700.0])]);
    let font = with_table(
        &font,
        b"name",
        name_table(&[
            (3, 0x0409, 1, "Vario"),
            (3, 0x0407, 1, "Vario Deutsch"),
            (3, 0x0409, 2, "Regular"),
            (3, 0x0409, 4, "Vario Regular"),
            (3, 0x0409, 6, "Vario-Regular"),
            (3, 0x0409, 257, "Light"),
            (3, 0x0409, 259, "Bold"),
        ]),
    );
    let dir = TempDir::new("cache-faces");
    fs::write(dir.path().join("vario.ttf"), font).unwrap();
    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.include_bundled = false;
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    let mut cache = ScanCache::new();
    let first = FontCatalog::scan_with_cache(&config, &mut cache);

    let mut restored = ScanCache::from_text(&cache.to_text());
    assert_eq!(restored, cache);
    let second = FontCatalog::scan_with_cache(&config, &mut restored);
    assert_eq!(second.report().cache_hits, 1);
    let (a, b) = (only_face(&first), only_face(&second));
    assert_eq!(a, b);
    assert_eq!(b.aliases, ["Vario Deutsch"]);
    assert_eq!(b.axes.len(), 1);
    assert_eq!(b.instances.len(), 2);
    assert!(second.has_family("vario deutsch"));
}
