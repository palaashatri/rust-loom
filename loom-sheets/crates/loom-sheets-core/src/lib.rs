//! Loom Sheets formula engine and workbook model — headless and testable.
//!
//! The engine implements a tokenizer, a recursive-descent parser, cell
//! reference resolution, and a dependency-graph evaluator with topological
//! ordering and cycle detection. CSV import/export is included for
//! interoperability. The GUI (a documented follow-on) consumes this engine.

use std::collections::{BTreeMap, HashMap, HashSet};

use loom_package::zip::PackageArchive;

pub mod charts;
pub use charts::{
    ChartKind, ChartPlacement, ChartSeries, ChartSpec, ChartUpdatePolicy, SheetChart,
};

pub mod banded;
pub mod dates;
pub mod functions;
pub(crate) mod functions_date;
pub(crate) mod functions_finance;
pub(crate) mod functions_find;
pub(crate) mod functions_format;
pub(crate) mod functions_logic;
pub(crate) mod functions_lookup;
pub(crate) mod functions_math;
pub(crate) mod functions_stats;
pub(crate) mod functions_text;
pub(crate) mod functions_util;
pub mod interop;
mod number_text;
pub mod objects;
pub mod persistence;
pub mod refs;
pub mod row_order;
pub mod style;
mod style_fit;
pub mod workbook;
pub mod xlsx;
pub use banded::{BandedMap, StoredValue};
pub use interop::{
    from_csv, from_csv_sniffed, from_csv_with_dialect, parse_csv_records, sniff_csv_dialect,
    to_csv, to_csv_with_formulas, to_csv_with_values, CsvDialect,
};
pub use objects::{SheetObject, SheetObjectKind};
pub use persistence::{
    sheet_from_json, sheet_to_json, workbook_from_json, workbook_to_json, WorkbookFile,
};
pub use row_order::RowOrder;
pub use style::{CellAlignment, CellStyle};
pub use xlsx::{
    export_xlsx_sheets, extract_xlsx_sheets, import_xlsx_sheets, XlsxChartType, XlsxImport,
    XlsxImportWarning,
};

/// Default width of a worksheet column in the desktop editor, in pixels.
///
/// Keeping this value in the core model gives the headless engine and the
/// Slint editor one source of truth for their initial grid geometry.
pub const DEFAULT_COL_WIDTH: f32 = 80.0;
/// Default height of a worksheet row in the desktop editor, in pixels.
pub const DEFAULT_ROW_HEIGHT: f32 = 24.0;

/// A cell coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    /// Row (0-based).
    pub row: u32,
    /// Column (0-based).
    pub col: u32,
}

/// The last column (XFD) and row an A1 reference can name, as in other
/// spreadsheets. Longer text is not a cell reference.
const MAX_COLUMNS: u32 = 16_384;
const MAX_ROWS: u32 = 1_048_576;

impl CellRef {
    /// Parse an A1-style reference like "B3" (col letter(s), then row number).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_uppercase();
        let bytes = s.as_bytes();
        let mut col: u32 = 0;
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            col = col * 26 + (bytes[i] - b'A' + 1) as u32;
            if col > MAX_COLUMNS {
                return None;
            }
            i += 1;
        }
        if col == 0 {
            return None;
        }
        let row_str = &s[i..];
        if row_str.is_empty() {
            return None;
        }
        let row: u32 = row_str.parse().ok()?;
        if row == 0 || row > MAX_ROWS {
            return None;
        }
        Some(Self {
            row: row - 1,
            col: col - 1,
        })
    }

    /// Render as A1 (e.g. `B3`).
    pub fn to_a1(self) -> String {
        let mut col = self.col + 1;
        let mut letters = String::new();
        while col > 0 {
            let rem = (col - 1) % 26;
            letters.insert(0, (b'A' + rem as u8) as char);
            col = (col - 1) / 26;
        }
        format!("{}{}", letters, self.row + 1)
    }
}

/// The logical dimensions of a worksheet's addressable grid.
///
/// Dimensions are derived from the sparse cells that are present, but are
/// always non-zero so an empty sheet still has an editable A1 cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SheetDimensions {
    /// Number of rows in the logical worksheet.
    pub rows: u32,
    /// Number of columns in the logical worksheet.
    pub cols: u32,
}

impl SheetDimensions {
    /// Creates dimensions with a one-cell minimum in each axis.
    pub const fn new(rows: u32, cols: u32) -> Self {
        Self {
            rows: if rows == 0 { 1 } else { rows },
            cols: if cols == 0 { 1 } else { cols },
        }
    }

    /// Returns the full content size for fixed row/column extents.
    pub fn content_size(self, row_height: f32, col_width: f32) -> (f32, f32) {
        (
            self.cols as f32 * col_width.max(1.0),
            self.rows as f32 * row_height.max(1.0),
        )
    }
}

/// The bounded portion of a worksheet currently projected into an editor grid.
///
/// The workbook stays sparse and unbounded; this value only records the
/// scroll offsets and the number of rows and columns a UI is rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetViewport {
    /// Zero-based first visible worksheet row.
    pub first_row: u32,
    /// Zero-based first visible worksheet column.
    pub first_col: u32,
    /// Number of visible rows. Always at least one.
    pub visible_rows: u32,
    /// Number of visible columns. Always at least one.
    pub visible_cols: u32,
}

impl SheetViewport {
    /// Starts at the worksheet origin with a non-empty visible window.
    pub fn new(visible_rows: u32, visible_cols: u32) -> Self {
        Self {
            first_row: 0,
            first_col: 0,
            visible_rows: visible_rows.max(1),
            visible_cols: visible_cols.max(1),
        }
    }

    /// Projects pixel scroll offsets into a bounded worksheet window.
    ///
    /// The projection clamps negative/over-large offsets to the workbook
    /// extent and computes the number of complete visible rows/columns from
    /// the viewport size. A caller can render just this slice while retaining
    /// the full sparse workbook in memory.
    pub fn from_scroll(
        scroll_x: f32,
        scroll_y: f32,
        viewport_width: f32,
        viewport_height: f32,
        row_height: f32,
        col_width: f32,
        dimensions: SheetDimensions,
    ) -> Self {
        let row_height = if row_height.is_finite() && row_height > 0.0 {
            row_height
        } else {
            1.0
        };
        let col_width = if col_width.is_finite() && col_width > 0.0 {
            col_width
        } else {
            1.0
        };
        let viewport_width = if viewport_width.is_finite() && viewport_width > 0.0 {
            viewport_width
        } else {
            col_width
        };
        let viewport_height = if viewport_height.is_finite() && viewport_height > 0.0 {
            viewport_height
        } else {
            row_height
        };
        let dimensions = SheetDimensions::new(dimensions.rows, dimensions.cols);
        let (content_width, content_height) = dimensions.content_size(row_height, col_width);
        let max_scroll_x = (content_width - viewport_width).max(0.0);
        let max_scroll_y = (content_height - viewport_height).max(0.0);
        let scroll_x = if scroll_x.is_finite() {
            scroll_x.clamp(0.0, max_scroll_x)
        } else {
            0.0
        };
        let scroll_y = if scroll_y.is_finite() {
            scroll_y.clamp(0.0, max_scroll_y)
        } else {
            0.0
        };
        let first_col = ((scroll_x / col_width).floor() as u32).min(dimensions.cols - 1);
        let first_row = ((scroll_y / row_height).floor() as u32).min(dimensions.rows - 1);
        let visible_cols = ((viewport_width / col_width).ceil() as u32)
            .max(1)
            .min(dimensions.cols - first_col);
        let visible_rows = ((viewport_height / row_height).ceil() as u32)
            .max(1)
            .min(dimensions.rows - first_row);
        Self {
            first_row,
            first_col,
            visible_rows,
            visible_cols,
        }
    }

    /// Returns the canonical pixel offset represented by this projection.
    pub fn scroll_offsets(self, row_height: f32, col_width: f32) -> (f32, f32) {
        (
            self.first_col as f32 * col_width.max(1.0),
            self.first_row as f32 * row_height.max(1.0),
        )
    }

    /// Returns whether the cell is currently inside the projected window.
    pub fn contains(self, cell: CellRef) -> bool {
        cell.row >= self.first_row
            && cell.row - self.first_row < self.visible_rows
            && cell.col >= self.first_col
            && cell.col - self.first_col < self.visible_cols
    }

    /// Moves the window only as far as needed to make `cell` visible.
    pub fn reveal(&mut self, cell: CellRef) {
        if cell.row < self.first_row {
            self.first_row = cell.row;
        } else if cell.row - self.first_row >= self.visible_rows {
            self.first_row = cell.row.saturating_sub(self.visible_rows - 1);
        }

        if cell.col < self.first_col {
            self.first_col = cell.col;
        } else if cell.col - self.first_col >= self.visible_cols {
            self.first_col = cell.col.saturating_sub(self.visible_cols - 1);
        }
    }

    /// Resolves a local visible-row index to a worksheet row.
    pub fn row_at(self, index: u32) -> Option<u32> {
        (index < self.visible_rows)
            .then(|| self.first_row.checked_add(index))
            .flatten()
    }

    /// Resolves a local visible-column index to a worksheet column.
    pub fn column_at(self, index: u32) -> Option<u32> {
        (index < self.visible_cols)
            .then(|| self.first_col.checked_add(index))
            .flatten()
    }
}

/// A cell value produced by evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number.
    Number(f64),
    /// A text string.
    Text(String),
    /// Boolean.
    Bool(bool),
    /// Empty cell.
    Empty,
    /// An error value produced by evaluation.
    Error(CalcError),
    /// Array result of a spill function: flat row-major values plus
    /// dimensions. Displays as its top-left element; neighbors show spilled
    /// elements, and array input outside aggregations is `#VALUE!`.
    Array(Vec<Value>, usize, usize),
}

impl Value {
    /// Display a value for export or console.
    pub fn display(&self) -> String {
        match self {
            Self::Number(n) => number_text::general_text(*n),
            Self::Text(s) => s.clone(),
            Self::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            Self::Empty => String::new(),
            Self::Error(e) => format!("#{}", e.code()),
            Self::Array(values, _, _) => values
                .first()
                .map(|first| first.display())
                .unwrap_or_default(),
        }
    }
}

/// Calculation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalcError {
    /// Division by zero.
    DivZero,
    /// Value not available (reference to empty/error).
    NA,
    /// Invalid value type.
    Value,
    /// Name not recognized.
    Name,
    /// Formula references its own cell (cycle).
    Ref,
    /// Spill range blocked by content.
    Spill,
    /// Parse error.
    Parse,
    /// Invalid numeric argument.
    Num,
    /// An array calculation produced an empty array.
    Calc,
}

impl CalcError {
    /// Spreadsheet-style error code.
    pub fn code(self) -> &'static str {
        match self {
            Self::DivZero => "DIV/0!",
            Self::NA => "N/A",
            Self::Value => "VALUE!",
            Self::Name => "NAME?",
            Self::Ref => "REF!",
            Self::Spill => "SPILL!",
            Self::Parse => "PARSE!",
            Self::Num => "NUM!",
            Self::Calc => "CALC!",
        }
    }
}

/// A cell in the workbook.
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Cell {
    /// Raw input: a formula like `=A1+B2` or a literal.
    pub raw: String,
}

impl Cell {
    /// Is this a formula? (starts with `=`)
    pub fn is_formula(&self) -> bool {
        self.raw.starts_with('=')
    }
}

impl banded::StoredValue for Cell {
    fn is_formula(&self) -> bool {
        self.raw.starts_with('=')
    }

    fn heap_bytes(&self) -> usize {
        self.raw.capacity()
    }
}

/// A pending formula-bar edit whose workbook mutation happens only on commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellEditTransaction {
    original: Option<String>,
    draft: String,
}

impl CellEditTransaction {
    /// Start an edit from the selected cell's exact raw state.
    pub fn begin(original: Option<&str>) -> Self {
        let original = original.map(str::to_owned);
        let draft = original.clone().unwrap_or_default();
        Self { original, draft }
    }

    /// Replace the uncommitted text without changing the source cell.
    pub fn update(&mut self, draft: impl Into<String>) {
        self.draft = draft.into();
    }

    /// Commit the draft as one raw edit, or return `None` for an unchanged edit.
    pub fn commit(self) -> Option<RawCellEdit> {
        if self.original.as_deref() == Some(self.draft.as_str()) {
            return None;
        }
        Some(RawCellEdit {
            before: self.original,
            after: self.draft,
        })
    }

    /// Cancel the edit and return the exact original raw state.
    pub fn cancel(self) -> Option<String> {
        self.original
    }
}

/// The before/after raw values for one committed cell edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCellEdit {
    before: Option<String>,
    after: String,
}

impl RawCellEdit {
    /// Return the exact raw value before the edit, preserving absent cells.
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }

    /// Return the exact raw value written by the edit.
    pub fn after(&self) -> &str {
        &self.after
    }
}

/// A single worksheet.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// Cells keyed by coordinate. Row bands are shared copy-on-write, so
    /// cloning a sheet does not copy its cells.
    pub cells: BandedMap<Cell>,
    /// Alignment styles keyed by cell coordinate.
    pub alignments: BandedMap<CellAlignment>,
    /// Cell formatting styles keyed by cell coordinate.
    pub styles: BandedMap<CellStyle>,
    /// Sheet name.
    pub name: String,
    /// Frozen top rows count.
    pub freeze_rows: u32,
    /// Frozen left columns count.
    pub freeze_cols: u32,
    /// Custom column widths in pixels keyed by column index.
    pub col_widths: BTreeMap<u32, f32>,
    /// Custom row heights in pixels keyed by row index.
    pub row_heights: BTreeMap<u32, f32>,
    /// Embedded live-linked chart overlay, if the user inserted one.
    pub chart: Option<SheetChart>,
    /// Drawings and local image attachments anchored to worksheet cells.
    pub objects: Vec<SheetObject>,
}

