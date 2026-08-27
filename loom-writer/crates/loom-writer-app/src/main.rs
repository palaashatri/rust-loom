//! Loom Writer desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

mod document_formatting;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use document_formatting::{
    formatting_state, set_document_alignment, set_document_bold, set_document_heading,
    set_document_italic, set_document_underline,
};
use loom_desktop::{
    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,
};
use loom_production::define_snapshot_recovery;
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use loom_writer_core::{RichBlock, WriterDocument};
use slint::{ComponentHandle, Model, PhysicalSize, SharedString, VecModel};

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

#[derive(Debug)]
struct Args {
    #[cfg_attr(not(feature = "visual-qa"), allow(dead_code))]
    screenshot: Option<String>,
    #[cfg_attr(not(feature = "visual-qa"), allow(dead_code))]
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
    template: Option<TemplateId>,
    #[cfg_attr(not(feature = "visual-qa"), allow(dead_code))]
    template_chooser: bool,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I, S>(raw_args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "light".to_string(),
        open: None,
        template: None,
        template_chooser: false,
    };
    let mut it = raw_args.into_iter().map(Into::into);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                #[cfg(feature = "visual-qa")]
                {
                    args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
                }
                #[cfg(not(feature = "visual-qa"))]
                {
                    return Err("--screenshot requires a visual-qa build".into());
                }
            }
            "--smoke" => {
                #[cfg(feature = "visual-qa")]
                {
                    args.smoke = true;
                }
                #[cfg(not(feature = "visual-qa"))]
                {
                    return Err("--smoke requires a visual-qa build".into());
                }
            }
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?);
            }
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
            "--template" => {
                let value = it.next().ok_or("--template needs an id")?;
                args.template = Some(parse_template_id(&value).ok_or_else(|| {
                    format!("unknown template: {value} (expected blank, report, letter, or cv)")
                })?);
            }
            "--template-chooser" => {
                args.template_chooser = true;
            }
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateId {
    Blank,
    Report,
    Letter,
    Cv,
}

impl TemplateId {
    fn as_str(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Report => "report",
            Self::Letter => "letter",
            Self::Cv => "cv",
        }
    }
}

fn parse_template_id(value: &str) -> Option<TemplateId> {
    match value {
        "blank" => Some(TemplateId::Blank),
        "report" => Some(TemplateId::Report),
        "letter" => Some(TemplateId::Letter),
        "cv" => Some(TemplateId::Cv),
        _ => None,
    }
}

fn template_document(template: TemplateId) -> WriterDocument {
    let (id, title) = (
        format!("template-{}", template.as_str()),
        match template {
            TemplateId::Blank => "Untitled Document",
            TemplateId::Report => "Untitled Report",
            TemplateId::Letter => "Untitled Letter",
            TemplateId::Cv => "Untitled CV",
        },
    );
    let mut document = WriterDocument::new(id, title);
    match template {
        TemplateId::Blank => {}
        TemplateId::Report => {
            document.push(RichBlock::new(
                document.next_id(),
                "heading1",
                "Report Title",
            ));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Executive summary",
            ));
            document.push(RichBlock::new(document.next_id(), "heading2", "Overview"));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Start writing your report here.",
            ));
        }
        TemplateId::Letter => {
            document.push(RichBlock::new(document.next_id(), "paragraph", "Your Name"));
            document.push(RichBlock::new(document.next_id(), "paragraph", "Date"));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Dear Recipient,",
            ));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Write your letter here.",
            ));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Sincerely,",
            ));
        }
        TemplateId::Cv => {
            document.push(RichBlock::new(document.next_id(), "heading1", "Your Name"));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Professional title · email@example.com",
            ));
            document.push(RichBlock::new(document.next_id(), "heading2", "Experience"));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Role — Company · 2020–Present",
            ));
            document.push(RichBlock::new(document.next_id(), "heading2", "Education"));
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                "Degree — Institution",
            ));
        }
    }
    document
}

fn blank_document() -> WriterDocument {
    template_document(TemplateId::Blank)
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
        "Type and format local text, save an inspectable Loom document, and export a deterministic PDF or Markdown file. All implemented workflows work offline.",
    ));
    d.push(RichBlock::new(d.next_id(), "heading2", "Getting started"));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Use New to create a document, Open to load an existing .loomdoc file, and Export PDF to produce a deterministic PDF. Undo and redo are fully wired.",
    ));
    d
}

