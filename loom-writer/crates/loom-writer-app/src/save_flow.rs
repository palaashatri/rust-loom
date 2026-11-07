//! Writer's ordinary DOCX save path and explicit `.loomdoc` compatibility path.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{FileFilter, NativeMenuBar, OpenFileRequest, SaveFileRequest};
use slint::{ComponentHandle, SharedString};

use crate::{
    document_content_equal, document_is_dirty, file_title, recovery, set_recovery_notice,
    DocxSaveWarningState, GuiState, WriterApp, WriterDocument,
};

const SAVE_FILENAME: &str = "loom-writer-document.docx";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveFormat {
    Docx,
    Loomdoc,
}

struct PreparedDocx {
    bytes: Vec<u8>,
    losses: Vec<String>,
}

struct PendingDocxSave {
    path: PathBuf,
    document: WriterDocument,
    prepared: PreparedDocx,
}

thread_local! {
    static PENDING_DOCX_SAVE: RefCell<Option<PendingDocxSave>> = const { RefCell::new(None) };
}

pub(super) fn load_file(path: &Path) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if has_extension(path, "docx") {
        loom_writer_core::import_docx(&bytes)
            .map_err(|error| format!("import docx {}: {error}", path.display()))
    } else {
        loom_writer_core::load_document(&bytes)
            .map_err(|error| format!("load {}: {error}", path.display()))
    }
}

/// Native package write retained for benchmark and recovery fixtures.
#[cfg(test)]
pub(super) fn save_file(path: &Path, document: &WriterDocument) -> Result<(), String> {
    let bytes = loom_writer_core::save_document(document).map_err(|error| error.to_string())?;
    loom_storage::atomic_write(path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))
}

pub(super) fn writer_open_request(state: &GuiState) -> OpenFileRequest {
    let docx_filter = FileFilter::new("Word document (.docx)", ["docx"])
        .expect("the built-in DOCX extension is valid");
    OpenFileRequest {
        title: "Open Writer Document".into(),
        initial_directory: crate::initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![docx_filter, state.document_filter.clone()],
    }
}

pub(super) fn writer_save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow();
    let suggested_name = path
        .as_deref()
        .map(|path| {
            if has_extension(path, "docx") {
                path.to_path_buf()
            } else {
                path.with_extension("docx")
            }
        })
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Writer Document".into(),
        initial_directory: crate::initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: save_filters(),
    }
}

pub(super) fn save_current_document(
    app: &WriterApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    let current_path = state.save_path.borrow().clone();
    let direct_path = if force_picker {
        None
    } else {
        current_path.filter(|path| match save_format(path) {
            Some(SaveFormat::Docx) => true,
            Some(SaveFormat::Loomdoc) => state.compatibility_save.get(),
            None => false,
        })
    };
    let path = match direct_path {
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
    let (path, format) = normalize_save_path(&path)?;
    let document = state.current.borrow().clone();

    match format {
        SaveFormat::Loomdoc => {
            let bytes =
                loom_writer_core::save_document(&document).map_err(|error| error.to_string())?;
            write_and_commit(app, state, path, document, bytes, format)
        }
        SaveFormat::Docx => {
            let prepared = prepare_docx(&document)?;
            if prepared.losses.is_empty() {
                write_and_commit(app, state, path, document, prepared.bytes, format)
            } else {
                show_loss_warning(app, path, document, prepared);
                Ok(false)
            }
        }
    }
}

pub(super) fn warning_is_open(app: &WriterApp) -> bool {
    app.global::<DocxSaveWarningState>().get_dialog_open()
}

pub(super) fn wire_loss_dialog(
    app: &WriterApp,
    state: &Rc<GuiState>,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.global::<DocxSaveWarningState>()
            .on_continue_docx(move || {
                let Some(app) = app_ref.upgrade() else { return };
                match continue_pending_docx_save(&app, &state) {
                    Ok(true) => finish_pending_replacement(&app, &state, &menu_service),
                    Ok(false) => {}
                    Err(error) => {
                        app.set_status_left(SharedString::from(format!("Save failed: {error}")));
                    }
                }
                app.invoke_focus_page();
            });
    }
    {
        let app_ref = app.as_weak();
        app.global::<DocxSaveWarningState>()
            .on_cancel_docx(move || {
                let Some(app) = app_ref.upgrade() else { return };
                PENDING_DOCX_SAVE.with(|pending| *pending.borrow_mut() = None);
                let warning = app.global::<DocxSaveWarningState>();
                warning.set_dialog_open(false);
                warning.set_warning_message("".into());
                app.set_status_left("Word save cancelled; no file was written".into());
                if !app.get_save_changes_open() {
                    app.invoke_focus_page();
                }
            });
    }
}

