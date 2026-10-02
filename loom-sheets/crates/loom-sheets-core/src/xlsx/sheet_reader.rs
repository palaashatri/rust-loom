//! Read a worksheet part and the shared-string table the way Excel writes
//! them: streaming XML, sparse cells, exact cell types, and the layout facts
//! (column widths, row heights, frozen panes, merged ranges) the model keeps
//! or has to report as lost.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::CellRef;

use super::cell_refs::parse_cell_ref;

/// Hostile-file guard: cells beyond the sheet limits are ignored and a sheet
/// with an absurd number of cells is refused instead of allocated.
const MAX_CELLS_PER_SHEET: usize = 10_000_000;

/// Shared-string table plus whether any entry carries per-run formatting.
#[derive(Debug, Default)]
pub(super) struct SharedStrings {
    pub(super) items: Vec<String>,
    /// Some entry formats part of its text (bold, colour, ...).
    pub(super) rich: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FormulaKind {
    Normal,
    Shared,
    Array,
}

#[derive(Debug, Clone)]
pub(super) struct SheetFormula {
    pub(super) kind: FormulaKind,
    /// Formula text without `=`; empty for a shared-formula member.
    pub(super) body: Option<String>,
    pub(super) shared_id: Option<u32>,
    /// `ref` attribute of an array formula, for example `F1:F2`.
    pub(super) array_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Cached {
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(String),
}

#[derive(Debug, Clone)]
pub(super) struct SheetCell {
    pub(super) at: CellRef,
    pub(super) style: usize,
    pub(super) value: Cached,
    pub(super) formula: Option<SheetFormula>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ColumnInfo {
    pub(super) first: u32,
    pub(super) last: u32,
    pub(super) width: Option<f32>,
    pub(super) custom: bool,
    pub(super) hidden: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RowInfo {
    pub(super) row: u32,
    pub(super) height: Option<f32>,
    pub(super) custom: bool,
    pub(super) hidden: bool,
}

#[derive(Debug, Default)]
pub(super) struct ParsedSheet {
    pub(super) cells: Vec<SheetCell>,
    pub(super) columns: Vec<ColumnInfo>,
    pub(super) rows: Vec<RowInfo>,
    pub(super) merged_ranges: usize,
    pub(super) frozen_rows: u32,
    pub(super) frozen_columns: u32,
    pub(super) default_row_height: Option<f32>,
}

fn local(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or("").to_string()
}

fn attributes(start: &BytesStart<'_>, path: &str) -> Result<Vec<(String, String)>, String> {
    let mut found = Vec::new();
    for attribute in start.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| format!("parse {path}: {error}"))?;
        let key = local(attribute.key.as_ref());
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|error| format!("parse {path}: {error}"))?
            .into_owned();
        found.push((key, value));
    }
    Ok(found)
}

fn find<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn flag(attrs: &[(String, String)], name: &str) -> bool {
    matches!(find(attrs, name), Some("1") | Some("true"))
}

/// XML newline normalisation, then `_xHHHH_` escapes (ECMA-376 22.4.2.4).
pub(super) fn decode_text(raw: &str) -> String {
    let normalised = raw.replace("\r\n", "\n").replace('\r', "\n");
    if !normalised.contains("_x") {
        return normalised;
    }
    let characters: Vec<char> = normalised.chars().collect();
    let mut out = String::with_capacity(normalised.len());
    let mut position = 0;
    while position < characters.len() {
        if characters[position] == '_'
            && characters.get(position + 1) == Some(&'x')
            && position + 6 < characters.len()
            && characters[position + 6] == '_'
        {
            let hex: String = characters[position + 2..position + 6].iter().collect();
            if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(decoded);
                position += 7;
                continue;
            }
        }
        out.push(characters[position]);
        position += 1;
    }
    out
}

fn push_reference(
    reader_text: &mut String,
    reference: &quick_xml::events::BytesRef<'_>,
    path: &str,
) -> Result<(), String> {
    let name = reference
        .decode()
        .map_err(|error| format!("parse {path}: {error}"))?;
    let resolved = match name.as_ref() {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => reference
            .resolve_char_ref()
            .map_err(|error| format!("parse {path}: {error}"))?
            .ok_or_else(|| format!("parse {path}: unknown entity &{name};"))?,
    };
    reader_text.push(resolved);
    Ok(())
}

