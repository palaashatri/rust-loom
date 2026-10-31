//! Editing a typed table through its flat editor text.
//!
//! The editor sends the whole table text after each change. [`apply_flat_text`]
//! turns that text back into cells: when the separators are unchanged, each
//! cell's text is diffed against its old text and the change is applied to the
//! cell's own runs, so bold and italic survive. [`flat_after_typing`] applies the
//! input rules before that: separators typed or pasted are dropped, line breaks
//! become spaces, and separators removed by a cross-cell delete are put back, so
//! a delete clears cell contents and never merges cells.

use std::ops::Range;

use loom_objects::table::{Table, TextRun};

use super::store::{flat_char, flat_table_text, is_table_separator};

/// Applies edited flat text to a table, cell by cell.
///
/// Returns an error, and changes nothing, when the separators differ from the
/// table's own: the edit would add or remove a row or a cell.
pub fn apply_flat_text(table: &Table, new_flat: &str) -> Result<Table, String> {
    let mut updated = table.clone();
    apply_flat_text_in_place(&mut updated, new_flat)?;
    Ok(updated)
}

/// Apply `new_flat` to `table` without cloning the whole grid first. The
/// separator and cell checks run before any cell changes, so an error leaves
/// the table untouched.
pub fn apply_flat_text_in_place(table: &mut Table, new_flat: &str) -> Result<(), String> {
    let old_flat = flat_table_text(table);
    let old_separators: Vec<char> = old_flat
        .chars()
        .filter(|c| is_table_separator(*c))
        .collect();
    let new_separators: Vec<char> = new_flat
        .chars()
        .filter(|c| is_table_separator(*c))
        .collect();
    if old_separators != new_separators {
        return Err("the edit would change the table's rows or cells".into());
    }
    let columns = table.col_count();
    let old_pieces: Vec<&str> = old_flat.split(is_table_separator).collect();
    let new_pieces: Vec<&str> = new_flat.split(is_table_separator).collect();
    if columns == 0 || old_pieces.len() != new_pieces.len() {
        return Err("the table has no cells to edit".into());
    }
    let changes: Vec<(usize, String, String)> = old_pieces
        .iter()
        .zip(&new_pieces)
        .enumerate()
        .filter(|(_, (old_piece, new_piece))| old_piece != new_piece)
        .map(|(index, (old_piece, new_piece))| {
            (index, (*old_piece).to_owned(), (*new_piece).to_owned())
        })
        .collect();
    if changes
        .iter()
        .any(|(index, _, _)| table.cell(index / columns, index % columns).is_none())
    {
        return Err("the table has no such cell".to_string());
    }
    for (index, old_piece, new_piece) in changes {
        let cell = table
            .cell_mut(index / columns, index % columns)
            .ok_or_else(|| "the table has no such cell".to_string())?;
        cell.content.runs = edit_runs(&cell.content.runs, &old_piece, &new_piece);
    }
    Ok(())
}

/// The whole table text after `inserted` replaces `range` of `old_flat`, and the
/// byte length of the text that was typed (the caret goes after it).
///
/// Line breaks in `inserted` become spaces; separators in `inserted` are dropped.
/// Separators inside `range` are put back right after the inserted text, so a
/// delete across cells clears each cell's part and keeps the grid.
pub(crate) fn flat_after_typing(
    old_flat: &str,
    range: Range<usize>,
    inserted: &str,
) -> (String, usize) {
    // A lone Enter does nothing inside a table; it keeps its selection too.
    if inserted == "\n" || inserted == "\r\n" {
        return (old_flat.to_string(), 0);
    }
    let kept: String = old_flat[range.clone()]
        .chars()
        .filter(|c| is_table_separator(*c))
        .collect();
    let typed: String = inserted
        .chars()
        .filter(|c| !is_table_separator(*c))
        .map(flat_char)
        .collect();
    let mut out = String::with_capacity(old_flat.len() + typed.len());
    out.push_str(&old_flat[..range.start]);
    out.push_str(&typed);
    out.push_str(&kept);
    out.push_str(&old_flat[range.end..]);
    (out, typed.len())
}

/// The runs of one cell after its text changes from `old` to `new`. Removed
/// characters leave their runs; inserted characters take the style of the
/// character before them (or after them, at the start of the text).
fn edit_runs(runs: &[TextRun], old: &str, new: &str) -> Vec<TextRun> {
    let mut templates: Vec<TextRun> = runs
        .iter()
        .map(|run| TextRun {
            text: String::new(),
            ..run.clone()
        })
        .collect();
    if templates.is_empty() {
        templates.push(TextRun::plain(""));
    }
    let mut chars: Vec<(char, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.text.chars().map(move |c| (flat_char(c), index)))
        .collect();
    let old_chars: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(a, b)| a == b)
        .count();
    let longest_suffix = old_chars.len().min(new_chars.len()) - prefix;
    let suffix = old_chars
        .iter()
        .rev()
        .zip(new_chars.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(longest_suffix);
    let removed = old_chars.len() - prefix - suffix;
    let inserted = &new_chars[prefix..new_chars.len() - suffix];
    let style = if prefix > 0 {
        chars.get(prefix - 1)
    } else {
        chars.get(prefix)
    }
    .map_or(0, |&(_, index)| index);
    chars.splice(
        prefix..prefix + removed,
        inserted.iter().map(|&c| (c, style)),
    );
    group(&chars, &templates)
}

