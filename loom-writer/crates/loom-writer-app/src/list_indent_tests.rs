//! A list item's row starts right of the body row, and its marker sits in the
//! room left of that text. Checked on the projection the page draws and on the
//! fallback rows used when the page layout is unavailable.

use super::{fallback_rows, project};
use loom_writer_core::{PageStyle, PageViewport, RichBlock, WriterDocument};

/// The body points a level of list indent adds (the core's LIST_LEVEL_INDENT_PT).
const LEVEL_INDENT_PT: f32 = 18.0;

fn sample() -> WriterDocument {
    let mut doc = WriterDocument::new("list", "List");
    doc.blocks.push(RichBlock::new(1, "paragraph", "Body text"));
    doc.blocks
        .push(RichBlock::new(2, "list-bulleted", "First item"));
    let mut nested = RichBlock::new(3, "list-bulleted", "Nested item");
    nested.style.list_level = 1;
    doc.blocks.push(nested);
    doc
}

#[test]
fn projected_list_rows_start_right_of_body_text_and_carry_their_markers() {
    let rows = project(&sample(), PageViewport::default(), None).rows;
    assert_eq!(rows.len(), 3, "one row per short block");
    let (body, item, nested) = (&rows[0], &rows[1], &rows[2]);
    assert!(
        item.x > body.x,
        "list text at {} starts right of body text at {}",
        item.x,
        body.x
    );
    assert!(
        (nested.x - item.x - LEVEL_INDENT_PT).abs() < 0.01,
        "a nested item steps in one level: {} vs {}",
        nested.x,
        item.x
    );
    assert!(body.marker.as_str().is_empty(), "body text has no marker");
    assert_eq!(item.marker.as_str(), "\u{2022}");
    assert_eq!(nested.marker.as_str(), "\u{25e6}");
}

#[test]
fn fallback_rows_keep_list_text_in_its_column_too() {
    let rows = fallback_rows(&sample(), &PageStyle::default());
    let (body, item) = (&rows[0], &rows[1]);
    assert!(
        item.x > body.x,
        "fallback list text at {} starts right of body text at {}",
        item.x,
        body.x
    );
    assert!(item.x >= 0.0, "the marker is drawn inside the content box");
    assert_eq!(item.marker.as_str(), "\u{2022}");
}