fn finish_pending_replacement(
    app: &WriterApp,
    state: &GuiState,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    if !app.get_save_changes_open() || state.pending_replacement.get().is_none() {
        return;
    }
    app.set_save_changes_open(false);
    crate::continue_pending_replacement(app, state, menu_service);
}

fn continue_pending_docx_save(app: &WriterApp, state: &GuiState) -> Result<bool, String> {
    let pending = PENDING_DOCX_SAVE.with(|pending| pending.borrow_mut().take());
    let Some(pending) = pending else {
        app.global::<DocxSaveWarningState>().set_dialog_open(false);
        return Ok(false);
    };
    let warning = app.global::<DocxSaveWarningState>();
    warning.set_dialog_open(false);
    warning.set_warning_message("".into());

    if !document_content_equal(&state.current.borrow(), &pending.document) {
        app.set_status_left(
            "Document changed during the Word save review. Nothing was written; save again to review the updated document."
                .into(),
        );
        return Ok(false);
    }
    write_and_commit(
        app,
        state,
        pending.path,
        pending.document,
        pending.prepared.bytes,
        SaveFormat::Docx,
    )
}

fn show_loss_warning(
    app: &WriterApp,
    path: PathBuf,
    document: WriterDocument,
    prepared: PreparedDocx,
) {
    let warning = app.global::<DocxSaveWarningState>();
    warning.set_warning_message(SharedString::from(format_loss_warning(
        &path,
        &prepared.losses,
    )));
    PENDING_DOCX_SAVE.with(|pending| {
        *pending.borrow_mut() = Some(PendingDocxSave {
            path,
            document,
            prepared,
        });
    });
    warning.set_dialog_open(true);
    app.set_status_left("Review the Word save notice before writing the file".into());
}