/// Regroups characters into runs, merging neighbours that share a style.
fn group(chars: &[(char, usize)], templates: &[TextRun]) -> Vec<TextRun> {
    let mut out: Vec<TextRun> = Vec::new();
    let mut last_style: Option<usize> = None;
    for &(c, style) in chars {
        match out.last_mut() {
            Some(run) if last_style == Some(style) => run.text.push(c),
            _ => {
                let mut run = templates[style].clone();
                run.text = c.to_string();
                out.push(run);
                last_style = Some(style);
            }
        }
    }
    out.retain(|run| !run.text.is_empty());
    out
}

/// The bytes of one cell's text that a selection covers, as cell-local offsets.
/// An empty cell is covered when the selection starts at or before it and
/// reaches past it, so a selection across cells styles the empty ones too.
pub(crate) fn cell_part(
    cell_start: usize,
    cell_len: usize,
    scope: &Range<usize>,
) -> Option<Range<usize>> {
    if cell_len == 0 {
        return (scope.start <= cell_start && cell_start < scope.end).then_some(0..0);
    }
    let from = scope.start.max(cell_start);
    let to = scope.end.min(cell_start + cell_len);
    (from < to).then(|| (from - cell_start)..(to - cell_start))
}

/// Whether the runs of one cell have `has` everywhere in `part`. A cell with no
/// text is covered by one of its empty runs carrying the style.
pub(crate) fn runs_cover(
    runs: &[TextRun],
    part: Range<usize>,
    has: &dyn Fn(&TextRun) -> bool,
) -> bool {
    let total: usize = runs.iter().map(|run| run.text.len()).sum();
    if total == 0 {
        return runs.iter().any(has);
    }
    let mut offset = 0;
    for run in runs {
        let (start, end) = (offset, offset + run.text.len());
        offset = end;
        if part.start.max(start) < part.end.min(end) && !has(run) {
            return false;
        }
    }
    true
}

/// The runs of one cell after `change` applies to the bytes `part` of its text.
/// Runs are split at the part's edges and neighbours with equal style are
/// merged. A cell with no text keeps one empty run that carries the style, so
/// text typed into it takes the style.
pub(crate) fn restyle_runs(
    runs: &[TextRun],
    part: Range<usize>,
    change: &dyn Fn(&mut TextRun),
) -> Vec<TextRun> {
    let total: usize = runs.iter().map(|run| run.text.len()).sum();
    if total == 0 {
        let mut run = runs.first().cloned().unwrap_or_else(|| TextRun::plain(""));
        run.text.clear();
        change(&mut run);
        return if run == TextRun::plain("") {
            Vec::new()
        } else {
            vec![run]
        };
    }
    let mut pieces: Vec<TextRun> = Vec::with_capacity(runs.len() + 2);
    let mut offset = 0;
    for run in runs {
        let len = run.text.len();
        let start = offset;
        offset += len;
        let from = char_edge(&run.text, part.start.saturating_sub(start));
        let to = char_edge(&run.text, part.end.saturating_sub(start));
        if to <= from {
            pieces.push(run.clone());
            continue;
        }
        pieces.extend(piece_of(run, 0, from, None));
        pieces.extend(piece_of(run, from, to, Some(change)));
        pieces.extend(piece_of(run, to, len, None));
    }
    let mut out: Vec<TextRun> = Vec::new();
    for piece in pieces {
        if piece.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if same_style(last, &piece) => last.text.push_str(&piece.text),
            _ => out.push(piece),
        }
    }
    out
}

/// `offset` clamped to the text and moved back to a character boundary.
fn char_edge(text: &str, offset: usize) -> usize {
    let mut index = offset.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// A copy of `run` holding bytes `from..to` of its text, changed by `change`
/// when given. `None` for an empty stretch.
fn piece_of(
    run: &TextRun,
    from: usize,
    to: usize,
    change: Option<&dyn Fn(&mut TextRun)>,
) -> Option<TextRun> {
    (from < to).then(|| {
        let mut piece = run.clone();
        piece.text = run.text[from..to].to_string();
        if let Some(change) = change {
            change(&mut piece);
        }
        piece
    })
}

/// Whether two runs share their formatting, ignoring their text.
fn same_style(a: &TextRun, b: &TextRun) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
        && a.superscript == b.superscript
        && a.subscript == b.subscript
        && a.font_size_pt == b.font_size_pt
        && a.font_family == b.font_family
        && a.color == b.color
}
