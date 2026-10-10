mod common;

use common::{inter, make_collection, rename_family, write_family_variants, TempDir};
use loom_fonts::{
    Availability, EmbeddingPermission, FaceSource, FallbackPolicy, FontCatalog, FontError,
    ScanCache, ScanConfig, ScanLimits,
};
use std::fs;
use std::time::Duration;

fn config(dirs: Vec<std::path::PathBuf>) -> ScanConfig {
    let mut config = ScanConfig::with_dirs(dirs);
    // Make the fallback chain independent of the host's platform default.
    config.policy = FallbackPolicy::new(["Liberation Sans"]);
    config
}

fn bundled() -> FontCatalog {
    FontCatalog::scan(&config(Vec::new()))
}

#[test]
fn bundled_catalog_lists_inter_with_its_six_faces() {
    let catalog = bundled();
    assert_eq!(catalog.families(), ["Inter"]);
    let faces = catalog.faces("Inter");
    let shape: Vec<(u16, bool)> = faces.iter().map(|(_, f)| (f.weight, f.italic)).collect();
    assert_eq!(
        shape,
        vec![
            (400, false),
            (400, true),
            (500, false),
            (600, false),
            (700, false),
            (700, true)
        ]
    );
    let regular = faces[0].1;
    assert_eq!(regular.family, "Inter");
    assert_eq!(regular.postscript_name, "Inter-Regular");
    assert_eq!(regular.width, 5);
    assert!(!regular.monospace);
    assert_eq!(regular.embedding, EmbeddingPermission::Installable);
    assert!(matches!(
        regular.source,
        FaceSource::Bundled("Inter-Regular.ttf")
    ));
}

#[test]
fn legacy_style_linked_names_resolve_to_the_same_family() {
    let catalog = bundled();
    // Inter Medium is a separate legacy family but part of typographic Inter.
    let medium = catalog.faces("inter medium");
    assert_eq!(medium.len(), 1);
    assert_eq!(medium[0].1.weight, 500);
    assert_eq!(
        catalog.families().len(),
        1,
        "no separate 'Inter Medium' row"
    );
    let resolved = catalog.resolve("Inter Medium", 400, false).unwrap();
    assert_eq!(catalog.face(resolved.primary()).unwrap().weight, 500);
    assert!(!resolved.substituted);
}

#[test]
fn lookup_is_case_and_whitespace_insensitive() {
    let catalog = bundled();
    assert!(catalog.has_family("  INTER "));
    assert_eq!(catalog.faces("iNtEr").len(), 6);
    assert!(!catalog.has_family("Comic Sans"));
}

#[test]
fn an_injected_directory_adds_families_sorted_and_deterministically() {
    let dir = TempDir::new("dirs");
    let regular = inter("Inter-Regular.ttf");
    fs::write(dir.path().join("b.ttf"), rename_family(&regular, "Zebra")).unwrap();
    fs::write(dir.path().join("a.otf"), rename_family(&regular, "Alpha")).unwrap();
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("c.ttf"), rename_family(&regular, "Mango")).unwrap();

    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    assert_eq!(catalog.families(), ["Alpha", "Inter", "Mango", "Zebra"]);
    let again = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    assert_eq!(catalog.families(), again.families());
    let order = |c: &FontCatalog| -> Vec<String> {
        c.faces_in_order()
            .map(|(_, f)| f.postscript_name.clone())
            .collect()
    };
    assert_eq!(order(&catalog), order(&again));
}

#[test]
fn the_same_face_in_two_directories_appears_once_and_bundled_wins() {
    let first = TempDir::new("dup1");
    let second = TempDir::new("dup2");
    for dir in [&first, &second] {
        fs::write(
            dir.path().join("Inter-Regular.ttf"),
            inter("Inter-Regular.ttf"),
        )
        .unwrap();
        fs::write(
            dir.path().join("Other.ttf"),
            rename_family(&inter("Inter-Bold.ttf"), "Other"),
        )
        .unwrap();
    }
    let catalog = FontCatalog::scan(&config(vec![
        first.path().to_owned(),
        second.path().to_owned(),
    ]));
    assert_eq!(
        catalog.faces("Inter").len(),
        6,
        "installed copy deduplicated"
    );
    assert_eq!(catalog.faces("Other").len(), 1);
    let regular = &catalog.faces("Inter")[0].1;
    assert!(matches!(regular.source, FaceSource::Bundled(_)));
    // Without the bundled faces the first directory wins.
    let mut no_bundle = config(vec![first.path().to_owned(), second.path().to_owned()]);
    no_bundle.include_bundled = false;
    let installed = FontCatalog::scan(&no_bundle);
    let source = &installed.faces("Inter")[0].1.source;
    assert_eq!(
        source,
        &FaceSource::File(first.path().join("Inter-Regular.ttf"))
    );
}

