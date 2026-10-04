//! Text overflow, as in every spreadsheet: left-aligned text runs on into the
//! empty cells to its right, right-aligned text into those on its left and
//! centered text into both, until a non-empty cell or the edge of the visible
//! window stops it. Numbers never overflow; they are clipped. Only the visible
//! window is examined, so scrolling stays constant-time.

use super::*;

/// Rough advance of one character as a fraction of the font size.
const CHAR_EM: f32 = 0.6;
const BOLD_EXTRA: f32 = 0.06;
const PADDING: f32 = 16.0;

pub(crate) struct CellSpill {
    pub items: Vec<SpillItem>,
    /// 0 plain, 1 drawn by the overflow layer, 2 number.
    pub kinds: Vec<i32>,
}

pub(crate) struct CellFace<'a> {
    pub cells: &'a [String],
    pub aligns: &'a [i32],
    pub bolds: &'a [bool],
    pub italics: &'a [bool],
    pub sizes: &'a [i32],
}

fn is_number(text: &str) -> bool {
    let plain: String = text
        .chars()
        .filter(|c| !matches!(c, '$' | ',' | '%' | '(' | ')' | ' ' | '\u{20ac}' | '\u{a3}'))
        .collect();
    !plain.is_empty() && plain.parse::<f64>().is_ok()
}

fn text_width(text: &str, size: f32, bold: bool) -> f32 {
    let em = CHAR_EM + if bold { BOLD_EXTRA } else { 0.0 };
    text.chars().count() as f32 * size * em + PADDING
}

pub(crate) fn compute(
    face: &CellFace,
    cols: usize,
    col_widths: &[f32],
    row_heights: &[f32],
    zoom: f32,
) -> CellSpill {
    let mut kinds = vec![0; face.cells.len()];
    let mut items = Vec::new();
    if cols == 0 || col_widths.len() < cols {
        return CellSpill { items, kinds };
    }
    let rows = face.cells.len() / cols;
    let mut starts = Vec::with_capacity(cols + 1);
    let mut x = 0.0;
    for width in &col_widths[..cols] {
        starts.push(x);
        x += width;
    }
    starts.push(x);
    let mut y = 0.0;
    for (row, &height) in row_heights.iter().enumerate().take(rows) {
        let mut claimed = vec![false; cols];
        for col in 0..cols {
            let idx = row * cols + col;
            let text = &face.cells[idx];
            if text.is_empty() {
                continue;
            }
            if is_number(text) {
                kinds[idx] = 2;
                continue;
            }
            let size = face.sizes.get(idx).copied().unwrap_or(14) as f32 * zoom;
            let bold = face.bolds.get(idx).copied().unwrap_or(false);
            let needed = text_width(text, size, bold);
            if needed <= col_widths[col] || claimed[col] {
                continue;
            }
            let align = face.aligns.get(idx).copied().unwrap_or(0);
            let free = |c: usize| face.cells[row * cols + c].is_empty() && !claimed[c];
            let (mut first, mut last) = (col, col);
            let mut width = col_widths[col];
            let mut prefer_right = align != 2;
            while width < needed {
                let can_right = align != 2 && last + 1 < cols && free(last + 1);
                let can_left = align != 0 && first > 0 && free(first - 1);
                let go_right = match (can_right, can_left) {
                    (false, false) => break,
                    (true, false) => true,
                    (false, true) => false,
                    (true, true) => prefer_right,
                };
                if go_right {
                    last += 1;
                    width += col_widths[last];
                } else {
                    first -= 1;
                    width += col_widths[first];
                }
                prefer_right = !prefer_right;
            }
            if first == last {
                continue;
            }
            for flag in &mut claimed[first..=last] {
                *flag = true;
            }
            kinds[idx] = 1;
            items.push(SpillItem {
                text: face.cells[idx].as_str().into(),
                x: starts[first],
                y,
                width: starts[last + 1] - starts[first],
                height,
                row: row as i32,
                col: col as i32,
                align,
                size,
                bold,
                italic: face.italics.get(idx).copied().unwrap_or(false),
            });
        }
        y += height;
    }
    CellSpill { items, kinds }
}
