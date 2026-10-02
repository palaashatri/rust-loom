//! Render one worksheet part: cells with typed cached values, array
//! formulas, styles, column widths, row heights and frozen panes.
//!
//! Every rule here exists because Microsoft Excel enforces it when it opens
//! a package (rows ascending and unique, error cells holding only real error
//! literals, formula text results typed `str`, formulas Excel can parse).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{CalcError, CellRef, Sheet, Value};

use super::cell_refs::column_letters;
use super::formula_text::{excel_formula, is_plausible_formula};
use super::styles::XfKey;
use super::xml::xml_escape_text;
use super::{MAIN_NS, REL_NS};

/// Loom measures sizes in pixels at 96 dpi; Excel row heights are points and
/// column widths are counts of the default font's maximum digit width
/// (7 px for Calibri 11, padding included in the stored value).
const POINTS_PER_PIXEL: f32 = 0.75;
const PIXELS_PER_CHARACTER: f32 = 7.0;

/// The shared-string table of a whole workbook.
#[derive(Default)]
pub(super) struct SharedStrings {
    index: BTreeMap<String, usize>,
    ordered: Vec<String>,
    references: usize,
}

impl SharedStrings {
    fn intern(&mut self, text: &str) -> usize {
        self.references += 1;
        if let Some(position) = self.index.get(text) {
            return *position;
        }
        let position = self.ordered.len();
        self.index.insert(text.to_string(), position);
        self.ordered.push(text.to_string());
        position
    }

    pub(super) fn render(&self) -> String {
        let items = self
            .ordered
            .iter()
            .map(|text| format!("<si><t xml:space=\"preserve\">{}</t></si>", xml_text(text)))
            .collect::<String>();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><sst xmlns=\"{MAIN_NS}\" count=\"{}\" uniqueCount=\"{}\">{items}</sst>",
            self.references,
            self.ordered.len()
        )
    }
}

