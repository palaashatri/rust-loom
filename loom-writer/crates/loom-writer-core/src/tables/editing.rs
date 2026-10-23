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
    let mut updated = table.clone();
    for (index, (old_piece, new_piece)) in old_pieces.iter().zip(&new_pieces).enumerate() {
        if old_piece == new_piece {
            continue;
        }
        let cell = updated
            .cell_mut(index / columns, index % columns)
            .ok_or_else(|| "the table has no such cell".to_string())?;
        cell.content.runs = edit_runs(&cell.content.runs, old_piece, new_piece);
    }
    Ok(updated)
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
