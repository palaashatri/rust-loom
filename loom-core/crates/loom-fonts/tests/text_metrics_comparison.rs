//! Compares this crate's Inter advances with the hard-coded tables in
//! `loom-writer-core/src/text_metrics.rs` (1/1000 em, ASCII 32..=126, kerning
//! not applied there). The tables are copied here as a fixture so the check
//! does not depend on Writer's source layout; Writer itself is not changed.
//!
//! Run with `cargo test -p loom-fonts --test text_metrics_comparison -- --nocapture`
//! to print the comparison tables.

use loom_fonts::{FallbackPolicy, FontCatalog, ScanConfig, ShapeSettings};

const WRITER_REGULAR: [u16; 95] = [
    281, 288, 466, 633, 642, 982, 644, 300, 365, 365, 501, 662, 288, 460, 288, 360, 631, 407, 610,
    618, 646, 608, 620, 566, 619, 620, 288, 302, 662, 662, 662, 511, 966, 690, 654, 730, 722, 601,
    590, 746, 743, 269, 571, 672, 565, 903, 753, 765, 639, 765, 644, 642, 646, 744, 690, 985, 682,
    679, 629, 365, 360, 365, 471, 456, 323, 562, 612, 571, 612, 583, 370, 613, 591, 242, 242, 549,
    242, 876, 591, 600, 612, 612, 376, 528, 327, 591, 562, 818, 546, 562, 552, 426, 333, 426, 662,
];

const WRITER_BOLD: [u16; 95] = [
    237, 338, 552, 649, 655, 1016, 672, 339, 377, 377, 559, 679, 334, 468, 334, 388, 674, 431, 630,
    646, 676, 639, 649, 582, 651, 649, 334, 343, 679, 679, 679, 560, 1016, 747, 662, 740, 722, 607,
    587, 750, 747, 281, 584, 719, 565, 932, 762, 771, 648, 777, 657, 655, 667, 732, 747, 1038, 738,
    731, 664, 377, 388, 377, 487, 476, 365, 581, 630, 588, 630, 596, 398, 632, 623, 271, 271, 580,
    271, 913, 623, 613, 630, 630, 407, 560, 366, 623, 600, 850, 580, 602, 573, 469, 372, 469, 679,
];

fn catalog() -> FontCatalog {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    FontCatalog::scan(&config)
}

/// Compares one weight; returns (worst relative deviation, mismatch list).
fn compare(weight: u16, table: &[u16; 95], label: &str) -> (f32, Vec<String>) {
    let catalog = catalog();
    let font = catalog.resolve("Inter", weight, false).unwrap();
    let face = catalog.load(font.primary()).unwrap();
    let mut worst = 0.0_f32;
    let mut rows = Vec::new();
    let mut offenders = Vec::new();
    for (i, expected) in table.iter().enumerate() {
        let ch = char::from(32 + i as u8);
        let ours = face.glyph_advance(face.glyph_id(ch).unwrap(), 1000.0);
        let expected = f32::from(*expected);
        let relative = (ours - expected).abs() / expected;
        worst = worst.max(relative);
        rows.push(format!(
            "{label} {ch:?} writer {expected:>5.0} loom-fonts {ours:>7.1} diff {:+.2}%",
            (ours - expected) / expected * 100.0
        ));
        if relative > 0.005 {
            offenders.push(rows.last().unwrap().clone());
        }
    }
    println!("{}", rows.join("\n"));
    (worst, offenders)
}

#[test]
fn inter_regular_ascii_advances_match_the_writer_table_within_half_a_percent() {
    let (worst, offenders) = compare(400, &WRITER_REGULAR, "regular");
    println!("regular worst deviation {:.3}%", worst * 100.0);
    assert!(offenders.is_empty(), "over 0.5%:\n{}", offenders.join("\n"));
}

#[test]
fn inter_bold_ascii_advances_match_the_writer_table_within_half_a_percent() {
    let (worst, offenders) = compare(700, &WRITER_BOLD, "bold");
    println!("bold worst deviation {:.3}%", worst * 100.0);
    assert!(offenders.is_empty(), "over 0.5%:\n{}", offenders.join("\n"));
}

#[test]
fn sentence_widths_agree_with_the_table_and_kerning_is_the_only_difference() {
    // Unkerned widths agree to 0.5%; kerned widths differ only by kerning.
    let catalog = catalog();
    let font = catalog.resolve("Inter", 400, false).unwrap();
    let face = catalog.load(font.primary()).unwrap();
    let sentences = [
        "The quick brown fox jumps over the lazy dog.",
        "Write, format, and export documents.",
        "AVATAR Toyota Wave Type Yellow",
        "Revenue grew 12.5% to $4,300,000 in Q3 2025 (net)",
        "iiiiiiiiii WWWWWWWWWW",
    ];
    let size = 12.0;
    let plain = ShapeSettings {
        kerning: false,
        ..ShapeSettings::default()
    };
    println!("sentence: writer-table | unkerned | kerned (points at 12 pt)");
    for text in sentences {
        let table: f32 = text
            .chars()
            .map(|c| f32::from(WRITER_REGULAR[c as usize - 32]))
            .sum::<f32>()
            * size
            / 1000.0;
        let unkerned = face.shape(text, size, &plain).width;
        let kerned = face.text_width(text, size);
        println!(
            "{text:?}: {table:.2} | {unkerned:.2} ({:+.2}%) | {kerned:.2} ({:+.2}%)",
            (unkerned - table) / table * 100.0,
            (kerned - table) / table * 100.0
        );
        assert!(
            (unkerned - table).abs() / table < 0.005,
            "unkerned {unkerned} vs table {table}"
        );
        // Kerning moves a sentence by a few percent either way (pairs such as
        // "AV" tighten, "WW" and "ii" loosen); Writer's table cannot see it.
        assert!((kerned - table).abs() / table < 0.05);
    }
}