/// Read `xl/sharedStrings.xml`. Phonetic guides (`rPh`) are not cell text.
pub(super) fn parse_shared_strings(xml: &str) -> Result<SharedStrings, String> {
    let path = "xl/sharedStrings.xml";
    let mut reader = Reader::from_str(xml);
    let mut table = SharedStrings::default();
    let mut stack: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut in_text = false;
    let mut formatted_run = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("parse {path}: {error}"))?;
        let (start, opens) = match &event {
            Event::Start(start) => (Some(start), true),
            Event::Empty(start) => (Some(start), false),
            _ => (None, false),
        };
        if let Some(start) = start {
            let name = local(start.name().as_ref());
            let under_phonetic = stack.iter().any(|open| open == "rPh");
            match name.as_str() {
                "si" => {
                    current = Some(String::new());
                    if !opens {
                        table.items.push(String::new());
                        current = None;
                    }
                }
                "t" if current.is_some() && !under_phonetic => in_text = opens,
                "b" | "i" | "u" | "strike" | "color" | "vertAlign"
                    if stack.iter().any(|open| open == "rPr") && !under_phonetic =>
                {
                    formatted_run = true;
                }
                _ => {}
            }
            if opens {
                stack.push(name);
            }
        }
        match event {
            Event::Text(text) if in_text => {
                if let Some(buffer) = current.as_mut() {
                    buffer.push_str(
                        &text
                            .decode()
                            .map_err(|error| format!("parse {path}: {error}"))?,
                    );
                }
            }
            Event::CData(text) if in_text => {
                if let Some(buffer) = current.as_mut() {
                    buffer.push_str(&String::from_utf8_lossy(&text.into_inner()));
                }
            }
            Event::GeneralRef(reference) if in_text => {
                if let Some(buffer) = current.as_mut() {
                    push_reference(buffer, &reference, path)?;
                }
            }
            Event::End(end) => {
                let name = local(end.name().as_ref());
                stack.pop();
                match name.as_str() {
                    "t" => in_text = false,
                    "si" => {
                        if let Some(text) = current.take() {
                            table.items.push(decode_text(&text));
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    table.rich = formatted_run;
    Ok(table)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    None,
    Value,
    Formula,
    Inline,
}

struct CellBuilder {
    at: Option<CellRef>,
    kind: String,
    style: usize,
    value: String,
    inline: String,
    formula: Option<SheetFormula>,
    formula_text: String,
}

fn cached_value(
    builder: &CellBuilder,
    shared: &SharedStrings,
    path: &str,
) -> Result<Cached, String> {
    let raw = builder.value.trim();
    Ok(match builder.kind.as_str() {
        "s" => {
            if raw.is_empty() {
                Cached::Empty
            } else {
                let index: usize = raw
                    .parse()
                    .map_err(|_| format!("{path}: shared-string index {raw:?} is not a number"))?;
                Cached::Text(shared.items.get(index).cloned().ok_or_else(|| {
                    format!(
                        "{path}: shared-string index {index} out of range ({} entries)",
                        shared.items.len()
                    )
                })?)
            }
        }
        "inlineStr" => Cached::Text(decode_text(&builder.inline)),
        "str" | "d" => Cached::Text(decode_text(&builder.value)),
        "b" => match raw {
            "" => Cached::Empty,
            "0" => Cached::Bool(false),
            _ => Cached::Bool(true),
        },
        "e" => Cached::Error(raw.to_string()),
        _ => {
            if raw.is_empty() {
                Cached::Empty
            } else {
                match raw.parse::<f64>() {
                    Ok(number) if number.is_finite() => Cached::Number(number),
                    _ => Cached::Text(raw.to_string()),
                }
            }
        }
    })
}

/// Parse one worksheet part.
pub(super) fn parse_worksheet(
    path: &str,
    xml: &str,
    shared: &SharedStrings,
) -> Result<ParsedSheet, String> {
    let mut reader = Reader::from_str(xml);
    let mut sheet = ParsedSheet::default();
    let mut stack: Vec<String> = Vec::new();
    let mut capture = Capture::None;
    let mut cell: Option<CellBuilder> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("parse {path}: {error}"))?;
        let (start, opens) = match &event {
            Event::Start(start) => (Some(start), true),
            Event::Empty(start) => (Some(start), false),
            _ => (None, false),
        };
        if let Some(start) = start {
            let name = local(start.name().as_ref());
            let attrs = attributes(start, path)?;
            let under_phonetic = stack.iter().any(|open| open == "rPh");
            match name.as_str() {
                "c" if stack.last().is_some_and(|parent| parent == "row") => {
                    let at = find(&attrs, "r").and_then(parse_cell_ref);
                    let builder = CellBuilder {
                        at,
                        kind: find(&attrs, "t").unwrap_or("n").to_string(),
                        style: find(&attrs, "s")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0),
                        value: String::new(),
                        inline: String::new(),
                        formula: None,
                        formula_text: String::new(),
                    };
                    if opens {
                        cell = Some(builder);
                    } else {
                        finish_cell(&mut sheet, builder, shared, path)?;
                    }
                }
                "f" if cell.is_some() => {
                    let kind = match find(&attrs, "t") {
                        Some("shared") => FormulaKind::Shared,
                        Some("array") => FormulaKind::Array,
                        _ => FormulaKind::Normal,
                    };
                    if let Some(builder) = cell.as_mut() {
                        builder.formula = Some(SheetFormula {
                            kind,
                            body: None,
                            shared_id: find(&attrs, "si").and_then(|value| value.parse().ok()),
                            array_ref: find(&attrs, "ref").map(str::to_string),
                        });
                    }
                    if opens {
                        capture = Capture::Formula;
                    }
                }
                "v" if cell.is_some() && opens => capture = Capture::Value,
                "t" if cell.is_some() && opens && !under_phonetic => capture = Capture::Inline,
                "row" => {
                    if let Some(row) = find(&attrs, "r").and_then(|value| value.parse::<u32>().ok())
                    {
                        sheet.rows.push(RowInfo {
                            row: row.saturating_sub(1),
                            height: find(&attrs, "ht").and_then(|value| value.parse().ok()),
                            custom: flag(&attrs, "customHeight"),
                            hidden: flag(&attrs, "hidden"),
                        });
                    }
                }
                "col" => {
                    let first = find(&attrs, "min").and_then(|value| value.parse::<u32>().ok());
                    let last = find(&attrs, "max").and_then(|value| value.parse::<u32>().ok());
                    if let (Some(first), Some(last)) = (first, last) {
                        sheet.columns.push(ColumnInfo {
                            first: first.saturating_sub(1),
                            last: last.saturating_sub(1),
                            width: find(&attrs, "width").and_then(|value| value.parse().ok()),
                            custom: flag(&attrs, "customWidth"),
                            hidden: flag(&attrs, "hidden"),
                        });
                    }
                }
                "mergeCell" => sheet.merged_ranges += 1,
                "sheetFormatPr" => {
                    sheet.default_row_height =
                        find(&attrs, "defaultRowHeight").and_then(|value| value.parse().ok());
                }
                "pane" => {
                    let frozen = matches!(find(&attrs, "state"), Some("frozen" | "frozenSplit"));
                    if frozen {
                        let split = |name: &str| {
                            find(&attrs, name)
                                .and_then(|value| value.parse::<f64>().ok())
                                .filter(|value| *value > 0.0)
                                .map_or(0, |value| value as u32)
                        };
                        sheet.frozen_columns = split("xSplit");
                        sheet.frozen_rows = split("ySplit");
                    }
                }
                _ => {}
            }
            if opens {
                stack.push(name);
            }
        }
        match event {
            Event::Text(text) => {
                if let Some(builder) = cell.as_mut() {
                    let text = text
                        .decode()
                        .map_err(|error| format!("parse {path}: {error}"))?;
                    append(builder, capture, &text);
                }
            }
            Event::CData(text) => {
                if let Some(builder) = cell.as_mut() {
                    append(
                        builder,
                        capture,
                        &String::from_utf8_lossy(&text.into_inner()),
                    );
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(builder) = cell.as_mut() {
                    let mut resolved = String::new();
                    push_reference(&mut resolved, &reference, path)?;
                    append(builder, capture, &resolved);
                }
            }
            Event::End(end) => {
                let name = local(end.name().as_ref());
                stack.pop();
                match name.as_str() {
                    "v" | "f" | "t" => capture = Capture::None,
                    "c" => {
                        if let Some(builder) = cell.take() {
                            finish_cell(&mut sheet, builder, shared, path)?;
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(sheet)
}

fn append(builder: &mut CellBuilder, capture: Capture, text: &str) {
    match capture {
        Capture::Value => builder.value.push_str(text),
        Capture::Formula => builder.formula_text.push_str(text),
        Capture::Inline => builder.inline.push_str(text),
        Capture::None => {}
    }
}

fn finish_cell(
    sheet: &mut ParsedSheet,
    builder: CellBuilder,
    shared: &SharedStrings,
    path: &str,
) -> Result<(), String> {
    // A cell without a usable coordinate cannot be placed.
    let Some(at) = builder.at else {
        return Ok(());
    };
    if sheet.cells.len() >= MAX_CELLS_PER_SHEET {
        return Err(format!(
            "{path}: more than {MAX_CELLS_PER_SHEET} cells exceeds the import limit"
        ));
    }
    let value = cached_value(&builder, shared, path)?;
    let mut formula = builder.formula;
    if let Some(formula) = formula.as_mut() {
        let body = decode_text(&builder.formula_text);
        if !body.trim().is_empty() {
            formula.body = Some(body);
        }
    }
    sheet.cells.push(SheetCell {
        at,
        style: builder.style,
        value,
        formula,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_and_line_endings_follow_the_spec() {
        assert_eq!(decode_text("a\r\nb"), "a\nb");
        assert_eq!(decode_text("a_x000D_b"), "a\rb");
        assert_eq!(decode_text("_x0041_BC"), "ABC");
        assert_eq!(decode_text("_x00ZZ_"), "_x00ZZ_");
    }

    #[test]
    fn shared_strings_skip_phonetic_runs_and_flag_formatting() {
        let xml = "<sst xmlns=\"x\"><si><t>plain</t></si>\
            <si><r><t>he</t></r><r><rPr><b/></rPr><t>llo</t></r><rPh sb=\"0\" eb=\"1\"><t>ZZ</t></rPh></si>\
            <si><t xml:space=\"preserve\"> a &amp; b </t></si><si/></sst>";
        let table = parse_shared_strings(xml).unwrap();
        assert_eq!(table.items, ["plain", "hello", " a & b ", ""]);
        assert!(table.rich);
        let plain = parse_shared_strings("<sst><si><t>x</t></si></sst>").unwrap();
        assert!(!plain.rich);
    }

    #[test]
    fn cells_keep_their_type_formula_and_layout() {
        let xml = "<worksheet><sheetViews><sheetView><pane xSplit=\"1\" ySplit=\"2\" state=\"frozen\"/></sheetView></sheetViews>\
            <cols><col min=\"1\" max=\"2\" width=\"20\" customWidth=\"1\"/></cols>\
            <sheetData><row r=\"1\" ht=\"30\" customHeight=\"1\">\
            <c r=\"A1\" t=\"s\" s=\"3\"><v>0</v></c>\
            <c r=\"B1\"><f>SUM(A2:A3)</f><v>5</v></c>\
            <c r=\"C1\" t=\"b\"><v>1</v></c>\
            <c r=\"D1\" t=\"e\"><v>#DIV/0!</v></c>\
            <c r=\"E1\" t=\"inlineStr\"><is><t>in&amp;line</t></is></c>\
            <c r=\"F1\" s=\"4\"/></row></sheetData>\
            <mergeCells count=\"1\"><mergeCell ref=\"A5:C5\"/></mergeCells></worksheet>";
        let shared = SharedStrings {
            items: vec!["hi".to_string()],
            rich: false,
        };
        let sheet = parse_worksheet("sheet1.xml", xml, &shared).unwrap();
        assert_eq!((sheet.frozen_rows, sheet.frozen_columns), (2, 1));
        assert_eq!(sheet.merged_ranges, 1);
        assert_eq!(sheet.columns[0].width, Some(20.0));
        assert!(sheet.columns[0].custom);
        assert_eq!(sheet.rows[0].height, Some(30.0));
        let values: Vec<_> = sheet.cells.iter().map(|cell| cell.value.clone()).collect();
        assert_eq!(
            values,
            [
                Cached::Text("hi".to_string()),
                Cached::Number(5.0),
                Cached::Bool(true),
                Cached::Error("#DIV/0!".to_string()),
                Cached::Text("in&line".to_string()),
                Cached::Empty,
            ]
        );
        assert_eq!(sheet.cells[0].style, 3);
        assert_eq!(
            sheet.cells[1].formula.as_ref().and_then(|f| f.body.clone()),
            Some("SUM(A2:A3)".to_string())
        );
    }
}