fn load_file(path: &Path) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    loom_writer_core::load_document(&bytes)
        .map_err(|error| format!("load {}: {error}", path.display()))
}

fn save_file(path: &Path, doc: &WriterDocument) -> Result<(), String> {
    let bytes = loom_writer_core::save_document(doc).map_err(|error| error.to_string())?;
    loom_storage::atomic_write(path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))
}

/// Commands exposed through the command palette. Each palette entry maps to
/// one of the application callbacks, so palette invocation and toolbar clicks
/// share a single dispatch path.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewDoc,
    OpenDoc,
    SaveDoc,
    SaveAsDoc,
    ExportPdf,
    Undo,
    Redo,
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    SetHeading(i32),
    SetAlignment(i32),
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &WriterApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewDoc,
            id: "writer.new",
            label: "New Document",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenDoc,
            id: "writer.open",
            label: "Open Document",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveDoc,
            id: "writer.save",
            label: "Save Document",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDoc,
            id: "writer.save-as",
            label: "Save Document As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "writer.export-pdf",
            label: "Export PDF",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "writer.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "writer.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::ToggleBold,
            id: "writer.style.bold-all",
            label: "Document Style: Bold",
            shortcut: "Ctrl+B",
        },
        PaletteCommand {
            action: PaletteAction::ToggleItalic,
            id: "writer.style.italic-all",
            label: "Document Style: Italic",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::ToggleUnderline,
            id: "writer.style.underline-all",
            label: "Document Style: Underline",
            shortcut: "Ctrl+U",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(1),
            id: "writer.style.heading-1-all",
            label: "All Paragraphs: Heading 1",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(2),
            id: "writer.style.heading-2-all",
            label: "All Paragraphs: Heading 2",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(3),
            id: "writer.style.heading-3-all",
            label: "All Paragraphs: Heading 3",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(0),
            id: "writer.style.body-all",
            label: "All Paragraphs: Body Text",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(0),
            id: "writer.align.left-all",
            label: "Align All Left",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(1),
            id: "writer.align.center-all",
            label: "Align All Center",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(2),
            id: "writer.align.right-all",
            label: "Align All Right",
            shortcut: "",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &WriterApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| {
            query_lower.is_empty()
                || c.label.to_lowercase().contains(&query_lower)
                || c.id.to_lowercase().contains(&query_lower)
        })
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
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
    save_path: RefCell<Option<PathBuf>>,
    history: RefCell<EditorHistory>,
    history_clock: Instant,
    syncing_editor: Cell<bool>,
    dialogs: Rc<dyn FileDialogService>,
    document_filter: FileFilter,
    pdf_filter: FileFilter,
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn writer_open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Loom Writer Document".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.document_filter.clone()],
    }
}

fn writer_save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow();
    let suggested_name = path
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Loom Writer Document".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.document_filter.clone()],
    }
}

fn writer_export_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Writer PDF".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(EXPORT_FILENAME.to_string()),
        filters: vec![state.pdf_filter.clone()],
    }
}

fn replace_opened_document(
    app: &WriterApp,
    state: &GuiState,
    path: PathBuf,
    document: WriterDocument,
) {
    *state.current.borrow_mut() = document;
    *state.save_path.borrow_mut() = Some(path);
    *state.history.borrow_mut() = EditorHistory::new();
    apply_state(app, state);
}

fn save_current_document(
    app: &WriterApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    let path = if !force_picker {
        state.save_path.borrow().clone()
    } else {
        None
    };
    let path = match path {
        Some(path) => Some(path),
        None => state
            .dialogs
            .save_file(&writer_save_request(state))
            .map_err(|error| error.to_string())?,
    };
    let Some(path) = path else {
        app.set_status_left("Save cancelled".into());
        return Ok(false);
    };
    save_file(&path, &state.current.borrow())?;
    *state.save_path.borrow_mut() = Some(path.clone());
    if let Ok(bytes) = loom_writer_core::save_document(&state.current.borrow()) {
        let _ = checkpoint_snapshot_recovery(bytes);
    }
    app.set_status_left(SharedString::from(format!("Saved {}", path.display())));
    Ok(true)
}

