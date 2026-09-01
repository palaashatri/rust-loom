//! Loom Sheets formula engine and workbook model — headless and testable.
//!
//! The engine implements a tokenizer, a recursive-descent parser, cell
//! reference resolution, and a dependency-graph evaluator with topological
//! ordering and cycle detection. CSV import/export is included for
//! interoperability. The GUI (a documented follow-on) consumes this engine.

use std::collections::{BTreeMap, HashMap, HashSet};

use loom_package::zip::PackageArchive;
use serde::{Deserialize, Serialize};

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

impl CellRef {
    /// Parse an A1-style reference like "B3" (col letter(s), then row number).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_uppercase();
        let bytes = s.as_bytes();
        let mut col: u32 = 0;
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            col = col * 26 + (bytes[i] - b'A' + 1) as u32;
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
        if row == 0 {
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
}

impl Value {
    /// Display a value for export or console.
    pub fn display(&self) -> String {
        match self {
            Self::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Self::Text(s) => s.clone(),
            Self::Bool(b) => b.to_string(),
            Self::Empty => String::new(),
            Self::Error(e) => format!("#{}", e.code()),
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
    /// Parse error.
    Parse,
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
            Self::Parse => "PARSE!",
        }
    }
}

/// A cell in the workbook.
#[derive(Debug, Clone, PartialEq)]
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

/// Text alignment within a spreadsheet cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellAlignment {
    #[default]
    General,
    Left,
    Center,
    Right,
}

/// A single worksheet.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// Cells keyed by coordinate.
    pub cells: BTreeMap<CellRef, Cell>,
    /// Alignment styles keyed by cell coordinate.
    pub alignments: BTreeMap<CellRef, CellAlignment>,
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
}