impl Sheet {
    /// New sheet with a name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cells: BandedMap::new(),
            alignments: BandedMap::new(),
            styles: BandedMap::new(),
            freeze_rows: 0,
            freeze_cols: 0,
            col_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            chart: None,
            objects: Vec::new(),
        }
    }

    /// Sets custom column width for column index.
    pub fn set_col_width(&mut self, col: u32, width: f32) {
        if width.is_finite() && width > 0.0 {
            self.col_widths.insert(col, width);
        } else {
            self.col_widths.remove(&col);
        }
    }

    /// Gets column width for column index (defaulting to [`DEFAULT_COL_WIDTH`]).
    pub fn col_width(&self, col: u32) -> f32 {
        self.col_widths
            .get(&col)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH)
    }

    /// Sets custom row height for row index.
    pub fn set_row_height(&mut self, row: u32, height: f32) {
        if height.is_finite() && height > 0.0 {
            self.row_heights.insert(row, height);
        } else {
            self.row_heights.remove(&row);
        }
    }

    /// Gets row height for row index (defaulting to [`DEFAULT_ROW_HEIGHT`]).
    pub fn row_height(&self, row: u32) -> f32 {
        self.row_heights
            .get(&row)
            .copied()
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    /// Sets text alignment for a single cell.
    pub fn set_cell_alignment(&mut self, r: CellRef, alignment: CellAlignment) {
        if alignment == CellAlignment::General {
            self.alignments.remove(&r);
        } else {
            self.alignments.insert(r, alignment);
        }
    }

    /// Sets text alignment across a rectangular range of cells.
    pub fn set_range_alignment(&mut self, start: CellRef, end: CellRef, alignment: CellAlignment) {
        let min_col = start.col.min(end.col);
        let max_col = start.col.max(end.col);
        let min_row = start.row.min(end.row);
        let max_row = start.row.max(end.row);
        for col in min_col..=max_col {
            for row in min_row..=max_row {
                self.set_cell_alignment(CellRef { col, row }, alignment);
            }
        }
    }

    /// Gets text alignment for a cell (defaulting to General).
    pub fn cell_alignment(&self, r: CellRef) -> CellAlignment {
        self.alignments.get(&r).copied().unwrap_or_default()
    }

    /// Gets style for a cell (defaulting to unstyled).
    pub fn cell_style(&self, r: CellRef) -> CellStyle {
        self.styles.get(&r).copied().unwrap_or_default()
    }

    /// Sets style for a single cell.
    pub fn set_cell_style(&mut self, r: CellRef, style: CellStyle) {
        if style.is_default() {
            self.styles.remove(&r);
        } else {
            self.styles.insert(r, style);
        }
    }

    /// Sets style across a rectangular range of cells.
    pub fn set_range_style(&mut self, start: CellRef, end: CellRef, style: CellStyle) {
        let min_col = start.col.min(end.col);
        let max_col = start.col.max(end.col);
        let min_row = start.row.min(end.row);
        let max_row = start.row.max(end.row);
        for col in min_col..=max_col {
            for row in min_row..=max_row {
                self.set_cell_style(CellRef { col, row }, style);
            }
        }
    }

    /// Freeze a number of top rows and left columns.
    pub fn freeze_panes(&mut self, rows: u32, cols: u32) {
        self.freeze_rows = rows;
        self.freeze_cols = cols;
    }

    /// Unfreeze all panes.
    pub fn unfreeze_panes(&mut self) {
        self.freeze_rows = 0;
        self.freeze_cols = 0;
    }
}

/// Formats a numeric value as currency (e.g. "$1,234.50").
pub fn format_number_currency(value: f64, symbol: &str, decimals: usize) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let abs_val = value.abs();
    let int_part = abs_val.trunc() as u64;
    let frac_part = abs_val.fract();

    // Group integer part by thousands
    let int_str = int_part.to_string();
    let mut grouped = String::new();
    let chars: Vec<char> = int_str.chars().collect();
    let len = chars.len();
    for (i, c) in chars.into_iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }

    if decimals == 0 {
        format!("{sign}{symbol}{grouped}")
    } else {
        let frac_str = format!("{:.1$}", frac_part, decimals);
        let frac_formatted = frac_str.trim_start_matches("0.");
        format!("{sign}{symbol}{grouped}.{frac_formatted}")
    }
}

/// Formats a fractional numeric value as percentage (e.g. "25.0%").
pub fn format_number_percentage(value: f64, decimals: usize) -> String {
    format!("{:.1$}%", value * 100.0, decimals)
}

/// Standard spreadsheet cell numeric display formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NumberFormat {
    #[default]
    General,
    Currency,
    Percentage,
    Number,
    Scientific,
    DateIso,
    PlainText,
}

/// Applies a number format to a raw string or evaluated numerical result.
pub fn format_cell_display(raw: &str, format: NumberFormat) -> String {
    if raw.is_empty() {
        return String::new();
    }
    match format {
        NumberFormat::General | NumberFormat::PlainText => raw.to_string(),
        NumberFormat::Currency => {
            if let Ok(num) = raw.parse::<f64>() {
                format_number_currency(num, "$", 2)
            } else {
                raw.to_string()
            }
        }
        NumberFormat::Percentage => {
            if let Ok(num) = raw.parse::<f64>() {
                format_number_percentage(num, 1)
            } else {
                raw.to_string()
            }
        }
        NumberFormat::Number => {
            if let Ok(num) = raw.parse::<f64>() {
                format!("{num:.2}")
            } else {
                raw.to_string()
            }
        }
        NumberFormat::Scientific => {
            if let Ok(num) = raw.parse::<f64>() {
                format!("{:e}", num)
            } else {
                raw.to_string()
            }
        }
        NumberFormat::DateIso => raw.to_string(),
    }
}

/// Auto-fill series expansion modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillSeriesType {
    #[default]
    Linear,
    Growth,
    DateDays,
}

/// Generates a numeric progression series for drag-to-fill operations.
pub fn generate_fill_series(
    start: f64,
    step_or_factor: f64,
    count: usize,
    kind: FillSeriesType,
) -> Vec<f64> {
    let mut series = Vec::with_capacity(count);
    let mut current = start;

    for _ in 0..count {
        series.push(current);
        match kind {
            FillSeriesType::Linear | FillSeriesType::DateDays => current += step_or_factor,
            FillSeriesType::Growth => current *= step_or_factor,
        }
    }
    series
}

/// Sorts a 2D matrix of row cells by the specified column index.
pub fn sort_range_rows(rows: &[Vec<String>], col_idx: usize, ascending: bool) -> Vec<Vec<String>> {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|a, b| {
        let val_a = a.get(col_idx).map(|s| s.as_str()).unwrap_or("");
        let val_b = b.get(col_idx).map(|s| s.as_str()).unwrap_or("");

        // Attempt numeric comparison if both values parse as numbers
        let cmp = match (val_a.parse::<f64>(), val_b.parse::<f64>()) {
            (Ok(na), Ok(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
            _ => val_a.cmp(val_b),
        };

        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });
    sorted
}

/// Shifts cell references within a formula string — see [`refs`].
pub fn shift_formula_references(formula: &str, delta_cols: i32, delta_rows: i32) -> String {
    refs::shift_formula_references(formula, delta_cols, delta_rows)
}

/// Dependency graph tracking precedent and dependent relationships between cells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    /// Maps precedent cells to the set of dependent cells that rely on them.
    precedent_to_dependents: std::collections::HashMap<CellRef, Vec<CellRef>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a dependency: `dependent` cell relies on `precedent` cell.
    pub fn add_dependency(&mut self, dependent: CellRef, precedent: CellRef) {
        let dependents = self.precedent_to_dependents.entry(precedent).or_default();
        if !dependents.contains(&dependent) {
            dependents.push(dependent);
        }
    }

    /// Clears all dependencies where `dependent` was relying on precedents.
    pub fn remove_dependent(&mut self, dependent: &CellRef) {
        for dependents in self.precedent_to_dependents.values_mut() {
            dependents.retain(|d| d != dependent);
        }
    }

    /// Gets immediate dependents of a modified precedent cell.
    pub fn get_direct_dependents(&self, precedent: &CellRef) -> &[CellRef] {
        self.precedent_to_dependents
            .get(precedent)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Computes a topological recalculation order for dirty cells and all their transitive dependents.
    pub fn get_recalculation_order(&self, dirty_roots: &[CellRef]) -> Vec<CellRef> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        for root in dirty_roots {
            queue.push_back(*root);
        }

        while let Some(current) = queue.pop_front() {
            if visited.insert(current) {
                order.push(current);
                if let Some(dependents) = self.precedent_to_dependents.get(&current) {
                    for dep in dependents {
                        queue.push_back(*dep);
                    }
                }
            }
        }

        order
    }
}

/// Data validation criteria types for constraining cell input values.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationCriteria {
    List(Vec<String>),
    WholeNumberBetween(i64, i64),
    DecimalBetween(f64, f64),
    TextLengthBetween(usize, usize),
}

/// Data validation rule attached to a cell or range.
#[derive(Debug, Clone, PartialEq)]
pub struct DataValidationRule {
    pub criteria: ValidationCriteria,
    pub allow_blank: bool,
    pub error_message: String,
}

impl DataValidationRule {
    pub fn new(criteria: ValidationCriteria, error_message: impl Into<String>) -> Self {
        Self {
            criteria,
            allow_blank: true,
            error_message: error_message.into(),
        }
    }

    /// Validates a candidate input string against this rule.
    pub fn validate(&self, input: &str) -> Result<(), String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            if self.allow_blank {
                return Ok(());
            } else {
                return Err(self.error_message.clone());
            }
        }

        match &self.criteria {
            ValidationCriteria::List(items) => {
                if items.iter().any(|item| item.eq_ignore_ascii_case(trimmed)) {
                    Ok(())
                } else {
                    Err(self.error_message.clone())
                }
            }
            ValidationCriteria::WholeNumberBetween(min, max) => {
                if let Ok(n) = trimmed.parse::<i64>() {
                    if n >= *min && n <= *max {
                        Ok(())
                    } else {
                        Err(self.error_message.clone())
                    }
                } else {
                    Err(self.error_message.clone())
                }
            }
            ValidationCriteria::DecimalBetween(min, max) => {
                if let Ok(n) = trimmed.parse::<f64>() {
                    if n >= *min && n <= *max {
                        Ok(())
                    } else {
                        Err(self.error_message.clone())
                    }
                } else {
                    Err(self.error_message.clone())
                }
            }
            ValidationCriteria::TextLengthBetween(min, max) => {
                let len = trimmed.chars().count();
                if len >= *min && len <= *max {
                    Ok(())
                } else {
                    Err(self.error_message.clone())
                }
            }
        }
    }
}

/// Performs a vertical lookup (VLOOKUP) across the first column of a 2D table.
pub fn vlookup(
    lookup_value: &str,
    table: &[Vec<String>],
    col_index_1based: usize,
    exact_match: bool,
) -> Result<String, String> {
    if table.is_empty() || col_index_1based == 0 {
        return Err("table is empty or invalid column index".into());
    }
    let target_col = col_index_1based - 1;

    for row in table {
        if row.is_empty() {
            continue;
        }
        let first_cell = &row[0];
        let matches = if exact_match {
            first_cell.eq_ignore_ascii_case(lookup_value)
        } else {
            first_cell
                .to_lowercase()
                .contains(&lookup_value.to_lowercase())
        };

        if matches {
            if target_col < row.len() {
                return Ok(row[target_col].clone());
            } else {
                return Ok(String::new());
            }
        }
    }

    Err(format!("#N/A: value '{lookup_value}' not found in table"))
}

/// Performs a horizontal lookup (HLOOKUP) across the first row of a 2D table.
pub fn hlookup(
    lookup_value: &str,
    table: &[Vec<String>],
    row_index_1based: usize,
    exact_match: bool,
) -> Result<String, String> {
    if table.is_empty() || row_index_1based == 0 || row_index_1based > table.len() {
        return Err("table is empty or invalid row index".into());
    }
    let first_row = &table[0];
    let target_row_idx = row_index_1based - 1;

    for (col_idx, header) in first_row.iter().enumerate() {
        let matches = if exact_match {
            header.eq_ignore_ascii_case(lookup_value)
        } else {
            header.to_lowercase().contains(&lookup_value.to_lowercase())
        };

        if matches {
            let row = &table[target_row_idx];
            if col_idx < row.len() {
                return Ok(row[col_idx].clone());
            } else {
                return Ok(String::new());
            }
        }
    }

    Err(format!("#N/A: value '{lookup_value}' not found in row"))
}

/// Finds the 1-based relative position of a value within a 1D slice (MATCH).
pub fn match_lookup(
    lookup_value: &str,
    array: &[String],
    exact_match: bool,
) -> Result<usize, String> {
    for (idx, item) in array.iter().enumerate() {
        let matches = if exact_match {
            item.eq_ignore_ascii_case(lookup_value)
        } else {
            item.to_lowercase().contains(&lookup_value.to_lowercase())
        };

        if matches {
            return Ok(idx + 1);
        }
    }
    Err(format!("#N/A: item '{lookup_value}' not found"))
}

/// Retrieves a value at 1-based (row, col) coordinates from a 2D matrix (INDEX).
pub fn index_lookup(
    table: &[Vec<String>],
    row_1based: usize,
    col_1based: usize,
) -> Result<String, String> {
    if row_1based == 0 || col_1based == 0 {
        return Err("#VALUE!: row and col index must be >= 1".into());
    }
    let r = row_1based - 1;
    let c = col_1based - 1;

    if let Some(row) = table.get(r) {
        if let Some(val) = row.get(c) {
            return Ok(val.clone());
        }
    }
    Err("#REF!: cell coordinates out of range".into())
}

/// Concatenates multiple strings into one (CONCATENATE).
pub fn text_concatenate(parts: &[&str]) -> String {
    parts.concat()
}

/// Returns the leftmost `count` characters of a string (LEFT).
pub fn text_left(s: &str, count: usize) -> String {
    s.chars().take(count).collect()
}

/// Returns the rightmost `count` characters of a string (RIGHT).
pub fn text_right(s: &str, count: usize) -> String {
    let char_count = s.chars().count();
    let skip = char_count.saturating_sub(count);
    s.chars().skip(skip).collect()
}

/// Returns `count` characters from `start_1based` (MID).
pub fn text_mid(s: &str, start_1based: usize, count: usize) -> String {
    if start_1based == 0 {
        return String::new();
    }
    s.chars().skip(start_1based - 1).take(count).collect()
}

/// Returns the character length of a string (LEN).
pub fn text_len(s: &str) -> usize {
    s.chars().count()
}

/// Strips leading, trailing, and repeated whitespace (TRIM).
pub fn text_trim(s: &str) -> String {
    s.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Converts string to UPPERCASE.
pub fn text_upper(s: &str) -> String {
    s.to_uppercase()
}

/// Converts string to lowercase.
pub fn text_lower(s: &str) -> String {
    s.to_lowercase()
}

/// Capitalizes the first letter of each word (PROPER).
pub fn text_proper(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;

    for c in s.chars() {
        if c.is_alphabetic() {
            if capitalize_next {
                result.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                result.extend(c.to_lowercase());
            }
        } else {
            result.push(c);
            capitalize_next = true;
        }
    }
    result
}

/// Joins values with a delimiter, skipping empty strings when `skip_empty` is true (TEXTJOIN).
pub fn text_join(delimiter: &str, skip_empty: bool, values: &[String]) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(values.len());
    for v in values {
        if skip_empty && v.is_empty() {
            continue;
        }
        parts.push(v);
    }
    parts.join(delimiter)
}

/// Splits text on a delimiter into fields; consecutive delimiters produce empty fields.
/// An empty delimiter is an error.
pub fn split_text_to_columns(text: &str, delimiter: &str) -> Result<Vec<String>, String> {
    if delimiter.is_empty() {
        return Err("#VALUE!: delimiter must not be empty".into());
    }
    Ok(text.split(delimiter).map(|s| s.to_string()).collect())
}

/// Repeats a text value `n` times; zero repetitions yield an empty string.
pub fn text_repeat(value: &str, n: usize) -> String {
    value.repeat(n)
}

/// Substitutes occurrences of `old_text` with `new_text`. With `instance_num` of 0 every
/// occurrence is replaced; a value greater than zero replaces only that nth occurrence.
pub fn text_substitute(
    text: &str,
    old_text: &str,
    new_text: &str,
    case_sensitive: bool,
    instance_num: usize,
) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("#VALUE!: old_text must not be empty".into());
    }

    let matches: Vec<usize> = {
        let mut found = Vec::new();
        let (haystack, needle) = if case_sensitive {
            (text.to_string(), old_text.to_string())
        } else {
            (text.to_lowercase(), old_text.to_lowercase())
        };
        let mut offset = 0;
        while let Some(pos) = haystack[offset..].find(&needle) {
            found.push(offset + pos);
            offset += pos + needle.len();
        }
        found
    };

    if matches.is_empty() || instance_num > matches.len() {
        return Ok(text.to_string());
    }

    let selected: Vec<usize> = if instance_num == 0 {
        matches
    } else {
        vec![matches[instance_num - 1]]
    };

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;
    for start in selected {
        result.push_str(&text[last_end..start]);
        result.push_str(new_text);
        last_end = start + old_text.len();
    }
    result.push_str(&text[last_end..]);
    Ok(result)
}