fn apply_document(app: &WriterApp, doc: &WriterDocument) {
    let text = doc.editor_text();
    let word_count = text.split_whitespace().count();
    let char_count = text.chars().count();
    let block_count = doc.len();
    let formatting = formatting_state(doc);

    app.set_doc_title(doc.title.as_str().into());
    app.set_doc_content(SharedString::from(text));
    app.set_is_bold(formatting.bold);
    app.set_is_italic(formatting.italic);
    app.set_is_underline(formatting.underline);
    app.set_heading_level(formatting.heading_level);
    app.set_text_alignment(formatting.alignment);
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

fn layout_breakpoints(width: u32) -> (bool, bool) {
    (width >= 1320, width >= 1180)
}

fn apply_layout_breakpoints(app: &WriterApp, width: u32) {
    let (wide_toolbar, labeled_export) = layout_breakpoints(width);
    app.set_wide_toolbar(wide_toolbar);
    app.set_labeled_export(labeled_export);
}

fn wire_responsive_layout(app: &WriterApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            apply_layout_breakpoints(&app, width.max(0.0) as u32);
        }
    });
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = match &args.open {
        Some(p) => load_file(Path::new(p))?,
        None => args
            .template
            .map(template_document)
            .unwrap_or_else(sample_document),
    };
    apply_document(&app, &doc);
    app.set_template_chooser_open(args.template_chooser);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let (w, h) = args.size;
    apply_layout_breakpoints(&app, w);
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_gui(args: &Args) -> Result<(), String> {
    run_gui_with_dialogs(args, Rc::new(NativeFileDialogs))
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    wire_responsive_layout(&app);

    let recovered = if args.open.is_none() {
        take_snapshot_recovery().and_then(|payload| loom_writer_core::load_document(&payload).ok())
    } else {
        None
    };
    let document_filter =
        FileFilter::new("Loom Writer document", ["loomdoc"]).map_err(|error| error.to_string())?;
    let pdf_filter = FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?;
    let state = Rc::new(GuiState {
        current: RefCell::new(if let Some(document) = recovered {
            document
        } else {
            match &args.open {
                Some(p) => load_file(Path::new(p))?,
                None => args
                    .template
                    .map(template_document)
                    .unwrap_or_else(sample_document),
            }
        }),
        save_path: RefCell::new(args.open.as_ref().map(PathBuf::from)),
        history: RefCell::new(EditorHistory::new()),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
        dialogs,
        document_filter,
        pdf_filter,
    });

    {
        let app_ref = app.as_weak();
        app.on_new_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                // Opening New must not mutate the current document until the
                // user confirms a real template seed in the chooser.
                app.set_template_selected(0);
                app.set_template_category(0);
                app.set_template_chooser_open(true);
                app.set_status_left("Choose a document template".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_create_template(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let template = match index {
                    1 => TemplateId::Report,
                    2 => TemplateId::Letter,
                    3 => TemplateId::Cv,
                    _ => TemplateId::Blank,
                };
                *state.current.borrow_mut() = if template == TemplateId::Blank {
                    blank_document()
                } else {
                    template_document(template)
                };
                *state.save_path.borrow_mut() = None;
                *state.history.borrow_mut() = EditorHistory::new();
                app.set_template_chooser_open(false);
                apply_state(&app, &state);
                app.set_status_left(SharedString::from(format!(
                    "Created {} document",
                    match template {
                        TemplateId::Blank => "blank",
                        TemplateId::Report => "report",
                        TemplateId::Letter => "letter",
                        TemplateId::Cv => "CV",
                    }
                )));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_cancel_template(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_template_chooser_open(false);
                app.set_status_left("New document cancelled".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_inspector(move || {
            if let Some(app) = app_ref.upgrade() {
                let visible = app.get_show_inspector();
                app.set_show_inspector(!visible);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&writer_open_request(&state)) {
                    Ok(Some(path)) => match load_file(&path) {
                        Ok(document) => {
                            replace_opened_document(&app, &state, path.clone(), document);
                            app.set_status_left(SharedString::from(format!(
                                "Opened {}",
                                path.display()
                            )));
                        }
                        Err(error) => {
                            app.set_status_left(SharedString::from(format!("Open failed: {error}")))
                        }
                    },
                    Ok(None) => app.set_status_left("Open cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Open dialog failed: {error}"
                    ))),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_document(&app, &state, false) {
                    app.set_status_left(SharedString::from(format!("Save failed: {error}")));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_document(&app, &state, true) {
                    app.set_status_left(SharedString::from(format!("Save As failed: {error}")));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&writer_export_request(&state)) {
                    Ok(Some(path)) => {
                        let bytes = loom_writer_core::export_pdf(&state.current.borrow());
                        match loom_storage::atomic_write(&path, &bytes) {
                            Ok(()) => app.set_status_left(SharedString::from(format!(
                                "Exported {}",
                                path.display()
                            ))),
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "Export failed: {error}"
                            ))),
                        }
                    }
                    Ok(None) => app.set_status_left("Export cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Export dialog failed: {error}"
                    ))),
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
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_bold(move || {
            if let Some(app) = app_ref.upgrade() {
                let enabled = app.get_is_bold();
                let mut next = state.current.borrow().clone();
                set_document_bold(&mut next, enabled);
                apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                app.set_status_right(SharedString::from(if enabled {
                    "Bold applied to all paragraphs"
                } else {
                    "Bold removed from all paragraphs"
                }));
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_italic(move || {
            if let Some(app) = app_ref.upgrade() {
                let enabled = app.get_is_italic();
                let mut next = state.current.borrow().clone();
                set_document_italic(&mut next, enabled);
                apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                app.set_status_right(SharedString::from(if enabled {
                    "Italic applied to all paragraphs"
                } else {
                    "Italic removed from all paragraphs"
                }));
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_underline(move || {
            if let Some(app) = app_ref.upgrade() {
                let enabled = app.get_is_underline();
                let mut next = state.current.borrow().clone();
                set_document_underline(&mut next, enabled);
                apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                app.set_status_right(SharedString::from(if enabled {
                    "Underline applied to all paragraphs"
                } else {
                    "Underline removed from all paragraphs"
                }));
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_heading(move |level| {
            if let Some(app) = app_ref.upgrade() {
                let mut next = state.current.borrow().clone();
                set_document_heading(&mut next, level);
                apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                let label = match level {
                    1 => "Heading 1",
                    2 => "Heading 2",
                    3 => "Heading 3",
                    _ => "Body Text",
                };
                app.set_status_right(SharedString::from(format!(
                    "{label} applied to all paragraphs"
                )));
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_alignment(move |align| {
            if let Some(app) = app_ref.upgrade() {
                let mut next = state.current.borrow().clone();
                set_document_alignment(&mut next, align);
                apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
                let label = match align {
                    1 => "Center",
                    2 => "Right",
                    3 => "Justify",
                    _ => "Left",
                };
                app.set_status_right(SharedString::from(format!(
                    "{label} alignment applied to all paragraphs"
                )));
            }
        });
    }

    wire_palette(&app);

    apply_state(&app, &state);
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    #[cfg(feature = "visual-qa")]
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    #[cfg(feature = "visual-qa")]
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-writer-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }
    run_gui(&args)
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = match &args.open {
        Some(p) => load_file(Path::new(p))?,
        None => args
            .template
            .map(template_document)
            .unwrap_or_else(sample_document),
    };
    apply_document(&app, &doc);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    let report = record_keyboard_palette_journey(&app, "writer", Path::new(out_dir), "ex")
        .map_err(|e| format!("journey failed: {e}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    Ok(())
}

impl PaletteProbe for WriterApp {
    fn palette_open(&self) -> bool {
        self.get_palette_open()
    }

    fn palette_commands(&self) -> usize {
        self.get_palette_commands().row_count()
    }

    fn palette_selected(&self) -> i32 {
        self.get_palette_selected()
    }

    fn palette_query(&self) -> String {
        self.get_palette_query().to_string()
    }

    fn open_palette(&self) {
        self.invoke_open_palette();
    }
}

/// Connect the command-palette callbacks. Invocation dispatches through the
/// same application callbacks as the toolbar, so palette and toolbar behave
/// identically, and the query model stays in Rust for testability.
fn wire_palette(app: &WriterApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| match c.action {
                        PaletteAction::Undo => app.get_can_undo(),
                        PaletteAction::Redo => app.get_can_redo(),
                        _ => true,
                    })
                    .filter(|c| {
                        let q = app.get_palette_query().trim().to_lowercase();
                        q.is_empty()
                            || c.label.to_lowercase().contains(&q)
                            || c.id.to_lowercase().contains(&q)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewDoc => app.invoke_new_doc(),
                        PaletteAction::OpenDoc => app.invoke_open_doc(),
                        PaletteAction::SaveDoc => app.invoke_save_doc(),
                        PaletteAction::SaveAsDoc => app.invoke_save_as_doc(),
                        PaletteAction::ExportPdf => app.invoke_export_pdf(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::ToggleBold => app.invoke_toggle_bold(),
                        PaletteAction::ToggleItalic => app.invoke_toggle_italic(),
                        PaletteAction::ToggleUnderline => app.invoke_toggle_underline(),
                        PaletteAction::SetHeading(level) => app.invoke_select_heading(level),
                        PaletteAction::SetAlignment(index) => app.invoke_select_alignment(index),
                    }
                }
            }
        });
    }
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

        history.record(first.clone(), second.clone(), HistoryKind::Typing, 100);
        history.record(second.clone(), third.clone(), HistoryKind::Typing, 200);

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

        history.record(first.clone(), second.clone(), HistoryKind::Typing, 100);
        history.record(
            second.clone(),
            third.clone(),
            HistoryKind::DocumentAction,
            200,
        );

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(second.clone()));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(second));
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
    fn new_document_is_blank_and_unsaved_ready() {
        let document = blank_document();
        assert!(document.blocks.is_empty());
        assert_eq!(document.title, "Untitled Document");
    }

    #[test]
    fn scripted_dialog_request_uses_the_current_document_directory() {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
            [Some(PathBuf::from("/tmp/next.loomdoc"))],
            [Some(PathBuf::from("/tmp/saved.loomdoc"))],
        ));
        let state = GuiState {
            current: RefCell::new(text_document("hello")),
            save_path: RefCell::new(Some(PathBuf::from("/tmp/current.loomdoc"))),
            history: RefCell::new(EditorHistory::new()),
            history_clock: Instant::now(),
            syncing_editor: Cell::new(false),
            dialogs,
            document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        };
        let request = writer_open_request(&state);
        assert_eq!(request.initial_directory, Some(PathBuf::from("/tmp")));
        assert_eq!(
            state.dialogs.open_file(&request).expect("open"),
            Some(PathBuf::from("/tmp/next.loomdoc"))
        );
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

    #[test]
    fn layout_breakpoints_match_supported_width_boundaries() {
        assert_eq!(layout_breakpoints(1024), (false, false));
        assert_eq!(layout_breakpoints(1179), (false, false));
        assert_eq!(layout_breakpoints(1180), (false, true));
        assert_eq!(layout_breakpoints(1199), (false, true));
        assert_eq!(layout_breakpoints(1319), (false, true));
        assert_eq!(layout_breakpoints(1320), (true, true));
        assert_eq!(layout_breakpoints(1440), (true, true));
    }

    #[test]
    fn window_resize_updates_breakpoint_flags() {
        set_platform();
        let app = WriterApp::new().expect("create WriterApp");
        apply_layout_breakpoints(&app, 1024);
        wire_responsive_layout(&app);

        app.window().set_size(PhysicalSize::new(1320, 800));
        let _ = snapshot_component(&app, 1320.0, 800.0, 1.0).expect("render resized window");

        assert!(app.get_wide_toolbar());
        assert!(app.get_labeled_export());
    }

    #[test]
    fn template_chooser_renders_at_reference_width() {
        set_platform();
        let app = WriterApp::new().expect("create WriterApp");
        apply_theme(&app, "dark");
        apply_layout_breakpoints(&app, 1440);
        app.set_template_selected(2);
        app.set_template_chooser_open(true);
        let image = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render chooser");
        assert_eq!(image.width(), 1440);
        assert_eq!(image.height(), 900);
    }

    #[test]
    #[cfg(feature = "visual-qa")]
    fn template_category_filters_visible_cards() {
        set_platform();
        let app = WriterApp::new().expect("create WriterApp");
        apply_theme(&app, "light");
        apply_layout_breakpoints(&app, 1440);
        app.set_template_chooser_open(true);
        app.set_template_category(0);
        let all = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render all templates");

        app.set_template_category(2);
        let letters =
            snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render letter templates");
        assert_eq!(app.get_template_category(), 2);
        assert_ne!(
            all.get_pixel(500, 220),
            letters.get_pixel(500, 220),
            "selecting Letters must remove the Report card from the chooser"
        );
    }

    #[test]
    fn template_ids_are_stable_and_seed_real_documents() {
        let cases = [
            (TemplateId::Blank, "blank", "Untitled Document"),
            (TemplateId::Report, "report", "Untitled Report"),
            (TemplateId::Letter, "letter", "Untitled Letter"),
            (TemplateId::Cv, "cv", "Untitled CV"),
        ];

        for (template, id, title) in cases {
            assert_eq!(template.as_str(), id);
            let document = template_document(template);
            assert_eq!(document.id, format!("template-{id}"));
            assert_eq!(document.title, title);
            if template == TemplateId::Blank {
                assert!(document.blocks.is_empty());
            } else {
                assert!(!document.blocks.is_empty());
                assert!(document
                    .blocks
                    .iter()
                    .any(|block| !block.text.as_str().trim().is_empty()));
            }
        }
    }

    #[test]
    fn template_id_parser_accepts_stable_names_only() {
        assert_eq!(parse_template_id("blank"), Some(TemplateId::Blank));
        assert_eq!(parse_template_id("report"), Some(TemplateId::Report));
        assert_eq!(parse_template_id("letter"), Some(TemplateId::Letter));
        assert_eq!(parse_template_id("cv"), Some(TemplateId::Cv));
        assert_eq!(parse_template_id("CV"), None);
        assert_eq!(parse_template_id("memo"), None);
    }

    #[test]
    #[cfg(feature = "visual-qa")]
    fn parse_args_accepts_template_without_changing_existing_flags() {
        let args = parse_args_from([
            "--template",
            "report",
            "--screenshot",
            "/tmp/writer-template.png",
            "--theme",
            "dark",
            "--size",
            "1440x900",
        ])
        .expect("parse template screenshot arguments");

        assert_eq!(args.template, Some(TemplateId::Report));
        assert_eq!(args.screenshot.as_deref(), Some("/tmp/writer-template.png"));
        assert_eq!(args.theme, "dark");
        assert_eq!(args.size, (1440, 900));
    }

    #[test]
    #[cfg(feature = "visual-qa")]
    fn template_chooser_flag_parses_and_changes_headless_render() {
        set_platform();
        let chooser_args = parse_args_from(["--template-chooser", "--size", "1440x900"])
            .expect("parse chooser screenshot arguments");
        let regular_args =
            parse_args_from(["--size", "1440x900"]).expect("parse regular screenshot arguments");

        let dir = std::env::temp_dir().join(format!(
            "loom-writer-template-chooser-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create screenshot directory");
        let chooser_path = dir.join("chooser.png");
        let regular_path = dir.join("regular.png");
        render_headless(&chooser_args, chooser_path.to_str().expect("chooser path"))
            .expect("render chooser screenshot");
        render_headless(&regular_args, regular_path.to_str().expect("regular path"))
            .expect("render regular screenshot");

        let chooser = loom_test_support::png::load_png(&chooser_path).expect("load chooser");
        let regular = loom_test_support::png::load_png(&regular_path).expect("load regular");
        assert_ne!(
            chooser.get_pixel(100, 200),
            regular.get_pixel(100, 200),
            "--template-chooser must render the chooser overlay, not the editor"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_args_rejects_unknown_template_id() {
        let error = parse_args_from(["--template", "memo"]).expect_err("unknown template");
        assert!(error.contains("unknown template"));
    }
}
