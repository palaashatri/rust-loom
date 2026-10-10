//! Prints the numbers quoted in the design report: scan time for a 200-font
//! directory (cold, then from the cache), shaping throughput and the Inter
//! kerning bound. Run in release for realistic figures:
//! `cargo run -p loom-fonts --example measure --release`.

#[path = "../tests/common/mod.rs"]
mod common;

use loom_fonts::{FallbackPolicy, FontCatalog, ScanCache, ScanConfig, ShapeSettings};
use std::time::Instant;

fn main() {
    let dir = common::TempDir::new("measure");
    let paths = common::write_family_variants(dir.path(), 200);
    let bytes: u64 = paths
        .iter()
        .map(|p| std::fs::metadata(p).map_or(0, |m| m.len()))
        .sum();
    println!("fixture: {} fonts, {} MiB", paths.len(), bytes >> 20);

    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    let mut cache = ScanCache::new();

    let start = Instant::now();
    let cold = FontCatalog::scan_with_cache(&config, &mut cache);
    let cold_time = start.elapsed();
    println!(
        "cold scan: {:?} ({} files parsed, {} faces, {} families)",
        cold_time,
        cold.report().files_parsed,
        cold.face_count(),
        cold.families().len()
    );
    let start = Instant::now();
    let warm = FontCatalog::scan_with_cache(&config, &mut cache);
    println!(
        "cached scan: {:?} ({} hits, {} parsed)",
        start.elapsed(),
        warm.report().cache_hits,
        warm.report().files_parsed
    );
    let start = Instant::now();
    let text = cache.to_text();
    let restored = ScanCache::from_text(&text);
    println!(
        "cache text: {} KiB, serialise + parse {:?}, {} entries",
        text.len() >> 10,
        start.elapsed(),
        restored.len()
    );

    let catalog = FontCatalog::scan(&ScanConfig::with_dirs(Vec::new()));
    let font = catalog.resolve("Inter", 400, false).unwrap();
    let face = catalog.load(font.primary()).unwrap();
    let sentence = "The quick brown fox jumps over the lazy dog, again and again.";
    let rounds = 2_000;
    let start = Instant::now();
    let mut total = 0.0_f32;
    for _ in 0..rounds {
        total += face.shape(sentence, 12.0, &ShapeSettings::default()).width;
    }
    let per = start.elapsed() / rounds;
    println!(
        "shape: {per:?} per {}-char line ({:.1} pt wide)",
        sentence.len(),
        total / rounds as f32
    );
    let start = Instant::now();
    for _ in 0..rounds {
        total += catalog
            .layout_text(sentence, &font, 12.0, loom_fonts::TextDirection::Auto)
            .width;
    }
    println!(
        "layout_line (bidi + fallback + shape): {:?} per line",
        start.elapsed() / rounds
    );
    let start = Instant::now();
    let loaded = FontCatalog::scan(&ScanConfig::with_dirs(Vec::new()));
    let id = loaded.faces("Inter")[0].0;
    let _ = loaded.load(id).unwrap();
    println!("bundled catalogue + first load: {:?}", start.elapsed());
    println!("(checksum {total:.1})");

    // The machine's real font directories, for scale.
    let start = Instant::now();
    let system = FontCatalog::system();
    let report = system.report();
    println!(
        "system scan: {:?}, {} files seen, {} parsed, {} faces, {} families, {} skipped, truncated {}",
        start.elapsed(),
        report.files_seen,
        report.files_parsed,
        report.faces,
        system.families().len(),
        report.issues.len(),
        report.truncated
    );
}