/// Multiplies corresponding components in given arrays and returns the sum of those products (SUMPRODUCT).
pub fn sumproduct(arrays: &[&[f64]]) -> Result<f64, String> {
    if arrays.is_empty() {
        return Ok(0.0);
    }
    let len = arrays[0].len();
    for arr in &arrays[1..] {
        if arr.len() != len {
            return Err("#VALUE!: arrays must have equal dimensions".into());
        }
    }

    let mut total = 0.0;
    for i in 0..len {
        let mut prod = 1.0;
        for arr in arrays {
            prod *= arr[i];
        }
        total += prod;
    }
    Ok(total)
}

/// Sums elements in a slice that satisfy a condition (SUMIF).
pub fn sumif(range: &[f64], criteria_fn: impl Fn(f64) -> bool) -> f64 {
    range.iter().filter(|&&val| criteria_fn(val)).sum()
}

/// Counts elements in a slice that satisfy a condition (COUNTIF).
pub fn countif(range: &[f64], criteria_fn: impl Fn(f64) -> bool) -> usize {
    range.iter().filter(|&&val| criteria_fn(val)).count()
}

/// Calculates the average of elements in a slice that satisfy a condition (AVERAGEIF).
pub fn averageif(range: &[f64], criteria_fn: impl Fn(f64) -> bool) -> Option<f64> {
    let matching: Vec<f64> = range
        .iter()
        .copied()
        .filter(|&val| criteria_fn(val))
        .collect();
    if matching.is_empty() {
        None
    } else {
        Some(matching.iter().sum::<f64>() / matching.len() as f64)
    }
}

/// Calculates the payment for a loan based on constant payments and interest rate (PMT).
pub fn pmt(rate: f64, nper: f64, pv: f64, fv: f64, end_of_period: bool) -> Result<f64, String> {
    if nper == 0.0 {
        return Err("#NUM!: nper cannot be zero".into());
    }
    if rate == 0.0 {
        return Ok(-(pv + fv) / nper);
    }
    let pvif = (1.0 + rate).powf(nper);
    let type_factor = if end_of_period { 1.0 } else { 1.0 + rate };
    let pmt_val = -(rate * (fv + pv * pvif)) / (type_factor * (pvif - 1.0));
    Ok(pmt_val)
}

/// Calculates the future value of an investment based on constant periodic payments and interest rate (FV).
pub fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, end_of_period: bool) -> Result<f64, String> {
    if rate == 0.0 {
        return Ok(-(pv + pmt * nper));
    }
    let pvif = (1.0 + rate).powf(nper);
    let type_factor = if end_of_period { 1.0 } else { 1.0 + rate };
    let fv_val = -pv * pvif - (pmt * type_factor * (pvif - 1.0) / rate);
    Ok(fv_val)
}

/// Calculates the present value of an investment or loan (PV).
pub fn pv(rate: f64, nper: f64, pmt: f64, fv: f64, end_of_period: bool) -> Result<f64, String> {
    if rate == 0.0 {
        return Ok(-(fv + pmt * nper));
    }
    let pvif = (1.0 + rate).powf(nper);
    let type_factor = if end_of_period { 1.0 } else { 1.0 + rate };
    let pv_val = (-fv - (pmt * type_factor * (pvif - 1.0) / rate)) / pvif;
    Ok(pv_val)
}

/// Calculates the statistical mode (most frequently occurring number).
pub fn mode_single(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("MODE requires at least one value".into());
    }

    let mut counts: std::collections::BTreeMap<i64, (f64, usize)> =
        std::collections::BTreeMap::new();
    for &v in values {
        let key = (v * 1_000_000.0).round() as i64;
        counts.entry(key).and_modify(|e| e.1 += 1).or_insert((v, 1));
    }

    let mut max_count = 0;
    let mut mode_val = None;

    for (_, (val, count)) in counts {
        if count > max_count {
            max_count = count;
            mode_val = Some(val);
        }
    }

    if max_count > 1 {
        Ok(mode_val.unwrap())
    } else {
        Err("No duplicate values found for MODE".into())
    }
}

/// Calculates the population variance (VAR.P).
pub fn var_p(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("VAR.P requires at least one value".into());
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sum_sq_diff: f64 = values.iter().map(|&x| (x - mean).powi(2)).sum();
    Ok(sum_sq_diff / n)
}

/// Calculates the sample variance (VAR.S).
pub fn var_s(values: &[f64]) -> Result<f64, String> {
    if values.len() < 2 {
        return Err("VAR.S requires at least two values".into());
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sum_sq_diff: f64 = values.iter().map(|&x| (x - mean).powi(2)).sum();
    Ok(sum_sq_diff / (n - 1.0))
}

/// Calculates the population standard deviation (STDEV.P).
pub fn stdev_p(values: &[f64]) -> Result<f64, String> {
    var_p(values).map(|v| v.sqrt())
}

/// Calculates the sample standard deviation (STDEV.S).
pub fn stdev_s(values: &[f64]) -> Result<f64, String> {
    var_s(values).map(|v| v.sqrt())
}

/// Aggregation operations available to pivot table value fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PivotAggregation {
    #[default]
    Sum,
    Count,
    Average,
    Min,
    Max,
}

/// Groups row values by key and aggregates the paired values.
pub fn compute_pivot(
    keys: &[String],
    values: &[f64],
    aggregation: PivotAggregation,
) -> Result<Vec<(String, f64)>, String> {
    if keys.len() != values.len() {
        return Err("#VALUE!: keys and values must have equal lengths".into());
    }

    // BTreeMap keeps groups sorted by key so output ordering is deterministic.
    let mut groups: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for (key, value) in keys.iter().zip(values.iter()) {
        groups.entry(key.as_str()).or_default().push(*value);
    }

    let mut result = Vec::with_capacity(groups.len());
    for (key, group) in groups {
        let aggregated: f64 = match aggregation {
            PivotAggregation::Sum => group.iter().sum(),
            PivotAggregation::Count => group.len() as f64,
            PivotAggregation::Average => group.iter().sum::<f64>() / group.len() as f64,
            PivotAggregation::Min => group.iter().copied().fold(f64::INFINITY, f64::min),
            PivotAggregation::Max => group.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        };
        result.push((key.to_string(), aggregated));
    }
    Ok(result)
}

impl Sheet {
    /// Set a cell. Coordinates A1-style.
    pub fn set_str(&mut self, a1: &str, raw: &str) {
        if let Some(r) = CellRef::parse(a1) {
            self.set_raw(r, raw);
        }
    }

    /// Set a cell's exact raw input at a resolved cell reference.
    pub fn set_raw(&mut self, r: CellRef, raw: impl Into<String>) {
        self.cells.insert(r, Cell { raw: raw.into() });
    }

    /// Get raw content of a cell.
    pub fn raw(&self, r: CellRef) -> Option<&str> {
        self.cells.get(&r).map(|c| c.raw.as_str())
    }

    /// Clear a cell at coordinate.
    pub fn clear_cell(&mut self, r: CellRef) -> Option<String> {
        self.cells.remove(&r).map(|c| c.raw)
    }

    /// Clear all cells in a rectangular range.
    pub fn clear_range(&mut self, start: CellRef, end: CellRef) -> usize {
        let min_col = start.col.min(end.col);
        let max_col = start.col.max(end.col);
        let min_row = start.row.min(end.row);
        let max_row = start.row.max(end.row);
        let mut cleared = 0;
        for col in min_col..=max_col {
            for row in min_row..=max_row {
                if self.cells.remove(&CellRef { col, row }).is_some() {
                    cleared += 1;
                }
            }
        }
        cleared
    }

    /// Return bounding box of used cells: (min_col, min_row, max_col, max_row), if non-empty.
    pub fn used_range(&self) -> Option<(u32, u32, u32, u32)> {
        // The cell map keeps its bounds current on every write, so this costs
        // one step per row band rather than one step per cell.
        self.cells
            .extent()
            .map(|e| (e.min_col, e.min_row, e.max_col, e.max_row))
    }

    /// Return the sparse worksheet's addressable dimensions.
    ///
    /// A sheet with no content still exposes one editable cell (`A1`).  The
    /// dimensions are based on the furthest cell that is present, rather than
    /// on a fixed UI grid, so a sparse value such as `AZ1000` projects a
    /// 1,000-row by 52-column workbook without allocating the intervening
    /// cells.
    pub fn dimensions(&self) -> SheetDimensions {
        let (mut rows, mut cols) = self
            .used_range()
            .map(|(_, _, max_col, max_row)| (max_row.saturating_add(1), max_col.saturating_add(1)))
            .unwrap_or((1, 1));
        for object in &self.objects {
            rows = rows.max(object.anchor.row.saturating_add(1));
            cols = cols.max(object.anchor.col.saturating_add(1));
        }
        SheetDimensions::new(rows, cols)
    }
}

/// A workbook = multiple sheets (single sheet used by engine core for now).
#[derive(Debug, Clone, Default)]
pub struct Workbook {
    /// Sheets in order.
    pub sheets: Vec<Sheet>,
}

impl Workbook {
    /// New workbook with one empty sheet.
    pub fn with_sheet(name: &str) -> Self {
        Self {
            sheets: vec![Sheet::new(name)],
        }
    }

    /// Number of sheets in workbook.
    pub fn len(&self) -> usize {
        self.sheets.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    /// Get a sheet by index.
    pub fn sheet(&self, i: usize) -> Option<&Sheet> {
        self.sheets.get(i)
    }

    /// Get mutable reference to a sheet by index.
    pub fn sheet_mut(&mut self, i: usize) -> Option<&mut Sheet> {
        self.sheets.get_mut(i)
    }

    /// Add a new empty sheet.
    pub fn add_sheet(&mut self, name: &str) -> usize {
        self.sheets.push(Sheet::new(name));
        self.sheets.len() - 1
    }

    /// Remove a sheet by index if more than one sheet exists.
    pub fn remove_sheet(&mut self, index: usize) -> Result<(), String> {
        if self.sheets.len() <= 1 {
            return Err("workbook must contain at least one sheet".into());
        }
        if index >= self.sheets.len() {
            return Err(format!("sheet index {index} out of bounds"));
        }
        self.sheets.remove(index);
        Ok(())
    }

    /// Rename a sheet by index.
    pub fn rename_sheet(&mut self, index: usize, new_name: &str) -> Result<(), String> {
        let sheet = self
            .sheets
            .get_mut(index)
            .ok_or_else(|| format!("sheet index {index} out of bounds"))?;
        sheet.name = new_name.to_string();
        Ok(())
    }
}

/// Token types for the formula lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Cell(CellRef),
    Ident(String),
    /// Quoted sheet name for cross-sheet references (`'My Sheet'!A1`).
    SheetName(String),
    /// `!` separating a sheet name from a cell reference.
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Comma,
    Colon,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
    Amp,
    Percent,
    /// An error literal such as `#N/A`.
    Error(CalcError),
}

/// The error an Excel error literal (`#N/A`, `#REF!`, ...) names.
fn error_literal(text: &str) -> Option<CalcError> {
    Some(match text.to_ascii_uppercase().as_str() {
        "#DIV/0!" => CalcError::DivZero,
        "#N/A" => CalcError::NA,
        "#VALUE!" => CalcError::Value,
        "#NAME?" => CalcError::Name,
        "#REF!" => CalcError::Ref,
        "#NUM!" => CalcError::Num,
        _ => return None,
    })
}

fn lex(input: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Amp);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '!' => {
                tokens.push(Token::Bang);
                i += 1;
            }
            '\'' => {
                // Quoted sheet name for cross-sheet references: `'My Sheet'!A1`
                // with '' as an escaped quote. Only meaningful directly before
                // a `!`; anything else fails at parse time.
                i += 1;
                let mut name = String::new();
                let mut closed = false;
                while let Some(ch) = input[i..].chars().next() {
                    if ch == '\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            name.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        name.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed || name.is_empty() {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::SheetName(name));
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            // Absolute-reference marker: `$A$1` pins column/row during
            // fill/copy shifting (see `shift_formula_references`). The marker
            // carries no evaluation semantics, so only a `$` directly
            // attached to a cell coordinate is accepted here.
            '$' => {
                if i + 1 < bytes.len() && (bytes[i + 1] as char).is_ascii_alphabetic() {
                    i += 1;
                } else {
                    return Err(CalcError::Parse);
                }
            }
            '=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Eq);
                    i += 2;
                } else {
                    tokens.push(Token::Eq);
                    i += 1;
                }
            }
            '<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '#' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'/' | b'!' | b'?')))
                    .map_or(bytes.len(), |p| i + 1 + p);
                let literal = error_literal(&input[i..end]).ok_or(CalcError::Parse)?;
                tokens.push(Token::Error(literal));
                i = end;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                let mut closed = false;
                while let Some(ch) = input[i..].chars().next() {
                    if ch == '"' {
                        if bytes.get(i + 1) == Some(&b'"') {
                            s.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        s.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::String(s));
            }
            c if c.is_ascii_digit()
                || (c == '.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                let start = i;
                let mut saw_decimal = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' && !saw_decimal {
                        saw_decimal = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                i = scan_exponent(bytes, i);
                let num: f64 = input[start..i].parse().map_err(|_| CalcError::Parse)?;
                tokens.push(Token::Number(num));
            }
            c if c.is_ascii_alphabetic() => {
                // Could be a cell ref (e.g. A1, $A$1), function name, or bare name.
                let letters_start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                    i += 1;
                }
                let letters = &input[letters_start..i];
                // A name with digits, dots or underscores is a function when a
                // `(` follows (`LOG10(`, `STDEV.S(`) and a sheet qualifier when
                // a `!` does (`Sheet2!A1`); otherwise it may still be a cell.
                let mut word_end = i;
                while word_end < bytes.len()
                    && (bytes[word_end].is_ascii_alphanumeric()
                        || bytes[word_end] == b'_'
                        || bytes[word_end] == b'.')
                {
                    word_end += 1;
                }
                if word_end > i {
                    let after = input[word_end..].trim_start();
                    if after.starts_with('(') || input[word_end..].starts_with('!') {
                        tokens.push(Token::Ident(
                            input[letters_start..word_end].to_ascii_uppercase(),
                        ));
                        i = word_end;
                        continue;
                    }
                }
                // Optional row-absolute marker between column and row (`A$1`).
                if i + 1 < bytes.len()
                    && bytes[i] == b'$'
                    && (bytes[i + 1] as char).is_ascii_digit()
                {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    // A cell reference: letters followed by digits, e.g. A1 or AA10.
                    let num_start = i;
                    while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                    let full = format!("{letters}{}", &input[num_start..i]);
                    if let Some(cr) = CellRef::parse(&full) {
                        tokens.push(Token::Cell(cr));
                    } else {
                        return Err(CalcError::Name);
                    }
                } else {
                    tokens.push(Token::Ident(letters.to_ascii_uppercase()));
                }
            }
            _ => return Err(CalcError::Parse),
        }
    }
    Ok(tokens)
}

