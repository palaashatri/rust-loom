//! Interactive Print Preview & Page Setup controller for Loom Sheets.

use std::path::PathBuf;
use std::rc::Rc;

use loom_sheets_core::{
    export_workbook_sheet_pdf_with_options, preview_workbook_sheet_page, CellAlignment, CellRef,
    PageOrientation, PaperSize, PdfExportOptions, PrintScale, SheetPagePreview,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{GuiState, PrintPreviewCellView, PrintPreviewHeaderView, PrintPreviewState, SheetsApp};

/// Margin presets in PDF points (72 pt = 1 inch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarginPreset {
    Narrow,
    Normal,
    Wide,
}

impl MarginPreset {
    fn from_index(idx: i32) -> Self {
        match idx {
            0 => Self::Narrow,
            2 => Self::Wide,
            _ => Self::Normal,
        }
    }

    fn margin_pt(self) -> f32 {
        match self {
            Self::Narrow => 18.0,
            Self::Normal => 36.0,
            Self::Wide => 72.0,
        }
    }
}

fn paper_size_from_index(idx: i32) -> PaperSize {
    match idx {
        1 => PaperSize::A4,
        2 => PaperSize::Legal,
        _ => PaperSize::Letter,
    }
}

fn orientation_from_index(idx: i32) -> PageOrientation {
    match idx {
        1 => PageOrientation::Landscape,
        _ => PageOrientation::Portrait,
    }
}

fn scale_from_index(idx: i32) -> PrintScale {
    match idx {
        1 => PrintScale::Scale75,
        2 => PrintScale::Scale50,
        3 => PrintScale::FitOnePageWide,
        _ => PrintScale::Actual100,
    }
}

/// Build `PdfExportOptions` from the current `PrintPreviewState` UI properties.
pub(crate) fn options_from_ui(app: &SheetsApp) -> PdfExportOptions {
    let ui = app.global::<PrintPreviewState>();
    let paper_size = paper_size_from_index(ui.get_paper_size());
    let orientation = orientation_from_index(ui.get_orientation());
    let (base_w, base_h) = paper_size.dimensions_pt();
    let (page_width_pt, page_height_pt) = match orientation {
        PageOrientation::Portrait => (base_w, base_h),
        PageOrientation::Landscape => (base_h, base_w),
    };
    PdfExportOptions {
        page_width_pt,
        page_height_pt,
        margin_pt: MarginPreset::from_index(ui.get_margin_preset()).margin_pt(),
        include_gridlines: ui.get_include_gridlines(),
        repeat_frozen_rows: ui.get_repeat_frozen_rows(),
        paper_size,
        orientation,
        scale: scale_from_index(ui.get_scale_mode()),
        include_headers: ui.get_include_headers(),
    }
}

/// Open the Print Preview & Page Setup modal and render page 1.
pub(crate) fn open(app: &SheetsApp, state: &GuiState) {
    // Commit a pending formula-bar draft first so the preview and PDF match
    // what is on screen. A rejected draft (validation, a paused recovery, or a
    // close in progress) keeps the edit visible and leaves the preview shut:
    // exporting the previous value would contradict the formula bar.
    let draft = app.get_formula_edit_buffer();
    if draft != app.get_selection_formula() && !commit_draft(app, state, draft.as_str()) {
        return;
    }
    let ui = app.global::<PrintPreviewState>();
    ui.set_current_page(1);
    ui.set_status_message(SharedString::new());
    ui.set_open(true);
    refresh_preview(app, state);
}

/// Commits the formula-bar draft and reports whether the sheet now holds it.
fn commit_draft(app: &SheetsApp, state: &GuiState, draft: &str) -> bool {
    let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) else {
        return false;
    };
    let before = state.current.borrow().raw(cell).map(str::to_owned);
    app.invoke_commit_selected_cell(SharedString::from(draft));
    let after = state.current.borrow().raw(cell).map(str::to_owned);
    before != after
}

