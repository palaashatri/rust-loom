//! The outline navigator: the document's headings, listed in a narrow pane
//! beside the page. Activating a heading moves the caret to it and scrolls it
//! to the top of the view. The list is only built while the pane is showing.

use std::cell::RefCell;
use std::rc::Rc;

use loom_writer_core::{extract_outline, OutlineEntry, TextSelection, WriterDocument};
use slint::{ComponentHandle, VecModel};

use crate::caret_scroll::PAGE_TOP_INSET;
use crate::{
    apply_document_with_viewport, normalize_page_scroll, projection, refresh_writer_registry,
    GuiState, WriterApp, WriterOutlineEntry,
};

/// Distance kept between the top of the view and a heading we jumped to.
const TOP_MARGIN: f32 = 28.0;

type Row = (String, i32, bool);

std::thread_local! {
    /// What the pane currently shows, so unchanged text does not rebuild it.
    static PUBLISHED: RefCell<Vec<Row>> = const { RefCell::new(Vec::new()) };
}

/// Bytes before the first character of block `index`; the same offset space
/// as the document selection.
fn block_start(doc: &WriterDocument, index: usize) -> usize {
    doc.blocks[..index]
        .iter()
        .map(|block| block.text.as_str().len() + 1)
        .sum()
}

/// Index of the block holding byte offset `offset`.
fn block_at(doc: &WriterDocument, offset: usize) -> usize {
    let mut start = 0;
    for (index, block) in doc.blocks.iter().enumerate() {
        let end = start + block.text.as_str().len();
        if offset <= end {
            return index;
        }
        start = end + 1;
    }
    doc.blocks.len().saturating_sub(1)
}

/// The last heading at or before the caret, as an index into `entries`.
fn current_heading(entries: &[OutlineEntry], doc: &WriterDocument) -> Option<usize> {
    let caret_block = block_at(doc, doc.selection().focus);
    entries
        .iter()
        .rposition(|entry| entry.block_index <= caret_block)
}

/// Shows the document's headings in the pane. Does nothing while the pane is
/// hidden.
pub(crate) fn publish(app: &WriterApp, doc: &WriterDocument) {
    if !app.get_show_navigator() {
        return;
    }
    let entries = extract_outline(&doc.blocks);
    let current = current_heading(&entries, doc);
    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            (
                entry.title.clone(),
                i32::from(entry.level),
                current == Some(index),
            )
        })
        .collect();
    if PUBLISHED.with(|published| *published.borrow() == rows) {
        return;
    }
    let model: Vec<WriterOutlineEntry> = rows
        .iter()
        .map(|(title, level, current)| WriterOutlineEntry {
            title: title.as_str().into(),
            level: *level,
            current: *current,
        })
        .collect();
    app.set_outline_entries(Rc::new(VecModel::from(model)).into());
    PUBLISHED.with(|published| *published.borrow_mut() = rows);
}

/// Moves the caret to the `index`th heading and scrolls it to the top of the
/// page view.
pub(crate) fn jump(app: &WriterApp, state: &GuiState, index: usize) {
    let (start, snapshot) = {
        let mut current = state.current.borrow_mut();
        let entries = extract_outline(&current.blocks);
        let Some(entry) = entries.get(index) else {
            return;
        };
        let start = block_start(&current, entry.block_index);
        current.set_selection(TextSelection::caret(start));
        (start, current.clone())
    };
    let mut viewport = *state.viewport.borrow();
    let style = snapshot.page.page_style();
    if let Ok(flow) = snapshot.flow(&style, projection::layout_viewport(&style, viewport)) {
        if let Some(rect) = flow.caret_rect(&snapshot, &TextSelection::caret(start)) {
            viewport.scroll_y = normalize_page_scroll(rect.rect.y + PAGE_TOP_INSET - TOP_MARGIN);
        }
    }
    *state.viewport.borrow_mut() = viewport;
    app.set_page_scroll_y(viewport.scroll_y);
    apply_document_with_viewport(app, &snapshot, viewport);
    refresh_writer_registry(app, state);
    app.invoke_focus_page();
}

/// Connects the pane's rows to [`jump`].
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_jump_outline(move |index| {
        if let (Some(app), Ok(index)) = (app_ref.upgrade(), usize::try_from(index)) {
            jump(&app, &state, index);
        }
    });
}