/// Extend a number token over an exponent (`E3`, `e-7`) when one follows.
fn scan_exponent(bytes: &[u8], i: usize) -> usize {
    if !bytes.get(i).is_some_and(|b| *b == b'E' || *b == b'e') {
        return i;
    }
    let mut j = i + 1;
    if bytes.get(j).is_some_and(|b| *b == b'+' || *b == b'-') {
        j += 1;
    }
    if !bytes.get(j).is_some_and(u8::is_ascii_digit) {
        return i;
    }
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
    }
    j
}

/// AST expression.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Number(f64),
    Text(String),
    Cell(CellRef),
    /// Cross-sheet cell reference (`Sheet2!A1`); sheet name as written.
    SheetCell {
        sheet: String,
        cell: CellRef,
    },
    /// Cross-sheet range (`Sheet2!A1:B2`).
    SheetRange {
        sheet: String,
        start: CellRef,
        end: CellRef,
    },
    Unary(Box<Expr>),
    Binary {
        lhs: Box<Expr>,
        op: BinOp,
        rhs: Box<Expr>,
    },
    Func {
        name: String,
        args: Vec<Expr>,
    },
    Range {
        start: CellRef,
        end: CellRef,
    },
    Bool(bool),
    /// An error literal typed into the formula.
    Error(CalcError),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
}

/// Parser produces an expression from tokens.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, t: &Token) -> Result<(), CalcError> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(CalcError::Parse)
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, CalcError> {
        // Handle comparison at top level.
        let mut lhs = self.parse_concat()?;
        // Comparisons chain left to right: `2>1>0` is `(2>1)>0`.
        while let Some(t) = self.peek() {
            let op = match t {
                Token::Eq => BinOp::Eq,
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,
                Token::Ne => BinOp::Ne,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_concat()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    /// `&` binds looser than `+`/`-` and tighter than comparisons.
    fn parse_concat(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_additive()?;
        while self.peek() == Some(&Token::Amp) {
            self.pos += 1;
            let rhs = self.parse_additive()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Concat,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => Some(BinOp::Add),
                Some(Token::Minus) => Some(BinOp::Sub),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_multiplicative()?;
                lhs = Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => Some(BinOp::Mul),
                Some(Token::Slash) => Some(BinOp::Div),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_unary()?;
                lhs = Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    /// Excel: unary minus binds tighter than `^` (`-2^2` is 4).
    fn parse_unary(&mut self) -> Result<Expr, CalcError> {
        self.parse_power()
    }

    fn parse_negated(&mut self) -> Result<Expr, CalcError> {
        if let Some(Token::Minus) = self.peek() {
            self.pos += 1;
            let inner = self.parse_negated()?;
            return Ok(Expr::Unary(Box::new(inner)));
        }
        let mut operand = self.parse_primary()?;
        // Postfix percent binds tighter than `^` and unary minus: `-50%` is -0.5.
        while let Some(Token::Percent) = self.peek() {
            self.pos += 1;
            operand = Expr::Binary {
                lhs: Box::new(operand),
                op: BinOp::Div,
                rhs: Box::new(Expr::Number(100.0)),
            };
        }
        Ok(operand)
    }

    fn parse_power(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_negated()?;
        while let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            let rhs = self.parse_negated()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Pow,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_primary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::String(s)) => Ok(Expr::Text(s)),
            Some(Token::Error(error)) => Ok(Expr::Error(error)),
            Some(Token::Cell(r)) => {
                // Check for range.
                if let Some(Token::Colon) = self.peek() {
                    self.pos += 1;
                    let end = match self.next() {
                        Some(Token::Cell(e)) => e,
                        _ => return Err(CalcError::Parse),
                    };
                    return Ok(Expr::Range { start: r, end });
                }
                Ok(Expr::Cell(r))
            }
            Some(Token::Ident(name)) => {
                // Cross-sheet reference: Name!A1 or Name!A1:B2.
                if let Some(Token::Bang) = self.peek() {
                    self.pos += 1;
                    return self.parse_sheet_ref(name);
                }
                // Function call or bare name.
                if let Some(Token::LParen) = self.peek() {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if let Some(Token::RParen) = self.peek() {
                        self.pos += 1;
                    } else {
                        loop {
                            let arg = self.parse_expr()?;
                            args.push(arg);
                            match self.next() {
                                Some(Token::Comma) => continue,
                                Some(Token::RParen) => break,
                                _ => return Err(CalcError::Parse),
                            }
                        }
                    }
                    Ok(Expr::Func { name, args })
                } else {
                    // Bare ident: could be TRUE/FALSE or named error.
                    match name.as_str() {
                        "TRUE" => Ok(Expr::Bool(true)),
                        "FALSE" => Ok(Expr::Bool(false)),
                        "NA" | "ERROR" => Err(CalcError::NA),
                        _ => Err(CalcError::Name),
                    }
                }
            }
            Some(Token::SheetName(name)) => {
                // Quoted cross-sheet reference: 'My Sheet'!A1.
                if let Some(Token::Bang) = self.peek() {
                    self.pos += 1;
                    return self.parse_sheet_ref(name);
                }
                Err(CalcError::Parse)
            }
            Some(Token::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            _ => Err(CalcError::Parse),
        }
    }

    /// Parse the cell or range half of a cross-sheet reference after the
    /// sheet name and `!` were consumed.
    fn parse_sheet_ref(&mut self, sheet: String) -> Result<Expr, CalcError> {
        let start = match self.next() {
            Some(Token::Cell(cell)) => cell,
            _ => return Err(CalcError::Parse),
        };
        if let Some(Token::Colon) = self.peek() {
            self.pos += 1;
            let end = match self.next() {
                Some(Token::Cell(cell)) => cell,
                _ => return Err(CalcError::Parse),
            };
            return Ok(Expr::SheetRange { sheet, start, end });
        }
        Ok(Expr::SheetCell { sheet, cell: start })
    }
}

/// A parsed formula ready for evaluation.
#[derive(Debug)]
pub struct Formula {
    pub(crate) root: Expr,
}

/// Parse a formula body (without leading `=`).
pub fn parse_formula(body: &str) -> Result<Formula, CalcError> {
    let tokens = lex(body)?;
    let mut p = Parser::new(tokens);
    let root = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(CalcError::Parse);
    }
    Ok(Formula { root })
}

/// Collect all cell references touched by an expression (for the dependency graph).
pub(crate) fn collect_refs(e: &Expr, out: &mut HashSet<CellRef>) {
    match e {
        Expr::Cell(r) => {
            out.insert(*r);
        }
        Expr::Range { start, end } => {
            for row in start.row.min(end.row)..=start.row.max(end.row) {
                for col in start.col.min(end.col)..=start.col.max(end.col) {
                    out.insert(CellRef { row, col });
                }
            }
        }
        Expr::Unary(x) => collect_refs(x, out),
        Expr::Binary { lhs, rhs, .. } => {
            collect_refs(lhs, out);
            collect_refs(rhs, out);
        }
        Expr::Func { args, .. } => {
            for a in args {
                collect_refs(a, out);
            }
        }
        _ => {}
    }
}

/// Evaluate an expression against a resolved-cell lookup.
pub(crate) fn eval_expr(e: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Value {
    match e {
        Expr::Number(n) => Value::Number(*n),
        Expr::Text(s) => Value::Text(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Error(error) => Value::Error(*error),
        Expr::Cell(r) => lookup(*r),
        // Cross-sheet nodes need workbook context (see `workbook::evaluate_workbook`).
        // In a single-sheet context they are unresolvable references.
        Expr::SheetCell { .. } | Expr::SheetRange { .. } => Value::Error(CalcError::Ref),
        Expr::Range { start, end: _ } => {
            // Range used directly where a scalar is expected -> take top-left.
            lookup(*start)
        }
        Expr::Unary(x) => match operand_value(x, lookup) {
            Value::Array(items, rows, cols) => {
                let negated = items.into_iter().map(negate).collect();
                collapse_array(negated, rows, cols)
            }
            scalar => negate(scalar),
        },
        Expr::Binary { lhs, op, rhs } => {
            let l = operand_value(lhs, lookup);
            let r = operand_value(rhs, lookup);
            if matches!(l, Value::Array(..)) || matches!(r, Value::Array(..)) {
                broadcast(&l, *op, &r)
            } else {
                eval_binary(&l, *op, &r)
            }
        }
        Expr::Func { name, args } => eval_function(name, args, lookup),
    }
}

/// An operand of an operator: a range becomes an array so operators apply
/// to every cell (`A1:A3*2`); anything else evaluates normally.
fn operand_value(expr: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Value {
    match expr {
        Expr::Range { start, end } => {
            let (top, bottom) = (start.row.min(end.row), start.row.max(end.row));
            let (left, right) = (start.col.min(end.col), start.col.max(end.col));
            let mut items = Vec::new();
            for row in top..=bottom {
                for col in left..=right {
                    items.push(lookup(CellRef { row, col }));
                }
            }
            Value::Array(
                items,
                (bottom - top + 1) as usize,
                (right - left + 1) as usize,
            )
        }
        other => eval_expr(other, lookup),
    }
}

fn negate(value: Value) -> Value {
    match functions_util::to_number(value) {
        Ok(n) => Value::Number(0.0 - n),
        Err(error) => Value::Error(error),
    }
}

/// A one-cell array is just its value.
fn collapse_array(mut items: Vec<Value>, rows: usize, cols: usize) -> Value {
    if items.len() == 1 {
        items.remove(0)
    } else {
        Value::Array(items, rows, cols)
    }
}

/// Apply an operator element by element. A one-row or one-column operand
/// repeats along the other's size; cells past a shorter operand are `#N/A`.
fn broadcast(left: &Value, op: BinOp, right: &Value) -> Value {
    fn shape(v: &Value) -> (&[Value], usize, usize) {
        match v {
            Value::Array(items, rows, cols) => (items.as_slice(), *rows, *cols),
            scalar => (std::slice::from_ref(scalar), 1, 1),
        }
    }
    let (litems, lrows, lcols) = shape(left);
    let (ritems, rrows, rcols) = shape(right);
    let extent = |a: usize, b: usize| {
        if a == b || b == 1 {
            a
        } else if a == 1 {
            b
        } else {
            a.max(b)
        }
    };
    let (rows, cols) = (extent(lrows, rrows), extent(lcols, rcols));
    let pick = |items: &[Value], r: usize, c: usize, nrows: usize, ncols: usize| {
        let (r, c) = (
            if nrows == 1 { 0 } else { r },
            if ncols == 1 { 0 } else { c },
        );
        if r < nrows && c < ncols {
            items[r * ncols + c].clone()
        } else {
            Value::Error(CalcError::NA)
        }
    };
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let l = pick(litems, r, c, lrows, lcols);
            let rt = pick(ritems, r, c, rrows, rcols);
            out.push(eval_binary(&l, op, &rt));
        }
    }
    collapse_array(out, rows, cols)
}

/// Numeric reading of a value in arithmetic: blanks are 0, `TRUE`/`FALSE` are
/// 1/0, numeric text converts; other text is `#VALUE!`.
fn num(v: &Value) -> Result<f64, CalcError> {
    functions_util::to_number(v.clone())
}

/// Text form of a value for `&`: Excel General number format (15 significant
/// digits), `TRUE`/`FALSE`, empty as "".
fn concat_text(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.clone()),
        Value::Number(n) => {
            let rounded: f64 = format!("{:.14e}", n).parse().unwrap_or(*n);
            Some(Value::Number(rounded).display())
        }
        Value::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.to_string()),
        Value::Empty => Some(String::new()),
        _ => None,
    }
}

/// Excel's ordering of two scalars: numbers < text < booleans, text without
/// regard to case, and a blank takes the type of the other side (0, "" or FALSE).
fn compare_scalars(l: &Value, r: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            _ => 0,
        }
    }
    match (l, r) {
        (Value::Array(..), _) | (_, Value::Array(..)) => None,
        // Excel compares numbers at 15 significant digits: 0.1+0.2 = 0.3.
        (Value::Number(a), Value::Number(b)) => {
            functions_util::clean(*a).partial_cmp(&functions_util::clean(*b))
        }
        (Value::Text(a), Value::Text(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Empty, Value::Empty) => Some(Ordering::Equal),
        (Value::Empty, other) => compare_scalars(&blank_like(other), other),
        (other, Value::Empty) => compare_scalars(other, &blank_like(other)),
        _ => Some(rank(l).cmp(&rank(r))),
    }
}

/// The blank cell as the type it is compared with.
fn blank_like(other: &Value) -> Value {
    match other {
        Value::Text(_) => Value::Text(String::new()),
        Value::Bool(_) => Value::Bool(false),
        _ => Value::Number(0.0),
    }
}

fn eval_binary(l: &Value, op: BinOp, r: &Value) -> Value {
    // Errors propagate unchanged so `#DIV/0!`, `#N/A`, and `#REF!` survive
    // through arithmetic instead of collapsing to a generic `#VALUE!`.
    if let Value::Error(_) = l {
        return l.clone();
    }
    if let Value::Error(_) = r {
        return r.clone();
    }
    if op == BinOp::Concat {
        return match (concat_text(l), concat_text(r)) {
            (Some(a), Some(b)) => Value::Text(a + &b),
            _ => Value::Error(CalcError::Value),
        };
    }
    // Comparison with text/bool/error semantics.
    if matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
    ) {
        let Some(order) = compare_scalars(l, r) else {
            return Value::Error(CalcError::Value);
        };
        return Value::Bool(match op {
            BinOp::Eq => order == std::cmp::Ordering::Equal,
            BinOp::Ne => order != std::cmp::Ordering::Equal,
            BinOp::Lt => order == std::cmp::Ordering::Less,
            BinOp::Gt => order == std::cmp::Ordering::Greater,
            BinOp::Le => order != std::cmp::Ordering::Greater,
            _ => order != std::cmp::Ordering::Less,
        });
    }
    let ln = match num(l) {
        Ok(n) => n,
        Err(_) => return Value::Error(CalcError::Value),
    };
    let rn = match num(r) {
        Ok(n) => n,
        Err(_) => return Value::Error(CalcError::Value),
    };
    match op {
        BinOp::Add => finite(ln + rn),
        BinOp::Sub => finite(ln - rn),
        BinOp::Mul => finite(ln * rn),
        BinOp::Div => {
            if rn == 0.0 {
                Value::Error(CalcError::DivZero)
            } else {
                finite(ln / rn)
            }
        }
        BinOp::Pow => match functions_util::excel_pow(ln, rn) {
            Ok(n) => Value::Number(n),
            Err(error) => Value::Error(error),
        },
        _ => Value::Error(CalcError::Value),
    }
}

/// A number result; overflow to infinity is `#NUM!` as in Excel.
fn finite(n: f64) -> Value {
    if n.is_finite() {
        Value::Number(n)
    } else {
        Value::Error(CalcError::Num)
    }
}

/// Push a value onto flattened function arguments, recursing into array
/// results so scalar aggregations compose over spills.
fn push_flattened(value: Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items, _, _) => {
            for item in items {
                push_flattened(item, out);
            }
        }
        scalar => out.push(scalar),
    }
}

