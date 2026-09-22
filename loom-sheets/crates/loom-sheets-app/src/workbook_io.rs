//! Workbook construction, templates, persistence, and recovery for Sheets.

use std::path::Path;

use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::{
    from_csv_sniffed, sheet_from_json, workbook_from_json, workbook_to_json, Sheet,
};

use crate::assets;

pub(crate) fn blank_sheet() -> Sheet {
    Sheet::new("Untitled")
}

/// A small, editable budget workbook used by explicit example/smoke captures.
/// Every amount has the same unit so the formulas show a trustworthy result.
pub(crate) fn starter_workbook() -> Sheet {
    let mut sheet = Sheet::new("Example Budget");
    for (c, v) in [
        ("A1", "Item"),
        ("A2", "Rent"),
        ("A3", "Food"),
        ("A4", "Transport"),
        ("A5", "Total"),
        ("A6", "Average"),
        ("B1", "Amount (USD/month)"),
        ("B2", "1200"),
        ("B3", "450"),
        ("B4", "150"),
        ("B5", "=SUM(B2:B4)"),
        ("B6", "=AVERAGE(B2:B4)"),
        ("C1", "Period"),
        ("C2", "Monthly"),
        ("C3", "Monthly"),
        ("C4", "Monthly"),
    ] {
        sheet.set_str(c, v);
    }
    // Keep the example's complete unit label visible without manual resizing.
    sheet.set_col_width(1, 190.0);
    sheet
}

