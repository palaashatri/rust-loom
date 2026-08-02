//! Loom Writer desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use loom_production::define_snapshot_recovery;
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_writer_core::{RichBlock, WriterDocument};
use slint::{ComponentHandle, PhysicalSize, SharedString};

slint::include_modules!();
define_snapshot_recovery!(
    application_id: "org.loom.writer",
    schema: "loom.writer.package/1"
);

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "loom-writer-document.loomdoc";
const EXPORT_FILENAME: &str = "loom-writer-export.pdf";
const HISTORY_MAX_ENTRIES: usize = 128;
const HISTORY_MAX_BYTES: usize = 8 * 1024 * 1024;
const TYPING_COALESCE_WINDOW_MS: u64 = 750;

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        size: DEFAULT_SIZE,
        theme: "light".to_string(),
        open: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
            }
            "--smoke" => args.smoke = true,
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    w.parse().map_err(|_| "bad --size width")?,
                    h.parse().map_err(|_| "bad --size height")?,
                );
            }
            "--theme" => {
                let t = it.next().ok_or("--theme needs a name")?;
                if !matches!(t.as_str(), "light" | "dark" | "high-contrast") {
                    return Err(format!("unknown theme: {t}"));
                }
                args.theme = t;
            }
            "--open" => {
                args.open = Some(it.next().ok_or("--open needs a path")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

/// A sample document used by `--smoke`, screenshots, and first launch.
fn sample_document() -> WriterDocument {
    let mut d = WriterDocument::new("quick-start", "Loom Writer — Quick Start");
    d.push(RichBlock::new(
        d.next_id(),
        "heading1",
        "Welcome to Loom Writer",
    ));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Loom Writer is a calm, professional word processor. Everything is stored in an open, inspectable .loomdoc package on your computer — no account, no cloud, no telemetry.",
    ));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Type styled text, insert images, track changes, and export to PDF or Markdown. All of it works offline.",
    ));
    d.push(RichBlock::new(d.next_id(), "heading2", "Getting started"));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Use New to create a document, Open to load an existing .loomdoc file, and Export PDF to produce a deterministic PDF. Undo and redo are fully wired.",
    ));
    d
}

fn load_file(path: &str) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    loom_writer_core::load_document(&bytes).map_err(|e| format!("load {path}: {e}"))
}

fn save_file(path: &str, doc: &WriterDocument) -> Result<(), String> {
    let bytes = loom_writer_core::save_document(doc).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("write {path}: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryKind {
    Typing,
    DocumentAction,
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    before: WriterDocument,
    after: WriterDocument,
    kind: HistoryKind,
    last_edit_ms: u64,
}

impl HistoryEntry {
    fn memory_bytes(&self) -> usize {
        self.before.to_content_json().len() + self.after.to_content_json().len()
    }
}

#[derive(Debug)]
struct EditorHistory {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    max_entries: usize,
    max_bytes: usize,
    total_bytes: usize,
}

impl EditorHistory {
    fn new() -> Self {
        Self::with_budget(HISTORY_MAX_ENTRIES, HISTORY_MAX_BYTES)
    }

    fn with_budget(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries,
            max_bytes,
            total_bytes: 0,
        }
    }

    fn record(
        &mut self,
        before: WriterDocument,
        after: WriterDocument,
        kind: HistoryKind,
        now_ms: u64,
    ) {
        if before == after {
            return;
        }
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            if kind == HistoryKind::Typing
                && last.kind == HistoryKind::Typing
                && last.after == before
                && now_ms.saturating_sub(last.last_edit_ms) <= TYPING_COALESCE_WINDOW_MS
            {
                last.after = after;
                last.last_edit_ms = now_ms;
                self.recalculate_and_trim();
                return;
            }
        }
        self.undo.push(HistoryEntry {
            before,
            after,
            kind,
            last_edit_ms: now_ms,
        });
        self.recalculate_and_trim();
    }

    fn undo(&mut self) -> Option<WriterDocument> {
        let entry = self.undo.pop()?;
        let before = entry.before.clone();
        self.redo.push(entry);
        Some(before)
    }

    fn redo(&mut self) -> Option<WriterDocument> {
        let entry = self.redo.pop()?;
        let after = entry.after.clone();
        self.undo.push(entry);
        Some(after)
    }

    #[cfg(test)]
    fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    fn redo_len(&self) -> usize {
        self.redo.len()
    }

    fn entry_count(&self) -> usize {
        self.undo.len() + self.redo.len()
    }

    #[cfg(test)]
    fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn recalculate_and_trim(&mut self) {
        self.total_bytes = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(HistoryEntry::memory_bytes)
            .sum();
        while self.entry_count() > self.max_entries || self.total_bytes > self.max_bytes {
            if self.undo.len() > 1 {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else if self.undo.len() == 1 {
                self.undo.clear();
            } else {
                break;
            }
            self.total_bytes = self
                .undo
                .iter()
                .chain(&self.redo)
                .map(HistoryEntry::memory_bytes)
                .sum();
        }
    }
}

