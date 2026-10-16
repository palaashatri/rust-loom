//! A table authored by real PowerPoint (through COM) imports as a table element.

use loom_present_core::{import_pptx, ElementType};

#[test]
fn a_powerpoint_authored_table_imports_with_its_cells() {
    let bytes = include_bytes!("fixtures/com_powerpoint_table.pptx");
    let deck = import_pptx(bytes).expect("import");
    let tables: Vec<_> = deck
        .slides
        .iter()
        .flat_map(|slide| slide.elements.iter())
        .filter(|e| e.element_type == ElementType::Table)
        .collect();
    assert!(
        !tables.is_empty(),
        "no table element; elements: {:?}",
        deck.slides
            .iter()
            .flat_map(|s| s
                .elements
                .iter()
                .map(|e| (e.element_type.clone(), e.width, e.height)))
            .collect::<Vec<_>>()
    );
    let table = tables[0].table().expect("table payload");
    assert!(table.row_count() >= 2 && table.col_count() >= 2);
    assert!(
        table.cell_text(0, 0).is_some_and(|t| !t.is_empty()),
        "first cell text survives"
    );
}

#[test]
fn imported_table_text_keeps_the_size_the_file_gives_it() {
    let deck = import_pptx(include_bytes!("fixtures/com_powerpoint_table.pptx")).expect("import");
    let table = deck
        .slides
        .iter()
        .flat_map(|s| s.elements.iter())
        .find(|e| e.element_type == ElementType::Table)
        .and_then(|e| e.table())
        .expect("table");
    // PowerPoint's default is 18 pt when a run names no size; the canvas and
    // the PDF both read this value.
    assert_eq!(loom_present_core::table_text_size(&table), Some(18.0));
}
