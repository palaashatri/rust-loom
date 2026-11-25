//! Wiring for `WriterHyperlinkState` dialog and document hyperlink mutations.

use std::ops::Range;
use std::rc::Rc;

use loom_objects::HyperlinkTarget;
use loom_writer_core::WriterDocument;
use slint::ComponentHandle;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp, WriterFormatState};

fn active_block_and_range(doc: &WriterDocument) -> Option<(u64, Range<usize>)> {
    let (sel_start, sel_end) = (
        doc.selection.anchor.min(doc.selection.focus),
        doc.selection.anchor.max(doc.selection.focus),
    );
    let mut offset = 0usize;
    for block in &doc.blocks {
        let len = block.text.len_bytes();
        let block_end = offset + len;
        if sel_start <= block_end && (sel_end >= offset || block.id == doc.blocks.last()?.id) {
            let local_start = sel_start.saturating_sub(offset).min(len);
            let local_end = sel_end.saturating_sub(offset).min(len);
            let range = if local_start < local_end {
                local_start..local_end
            } else if let Some(existing) = doc.hyperlinks.at(block.id, local_start) {
                existing.start..existing.end
            } else {
                local_start..local_start
            };
            return Some((block.id, range));
        }
        offset = block_end + 1;
    }
    None
}

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let g = app.global::<WriterFormatState>();

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_hyperlink_open_requested(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let doc = state.current.borrow();
            let g = app.global::<WriterFormatState>();
            g.set_hyperlink_error("".into());
            if let Some((block_id, range)) = active_block_and_range(&doc) {
                if let Some(existing) = doc.hyperlink_at(block_id, range.start) {
                    let url = match &existing.link.target {
                        HyperlinkTarget::Url(u) => u.as_str(),
                        HyperlinkTarget::Bookmark(b) => b.as_str(),
                        HyperlinkTarget::Email(e) => e.as_str(),
                        _ => "",
                    };
                    g.set_hyperlink_url(url.into());
                    g.set_hyperlink_tooltip(existing.link.tooltip.as_deref().unwrap_or("").into());
                    g.set_hyperlink_has_existing(true);
                } else {
                    g.set_hyperlink_url("".into());
                    g.set_hyperlink_tooltip("".into());
                    g.set_hyperlink_has_existing(false);
                }
            }
            g.set_hyperlink_open(true);
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_hyperlink_apply(move |url, tooltip| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let g = app.global::<WriterFormatState>();
            let mut next = state.current.borrow().clone();
            let Some((block_id, range)) = active_block_and_range(&next) else {
                g.set_hyperlink_open(false);
                return;
            };
            let tip = if tooltip.trim().is_empty() {
                None
            } else {
                Some(tooltip.trim().to_string())
            };
            match next.set_hyperlink(block_id, range, url.as_str(), tip) {
                Ok(()) => {
                    let _ = apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                    g.set_hyperlink_error("".into());
                    g.set_hyperlink_open(false);
                    app.set_status_left("Hyperlink applied".into());
                }
                Err(err) => {
                    g.set_hyperlink_error(err.to_string().into());
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_hyperlink_remove(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let g = app.global::<WriterFormatState>();
            let mut next = state.current.borrow().clone();
            if let Some((block_id, range)) = active_block_and_range(&next) {
                if next.remove_hyperlink(block_id, range) {
                    let _ = apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                    app.set_status_left("Hyperlink removed".into());
                }
            }
            g.set_hyperlink_open(false);
        });
    }

    {
        let app_ref = app.as_weak();
        g.on_hyperlink_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<WriterFormatState>();
                g.set_hyperlink_error("".into());
                g.set_hyperlink_open(false);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_writer_core::RichBlock;

    #[test]
    fn active_block_and_range_uses_selection_or_existing_link_at_caret() {
        let mut doc = WriterDocument::new("h1", "Hyperlinks");
        doc.blocks = vec![RichBlock::new(1, "paragraph", "Click here now")];
        doc.selection.anchor = 6;
        doc.selection.focus = 10;
        assert_eq!(active_block_and_range(&doc), Some((1, 6..10)));

        doc.set_hyperlink(1, 6..10, "https://example.com", None)
            .unwrap();
        doc.selection.anchor = 8;
        doc.selection.focus = 8;
        assert_eq!(active_block_and_range(&doc), Some((1, 6..10)));

        doc.selection.anchor = 1;
        doc.selection.focus = 1;
        assert_eq!(active_block_and_range(&doc), Some((1, 1..1)));
    }
}