/// Recompute paginated preview from the active sheet and current Page Setup options.
pub(crate) fn refresh_preview(app: &SheetsApp, state: &GuiState) {
    let options = options_from_ui(app);
    let ui = app.global::<PrintPreviewState>();
    let requested = ui.get_current_page().max(1) as usize;
    let (total, page) = {
        let (sheets, active) = crate::workbook_sheets(state);
        preview_workbook_sheet_page(&sheets, active, options, requested)
    };
    let total = total.max(1);
    let current = requested.min(total);
    ui.set_total_pages(total as i32);
    ui.set_current_page(current as i32);
    apply_page_to_ui(&ui, &page);
}

fn apply_page_to_ui(ui: &PrintPreviewState<'_>, page: &SheetPagePreview) {
    ui.set_page_width_pt(page.width_pt);
    ui.set_page_height_pt(page.height_pt);
    ui.set_margin_pt(page.margin_pt);
    ui.set_sheet_title(SharedString::from(page.sheet_title.as_str()));
    ui.set_is_empty(page.is_empty);

    let mut headers = Vec::new();
    if page.include_headers && !page.is_empty {
        let col_hdr_y = page.margin_pt + page.page_header_height_pt;
        headers.push(PrintPreviewHeaderView {
            x_pt: page.margin_pt,
            y_pt: col_hdr_y,
            w_pt: page.row_gutter_pt,
            h_pt: page.col_header_height_pt,
            label: SharedString::new(),
        });
        let mut x = page.margin_pt + page.row_gutter_pt;
        for (label, &w) in page.col_headers.iter().zip(page.col_widths_pt.iter()) {
            headers.push(PrintPreviewHeaderView {
                x_pt: x,
                y_pt: col_hdr_y,
                w_pt: w,
                h_pt: page.col_header_height_pt,
                label: SharedString::from(label.as_str()),
            });
            x += w;
        }
        let mut y = col_hdr_y + page.col_header_height_pt;
        for (label, &h) in page.row_headers.iter().zip(page.row_heights_pt.iter()) {
            headers.push(PrintPreviewHeaderView {
                x_pt: page.margin_pt,
                y_pt: y,
                w_pt: page.row_gutter_pt,
                h_pt: h,
                label: SharedString::from(label.as_str()),
            });
            y += h;
        }
    }
    ui.set_headers(ModelRc::new(VecModel::from(headers)));

    let cells: Vec<PrintPreviewCellView> = page
        .cells
        .iter()
        .map(|cell| {
            let (has_fill, fill_color) = match cell.fill_rgb {
                Some((r, g, b)) => (true, rgb_to_color((r, g, b))),
                None => (false, Color::from_argb_u8(0, 0, 0, 0)),
            };
            let align = match cell.align {
                CellAlignment::Center => 1,
                CellAlignment::Right => 2,
                _ => 0, // `cell.align` is already General-resolved by the core
            };
            PrintPreviewCellView {
                x_pt: cell.x_pt,
                y_pt: cell.y_pt,
                w_pt: cell.width_pt,
                h_pt: cell.height_pt,
                text: SharedString::from(cell.text.as_str()),
                bold: cell.bold,
                italic: cell.italic,
                align,
                has_fill,
                fill_color,
                text_color: rgb_to_color(cell.text_rgb),
                has_border: cell.has_border,
                font_pt: cell.font_size_pt,
            }
        })
        .collect();
    ui.set_cells(ModelRc::new(VecModel::from(cells)));
}

