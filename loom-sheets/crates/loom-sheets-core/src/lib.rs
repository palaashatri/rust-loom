//! Loom Sheets formula engine and workbook model — headless and testable.
//!
//! The engine implements a tokenizer, a recursive-descent parser, cell
//! reference resolution, and a dependency-graph evaluator with topological
//! ordering and cycle detection. CSV import/export is included for
//! interoperability. The GUI (a documented follow-on) consumes this engine.

use std::collections::{BTreeMap, HashMap, HashSet};

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
        if width > 0.0 {
            self.col_widths.insert(col, width);
        } else {
            self.col_widths.remove(&col);
        }
    }

    /// Gets column width for column index (defaulting to 80.0 px).
    pub fn col_width(&self, col: u32) -> f32 {
        self.col_widths.get(&col).copied().unwrap_or(80.0)
    }

    /// Sets custom row height for row index.
    pub fn set_row_height(&mut self, row: u32, height: f32) {
        if height > 0.0 {
            self.row_heights.insert(row, height);
        } else {
            self.row_heights.remove(&row);
        }
    }

    /// Gets row height for row index (defaulting to 24.0 px).
    pub fn row_height(&self, row: u32) -> f32 {
        self.row_heights.get(&row).copied().unwrap_or(24.0)
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
    Ok(sheet)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        assert_eq!(back.name, "t");
        assert_eq!(back.raw(CellRef::parse("B2").unwrap()), Some("=A1+1"));
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
}