/// Escape character data. XML 1.0 forbids most control characters and turns a
/// raw carriage return into a line feed, so those travel in the `_xHHHH_`
/// form Excel itself uses; a literal text that already looks like that form
/// has its underscore escaped so it survives a round trip.
pub(super) fn xml_text(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (position, &character) in characters.iter().enumerate() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' | '\t' => out.push(character),
            '_' if looks_like_escape(&characters[position..]) => out.push_str("_x005F_"),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {
                out.push_str(&format!("_x{:04X}_", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn looks_like_escape(rest: &[char]) -> bool {
    rest.len() >= 7
        && rest[1] == 'x'
        && rest[2..6].iter().all(char::is_ascii_hexdigit)
        && rest[6] == '_'
}

/// What the worksheet needs from the workbook around it.
pub(super) struct WorksheetInput<'a> {
    pub sheet: &'a Sheet,
    pub values: &'a HashMap<CellRef, Value>,
    pub style_ids: &'a [(XfKey, usize)],
    pub sheet_names: &'a [(String, String)],
    pub selected: bool,
    pub has_drawing: bool,
}

/// The Excel error literal for a calculation error. Parse failures never get
/// here: a formula Excel could not parse is exported as its text instead.
fn error_literal(error: CalcError) -> &'static str {
    match error {
        CalcError::DivZero => "#DIV/0!",
        CalcError::NA => "#N/A",
        CalcError::Value => "#VALUE!",
        CalcError::Name | CalcError::Parse => "#NAME?",
        CalcError::Ref => "#REF!",
        CalcError::Spill => "#SPILL!",
    }
}

/// `(t attribute, <v> content)`. Formula results that are text are `str`;
/// literal text is a shared string.
fn typed_value(
    value: &Value,
    formula: bool,
    strings: &mut SharedStrings,
) -> (Option<&'static str>, Option<String>) {
    match value {
        Value::Number(number) if number.is_finite() => (None, Some(format!("{number}"))),
        Value::Number(_) => (Some("e"), Some("#NUM!".to_string())),
        Value::Bool(flag) => (Some("b"), Some(u8::from(*flag).to_string())),
        Value::Error(error) => (Some("e"), Some(error_literal(*error).to_string())),
        Value::Text(text) if formula => (Some("str"), Some(xml_text(text))),
        Value::Text(text) => (Some("s"), Some(strings.intern(text).to_string())),
        Value::Empty if formula => (Some("str"), Some(String::new())),
        Value::Empty => (None, None),
        Value::Array(items, _, _) => match items.first() {
            Some(first) => typed_value(first, formula, strings),
            None => (None, None),
        },
    }
}

struct ArrayOwner {
    rows: usize,
    cols: usize,
}

pub(super) fn render_worksheet(input: &WorksheetInput<'_>, strings: &mut SharedStrings) -> String {
    let sheet = input.sheet;
    let mut array_owners = BTreeMap::<CellRef, ArrayOwner>::new();
    let mut spill_values = BTreeMap::<CellRef, &Value>::new();
    for (cell, entry) in &sheet.cells {
        if !entry.raw.starts_with('=') {
            continue;
        }
        if let Some(Value::Array(items, rows, cols)) = input.values.get(cell) {
            if rows * cols > 1 && items.len() == rows * cols {
                for row in 0..*rows {
                    for col in 0..*cols {
                        if row == 0 && col == 0 {
                            continue;
                        }
                        let target = CellRef {
                            row: cell.row + row as u32,
                            col: cell.col + col as u32,
                        };
                        spill_values.insert(target, &items[row * cols + col]);
                    }
                }
                array_owners.insert(
                    *cell,
                    ArrayOwner {
                        rows: *rows,
                        cols: *cols,
                    },
                );
            }
        }
    }

    let mut coordinates = BTreeSet::<CellRef>::new();
    coordinates.extend(sheet.cells.keys().copied());
    coordinates.extend(spill_values.keys().copied());
    for cell in sheet.styles.keys().chain(sheet.alignments.keys()) {
        if !XfKey::from_sheet(sheet, *cell).is_default() {
            coordinates.insert(*cell);
        }
    }

    let mut rows = BTreeMap::<u32, Vec<CellRef>>::new();
    for cell in &coordinates {
        rows.entry(cell.row).or_default().push(*cell);
    }
    for row in sheet.row_heights.keys() {
        rows.entry(*row).or_default();
    }

    let mut sheet_data = String::new();
    for (row, cells) in &rows {
        let mut open = format!("<row r=\"{}\"", row + 1);
        if let Some(height) = sheet.row_heights.get(row) {
            open.push_str(&format!(
                " ht=\"{}\" customHeight=\"1\"",
                trim_number(height * POINTS_PER_PIXEL)
            ));
        }
        if cells.is_empty() {
            sheet_data.push_str(&format!("{open}/>"));
            continue;
        }
        sheet_data.push_str(&open);
        sheet_data.push('>');
        for cell in cells {
            sheet_data.push_str(&render_cell(
                input,
                *cell,
                array_owners.get(cell),
                spill_values.get(cell).copied(),
                strings,
            ));
        }
        sheet_data.push_str("</row>");
    }

    let dimension = match (coordinates.first(), coordinates.last()) {
        (Some(_), Some(_)) => {
            let max_col = coordinates.iter().map(|cell| cell.col).max().unwrap_or(0);
            let max_row = coordinates.iter().map(|cell| cell.row).max().unwrap_or(0);
            let min_col = coordinates.iter().map(|cell| cell.col).min().unwrap_or(0);
            let min_row = coordinates.iter().map(|cell| cell.row).min().unwrap_or(0);
            format!(
                "{}{}:{}{}",
                column_letters(min_col as usize),
                min_row + 1,
                column_letters(max_col as usize),
                max_row + 1
            )
        }
        _ => "A1".to_string(),
    };

    let columns = if sheet.col_widths.is_empty() {
        String::new()
    } else {
        let items = sheet
            .col_widths
            .iter()
            .map(|(col, width)| {
                format!(
                    "<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>",
                    col + 1,
                    trim_number(width / PIXELS_PER_CHARACTER)
                )
            })
            .collect::<String>();
        format!("<cols>{items}</cols>")
    };

    let default_width = trim_number(crate::DEFAULT_COL_WIDTH / PIXELS_PER_CHARACTER);
    let default_height = trim_number(crate::DEFAULT_ROW_HEIGHT * POINTS_PER_PIXEL);
    let drawing = if input.has_drawing {
        "<drawing r:id=\"rId1\"/>"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"{MAIN_NS}\" xmlns:r=\"{REL_NS}\"><dimension ref=\"{dimension}\"/>{}<sheetFormatPr defaultColWidth=\"{default_width}\" defaultRowHeight=\"{default_height}\" customHeight=\"1\"/>{columns}<sheetData>{sheet_data}</sheetData><pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>{drawing}</worksheet>",
        sheet_views(sheet, input.selected)
    )
}

fn trim_number(value: f32) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn sheet_views(sheet: &Sheet, selected: bool) -> String {
    let tab = if selected { " tabSelected=\"1\"" } else { "" };
    let (rows, cols) = (sheet.freeze_rows, sheet.freeze_cols);
    let panes = if rows == 0 && cols == 0 {
        String::new()
    } else {
        let top_left = format!("{}{}", column_letters(cols as usize), rows + 1);
        let split = |name: &str, count: u32| {
            if count > 0 {
                format!(" {name}=\"{count}\"")
            } else {
                String::new()
            }
        };
        let active = match (rows > 0, cols > 0) {
            (true, true) => "bottomRight",
            (true, false) => "bottomLeft",
            _ => "topRight",
        };
        let mut selections = String::new();
        if rows > 0 && cols > 0 {
            selections.push_str("<selection pane=\"topRight\"/><selection pane=\"bottomLeft\"/>");
        }
        selections.push_str(&format!(
            "<selection pane=\"{active}\" activeCell=\"{top_left}\" sqref=\"{top_left}\"/>"
        ));
        format!(
            "<pane{}{} topLeftCell=\"{top_left}\" activePane=\"{active}\" state=\"frozen\"/>{selections}",
            split("xSplit", cols),
            split("ySplit", rows),
        )
    };
    format!("<sheetViews><sheetView{tab} workbookViewId=\"0\">{panes}</sheetView></sheetViews>")
}

fn render_cell(
    input: &WorksheetInput<'_>,
    cell: CellRef,
    array: Option<&ArrayOwner>,
    spilled: Option<&Value>,
    strings: &mut SharedStrings,
) -> String {
    let reference = format!("{}{}", column_letters(cell.col as usize), cell.row + 1);
    let key = XfKey::from_sheet(input.sheet, cell);
    let style = if key.is_default() {
        String::new()
    } else {
        input
            .style_ids
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, id)| format!(" s=\"{id}\""))
            .unwrap_or_default()
    };
    let raw = input.sheet.raw(cell).unwrap_or_default();
    let value = spilled.or_else(|| input.values.get(&cell));
    // Loom's grammar is smaller than Excel's, so "Loom could not evaluate it"
    // says nothing about whether Excel can: judge the text's structure.
    let unparseable = raw.starts_with('=') && !is_plausible_formula(raw);
    let is_formula = raw.starts_with('=') && !unparseable;

    let (kind, cached) = if unparseable {
        // Excel refuses a formula it cannot parse, so keep the text.
        (Some("s"), Some(strings.intern(raw).to_string()))
    } else {
        match value {
            // Loom has no result for this formula (or depends on one it
            // could not parse); store none so Excel calculates it.
            Some(Value::Error(CalcError::Parse)) if is_formula => (None, None),
            Some(value) => typed_value(value, is_formula, strings),
            None if is_formula => (Some("str"), Some(String::new())),
            None => (None, None),
        }
    };
    let kind = kind
        .map(|kind| format!(" t=\"{kind}\""))
        .unwrap_or_default();
    let formula = if is_formula {
        let text = excel_formula(raw, input.sheet_names);
        match array {
            Some(owner) => {
                let last = CellRef {
                    row: cell.row + owner.rows as u32 - 1,
                    col: cell.col + owner.cols as u32 - 1,
                };
                format!(
                    "<f t=\"array\" ref=\"{reference}:{}{}\">{}</f>",
                    column_letters(last.col as usize),
                    last.row + 1,
                    xml_escape_text(&text)
                )
            }
            None => format!("<f>{}</f>", xml_escape_text(&text)),
        }
    } else {
        String::new()
    };
    let cached = cached
        .map(|text| format!("<v>{text}</v>"))
        .unwrap_or_default();
    if formula.is_empty() && cached.is_empty() {
        format!("<c r=\"{reference}\"{style}/>")
    } else {
        format!("<c r=\"{reference}\"{style}{kind}>{formula}{cached}</c>")
    }
}
