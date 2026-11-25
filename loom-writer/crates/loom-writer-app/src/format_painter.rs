//! Format Painter state machine backed by `loom_clipboard::StyleClipboard`.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(test)]
use loom_clipboard::FormatPainterMode;
use loom_clipboard::StyleClipboard;
use loom_text::{Alignment, CharacterStyle, ParagraphDirection};
use loom_writer_core::WriterDocument;
use slint::ComponentHandle;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp, WriterFormatState};

/// Captured character + paragraph formatting snapshot for Format Painter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapturedWriterStyle {
    pub character: CharacterStyle,
    pub alignment: Option<Alignment>,
    pub direction: ParagraphDirection,
}

/// Format Painter controller for Loom Writer.
#[derive(Debug, Clone, Default)]
pub(crate) struct WriterFormatPainter {
    clipboard: StyleClipboard<CapturedWriterStyle>,
}

impl WriterFormatPainter {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn copy_style(&mut self, style: CapturedWriterStyle) {
        self.clipboard.pick(style);
    }

    pub(crate) fn copy_style_locked(&mut self, style: CapturedWriterStyle) {
        self.clipboard.pick_locked(style);
    }

    pub(crate) fn apply_style(&mut self) -> Option<CapturedWriterStyle> {
        self.clipboard.apply_take()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.clipboard.is_active()
    }

    pub(crate) fn is_locked(&self) -> bool {
        self.clipboard.is_locked()
    }

    pub(crate) fn cancel(&mut self) {
        self.clipboard.cancel();
    }

    #[cfg(test)]
    pub(crate) fn mode(&self) -> FormatPainterMode {
        self.clipboard.mode()
    }
}

thread_local! {
    static FORMAT_PAINTER: RefCell<WriterFormatPainter> = RefCell::new(WriterFormatPainter::default());
}

fn capture_from_document(doc: &WriterDocument) -> CapturedWriterStyle {
    let (sel_start, sel_end) = (
        doc.selection.anchor.min(doc.selection.focus),
        doc.selection.anchor.max(doc.selection.focus),
    );
    let mut offset = 0usize;
    for block in &doc.blocks {
        let len = block.text.len_bytes();
        let end = offset + len;
        if sel_start <= end
            && (sel_end >= offset || block.id == doc.blocks.last().map_or(0, |b| b.id))
        {
            let local = sel_start.saturating_sub(offset).min(len);
            let ch = block
                .runs
                .iter()
                .find(|r| local >= r.start && local < r.end)
                .or_else(|| block.runs.first())
                .map(|r| r.style.clone())
                .unwrap_or_default();
            return CapturedWriterStyle {
                character: ch,
                alignment: Some(block.style.alignment),
                direction: block.style.direction,
            };
        }
        offset = end + 1;
    }
    CapturedWriterStyle {
        character: CharacterStyle::default(),
        alignment: None,
        direction: ParagraphDirection::default(),
    }
}

fn apply_to_document(doc: &mut WriterDocument, captured: &CapturedWriterStyle) -> bool {
    let (sel_start, sel_end) = (
        doc.selection.anchor.min(doc.selection.focus),
        doc.selection.anchor.max(doc.selection.focus),
    );
    let mut offset = 0usize;
    let mut changed = false;
    for block in &mut doc.blocks {
        let len = block.text.len_bytes();
        let end = offset + len;
        if sel_start <= end && sel_end >= offset {
            if let Some(align) = captured.alignment {
                block.style.alignment = align;
            }
            block.style.direction = captured.direction;
            let local_start = sel_start.saturating_sub(offset).min(len);
            let local_end = sel_end.saturating_sub(offset).min(len);
            let (start, end) = if local_start < local_end {
                (local_start, local_end)
            } else {
                (0, len)
            };
            block.runs.retain(|r| r.end <= start || r.start >= end);
            block.runs.push(loom_text::StyleRun {
                start,
                end,
                style: captured.character.clone(),
            });
            block.runs.sort_by_key(|r| r.start);
            changed = true;
        }
        offset = end + 1;
    }
    changed
}

pub(crate) fn register_commands(registry: &mut loom_command::CommandRegistry) {
    for (id, label) in [
        ("format.copy_style", "Copy Format"),
        ("format.apply_style", "Apply Format"),
        ("insert.hyperlink", "Insert Hyperlink…"),
    ] {
        registry.register_fn(
            loom_command::CommandSpec::new(id, label).with_category("Format"),
            move |_inv| Ok(loom_command::CommandOutcome::success(id)),
        );
    }
}

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let fmt = app.global::<WriterFormatState>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        fmt.on_format_painter_copy(move || {
            if let Some(app) = app_ref.upgrade() {
                let captured = capture_from_document(&state.current.borrow());
                FORMAT_PAINTER.with(|fp| fp.borrow_mut().copy_style(captured));
                let g = app.global::<WriterFormatState>();
                g.set_format_painter_active(true);
                g.set_format_painter_locked(false);
                app.set_status_left("Format Painter ready".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        fmt.on_format_painter_copy_locked(move || {
            if let Some(app) = app_ref.upgrade() {
                let captured = capture_from_document(&state.current.borrow());
                FORMAT_PAINTER.with(|fp| fp.borrow_mut().copy_style_locked(captured));
                let g = app.global::<WriterFormatState>();
                g.set_format_painter_active(true);
                g.set_format_painter_locked(true);
                app.set_status_left("Format Painter locked".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        fmt.on_format_painter_apply(move || {
            if let Some(app) = app_ref.upgrade() {
                let captured = FORMAT_PAINTER.with(|fp| {
                    let mut p = fp.borrow_mut();
                    let out = p.apply_style();
                    if !p.is_locked() {
                        p.cancel();
                    }
                    out
                });
                if let Some(style) = captured {
                    let mut next = state.current.borrow().clone();
                    if apply_to_document(&mut next, &style) {
                        let _ = apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                        app.set_status_left("Applied copied format".into());
                    }
                }
                let (active, locked) = FORMAT_PAINTER.with(|fp| {
                    let p = fp.borrow();
                    (p.is_active(), p.is_locked())
                });
                let g = app.global::<WriterFormatState>();
                g.set_format_painter_active(active);
                g.set_format_painter_locked(locked);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_shot_and_locked_transitions() {
        let mut fp = WriterFormatPainter::new();
        assert_eq!(fp.mode(), FormatPainterMode::Inactive);
        let s = CapturedWriterStyle {
            character: CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
            alignment: Some(Alignment::Center),
            direction: ParagraphDirection::LeftToRight,
        };
        fp.copy_style(s.clone());
        assert!(fp.is_active());
        assert!(!fp.is_locked());
        assert_eq!(fp.apply_style(), Some(s.clone()));
        assert!(!fp.is_active());

        fp.copy_style_locked(s.clone());
        assert!(fp.is_locked());
        assert_eq!(fp.apply_style(), Some(s.clone()));
        assert!(fp.is_locked());
        fp.cancel();
        assert!(!fp.is_active());
    }
}