fn eval_function(name: &str, args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Value {
    if let Some(res) = functions::eval_extended_function(name, args, lookup) {
        return res;
    }
    // Flatten each argument: ranges expand to individual cell lookups, and
    // array results flatten to their elements so aggregations compose.
    let mut values: Vec<Value> = Vec::new();
    for a in args {
        match a {
            Expr::Range { start, end } => {
                for row in start.row.min(end.row)..=start.row.max(end.row) {
                    for col in start.col.min(end.col)..=start.col.max(end.col) {
                        values.push(lookup(CellRef { row, col }));
                    }
                }
            }
            _ => push_flattened(eval_expr(a, lookup), &mut values),
        }
    }
    match name {
        "ABS" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => Value::Number(n.abs()),
                Err(error) => Value::Error(error),
            }
        }
        "CONCAT" | "CONCATENATE" => {
            let mut s = String::new();
            for v in &values {
                match v {
                    Value::Text(t) => s.push_str(t),
                    Value::Number(n) => s.push_str(&Value::Number(*n).display()),
                    Value::Bool(_) => s.push_str(&v.display()),
                    Value::Empty => {}
                    // Arguments flatten above, so arrays never arrive here.
                    Value::Array(_, _, _) => return Value::Error(CalcError::Value),
                    Value::Error(_) => return v.clone(),
                }
            }
            Value::Text(s)
        }
        "COUNTA" => {
            let mut count = 0.0;
            for v in &values {
                if *v != Value::Empty {
                    count += 1.0;
                }
            }
            Value::Number(count)
        }
        "SQRT" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => {
                    if n < 0.0 {
                        Value::Error(CalcError::Num)
                    } else {
                        Value::Number(n.sqrt())
                    }
                }
                Err(error) => Value::Error(error),
            }
        }
        "POWER" => {
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            match (num(&values[0]), num(&values[1])) {
                (Ok(base), Ok(exp)) => match functions_util::excel_pow(base, exp) {
                    Ok(n) => Value::Number(n),
                    Err(error) => Value::Error(error),
                },
                _ => Value::Error(CalcError::Value),
            }
        }
        "MOD" => {
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            match (num(&values[0]), num(&values[1])) {
                (Ok(n), Ok(d)) => {
                    if d == 0.0 {
                        Value::Error(CalcError::DivZero)
                    } else {
                        // Excel's result takes the sign of the divisor.
                        let r = n % d;
                        Value::Number(if r != 0.0 && (r < 0.0) != (d < 0.0) {
                            r + d
                        } else {
                            r
                        })
                    }
                }
                _ => Value::Error(CalcError::Value),
            }
        }
        "FLOOR" | "CEILING" => {
            // Excel requires the significance argument.
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            let (x, sig) = match (num(&values[0]), num(&values[1])) {
                (Ok(x), Ok(sig)) => (x, sig),
                _ => return Value::Error(CalcError::Value),
            };
            let floor = name == "FLOOR";
            if sig == 0.0 {
                return if floor {
                    Value::Error(CalcError::DivZero)
                } else {
                    Value::Number(0.0)
                };
            }
            if x > 0.0 && sig < 0.0 {
                return Value::Error(CalcError::Num);
            }
            // Clean binary noise first so FLOOR(0.3,0.1) is 0.3, not 0.2.
            let q = functions_util::clean(x / sig);
            Value::Number(if floor { q.floor() } else { q.ceil() } * sig)
        }
        _ => Value::Error(CalcError::Name),
    }
}

/// A resolved cell value plus its formula dependencies (for auditing).
#[derive(Debug, Clone)]
pub struct EvaluatedCell {
    /// Final value.
    pub value: Value,
}

/// Evaluate a sheet, returning a map of raw -> resolved value for every cell.
///
/// Shares the workbook evaluator (single-sheet workbook): identical ordering,
/// cycle, and error semantics; unqualified formulas behave exactly as before.
pub fn evaluate(sheet: &Sheet) -> HashMap<CellRef, Value> {
    workbook::evaluate_workbook(std::slice::from_ref(sheet))
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Parse a literal cell value (numbers, bools, text, empty).
pub fn parse_literal(s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        Value::Empty
    } else if let Ok(n) = s.parse::<f64>() {
        Value::Number(n)
    } else if s.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if s.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else {
        Value::Text(s.to_string())
    }
}

// Sheet JSON persistence lives in `persistence`; see
// `persistence::sheet_to_json` and `persistence::sheet_from_json`.
/// Inclusive rectangular cell range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRange {
    /// Top-left cell.
    pub start: CellRef,
    /// Bottom-right cell.
    pub end: CellRef,
}

impl CellRange {
    /// Creates a normalized range.
    pub fn new(a: CellRef, b: CellRef) -> Self {
        Self {
            start: CellRef {
                row: a.row.min(b.row),
                col: a.col.min(b.col),
            },
            end: CellRef {
                row: a.row.max(b.row),
                col: a.col.max(b.col),
            },
        }
    }

    /// Parses `A1:B4` or a single-cell `A1` range.
    pub fn parse(input: &str) -> Option<Self> {
        if let Some((left, right)) = input.split_once(':') {
            Some(Self::new(CellRef::parse(left)?, CellRef::parse(right)?))
        } else {
            let cell = CellRef::parse(input)?;
            Some(Self::new(cell, cell))
        }
    }

    /// Whether a cell is inside the range.
    pub fn contains(self, cell: CellRef) -> bool {
        cell.row >= self.start.row
            && cell.row <= self.end.row
            && cell.col >= self.start.col
            && cell.col <= self.end.col
    }

    /// Cells in row-major order.
    pub fn cells(self) -> Vec<CellRef> {
        let mut cells = Vec::new();
        for row in self.start.row..=self.end.row {
            for col in self.start.col..=self.end.col {
                cells.push(CellRef { row, col });
            }
        }
        cells
    }

    /// Renders A1 notation.
    pub fn to_a1(self) -> String {
        if self.start == self.end {
            self.start.to_a1()
        } else {
            format!("{}:{}", self.start.to_a1(), self.end.to_a1())
        }
    }
}

/// A worksheet selection that preserves the cell where the selection started
/// independently from the current keyboard/mouse focus cell.
///
/// Keeping both coordinates is important for Shift+arrow extension: the
/// normalized rectangle can move in either direction while the anchor remains
/// stable for the next extension or contraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridSelection {
    /// Cell where the selection began.
    pub anchor: CellRef,
    /// Cell currently receiving focus.
    pub focus: CellRef,
}

impl GridSelection {
    /// Creates a selection from an anchor and focus cell.
    pub const fn new(anchor: CellRef, focus: CellRef) -> Self {
        Self { anchor, focus }
    }

    /// Returns the normalized rectangular range represented by the selection.
    pub fn range(self) -> CellRange {
        CellRange::new(self.anchor, self.focus)
    }

    /// Returns whether `cell` is inside the selected rectangle.
    pub fn contains(self, cell: CellRef) -> bool {
        self.range().contains(cell)
    }

    /// Returns the A1 label shown in a name box or accessibility announcement.
    pub fn label(self) -> String {
        self.range().to_a1()
    }

    /// Moves focus while retaining the anchor (the Shift+arrow behavior).
    pub const fn extend(self, focus: CellRef) -> Self {
        Self {
            anchor: self.anchor,
            focus,
        }
    }

    /// Collapses the selection to a new active cell and uses it as the anchor.
    pub const fn collapse(self, cell: CellRef) -> Self {
        Self {
            anchor: cell,
            focus: cell,
        }
    }
}

/// Compatibility alias for callers that describe a grid selection as a range
/// selection.  Both names intentionally represent the same anchor/focus
/// semantics.
pub type RangeSelection = GridSelection;

/// A reversible sparse edit over a rectangular range.
///
/// Both the before and after maps retain `None` for absent cells.  That keeps
/// undo exact: reverting a fill/copy does not turn previously empty cells into
/// stored empty strings, which would incorrectly expand the worksheet's used
/// dimensions after reopening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeEdit {
    before: BTreeMap<CellRef, Option<String>>,
    after: BTreeMap<CellRef, Option<String>>,
}

impl RangeEdit {
    /// Builds an edit that replaces a single cell's raw value, preserving absent cells.
    pub fn replace(sheet: &Sheet, cell: CellRef, after: Option<String>) -> Self {
        let mut before = BTreeMap::new();
        let mut after_map = BTreeMap::new();
        before.insert(cell, sheet.raw(cell).map(str::to_owned));
        after_map.insert(cell, after);
        Self {
            before,
            after: after_map,
        }
    }

    /// Builds an edit that copies `source` to a destination top-left cell.
    pub fn copy(sheet: &Sheet, source: CellRange, destination: CellRef) -> Self {
        let source_values: BTreeMap<CellRef, Option<String>> = source
            .cells()
            .into_iter()
            .map(|cell| (cell, sheet.raw(cell).map(str::to_owned)))
            .collect();
        let row_delta = destination.row as i64 - source.start.row as i64;
        let col_delta = destination.col as i64 - source.start.col as i64;
        let mut before = BTreeMap::new();
        let mut after = BTreeMap::new();

        for source_cell in source.cells() {
            let target = CellRef {
                row: shifted_coordinate(source_cell.row, row_delta),
                col: shifted_coordinate(source_cell.col, col_delta),
            };
            before.insert(target, sheet.raw(target).map(str::to_owned));
            after.insert(
                target,
                shifted_raw(
                    source_values.get(&source_cell).cloned().flatten(),
                    col_delta,
                    row_delta,
                ),
            );
        }

        Self { before, after }
    }

    /// Builds an edit that repeats `source` over every cell in `target`.
    ///
    /// Source values repeat by row and column.  Relative formula references are
    /// shifted from the source cell to the destination cell; absolute anchors
    /// remain fixed, matching spreadsheet copy/fill semantics.
    pub fn fill(sheet: &Sheet, source: CellRange, target: CellRange) -> Self {
        let source_rows = source.end.row - source.start.row + 1;
        let source_cols = source.end.col - source.start.col + 1;
        let source_values: BTreeMap<CellRef, Option<String>> = source
            .cells()
            .into_iter()
            .map(|cell| (cell, sheet.raw(cell).map(str::to_owned)))
            .collect();
        let mut before = BTreeMap::new();
        let mut after = BTreeMap::new();

        for destination in target.cells() {
            let source_cell = CellRef {
                row: source.start.row + (destination.row - target.start.row) % source_rows,
                col: source.start.col + (destination.col - target.start.col) % source_cols,
            };
            let row_delta = destination.row as i64 - source_cell.row as i64;
            let col_delta = destination.col as i64 - source_cell.col as i64;
            before.insert(destination, sheet.raw(destination).map(str::to_owned));
            after.insert(
                destination,
                shifted_raw(
                    source_values.get(&source_cell).cloned().flatten(),
                    col_delta,
                    row_delta,
                ),
            );
        }

        Self { before, after }
    }

    /// Applies the edit to a worksheet.
    pub fn apply(&self, sheet: &mut Sheet) {
        apply_sparse_values(sheet, &self.after);
    }

    /// Reverts the edit to the exact sparse state captured at construction.
    pub fn revert(&self, sheet: &mut Sheet) {
        apply_sparse_values(sheet, &self.before);
    }

    /// Whether every target cell already has its requested value.
    pub fn is_noop(&self) -> bool {
        self.before == self.after
    }

    /// Approximate owned heap bytes retained by this edit.
    ///
    /// History uses this value for its byte budget. Counting string capacity
    /// (rather than only logical length) matches the memory released when the
    /// maps and their owned values are dropped.
    pub fn memory_bytes(&self) -> usize {
        fn map_bytes(map: &BTreeMap<CellRef, Option<String>>) -> usize {
            std::mem::size_of_val(map)
                + map
                    .values()
                    .map(|value| value.as_ref().map_or(0, String::capacity))
                    .sum::<usize>()
                + map.len() * std::mem::size_of::<(CellRef, Option<String>)>()
        }
        map_bytes(&self.before) + map_bytes(&self.after)
    }

    /// Number of target cells touched by this edit.
    pub fn len(&self) -> usize {
        self.after.len()
    }

    /// Whether no target cells are present.
    pub fn is_empty(&self) -> bool {
        self.after.is_empty()
    }
}

fn shifted_coordinate(value: u32, delta: i64) -> u32 {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs().min(u32::MAX as u64) as u32)
    } else {
        value.saturating_add(delta.min(u32::MAX as i64) as u32)
    }
}

fn shifted_raw(raw: Option<String>, col_delta: i64, row_delta: i64) -> Option<String> {
    raw.map(|value| {
        shift_formula_references(
            &value,
            col_delta.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            row_delta.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        )
    })
}

fn apply_sparse_values(sheet: &mut Sheet, values: &BTreeMap<CellRef, Option<String>>) {
    for (cell, raw) in values {
        match raw {
            Some(raw) => sheet.set_raw(*cell, raw.clone()),
            None => {
                sheet.clear_cell(*cell);
            }
        }
    }
}

/// Named range available to formulas and navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedRange {
    /// Case-insensitive stable name.
    pub name: String,
    /// Target cells.
    pub range: CellRange,
}

/// Cell validation rule.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationRule {
    /// Any value is valid.
    Any,
    /// Number within optional inclusive bounds.
    Number {
        /// Minimum value.
        min: Option<f64>,
        /// Maximum value.
        max: Option<f64>,
    },
    /// Text from a fixed set.
    List(Vec<String>),
    /// Non-empty value.
    Required,
}

impl ValidationRule {
    /// Checks a resolved value.
    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            ValidationRule::Any => true,
            ValidationRule::Number { min, max } => match value {
                Value::Number(number) => {
                    min.map(|min| *number >= min).unwrap_or(true)
                        && max.map(|max| *number <= max).unwrap_or(true)
                }
                _ => false,
            },
            ValidationRule::List(options) => {
                options.iter().any(|option| option == &value.display())
            }
            ValidationRule::Required => {
                !matches!(value, Value::Empty) && !value.display().is_empty()
            }
        }
    }
}

/// Simple conditional-format comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatCondition {
    /// Numeric value is above threshold.
    GreaterThan(f64),
    /// Numeric value is below threshold.
    LessThan(f64),
    /// Display text contains substring, case-insensitive.
    TextContains(String),
    /// Cell contains any error.
    IsError,
}

impl FormatCondition {
    /// Evaluates a condition.
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            FormatCondition::GreaterThan(threshold) => {
                matches!(value, Value::Number(number) if number > threshold)
            }
            FormatCondition::LessThan(threshold) => {
                matches!(value, Value::Number(number) if number < threshold)
            }
            FormatCondition::TextContains(query) => value
                .display()
                .to_lowercase()
                .contains(&query.to_lowercase()),
            FormatCondition::IsError => matches!(value, Value::Error(_)),
        }
    }
}

/// Conditional-format rule carrying a semantic style id.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalFormatRule {
    /// Target range.
    pub range: CellRange,
    /// Predicate.
    pub condition: FormatCondition,
    /// Stable style identifier resolved by the UI.
    pub style_id: String,
}

/// Row filter.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterPredicate {
    /// Keep every row.
    All,
    /// Display value contains text.
    Contains(String),
    /// Numeric value is at least threshold.
    NumberAtLeast(f64),
    /// Resolved value is non-empty.
    NonEmpty,
}

impl FilterPredicate {
    fn matches(&self, value: &Value) -> bool {
        match self {
            FilterPredicate::All => true,
            FilterPredicate::Contains(query) => value
                .display()
                .to_lowercase()
                .contains(&query.to_lowercase()),
            FilterPredicate::NumberAtLeast(threshold) => {
                matches!(value, Value::Number(number) if number >= threshold)
            }
            FilterPredicate::NonEmpty => !matches!(value, Value::Empty),
        }
    }
}

