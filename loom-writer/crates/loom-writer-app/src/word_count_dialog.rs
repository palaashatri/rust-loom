use std::rc::Rc;

use loom_writer_core::{WriterDocument, CHART_BLOCK_KIND, IMAGE_BLOCK_KIND, TABLE_BLOCK_KIND};
use slint::{ComponentHandle, SharedString};

use crate::{GuiState, WordCountState, WriterApp};

/// Computed document statistics projected into [`WordCountState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentStatisticsSnapshot {
    pub words: i32,
    pub chars_with_spaces: i32,
    pub chars_without_spaces: i32,
    pub paragraphs: i32,
    pub sentences: i32,
    pub pages: i32,
    pub tables: i32,
    pub images: i32,
    pub charts: i32,
    pub footnotes: i32,
    pub comments: i32,
    pub tracked_changes: i32,
    pub reading_time: String,
}

fn clamp_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Computes live word, character, pagination, object, and review statistics from a document.
pub(crate) fn compute_statistics(doc: &WriterDocument) -> DocumentStatisticsSnapshot {
    let stats = doc.statistics();
    let style = doc.page.page_style();
    let pages = doc
        .paginate(&style)
        .map(|paginated| paginated.len().max(1))
        .unwrap_or(1);

    let mut tables = 0usize;
    let mut images = 0usize;
    let mut charts = 0usize;
    for block in &doc.blocks {
        if block.kind == TABLE_BLOCK_KIND {
            tables += 1;
        } else if block.kind == IMAGE_BLOCK_KIND {
            images += 1;
        } else if block.kind == CHART_BLOCK_KIND {
            charts += 1;
        }
    }
    let paragraphs = stats.block_count.saturating_sub(tables + images + charts);
    let reading_minutes = crate::reading_time::minutes_for_words(stats.word_count);

    DocumentStatisticsSnapshot {
        words: clamp_i32(stats.word_count),
        chars_with_spaces: clamp_i32(stats.char_count),
        chars_without_spaces: clamp_i32(stats.char_count_no_spaces),
        paragraphs: clamp_i32(paragraphs),
        sentences: clamp_i32(stats.sentence_count),
        pages: clamp_i32(pages),
        tables: clamp_i32(tables),
        images: clamp_i32(images),
        charts: clamp_i32(charts),
        footnotes: clamp_i32(doc.footnotes.len()),
        comments: clamp_i32(doc.comments.len()),
        tracked_changes: clamp_i32(doc.tracked_changes.revisions().len()),
        reading_time: format!("{reading_minutes} min"),
    }
}

/// Refreshes [`WordCountState`] properties from the active document.
pub(crate) fn refresh(app: &WriterApp, state: &GuiState) {
    let snapshot = {
        let doc = state.current.borrow();
        compute_statistics(&doc)
    };
    let modal = app.global::<WordCountState>();
    modal.set_words(snapshot.words);
    modal.set_chars_with_spaces(snapshot.chars_with_spaces);
    modal.set_chars_without_spaces(snapshot.chars_without_spaces);
    modal.set_paragraphs(snapshot.paragraphs);
    modal.set_sentences(snapshot.sentences);
    modal.set_pages(snapshot.pages);
    modal.set_tables(snapshot.tables);
    modal.set_images(snapshot.images);
    modal.set_charts(snapshot.charts);
    modal.set_footnotes(snapshot.footnotes);
    modal.set_comments(snapshot.comments);
    modal.set_tracked_changes(snapshot.tracked_changes);
    modal.set_reading_time(SharedString::from(snapshot.reading_time));
}

/// Refreshes counts and opens the Word Count & Document Statistics modal.
pub(crate) fn open(app: &WriterApp, state: &GuiState) {
    refresh(app, state);
    app.global::<WordCountState>().set_open(true);
    app.set_status_left("Opened Word Count & Document Statistics".into());
}