/// Mutable state shared between the UI callbacks.
struct GuiState {
    current: RefCell<WriterDocument>,
    save_path: RefCell<Option<String>>,
    history: RefCell<EditorHistory>,
    history_clock: Instant,
    syncing_editor: Cell<bool>,
}

fn apply_document(app: &WriterApp, doc: &WriterDocument) {
    let text = doc.editor_text();
    let word_count = text.split_whitespace().count();
    let char_count = text.chars().count();
    let block_count = doc.len();

    app.set_doc_title(doc.title.as_str().into());
    app.set_doc_content(SharedString::from(text));
    app.set_status_left(SharedString::from(format!(
        "{} words · {} chars · {} blocks",
        word_count, char_count, block_count
    )));
    app.set_status_right("Offline".into());
}

fn apply_state(app: &WriterApp, state: &GuiState) {
    // TextEdit owns a native text buffer. Rebinding it after a model/history
    // operation must not be observed as another user edit transaction.
    state.syncing_editor.set(true);
    let current = state.current.borrow();
    apply_document(app, &current);
    if let Ok(bytes) = loom_writer_core::save_document(&current) {
        let _ = record_snapshot_recovery("writer state", bytes);
    }
    drop(current);
    let history = state.history.borrow();
    app.set_can_undo(!history.undo.is_empty());
    app.set_can_redo(!history.redo.is_empty());
    drop(history);
    state.syncing_editor.set(false);
}