/// Higher-level worksheet features layered over the formula engine.
#[derive(Debug, Clone)]
pub struct SheetModel {
    /// Cell data.
    pub sheet: Sheet,
    /// Named ranges keyed by uppercase name.
    pub named_ranges: BTreeMap<String, CellRange>,
    /// Validation rules.
    pub validations: Vec<(CellRange, ValidationRule)>,
    /// Conditional formatting.
    pub conditional_formats: Vec<ConditionalFormatRule>,
    /// Hidden row indexes.
    pub hidden_rows: HashSet<u32>,
    /// Frozen row count.
    pub frozen_rows: u32,
    /// Frozen column count.
    pub frozen_columns: u32,
}

impl SheetModel {
    /// Wraps a sheet.
    pub fn new(sheet: Sheet) -> Self {
        Self {
            sheet,
            named_ranges: BTreeMap::new(),
            validations: Vec::new(),
            conditional_formats: Vec::new(),
            hidden_rows: HashSet::new(),
            frozen_rows: 0,
            frozen_columns: 0,
        }
    }

    /// Adds or replaces a named range.
    pub fn set_named_range(&mut self, name: &str, range: CellRange) -> Result<(), String> {
        if !valid_named_range(name) {
            return Err(format!("invalid named range {name:?}"));
        }
        self.named_ranges.insert(name.to_ascii_uppercase(), range);
        Ok(())
    }

    /// Returns a temporary sheet whose formulas have named ranges expanded to A1 syntax.
    pub fn resolved_sheet(&self) -> Sheet {
        let mut sheet = self.sheet.clone();
        // Without named ranges there is nothing to expand, so the shared cell
        // bands stay shared instead of being copied for an identical rewrite.
        if self.named_ranges.is_empty() {
            return sheet;
        }
        sheet.cells.for_each_value_mut(|cell| {
            if cell.is_formula() {
                cell.raw = expand_named_ranges(&cell.raw, &self.named_ranges);
            }
        });
        sheet
    }

    /// Evaluates formulas with named ranges.
    pub fn evaluate(&self) -> HashMap<CellRef, Value> {
        evaluate(&self.resolved_sheet())
    }

    /// Sets one cell only when all matching validation rules accept it.
    pub fn set_validated(&mut self, cell: CellRef, raw: &str) -> Result<(), String> {
        let mut preview = self.sheet.clone();
        preview.set_raw(cell, raw);
        let values = evaluate(&preview);
        let value = values.get(&cell).cloned().unwrap_or(Value::Empty);
        for (range, rule) in &self.validations {
            if range.contains(cell) && !rule.accepts(&value) {
                return Err(format!(
                    "value {:?} violates validation for {}",
                    value,
                    range.to_a1()
                ));
            }
        }
        self.sheet = preview;
        Ok(())
    }

    /// Style ids matching a cell's resolved value.
    pub fn conditional_style_ids(&self, cell: CellRef) -> Vec<&str> {
        let values = self.evaluate();
        let value = values.get(&cell).cloned().unwrap_or(Value::Empty);
        self.conditional_formats
            .iter()
            .filter(|rule| rule.range.contains(cell) && rule.condition.matches(&value))
            .map(|rule| rule.style_id.as_str())
            .collect()
    }

    /// Filters rows in `range` by a column relative to the range start.
    pub fn filter_rows(
        &mut self,
        range: CellRange,
        relative_column: u32,
        predicate: &FilterPredicate,
    ) -> Result<Vec<u32>, String> {
        let column = range
            .start
            .col
            .checked_add(relative_column)
            .ok_or_else(|| "filter column overflow".to_string())?;
        if column > range.end.col {
            return Err("filter column is outside the range".into());
        }
        let values = self.evaluate();
        let mut hidden = Vec::new();
        for row in range.start.row..=range.end.row {
            let value = values
                .get(&CellRef { row, col: column })
                .cloned()
                .unwrap_or(Value::Empty);
            if predicate.matches(&value) {
                self.hidden_rows.remove(&row);
            } else {
                self.hidden_rows.insert(row);
                hidden.push(row);
            }
        }
        Ok(hidden)
    }

    /// Sorts complete rows in a range by a relative column.
    pub fn sort_rows(
        &mut self,
        range: CellRange,
        relative_column: u32,
        ascending: bool,
    ) -> Result<(), String> {
        let values = self.evaluate();
        let order = RowOrder::sorted(range, relative_column, ascending, &values)?;
        order.apply(&mut self.sheet);
        Ok(())
    }
}

fn valid_named_range(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && CellRef::parse(name).is_none()
}

fn expand_named_ranges(formula: &str, ranges: &BTreeMap<String, CellRange>) -> String {
    let mut output = String::with_capacity(formula.len());
    let bytes = formula.as_bytes();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        let character = bytes[index] as char;
        if character == '"' {
            quoted = !quoted;
            output.push(character);
            index += 1;
            continue;
        }
        if !quoted && (character.is_ascii_alphabetic() || character == '_') {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                if next.is_ascii_alphanumeric() || next == '_' {
                    index += 1;
                } else {
                    break;
                }
            }
            let token = &formula[start..index];
            if let Some(range) = ranges.get(&token.to_ascii_uppercase()) {
                output.push_str(&range.to_a1());
            } else {
                output.push_str(token);
            }
        } else {
            output.push(character);
            index += 1;
        }
    }
    output
}

fn compare_values(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.total_cmp(right),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Empty, Value::Empty) => std::cmp::Ordering::Equal,
        (Value::Empty, _) => std::cmp::Ordering::Greater,
        (_, Value::Empty) => std::cmp::Ordering::Less,
        _ => left
            .display()
            .to_lowercase()
            .cmp(&right.display().to_lowercase()),
    }
}

/// Cached dependency graph and values for incremental recalculation.
#[derive(Debug, Clone, Default)]
pub struct CalculationCache {
    /// Last resolved values.
    pub values: HashMap<CellRef, Value>,
    dependencies: HashMap<CellRef, HashSet<CellRef>>,
    dependents: HashMap<CellRef, HashSet<CellRef>>,
}

impl CalculationCache {
    /// Performs a full calculation and dependency-graph rebuild.
    pub fn rebuild(&mut self, sheet: &Sheet) {
        self.values = evaluate(sheet);
        let (dependencies, dependents) = dependency_graph(sheet);
        self.dependencies = dependencies;
        self.dependents = dependents;
    }

    /// Recalculates changed cells and their transitive dependents.
    ///
    /// Formula parsing for the graph is linear in the sheet's formula count,
    /// while evaluation is restricted to the affected subgraph.
    pub fn recalculate(&mut self, sheet: &Sheet, changed: &[CellRef]) -> HashSet<CellRef> {
        let (dependencies, dependents) = dependency_graph(sheet);
        self.dependencies = dependencies;
        self.dependents = dependents;
        let mut affected: HashSet<CellRef> = changed.iter().copied().collect();
        let mut queue: Vec<CellRef> = changed.to_vec();
        while let Some(cell) = queue.pop() {
            if let Some(next) = self.dependents.get(&cell) {
                for dependent in next {
                    if affected.insert(*dependent) {
                        queue.push(*dependent);
                    }
                }
            }
        }
        for cell in &affected {
            self.values.remove(cell);
        }
        let mut remaining = affected.clone();
        let mut progress = true;
        while progress && !remaining.is_empty() {
            progress = false;
            let ready: Vec<CellRef> = remaining
                .iter()
                .copied()
                .filter(|cell| {
                    self.dependencies
                        .get(cell)
                        .map(|deps| deps.iter().all(|dep| !remaining.contains(dep)))
                        .unwrap_or(true)
                })
                .collect();
            for cell in ready {
                let value = match sheet.cells.get(&cell) {
                    None => Value::Empty,
                    Some(raw) if !raw.is_formula() => parse_literal(&raw.raw),
                    Some(raw) => match parse_formula(raw.raw[1..].trim()) {
                        Ok(formula) => {
                            let lookup = |reference: CellRef| {
                                self.values.get(&reference).cloned().unwrap_or(Value::Empty)
                            };
                            eval_expr(&formula.root, &lookup)
                        }
                        Err(error) => Value::Error(error),
                    },
                };
                self.values.insert(cell, value);
                remaining.remove(&cell);
                progress = true;
            }
        }
        for cell in remaining {
            self.values.insert(cell, Value::Error(CalcError::Ref));
        }
        affected
    }
}

fn dependency_graph(
    sheet: &Sheet,
) -> (
    HashMap<CellRef, HashSet<CellRef>>,
    HashMap<CellRef, HashSet<CellRef>>,
) {
    let mut dependencies = HashMap::new();
    let mut dependents: HashMap<CellRef, HashSet<CellRef>> = HashMap::new();
    for (cell_ref, cell) in &sheet.cells {
        if !cell.is_formula() {
            dependencies.insert(*cell_ref, HashSet::new());
            continue;
        }
        let mut refs = HashSet::new();
        if let Ok(formula) = parse_formula(cell.raw[1..].trim()) {
            collect_refs(&formula.root, &mut refs);
        }
        for dependency in &refs {
            dependents.entry(*dependency).or_default().insert(*cell_ref);
        }
        dependencies.insert(*cell_ref, refs);
    }
    (dependencies, dependents)
}

/// Solves f(x) = target for x within [lo, hi] using bisection. `f` must be continuous and
/// change sign across the bracket after accounting for the target (f(lo)-target and
/// f(hi)-target opposite signs). Iterates up to `max_iter` times or until the bracket width
/// < tolerance. Returns Ok(x) or Err describing no-sign-change or invalid inputs
/// (non-finite lo/hi/tolerance, hi <= lo, max_iter == 0).
pub fn goal_seek_bisection<F>(
    mut f: F,
    lo: f64,
    hi: f64,
    target: f64,
    tolerance: f64,
    max_iter: u32,
) -> Result<f64, String>
where
    F: FnMut(f64) -> f64,
{
    if !lo.is_finite() || !hi.is_finite() || !tolerance.is_finite() {
        return Err("goal seek requires finite lo, hi, and tolerance".to_string());
    }
    if hi <= lo {
        return Err(format!("goal seek requires hi > lo but got [{lo}, {hi}]"));
    }
    if max_iter == 0 {
        return Err("goal seek requires at least one iteration".to_string());
    }
    let g_lo = f(lo) - target;
    let g_hi = f(hi) - target;
    if !g_lo.is_finite() || !g_hi.is_finite() {
        return Err("goal seek produced non-finite values at the bracket ends".to_string());
    }
    if g_lo == 0.0 {
        return Ok(lo);
    }
    if g_hi == 0.0 {
        return Ok(hi);
    }
    if g_lo.signum() == g_hi.signum() {
        return Err(format!(
            "goal seek found no sign change across [{lo}, {hi}]: \
             g(lo) = {g_lo}, g(hi) = {g_hi}; the function may never reach {target}"
        ));
    }
    let mut a = lo;
    let mut b = hi;
    let mut g_a = g_lo;
    for _ in 0..max_iter {
        let mid = 0.5 * (a + b);
        let g_mid = f(mid) - target;
        if !g_mid.is_finite() {
            return Err(format!(
                "goal seek produced a non-finite value while evaluating inside [{a}, {b}]"
            ));
        }
        if g_mid == 0.0 {
            return Ok(mid);
        }
        if g_a.signum() == g_mid.signum() {
            a = mid;
            g_a = g_mid;
        } else {
            b = mid;
        }
        if b - a < tolerance {
            break;
        }
    }
    Ok(0.5 * (a + b))
}

/// Returns true for Gregorian leap years (divisible by 4, except centuries unless divisible
/// by 400).
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in a month (1..=12) for a given year. Month out of range => Err.
pub fn days_in_month(year: i32, month: u32) -> Result<u32, String> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Ok(31),
        4 | 6 | 9 | 11 => Ok(30),
        2 if is_leap_year(year) => Ok(29),
        2 => Ok(28),
        _ => Err(format!("month must be in 1..=12 but got {month}")),
    }
}