fn format_loss_warning(path: &Path, losses: &[String]) -> String {
    let bullets = losses
        .iter()
        .map(|loss| format!("• {loss}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "This Word file will not preserve all Writer content:\n\n{bullets}\n\nNo file has been written yet. Continue to save this DOCX with those differences, or cancel and use Save As to choose Loom Writer full-fidelity (.loomdoc).\n\nDestination:\n{}",
        path.display()
    )
}

fn prepare_docx(document: &WriterDocument) -> Result<PreparedDocx, String> {
    // The existing inspector handles unsupported OOXML input constructs
    // (unsupported backgrounds, equations, legacy/floating images, SmartArt,
    // OLE/VBA, endnotes, and unsupported image formats). The
    // export/import comparison below covers the complete Writer model: format-8
    // page settings and appearance, headers/footers, footnotes, tracked
    // revisions, comment bodies/anchors/resolved state, and each rich block's
    // text, style, table, image, and chart payload. Import errors stop before
    // writing; differences become a pre-write confirmation.
    let export = loom_writer_core::export_docx(document).map_err(|error| error.to_string())?;
    let imported = loom_writer_core::import_docx(&export.bytes)
        .map_err(|error| format!("DOCX preflight could not reopen the export: {error}"))?;
    let mut losses = docx_round_trip_differences(document, &imported, export.comments_skipped);
    losses.extend(
        loom_writer_core::inspect_docx_unsupported(&export.bytes)
            .map_err(|error| format!("DOCX preflight could not inspect the export: {error}"))?,
    );
    deduplicate(&mut losses);
    Ok(PreparedDocx {
        bytes: export.bytes,
        losses,
    })
}

fn docx_round_trip_differences(
    source: &WriterDocument,
    reopened: &WriterDocument,
    comments_skipped: usize,
) -> Vec<String> {
    let mut losses = Vec::new();
    if source.title != reopened.title {
        losses.push("document title".into());
    }
    if source.page.appearance != reopened.page.appearance {
        losses.push("page appearance".into());
    }
    if !page_geometry_matches(source.page, reopened.page) {
        losses.push("paper size, orientation, margins, or custom page geometry".into());
    }
    if source.header_footer != reopened.header_footer {
        losses.push("headers or footers".into());
    }
    if !footnotes_match(source, reopened) {
        losses.push("footnote text, anchors, or numbering".into());
    }
    if !tracked_changes_match(source, reopened) {
        losses.push("unresolved tracked changes".into());
    }
    if comments_skipped > 0 {
        let noun = if comments_skipped == 1 {
            "comment thread"
        } else {
            "comment threads"
        };
        losses.push(format!(
            "{comments_skipped} {noun} cannot be represented in DOCX"
        ));
    }
    if !comments_match(source, reopened) {
        losses.push("comment text, anchors, or resolved state of representable threads".into());
    }
    losses.extend(block_differences(source, reopened));
    losses
}

fn page_geometry_matches(
    source: loom_writer_core::PageSetup,
    reopened: loom_writer_core::PageSetup,
) -> bool {
    let source = source.page_style();
    let reopened = reopened.page_style();
    // DOCX stores geometry in twips and paper sizes are written from their
    // millimetre definitions, so an equal page can differ by several tenths of
    // a point after a round trip. Half a point is still far below a visibly
    // different page and rejects a 1 pt or larger change.
    let same = |left: f32, right: f32| (left - right).abs() < 0.5;
    same(source.width_pt, reopened.width_pt)
        && same(source.height_pt, reopened.height_pt)
        && same(source.margin_top_pt, reopened.margin_top_pt)
        && same(source.margin_bottom_pt, reopened.margin_bottom_pt)
        && same(source.margin_left_pt, reopened.margin_left_pt)
        && same(source.margin_right_pt, reopened.margin_right_pt)
}

fn footnotes_match(source: &WriterDocument, reopened: &WriterDocument) -> bool {
    // DOCX can remap internal IDs; the anchor and note body are the user-visible
    // content that must survive the round trip.
    source.footnotes.len() == reopened.footnotes.len()
        && source
            .footnotes
            .iter()
            .zip(&reopened.footnotes)
            .all(|(left, right)| {
                left.offset == right.offset
                    && left.body == right.body
                    && block_index(source, left.block_id) == block_index(reopened, right.block_id)
            })
}

fn comments_match(source: &WriterDocument, reopened: &WriterDocument) -> bool {
    let expected: Vec<_> = source
        .comments
        .iter()
        .filter(|comment| comment_can_export(source, comment))
        .collect();
    expected.len() == reopened.comments.len()
        && expected
            .iter()
            .zip(&reopened.comments)
            .all(|(left, right)| {
                left.author == right.author
                    && left.start == right.start
                    && left.end == right.end
                    && left.body == right.body
                    && left.resolved == right.resolved
                    && block_index(source, left.block_id) == block_index(reopened, right.block_id)
            })
}

fn comment_can_export(
    document: &WriterDocument,
    comment: &loom_writer_core::CommentThread,
) -> bool {
    let Some(block) = document
        .blocks
        .iter()
        .find(|block| block.id == comment.block_id)
    else {
        return false;
    };
    let text = block.text.as_str();
    !comment.orphaned
        && block.kind != loom_writer_core::TABLE_BLOCK_KIND
        && comment.start <= comment.end
        && comment.end <= text.len()
        && text.is_char_boundary(comment.start)
        && text.is_char_boundary(comment.end)
}

fn tracked_changes_match(source: &WriterDocument, reopened: &WriterDocument) -> bool {
    let left = &source.tracked_changes;
    let right = &reopened.tracked_changes;
    left.recording() == right.recording()
        && left.author() == right.author()
        && left.next_revision_id() == right.next_revision_id()
        && left.revisions().len() == right.revisions().len()
        && left
            .revisions()
            .iter()
            .zip(right.revisions())
            .all(|(left, right)| {
                left.id == right.id
                    && left.author == right.author
                    && left.kind == right.kind
                    && left.boundary_kind == right.boundary_kind
                    && left.before == right.before
                    && left.after == right.after
                    && left.anchor.start == right.anchor.start
                    && left.anchor.end == right.anchor.end
                    && left.anchor.prefix_sha256 == right.anchor.prefix_sha256
                    && block_index(source, left.anchor.block_id)
                        == block_index(reopened, right.anchor.block_id)
                    && left
                        .anchor
                        .end_block_id
                        .and_then(|id| block_index(source, id))
                        == right
                            .anchor
                            .end_block_id
                            .and_then(|id| block_index(reopened, id))
            })
}

fn block_index(document: &WriterDocument, id: u64) -> Option<usize> {
    document.blocks.iter().position(|block| block.id == id)
}

fn block_differences(source: &WriterDocument, reopened: &WriterDocument) -> Vec<String> {
    let mut losses = Vec::new();
    let count = source.blocks.len().max(reopened.blocks.len());
    for index in 0..count {
        let left = source.blocks.get(index);
        let right = reopened.blocks.get(index);
        if matches!((left, right), (Some(left), Some(right)) if blocks_match(left, right)) {
            continue;
        }
        let kind = left.or(right);
        let Some(block) = kind else {
            continue;
        };
        let loss = if block.table.is_some() {
            "table content or formatting"
        } else if block.image.is_some() {
            "image payload or placement"
        } else if block.chart.is_some() {
            "chart data or formatting"
        } else {
            "text, paragraph formatting, or inline styling"
        };
        if !losses.iter().any(|existing| existing == loss) {
            losses.push(loss.into());
        }
    }
    losses
}

fn blocks_match(left: &loom_writer_core::RichBlock, right: &loom_writer_core::RichBlock) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.id = 0;
    right.id = 0;
    if let Some(table) = left.table.as_mut() {
        Arc::make_mut(table).id.clear();
    }
    if let Some(table) = right.table.as_mut() {
        Arc::make_mut(table).id.clear();
    }
    left == right
}