#[test]
fn a_collection_yields_every_member_with_its_index() {
    let dir = TempDir::new("ttc");
    let regular = inter("Inter-Regular.ttf");
    let collection = make_collection(&[
        rename_family(&regular, "TwinA"),
        rename_family(&regular, "TwinB"),
    ]);
    fs::write(dir.path().join("twins.ttc"), collection).unwrap();
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.include_bundled = false;
    let catalog = FontCatalog::scan(&cfg);
    assert_eq!(catalog.families(), ["TwinA", "TwinB"]);
    let a = catalog.faces("TwinA")[0];
    let b = catalog.faces("TwinB")[0];
    assert_eq!((a.1.index, b.1.index), (0, 1));
    // Both members load and measure.
    let width_a = catalog.load(a.0).unwrap().text_width("Hello", 16.0);
    let width_b = catalog.load(b.0).unwrap().text_width("Hello", 16.0);
    assert!(width_a > 0.0 && (width_a - width_b).abs() < 1e-3);
}

#[test]
fn damaged_and_foreign_files_are_reported_not_fatal() {
    let dir = TempDir::new("bad");
    fs::write(dir.path().join("garbage.ttf"), b"this is not a font at all").unwrap();
    fs::write(dir.path().join("empty.otf"), b"").unwrap();
    let truncated = &inter("Inter-Regular.ttf")[..400];
    fs::write(dir.path().join("cut.ttf"), truncated).unwrap();
    fs::write(dir.path().join("web.woff2"), b"wOF2....").unwrap();
    fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
    fs::write(
        dir.path().join("good.ttf"),
        rename_family(&inter("Inter-Regular.ttf"), "Gooda"),
    )
    .unwrap();

    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    let report = catalog.report();
    assert!(catalog.has_family("Gooda"));
    assert_eq!(report.files_seen, 4, "woff2 and txt are not candidates");
    assert_eq!(report.issues.len(), 3);
    assert!(report
        .issues
        .iter()
        .all(|issue| !issue.reason.is_empty() && issue.path.starts_with(dir.path())));
}

#[test]
fn a_missing_directory_is_not_an_error() {
    let catalog = FontCatalog::scan(&config(vec!["/definitely/not/here".into()]));
    assert_eq!(catalog.families(), ["Inter"]);
    assert!(!catalog.report().truncated);
}

#[test]
fn resolve_matches_weight_and_italic_within_a_family() {
    let catalog = bundled();
    let check = |weight: u16, italic: bool| {
        let font = catalog.resolve("Inter", weight, italic).unwrap();
        let face = catalog.face(font.primary()).unwrap();
        (
            face.weight,
            face.italic,
            font.synthetic_bold,
            font.synthetic_italic,
        )
    };
    assert_eq!(check(400, false), (400, false, false, false));
    assert_eq!(check(700, true), (700, true, false, false));
    assert_eq!(check(500, false), (500, false, false, false));
    // 300 has nothing lighter: the next heavier face.
    assert_eq!(check(300, false), (400, false, false, false));
    // 900 has nothing heavier: Bold.
    assert_eq!(check(900, false), (700, false, false, false));
    // 600 italic exists only as Bold Italic.
    assert_eq!(check(600, true), (700, true, false, false));
}

#[test]
fn a_family_without_italic_asks_for_synthesis() {
    let dir = TempDir::new("noital");
    fs::write(
        dir.path().join("only.ttf"),
        rename_family(&inter("Inter-Regular.ttf"), "Plain"),
    )
    .unwrap();
    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    let font = catalog.resolve("Plain", 700, true).unwrap();
    assert!(!font.substituted);
    assert!(font.synthetic_italic);
    assert!(font.synthetic_bold);
    assert_eq!(catalog.face(font.primary()).unwrap().family, "Plain");
}

#[test]
fn a_missing_family_is_substituted_and_flagged() {
    let catalog = bundled();
    let font = catalog.resolve("Papyrus Deluxe", 400, false).unwrap();
    assert!(font.substituted);
    assert!(!font.metric_compatible);
    assert_eq!(font.requested_family, "Papyrus Deluxe");
    let primary = catalog.face(font.primary()).unwrap();
    assert_eq!(primary.family, "Inter");
    assert!(!font.chain.is_empty());
    assert_eq!(
        catalog.availability("Papyrus Deluxe"),
        Availability::Missing {
            substitute: "Inter".into()
        }
    );
}