/// Converts a civil date to a day count relative to 1970-01-01 using Howard Hinnant's
/// `days_from_civil` algorithm (pure integer math, valid for any proleptic Gregorian date).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let m = i64::from(m);
    let y = i64::from(y) - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`] (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    ((y + i64::from(m <= 2)) as i32, m as u32, d as u32)
}

/// Validates that `month` is in 1..=12 and `day` fits that month for `year`.
fn validate_civil_date(year: i32, month: u32, day: u32) -> Result<(), String> {
    let last = days_in_month(year, month)?;
    if day == 0 || day > last {
        return Err(format!(
            "day must be in 1..={last} for {year}-{month:02} but got {day}"
        ));
    }
    Ok(())
}

/// Adds `days` (may be negative) to a civil date (year, month, day), normalizing across month
/// and year boundaries. Invalid input dates => Err.
pub fn add_days(year: i32, month: u32, day: u32, days: i64) -> Result<(i32, u32, u32), String> {
    validate_civil_date(year, month, day)?;
    let serial = days_from_civil(year, month, day);
    Ok(civil_from_days(serial + days))
}

/// Whole days between two civil dates (later - earlier keeps positive sign either direction).
pub fn days_between(y1: i32, m1: u32, d1: u32, y2: i32, m2: u32, d2: u32) -> Result<i64, String> {
    validate_civil_date(y1, m1, d1)?;
    validate_civil_date(y2, m2, d2)?;
    Ok(days_from_civil(y2, m2, d2) - days_from_civil(y1, m1, d1))
}

/// One recognized table cell from a Vision extraction.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrTableCell {
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub confidence: f32,
}

/// Converts recognized cells into a dense editable grid plus confidence grid. Row/column
/// indices must start at zero with no gaps (dense requirement) else Err naming the missing
/// coordinate. Duplicate coordinates err. Empty input errs.
#[allow(clippy::type_complexity)]
pub fn grid_from_ocr_table(
    cells: &[OcrTableCell],
) -> Result<(Vec<Vec<String>>, Vec<Vec<f32>>), String> {
    use std::collections::HashSet;
    if cells.is_empty() {
        return Err("table extraction produced no cells".to_string());
    }
    let mut coords: HashSet<(usize, usize)> = HashSet::new();
    let mut rows_seen: HashSet<usize> = HashSet::new();
    let mut cols_seen: HashSet<usize> = HashSet::new();
    let mut max_row = 0usize;
    let mut max_col = 0usize;
    for cell in cells {
        if !coords.insert((cell.row, cell.column)) {
            return Err(format!(
                "duplicate table cell at row {}, column {}",
                cell.row, cell.column
            ));
        }
        rows_seen.insert(cell.row);
        cols_seen.insert(cell.column);
        max_row = max_row.max(cell.row);
        max_col = max_col.max(cell.column);
    }
    for r in 0..=max_row {
        if !rows_seen.contains(&r) {
            return Err(format!(
                "dense grid requires every row in 0..={max_row} but row {r} is missing"
            ));
        }
    }
    for c in 0..=max_col {
        if !cols_seen.contains(&c) {
            return Err(format!(
                "dense grid requires every column in 0..={max_col} but column {c} is missing"
            ));
        }
    }
    let (n_rows, n_cols) = (max_row + 1, max_col + 1);
    let mut text = vec![vec![String::new(); n_cols]; n_rows];
    let mut confidence = vec![vec![0.0_f32; n_cols]; n_rows];
    for cell in cells {
        text[cell.row][cell.column] = cell.text.clone();
        confidence[cell.row][cell.column] = cell.confidence;
    }
    Ok((text, confidence))
}

/// FNV-1a 64-bit hash over bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Workbook {
    /// Stable integrity digest over every sheet's populated cells: iterates sheets
    /// in order, hashing each sheet's name, then every cell coordinate and raw value.
    /// Cells come from a [`BTreeMap`] keyed by [`CellRef`] (`Ord` on row then column),
    /// so iteration is already deterministic without sorting. Uses [`fnv1a64`].
    pub fn integrity_digest(&self) -> u64 {
        let mut input = format!("sheets:{}\n", self.sheets.len());
        for sheet in &self.sheets {
            input.push_str(&format!("sheet:{}\n", sheet.name));
            for (cell_ref, cell) in &sheet.cells {
                input.push_str(&format!(
                    "cell:{},{}:{}\n",
                    cell_ref.row, cell_ref.col, cell.raw
                ));
            }
        }
        fnv1a64(input.as_bytes())
    }
}

// ---- XLSX semantic cell extraction (READ_PARTIAL-class import) -----------

/// Upper bounds on worksheet coordinates, mirroring the OOXML sheet limits of
/// 1,048,576 rows by 16,384 columns. Cells outside them cannot occur in valid
/// files and are ignored so hostile archives cannot drive allocation.
const MAX_XLSX_ROWS: u32 = 1_048_576;
const MAX_XLSX_COLS: u32 = 16_384;

/// Dense grids above this many cells are rejected instead of allocated; a
/// sparse import path remains future work.
const MAX_XLSX_DENSE_CELLS: usize = 10_000_000;

/// Extracts the used range of the first worksheet from a .xlsx archive as a dense grid of
/// display strings. Reads `xl/sharedStrings.xml` (when present) into the shared-string
/// table, then `xl/worksheets/sheet1.xml`, resolving each `<c>` cell:
/// - t="s": <v> holds a shared-string index
/// - t="inlineStr": uses <is><t>...</t></is>
/// - otherwise: <v> holds the raw value (numbers, booleans as "1"/"0" -> "TRUE"/"FALSE")
///
/// Cells map by their `r="A1"` coordinate; gaps become empty strings. Err on unreadable
/// archives or missing sheet part.
///
/// This is a targeted scanner, not a full XML parser: shared strings are read by
/// concatenating every `<t>` run inside each `<si>` block, cells by matching `<c>`
/// elements. Only the five predefined XML entities are unescaped; CDATA sections and
/// namespaces are not interpreted. An out-of-range or malformed shared-string index
/// errs, as does a used range beyond [`MAX_XLSX_DENSE_CELLS`]. A sheet with no cells
/// yields an empty grid.
/// Exports a dense grid into a minimal valid `.xlsx` archive with
/// Excel-native value types: numbers write as numeric `<v>` cells, booleans
/// as `t="b"`, errors as `t="e"`, and text as shared strings. Empty strings
/// become cell gaps and fully empty rows are dropped (absence is not
/// content). Round-trips through [`extract_xlsx_grid`] for all populated rows.
pub fn export_xlsx_from_grid(grid: &[Vec<String>]) -> Result<Vec<u8>, String> {
    let sheet = XlsxSheetData {
        name: "Sheet1".to_string(),
        grid: grid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| XlsxCellData {
                        display: value.clone(),
                        formula: None,
                        number: value.parse::<f64>().ok(),
                        boolean: match value.as_str() {
                            "TRUE" => Some(true),
                            "FALSE" => Some(false),
                            _ => None,
                        },
                    })
                    .collect()
            })
            .collect(),
    };
    export_xlsx_workbook(std::slice::from_ref(&sheet))
}

/// One cell for XLSX export.
pub struct XlsxCellData {
    /// Displayed text (cached value for formula cells).
    pub display: String,
    /// Formula without the leading `=`; Loom syntax matches Excel for plain
    /// names, `$` absolutes, cross-sheet qualifiers, and common functions.
    pub formula: Option<String>,
    /// Native numeric cached value, when numeric.
    pub number: Option<f64>,
    /// Boolean cached value, when boolean.
    pub boolean: Option<bool>,
}

/// One worksheet for XLSX export: dense display grid with formulas.
pub struct XlsxSheetData {
    /// Desired tab name (sanitized to Excel rules on export).
    pub name: String,
    /// Dense rows of cells; empty displays without formulas become gaps.
    pub grid: Vec<Vec<XlsxCellData>>,
}

/// Dense worksheet values returned by the multi-sheet XLSX importer.
pub type XlsxWorkbookData = Vec<(String, Vec<Vec<String>>)>;

/// Build export cells for a sheet from evaluated values: display text plus
/// the raw formula and typed cached values underneath.
pub fn sheet_to_xlsx_data(
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
) -> XlsxSheetData {
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for cell in sheet.cells.keys() {
        max_row = max_row.max(cell.row);
        max_col = max_col.max(cell.col);
    }
    let mut grid = Vec::new();
    for row in 0..=max_row {
        let mut row_vec = Vec::new();
        for col in 0..=max_col {
            let cell = CellRef { row, col };
            let value = vals.get(&cell).cloned().unwrap_or(Value::Empty);
            let formula = sheet
                .raw(cell)
                .filter(|raw| raw.starts_with('='))
                .map(|raw| raw[1..].to_string());
            let (number, boolean) = match &value {
                Value::Number(n) if n.is_finite() => (Some(*n), None),
                Value::Bool(b) => (None, Some(*b)),
                _ => (None, None),
            };
            row_vec.push(XlsxCellData {
                display: value.display(),
                formula,
                number,
                boolean,
            });
        }
        grid.push(row_vec);
    }
    XlsxSheetData {
        name: sheet.name.clone(),
        grid,
    }
}

/// Sanitize a tab name to Excel sheet-name rules: strip `[]:*?/\`, `"`,
/// and control characters; trim spaces and edge apostrophes; truncate to 31
/// characters; and fall back to `Sheet{N}` when empty.
pub fn sanitize_xlsx_sheet_name(name: &str, fallback_index: usize) -> String {
    let mut clean: String = name
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\' | '"') && !c.is_control())
        .collect::<String>()
        .trim()
        .trim_matches('\'')
        .to_string();
    while clean.chars().count() > 31 {
        clean.pop();
    }
    if clean.is_empty() {
        format!("Sheet{}", fallback_index + 1)
    } else {
        clean
    }
}

/// Return valid, case-insensitively unique exported names and the mapping
/// used to rewrite formulas. Duplicate source names are ambiguous, so reject
/// them instead of silently pointing formulas at the wrong worksheet.
pub(crate) struct XlsxSheetNames {
    pub names: Vec<String>,
    pub formula_mapping: Vec<(String, String)>,
}

pub(crate) fn unique_xlsx_sheet_names(sheets: &[XlsxSheetData]) -> Result<XlsxSheetNames, String> {
    let mut source_names = HashSet::new();
    let mut used_names = HashSet::new();
    let mut names = Vec::with_capacity(sheets.len());
    let mut mapping = Vec::with_capacity(sheets.len());

    for (index, sheet) in sheets.iter().enumerate() {
        if !source_names.insert(sheet.name.to_ascii_lowercase()) {
            return Err(format!(
                "xlsx export cannot map duplicate sheet name {:?}",
                sheet.name
            ));
        }

        let base = sanitize_xlsx_sheet_name(&sheet.name, index);
        let mut candidate = base.clone();
        let mut suffix_number = 2usize;
        while used_names.contains(&candidate.to_ascii_lowercase()) {
            let suffix = format!(" ({suffix_number})");
            let prefix_chars = 31usize.saturating_sub(suffix.chars().count());
            let prefix: String = base.chars().take(prefix_chars).collect();
            candidate = format!("{prefix}{suffix}");
            suffix_number += 1;
        }

        used_names.insert(candidate.to_ascii_lowercase());
        mapping.push((sheet.name.clone(), candidate.clone()));
        names.push(candidate);
    }

    Ok(XlsxSheetNames {
        names,
        formula_mapping: mapping,
    })
}

/// Escape XML attribute text (element text plus both quote styles).
fn xml_escape_attr(value: &str) -> String {
    xml_escape_cell(value).replace('"', "&quot;")
}

/// Exports whole workbooks into a minimal valid `.xlsx` archive: one
/// worksheet part per tab (names sanitized), a global shared-string table,
/// native numeric/boolean/error cached values, and preserved formulas.
pub fn export_xlsx_workbook(sheets: &[XlsxSheetData]) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;

    if sheets.is_empty() {
        return Err("xlsx export needs at least one sheet".to_string());
    }
    let unique_names = unique_xlsx_sheet_names(sheets)?;
    let names = unique_names.names;
    let formula_sheet_names = unique_names.formula_mapping;

    // Shared strings carry text displays only; numbers, booleans, and errors
    // write inline, and error displays never enter the table.
    let mut table: BTreeMap<&str, usize> = BTreeMap::new();
    let mut ordered: Vec<&str> = Vec::new();
    for sheet in sheets {
        for row in &sheet.grid {
            for cell in row {
                if cell.number.is_some() || cell.boolean.is_some() {
                    continue;
                }
                if cell.display.is_empty() || cell.display.starts_with('#') {
                    continue;
                }
                if !table.contains_key(cell.display.as_str()) {
                    table.insert(&cell.display, ordered.len());
                    ordered.push(&cell.display);
                }
            }
        }
    }

    fn is_error_display(display: &str) -> bool {
        display.starts_with('#')
    }

    let column_letters = |mut col: usize| -> String {
        let mut letters = String::new();
        loop {
            letters.insert(0, (b'A' + (col % 26) as u8) as char);
            if col < 26 {
                break;
            }
            col = col / 26 - 1;
        }
        letters
    };

    let mut sheet_parts: Vec<String> = Vec::new();
    for sheet in sheets {
        let mut sheet_rows = String::new();
        for (row_index_zero_based, row) in sheet.grid.iter().enumerate() {
            if row
                .iter()
                .all(|cell| cell.display.is_empty() && cell.formula.is_none())
            {
                continue;
            }
            let mut cells = String::new();
            for (col_index, cell) in row.iter().enumerate() {
                if cell.display.is_empty() && cell.formula.is_none() {
                    continue;
                }
                let reference =
                    format!("{}{}", column_letters(col_index), row_index_zero_based + 1);
                let mut open = format!("<c r=\"{reference}\"");
                if cell.boolean.is_some() {
                    open.push_str(" t=\"b\"");
                } else if is_error_display(&cell.display) {
                    open.push_str(" t=\"e\"");
                } else if cell.number.is_none() && !cell.display.is_empty() {
                    open.push_str(" t=\"s\"");
                }
                open.push('>');
                cells.push_str(&open);
                if let Some(formula) = cell.formula.as_deref() {
                    let formula = refs::remap_sheet_references_in_formula(
                        &format!("={formula}"),
                        &formula_sheet_names,
                    );
                    let formula = formula.strip_prefix('=').unwrap_or(&formula);
                    cells.push_str(&format!("<f>{}</f>", xml_escape_cell(formula)));
                }
                if let Some(number) = cell.number {
                    cells.push_str(&format!("<v>{number}</v>"));
                } else if let Some(boolean) = cell.boolean {
                    cells.push_str(&format!("<v>{}</v>", if boolean { 1 } else { 0 }));
                } else if is_error_display(&cell.display) {
                    cells.push_str(&format!("<v>{}</v>", xml_escape_cell(&cell.display)));
                } else if !cell.display.is_empty() {
                    let index = table[cell.display.as_str()];
                    cells.push_str(&format!("<v>{index}</v>"));
                }
                cells.push_str("</c>");
            }
            let row_no = row_index_zero_based + 1;
            sheet_rows.push_str(&format!("<row r=\"{row_no}\">{cells}</row>"));
        }
        sheet_parts.push(format!(
            "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{sheet_rows}</sheetData></worksheet>"
        ));
    }

    let shared_strings: String = ordered
        .iter()
        .map(|s| {
            format!(
                "<si><t xml:space=\"preserve\">{}</t></si>",
                xml_escape_cell(s)
            )
        })
        .collect();

    let mut content_overrides = String::new();
    let mut workbook_sheets = String::new();
    let mut workbook_relationships = String::new();
    for (index, name) in names.iter().enumerate() {
        let part = index + 1;
        content_overrides.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{part}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
        workbook_sheets.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{part}\" r:id=\"rId{part}\"/>",
            xml_escape_attr(name)
        ));
        workbook_relationships.push_str(&format!("<Relationship Id=\"rId{part}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{part}.xml\"/>"));
    }
    let shared_id = sheets.len() + 1;
    workbook_relationships.push_str(&format!("<Relationship Id=\"rId{shared_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>"));

    let content_types = format!("<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>{content_overrides}<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>");
    let root_rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let workbook = format!("<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>{workbook_sheets}</sheets></workbook>");
    let workbook_rels = format!("<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{workbook_relationships}</Relationships>");
    let shared_xml = format!(
        "<?xml version=\"1.0\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{0}\" uniqueCount=\"{0}\">{shared_strings}</sst>",
        ordered.len()
    );

    let mut archive = PackageArchive::new();
    archive
        .add("[Content_Types].xml", content_types.into_bytes())
        .map_err(|e| format!("xlsx export failed: {e}"))?;
    archive
        .add("_rels/.rels", root_rels.as_bytes().to_vec())
        .map_err(|e| format!("xlsx export failed: {e}"))?;
    archive
        .add("xl/workbook.xml", workbook.into_bytes())
        .map_err(|e| format!("xlsx export failed: {e}"))?;
    archive
        .add("xl/_rels/workbook.xml.rels", workbook_rels.into_bytes())
        .map_err(|e| format!("xlsx export failed: {e}"))?;
    archive
        .add("xl/sharedStrings.xml", shared_xml.into_bytes())
        .map_err(|e| format!("xlsx export failed: {e}"))?;
    for (index, part) in sheet_parts.iter().enumerate() {
        archive
            .add(
                &format!("xl/worksheets/sheet{}.xml", index + 1),
                part.clone().into_bytes(),
            )
            .map_err(|e| format!("xlsx export failed: {e}"))?;
    }
    archive
        .to_bytes()
        .map_err(|e| format!("xlsx export failed: {e}"))
}

/// Escapes XML text content for spreadsheet cells.
fn xml_escape_cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn extract_xlsx_grid(xlsx_bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    let shared: Vec<String> = match archive.get("xl/sharedStrings.xml") {
        Some(bytes) => {
            let xml = std::str::from_utf8(bytes)
                .map_err(|_| "xl/sharedStrings.xml is not valid UTF-8".to_string())?;
            parse_shared_strings(xml)
        }
        None => Vec::new(),
    };
    let sheet_bytes = archive
        .get("xl/worksheets/sheet1.xml")
        .ok_or_else(|| "missing worksheet part xl/worksheets/sheet1.xml".to_string())?;
    let sheet_xml = std::str::from_utf8(sheet_bytes)
        .map_err(|_| "xl/worksheets/sheet1.xml is not valid UTF-8".to_string())?;
    extract_sheet_grid(sheet_xml, &shared)
}