impl Sheet {
    /// New sheet with a name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cells: BTreeMap::new(),
            alignments: BTreeMap::new(),
            freeze_rows: 0,
            freeze_cols: 0,
            col_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumberFormat {
    #[default]
    General,
    Currency,
    Percentage,
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

/// Shifts cell references within a formula string by `delta_cols` and `delta_rows`,
/// respecting absolute `$` anchors (e.g. `$A$1`, `A$1`, `$A1`, `A1`).
pub fn shift_formula_references(formula: &str, delta_cols: i32, delta_rows: i32) -> String {
    if !formula.starts_with('=') {
        return formula.to_string();
    }

    let mut result = String::with_capacity(formula.len());
    let chars: Vec<char> = formula.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for start of cell reference (optional '$' followed by letters then optional '$' then digits)
        let is_ref_start = if chars[i] == '$' {
            i + 1 < chars.len() && chars[i + 1].is_ascii_alphabetic()
        } else if chars[i].is_ascii_alphabetic() {
            // Must not be preceded by alphanumeric or underscore (which would make it a function name like SUM)
            !(i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_'))
        } else {
            false
        };

        if is_ref_start {
            let start = i;
            let mut col_abs = false;
            if chars[i] == '$' {
                col_abs = true;
                i += 1;
            }

            let col_start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let col_str: String = chars[col_start..i].iter().collect();

            let mut row_abs = false;
            if i < chars.len() && chars[i] == '$' {
                row_abs = true;
                i += 1;
            }

            let row_start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let row_str: String = chars[row_start..i].iter().collect();

            // Check if this formed a valid cell reference (e.g., has digits for row)
            if !col_str.is_empty() && !row_str.is_empty() {
                // Parse column index (0-based)
                let mut col_idx: u32 = 0;
                for c in col_str.to_ascii_uppercase().chars() {
                    col_idx = col_idx * 26 + (c as u32 - 'A' as u32 + 1);
                }
                let mut col_num = (col_idx - 1) as i32;

                // Parse row index (0-based)
                let mut row_num = row_str.parse::<i32>().unwrap_or(1) - 1;

                if !col_abs {
                    col_num = (col_num + delta_cols).max(0);
                }
                if !row_abs {
                    row_num = (row_num + delta_rows).max(0);
                }

                // Re-encode column string
                let mut new_col = String::new();
                let mut cn = col_num + 1;
                while cn > 0 {
                    let rem = ((cn - 1) % 26) as u8;
                    new_col.insert(0, (b'A' + rem) as char);
                    cn = (cn - 1) / 26;
                }

                if col_abs {
                    result.push('$');
                }
                result.push_str(&new_col);
                if row_abs {
                    result.push('$');
                }
                result.push_str(&(row_num + 1).to_string());
            } else {
                // Not a valid cell reference, push original matched substring
                for ch in &chars[start..i] {
                    result.push(*ch);
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
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
        if self.cells.is_empty() {
            return None;
        }
        let mut min_col = u32::MAX;
        let mut min_row = u32::MAX;
        let mut max_col = 0;
        let mut max_row = 0;
        for r in self.cells.keys() {
            min_col = min_col.min(r.col);
            min_row = min_row.min(r.row);
            max_col = max_col.max(r.col);
            max_row = max_row.max(r.row);
        }
        Some((min_col, min_row, max_col, max_row))
    }

    /// Return the sparse worksheet's addressable dimensions.
    ///
    /// A sheet with no content still exposes one editable cell (`A1`).  The
    /// dimensions are based on the furthest cell that is present, rather than
    /// on a fixed UI grid, so a sparse value such as `AZ1000` projects a
    /// 1,000-row by 52-column workbook without allocating the intervening
    /// cells.
    pub fn dimensions(&self) -> SheetDimensions {
        self.used_range()
            .map(|(_, _, max_col, max_row)| {
                SheetDimensions::new(max_row.saturating_add(1), max_col.saturating_add(1))
            })
            .unwrap_or_else(|| SheetDimensions::new(1, 1))
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
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
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
            '"' => {
                i += 1;
                let mut s = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch == '"' {
                        if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                            s.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        s.push(ch);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::String(s));
            }
            c if c.is_ascii_digit() => {
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
                let num: f64 = input[start..i].parse().map_err(|_| CalcError::Parse)?;
                tokens.push(Token::Number(num));
            }
            c if c.is_ascii_alphabetic() => {
                // Could be a cell ref (e.g. A1), function name, or bare name.
                let letters_start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                    i += 1;
                }
                let letters = &input[letters_start..i];
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

/// AST expression.
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Text(String),
    Cell(CellRef),
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
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
        let lhs = self.parse_additive()?;
        if let Some(t) = self.peek() {
            let op = match t {
                Token::Eq => Some(BinOp::Eq),
                Token::Lt => Some(BinOp::Lt),
                Token::Gt => Some(BinOp::Gt),
                Token::Le => Some(BinOp::Le),
                Token::Ge => Some(BinOp::Ge),
                Token::Ne => Some(BinOp::Ne),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_additive()?;
                return Ok(Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                });
            }
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

    fn parse_unary(&mut self) -> Result<Expr, CalcError> {
        if let Some(Token::Minus) = self.peek() {
            self.pos += 1;
            let inner = self.parse_unary()?;
            return Ok(Expr::Unary(Box::new(Expr::Number(-1.0).mul(inner))));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<Expr, CalcError> {
        let lhs = self.parse_primary()?;
        if let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            let rhs = self.parse_unary()?;
            return Ok(Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Pow,
                rhs: Box::new(rhs),
            });
        }
        Ok(lhs)
    }

    fn parse_primary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::String(s)) => Ok(Expr::Text(s)),
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
            Some(Token::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            _ => Err(CalcError::Parse),
        }
    }
}

impl Expr {
    /// Multiply helper (for unary minus construction).
    fn mul(self, rhs: Expr) -> Expr {
        Expr::Binary {
            lhs: Box::new(self),
            op: BinOp::Mul,
            rhs: Box::new(rhs),
        }
    }
}

/// A parsed formula ready for evaluation.
#[derive(Debug)]
pub struct Formula {
    root: Expr,
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
fn collect_refs(e: &Expr, out: &mut HashSet<CellRef>) {
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
fn eval_expr(e: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Value {
    match e {
        Expr::Number(n) => Value::Number(*n),
        Expr::Text(s) => Value::Text(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Cell(r) => lookup(*r),
        Expr::Range { start, end: _ } => {
            // Range used directly where a scalar is expected -> take top-left.
            lookup(*start)
        }
        Expr::Unary(x) => {
            let v = eval_expr(x, lookup);
            match v {
                Value::Number(n) => Value::Number(-n),
                _ => Value::Error(CalcError::Value),
            }
        }
        Expr::Binary { lhs, op, rhs } => {
            let l = eval_expr(lhs, lookup);
            let r = eval_expr(rhs, lookup);
            eval_binary(&l, *op, &r)
        }
        Expr::Func { name, args } => eval_function(name, args, lookup),
    }
}

fn num(v: &Value) -> Result<f64, CalcError> {
    match v {
        Value::Number(n) => Ok(*n),
        _ => Err(CalcError::Value),
    }
}

fn eval_binary(l: &Value, op: BinOp, r: &Value) -> Value {
    // Comparison with text/bool/error semantics.
    if matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
    ) {
        if let (Ok(ln), Ok(rn)) = (num(l), num(r)) {
            let b = match op {
                BinOp::Eq => ln == rn,
                BinOp::Ne => ln != rn,
                BinOp::Lt => ln < rn,
                BinOp::Gt => ln > rn,
                BinOp::Le => ln <= rn,
                BinOp::Ge => ln >= rn,
                _ => unreachable!(),
            };
            return Value::Bool(b);
        }
        match op {
            BinOp::Eq => return Value::Bool(l == r),
            BinOp::Ne => return Value::Bool(l != r),
            _ => return Value::Error(CalcError::Value),
        }
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
        BinOp::Add => Value::Number(ln + rn),
        BinOp::Sub => Value::Number(ln - rn),
        BinOp::Mul => Value::Number(ln * rn),
        BinOp::Div => {
            if rn == 0.0 {
                Value::Error(CalcError::DivZero)
            } else {
                Value::Number(ln / rn)
            }
        }
        BinOp::Pow => Value::Number(ln.powf(rn)),
        _ => Value::Error(CalcError::Value),
    }
}

fn eval_function(name: &str, args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Value {
    // Flatten each argument: ranges expand to individual cell lookups.
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
            _ => values.push(eval_expr(a, lookup)),
        }
    }
    match name {
        "SUM" => {
            let mut t = 0.0;
            for v in &values {
                match v {
                    Value::Number(n) => t += n,
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            Value::Number(t)
        }
        "AVERAGE" => {
            let mut t = 0.0;
            let mut n = 0.0;
            for v in &values {
                match v {
                    Value::Number(x) => {
                        t += x;
                        n += 1.0;
                    }
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if n == 0.0 {
                Value::Error(CalcError::DivZero)
            } else {
                Value::Number(t / n)
            }
        }
        "MIN" => {
            let mut best = f64::INFINITY;
            for v in &values {
                match v {
                    Value::Number(x) => best = best.min(*x),
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if best.is_infinite() {
                Value::Error(CalcError::NA)
            } else {
                Value::Number(best)
            }
        }
        "MAX" => {
            let mut best = f64::NEG_INFINITY;
            for v in &values {
                match v {
                    Value::Number(x) => best = best.max(*x),
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if best.is_infinite() {
                Value::Error(CalcError::NA)
            } else {
                Value::Number(best)
            }
        }
        "ABS" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => Value::Number(n.abs()),
                Err(_) => Value::Error(CalcError::Value),
            }
        }
        "ROUND" => {
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            match (num(&values[0]), num(&values[1])) {
                (Ok(n), Ok(d)) => {
                    let f = 10f64.powf(d);
                    Value::Number((n * f).round() / f)
                }
                _ => Value::Error(CalcError::Value),
            }
        }
        "CONCAT" => {
            let mut s = String::new();
            for v in &values {
                match v {
                    Value::Text(t) => s.push_str(t),
                    Value::Number(n) => s.push_str(&Value::Number(*n).display()),
                    Value::Bool(b) => s.push_str(&b.to_string()),
                    Value::Empty => {}
                    Value::Error(_) => return v.clone(),
                }
            }
            Value::Text(s)
        }
        "COUNT" => {
            let mut count = 0.0;
            for v in &values {
                if let Value::Number(_) = v {
                    count += 1.0;
                }
            }
            Value::Number(count)
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
        "IF" => {
            if values.len() < 2 || values.len() > 3 {
                return Value::Error(CalcError::Value);
            }
            let condition = match &values[0] {
                Value::Bool(b) => *b,
                Value::Number(n) => *n != 0.0,
                Value::Text(s) => !s.is_empty(),
                Value::Empty => false,
                Value::Error(_) => return values[0].clone(),
            };
            if condition {
                values[1].clone()
            } else if values.len() == 3 {
                values[2].clone()
            } else {
                Value::Bool(false)
            }
        }
        "AND" => {
            if values.is_empty() {
                return Value::Error(CalcError::Value);
            }
            for v in &values {
                match v {
                    Value::Bool(b) => {
                        if !b {
                            return Value::Bool(false);
                        }
                    }
                    Value::Number(n) => {
                        if *n == 0.0 {
                            return Value::Bool(false);
                        }
                    }
                    Value::Error(_) => return v.clone(),
                    _ => return Value::Error(CalcError::Value),
                }
            }
            Value::Bool(true)
        }
        "OR" => {
            if values.is_empty() {
                return Value::Error(CalcError::Value);
            }
            let mut any_true = false;
            for v in &values {
                match v {
                    Value::Bool(b) => {
                        if *b {
                            any_true = true;
                        }
                    }
                    Value::Number(n) => {
                        if *n != 0.0 {
                            any_true = true;
                        }
                    }
                    Value::Error(_) => return v.clone(),
                    _ => return Value::Error(CalcError::Value),
                }
            }
            Value::Bool(any_true)
        }
        "NOT" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match &values[0] {
                Value::Bool(b) => Value::Bool(!b),
                Value::Number(n) => Value::Bool(*n == 0.0),
                Value::Error(_) => values[0].clone(),
                _ => Value::Error(CalcError::Value),
            }
        }
        "SQRT" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => {
                    if n < 0.0 {
                        Value::Error(CalcError::Value)
                    } else {
                        Value::Number(n.sqrt())
                    }
                }
                Err(_) => Value::Error(CalcError::Value),
            }
        }
        "POWER" => {
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            match (num(&values[0]), num(&values[1])) {
                (Ok(base), Ok(exp)) => Value::Number(base.powf(exp)),
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
                        Value::Number(n % d)
                    }
                }
                _ => Value::Error(CalcError::Value),
            }
        }
        "FLOOR" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => Value::Number(n.floor()),
                Err(_) => Value::Error(CalcError::Value),
            }
        }
        "CEILING" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => Value::Number(n.ceil()),
                Err(_) => Value::Error(CalcError::Value),
            }
        }
        "MEDIAN" => {
            let mut numbers: Vec<f64> = Vec::new();
            for v in &values {
                match v {
                    Value::Number(x) => numbers.push(*x),
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if numbers.is_empty() {
                return Value::Error(CalcError::NA);
            }
            numbers.sort_by(|a, b| a.total_cmp(b));
            let mid = numbers.len() / 2;
            if numbers.len() % 2 == 0 {
                Value::Number((numbers[mid - 1] + numbers[mid]) / 2.0)
            } else {
                Value::Number(numbers[mid])
            }
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
pub fn evaluate(sheet: &Sheet) -> HashMap<CellRef, Value> {
    let mut result: HashMap<CellRef, Value> = HashMap::new();

    // First pass: literal cells (non-formula).
    let mut formula_cells: Vec<CellRef> = Vec::new();
    for (r, c) in &sheet.cells {
        if c.is_formula() {
            formula_cells.push(*r);
        } else {
            result.insert(*r, parse_literal(&c.raw));
        }
    }

    // Build dependency graph among formula cells only.
    let mut deps: HashMap<CellRef, HashSet<CellRef>> = HashMap::new();
    let mut parsed: HashMap<CellRef, Formula> = HashMap::new();
    for r in &formula_cells {
        let body = sheet.raw(*r).unwrap()[1..].trim();
        match parse_formula(body) {
            Ok(f) => {
                let mut refs = HashSet::new();
                collect_refs(&f.root, &mut refs);
                // Only keep refs that are formula cells (literal deps need no ordering).
                let formula_refs: HashSet<CellRef> = refs
                    .iter()
                    .copied()
                    .filter(|rr| sheet.cells.get(rr).map(|c| c.is_formula()).unwrap_or(false))
                    .collect();
                deps.insert(*r, formula_refs);
                parsed.insert(*r, f);
            }
            Err(e) => {
                result.insert(*r, Value::Error(e));
            }
        }
    }

    // Topological order (Kahn's algorithm) with cycle detection.
    let mut in_degree: HashMap<CellRef, usize> = HashMap::new();
    for r in &formula_cells {
        in_degree.insert(*r, deps.get(r).map(|d| d.len()).unwrap_or(0));
    }
    let mut ready: Vec<CellRef> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(r, _)| *r)
        .collect();
    // Deterministic order.
    ready.sort();

    let mut evaluated_count = 0usize;
    let visited: &mut HashSet<CellRef> = &mut HashSet::new();
    while let Some(r) = ready.first().copied() {
        ready.remove(0);
        if visited.contains(&r) {
            continue;
        }
        visited.insert(r);
        // Evaluate.
        if let Some(f) = parsed.get(&r) {
            let lookup = |cr: CellRef| -> Value {
                if let Some(v) = result.get(&cr) {
                    v.clone()
                } else {
                    Value::Empty
                }
            };
            result.insert(r, eval_expr(&f.root, &lookup));
        }
        evaluated_count += 1;
        // Decrease in-degree of dependents.
        let dependents: Vec<CellRef> = formula_cells
            .iter()
            .copied()
            .filter(|other| deps.get(other).map(|d| d.contains(&r)).unwrap_or(false))
            .collect();
        for d in dependents {
            if let Some(deg) = in_degree.get_mut(&d) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 && !visited.contains(&d) {
                    ready.push(d);
                }
            }
        }
        ready.sort();
    }

    // Cycles or incomplete -> mark remaining formula cells as REF error.
    let total_formulas = formula_cells.len();
    if evaluated_count < total_formulas {
        for r in &formula_cells {
            if !visited.contains(r) {
                result.insert(*r, Value::Error(CalcError::Ref));
            }
        }
    }

    result
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

/// Export a sheet to CSV.
pub fn to_csv(sheet: &Sheet) -> String {
    let vals = evaluate(sheet);
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for r in sheet.cells.keys() {
        max_row = max_row.max(r.row);
        max_col = max_col.max(r.col);
    }
    let mut out = String::new();
    for row in 0..=max_row {
        let mut line = Vec::new();
        for col in 0..=max_col {
            let cr = CellRef { row, col };
            let v = vals.get(&cr).cloned().unwrap_or(Value::Empty);
            line.push(csv_escape(&v.display()));
        }
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Robust RFC 4180 CSV parser supporting multiline quoted fields and configurable delimiters.
pub fn parse_csv_records(csv: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut current_row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = csv.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delimiter {
            current_row.push(field.clone());
            field.clear();
        } else if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            current_row.push(field.clone());
            field.clear();
            records.push(current_row.clone());
            current_row.clear();
        } else if c == '\n' {
            current_row.push(field.clone());
            field.clear();
            records.push(current_row.clone());
            current_row.clear();
        } else {
            field.push(c);
        }
    }

    if !field.is_empty() || !current_row.is_empty() {
        current_row.push(field);
        records.push(current_row);
    }

    records
}

/// A detected CSV dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvDialect {
    pub delimiter: char,
    /// True when fields were observed wrapped in double quotes.
    pub quoted: bool,
}

/// Sniffs the CSV dialect used by `sample`.
///
/// The delimiter is detected among `','`, `';'`, `'\t'`, and `'|'` by counting,
/// for each candidate, its occurrences outside double quotes in the first ten
/// non-empty lines. A candidate is *consistent* when every sampled line contains
/// the same number of occurrences, and the winner is the consistent candidate
/// with the highest count.
///
/// Exact tie-breaking: candidates are visited in the fixed order `','`, `';'`,
/// `'\t'`, `'|'`, and a candidate only replaces the current winner on a
/// strictly greater count, so ties always prefer the earlier candidate in that
/// list. When every candidate scores zero (or none is consistent) the dialect
/// defaults to `','`.
///
/// `quoted` is true when any sampled field starts and ends with `'"'` after
/// splitting on the detected delimiter. Empty input (no non-empty lines)
/// returns `Err`.
pub fn sniff_csv_dialect(sample: &str) -> Result<CsvDialect, String> {
    let mut lines = Vec::new();
    for line in sample.lines() {
        if !line.trim().is_empty() {
            lines.push(line);
            if lines.len() >= CSV_SNIFF_SAMPLE_LINES {
                break;
            }
        }
    }
    if lines.is_empty() {
        return Err("cannot sniff CSV dialect from empty input".to_string());
    }

    let csv_delimiter_candidates = [',', ';', '\t', '|'];
    let mut best_count = 0usize;
    let mut delimiter = ',';
    for candidate in csv_delimiter_candidates {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| {
                csv_fields_outside_quotes(line, candidate)
                    .len()
                    .saturating_sub(1)
            })
            .collect();
        let first = counts[0];
        let consistent = counts.iter().all(|&count| count == first);
        if consistent && first > best_count {
            best_count = first;
            delimiter = candidate;
        }
    }

    let quoted = lines.iter().any(|line| {
        csv_fields_outside_quotes(line, delimiter)
            .into_iter()
            .any(|field| field.len() > 1 && field.starts_with('"') && field.ends_with('"'))
    });

    Ok(CsvDialect { delimiter, quoted })
}

const CSV_SNIFF_SAMPLE_LINES: usize = 10;

/// Split `line` on `delimiter`, treating double-quoted spans as opaque, mirroring
/// the quoting conventions of [`parse_csv_records`]. Delimiters inside quotes are
/// not field boundaries, so the number of boundaries is `fields.len() - 1`.
fn csv_fields_outside_quotes(line: &str, delimiter: char) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut field_start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek().map(|&(_, next)| next) == Some('"') {
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delimiter {
            fields.push(&line[field_start..idx]);
            field_start = idx + c.len_utf8();
        }
    }
    fields.push(&line[field_start..]);
    fields
}

/// Import a CSV into a sheet.
pub fn from_csv(name: &str, csv: &str) -> Sheet {
    let mut sheet = Sheet::new(name);
    let records = parse_csv_records(csv, ',');
    for (row, fields) in records.iter().enumerate() {
        for (col, f) in fields.iter().enumerate() {
            let cr = CellRef {
                row: row as u32,
                col: col as u32,
            };
            sheet.cells.insert(cr, Cell { raw: f.clone() });
        }
    }
    sheet
}

/// Serialize a sheet to the `.loomtable` content JSON.
pub fn sheet_to_json(sheet: &Sheet) -> String {
    let mut s = String::with_capacity(128);
    s.push('{');
    s.push_str("\"name\":\"");
    s.push_str(&sheet.name.replace('"', "\\\""));
    s.push_str("\",\"cells\":[");
    let mut first = true;
    for (r, c) in &sheet.cells {
        if !first {
            s.push(',');
        }
        first = false;
        s.push('{');
        s.push_str("\"ref\":\"");
        s.push_str(&r.to_a1());
        s.push_str("\",\"raw\":\"");
        s.push_str(
            &c.raw
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n"),
        );
        s.push_str("\"}");
    }
    s.push(']');
    s.push_str(",\"col_widths\":{");
    let mut first_width = true;
    for (col, width) in &sheet.col_widths {
        if !first_width {
            s.push(',');
        }
        first_width = false;
        s.push('"');
        s.push_str(&col.to_string());
        s.push_str("\":");
        s.push_str(&width.to_string());
    }
    s.push_str("},\"row_heights\":{");
    let mut first_height = true;
    for (row, height) in &sheet.row_heights {
        if !first_height {
            s.push(',');
        }
        first_height = false;
        s.push('"');
        s.push_str(&row.to_string());
        s.push_str("\":");
        s.push_str(&height.to_string());
    }
    s.push('}');
    s.push('}');
    s
}

/// Parse sheet JSON back.
pub fn sheet_from_json(s: &str) -> Result<Sheet, String> {
    // Extract name and cells using a minimal parser (name is up to first "cells").
    let mut name = String::new();
    if let Some(prefix) = s.split("\"cells\"").next() {
        if let Some(n) = prefix.split("\"name\":\"").nth(1) {
            // Cut at the closing quote that precedes `,"cells"`.
            let end = n.find('"').unwrap_or(n.len());
            name = n[..end].replace("\\\"", "\"").replace("\\\\", "\\");
        }
    }
    let mut sheet = Sheet::new(&name);
    // Parse each {"ref":"A1","raw":"..."}.
    let body = s.split("\"cells\":").nth(1).unwrap_or("[]");
    for frag in body.split("{\"ref\":\"") {
        if frag.is_empty() {
            continue;
        }
        let Some(end) = frag.find("\",\"raw\":\"") else {
            continue;
        };
        let a1 = &frag[..end];
        let rest = &frag[end + "\",\"raw\":\"".len()..];
        let Some(end2) = rest.find("\"}") else {
            continue;
        };
        let raw = &rest[..end2];
        let raw = raw
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n");
        sheet.set_str(a1, &raw);
    }
    for (index, width) in parse_dimension_map(s, "col_widths") {
        sheet.set_col_width(index, width);
    }
    for (index, height) in parse_dimension_map(s, "row_heights") {
        sheet.set_row_height(index, height);
    }
    Ok(sheet)
}

/// Parse the optional numeric dimension maps emitted by [`sheet_to_json`].
///
/// Older workbook packages do not contain these fields, so a missing or
/// malformed map simply yields an empty result and leaves the model defaults
/// in place.  This intentionally small parser matches the hand-rolled sheet
/// serializer above while keeping backwards compatibility with existing
/// `.loomtable` content.
fn parse_dimension_map(s: &str, key: &str) -> BTreeMap<u32, f32> {
    let marker = format!("\"{key}\":{{");
    let Some(start) = s.find(&marker).map(|index| index + marker.len()) else {
        return BTreeMap::new();
    };
    let rest = &s[start..];
    let body = rest.split('}').next().unwrap_or_default();
    body.split(',')
        .filter_map(|entry| {
            let (raw_index, raw_value) = entry.split_once(':')?;
            let index = raw_index.trim().trim_matches('"').parse::<u32>().ok()?;
            let value = raw_value.trim().parse::<f32>().ok()?;
            value.is_finite().then_some((index, value))
        })
        .collect()
}

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
        for cell in sheet.cells.values_mut() {
            if cell.is_formula() {
                cell.raw = expand_named_ranges(&cell.raw, &self.named_ranges);
            }
        }
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
        let sort_column = range
            .start
            .col
            .checked_add(relative_column)
            .ok_or_else(|| "sort column overflow".to_string())?;
        if sort_column > range.end.col {
            return Err("sort column is outside the range".into());
        }
        let values = self.evaluate();
        let mut rows: Vec<u32> = (range.start.row..=range.end.row).collect();
        rows.sort_by(|left, right| {
            let left_value = values
                .get(&CellRef {
                    row: *left,
                    col: sort_column,
                })
                .cloned()
                .unwrap_or(Value::Empty);
            let right_value = values
                .get(&CellRef {
                    row: *right,
                    col: sort_column,
                })
                .cloned()
                .unwrap_or(Value::Empty);
            let ordering = compare_values(&left_value, &right_value);
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
        let original = self.sheet.cells.clone();
        for (destination_offset, source_row) in rows.into_iter().enumerate() {
            let destination_row = range.start.row + destination_offset as u32;
            for col in range.start.col..=range.end.col {
                let source = CellRef {
                    row: source_row,
                    col,
                };
                let destination = CellRef {
                    row: destination_row,
                    col,
                };
                match original.get(&source) {
                    Some(cell) => {
                        self.sheet.cells.insert(destination, cell.clone());
                    }
                    None => {
                        self.sheet.cells.remove(&destination);
                    }
                }
            }
        }
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

/// Chart type presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChartKind {
    #[default]
    Line,
    Bar,
    Pie,
    Scatter,
}

/// One data series: category/value pairs plus display metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartSeries {
    pub name: String,
    /// Category labels parallel to `values`; mismatched lengths must be rejected by validate.
    pub categories: Vec<String>,
    pub values: Vec<f64>,
}

/// A chart specification bound to sheet ranges conceptually; pure-data model with validation
/// and derived axis metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub title: String,
    pub series: Vec<ChartSeries>,
}

impl ChartSpec {
    /// Validates: non-empty title, at least one series, every series has equal-length
    /// categories/values and no NaN values. Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("chart title must not be empty".to_string());
        }
        if self.series.is_empty() {
            return Err("chart must contain at least one series".to_string());
        }
        for series in &self.series {
            if series.categories.len() != series.values.len() {
                return Err(format!(
                    "series '{}' has {} categories but {} values; lengths must match",
                    series.name,
                    series.categories.len(),
                    series.values.len()
                ));
            }
            for (index, value) in series.values.iter().enumerate() {
                if value.is_nan() {
                    return Err(format!(
                        "series '{}' contains a NaN value at index {}",
                        series.name, index
                    ));
                }
            }
        }
        Ok(())
    }

    /// (min, max) across all series values; Err when validation fails or no finite values.
    pub fn value_range(&self) -> Result<(f64, f64), String> {
        self.validate()?;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for series in &self.series {
            for &value in &series.values {
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
            }
        }
        if !min.is_finite() || !max.is_finite() {
            return Err("chart contains no finite values".to_string());
        }
        Ok((min, max))
    }

    /// Normalizes each value to 0..=1 against the computed range; Err conditions as above.
    pub fn normalized_points(&self) -> Result<Vec<Vec<f64>>, String> {
        let (min, max) = self.value_range()?;
        Ok(self
            .series
            .iter()
            .map(|series| {
                series
                    .values
                    .iter()
                    .map(|&value| {
                        if min == max {
                            1.0
                        } else {
                            (value - min) / (max - min)
                        }
                    })
                    .collect::<Vec<f64>>()
            })
            .collect())
    }
}

/// How a placed chart receives updates after being exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChartUpdatePolicy {
    /// Snapshot only; host never refreshes.
    #[default]
    StaticSnapshot,
    /// Host may re-read the bound range on demand.
    RefreshOnOpen,
}

/// Describes one chart exported to a host document, keeping the source range addressable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPlacement {
    pub chart_id: String,
    pub sheet_name: String,
    /// A1-style range feeding the series, e.g. "B2:D9".
    pub source_range: String,
    pub spec: ChartSpec,
    pub update_policy: ChartUpdatePolicy,
}