fn deduplicate(items: &mut Vec<String>) {
    let mut seen = Vec::new();
    items.retain(|item| {
        if seen.contains(item) {
            false
        } else {
            seen.push(item.clone());
            true
        }
    });
}

fn write_and_commit(
    app: &WriterApp,
    state: &GuiState,
    path: PathBuf,
    document: WriterDocument,
    bytes: Vec<u8>,
    format: SaveFormat,
) -> Result<bool, String> {
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))?;

    *state.save_path.borrow_mut() = Some(path.clone());
    state.compatibility_save.set(format == SaveFormat::Loomdoc);
    let checkpoint_result = recovery::checkpoint_document(&document, Some(path.as_path()));
    *state.last_saved.borrow_mut() = document;
    set_recovery_notice(None);
    file_title::sync(app, state);
    app.set_document_dirty(document_is_dirty(state));
    let status = match (format, checkpoint_result) {
        (_, Err(error)) => {
            eprintln!("Writer recovery checkpoint failed: {error}");
            "Saved. Recovery checkpoint failed; keep another copy.".to_owned()
        }
        (SaveFormat::Docx, Ok(())) => format!("Saved Word document {}", path.display()),
        (SaveFormat::Loomdoc, Ok(())) => {
            format!("Saved Loom full-fidelity document {}", path.display())
        }
    };
    app.set_status_left(SharedString::from(status));
    Ok(true)
}

fn normalize_save_path(path: &Path) -> Result<(PathBuf, SaveFormat), String> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err("Writer destination is empty".into());
    }
    let Some(extension) = path.extension() else {
        return Ok((path.with_extension("docx"), SaveFormat::Docx));
    };
    let extension = extension.to_string_lossy();
    let format = if extension.eq_ignore_ascii_case("docx") {
        SaveFormat::Docx
    } else if extension.eq_ignore_ascii_case("loomdoc") {
        SaveFormat::Loomdoc
    } else {
        return Err(format!(
            "Unsupported Writer extension '.{extension}'; choose .docx or .loomdoc"
        ));
    };
    let inner_extension = path
        .file_stem()
        .and_then(|stem| Path::new(stem).extension())
        .map(|extension| extension.to_string_lossy());
    if inner_extension.is_some_and(|inner| {
        (inner.eq_ignore_ascii_case("docx") || inner.eq_ignore_ascii_case("loomdoc"))
            && !inner.eq_ignore_ascii_case(&extension)
    }) {
        return Err(format!(
            "Conflicting Writer extensions in '{}'; choose one of .docx or .loomdoc",
            path.display()
        ));
    }
    Ok((path.to_path_buf(), format))
}