/// Connects Word Count modal open/close callbacks to Writer state and focus return.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let modal = app.global::<WordCountState>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        modal.on_open_dialog(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            open(&app, &state);
        });
    }
    {
        let app_ref = app.as_weak();
        modal.on_close_dialog(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            app.global::<WordCountState>().set_open(false);
            app.invoke_focus_page();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};
    use loom_desktop::ScriptedFileDialogs;
    use loom_writer_core::{ImageFormat, InlineImage, RichBlock};
    use std::sync::Arc;

    #[test]
    fn empty_document_reports_zero_counts_and_single_page() {
        let doc = WriterDocument::new("empty", "Empty");
        let snap = compute_statistics(&doc);

        assert_eq!(snap.words, 0);
        assert_eq!(snap.chars_with_spaces, 0);
        assert_eq!(snap.chars_without_spaces, 0);
        assert_eq!(snap.paragraphs, 0);
        assert_eq!(snap.sentences, 0);
        assert_eq!(snap.pages, 1);
        assert_eq!(snap.tables, 0);
        assert_eq!(snap.images, 0);
        assert_eq!(snap.charts, 0);
        assert_eq!(snap.footnotes, 0);
        assert_eq!(snap.comments, 0);
        assert_eq!(snap.tracked_changes, 0);
        assert_eq!(snap.reading_time, "0 min");
    }

    #[test]
    fn statistics_count_paragraphs_tables_images_charts_annotations_and_revisions() {
        let mut doc = WriterDocument::new("rich", "Rich Metrics Doc");
        let p1 = doc.next_id();
        doc.push(RichBlock::new(p1, "heading1", "Quarterly Review."));
        let p2 = doc.next_id();
        doc.push(RichBlock::new(
            p2,
            "paragraph",
            "Revenue grew twenty percent across core regions!",
        ));
        doc.insert_footnote(p1, 4, "Methodology note")
            .expect("insert footnote");
        doc.add_comment_thread(p2, 0, 7, "Verify regional breakdown.")
            .expect("add comment");
        doc.tracked_changes.set_recording(true);
        let updated_stream = format!("{} Added edit.", doc.editor_text());
        doc.replace_editor_text_at(&updated_stream, None)
            .expect("record tracked edit");
        doc.tracked_changes.set_recording(false);

        let end_offset = doc.editor_text().len();
        doc.insert_table_block(end_offset, 2, 2)
            .expect("insert table");

        let mut img_bytes = Vec::new();
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(
                &mut std::io::Cursor::new(&mut img_bytes),
                image::ImageFormat::Png,
            )
            .expect("encode 1x1 png");
        let img_id = doc.next_id();
        let mut img_block = RichBlock::new(img_id, IMAGE_BLOCK_KIND, "");
        img_block.image = Some(InlineImage {
            name: "figure.png".into(),
            format: ImageFormat::Png,
            width_px: 1,
            height_px: 1,
            width_pt: 48.0,
            height_pt: 48.0,
            bytes: Arc::new(img_bytes),
        });
        doc.push(img_block);

        let chart_id = doc.next_id();
        let mut chart_block = RichBlock::new(chart_id, CHART_BLOCK_KIND, "");
        chart_block.chart = Some(Arc::new("{\"type\":\"bar\"}".into()));
        doc.push(chart_block);

        let expected_stats = doc.statistics();
        let snap = compute_statistics(&doc);

        assert_eq!(snap.words, clamp_i32(expected_stats.word_count));
        assert_eq!(snap.chars_with_spaces, clamp_i32(expected_stats.char_count));
        assert_eq!(
            snap.chars_without_spaces,
            clamp_i32(expected_stats.char_count_no_spaces)
        );
        assert_eq!(snap.paragraphs, 2);
        assert!(snap.sentences >= 2);
        assert_eq!(snap.pages, 1);
        assert_eq!(snap.tables, 1);
        assert_eq!(snap.images, 1);
        assert_eq!(snap.charts, 1);
        assert_eq!(snap.footnotes, 1);
        assert_eq!(snap.comments, 1);
        assert!(!doc.tracked_changes.revisions().is_empty());
        assert_eq!(
            snap.tracked_changes,
            clamp_i32(doc.tracked_changes.revisions().len())
        );
        assert_eq!(snap.reading_time, "1 min");
    }

    #[test]
    fn open_and_close_callbacks_project_statistics_to_global_state() {
        let mut doc = text_document("First sentence. Second sentence?");
        for idx in 0..80 {
            let id = doc.next_id();
            doc.push(RichBlock::new(
                id,
                "paragraph",
                &format!("Paragraph {idx} adds enough copy to span multiple pages."),
            ));
        }

        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(doc, dialogs);
        wire(&app, &state);

        let modal = app.global::<WordCountState>();
        assert!(!modal.get_open());

        modal.invoke_open_dialog();
        assert!(modal.get_open());
        assert!(modal.get_words() > 500);
        assert!(modal.get_pages() >= 2);
        assert_eq!(modal.get_paragraphs(), 81);
        assert_eq!(modal.get_reading_time(), "3 min");

        modal.invoke_close_dialog();
        assert!(!modal.get_open());
    }
}
