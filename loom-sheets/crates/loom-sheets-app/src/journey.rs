//! Headless journey execution and verification for Loom Sheets.

use std::path::Path;

use loom_sheets_core::{evaluate, CellRange, CellRef, GridSelection, Sheet};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{ComponentHandle, Model, PhysicalSize};

use crate::palette::{rebuild_palette, wire_palette};
use crate::{
    apply_headless_viewport_size, apply_layout_breakpoints, apply_sheet,
    apply_sheet_without_reveal, apply_theme, commit_formula_edit, configure_direction,
    fill_selection_down, fill_target_range, load_sheet, save_sheet, starter_workbook,
    update_selection, update_selection_range, Args, SheetsApp,
};

impl PaletteProbe for SheetsApp {
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

/// Record the keyboard command-palette journey with per-step screenshots.
pub fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    let sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None => starter_workbook(),
    };
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    apply_headless_viewport_size(&app, args.size.0, args.size.1);
    apply_sheet(&app, &sheet);
    let report = record_keyboard_palette_journey(&app, "sheets", Path::new(out_dir), "save")
        .map_err(|e| format!("journey failed: {e}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    run_sparse_edit_journey(args, Path::new(out_dir))?;
    Ok(())
}

/// Exercise the sparse-workbook path with the same controller helpers used by
/// the desktop app. The journey intentionally keeps only a handful of cells
/// in a 1,000-row worksheet, then records each durable transition so visual
/// and persistence evidence can be inspected alongside the keyboard journey.
pub fn run_sparse_edit_journey(args: &Args, out_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out_dir).map_err(|error| format!("journey output: {error}"))?;
    let app = SheetsApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    apply_headless_viewport_size(&app, args.size.0, args.size.1);
    app.set_show_inspector(true);

    let mut sheet = Sheet::new("Sparse 1000");
    for (c, v) in [
        ("A1", "10"),
        ("A2", "20"),
        ("A995", "10"),
        ("A996", "20"),
        ("A1000", "tail"),
    ] {
        sheet.set_str(c, v);
    }
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "01-start", args.size)?;

    // Scroll to the sparse tail. The negative Flickable viewport coordinate
    // is the same value that touchpad/mouse wheel gestures update.
    app.set_grid_scroll_y(-26_600.0);
    apply_sheet_without_reveal(&app, &sheet);
    capture_journey_frame(&app, out_dir, "02-scroll", args.size)?;

    let tail_selection = GridSelection::new(
        CellRef::parse("A995").expect("valid source coordinate"),
        CellRef::parse("A996").expect("valid source coordinate"),
    );
    update_selection_range(&app, &sheet, &evaluate(&sheet), tail_selection);
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "03-range", args.size)?;

    // Enter a formula in the active cell's adjacent column, then restore the
    // range as the fill source. Each operation contributes exactly one
    // typed operation to the existing undo stack.
    let formula_cell = CellRef::parse("B995").expect("valid formula coordinate");
    assert!(commit_formula_edit(
        &mut sheet,
        &mut undo,
        &mut redo,
        formula_cell,
        "=A995+1",
    ));
    update_selection(&app, &sheet, &evaluate(&sheet), formula_cell);
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "04-formula", args.size)?;

    update_selection_range(&app, &sheet, &evaluate(&sheet), tail_selection);
    assert!(fill_selection_down(
        &mut sheet,
        &mut undo,
        &mut redo,
        tail_selection
    ));
    let filled_range = CellRange::new(
        tail_selection.range().start,
        fill_target_range(tail_selection.range())
            .expect("multi-cell range has a fill target")
            .end,
    );
    update_selection_range(
        &app,
        &sheet,
        &evaluate(&sheet),
        GridSelection::new(filled_range.start, filled_range.end),
    );
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "05-fill", args.size)?;
    if sheet.raw(CellRef::parse("A997").unwrap()) != Some("10")
        || sheet.raw(CellRef::parse("A998").unwrap()) != Some("20")
    {
        return Err("sparse fill journey wrote unexpected values".to_string());
    }

    let Some(edit) = undo.pop() else {
        return Err("sparse fill journey did not record undo".to_string());
    };
    edit.revert(&mut sheet);
    redo.push(edit);
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "06-undo", args.size)?;
    if sheet.raw(CellRef::parse("A997").unwrap()).is_some()
        || sheet.raw(CellRef::parse("A998").unwrap()).is_some()
    {
        return Err("sparse fill journey undo left filled cells".to_string());
    }

    let save_path = out_dir.join("sparse-1000.loomtable");
    save_sheet(&save_path, &sheet)?;
    let reopened = load_sheet(&save_path)?;
    if reopened.dimensions().rows != 1_000
        || reopened.raw(CellRef::parse("B995").unwrap()) != Some("=A995+1")
    {
        return Err("sparse save/reopen changed workbook semantics".to_string());
    }
    apply_sheet(&app, &reopened);
    capture_journey_frame(&app, out_dir, "07-save-reopen", args.size)?;

    let evidence = format!(
        "rows={} cells={} selection={} formula={} undo={} save={}\n",
        reopened.dimensions().rows,
        reopened.cells.len(),
        tail_selection.label(),
        reopened.raw(formula_cell).unwrap_or(""),
        sheet.raw(CellRef::parse("A997").unwrap()).is_none(),
        save_path.display(),
    );
    std::fs::write(out_dir.join("sparse-journey.txt"), evidence)
        .map_err(|error| format!("journey evidence: {error}"))?;
    println!("sparse workbook journey: PASS ({})", out_dir.display());
    Ok(())
}

pub fn capture_journey_frame(
    app: &SheetsApp,
    out_dir: &Path,
    name: &str,
    size: (u32, u32),
) -> Result<(), String> {
    let image = snapshot_component(app, size.0 as f32, size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let path = out_dir.join(format!("{name}.png"));
    loom_test_support::png::save_png(&path, &image).map_err(|error| error.to_string())
}