fn history_now_ms(state: &GuiState) -> u64 {
    state
        .history_clock
        .elapsed()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn apply_with_history(app: &WriterApp, state: &GuiState, next: WriterDocument, kind: HistoryKind) {
    let current = state.current.borrow().clone();
    if current == next {
        return;
    }
    state
        .history
        .borrow_mut()
        .record(current, next.clone(), kind, history_now_ms(state));
    *state.current.borrow_mut() = next;
    apply_state(app, state);
}

fn apply_theme(app: &WriterApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
    // The editor itself is intentionally rendered on a paper-colored surface.
    // Keep its native palette light so text remains ink-dark in all surrounding
    // application themes; chrome and controls continue to use `Theme`.
    WidgetPalette::get(app)
        .set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Light);
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = match &args.open {
        Some(p) => load_file(p)?,
        None => sample_document(),
    };
    apply_document(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_gui(args: &Args) -> Result<(), String> {
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let recovered = if args.open.is_none() {
        take_snapshot_recovery()
            .and_then(|payload| loom_writer_core::load_document(&payload).ok())
    } else {
        None
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(if let Some(document) = recovered {
            document
        } else {
            match &args.open {
                Some(p) => load_file(p)?,
                None => sample_document(),
            }
        }),
        save_path: RefCell::new(args.open.clone()),
        history: RefCell::new(EditorHistory::new()),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                apply_with_history(&app, &state, sample_document(), HistoryKind::DocumentAction);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match load_file(&p) {
                    Ok(doc) => apply_with_history(&app, &state, doc, HistoryKind::DocumentAction),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("open failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match save_file(&p, &state.current.borrow()) {
                    Ok(()) => {
                        if let Ok(bytes) = loom_writer_core::save_document(&state.current.borrow()) {
                            let _ = checkpoint_snapshot_recovery(bytes);
                        }
                        app.set_status_left(SharedString::from(format!("saved {p}")));
                    }
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                let bytes = loom_writer_core::export_pdf(&state.current.borrow());
                match std::fs::write(EXPORT_FILENAME, bytes) {
                    Ok(()) => app
                        .set_status_left(SharedString::from(format!("exported {EXPORT_FILENAME}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("export failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(prev) = state.history.borrow_mut().undo() {
                    *state.current.borrow_mut() = prev;
                    apply_state(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(next) = state.history.borrow_mut().redo() {
                    *state.current.borrow_mut() = next;
                    apply_state(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_document_edited(move |text| {
            if let Some(app) = app_ref.upgrade() {
                if state.syncing_editor.get() {
                    return;
                }
                let mut next = state.current.borrow().clone();
                next.replace_paragraphs(text.as_str());
                let current = state.current.borrow().clone();
                if next != current {
                    apply_with_history(&app, &state, next, HistoryKind::Typing);
                } else if next.editor_text() != text.as_str() {
                    // Extra blank lines and non-canonical line endings are a
                    // view normalization, not a document edit. Rebind the
                    // visible editor without adding a history transaction.
                    apply_state(&app, &state);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_bold(move || {
            if let Some(app) = app_ref.upgrade() {
                let bold = app.get_is_bold();
                app.set_status_right(SharedString::from(if bold {
                    "Bold ON"
                } else {
                    "Bold OFF"
                }));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_italic(move || {
            if let Some(app) = app_ref.upgrade() {
                let italic = app.get_is_italic();
                app.set_status_right(SharedString::from(if italic {
                    "Italic ON"
                } else {
                    "Italic OFF"
                }));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_underline(move || {
            if let Some(app) = app_ref.upgrade() {
                let underline = app.get_is_underline();
                app.set_status_right(SharedString::from(if underline {
                    "Underline ON"
                } else {
                    "Underline OFF"
                }));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_select_heading(move |level| {
            if let Some(app) = app_ref.upgrade() {
                let label = match level {
                    1 => "Heading 1",
                    2 => "Heading 2",
                    3 => "Heading 3",
                    _ => "Normal",
                };
                app.set_status_right(SharedString::from(format!("Style: {label}")));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_select_alignment(move |align| {
            if let Some(app) = app_ref.upgrade() {
                let label = match align {
                    1 => "Center",
                    2 => "Right",
                    _ => "Left",
                };
                app.set_status_right(SharedString::from(format!("Align: {label}")));
            }
        });
    }

    apply_state(&app, &state);
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-writer-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }
    run_gui(&args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_document(text: &str) -> WriterDocument {
        let mut document = WriterDocument::new("test", "Test");
        document.replace_paragraphs(text);
        document
    }

    #[test]
    fn coalesces_adjacent_typing_edits() {
        let mut history = EditorHistory::with_budget(16, usize::MAX);
        let first = text_document("a");
        let second = text_document("ab");
        let third = text_document("abc");

        history.record(
            first.clone(),
            second.clone(),
            HistoryKind::Typing,
            100,
        );
        history.record(
            second.clone(),
            third.clone(),
            HistoryKind::Typing,
            200,
        );

        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(third));
    }

    #[test]
    fn document_action_breaks_typing_coalescing() {
        let mut history = EditorHistory::with_budget(16, usize::MAX);
        let first = text_document("a");
        let second = text_document("ab");
        let third = text_document("new document");

        history.record(
            first.clone(),
            second.clone(),
            HistoryKind::Typing,
            100,
        );
        history.record(
            second,
            third.clone(),
            HistoryKind::DocumentAction,
            200,
        );

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(first.clone()));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(third));
    }

    #[test]
    fn history_budget_evicts_oldest_entries() {
        let mut history = EditorHistory::with_budget(2, usize::MAX);
        let a = text_document("a");
        let b = text_document("b");
        let c = text_document("c");
        let d = text_document("d");

        history.record(a, b.clone(), HistoryKind::DocumentAction, 0);
        history.record(b, c.clone(), HistoryKind::DocumentAction, 1);
        history.record(c.clone(), d, HistoryKind::DocumentAction, 2);

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(c));
    }

    #[test]
    fn history_byte_budget_is_bounded() {
        let mut history = EditorHistory::with_budget(32, 1000);
        let a = text_document(&"a".repeat(400));
        let b = text_document(&"b".repeat(400));
        let c = text_document(&"c".repeat(400));

        history.record(a, b.clone(), HistoryKind::DocumentAction, 0);
        history.record(b, c, HistoryKind::DocumentAction, 1);

        assert!(history.total_bytes() <= 1000);
    }

    #[test]
    fn redo_is_cleared_by_new_edit() {
        let mut history = EditorHistory::with_budget(16, usize::MAX);
        let a = text_document("a");
        let b = text_document("b");
        let c = text_document("c");

        history.record(a.clone(), b.clone(), HistoryKind::DocumentAction, 0);
        assert_eq!(history.undo(), Some(a.clone()));
        assert_eq!(history.redo_len(), 1);

        history.record(a, c, HistoryKind::DocumentAction, 1);
        assert_eq!(history.redo_len(), 0);
    }
}
