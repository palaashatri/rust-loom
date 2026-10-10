//! Memory limits: the per-file size cap and the loaded-face cache budget.

mod common;

use common::{inter, rename_family, write_family_variants, TempDir};
use loom_fonts::{FallbackPolicy, FontCatalog, FontError, ScanConfig, MAX_FONT_FILE_BYTES};
use std::fs::{self, OpenOptions};
use std::sync::Arc;

fn file_only_config(dir: &TempDir) -> ScanConfig {
    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.include_bundled = false;
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    config
}

/// A real, parseable font followed by `len - font.len()` bytes of sparse
/// zeros, so the file is `len` bytes long without using disk for the tail.
fn font_padded_to(dir: &TempDir, name: &str, family: &str, len: u64) -> std::path::PathBuf {
    let bytes = rename_family(&inter("Inter-Regular.ttf"), family);
    assert!((bytes.len() as u64) < len);
    let path = dir.path().join(name);
    fs::write(&path, &bytes).unwrap();
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(len).unwrap();
    path
}

#[test]
fn a_font_file_over_the_cap_is_refused_before_it_is_read() {
    let dir = TempDir::new("cap-over");
    font_padded_to(&dir, "Huge.ttf", "Hugeo", MAX_FONT_FILE_BYTES + 1);
    let catalog = FontCatalog::scan(&file_only_config(&dir));
    // The scan reads only table headers, so the oversized file is catalogued...
    assert!(catalog.has_family("Hugeo"));
    let id = catalog.faces("Hugeo")[0].0;
    // ...but loading it is refused, and nothing is held in memory.
    assert_eq!(catalog.load(id).err(), Some(FontError::TooLarge));
    assert_eq!(catalog.loaded_bytes(), 0);
    assert_eq!(catalog.loaded_count(), 0);
    assert!(FontError::TooLarge.to_string().contains("larger"));
}

#[test]
fn a_font_file_exactly_at_the_cap_still_loads() {
    let dir = TempDir::new("cap-edge");
    font_padded_to(&dir, "Edge.ttf", "Edgeo", MAX_FONT_FILE_BYTES);
    let catalog = FontCatalog::scan(&file_only_config(&dir));
    let id = catalog.faces("Edgeo")[0].0;
    let face = catalog.load(id).expect("a file at the cap is allowed");
    assert_eq!(face.bytes().len() as u64, MAX_FONT_FILE_BYTES);
    assert!(face.text_width("Hello", 12.0) > 0.0);
}

#[test]
fn the_cache_respects_its_budget_even_for_the_newest_face() {
    let dir = TempDir::new("budget-newest");
    write_family_variants(dir.path(), 4);
    let catalog = FontCatalog::scan(&file_only_config(&dir));
    let ids: Vec<_> = catalog.faces_in_order().map(|(id, _)| id).collect();

    // A budget below any single face: the face is returned but not retained.
    catalog.set_load_budget(1);
    let face = catalog.load(ids[0]).unwrap();
    assert!(
        face.text_width("abc", 10.0) > 0.0,
        "the caller still has it"
    );
    assert_eq!(catalog.loaded_count(), 0);
    assert_eq!(catalog.loaded_bytes(), 0);

    // Room for two faces: the third evicts the oldest, and bytes never pass
    // the budget at any point.
    let sizes: Vec<usize> = ids[..3]
        .iter()
        .map(|id| catalog.load(*id).unwrap().bytes().len())
        .collect();
    let budget = sizes[0].max(sizes[1]) + sizes[1].max(sizes[2]) + 16;
    catalog.set_load_budget(budget);
    for id in &ids[..3] {
        catalog.load(*id).unwrap();
        assert!(
            catalog.loaded_bytes() <= budget,
            "{} bytes over a budget of {budget}",
            catalog.loaded_bytes()
        );
    }
    assert!(catalog.loaded_count() < 3, "something was evicted");
    let newest = catalog.load(ids[2]).unwrap();
    assert!(
        Arc::ptr_eq(&newest, &catalog.load(ids[2]).unwrap()),
        "the newest face fits and is cached"
    );

    // Lowering the budget evicts at once.
    catalog.set_load_budget(1);
    assert_eq!(catalog.loaded_bytes(), 0);
    assert_eq!(catalog.loaded_count(), 0);
}

#[test]
fn a_face_larger_than_the_budget_does_not_evict_the_ones_that_fit() {
    let dir = TempDir::new("budget-big");
    write_family_variants(dir.path(), 6);
    let catalog = FontCatalog::scan(&file_only_config(&dir));
    let mut by_size: Vec<(usize, loom_fonts::FaceId)> = catalog
        .faces_in_order()
        .map(|(id, _)| (catalog.load(id).unwrap().bytes().len(), id))
        .collect();
    by_size.sort();
    let (small, small_id) = by_size[0];
    let (big, big_id) = *by_size.last().unwrap();
    assert!(big > small, "the fixture needs faces of different sizes");

    catalog.set_load_budget(0); // drop everything the sizing loop cached
    assert_eq!(catalog.loaded_count(), 0);
    catalog.set_load_budget(small);
    let kept = catalog.load(small_id).unwrap();
    assert_eq!(catalog.loaded_count(), 1);
    // The big face cannot be kept; it must not push the small one out.
    catalog.load(big_id).unwrap();
    assert_eq!(catalog.loaded_count(), 1);
    assert_eq!(catalog.loaded_bytes(), small);
    assert!(Arc::ptr_eq(&kept, &catalog.load(small_id).unwrap()));
}

#[test]
fn bundled_faces_do_not_count_against_the_budget() {
    let catalog = FontCatalog::bundled_only();
    catalog.set_load_budget(1);
    let id = catalog.faces("Inter")[0].0;
    let a = catalog.load(id).unwrap();
    // The bytes live in the binary, not on the heap, so the face is kept.
    assert_eq!(catalog.loaded_bytes(), 0);
    assert_eq!(catalog.loaded_count(), 1);
    assert!(Arc::ptr_eq(&a, &catalog.load(id).unwrap()));
}