fn rgb_to_color((r, g, b): (f32, f32, f32)) -> Color {
    Color::from_rgb_u8(
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Export the active sheet to a user-chosen PDF path using current Page Setup options.
pub(crate) fn export_pdf_with_setup(app: &SheetsApp, state: &GuiState) {
    let request = crate::export_pdf_request(state);
    match state.dialogs.save_file(&request) {
        Ok(Some(path)) => {
            let options = options_from_ui(app);
            let (sheets, active) = crate::workbook_sheets(state);
            let bytes = export_workbook_sheet_pdf_with_options(&sheets, active, options);
            let ui = app.global::<PrintPreviewState>();
            match loom_storage::atomic_write(&path, &bytes) {
                Ok(()) => {
                    let msg = format!("Exported PDF to {}", path.display());
                    ui.set_status_message(SharedString::from(msg.as_str()));
                    app.set_status_left(SharedString::from(msg));
                }
                Err(err) => {
                    let msg = format!("PDF export failed: {err}");
                    ui.set_status_message(SharedString::from(msg.as_str()));
                    app.set_status_left(SharedString::from(msg));
                }
            }
        }
        Ok(None) => {
            app.global::<PrintPreviewState>()
                .set_status_message("PDF export cancelled".into());
        }
        Err(err) => {
            let msg = format!("Export dialog failed: {err}");
            app.global::<PrintPreviewState>()
                .set_status_message(SharedString::from(msg.as_str()));
        }
    }
}

/// Write a temporary PDF for printing and return its path.
pub(crate) fn write_print_temp_pdf(app: &SheetsApp, state: &GuiState) -> Result<PathBuf, String> {
    let options = options_from_ui(app);
    let (sheets, active) = crate::workbook_sheets(state);
    let bytes = export_workbook_sheet_pdf_with_options(&sheets, active, options);
    let sanitized: String = sheets[active]
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = if sanitized.is_empty() {
        "sheet".to_string()
    } else {
        sanitized
    };
    static PRINT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = PRINT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "loom-sheets-print-{stem}-{}-{nanos}-{seq}.pdf",
        std::process::id()
    ));
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|err| format!("Failed to prepare print PDF: {err}"))?;
    Ok(path)
}

/// Generate a temporary PDF with current Page Setup options and open it in the
/// platform print/preview handler.
pub(crate) fn print_document(app: &SheetsApp, state: &GuiState) {
    let ui = app.global::<PrintPreviewState>();
    let path = match write_print_temp_pdf(app, state) {
        Ok(path) => path,
        Err(err) => {
            ui.set_status_message(SharedString::from(err.as_str()));
            app.set_status_left(SharedString::from(err));
            return;
        }
    };
    match open_platform_print_target(&path) {
        Ok(()) => {
            let msg = format!("Opened {} for printing", path.display());
            ui.set_status_message(SharedString::from(msg.as_str()));
            app.set_status_left(SharedString::from(msg));
        }
        Err(err) => {
            let msg = format!(
                "Prepared {} (system open unavailable: {err})",
                path.display()
            );
            ui.set_status_message(SharedString::from(msg.as_str()));
            app.set_status_left(SharedString::from(msg));
        }
    }
}

fn open_platform_print_target(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn()
            .map(|_| ())
    }
}