#[test]
fn generic_families_resolve_without_being_substituted() {
    let catalog = bundled();
    for generic in ["sans-serif", "serif", "monospace", "system-ui"] {
        let font = catalog.resolve(generic, 400, false).unwrap();
        assert!(
            !font.substituted,
            "{generic} is a keyword, not a missing font"
        );
        assert_eq!(catalog.availability(generic), Availability::Generic);
        assert!(!font.chain.is_empty());
    }
}

#[test]
fn a_metric_compatible_stand_in_is_used_and_marked() {
    let dir = TempDir::new("metric");
    // Arimo is the metric-compatible stand-in the table lists for Arial.
    let arimo = rename_family(&inter("Inter-Regular.ttf"), "Arimo");
    fs::write(dir.path().join("arimo.ttf"), arimo).unwrap();
    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    let font = catalog.resolve("Arial", 400, false).unwrap();
    assert!(font.substituted);
    assert!(font.metric_compatible);
    assert_eq!(catalog.face(font.primary()).unwrap().family, "Arimo");
    assert_eq!(
        catalog.availability("Arial"),
        Availability::MetricCompatible {
            substitute: "Arimo".into()
        }
    );
    assert_eq!(catalog.availability("Arimo"), Availability::Installed);
    assert_eq!(catalog.availability("Inter"), Availability::Bundled);
}

#[test]
fn the_fallback_chain_lists_policy_families_then_inter_without_repeats() {
    let dir = TempDir::new("chain");
    fs::write(
        dir.path().join("l.ttf"),
        rename_family(&inter("Inter-Regular.ttf"), "Zzzzz"),
    )
    .unwrap();
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.policy = FallbackPolicy::new(["Zzzzz", "Missing Font"]);
    let catalog = FontCatalog::scan(&cfg);
    let font = catalog.resolve("Inter", 400, false).unwrap();
    let families: Vec<&str> = font
        .chain
        .iter()
        .map(|id| catalog.face(*id).unwrap().family.as_str())
        .collect();
    assert_eq!(families, ["Inter", "Zzzzz"]);
    let unique: std::collections::HashSet<_> = font.chain.iter().collect();
    assert_eq!(unique.len(), font.chain.len());
}

#[test]
fn an_empty_catalogue_cannot_resolve() {
    let catalog = FontCatalog::empty(FallbackPolicy::new(Vec::<String>::new()));
    assert!(catalog.is_empty());
    assert_eq!(
        catalog.resolve("Inter", 400, false),
        Err(FontError::EmptyCatalog)
    );
}

#[test]
fn scan_limits_stop_a_scan_and_say_so() {
    let dir = TempDir::new("limit");
    write_family_variants(dir.path(), 30);
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.limits = ScanLimits {
        max_files: 10,
        ..ScanLimits::default()
    };
    let catalog = FontCatalog::scan(&cfg);
    assert!(catalog.report().truncated);
    assert!(catalog.report().files_seen <= 10);

    cfg.limits = ScanLimits {
        max_duration: Some(Duration::ZERO),
        ..ScanLimits::default()
    };
    let catalog = FontCatalog::scan(&cfg);
    assert!(catalog.report().truncated);
    // The bundled faces are always present, even for a stopped scan.
    assert!(catalog.has_family("Inter"));
}

#[test]
fn the_cache_skips_unchanged_files_and_reparses_changed_ones() {
    let dir = TempDir::new("cache");
    let paths = write_family_variants(dir.path(), 12);
    let cfg = config(vec![dir.path().to_owned()]);
    let mut cache = ScanCache::new();

    let cold = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(cold.report().files_parsed, 12);
    assert_eq!(cold.report().cache_hits, 0);
    assert_eq!(cache.len(), 12);

    let warm = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(warm.report().files_parsed, 0);
    assert_eq!(warm.report().cache_hits, 12);
    assert_eq!(warm.families(), cold.families());
    assert_eq!(warm.face_count(), cold.face_count());

    // A changed size invalidates one entry; a removed file leaves the cache.
    let mut bytes = fs::read(&paths[0]).unwrap();
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    fs::write(&paths[0], bytes).unwrap();
    fs::remove_file(&paths[1]).unwrap();
    let after = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(after.report().files_parsed, 1);
    assert_eq!(after.report().cache_hits, 10);
    assert_eq!(cache.len(), 11);
}