/// Build the workbook for a template-chooser card index. Every advertised
/// card (0-10) produces its named sheet with live formulas; unknown indices
/// fall back to a blank sheet.
pub(crate) fn template_sheet(idx: i32) -> Sheet {
    match idx {
        1 => seeded_sheet(
            "Monthly Budget",
            &[
                ("A1", "Category"),
                ("B1", "Projected"),
                ("C1", "Actual"),
                ("A2", "Housing"),
                ("B2", "1200"),
                ("C2", "1200"),
                ("A3", "Food"),
                ("B3", "400"),
                ("C3", "450"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        2 => seeded_sheet(
            "Invoice",
            &[
                ("A1", "Description"),
                ("B1", "Hours"),
                ("C1", "Rate"),
                ("D1", "Amount"),
                ("A2", "Design Work"),
                ("B2", "20"),
                ("C2", "85"),
                ("D2", "=B2*C2"),
            ],
        ),
        3 => seeded_sheet(
            "Checklist",
            &[
                ("A1", "Task"),
                ("B1", "Done"),
                ("A2", "Review budget"),
                ("B2", "x"),
                ("A3", "Pay rent"),
                ("B3", "x"),
                ("A4", "Book flights"),
                ("B4", ""),
                ("A5", "Call bank"),
                ("B5", ""),
                ("A7", "Done"),
                ("B7", "=COUNTIF(B2:B5, \"x\")"),
                ("A8", "Total"),
                ("B8", "=COUNTA(A2:A5)"),
            ],
        ),
        4 => seeded_sheet(
            "Table and Chart",
            &[
                ("A1", "Quarter"),
                ("B1", "Revenue"),
                ("C1", "Expenses"),
                ("A2", "Q1"),
                ("B2", "15000"),
                ("C2", "9200"),
                ("A3", "Q2"),
                ("B3", "18500"),
                ("C3", "11000"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        5 => seeded_sheet(
            "Expense Summary",
            &[
                ("A1", "Category"),
                ("B1", "Amount"),
                ("A2", "Food"),
                ("B2", "320"),
                ("A3", "Travel"),
                ("B3", "150"),
                ("A4", "Food"),
                ("B4", "85"),
                ("A5", "Utilities"),
                ("B5", "120"),
                ("A7", "Total"),
                ("B7", "=SUM(B2:B5)"),
                ("A8", "Food total"),
                ("B8", "=SUMIF(A2:A5, \"Food\", B2:B5)"),
            ],
        ),
        6 => seeded_sheet(
            "Sales Chart",
            &[
                ("A1", "Month"),
                ("B1", "Sales"),
                ("A2", "Jan"),
                ("B2", "4200"),
                ("A3", "Feb"),
                ("B3", "5100"),
                ("A4", "Mar"),
                ("B4", "4800"),
                ("A5", "Apr"),
                ("B5", "6300"),
                ("A6", "Total"),
                ("B6", "=SUM(B2:B5)"),
            ],
        ),
        7 => seeded_sheet(
            "Budget",
            &[
                ("A1", "Category"),
                ("B1", "Budget"),
                ("C1", "Spent"),
                ("A2", "Housing"),
                ("B2", "1500"),
                ("C2", "1500"),
                ("A3", "Food"),
                ("B3", "600"),
                ("C3", "520"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        8 => seeded_sheet(
            "Monthly Goal",
            &[
                ("A1", "Goal"),
                ("B1", "Target"),
                ("C1", "Saved"),
                ("D1", "Progress"),
                ("A2", "Emergency fund"),
                ("B2", "5000"),
                ("C2", "3250"),
                ("D2", "=C2/B2"),
                ("A3", "Vacation"),
                ("B3", "2000"),
                ("C3", "2000"),
                ("D3", "=C3/B3"),
                ("A4", "Totals"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        9 => seeded_sheet(
            "Portfolio",
            &[
                ("A1", "Holding"),
                ("B1", "Shares"),
                ("C1", "Price"),
                ("D1", "Value"),
                ("A2", "LOOM"),
                ("B2", "100"),
                ("C2", "42.5"),
                ("D2", "=B2*C2"),
                ("A3", "ACME"),
                ("B3", "50"),
                ("C3", "18"),
                ("D3", "=B3*C3"),
                ("A4", "Total"),
                ("D4", "=SUM(D2:D3)"),
            ],
        ),
        10 => seeded_sheet(
            "Net Worth",
            &[
                ("A1", "Item"),
                ("B1", "Amount"),
                ("A2", "Savings"),
                ("B2", "12000"),
                ("A3", "Investments"),
                ("B3", "8500"),
                ("A4", "Total assets"),
                ("B4", "=SUM(B2:B3)"),
                ("A5", "Credit card"),
                ("B5", "1200"),
                ("A6", "Net worth"),
                ("B6", "=B4-B5"),
            ],
        ),
        _ => blank_sheet(),
    }
}

/// Fill a named sheet from (cell, raw) pairs.
fn seeded_sheet(name: &str, cells: &[(&str, &str)]) -> Sheet {
    let mut sheet = Sheet::new(name);
    for (cell, raw) in cells {
        sheet.set_str(cell, raw);
    }
    sheet
}

/// Restore a crash-recovery payload. New snapshots use the same validated
/// package as a saved workbook, so embedded image bytes survive a source file
/// disappearing. Older builds wrote plain JSON, which remains a fallback.
pub(crate) fn restore_workbook_from_snapshot(
    payload: &[u8],
) -> Option<loom_sheets_core::persistence::WorkbookFile> {
    use loom_sheets_core::persistence::WorkbookFile;
    if let Ok(archive) = PackageArchive::from_bytes(payload) {
        if let Ok(workbook) = workbook_from_package(&archive) {
            return Some(workbook);
        }
    }
    let json = std::str::from_utf8(payload).ok()?;
    workbook_from_json(json).ok().or_else(|| {
        sheet_from_json(json).ok().map(|sheet| WorkbookFile {
            sheets: vec![sheet],
            active: 0,
        })
    })
}

pub(crate) fn load_sheet(path: &Path) -> Result<Sheet, String> {
    let workbook = load_workbook(path)?;
    Ok(workbook
        .sheets
        .get(workbook.active)
        .cloned()
        .unwrap_or_else(|| Sheet::new("Untitled")))
}

/// Load a workbook: all tabs plus the active tab index. Legacy single-sheet
/// packages (`content/sheet.json`) load as a one-tab workbook.
pub(crate) fn load_workbook(
    path: &Path,
) -> Result<loom_sheets_core::persistence::WorkbookFile, String> {
    use loom_sheets_core::persistence::WorkbookFile;
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        let csv = std::str::from_utf8(&bytes).map_err(|e| format!("csv utf8: {e}"))?;
        return Ok(WorkbookFile {
            sheets: vec![from_csv_sniffed("imported", csv)],
            active: 0,
        });
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    {
        let sheets = loom_sheets_core::extract_xlsx_sheets(&bytes)?;
        if sheets.is_empty() {
            return Err("xlsx workbook has no worksheets".to_string());
        }
        return Ok(WorkbookFile { sheets, active: 0 });
    }
    let arch = PackageArchive::from_bytes(&bytes).map_err(|e| format!("archive: {e}"))?;
    workbook_from_package(&arch)
}

pub(crate) fn save_sheet(path: &Path, sheet: &Sheet) -> Result<(), String> {
    save_workbook(path, std::slice::from_ref(sheet), 0)
}

/// Save every tab plus the active tab index as `content/workbook.json`.
pub(crate) fn save_workbook(path: &Path, sheets: &[Sheet], active: usize) -> Result<(), String> {
    let bytes = workbook_package_bytes(sheets, active)?;
    loom_storage::atomic_write(path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))
}

/// Build the complete native workbook package in memory. Recovery snapshots
/// use this exact path so package assets, manifest checksums, and workbook
/// JSON cannot drift apart from normal saves.
pub(crate) fn workbook_package_bytes(sheets: &[Sheet], active: usize) -> Result<Vec<u8>, String> {
    if sheets.is_empty() {
        return Err("cannot save an empty workbook".to_string());
    }
    let (persisted_sheets, embedded_assets) = assets::prepare_workbook(sheets)?;
    let mut arch = PackageArchive::new();
    let json = workbook_to_json(&persisted_sheets, active);
    arch.add("content/workbook.json", json.clone().into_bytes())
        .map_err(|e| e.to_string())?;
    let title = persisted_sheets
        .get(active.min(sheets.len() - 1))
        .map(|sheet| sheet.name.clone())
        .unwrap_or_else(|| "Untitled".to_string());
    let mut entries = vec![ManifestEntry {
        path: "content/workbook.json".into(),
        mime: MimeType::parse("application/vnd.loom.sheet-content")
            .map_err(|e| format!("invalid built-in sheets MIME type: {e}"))?,
        size: json.len() as u64,
        sha256: Checksum::from_bytes(loom_package::zip::sha256(json.as_bytes())),
    }];
    for asset in embedded_assets {
        let size = asset.bytes.len() as u64;
        let sha256 = Checksum::from_bytes(loom_package::zip::sha256(&asset.bytes));
        arch.add(&asset.path, asset.bytes)
            .map_err(|e| e.to_string())?;
        entries.push(ManifestEntry {
            path: asset.path,
            mime: MimeType::parse(asset.mime)
                .map_err(|e| format!("invalid image MIME type: {e}"))?,
            size,
            sha256,
        });
    }
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Sheets,
        id: "sheets-doc".to_string(),
        title,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries,
    };
    let manifest_str = pkg_json::write(&manifest);
    arch.add("manifest.json", manifest_str.into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

/// Decode and validate a native workbook package, including its embedded
/// image assets. This is shared by disk loads and crash-recovery restores.
fn workbook_from_package(
    arch: &PackageArchive,
) -> Result<loom_sheets_core::persistence::WorkbookFile, String> {
    use loom_sheets_core::persistence::WorkbookFile;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Sheets {
        return Err("not a Sheets workbook".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    if let Some(content) = arch.get("content/workbook.json") {
        let s = std::str::from_utf8(content).map_err(|_| "workbook not utf8".to_string())?;
        let mut workbook = workbook_from_json(s).map_err(|e| format!("workbook: {e}"))?;
        assets::attach_workbook_assets(&mut workbook, arch)?;
        return Ok(workbook);
    }
    let content = arch
        .get("content/sheet.json")
        .ok_or_else(|| "missing sheet.json".to_string())?;
    let s = std::str::from_utf8(content).map_err(|_| "sheet not utf8".to_string())?;
    let sheet = sheet_from_json(s).map_err(|e| format!("sheet: {e}"))?;
    let mut workbook = WorkbookFile {
        sheets: vec![sheet],
        active: 0,
    };
    assets::attach_workbook_assets(&mut workbook, arch)?;
    Ok(workbook)
}