/// Wire `PrintPreviewState` callbacks on `app`.
pub(crate) fn wire_callbacks(app: &SheetsApp, state: &Rc<GuiState>) {
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>().on_open_preview(move || {
            if let Some(app) = app_ref.upgrade() {
                open(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.global::<PrintPreviewState>().on_close_preview(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<PrintPreviewState>().set_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>()
            .on_settings_changed(move || {
                if let Some(app) = app_ref.upgrade() {
                    refresh_preview(&app, &state);
                }
            });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>().on_prev_page(move || {
            if let Some(app) = app_ref.upgrade() {
                let ui = app.global::<PrintPreviewState>();
                let cur = ui.get_current_page();
                if cur > 1 {
                    ui.set_current_page(cur - 1);
                    refresh_preview(&app, &state);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>().on_next_page(move || {
            if let Some(app) = app_ref.upgrade() {
                let ui = app.global::<PrintPreviewState>();
                let cur = ui.get_current_page();
                let total = ui.get_total_pages();
                if cur < total {
                    ui.set_current_page(cur + 1);
                    refresh_preview(&app, &state);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>().on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                export_pdf_with_setup(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<PrintPreviewState>()
            .on_print_document(move || {
                if let Some(app) = app_ref.upgrade() {
                    print_document(&app, &state);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::{FileFilter, ScriptedFileDialogs};
    use loom_sheets_core::{CellRef, Sheet};
    use slint::Model;

    fn test_state_with_dialogs(sheet: Sheet, save_paths: Vec<Option<PathBuf>>) -> Rc<GuiState> {
        let dialogs = Rc::new(ScriptedFileDialogs::new([], save_paths));
        Rc::new(GuiState::new(
            sheet,
            None,
            dialogs,
            FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("Excel", ["xlsx"]).expect("filter"),
        ))
    }

    #[test]
    fn open_and_page_setup_controls_update_preview_and_pagination() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("Q4");
        for row in 0..60u32 {
            for col in 0..10u32 {
                sheet.set_str(&CellRef { row, col }.to_a1(), &format!("R{row}C{col}"));
            }
        }
        let state = test_state_with_dialogs(sheet, vec![]);
        wire_callbacks(&app, &state);

        open(&app, &state);
        let ui = app.global::<PrintPreviewState>();
        assert!(ui.get_open());
        assert_eq!(ui.get_current_page(), 1);
        assert!(ui.get_total_pages() >= 2);
        assert_eq!(ui.get_page_width_pt(), 612.0);
        assert_eq!(ui.get_page_height_pt(), 792.0);
        assert!(ui.get_cells().row_count() > 0);

        ui.invoke_next_page();
        assert_eq!(ui.get_current_page(), 2);
        ui.invoke_prev_page();
        assert_eq!(ui.get_current_page(), 1);

        // Switch to Landscape + A4 + FitOnePageWide
        ui.set_paper_size(1);
        ui.set_orientation(1);
        ui.set_scale_mode(3);
        ui.invoke_settings_changed();
        assert_eq!(ui.get_page_width_pt(), 841.89);
        assert_eq!(ui.get_page_height_pt(), 595.28);

        // Toggle headers off
        ui.set_include_headers(false);
        ui.invoke_settings_changed();
        assert_eq!(ui.get_headers().row_count(), 0);

        ui.invoke_close_preview();
        assert!(!ui.get_open());
    }

    #[test]
    fn export_pdf_and_write_print_temp_pdf_produce_valid_pdf_files() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("Invoice");
        sheet.set_str("A1", "Item");
        sheet.set_str("B1", "Amount");
        sheet.set_str("A2", "Hosting");
        sheet.set_str("B2", "49");

        let export_path = std::env::temp_dir().join("loom-sheets-preview-export-test.pdf");
        let _ = std::fs::remove_file(&export_path);
        let state = test_state_with_dialogs(sheet, vec![Some(export_path.clone())]);
        wire_callbacks(&app, &state);

        open(&app, &state);
        app.global::<PrintPreviewState>().invoke_export_pdf();
        let exported = std::fs::read(&export_path).expect("exported pdf exists");
        assert!(exported.starts_with(b"%PDF-1.4"));
        assert!(exported.windows(5).any(|w| w == b"%%EOF"));
        let _ = std::fs::remove_file(&export_path);

        let print_path = write_print_temp_pdf(&app, &state).expect("temp print pdf");
        let printed = std::fs::read(&print_path).expect("print pdf exists");
        assert!(printed.starts_with(b"%PDF-1.4"));
        assert!(printed.windows(5).any(|w| w == b"%%EOF"));
        let _ = std::fs::remove_file(&print_path);
    }

    #[test]
    fn print_temp_pdfs_are_unique_per_operation() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("Invoice");
        sheet.set_str("A1", "x");
        let state = test_state_with_dialogs(sheet, vec![]);
        wire_callbacks(&app, &state);
        let a = write_print_temp_pdf(&app, &state).expect("first");
        let b = write_print_temp_pdf(&app, &state).expect("second");
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn preview_right_aligns_general_numbers_and_shows_only_selected_page() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("Nums");
        for row in 1..=200 {
            sheet.set_str(&format!("A{row}"), &row.to_string());
        }
        let state = test_state_with_dialogs(sheet, vec![]);
        wire_callbacks(&app, &state);
        open(&app, &state);
        let ui = app.global::<PrintPreviewState>();
        assert!(ui.get_total_pages() > 1);
        let first = ui.get_cells().row_data(0).expect("cell");
        assert_eq!(first.align, 2);
        assert!(ui.get_cells().row_count() < 200);
    }

    #[test]
    fn open_commits_pending_formula_draft() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("Draft");
        sheet.set_str("A1", "old");
        let state = test_state_with_dialogs(sheet, vec![]);
        crate::cell_actions::register_cell_edit_action(
            &app,
            &state,
            &std::sync::Arc::new(crate::NativeMenuBar::new()),
        );
        wire_callbacks(&app, &state);
        app.set_selected_cell("A1".into());
        app.set_selection_formula("old".into());
        app.set_formula_edit_buffer("new".into());
        open(&app, &state);
        assert_eq!(
            state.current.borrow().raw(CellRef::parse("A1").unwrap()),
            Some("new")
        );
    }
}