/// Extract every worksheet from an `.xlsx` archive in workbook tab order.
/// Formula cells keep their formula text (with the leading `=`), while
/// ordinary cells retain the displayed value. This is intentionally a small
/// OOXML reader: it covers the worksheet/shared-string subset produced by
/// Loom and common spreadsheet exports without pretending to import charts,
/// pivots, or external links.
pub fn extract_xlsx_workbook(xlsx_bytes: &[u8]) -> Result<XlsxWorkbookData, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    let shared: Vec<String> = match archive.get("xl/sharedStrings.xml") {
        Some(bytes) => {
            let xml = std::str::from_utf8(bytes)
                .map_err(|_| "xl/sharedStrings.xml is not valid UTF-8".to_string())?;
            parse_shared_strings(xml)
        }
        None => Vec::new(),
    };
    // Keep the legacy sheet1-only scanner usable for small fixture archives
    // that contain a worksheet part but omit workbook metadata. Real Excel
    // files take the relationship-aware path below.
    let Some(workbook_bytes) = archive.get("xl/workbook.xml") else {
        let sheet_bytes = archive
            .get("xl/worksheets/sheet1.xml")
            .ok_or_else(|| "missing worksheet part xl/worksheets/sheet1.xml".to_string())?;
        let sheet_xml = std::str::from_utf8(sheet_bytes)
            .map_err(|_| "xl/worksheets/sheet1.xml is not valid UTF-8".to_string())?;
        return Ok(vec![(
            "Sheet1".to_string(),
            extract_sheet_grid_with_formulas(sheet_xml, &shared)?,
        )]);
    };
    let workbook_xml = std::str::from_utf8(workbook_bytes)
        .map_err(|_| "xl/workbook.xml is not valid UTF-8".to_string())?;
    let rels_bytes = archive
        .get("xl/_rels/workbook.xml.rels")
        .ok_or_else(|| "missing workbook relationships".to_string())?;
    let rels_xml = std::str::from_utf8(rels_bytes)
        .map_err(|_| "xl/_rels/workbook.xml.rels is not valid UTF-8".to_string())?;

    let mut relationships = BTreeMap::new();
    let mut rest = rels_xml;
    while let Some(offset) = next_tag_open(rest, "Relationship") {
        rest = &rest[offset..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let attrs = &rest[1..tag_end];
        if let (Some(id), Some(target)) = (
            attribute_value(attrs, "Id"),
            attribute_value(attrs, "Target"),
        ) {
            relationships.insert(id.to_string(), target.to_string());
        }
        rest = &rest[tag_end + 1..];
    }

    let mut sheets = Vec::new();
    let mut rest = workbook_xml;
    while let Some(offset) = next_tag_open(rest, "sheet") {
        rest = &rest[offset..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let attrs = &rest[1..tag_end];
        let Some(name) = attribute_value(attrs, "name").map(xml_unescape) else {
            rest = &rest[tag_end + 1..];
            continue;
        };
        let Some(rel_id) = attribute_value(attrs, "r:id") else {
            rest = &rest[tag_end + 1..];
            continue;
        };
        let Some(target) = relationships.get(rel_id) else {
            rest = &rest[tag_end + 1..];
            continue;
        };
        let target = normalize_xlsx_target(target);
        let Some(sheet_bytes) = archive.get(&target) else {
            return Err(format!("missing worksheet part {target}"));
        };
        let sheet_xml =
            std::str::from_utf8(sheet_bytes).map_err(|_| format!("{target} is not valid UTF-8"))?;
        sheets.push((name, extract_sheet_grid_with_formulas(sheet_xml, &shared)?));
        rest = &rest[tag_end + 1..];
    }
    Ok(sheets)
}

fn normalize_xlsx_target(target: &str) -> String {
    let target = target.trim_start_matches('/');
    if let Some(target) = target.strip_prefix("xl/") {
        format!("xl/{target}")
    } else if let Some(target) = target.strip_prefix("../") {
        format!("xl/{}", target.trim_start_matches("../"))
    } else {
        format!("xl/{target}")
    }
}

/// Walks `sheet_xml`, resolves every `<c>` against `shared`, and densifies the
/// used range up to the maximum row/column actually seen.
fn extract_sheet_grid(sheet_xml: &str, shared: &[String]) -> Result<Vec<Vec<String>>, String> {
    extract_sheet_grid_with_formulas_inner(sheet_xml, shared, false)
}

fn extract_sheet_grid_with_formulas(
    sheet_xml: &str,
    shared: &[String],
) -> Result<Vec<Vec<String>>, String> {
    extract_sheet_grid_with_formulas_inner(sheet_xml, shared, true)
}

/// Walks a worksheet part, optionally retaining `<f>` formula text instead of
/// returning its cached `<v>` display value.
fn extract_sheet_grid_with_formulas_inner(
    sheet_xml: &str,
    shared: &[String],
    keep_formulas: bool,
) -> Result<Vec<Vec<String>>, String> {
    #[derive(Debug)]
    struct ParsedFormula {
        shared_id: Option<u32>,
        body: Option<String>,
    }

    #[derive(Debug)]
    struct ParsedCell {
        row: u32,
        col: u32,
        cached: String,
        formula: Option<ParsedFormula>,
    }

    let mut parsed_cells = Vec::new();
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    let mut rest = sheet_xml;
    while let Some(offset) = next_tag_open(rest, "c") {
        rest = &rest[offset..];
        // Opening tag runs to the first '>' (attributes cannot contain one).
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let attrs = &rest[1..tag_end];
        let self_closing = attrs.ends_with('/');
        rest = &rest[tag_end + 1..];
        if self_closing {
            continue;
        }
        let Some(close_rel) = rest.find("</c>") else {
            break;
        };
        let body = &rest[..close_rel];
        rest = &rest[close_rel + 4..];

        // A cell without a usable coordinate cannot be placed; skip it.
        let Some((col, row)) = attribute_value(attrs, "r").and_then(parse_cell_coordinate) else {
            continue;
        };
        if row >= MAX_XLSX_ROWS || col >= MAX_XLSX_COLS {
            continue;
        }

        let cell_type = attribute_value(attrs, "t").unwrap_or("").to_string();
        let cached = match cell_type.as_str() {
            "s" => {
                let raw = first_element_text(body, "v")
                    .ok_or_else(|| "shared-string cell without <v>".to_string())?;
                let index: usize = raw
                    .trim()
                    .parse()
                    .map_err(|_| format!("shared-string index {raw:?} is not a number"))?;
                shared.get(index).cloned().ok_or_else(|| {
                    format!(
                        "shared-string index {index} out of range ({} entries)",
                        shared.len()
                    )
                })?
            }
            "inlineStr" => match first_element_block(body, "is") {
                Some(block) => rich_text(block),
                None => String::new(),
            },
            "b" => match first_element_text(body, "v") {
                Some(v) => match v.trim() {
                    "0" => "FALSE".to_string(),
                    "1" => "TRUE".to_string(),
                    other => other.to_string(),
                },
                None => String::new(),
            },
            _ => first_element_text(body, "v").unwrap_or_default(),
        };
        parsed_cells.push(ParsedCell {
            row,
            col,
            cached,
            formula: first_formula_descriptor(body)
                .map(|(shared_id, body)| ParsedFormula { shared_id, body }),
        });
        max_row = max_row.max(row);
        max_col = max_col.max(col);
    }

    if parsed_cells.is_empty() {
        return Ok(Vec::new());
    }

    // OOXML permits a shared-formula member to appear before its master in
    // the worksheet XML. Collect masters first so every member is resolved
    // from the worksheet-local `(sheet, si)` index in a second pass.
    let mut shared_formulas = crate::interop::SharedFormulaIndex::default();
    for cell in &parsed_cells {
        let Some(formula) = &cell.formula else {
            continue;
        };
        if let (Some(shared_id), Some(body)) = (formula.shared_id, formula.body.as_deref()) {
            shared_formulas.insert_master(
                shared_id,
                CellRef {
                    row: cell.row,
                    col: cell.col,
                },
                body,
            );
        }
    }

    let placed: Vec<(u32, u32, String)> = parsed_cells
        .into_iter()
        .map(|cell| {
            let text = if !keep_formulas {
                cell.cached
            } else if let Some(formula) = cell.formula {
                let reference = CellRef {
                    row: cell.row,
                    col: cell.col,
                };
                if let Some(shared_id) = formula.shared_id {
                    shared_formulas
                        .resolve(shared_id, reference)
                        .or_else(|| formula.body.map(|body| format!("={body}")))
                        .unwrap_or(cell.cached)
                } else {
                    formula
                        .body
                        .map(|body| format!("={body}"))
                        .unwrap_or(cell.cached)
                }
            } else {
                cell.cached
            };
            (cell.row, cell.col, text)
        })
        .collect();

    let n_rows = max_row as usize + 1;
    let n_cols = max_col as usize + 1;
    if n_rows.saturating_mul(n_cols) > MAX_XLSX_DENSE_CELLS {
        return Err(format!(
            "worksheet used range {n_rows}x{n_cols} exceeds dense import budget \
             ({MAX_XLSX_DENSE_CELLS} cells)"
        ));
    }
    let mut grid = vec![vec![String::new(); n_cols]; n_rows];
    for (row, col, text) in placed {
        grid[row as usize][col as usize] = text;
    }
    Ok(grid)
}

/// Read the first worksheet formula, retaining the shared-formula ID and
/// whether this cell carries the master formula body. XML text is unescaped
/// here so the formula shifter receives the actual spreadsheet expression.
fn first_formula_descriptor(body: &str) -> Option<(Option<u32>, Option<String>)> {
    let offset = next_tag_open(body, "f")?;
    let after_name = &body[offset + 1 + "f".len()..];
    let tag_end = after_name.find('>')?;
    let open_tag = &after_name[..tag_end];
    let shared_id = attribute_value(open_tag, "si").and_then(|raw| raw.parse::<u32>().ok());
    if open_tag.trim_end().ends_with('/') {
        return Some((shared_id, None));
    }
    let inner = &after_name[tag_end + 1..];
    let formula_end = inner.find("</f>")?;
    Some((shared_id, Some(xml_unescape(&inner[..formula_end]))))
}

/// Builds the shared-string table by concatenating the `<t>` runs of every
/// `<si>` block. Self-closing `<si/>` yields an empty entry so indices stay
/// aligned with the file's numbering.
fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut rest = xml;
    while let Some(offset) = next_tag_open(rest, "si") {
        rest = &rest[offset..];
        let after_name = &rest[1 + "si".len()..];
        if let Some(stripped) = after_name.strip_prefix('/') {
            match stripped.find('>') {
                Some(gt) => {
                    items.push(String::new());
                    rest = &stripped[gt + 1..];
                }
                None => break,
            }
            continue;
        }
        let Some(gt) = after_name.find('>') else {
            break;
        };
        let body = &after_name[gt + 1..];
        match body.find("</si>") {
            Some(end) => {
                items.push(rich_text(&body[..end]));
                rest = &body[end + "</si>".len()..];
            }
            None => break,
        }
    }
    items
}

/// Offset of the next opening tag named `tag`, requiring the name to be
/// followed by `>`, whitespace, or `/` so `<cols>` never matches `<c>`.
fn next_tag_open(xml: &str, tag: &str) -> Option<usize> {
    let mut from = 0usize;
    while let Some(rel) = xml[from..].find('<') {
        let abs = from + rel;
        let after = &xml[abs + 1..];
        if after.len() >= tag.len()
            && after.as_bytes()[..tag.len()] == tag.as_bytes()[..]
            && after[tag.len()..]
                .chars()
                .next()
                .is_some_and(|c| c == '>' || c == ' ' || c == '/')
        {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

/// Raw (still escaped) inner texts of every non-self-closing `tag` element in
/// document order.
fn element_texts<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let close = format!("</{tag}>");
    let mut texts = Vec::new();
    let mut rest = xml;
    while let Some(offset) = next_tag_open(rest, tag) {
        rest = &rest[offset..];
        let after_name = &rest[1 + tag.len()..];
        if let Some(stripped) = after_name.strip_prefix('/') {
            match stripped.find('>') {
                Some(gt) => rest = &stripped[gt + 1..],
                None => break,
            }
            continue;
        }
        let Some(gt) = after_name.find('>') else {
            break;
        };
        let body = &after_name[gt + 1..];
        match body.find(&close) {
            Some(end) => {
                texts.push(&body[..end]);
                rest = &body[end + close.len()..];
            }
            None => break,
        }
    }
    texts
}

/// Unescaped text of the first `tag` element in `xml`, if present.
fn first_element_text(xml: &str, tag: &str) -> Option<String> {
    element_texts(xml, tag).into_iter().next().map(xml_unescape)
}

/// Inner content of the first non-self-closing `tag` element in `xml`.
fn first_element_block<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let offset = next_tag_open(xml, tag)?;
    let after_name = &xml[offset + 1 + tag.len()..];
    if after_name.starts_with('/') {
        return None;
    }
    let gt = after_name.find('>')?;
    let body = &after_name[gt + 1..];
    let end = body.find(&format!("</{tag}>"))?;
    Some(&body[..end])
}

/// Concatenated, unescaped `<t>` run text of an `<si>` or `<is>` block.
fn rich_text(block: &str) -> String {
    let mut out = String::new();
    for raw in element_texts(block, "t") {
        out.push_str(&xml_unescape(raw));
    }
    out
}

/// Reads attribute `name` from an opening-tag fragment such as
/// `<c r="B2" t="s"`. The name must start at an attribute boundary and the
/// value must be quoted with `"` or `'`; anything else keeps scanning.
fn attribute_value<'a>(open_tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=");
    let mut from = 0usize;
    while from <= open_tag.len() {
        let rel = open_tag[from..].find(&needle)?;
        let abs = from + rel;
        let at_boundary = abs == 0
            || open_tag[..abs]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let tail = &open_tag[abs + needle.len()..];
        let quote = tail.chars().next();
        if at_boundary && matches!(quote, Some('"') | Some('\'')) {
            let q = quote.unwrap_or('"');
            let inner = &tail[1..];
            let end = inner.find(q)?;
            return Some(&inner[..end]);
        }
        from = abs + needle.len();
    }
    None
}

/// Parses an A1-style coordinate such as `AB12` into zero-based
/// (column, row). Rejects missing letters, zero rows, non-digit tails, and
/// values beyond `u32` range so hostile input cannot overflow.
fn parse_cell_coordinate(raw: &str) -> Option<(u32, u32)> {
    let coord = raw.trim();
    let bytes = coord.as_bytes();
    let mut col: u64 = 0;
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_alphabetic() {
        col = col
            .checked_mul(26)?
            .checked_add(u64::from(bytes[idx].to_ascii_uppercase() - b'A') + 1)?;
        idx += 1;
    }
    if idx == 0 || !coord[idx..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row: u64 = coord[idx..].parse().ok()?;
    if col == 0 || row == 0 || col > u64::from(u32::MAX) || row > u64::from(u32::MAX) {
        return None;
    }
    Some(((col - 1) as u32, (row - 1) as u32))
}

/// Unescapes the five predefined XML entities in a single left-to-right pass,
/// so `&amp;lt;` correctly decodes to the literal `&lt;`. Unknown escapes pass
/// through unchanged.
fn xml_unescape(text: &str) -> String {
    const ENTITIES: [(&str, &str); 5] = [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&apos;", "'"),
        ("&amp;", "&"),
    ];
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        match ENTITIES.iter().find(|(name, _)| tail.starts_with(name)) {
            Some((name, replacement)) => {
                out.push_str(replacement);
                rest = &tail[name.len()..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