fn save_format(path: &Path) -> Option<SaveFormat> {
    if has_extension(path, "docx") {
        Some(SaveFormat::Docx)
    } else if has_extension(path, "loomdoc") {
        Some(SaveFormat::Loomdoc)
    } else {
        None
    }
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn save_filters() -> Vec<FileFilter> {
    vec![
        FileFilter::new("Word document (.docx)", ["docx"])
            .expect("the built-in DOCX extension is valid"),
        FileFilter::new("Loom Writer full-fidelity (.loomdoc)", ["loomdoc"])
            .expect("the built-in Loom extension is valid"),
    ]
}

#[cfg(test)]
fn prepare_docx_for_test(document: &WriterDocument) -> Result<Vec<String>, String> {
    Ok(prepare_docx(document)?.losses)
}

#[cfg(test)]
mod tests {
    use super::{normalize_save_path, prepare_docx_for_test, SaveFormat};
    use loom_writer_core::{
        CommentThread, FootnoteEntry, ImageFormat, InlineImage, PageAppearance, PageSetup,
        RichBlock, WriterDocument,
    };
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn extension_resolution_rejects_conflicting_or_unsupported_suffixes() {
        assert_eq!(
            normalize_save_path(Path::new("report")),
            Ok((Path::new("report.docx").to_path_buf(), SaveFormat::Docx))
        );
        assert_eq!(
            normalize_save_path(Path::new("report.loomdoc")),
            Ok((
                Path::new("report.loomdoc").to_path_buf(),
                SaveFormat::Loomdoc
            ))
        );
        assert!(normalize_save_path(Path::new("report.docx.loomdoc")).is_err());
        assert!(normalize_save_path(Path::new("report.rtf")).is_err());
    }

    #[test]
    fn save_filters_offer_docx_before_explicit_loomdoc_compatibility() {
        let filters = super::save_filters();
        assert_eq!(filters[0].extensions, vec!["docx"]);
        assert_eq!(filters[1].extensions, vec!["loomdoc"]);
    }

    #[test]
    fn supported_black_page_appearance_does_not_trigger_loss_confirmation() {
        let mut document = WriterDocument::new("appearance", "Black page");
        document.page.appearance = PageAppearance::Black;
        document
            .blocks
            .push(RichBlock::new(1, "paragraph", "Readable text"));

        let losses = prepare_docx_for_test(&document).expect("preflight exported DOCX");
        assert!(losses.is_empty(), "supported black appearance: {losses:?}");
    }

    #[test]
    fn page_geometry_preflight_compares_effective_measurements() {
        let mut source = WriterDocument::new("page-geometry", "Standard page");
        source
            .blocks
            .push(RichBlock::new(1, "paragraph", "Page geometry"));
        let mut reopened = source.clone();
        let style = source.page.page_style();
        reopened
            .page
            .set_custom_geometry_points(
                style.width_pt,
                style.height_pt,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .expect("represent the same geometry as custom settings");

        assert!(super::docx_round_trip_differences(&source, &reopened, 0).is_empty());
    }

    #[test]
    fn page_geometry_matches_twip_rounding_but_not_a_real_size_change() {
        let default = PageSetup::default();
        let style = default.page_style();
        let mut rounded = PageSetup::default();
        rounded
            .set_custom_geometry_points(
                595.3,
                841.9,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .expect("A4 rounding is valid geometry");
        assert!(
            super::page_geometry_matches(default, rounded),
            "A4 imported from DOCX twips is the same page"
        );

        let mut one_point_wider = rounded;
        one_point_wider
            .set_custom_geometry_points(
                596.3,
                841.9,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .expect("valid width");
        assert!(
            !super::page_geometry_matches(default, one_point_wider),
            "a 1 pt width change must be reported"
        );

        let mut one_centimetre_narrower = rounded;
        one_centimetre_narrower
            .set_custom_geometry_points(
                595.3 - 28.346,
                841.9,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .expect("valid width");
        assert!(
            !super::page_geometry_matches(default, one_centimetre_narrower),
            "a 1 cm width change must be reported"
        );
    }

    #[test]
    fn loss_map_names_page_metadata_review_and_each_rich_block_category() {
        let mut source = WriterDocument::new("features", "Features");
        source.blocks.push(RichBlock::new(1, "paragraph", "source"));
        let table = loom_objects::Table::new(1, 1);
        source.blocks.push(RichBlock::table_block(2, table));
        let mut image = RichBlock::new(3, "image", "image marker");
        image.image = Some(InlineImage {
            name: "fixture.png".into(),
            format: ImageFormat::Png,
            width_px: 1,
            height_px: 1,
            width_pt: 0.75,
            height_pt: 0.75,
            bytes: Arc::new(vec![1, 2, 3]),
        });
        source.blocks.push(image);
        let mut chart = RichBlock::new(4, "chart", "chart marker");
        chart.chart = Some(Arc::new("{\"chart\":1}".into()));
        source.blocks.push(chart);

        let mut reopened = source.clone();
        reopened.page.appearance = PageAppearance::Black;
        reopened.header_footer.header_text = "changed header".into();
        reopened.footnotes.push(FootnoteEntry {
            id: 1,
            block_id: 1,
            offset: 0,
            body: "note".into(),
        });
        reopened.tracked_changes.set_author("Reviewer").unwrap();
        reopened.comments.push(CommentThread {
            id: "1".into(),
            author: "Reviewer".into(),
            block_id: 1,
            start: 0,
            end: 1,
            body: "comment".into(),
            resolved: false,
            orphaned: false,
        });
        reopened.blocks[0].text = loom_writer_core::Text::from_str("changed");
        Arc::make_mut(reopened.blocks[1].table.as_mut().unwrap()).set_cell_text(0, 0, "changed");
        Arc::make_mut(&mut reopened.blocks[2].image.as_mut().unwrap().bytes).push(4);
        reopened.blocks[3].chart = Some(Arc::new("{\"chart\":2}".into()));

        let losses = super::docx_round_trip_differences(&source, &reopened, 0);

        for expected in [
            "page appearance",
            "headers or footers",
            "footnote text, anchors, or numbering",
            "unresolved tracked changes",
            "comment text, anchors, or resolved state of representable threads",
            "text, paragraph formatting, or inline styling",
            "table content or formatting",
            "image payload or placement",
            "chart data or formatting",
        ] {
            assert!(
                losses.iter().any(|loss| loss == expected),
                "{expected}: {losses:?}"
            );
        }
    }

    #[test]
    fn docx_preflight_reports_tracked_revision_boundary_kind_mismatch() {
        let mut source = WriterDocument::new("revision", "Revision boundary");
        source.blocks.push(RichBlock::new(1, "paragraph", "a!b"));
        source.tracked_changes.set_recording(true);
        source.tracked_changes.set_author("Reviewer").unwrap();
        source
            .record_revision(
                loom_writer_core::RevisionKind::Insertion,
                1,
                1..2,
                None,
                Some(loom_writer_core::RevisionPayload::plain("!")),
            )
            .unwrap();

        let export = loom_writer_core::export_docx(&source).expect("export fixture");
        let imported = loom_writer_core::import_docx(&export.bytes).expect("reopen fixture");
        assert!(
            super::tracked_changes_match(&source, &imported),
            "the baseline DOCX round trip should preserve the revision"
        );

        let mut altered_json = imported.to_content_json();
        let text_boundary = r#""boundary_kind":"text""#;
        assert_eq!(altered_json.matches(text_boundary).count(), 1);
        altered_json = altered_json.replace(text_boundary, r#""boundary_kind":"paragraph_mark""#);
        let altered_round_trip =
            WriterDocument::from_content_json(&altered_json).expect("parse altered fixture");

        let losses = super::docx_round_trip_differences(&source, &altered_round_trip, 0);
        assert!(
            losses
                .iter()
                .any(|loss| loss == "unresolved tracked changes"),
            "boundary-kind mismatch must be disclosed: {losses:?}"
        );
    }
}
