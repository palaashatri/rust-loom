//! Formula authoring UX: function autocomplete, argument signature hints,
//! F4 reference absolute/relative cycling, and grid reference highlighting.

use std::rc::Rc;

use loom_sheets_core::{refs::normalize_sheet_name, CellRange, CellRef};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::formula_catalog::{
    format_signature, lookup_function, matching_functions, split_signature_for_arg,
};
use crate::{FormulaAssist, FormulaRefHighlight, FormulaSuggestion, GuiState, SheetsApp};

const MAX_SUGGESTIONS: usize = 8;
const MAX_HIGHLIGHTS: usize = 24;
const HIGHLIGHT_PALETTE_LEN: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignatureHint {
    pub func_name: String,
    pub prefix: String,
    pub active_param: String,
    pub suffix: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SuggestionItem {
    pub name: String,
    pub signature: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormulaCursorAnalysis {
    pub active: bool,
    pub prefix_span: Option<(usize, usize)>,
    pub suggestions: Vec<SuggestionItem>,
    pub signature: Option<SignatureHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HighlightRange {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    pub color_index: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedRefToken<'a> {
    abs_col: bool,
    col_str: &'a str,
    abs_row: bool,
    row_str: &'a str,
    cell: CellRef,
    end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FormulaRefSpan<'a> {
    start: usize,
    end: usize,
    sheet_qualifier: Option<String>,
    first: ParsedRefToken<'a>,
    second: Option<ParsedRefToken<'a>>,
}

fn clamp_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

fn scan_quoted_string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn scan_quoted_sheet_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                i += 2;
            } else {
                return Some(i + 1);
            }
        } else {
            i += 1;
        }
    }
    None
}