#[test]
fn the_cache_survives_a_text_round_trip() {
    let dir = TempDir::new("cachetxt");
    write_family_variants(dir.path(), 8);
    let cfg = config(vec![dir.path().to_owned()]);
    let mut cache = ScanCache::new();
    let first = FontCatalog::scan_with_cache(&cfg, &mut cache);

    let text = cache.to_text();
    let mut restored = ScanCache::from_text(&text);
    assert_eq!(restored, cache);
    let second = FontCatalog::scan_with_cache(&cfg, &mut restored);
    assert_eq!(second.report().files_parsed, 0);
    assert_eq!(second.report().cache_hits, 8);
    assert_eq!(second.families(), first.families());
    let weights = |c: &FontCatalog| -> Vec<(u16, bool)> {
        c.faces_in_order()
            .map(|(_, f)| (f.weight, f.italic))
            .collect()
    };
    assert_eq!(weights(&second), weights(&first));
}

#[test]
fn loaded_faces_are_cached_and_evicted_within_the_budget() {
    let dir = TempDir::new("budget");
    write_family_variants(dir.path(), 6);
    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    let ids: Vec<_> = catalog.faces_in_order().map(|(id, _)| id).collect();
    // Room for exactly one face: the cache keeps the newest and evicts the rest.
    let one = catalog.load(ids[0]).unwrap().bytes().len();
    catalog.set_load_budget(one);
    let a = catalog.load(ids[0]).unwrap();
    assert_eq!(catalog.loaded_count(), 1);
    let again = catalog.load(ids[0]).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&a, &again),
        "cache hit shares the face"
    );
    // Other faces of different sizes either replace it or are not retained;
    // either way the bytes held never exceed the budget.
    catalog.load(ids[1]).unwrap();
    assert!(catalog.loaded_bytes() <= one);
    catalog.load(ids[2]).unwrap();
    assert!(catalog.loaded_bytes() <= one);
    assert!(catalog.loaded_count() <= 1, "older faces were evicted");
    // An evicted face still loads again.
    assert!(catalog.load(ids[0]).unwrap().text_width("abc", 10.0) > 0.0);
}

#[test]
fn scanning_two_hundred_fonts_is_bounded_and_complete() {
    let dir = TempDir::new("two-hundred");
    write_family_variants(dir.path(), 200);
    let started = std::time::Instant::now();
    let catalog = FontCatalog::scan(&config(vec![dir.path().to_owned()]));
    let elapsed = started.elapsed();
    println!("200-font scan took {elapsed:?}");
    let report = catalog.report();
    assert_eq!(report.files_seen, 200);
    assert_eq!(report.files_parsed, 200);
    assert!(report.issues.is_empty() && !report.truncated);
    // 200 renamed faces in 34 families plus the six bundled Inter faces.
    assert_eq!(catalog.face_count(), 206);
    assert_eq!(catalog.families().len(), 35);
    // Generous enough for a loaded CI machine and a debug build; the point is
    // that only table headers are read, not the 78 MiB of glyph data.
    assert!(elapsed < Duration::from_secs(10), "scan took {elapsed:?}");
}

#[test]
fn bundled_only_is_complete_without_touching_the_disk() {
    let catalog = FontCatalog::bundled_only();
    assert_eq!(catalog.families(), ["Inter"]);
    assert_eq!(catalog.report().files_seen, 0);
    assert!(catalog.resolve("Inter", 400, false).is_ok());
}

#[test]
fn the_catalogue_and_loaded_faces_can_cross_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FontCatalog>();
    assert_send_sync::<loom_fonts::LoadedFace>();
    assert_send_sync::<loom_fonts::FontRef>();
    let catalog = std::sync::Arc::new(bundled());
    let handles: Vec<_> = (0..4)
        .map(|n| {
            let catalog = std::sync::Arc::clone(&catalog);
            std::thread::spawn(move || {
                let font = catalog.resolve("Inter", 400, false).unwrap();
                let text = format!("Thread {n} measures this sentence");
                catalog.text_width(&text, &font, 14.0)
            })
        })
        .collect();
    let widths: Vec<f32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(widths.iter().all(|w| *w > 0.0));
}

#[test]
fn a_file_that_vanishes_after_the_scan_fails_to_load_cleanly() {
    let dir = TempDir::new("gone");
    let paths = write_family_variants(dir.path(), 1);
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.include_bundled = false;
    let catalog = FontCatalog::scan(&cfg);
    fs::remove_file(&paths[0]).unwrap();
    let id = catalog.faces_in_order().next().unwrap().0;
    assert!(matches!(catalog.load(id), Err(FontError::Io(_))));
}