/// True when `part` is one corner of an A1 range: ASCII column letters followed by ASCII
/// row digits (case-insensitive), e.g. "B2" or "aa10".
fn is_a1_corner(part: &str) -> bool {
    let mut chars = part.chars();
    let mut saw_letter = false;
    let mut saw_digit = false;
    for character in chars.by_ref() {
        if character.is_ascii_alphabetic() {
            if saw_digit {
                return false;
            }
            saw_letter = true;
        } else if character.is_ascii_digit() {
            if !saw_letter {
                return false;
            }
            saw_digit = true;
        } else {
            return false;
        }
    }
    saw_letter && saw_digit
}

impl ChartPlacement {
    /// Validates: non-empty chart id and sheet name; non-empty source range matching
    /// loose A1 grammar of column letters + row digits on both sides of exactly one ':'
    /// (e.g. B2:D9, case-insensitive). Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.chart_id.trim().is_empty() {
            return Err("chart placement id must not be empty".to_string());
        }
        if self.sheet_name.trim().is_empty() {
            return Err("chart placement sheet name must not be empty".to_string());
        }
        if self.source_range.is_empty() {
            return Err("chart placement source range must not be empty".to_string());
        }
        let corners: Vec<&str> = self.source_range.split(':').collect();
        if corners.len() != 2 {
            return Err(format!(
                "source range '{}' must contain exactly one ':' separator",
                self.source_range
            ));
        }
        for corner in corners {
            if !is_a1_corner(corner) {
                return Err(format!(
                    "source range corner '{}' must be column letters followed by row digits",
                    corner
                ));
            }
        }
        Ok(())
    }

    /// True when two placements would collide in a host document: same chart_id, or
    /// identical sheet_name+source_range while at least one side updates non-statically.
    pub fn collides_with(&self, other: &ChartPlacement) -> bool {
        if self.chart_id == other.chart_id {
            return true;
        }
        self.sheet_name == other.sheet_name
            && self.source_range == other.source_range
            && (self.update_policy != ChartUpdatePolicy::StaticSnapshot
                || other.update_policy != ChartUpdatePolicy::StaticSnapshot)
    }
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
/// Exports a dense grid into a minimal valid `.xlsx` archive. Every cell is written as a
/// shared-string reference (numbers export in their display form); empty strings become
/// cell gaps and fully empty rows are dropped (absence is not content). Round-trips
/// losslessly through [`extract_xlsx_grid`] for all populated rows.
pub fn export_xlsx_from_grid(grid: &[Vec<String>]) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;

    let mut table: BTreeMap<&str, usize> = BTreeMap::new();
    let mut ordered: Vec<&str> = Vec::new();
    for row in grid {
        for value in row {
            if !value.is_empty() && !table.contains_key(value.as_str()) {
                table.insert(value, ordered.len());
                ordered.push(value);
            }
        }
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

    let mut sheet_rows = String::new();
    for (row_index_zero_based, row) in grid.iter().enumerate() {
        if row.iter().all(|value| value.is_empty()) {
            continue;
        }
        let row_index = row_index_zero_based;
        let mut cells = String::new();
        for (col_index, value) in row.iter().enumerate() {
            if value.is_empty() {
                continue;
            }
            let index = table[value.as_str()];
            let letter = column_letters(col_index);
            let row_no = row_index + 1;
            cells.push_str(&format!(
                "<c r=\"{letter}{row_no}\" t=\"s\"><v>{index}</v></c>"
            ));
        }
        let row_no = row_index + 1;
        sheet_rows.push_str(&format!("<row r=\"{row_no}\">{cells}</row>"));
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

    let content_types = "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>";
    let root_rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let workbook = "<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    let workbook_rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>";
    let shared_xml = format!(
        "<?xml version=\"1.0\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{0}\" uniqueCount=\"{0}\">{shared_strings}</sst>",
        ordered.len()
    );
    let sheet_xml = format!(
        "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{sheet_rows}</sheetData></worksheet>"
    );

    let parts: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", root_rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook.as_bytes().to_vec()),
        (
            "xl/_rels/workbook.xml.rels",
            workbook_rels.as_bytes().to_vec(),
        ),
        ("xl/sharedStrings.xml", shared_xml.into_bytes()),
        ("xl/worksheets/sheet1.xml", sheet_xml.into_bytes()),
    ];

    let mut archive = PackageArchive::new();
    for (path, data) in &parts {
        archive
            .add(path, data.clone())
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

/// Walks `sheet_xml`, resolves every `<c>` against `shared`, and densifies the
/// used range up to the maximum row/column actually seen.
fn extract_sheet_grid(sheet_xml: &str, shared: &[String]) -> Result<Vec<Vec<String>>, String> {
    let mut placed: Vec<(u32, u32, String)> = Vec::new();
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

        let cell_type = attribute_value(attrs, "t").unwrap_or("");
        let text = match cell_type {
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
        placed.push((row, col, text));
        max_row = max_row.max(row);
        max_col = max_col.max(col);
    }

    if placed.is_empty() {
        return Ok(Vec::new());
    }
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
mod tests {
    use super::*;

    #[test]
    fn workbook_integrity_digest_stability() {
        let make_workbook = || {
            let mut wb = Workbook::with_sheet("Data");
            wb.add_sheet("Summary");
            wb.sheet_mut(0)
                .expect("sheet 0 exists")
                .set_str("A1", "Revenue");
            wb.sheet_mut(0).expect("sheet 0 exists").set_str("B2", "42");
            wb.sheet_mut(1)
                .expect("sheet 1 exists")
                .set_str("A1", "Total");
            wb
        };
        let mut workbook = make_workbook();
        let digest = workbook.integrity_digest();
        assert_eq!(
            digest,
            workbook.integrity_digest(),
            "repeated calls must agree"
        );
        assert_eq!(
            digest,
            make_workbook().integrity_digest(),
            "identical workbooks must produce equal digests"
        );

        workbook
            .sheet_mut(0)
            .expect("sheet 0 exists")
            .set_str("B2", "43");
        let changed_value = workbook.integrity_digest();
        assert_ne!(
            digest, changed_value,
            "changing one cell's raw value must change the digest"
        );

        workbook
            .rename_sheet(1, "Overview")
            .expect("rename succeeds");
        assert_ne!(
            changed_value,
            workbook.integrity_digest(),
            "renaming a sheet must change the digest"
        );

        assert_ne!(
            Workbook::with_sheet("Data").integrity_digest(),
            digest,
            "an empty workbook must not share a digest with a populated one"
        );
    }

    #[test]
    fn ocr_table_to_editable_grid() {
        let cells = vec![
            OcrTableCell {
                row: 0,
                column: 0,
                text: "Item".to_string(),
                confidence: 0.95,
            },
            OcrTableCell {
                row: 0,
                column: 1,
                text: "Qty".to_string(),
                confidence: 0.91,
            },
            OcrTableCell {
                row: 1,
                column: 0,
                text: "Widget".to_string(),
                confidence: 0.42,
            },
            OcrTableCell {
                row: 1,
                column: 1,
                text: "7".to_string(),
                confidence: 0.03,
            },
        ];
        let (text, confidence) = grid_from_ocr_table(&cells).unwrap();
        assert_eq!(
            text,
            vec![
                vec!["Item".to_string(), "Qty".to_string()],
                vec!["Widget".to_string(), "7".to_string()],
            ]
        );
        assert_eq!(confidence, vec![vec![0.95, 0.91], vec![0.42, 0.03]]);

        let row_gap = vec![
            OcrTableCell {
                row: 0,
                column: 0,
                text: "a".to_string(),
                confidence: 0.5,
            },
            OcrTableCell {
                row: 2,
                column: 0,
                text: "b".to_string(),
                confidence: 0.5,
            },
        ];
        let err = grid_from_ocr_table(&row_gap).unwrap_err();
        assert!(err.contains("row 1"), "unexpected error: {err}");

        let duplicated = vec![
            OcrTableCell {
                row: 0,
                column: 0,
                text: "a".to_string(),
                confidence: 0.5,
            },
            OcrTableCell {
                row: 0,
                column: 0,
                text: "b".to_string(),
                confidence: 0.5,
            },
        ];
        assert!(grid_from_ocr_table(&duplicated)
            .unwrap_err()
            .contains("duplicate"));

        assert!(grid_from_ocr_table(&[]).is_err());
    }

    #[test]
    fn cell_ref_parse_render() {
        let a1 = CellRef::parse("A1").unwrap();
        assert_eq!(a1, CellRef { row: 0, col: 0 });
        assert_eq!(a1.to_a1(), "A1");
        assert_eq!(CellRef::parse("B3").unwrap().to_a1(), "B3");
        assert_eq!(CellRef::parse("AA10").unwrap(), CellRef { row: 9, col: 26 });
        assert!(CellRef::parse("").is_none());
        assert!(CellRef::parse("1A").is_none());
        assert!(CellRef::parse("A0").is_none());
    }

    #[test]
    fn basic_arithmetic() {
        let f = parse_formula("1+2*3").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        let v = eval_expr(&f.root, &lookup);
        assert_eq!(v, Value::Number(7.0));
    }

    #[test]
    fn precedence_and_parens() {
        let f = parse_formula("(1+2)*3").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        assert_eq!(eval_expr(&f.root, &lookup), Value::Number(9.0));
        let f2 = parse_formula("2^3^2").unwrap();
        assert_eq!(eval_expr(&f2.root, &lookup), Value::Number(512.0));
    }

    #[test]
    fn division_by_zero() {
        let f = parse_formula("1/0").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        assert_eq!(
            eval_expr(&f.root, &lookup),
            Value::Error(CalcError::DivZero)
        );
    }

    #[test]
    fn cell_references_resolve() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "5");
        sheet.set_str("C1", "=A1+B1*2");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("C1").unwrap()),
            Some(&Value::Number(20.0))
        );
    }

    #[test]
    fn dependency_order_and_cycle() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=B1+1");
        sheet.set_str("B1", "=C1+1");
        sheet.set_str("C1", "5");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(6.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Number(7.0))
        );

        // Cycle A1 <-> B1.
        let mut cyc = Sheet::new("c");
        cyc.set_str("A1", "=B1");
        cyc.set_str("B1", "=A1");
        let vals = evaluate(&cyc);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Ref))
        );
    }

    #[test]
    fn functions() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "2");
        sheet.set_str("A3", "3");
        sheet.set_str("B1", "=SUM(A1:A3)");
        sheet.set_str("B2", "=AVERAGE(A1:A3)");
        sheet.set_str("B3", "=ABS(-42)");
        sheet.set_str("B4", "=ROUND(1.567,2)");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(6.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B2").unwrap()),
            Some(&Value::Number(2.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B3").unwrap()),
            Some(&Value::Number(42.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B4").unwrap()),
            Some(&Value::Number(1.57))
        );
    }

    #[test]
    fn comparison_and_text() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=1<2");
        sheet.set_str("A2", "=CONCAT(\"loom\",\"-\",\"sheets\")");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            vals.get(&CellRef::parse("A2").unwrap()),
            Some(&Value::Text("loom-sheets".to_string()))
        );
    }

    #[test]
    fn parse_error_reports() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=1+");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Parse))
        );
    }

    #[test]
    fn csv_roundtrip() {
        let mut sheet = Sheet::new("data");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "\"a,b\"");
        sheet.set_str("A2", "=A1*2");
        let csv = to_csv(&sheet);
        assert!(csv.contains("10"));
        assert!(csv.contains("\"a,b\""));
        let loaded = from_csv("data", &csv);
        let vals = evaluate(&loaded);
        // A1 = 10 (literal), A2 self-referential cycle? No: A2 depends on A1 literal.
        assert_eq!(
            vals.get(&CellRef::parse("A2").unwrap()),
            Some(&Value::Number(20.0))
        );
    }

    #[test]
    fn json_roundtrip() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("B2", "=A1+1");
        sheet.set_col_width(1, 140.0);
        sheet.set_row_height(1, 32.0);
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        assert_eq!(back.name, "t");
        assert_eq!(back.raw(CellRef::parse("B2").unwrap()), Some("=A1+1"));
        assert_eq!(back.col_width(1), 140.0);
        assert_eq!(back.row_height(1), 32.0);
        let vals = evaluate(&back);
        assert_eq!(
            vals.get(&CellRef::parse("B2").unwrap()),
            Some(&Value::Number(2.0))
        );
    }

    #[test]
    fn editing_a_cell_retains_formula_and_empty_raw_semantics() {
        let mut sheet = Sheet::new("t");
        let cell = CellRef::parse("B1").unwrap();
        sheet.set_str("A1", "2");

        sheet.set_raw(cell, "=A1+1");
        assert_eq!(sheet.raw(cell), Some("=A1+1"));
        assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Number(3.0)));

        sheet.set_raw(cell, "");
        assert_eq!(sheet.raw(cell), Some(""));
        assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Empty));
    }

    #[test]
    fn formula_edit_transaction_commits_once_and_cancels_without_mutation() {
        let mut canceled = CellEditTransaction::begin(Some("=A1+1"));
        canceled.update("=A1+12");
        assert_eq!(canceled.cancel(), Some("=A1+1".to_string()));

        let mut committed = CellEditTransaction::begin(Some("=A1+1"));
        committed.update("=A1+12");
        let edit = committed.commit().expect("changed draft commits");
        assert_eq!(edit.before(), Some("=A1+1"));
        assert_eq!(edit.after(), "=A1+12");

        let mut unchanged = CellEditTransaction::begin(Some("=A1+1"));
        unchanged.update("=A1+1");
        assert!(unchanged.commit().is_none());

        let mut empty = CellEditTransaction::begin(Some("=A1+1"));
        empty.update("");
        let empty_edit = empty.commit().expect("empty text is a raw edit");
        assert_eq!(empty_edit.after(), "");

        let mut new_empty = CellEditTransaction::begin(None);
        new_empty.update("");
        let new_empty_edit = new_empty
            .commit()
            .expect("empty text differs from absent raw");
        assert_eq!(new_empty_edit.before(), None);
        assert_eq!(new_empty_edit.after(), "");
    }

    #[test]
    fn literal_parsing() {
        assert_eq!(parse_literal("  "), Value::Empty);
        assert_eq!(parse_literal("3.5"), Value::Number(3.5));
        assert_eq!(parse_literal("TRUE"), Value::Bool(true));
        assert_eq!(parse_literal("hello"), Value::Text("hello".to_string()));
    }

    #[test]
    fn named_ranges_validation_and_conditional_formatting_work() {
        let mut sheet = Sheet::new("model");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("B1", "=SUM(DATA)");
        let mut model = SheetModel::new(sheet);
        model
            .set_named_range("DATA", CellRange::parse("A1:A2").unwrap())
            .unwrap();
        model.validations.push((
            CellRange::parse("A1:A2").unwrap(),
            ValidationRule::Number {
                min: Some(0.0),
                max: Some(100.0),
            },
        ));
        model.conditional_formats.push(ConditionalFormatRule {
            range: CellRange::parse("A1:A2").unwrap(),
            condition: FormatCondition::GreaterThan(15.0),
            style_id: "high".into(),
        });
        assert_eq!(
            model.evaluate().get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(30.0))
        );
        assert!(model
            .set_validated(CellRef::parse("A1").unwrap(), "-1")
            .is_err());
        assert_eq!(
            model.conditional_style_ids(CellRef::parse("A2").unwrap()),
            vec!["high"]
        );
    }

    #[test]
    fn rows_sort_filter_and_ranges_are_deterministic() {
        let mut sheet = Sheet::new("rows");
        sheet.set_str("A1", "b");
        sheet.set_str("B1", "2");
        sheet.set_str("A2", "a");
        sheet.set_str("B2", "1");
        let mut model = SheetModel::new(sheet);
        let range = CellRange::parse("A1:B2").unwrap();
        model.sort_rows(range, 1, true).unwrap();
        assert_eq!(model.sheet.raw(CellRef::parse("A1").unwrap()), Some("a"));
        let hidden = model
            .filter_rows(range, 0, &FilterPredicate::Contains("a".into()))
            .unwrap();
        assert_eq!(hidden, vec![1]);
        assert_eq!(range.cells().len(), 4);
    }

    #[test]
    fn calculation_cache_recalculates_transitive_dependents() {
        let mut sheet = Sheet::new("incremental");
        sheet.set_str("A1", "1");
        sheet.set_str("B1", "=A1+1");
        sheet.set_str("C1", "=B1+1");
        sheet.set_str("Z1", "99");
        let mut cache = CalculationCache::default();
        cache.rebuild(&sheet);
        sheet.set_str("A1", "10");
        let affected = cache.recalculate(&sheet, &[CellRef::parse("A1").unwrap()]);
        assert!(affected.contains(&CellRef::parse("C1").unwrap()));
        assert!(!affected.contains(&CellRef::parse("Z1").unwrap()));
        assert_eq!(
            cache.values.get(&CellRef::parse("C1").unwrap()),
            Some(&Value::Number(12.0))
        );
    }

    #[test]
    fn extended_formula_functions_evaluate_correctly() {
        let mut sheet = Sheet::new("Formulas");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("A3", "30");
        sheet.set_str("B1", "=COUNT(A1:A3)");
        sheet.set_str("B2", "=IF(A1>5, \"High\", \"Low\")");
        sheet.set_str("B3", "=IF(A1>15, \"High\", \"Low\")");
        sheet.set_str("B4", "=AND(A1>5, A2>15)");
        sheet.set_str("B5", "=OR(A1>100, A2>15)");
        sheet.set_str("B6", "=NOT(A1>100)");

        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(3.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B2").unwrap()),
            Some(&Value::Text("High".into()))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B3").unwrap()),
            Some(&Value::Text("Low".into()))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B4").unwrap()),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B5").unwrap()),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B6").unwrap()),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn workbook_sheet_management_operations() {
        let mut wb = Workbook::with_sheet("Summary");
        assert_eq!(wb.len(), 1);
        assert!(!wb.is_empty());

        let idx2 = wb.add_sheet("Expenses");
        assert_eq!(idx2, 1);
        assert_eq!(wb.len(), 2);
        assert_eq!(wb.sheet(1).unwrap().name, "Expenses");

        wb.rename_sheet(1, "Q1 Expenses").unwrap();
        assert_eq!(wb.sheet(1).unwrap().name, "Q1 Expenses");

        wb.remove_sheet(1).unwrap();
        assert_eq!(wb.len(), 1);
        assert!(wb.remove_sheet(0).is_err()); // Cannot remove only remaining sheet
    }

    #[test]
    fn sheet_clear_and_used_range_operations() {
        let mut sheet = Sheet::new("Data");
        assert_eq!(sheet.used_range(), None);

        sheet.set_str("B2", "10");
        sheet.set_str("D5", "20");
        let bounds = sheet.used_range().unwrap();
        assert_eq!(bounds, (1, 1, 3, 4));

        let c1 = CellRef::parse("B2").unwrap();
        let c2 = CellRef::parse("D5").unwrap();
        let cleared = sheet.clear_range(c1, c2);
        assert_eq!(cleared, 2);
        assert_eq!(sheet.used_range(), None);
    }

    #[test]
    fn math_and_statistical_functions_evaluate_correctly() {
        let mut sheet = Sheet::new("Math");
        sheet.set_str("A1", "=SQRT(16)");
        sheet.set_str("A2", "=POWER(2, 8)");
        sheet.set_str("A3", "=MOD(17, 5)");
        sheet.set_str("A4", "=FLOOR(3.7)");
        sheet.set_str("A5", "=CEILING(3.2)");
        sheet.set_str("A6", "=MEDIAN(10, 20, 30, 40, 50)");

        let evaluated = evaluate(&sheet);
        assert_eq!(
            evaluated.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Number(4.0))
        );
        assert_eq!(
            evaluated.get(&CellRef::parse("A2").unwrap()),
            Some(&Value::Number(256.0))
        );
        assert_eq!(
            evaluated.get(&CellRef::parse("A3").unwrap()),
            Some(&Value::Number(2.0))
        );
        assert_eq!(
            evaluated.get(&CellRef::parse("A4").unwrap()),
            Some(&Value::Number(3.0))
        );
        assert_eq!(
            evaluated.get(&CellRef::parse("A5").unwrap()),
            Some(&Value::Number(4.0))
        );
        assert_eq!(
            evaluated.get(&CellRef::parse("A6").unwrap()),
            Some(&Value::Number(30.0))
        );
    }

    #[test]
    fn parse_csv_records_handles_multiline_quotes_and_custom_delimiters() {
        let csv_text =
            "Name,Description,Value\n\"Item 1\",\"Line 1\nLine 2\",100\n\"Item 2\",\"Simple\",200";
        let records = parse_csv_records(csv_text, ',');
        assert_eq!(records.len(), 3);
        assert_eq!(records[0], vec!["Name", "Description", "Value"]);
        assert_eq!(records[1], vec!["Item 1", "Line 1\nLine 2", "100"]);
        assert_eq!(records[2], vec!["Item 2", "Simple", "200"]);

        // Semicolon delimiter
        let semi_csv = "A;B;C\n1;2;3";
        let semi_records = parse_csv_records(semi_csv, ';');
        assert_eq!(semi_records.len(), 2);
        assert_eq!(semi_records[1], vec!["1", "2", "3"]);
    }

    #[test]
    fn csv_dialect_sniffing() {
        // Comma detected with comma.
        let dialect = sniff_csv_dialect("a,b,c\n1,2,3").unwrap();
        assert_eq!(dialect.delimiter, ',');
        assert!(!dialect.quoted);

        // Semicolon-separated input.
        let dialect = sniff_csv_dialect("a;b\nc;d").unwrap();
        assert_eq!(dialect.delimiter, ';');
        assert!(!dialect.quoted);

        // Tab delimiter.
        let dialect = sniff_csv_dialect("a\tb\n1\t2").unwrap();
        assert_eq!(dialect.delimiter, '\t');

        // Pipe delimiter.
        let dialect = sniff_csv_dialect("a|b\n1|2").unwrap();
        assert_eq!(dialect.delimiter, '|');

        // Quoted fields are detected; the comma inside quotes is not counted.
        let dialect = sniff_csv_dialect("\"x,y\",z").unwrap();
        assert_eq!(dialect.delimiter, ',');
        assert!(dialect.quoted);

        // Empty input errors.
        assert!(sniff_csv_dialect("").is_err());
        assert!(sniff_csv_dialect("  \n\t\n").is_err());

        // All candidates at zero occurrences: documented default to ','.
        let dialect = sniff_csv_dialect("abc\ndef").unwrap();
        assert_eq!(dialect.delimiter, ',');
        assert!(!dialect.quoted);
    }

    #[test]
    fn freeze_panes_configuration_and_unfreeze() {
        let mut sheet = Sheet::new("Dashboard");
        assert_eq!(sheet.freeze_rows, 0);
        assert_eq!(sheet.freeze_cols, 0);

        sheet.freeze_panes(2, 1);
        assert_eq!(sheet.freeze_rows, 2);
        assert_eq!(sheet.freeze_cols, 1);

        sheet.unfreeze_panes();
        assert_eq!(sheet.freeze_rows, 0);
        assert_eq!(sheet.freeze_cols, 0);
    }

    #[test]
    fn cell_and_range_alignment() {
        let mut sheet = Sheet::new("Sales");
        let a1 = CellRef::parse("A1").unwrap();
        let b2 = CellRef::parse("B2").unwrap();
        assert_eq!(sheet.cell_alignment(a1), CellAlignment::General);

        sheet.set_cell_alignment(a1, CellAlignment::Center);
        assert_eq!(sheet.cell_alignment(a1), CellAlignment::Center);

        // Range alignment
        let start = CellRef::parse("B1").unwrap();
        let end = CellRef::parse("C3").unwrap();
        sheet.set_range_alignment(start, end, CellAlignment::Right);
        assert_eq!(sheet.cell_alignment(b2), CellAlignment::Right);
        assert_eq!(
            sheet.cell_alignment(CellRef::parse("C3").unwrap()),
            CellAlignment::Right
        );
    }

    #[test]
    fn currency_and_percentage_formatting() {
        assert_eq!(format_number_currency(1234.56, "$", 2), "$1,234.56");
        assert_eq!(format_number_currency(-999999.0, "£", 0), "-£999,999");
        assert_eq!(format_number_currency(0.0, "$", 2), "$0.00");

        assert_eq!(format_number_percentage(0.255, 1), "25.5%");
        assert_eq!(format_number_percentage(1.0, 0), "100%");
    }

    #[test]
    fn custom_column_and_row_sizing() {
        let mut sheet = Sheet::new("Dimensions");
        assert_eq!(sheet.col_width(0), 80.0);
        assert_eq!(sheet.row_height(0), 24.0);

        sheet.set_col_width(0, 150.0);
        sheet.set_row_height(5, 36.0);
        assert_eq!(sheet.col_width(0), 150.0);
        assert_eq!(sheet.row_height(5), 36.0);

        // Reset with 0.0
        sheet.set_col_width(0, 0.0);
        assert_eq!(sheet.col_width(0), 80.0);
    }

    #[test]
    fn cell_number_format_display() {
        assert_eq!(
            format_cell_display("123.45", NumberFormat::Currency),
            "$123.45"
        );
        assert_eq!(
            format_cell_display("0.75", NumberFormat::Percentage),
            "75.0%"
        );
        assert_eq!(format_cell_display("1000", NumberFormat::Scientific), "1e3");
        assert_eq!(format_cell_display("hello", NumberFormat::General), "hello");
        assert_eq!(format_cell_display("", NumberFormat::Currency), "");
    }

    #[test]
    fn fill_series_and_range_sorting() {
        let linear = generate_fill_series(10.0, 5.0, 4, FillSeriesType::Linear);
        assert_eq!(linear, vec![10.0, 15.0, 20.0, 25.0]);

        let growth = generate_fill_series(2.0, 3.0, 4, FillSeriesType::Growth);
        assert_eq!(growth, vec![2.0, 6.0, 18.0, 54.0]);

        let rows = vec![
            vec!["Cherry".into(), "30".into()],
            vec!["Apple".into(), "10".into()],
            vec!["Banana".into(), "20".into()],
        ];

        // Sort ascending by column 0 (string)
        let sorted_str = sort_range_rows(&rows, 0, true);
        assert_eq!(sorted_str[0][0], "Apple");
        assert_eq!(sorted_str[1][0], "Banana");
        assert_eq!(sorted_str[2][0], "Cherry");

        // Sort descending by column 1 (numeric)
        let sorted_num = sort_range_rows(&rows, 1, false);
        assert_eq!(sorted_num[0][1], "30");
        assert_eq!(sorted_num[1][1], "20");
        assert_eq!(sorted_num[2][1], "10");
    }

    #[test]
    fn formula_reference_shifting() {
        // Relative reference: =A1 shifted right 1 col and down 2 rows -> =B3
        assert_eq!(shift_formula_references("=A1", 1, 2), "=B3");

        // Mixed references: =$A1 + B$2 + $C$3 + D4 shifted right 1 col, down 1 row:
        // $A1 -> $A2 (col absolute, row relative)
        // B$2 -> C$2 (col relative, row absolute)
        // $C$3 -> $C$3 (both absolute)
        // D4 -> E5 (both relative)
        assert_eq!(
            shift_formula_references("=$A1+B$2+$C$3+D4", 1, 1),
            "=$A2+C$2+$C$3+E5"
        );

        // Non-formula string unchanged
        assert_eq!(shift_formula_references("Hello World", 1, 1), "Hello World");
    }

    #[test]
    fn dependency_graph_and_recalculation_order() {
        let mut graph = DependencyGraph::new();
        let a1 = CellRef::parse("A1").unwrap();
        let b1 = CellRef::parse("B1").unwrap();
        let c1 = CellRef::parse("C1").unwrap();

        // B1 depends on A1 (=A1 * 2)
        graph.add_dependency(b1, a1);
        // C1 depends on B1 (=B1 + 10)
        graph.add_dependency(c1, b1);

        assert_eq!(graph.get_direct_dependents(&a1), &[b1]);
        assert_eq!(graph.get_direct_dependents(&b1), &[c1]);

        // Recalculation order starting from dirty cell A1
        let order = graph.get_recalculation_order(&[a1]);
        assert_eq!(order, vec![a1, b1, c1]);
    }

    #[test]
    fn cell_data_validation_rules() {
        let list_rule = DataValidationRule::new(
            ValidationCriteria::List(vec!["Red".into(), "Green".into(), "Blue".into()]),
            "Must be a valid color",
        );
        assert!(list_rule.validate("Red").is_ok());
        assert!(list_rule.validate("green").is_ok());
        assert!(list_rule.validate("Yellow").is_err());
        assert!(list_rule.validate("").is_ok()); // allow_blank = true

        let num_rule = DataValidationRule::new(
            ValidationCriteria::WholeNumberBetween(1, 100),
            "Must be 1..=100",
        );
        assert!(num_rule.validate("50").is_ok());
        assert!(num_rule.validate("101").is_err());
        assert!(num_rule.validate("abc").is_err());
    }

    #[test]
    fn vlookup_hlookup_and_index_match() {
        let table = vec![
            vec!["ID".into(), "Name".into(), "Price".into()],
            vec!["P101".into(), "Widget".into(), "9.99".into()],
            vec!["P102".into(), "Gadget".into(), "19.99".into()],
            vec!["P103".into(), "Doohickey".into(), "4.99".into()],
        ];

        // VLOOKUP P102 -> Col 2 (Name) = "Gadget"
        assert_eq!(vlookup("P102", &table, 2, true).unwrap(), "Gadget");
        // VLOOKUP P103 -> Col 3 (Price) = "4.99"
        assert_eq!(vlookup("P103", &table, 3, true).unwrap(), "4.99");
        assert!(vlookup("P999", &table, 2, true).is_err());

        // HLOOKUP Price -> Row 3 (P102's price) = "19.99"
        assert_eq!(hlookup("Price", &table, 3, true).unwrap(), "19.99");

        // MATCH Gadget in Col 2
        let names = vec!["Widget".into(), "Gadget".into(), "Doohickey".into()];
        assert_eq!(match_lookup("Gadget", &names, true).unwrap(), 2);

        // INDEX table row 2 (P101), col 2 (Name) = "Widget"
        assert_eq!(index_lookup(&table, 2, 2).unwrap(), "Widget");
    }

    #[test]
    fn text_manipulation_formulas() {
        assert_eq!(text_concatenate(&["Hello", " ", "World"]), "Hello World");
        assert_eq!(text_left("Quarterly Report", 9), "Quarterly");
        assert_eq!(text_right("Quarterly Report", 6), "Report");
        assert_eq!(text_mid("Loom Studio 2026", 6, 6), "Studio");
        assert_eq!(text_len("Supercalifragilistic"), 20);
        assert_eq!(text_trim("   Too   many   spaces   "), "Too many spaces");
        assert_eq!(text_upper("loom sheets"), "LOOM SHEETS");
        assert_eq!(text_lower("LOOM SHEETS"), "loom sheets");
        assert_eq!(text_proper("the quick brown fox"), "The Quick Brown Fox");
    }

    #[test]
    fn sumproduct_and_conditional_aggregations() {
        let quantities = vec![2.0, 5.0, 10.0];
        let unit_prices = vec![10.0, 20.0, 5.0];

        // SUMPRODUCT: (2*10) + (5*20) + (10*5) = 20 + 100 + 50 = 170.0
        let total = sumproduct(&[&quantities, &unit_prices]).unwrap();
        assert_eq!(total, 170.0);

        let sales = vec![100.0, 250.0, 50.0, 400.0, 150.0];

        // SUMIF sales > 100 -> 250 + 400 + 150 = 800.0
        assert_eq!(sumif(&sales, |v| v > 100.0), 800.0);

        // COUNTIF sales >= 200 -> 2
        assert_eq!(countif(&sales, |v| v >= 200.0), 2);

        // AVERAGEIF sales < 200 -> (100 + 50 + 150) / 3 = 100.0
        assert_eq!(averageif(&sales, |v| v < 200.0), Some(100.0));
    }

    #[test]
    fn financial_formula_pmt_fv_pv() {
        // Loan of $10,000 at 5% annual interest (0.05/12 per month) for 36 months
        let monthly_rate = 0.05 / 12.0;
        let monthly_payment = pmt(monthly_rate, 36.0, 10000.0, 0.0, true).unwrap();
        // PMT should be approximately -$299.71
        assert!((monthly_payment - (-299.71)).abs() < 0.1);

        // Future value of $100/mo at 6% annual for 10 years (120 months)
        let rate_6 = 0.06 / 12.0;
        let future_val = fv(rate_6, 120.0, -100.0, 0.0, true).unwrap();
        // FV should be approximately $16,387.93
        assert!((future_val - 16387.93).abs() < 1.0);

        // Present value of $10,000 in 5 years at 5%
        let present_val = pv(0.05, 5.0, 0.0, 10000.0, true).unwrap();
        // PV should be approximately -$7,835.26
        assert!((present_val - (-7835.26)).abs() < 1.0);
    }

    #[test]
    fn statistical_formulas_mode_stdev_var() {
        let dataset = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];

        // Mode of dataset is 4.0
        assert_eq!(mode_single(&dataset).unwrap(), 4.0);

        // Population variance & stdev: mean = 5.0, sum((x-5)^2) = 9+1+1+1+0+0+4+16 = 32. var = 32/8 = 4.0. stdev = 2.0
        assert_eq!(var_p(&dataset).unwrap(), 4.0);
        assert_eq!(stdev_p(&dataset).unwrap(), 2.0);

        // Sample variance: 32 / 7 = 4.5714...
        let v_s = var_s(&dataset).unwrap();
        assert!((v_s - (32.0 / 7.0)).abs() < 1e-5);
        assert!((stdev_s(&dataset).unwrap() - (32.0 / 7.0f64).sqrt()).abs() < 1e-5);
    }

    #[test]
    fn pivot_grouping_and_aggregation() {
        let keys = vec![
            "East".to_string(),
            "West".to_string(),
            "East".to_string(),
            "South".to_string(),
            "West".to_string(),
        ];
        let values = vec![10.0, 20.0, 15.0, 30.0, 5.0];

        // SUM: East = 10+15 = 25, South = 30, West = 20+5 = 25.
        // Results are sorted by group key ascending: East < South < West.
        assert_eq!(
            compute_pivot(&keys, &values, PivotAggregation::Sum).unwrap(),
            vec![
                ("East".to_string(), 25.0),
                ("South".to_string(), 30.0),
                ("West".to_string(), 25.0),
            ]
        );

        // AVERAGE: East = (10+15)/2 = 12.5
        let averages = compute_pivot(&keys, &values, PivotAggregation::Average).unwrap();
        assert_eq!(averages[0], ("East".to_string(), 12.5));

        // MIN/MAX/COUNT for the East group: min = 10, max = 15, count = 2.
        let mins = compute_pivot(&keys, &values, PivotAggregation::Min).unwrap();
        let maxes = compute_pivot(&keys, &values, PivotAggregation::Max).unwrap();
        let counts = compute_pivot(&keys, &values, PivotAggregation::Count).unwrap();
        assert_eq!(mins[0], ("East".to_string(), 10.0));
        assert_eq!(maxes[0], ("East".to_string(), 15.0));
        assert_eq!(counts[0], ("East".to_string(), 2.0));

        // Mismatched lengths must be rejected.
        assert!(compute_pivot(&keys[..1], &values, PivotAggregation::Sum).is_err());

        // Empty input yields an empty result.
        let empty: Vec<String> = Vec::new();
        assert!(compute_pivot(&empty, &[], PivotAggregation::Sum)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn chart_spec_validation_and_normalization() {
        let series_a = ChartSeries {
            name: "Revenue".to_string(),
            categories: vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
            values: vec![0.0, 5.0, 10.0],
        };
        let series_b = ChartSeries {
            name: "Costs".to_string(),
            categories: vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
            values: vec![10.0, 5.0, 0.0],
        };
        let spec = ChartSpec {
            kind: ChartKind::Bar,
            title: "Quarterly".to_string(),
            series: vec![series_a.clone(), series_b.clone()],
        };

        assert!(spec.validate().is_ok());
        assert_eq!(spec.value_range().unwrap(), (0.0, 10.0));

        let points = spec.normalized_points().unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0], vec![0.0, 0.5, 1.0]);
        assert_eq!(points[1], vec![1.0, 0.5, 0.0]);
        for series_points in &points {
            for &point in series_points {
                assert!((0.0..=1.0).contains(&point));
            }
        }

        // A constant-value series maps to 1.0 everywhere (min == max).
        let constant = ChartSpec {
            kind: ChartKind::Line,
            title: "Flat".to_string(),
            series: vec![ChartSeries {
                name: "Constant".to_string(),
                categories: vec!["a".to_string(), "b".to_string()],
                values: vec![7.0, 7.0],
            }],
        };
        assert_eq!(constant.value_range().unwrap(), (7.0, 7.0));
        assert_eq!(constant.normalized_points().unwrap(), vec![vec![1.0, 1.0]]);

        // Validation errors name the violated rule.
        let empty_title = ChartSpec {
            kind: ChartKind::Pie,
            title: String::new(),
            series: vec![series_a.clone()],
        };
        assert_eq!(
            empty_title.validate().unwrap_err(),
            "chart title must not be empty"
        );

        let no_series = ChartSpec {
            kind: ChartKind::Line,
            title: "Empty".to_string(),
            series: Vec::new(),
        };
        assert_eq!(
            no_series.validate().unwrap_err(),
            "chart must contain at least one series"
        );

        let mismatch = ChartSpec {
            kind: ChartKind::Scatter,
            title: "Mismatched".to_string(),
            series: vec![ChartSeries {
                name: series_b.name.clone(),
                categories: vec!["Q1".to_string()],
                values: vec![10.0, 5.0],
            }],
        };
        assert_eq!(
            mismatch.validate().unwrap_err(),
            "series 'Costs' has 1 categories but 2 values; lengths must match"
        );

        let nan = ChartSpec {
            kind: ChartKind::Line,
            title: "NaN".to_string(),
            series: vec![ChartSeries {
                name: series_a.name.clone(),
                categories: vec!["Q1".to_string(), "Q2".to_string()],
                values: vec![1.0, f64::NAN],
            }],
        };
        assert_eq!(
            nan.validate().unwrap_err(),
            "series 'Revenue' contains a NaN value at index 1"
        );
    }

    #[test]
    fn chart_placement_export_validation() {
        let spec = ChartSpec {
            kind: ChartKind::Bar,
            title: "Quarterly".to_string(),
            series: vec![ChartSeries {
                name: "Revenue".to_string(),
                categories: vec!["Q1".to_string(), "Q2".to_string()],
                values: vec![1.0, 2.0],
            }],
        };
        let placement = |chart_id: &str, range: &str, policy: ChartUpdatePolicy| ChartPlacement {
            chart_id: chart_id.to_string(),
            sheet_name: "Sales".to_string(),
            source_range: range.to_string(),
            spec: spec.clone(),
            update_policy: policy,
        };

        // Valid ranges pass, including lowercase corners.
        let valid = placement("chart-1", "B2:D9", ChartUpdatePolicy::RefreshOnOpen);
        assert!(valid.validate().is_ok());
        assert!(
            placement("chart-1", "b2:d9", ChartUpdatePolicy::StaticSnapshot)
                .validate()
                .is_ok()
        );
        assert!(
            placement("chart-1", "AA10:AB11", ChartUpdatePolicy::StaticSnapshot)
                .validate()
                .is_ok()
        );

        // Bad range shapes name the violated rule.
        let missing_separator = placement("chart-1", "B2", ChartUpdatePolicy::StaticSnapshot);
        assert_eq!(
            missing_separator.validate().unwrap_err(),
            "source range 'B2' must contain exactly one ':' separator"
        );

        let digits_first = placement("chart-1", "2B:B2", ChartUpdatePolicy::StaticSnapshot);
        assert_eq!(
            digits_first.validate().unwrap_err(),
            "source range corner '2B' must be column letters followed by row digits"
        );

        let open_ended = placement("chart-1", "B2:", ChartUpdatePolicy::StaticSnapshot);
        assert_eq!(
            open_ended.validate().unwrap_err(),
            "source range corner '' must be column letters followed by row digits"
        );

        let letters_only = placement("chart-1", "B:D", ChartUpdatePolicy::StaticSnapshot);
        assert!(letters_only.validate().is_err());

        // Empty ids and sheet names are rejected.
        assert!(placement("", "B2:D9", ChartUpdatePolicy::StaticSnapshot)
            .validate()
            .is_err());
        let no_sheet = ChartPlacement {
            chart_id: "chart-1".to_string(),
            sheet_name: String::new(),
            source_range: "B2:D9".to_string(),
            spec: spec.clone(),
            update_policy: ChartUpdatePolicy::StaticSnapshot,
        };
        assert!(no_sheet.validate().is_err());

        // Collision rule 1: same chart_id collides regardless of range or policy.
        let same_id = placement("chart-1", "C3:E8", ChartUpdatePolicy::StaticSnapshot);
        assert!(valid.collides_with(&same_id));
        assert!(same_id.collides_with(&valid));

        // Collision rule 2: identical sheet+range with a non-static policy collides.
        let refreshed = placement("chart-2", "B2:D9", ChartUpdatePolicy::RefreshOnOpen);
        assert!(valid.collides_with(&refreshed));

        // A static twin still collides because the refreshed side re-reads the shared range.
        let static_twin = placement("chart-2", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
        assert!(valid.collides_with(&static_twin));

        // Two pure snapshots sharing the range never fight over updates.
        let static_pair_a = placement("chart-2", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
        let static_pair_b = placement("chart-3", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
        assert!(!static_pair_a.collides_with(&static_pair_b));

        // Fully distinct placements never collide.
        let distinct =
            placement("chart-4", "F1:G4", ChartUpdatePolicy::RefreshOnOpen).collides_with(&valid);
        assert!(!distinct);
    }

    #[test]
    fn goal_seek_solves_equations() {
        let x = goal_seek_bisection(|v| v * v, 0.0, 10.0, 25.0, 1e-6, 200).unwrap();
        assert!((x - 5.0).abs() < 1e-6, "x^2=25 gave {x}");

        let r = goal_seek_bisection(
            |rate| 100.0 * (1.0 + rate).powi(10),
            0.0,
            0.2,
            200.0,
            1e-6,
            200,
        )
        .unwrap();
        let expected_r = 2f64.powf(0.1) - 1.0;
        assert!((r - expected_r).abs() < 1e-6, "rate solve gave {r}");

        // Linear function whose first midpoint is the exact root: returns after
        // only the two bracket evaluations plus one midpoint evaluation.
        let calls = std::cell::Cell::new(0u32);
        let hit = goal_seek_bisection(
            |v| {
                calls.set(calls.get() + 1);
                v - 4.0
            },
            0.0,
            8.0,
            0.0,
            1e-9,
            1000,
        )
        .unwrap();
        assert_eq!(hit, 4.0);
        assert_eq!(calls.get(), 3);

        let same_signs = goal_seek_bisection(|v| v + 10.0, -1.0, 1.0, 0.0, 1e-6, 64).unwrap_err();
        assert!(same_signs.contains("no sign change"), "{same_signs}");

        let inverted = goal_seek_bisection(|v| v * v, 10.0, 0.0, 25.0, 1e-6, 64).unwrap_err();
        assert!(inverted.contains("hi > lo"), "{inverted}");

        let constant = goal_seek_bisection(|_| 7.0, 0.0, 5.0, 3.0, 1e-6, 64).unwrap_err();
        assert!(constant.contains("no sign change"), "{constant}");
    }

    #[test]
    fn text_join_split_substitute_repeat() {
        // TEXTJOIN with and without skipping empties
        let values = vec!["a".to_string(), String::new(), "b".to_string()];
        assert_eq!(text_join("-", false, &values), "a--b");
        assert_eq!(text_join("-", true, &values), "a-b");
        assert_eq!(text_join(",", true, &[]), "");

        // SPLIT keeps empty fields from consecutive delimiters
        let fields = split_text_to_columns("a,,b", ",").unwrap();
        assert_eq!(
            fields,
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(
            split_text_to_columns("x|y", "|").unwrap(),
            vec!["x".to_string(), "y".to_string()]
        );
        assert!(split_text_to_columns("abc", "").is_err());

        // REPT
        assert_eq!(text_repeat("ab", 3), "ababab");
        assert_eq!(text_repeat("ab", 0), "");

        // SUBSTITUTE
        // "Banana" contains "na" at bytes 2 and 4; replacing all yields Ba|ny|ny
        assert_eq!(
            text_substitute("Banana", "na", "ny", true, 0).unwrap(),
            "Banyny"
        );
        assert_eq!(
            text_substitute("Banana", "NA", "ny", false, 0).unwrap(),
            "Banyny"
        );
        assert_eq!(
            text_substitute("Banana", "NA", "ny", true, 0).unwrap(),
            "Banana"
        );
        // Only the second instance replaced
        assert_eq!(
            text_substitute("Banana", "na", "X", true, 2).unwrap(),
            "BanaX"
        );
        // Out-of-range instance leaves the text unchanged
        assert_eq!(
            text_substitute("Banana", "na", "X", true, 9).unwrap(),
            "Banana"
        );
        assert!(text_substitute("abc", "", "x", true, 0).is_err());
    }

    #[test]
    fn date_arithmetic_civil_calendar() {
        // Leap years: divisible by 4, except centuries unless divisible by 400.
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));

        assert_eq!(days_in_month(2024, 2).unwrap(), 29);
        assert_eq!(days_in_month(1900, 2).unwrap(), 28);
        assert!(days_in_month(2024, 13).is_err());

        // Month and year boundary crossings in both directions.
        assert_eq!(add_days(2024, 1, 31, 1).unwrap(), (2024, 2, 1));
        assert_eq!(add_days(2023, 12, 31, 1).unwrap(), (2024, 1, 1));
        assert_eq!(add_days(2024, 3, 1, -1).unwrap(), (2024, 2, 29));

        // Whole-day difference spans a leap day; sign reflects direction.
        assert_eq!(days_between(2023, 3, 1, 2024, 3, 1).unwrap(), 366);
        assert_eq!(days_between(2024, 3, 1, 2023, 3, 1).unwrap(), -366);

        // Invalid civil dates are rejected before any arithmetic.
        assert!(add_days(2024, 2, 30, 1).is_err());
        assert!(days_between(2024, 2, 30, 2024, 3, 1).is_err());
        assert!(days_in_month(2024, 0).is_err());

        // Large-offset round trip: +10000 days, measure, then subtract back.
        let (y0, m0, d0) = (2021, 6, 15u32);
        let (y1, m1, d1) = add_days(y0, m0, d0, 10_000).unwrap();
        assert_eq!(days_between(y0, m0, d0, y1, m1, d1).unwrap(), 10_000);
        assert_eq!(add_days(y1, m1, d1, -10_000).unwrap(), (y0, m0, d0));
    }

    /// Builds an in-memory xlsx image with the given part contents.
    fn build_xlsx(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut archive = PackageArchive::new();
        for (path, xml) in parts {
            archive
                .add(path, xml.as_bytes().to_vec())
                .expect("part adds");
        }
        archive.to_bytes().expect("archive serializes")
    }

    #[test]
    fn extract_xlsx_grid_resolves_shared_inline_numeric_and_boolean_cells() {
        let xlsx = build_xlsx(&[
            (
                "xl/sharedStrings.xml",
                r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                    <si><t>Alpha</t></si>
                    <si><t>Beta &amp; Gamma</t></si>
                </sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                    <sheetData>
                        <row r="1">
                            <c r="A1" t="s"><v>0</v></c>
                            <c r="B1" t="s"><v>1</v></c>
                        </row>
                        <row r="2">
                            <c r="A2"><v>42</v></c>
                            <c r="B2" t="b"><v>1</v></c>
                        </row>
                        <row r="3">
                            <c r="C3" t="inlineStr"><is><t>Inline</t></is></c>
                        </row>
                    </sheetData>
                </worksheet>"#,
            ),
        ]);

        let grid = extract_xlsx_grid(&xlsx).expect("extraction succeeds");
        assert_eq!(grid.len(), 3);
        assert_eq!(
            grid,
            vec![
                vec![
                    "Alpha".to_string(),
                    "Beta & Gamma".to_string(),
                    String::new()
                ],
                vec!["42".to_string(), "TRUE".to_string(), String::new()],
                vec![String::new(), String::new(), "Inline".to_string()],
            ]
        );
    }

    #[test]
    fn extract_xlsx_grid_errs_on_corrupt_archive_or_missing_sheet_part() {
        assert!(extract_xlsx_grid(b"not a zip at all")
            .unwrap_err()
            .contains("unreadable xlsx archive"));

        // A well-formed archive without the worksheet part must also err.
        let xlsx = build_xlsx(&[("xl/sharedStrings.xml", "<sst><si><t>Alpha</t></si></sst>")]);
        assert!(extract_xlsx_grid(&xlsx)
            .unwrap_err()
            .contains("missing worksheet part xl/worksheets/sheet1.xml"));
    }

    #[test]
    fn xlsx_extraction_helpers_handle_entities_coordinates_and_gaps() {
        // Single-pass entity decoding must not double-decode.
        assert_eq!(xml_unescape("Beta &amp; Gamma"), "Beta & Gamma");
        assert_eq!(xml_unescape("&amp;lt;"), "&lt;");
        assert_eq!(
            xml_unescape("&lt;a&gt; &quot;q&quot; &apos;p&apos;"),
            "<a> \"q\" 'p'"
        );
        assert_eq!(
            xml_unescape("plain & unknown; stays"),
            "plain & unknown; stays"
        );

        // Coordinate parsing: letters to zero-based column, digits to row-1.
        assert_eq!(parse_cell_coordinate("A1"), Some((0, 0)));
        assert_eq!(parse_cell_coordinate("AB12"), Some((27, 11)));
        assert_eq!(parse_cell_coordinate(" C3 "), Some((2, 2)));
        assert_eq!(parse_cell_coordinate(""), None);
        assert_eq!(parse_cell_coordinate("1A"), None);
        assert_eq!(parse_cell_coordinate("A0"), None);

        // Out-of-range shared-string index errs instead of emitting a value.
        let sheet = r#"<sheetData><row r="1"><c r="A1" t="s"><v>9</v></c></row></sheetData>"#;
        let err = extract_sheet_grid(sheet, &["Alpha".to_string()]).unwrap_err();
        assert!(err.contains("out of range"), "unexpected error: {err}");

        // Cells beyond the used range densify as empty strings.
        let sparse = r#"<sheetData>
            <row r="1"><c r="B1"><v>7</v></c></row>
            <row r="4"><c r="E4"><v>x</v></c></row>
        </sheetData>"#;
        let grid = extract_sheet_grid(sparse, &[]).expect("sparse extraction");
        assert_eq!(grid.len(), 4);
        assert!(grid.iter().all(|row| row.len() == 5));
        assert_eq!(grid[0][1], "7");
        assert_eq!(grid[3][4], "x");
        assert_eq!(grid[0][0], "");
    }

    #[test]
    fn xlsx_export_round_trips_through_import() {
        let grid = vec![
            vec!["Region".to_string(), "Q1".to_string(), "Q2".to_string()],
            vec!["East".to_string(), "10".to_string(), String::new()],
            vec![
                "West & Co <Ltd>".to_string(),
                String::new(),
                "TRUE".to_string(),
            ],
            vec![String::new(); 3],
        ];

        let xlsx = export_xlsx_from_grid(&grid).expect("export succeeds");
        let parsed = extract_xlsx_grid(&xlsx).expect("re-import succeeds");

        // Populated rows round-trip exactly; the trailing all-empty row is dropped
        // (documented rule: absence is not content).
        assert_eq!(parsed.len(), grid.len() - 1);
        for (expected, actual) in grid.iter().zip(parsed.iter()) {
            assert_eq!(actual, expected);
        }
        assert_eq!(parsed[2][0], "West & Co <Ltd>");
        assert_eq!(parsed[1][2], "");
        assert_eq!(parsed[0], grid[0]);

        // Repeated strings share one shared-string entry (structural check).
        let archive = PackageArchive::from_bytes(&xlsx).unwrap();
        let sst = std::str::from_utf8(archive.get("xl/sharedStrings.xml").unwrap()).unwrap();
        let unique = sst.matches("<si>").count();
        let referenced = sst.matches("count=\"").count() > 0;
        assert!(referenced && unique == 7, "unique={unique}");

        // Empty grids export an empty sheet and import back empty.
        let empty = export_xlsx_from_grid(&[]).unwrap();
        assert!(extract_xlsx_grid(&empty).unwrap().is_empty());
    }

    #[test]
    fn viewport_reveals_a_selection_outside_its_visible_window() {
        let mut viewport = SheetViewport::new(15, 8);

        viewport.reveal(CellRef { row: 24, col: 9 });

        assert_eq!(viewport.first_row, 10);
        assert_eq!(viewport.first_col, 2);
        assert!(viewport.contains(CellRef { row: 24, col: 9 }));
        assert_eq!(viewport.row_at(0), Some(10));
        assert_eq!(viewport.column_at(0), Some(2));
    }

    #[test]
    fn viewport_projects_scroll_offsets_against_workbook_dimensions() {
        let dimensions = SheetDimensions::new(1_000, 52);
        let viewport =
            SheetViewport::from_scroll(180.0, 672.0, 360.0, 280.0, 28.0, 90.0, dimensions);

        assert_eq!(viewport.first_row, 24);
        assert_eq!(viewport.first_col, 2);
        assert_eq!(viewport.visible_rows, 10);
        assert_eq!(viewport.visible_cols, 4);
        assert_eq!(viewport.row_at(0), Some(24));
        assert_eq!(viewport.column_at(3), Some(5));
        assert_eq!(dimensions.content_size(28.0, 90.0), (4_680.0, 28_000.0));
    }

    #[test]
    fn viewport_clamps_invalid_and_out_of_range_scroll_offsets() {
        let dimensions = SheetDimensions::new(10, 8);
        let origin =
            SheetViewport::from_scroll(-100.0, f32::NAN, 160.0, 48.0, 24.0, 80.0, dimensions);
        assert_eq!(origin.first_row, 0);
        assert_eq!(origin.first_col, 0);
        assert_eq!(origin.visible_rows, 2);
        assert_eq!(origin.visible_cols, 2);

        let tail =
            SheetViewport::from_scroll(10_000.0, 10_000.0, 160.0, 48.0, 24.0, 80.0, dimensions);
        assert_eq!(tail.first_row, 8);
        assert_eq!(tail.first_col, 6);
        assert_eq!(tail.visible_rows, 2);
        assert_eq!(tail.visible_cols, 2);
    }

    #[test]
    fn sheet_dimensions_follow_sparse_used_cells_with_nonempty_minimum() {
        let mut empty = Sheet::new("empty");
        assert_eq!(empty.dimensions(), SheetDimensions::new(1, 1));

        empty.set_str("AZ1000", "tail");
        assert_eq!(empty.dimensions(), SheetDimensions::new(1_000, 52));
    }

    #[test]
    fn sheet_grid_defaults_are_shared_and_invalid_dimensions_fall_back() {
        let mut sheet = Sheet::new("defaults");
        assert_eq!(DEFAULT_COL_WIDTH, 80.0);
        assert_eq!(DEFAULT_ROW_HEIGHT, 24.0);
        assert_eq!(sheet.col_width(0), DEFAULT_COL_WIDTH);
        assert_eq!(sheet.row_height(0), DEFAULT_ROW_HEIGHT);

        sheet.set_col_width(0, f32::NAN);
        sheet.set_col_width(1, f32::INFINITY);
        sheet.set_row_height(0, f32::NAN);
        sheet.set_row_height(1, f32::INFINITY);
        assert_eq!(sheet.col_width(0), DEFAULT_COL_WIDTH);
        assert_eq!(sheet.col_width(1), DEFAULT_COL_WIDTH);
        assert_eq!(sheet.row_height(0), DEFAULT_ROW_HEIGHT);
        assert_eq!(sheet.row_height(1), DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn grid_selection_preserves_anchor_and_normalizes_range() {
        let anchor = CellRef::parse("D7").unwrap();
        let focus = CellRef::parse("B3").unwrap();
        let selection = GridSelection::new(anchor, focus);

        assert_eq!(selection.anchor, anchor);
        assert_eq!(selection.focus, focus);
        assert_eq!(selection.range().to_a1(), "B3:D7");
        assert!(selection.contains(CellRef::parse("C5").unwrap()));
        assert!(!selection.contains(CellRef::parse("E7").unwrap()));
        assert_eq!(selection.label(), "B3:D7");
    }

    #[test]
    fn grid_selection_extend_and_collapse_keep_keyboard_semantics() {
        let anchor = CellRef::parse("C3").unwrap();
        let selection = GridSelection::new(anchor, anchor).extend(CellRef::parse("E5").unwrap());
        assert_eq!(selection.anchor, anchor);
        assert_eq!(selection.focus, CellRef::parse("E5").unwrap());
        assert_eq!(selection.label(), "C3:E5");

        let collapsed = selection.collapse(CellRef::parse("B2").unwrap());
        assert_eq!(collapsed.anchor, CellRef::parse("B2").unwrap());
        assert_eq!(collapsed.focus, CellRef::parse("B2").unwrap());
        assert_eq!(collapsed.label(), "B2");
    }

    #[test]
    fn range_edit_fills_formulas_and_reverts_without_losing_absent_cells() {
        let mut sheet = Sheet::new("fill");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "=A1+1");
        sheet.set_str("A2", "20");

        let copy = RangeEdit::copy(
            &sheet,
            CellRange::parse("B1").unwrap(),
            CellRef::parse("C1").unwrap(),
        );
        copy.apply(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), Some("=B1+1"));
        copy.revert(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), None);

        let fill = RangeEdit::fill(
            &sheet,
            CellRange::parse("A1:A2").unwrap(),
            CellRange::parse("A3:A6").unwrap(),
        );
        fill.apply(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), Some("10"));
        assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), Some("20"));
        assert_eq!(sheet.raw(CellRef::parse("A5").unwrap()), Some("10"));
        assert_eq!(sheet.raw(CellRef::parse("A6").unwrap()), Some("20"));
        fill.revert(&mut sheet);
        for row in 3..=6 {
            assert_eq!(
                sheet.raw(CellRef {
                    row: row - 1,
                    col: 0
                }),
                None
            );
        }
    }

    #[test]
    fn range_edit_copy_handles_multi_cell_formulas_and_noop_detection() {
        let mut sheet = Sheet::new("copy");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "=A1+1");
        sheet.set_str("A2", "20");

        let source = CellRange::parse("A1:B2").unwrap();
        let copy = RangeEdit::copy(&sheet, source, CellRef::parse("D3").unwrap());
        assert_eq!(copy.len(), 4);
        assert!(!copy.is_noop());
        copy.apply(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("D3").unwrap()), Some("10"));
        assert_eq!(sheet.raw(CellRef::parse("E3").unwrap()), Some("=D3+1"));
        assert_eq!(sheet.raw(CellRef::parse("D4").unwrap()), Some("20"));
        assert_eq!(sheet.raw(CellRef::parse("E4").unwrap()), None);

        copy.revert(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("D3").unwrap()), None);
        assert_eq!(sheet.raw(CellRef::parse("E3").unwrap()), None);
        assert_eq!(sheet.raw(CellRef::parse("D4").unwrap()), None);
        assert_eq!(sheet.raw(CellRef::parse("E4").unwrap()), None);

        let noop = RangeEdit::copy(
            &sheet,
            CellRange::parse("A1").unwrap(),
            CellRef::parse("A1").unwrap(),
        );
        assert!(noop.is_noop());
    }

    #[test]
    fn range_edit_replace_preserves_present_empty_and_absent_raw_values() {
        let mut sheet = Sheet::new("replace");
        let cell = CellRef::parse("A1").unwrap();

        let insert_empty = RangeEdit::replace(&sheet, cell, Some(String::new()));
        assert!(!insert_empty.is_noop());
        insert_empty.apply(&mut sheet);
        assert_eq!(sheet.raw(cell), Some(""));
        insert_empty.revert(&mut sheet);
        assert_eq!(sheet.raw(cell), None);

        sheet.set_raw(cell, "old");
        let clear_to_empty = RangeEdit::replace(&sheet, cell, Some(String::new()));
        clear_to_empty.apply(&mut sheet);
        assert_eq!(sheet.raw(cell), Some(""));
        clear_to_empty.revert(&mut sheet);
        assert_eq!(sheet.raw(cell), Some("old"));

        let remove = RangeEdit::replace(&sheet, cell, None);
        remove.apply(&mut sheet);
        assert_eq!(sheet.raw(cell), None);
        remove.revert(&mut sheet);
        assert_eq!(sheet.raw(cell), Some("old"));
    }
}