pub(crate) fn analyze_formula_cursor(
    formula: &str,
    cursor_byte: Option<usize>,
) -> FormulaCursorAnalysis {
    if !formula.starts_with('=') {
        return FormulaCursorAnalysis {
            active: false,
            prefix_span: None,
            suggestions: Vec::new(),
            signature: None,
        };
    }

    let cursor = clamp_char_boundary(formula, cursor_byte.unwrap_or(formula.len()));
    let bytes = formula.as_bytes();
    let mut stack: Vec<(Option<String>, usize)> = Vec::new();
    let mut i = 1usize;
    let mut in_string = false;

    while i < cursor {
        if bytes[i] == b'"' {
            let end = scan_quoted_string_end(bytes, i);
            if end > cursor || (end == bytes.len() && !formula[i..].ends_with('"')) {
                in_string = true;
                break;
            }
            i = end;
            continue;
        }

        if bytes[i] == b'\'' {
            if let Some(after) = scan_quoted_sheet_end(bytes, i) {
                if bytes.get(after) == Some(&b'!') && after < cursor {
                    i = after + 1;
                    continue;
                }
            }
            i += 1;
            continue;
        }

        if bytes[i].is_ascii_alphabetic() && (i == 1 || !is_ident_char(bytes[i - 1])) {
            let start = i;
            let mut j = i + 1;
            while j < cursor && is_ident_char(bytes[j]) {
                j += 1;
            }
            if j < cursor && bytes[j] == b'(' {
                stack.push((Some(formula[start..j].to_string()), 0));
                i = j + 1;
                continue;
            }
            i = j;
            continue;
        }

        match bytes[i] {
            b'(' => stack.push((None, 0)),
            b')' => {
                stack.pop();
            }
            b',' => {
                if let Some((_, arg_idx)) = stack.last_mut() {
                    *arg_idx += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if in_string {
        return FormulaCursorAnalysis {
            active: true,
            prefix_span: None,
            suggestions: Vec::new(),
            signature: None,
        };
    }

    let mut prefix_span = None;
    let mut suggestions = Vec::new();
    if cursor > 1 && is_ident_char(bytes[cursor - 1]) {
        let mut start = cursor - 1;
        while start > 1 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        let preceded_by_ref_marker = start > 1 && matches!(bytes[start - 1], b'!' | b'\'' | b'$');
        let followed_by_paren = bytes.get(cursor) == Some(&b'(');
        let followed_by_bang = bytes.get(cursor) == Some(&b'!');
        if !preceded_by_ref_marker
            && !followed_by_paren
            && !followed_by_bang
            && bytes[start].is_ascii_alphabetic()
        {
            let prefix = &formula[start..cursor];
            let docs = matching_functions(prefix, MAX_SUGGESTIONS);
            if !docs.is_empty() {
                prefix_span = Some((start, cursor));
                suggestions = docs
                    .into_iter()
                    .map(|doc| SuggestionItem {
                        name: doc.name.to_string(),
                        signature: format_signature(doc),
                        summary: doc.summary.to_string(),
                    })
                    .collect();
            }
        }
    }

    let signature = stack.iter().rev().find_map(|(maybe_name, arg_idx)| {
        let name = maybe_name.as_deref()?;
        let doc = lookup_function(name)?;
        let (prefix, active_param, suffix) = split_signature_for_arg(doc, *arg_idx);
        Some(SignatureHint {
            func_name: doc.name.to_string(),
            prefix,
            active_param,
            suffix,
            summary: doc.summary.to_string(),
        })
    });

    FormulaCursorAnalysis {
        active: true,
        prefix_span,
        suggestions,
        signature,
    }
}

pub(crate) fn apply_suggestion(
    formula: &str,
    cursor_byte: Option<usize>,
    func_name: &str,
) -> Option<(String, usize)> {
    let analysis = analyze_formula_cursor(formula, cursor_byte);
    let (start, end) = analysis.prefix_span?;
    let mut out = String::with_capacity(formula.len() + func_name.len() + 2);
    out.push_str(&formula[..start]);
    out.push_str(func_name);
    let has_open_paren = formula.as_bytes().get(end) == Some(&b'(');
    if !has_open_paren {
        out.push('(');
    }
    let new_cursor = out.len();
    out.push_str(&formula[end..]);
    Some((out, new_cursor))
}

fn parse_ref_token_at<'a>(
    formula: &'a str,
    bytes: &'a [u8],
    start: usize,
) -> Option<ParsedRefToken<'a>> {
    let mut i = start;
    let abs_col = bytes.get(i) == Some(&b'$');
    if abs_col {
        i += 1;
    }
    let col_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    let col_len = i - col_start;
    if col_len == 0 || col_len > 3 {
        return None;
    }
    let col_str = &formula[col_start..i];

    let abs_row = bytes.get(i) == Some(&b'$');
    if abs_row {
        i += 1;
    }
    let row_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let row_len = i - row_start;
    if row_len == 0 || row_len > 7 || bytes[row_start] == b'0' {
        return None;
    }
    let row_str = &formula[row_start..i];

    // Reject if immediately followed by identifier characters or `(` (function name) or `!` (sheet name).
    if let Some(&next) = bytes.get(i) {
        if is_ident_char(next) || next == b'(' || next == b'!' || next == b'$' {
            return None;
        }
    }

    let plain = format!("{col_str}{row_str}");
    let cell = CellRef::parse(&plain)?;
    Some(ParsedRefToken {
        abs_col,
        col_str,
        abs_row,
        row_str,
        cell,
        end: i,
    })
}

fn scan_formula_refs(formula: &str) -> Vec<FormulaRefSpan<'_>> {
    if !formula.starts_with('=') {
        return Vec::new();
    }
    let bytes = formula.as_bytes();
    let mut spans = Vec::new();
    let mut i = 1usize;

    while i < bytes.len() {
        if bytes[i] == b'"' {
            i = scan_quoted_string_end(bytes, i);
            continue;
        }

        let mut sheet_qualifier: Option<String> = None;
        let mut ref_start = i;

        // Quoted sheet qualifier: 'Sheet Name'!
        if bytes[i] == b'\'' {
            if let Some(after) = scan_quoted_sheet_end(bytes, i) {
                if bytes.get(after) == Some(&b'!') {
                    sheet_qualifier = Some(formula[i..after].to_string());
                    ref_start = after + 1;
                } else {
                    i += 1;
                    continue;
                }
            } else {
                i += 1;
                continue;
            }
        } else if bytes[i].is_ascii_alphabetic()
            && (i == 1 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        {
            // Check for unquoted sheet qualifier: Sheet1!
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
            {
                j += 1;
            }
            if j > i && bytes.get(j) == Some(&b'!') {
                sheet_qualifier = Some(formula[i..j].to_string());
                ref_start = j + 1;
            } else if bytes.get(j) == Some(&b'(') {
                // Function call, not a reference.
                i = j + 1;
                continue;
            }
        } else if bytes[i] != b'$' {
            i += 1;
            continue;
        }

        // Only start a cell reference at a clean boundary (unless preceded by `!`).
        if sheet_qualifier.is_none()
            && ref_start > 1
            && (bytes[ref_start - 1].is_ascii_alphanumeric()
                || bytes[ref_start - 1] == b'_'
                || bytes[ref_start - 1] == b'.')
        {
            i += 1;
            continue;
        }

        if let Some(first) = parse_ref_token_at(formula, bytes, ref_start) {
            let mut end = first.end;
            let mut second = None;
            if bytes.get(first.end) == Some(&b':') {
                if let Some(rhs) = parse_ref_token_at(formula, bytes, first.end + 1) {
                    end = rhs.end;
                    second = Some(rhs);
                }
            }
            spans.push(FormulaRefSpan {
                start: ref_start,
                end,
                sheet_qualifier,
                first,
                second,
            });
            i = end;
            continue;
        }

        i = if sheet_qualifier.is_some() {
            ref_start
        } else {
            i + 1
        };
    }

    spans
}

fn next_abs_mode(abs_col: bool, abs_row: bool) -> (bool, bool) {
    match (abs_col, abs_row) {
        (false, false) => (true, true),  // A1 -> $A$1
        (true, true) => (false, true),   // $A$1 -> A$1
        (false, true) => (true, false),  // A$1 -> $A1
        (true, false) => (false, false), // $A1 -> A1
    }
}

fn format_ref_token(tok: &ParsedRefToken<'_>, abs_col: bool, abs_row: bool) -> String {
    format!(
        "{}{}{}{}",
        if abs_col { "$" } else { "" },
        tok.col_str,
        if abs_row { "$" } else { "" },
        tok.row_str
    )
}

/// Cycle the cell/range reference at `cursor_byte` (or the last reference if
/// `cursor_byte` is `None`) through `$A$1` -> `A$1` -> `$A1` -> `A1`.
pub(crate) fn cycle_reference_in_formula(
    formula: &str,
    cursor_byte: Option<usize>,
) -> Option<(String, usize)> {
    let spans = scan_formula_refs(formula);
    if spans.is_empty() {
        return None;
    }

    let target = match cursor_byte {
        Some(pos) => {
            let pos = clamp_char_boundary(formula, pos);
            spans
                .iter()
                .find(|s| pos >= s.start && pos <= s.end)
                .or_else(|| spans.iter().rev().find(|s| s.end <= pos))?
        }
        None => spans.last()?,
    };

    let (next_col, next_row) = next_abs_mode(target.first.abs_col, target.first.abs_row);
    let replacement = match target.second {
        Some(second) => format!(
            "{}:{}",
            format_ref_token(&target.first, next_col, next_row),
            format_ref_token(&second, next_col, next_row)
        ),
        None => format_ref_token(&target.first, next_col, next_row),
    };

    let mut out = String::with_capacity(formula.len() + 4);
    out.push_str(&formula[..target.start]);
    out.push_str(&replacement);
    let new_cursor = out.len();
    out.push_str(&formula[target.end..]);
    Some((out, new_cursor))
}

pub(crate) fn extract_ref_highlights(
    formula: &str,
    active_sheet_name: &str,
) -> Vec<HighlightRange> {
    let spans = scan_formula_refs(formula);
    if spans.is_empty() {
        return Vec::new();
    }

    let active_norm = normalize_sheet_name(active_sheet_name);
    let mut highlights: Vec<HighlightRange> = Vec::new();

    for span in spans {
        if let Some(qual) = &span.sheet_qualifier {
            if normalize_sheet_name(qual) != active_norm {
                continue;
            }
        }
        let range = match span.second {
            Some(second) => CellRange::new(span.first.cell, second.cell),
            None => CellRange::new(span.first.cell, span.first.cell),
        };
        let already_present = highlights.iter().any(|h| {
            h.start_row == range.start.row
                && h.start_col == range.start.col
                && h.end_row == range.end.row
                && h.end_col == range.end.col
        });
        if already_present {
            continue;
        }
        let color_index = (highlights.len() as i32) % HIGHLIGHT_PALETTE_LEN;
        highlights.push(HighlightRange {
            start_row: range.start.row,
            start_col: range.start.col,
            end_row: range.end.row,
            end_col: range.end.col,
            color_index,
        });
        if highlights.len() >= MAX_HIGHLIGHTS {
            break;
        }
    }

    highlights
}

pub(crate) fn clear_assist(app: &SheetsApp) {
    let assist = app.global::<FormulaAssist>();
    assist.set_active(false);
    assist.set_popup_open(false);
    assist.set_suggestions(ModelRc::default());
    assist.set_selected_suggestion(0);
    assist.set_signature_prefix(SharedString::new());
    assist.set_signature_active(SharedString::new());
    assist.set_signature_suffix(SharedString::new());
    assist.set_signature_summary(SharedString::new());
    assist.set_ref_highlights(ModelRc::default());
}

fn current_caret_byte(app: &SheetsApp) -> Option<usize> {
    let offset = app.global::<FormulaAssist>().get_caret_byte_offset();
    (offset >= 0).then_some(offset as usize)
}

pub(crate) fn sync_assist(app: &SheetsApp, state: &GuiState, formula: &str) {
    if !formula.starts_with('=') {
        clear_assist(app);
        return;
    }

    let active_sheet_name = state.current.borrow().name.clone();
    let analysis = analyze_formula_cursor(formula, current_caret_byte(app));
    let highlights = extract_ref_highlights(formula, &active_sheet_name);

    let assist = app.global::<FormulaAssist>();
    assist.set_active(true);

    let suggestions_vec: Vec<FormulaSuggestion> = analysis
        .suggestions
        .iter()
        .map(|item| FormulaSuggestion {
            name: SharedString::from(item.name.as_str()),
            signature: SharedString::from(item.signature.as_str()),
            summary: SharedString::from(item.summary.as_str()),
        })
        .collect();
    let has_suggestions = !suggestions_vec.is_empty();
    let current_idx = assist.get_selected_suggestion();
    let clamped_idx = if suggestions_vec.is_empty() {
        0
    } else {
        current_idx.clamp(0, suggestions_vec.len() as i32 - 1)
    };
    assist.set_suggestions(ModelRc::from(Rc::new(VecModel::from(suggestions_vec))));
    assist.set_selected_suggestion(clamped_idx);
    assist.set_popup_open(has_suggestions);

    if let Some(sig) = analysis.signature {
        assist.set_signature_prefix(SharedString::from(sig.prefix.as_str()));
        assist.set_signature_active(SharedString::from(sig.active_param.as_str()));
        assist.set_signature_suffix(SharedString::from(sig.suffix.as_str()));
        assist.set_signature_summary(SharedString::from(sig.summary.as_str()));
    } else {
        assist.set_signature_prefix(SharedString::new());
        assist.set_signature_active(SharedString::new());
        assist.set_signature_suffix(SharedString::new());
        assist.set_signature_summary(SharedString::new());
    }

    let hl_vec: Vec<FormulaRefHighlight> = highlights
        .into_iter()
        .map(|h| FormulaRefHighlight {
            start_row: h.start_row as i32,
            start_col: h.start_col as i32,
            end_row: h.end_row as i32,
            end_col: h.end_col as i32,
            color_index: h.color_index,
        })
        .collect();
    assist.set_ref_highlights(ModelRc::from(Rc::new(VecModel::from(hl_vec))));
}

pub(crate) fn register_callbacks(app: &SheetsApp, state: &Rc<GuiState>) {
    let assist = app.global::<FormulaAssist>();

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        assist.on_formula_edited(move |text| {
            if let Some(app) = app_ref.upgrade() {
                sync_assist(&app, &state, text.as_str());
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        assist.on_caret_moved(move |offset| {
            if let Some(app) = app_ref.upgrade() {
                let assist = app.global::<FormulaAssist>();
                assist.set_caret_byte_offset(offset);
                let buf = app.get_formula_edit_buffer();
                if buf.starts_with('=') {
                    sync_assist(&app, &state, buf.as_str());
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        assist.on_editing_state_changed(move |editing| {
            if let Some(app) = app_ref.upgrade() {
                if editing {
                    let buf = app.get_formula_edit_buffer();
                    sync_assist(&app, &state, buf.as_str());
                } else {
                    clear_assist(&app);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        assist.on_accept_suggestion(move |idx| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let assist = app.global::<FormulaAssist>();
            let suggestions = assist.get_suggestions();
            let Some(item) = suggestions.row_data(idx as usize) else {
                return;
            };
            let current = app.get_formula_edit_buffer();
            if let Some((updated, new_cursor)) = apply_suggestion(
                current.as_str(),
                current_caret_byte(&app),
                item.name.as_str(),
            ) {
                app.set_formula_edit_buffer(SharedString::from(updated.as_str()));
                assist.set_caret_byte_offset(new_cursor as i32);
                assist.set_restore_caret_seq(assist.get_restore_caret_seq().wrapping_add(1));
                sync_assist(&app, &state, &updated);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        assist.on_dismiss_popup(move || {
            if let Some(app) = app_ref.upgrade() {
                let assist = app.global::<FormulaAssist>();
                assist.set_popup_open(false);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        assist.on_cycle_f4(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let current = app.get_formula_edit_buffer();
            if let Some((updated, new_cursor)) =
                cycle_reference_in_formula(current.as_str(), current_caret_byte(&app))
            {
                let assist = app.global::<FormulaAssist>();
                app.set_formula_edit_buffer(SharedString::from(updated.as_str()));
                assist.set_caret_byte_offset(new_cursor as i32);
                assist.set_restore_caret_seq(assist.get_restore_caret_seq().wrapping_add(1));
                sync_assist(&app, &state, &updated);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_formula_cursor, apply_suggestion, cycle_reference_in_formula,
        extract_ref_highlights, HighlightRange,
    };

    #[test]
    fn autocomplete_matches_prefix_and_applies_completion() {
        let analysis = analyze_formula_cursor("=su", None);
        assert!(analysis.active);
        let names: Vec<&str> = analysis
            .suggestions
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"SUM"));
        assert!(names.contains(&"SUMIF"));
        assert!(names.contains(&"SUMIFS"));

        let (applied, cursor) = apply_suggestion("=su", None, "SUM").unwrap();
        assert_eq!(applied, "=SUM(");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn nested_function_autocomplete_and_signature_hint_track_argument_index() {
        let analysis = analyze_formula_cursor("=IF(A1>0, av", None);
        assert_eq!(analysis.suggestions[0].name, "AVERAGE");
        let sig = analysis.signature.expect("IF signature");
        assert_eq!(sig.func_name, "IF");
        assert_eq!(sig.prefix, "IF(logical_test, ");
        assert_eq!(sig.active_param, "value_if_true");
        assert_eq!(sig.suffix, ", [value_if_false])");

        let (applied, _) = apply_suggestion("=IF(A1>0, av", None, "AVERAGE").unwrap();
        assert_eq!(applied, "=IF(A1>0, AVERAGE(");
    }

    #[test]
    fn signature_hint_ignores_commas_inside_strings_and_inner_calls() {
        let analysis = analyze_formula_cursor("=IF(SUM(A1, B1)>10, \"a, b\", ", None);
        let sig = analysis.signature.expect("outer IF signature");
        assert_eq!(sig.func_name, "IF");
        assert_eq!(sig.active_param, "[value_if_false]");
    }

    #[test]
    fn f4_cycles_single_cell_and_range_references() {
        let (f1, _) = cycle_reference_in_formula("=A1", None).unwrap();
        assert_eq!(f1, "=$A$1");
        let (f2, _) = cycle_reference_in_formula(&f1, None).unwrap();
        assert_eq!(f2, "=A$1");
        let (f3, _) = cycle_reference_in_formula(&f2, None).unwrap();
        assert_eq!(f3, "=$A1");
        let (f4, _) = cycle_reference_in_formula(&f3, None).unwrap();
        assert_eq!(f4, "=A1");

        let (r1, _) = cycle_reference_in_formula("=SUM(B2:D10)", Some(10)).unwrap();
        assert_eq!(r1, "=SUM($B$2:$D$10)");
        let (r2, _) = cycle_reference_in_formula(&r1, Some(10)).unwrap();
        assert_eq!(r2, "=SUM(B$2:D$10)");
    }

    #[test]
    fn f4_preserves_sheet_qualifier_and_ignores_non_formulas() {
        let (q1, _) = cycle_reference_in_formula("='Q1 Sales'!C3", None).unwrap();
        assert_eq!(q1, "='Q1 Sales'!$C$3");
        assert_eq!(cycle_reference_in_formula("A1", None), None);
        assert_eq!(cycle_reference_in_formula("=\"A1\"", None), None);
    }

    #[test]
    fn extract_highlights_filters_other_sheets_and_deduplicates() {
        let highlights = extract_ref_highlights(
            "=SUM(A1:B2, C3, A1:B2, 'Other Sheet'!D4, Sheet1!E5) + LOG10(C3)",
            "Sheet1",
        );
        assert_eq!(
            highlights,
            vec![
                HighlightRange {
                    start_row: 0,
                    start_col: 0,
                    end_row: 1,
                    end_col: 1,
                    color_index: 0,
                },
                HighlightRange {
                    start_row: 2,
                    start_col: 2,
                    end_row: 2,
                    end_col: 2,
                    color_index: 1,
                },
                HighlightRange {
                    start_row: 4,
                    start_col: 4,
                    end_row: 4,
                    end_col: 4,
                    color_index: 2,
                },
            ]
        );
    }
}
