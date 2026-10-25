//! PDF drawing for styled Writer text lines.

use crate::text_metrics::justification::LineGeometry;
use crate::text_metrics::{is_bold_weight, pdf_text_width};

/// One laid-out line, including its hanging list marker and inline runs.
pub(super) struct StyledLine<'a> {
    pub(super) x: f32,
    pub(super) marker_x: f32,
    pub(super) y: f32,
    pub(super) marker: &'a str,
    pub(super) text: &'a str,
    pub(super) text_start: usize,
    pub(super) runs: &'a [loom_text::StyleRun],
    pub(super) geometry: &'a LineGeometry,
}

/// Draw text fragments as one PDF text object, then draw their underline rules.
pub(super) fn draw_styled_line(
    pdf: &mut loom_pdf::PdfDocument,
    page: loom_pdf::PageIndex,
    line: &StyledLine<'_>,
    base: &loom_pdf::TextStyle,
) {
    use loom_pdf::{text_lines::PositionedTextRun, PathStyle};

    let mut positioned = Vec::new();
    if !line.marker.is_empty() {
        positioned.push(PositionedTextRun {
            x: line.marker_x,
            y: line.y,
            text: line.marker,
            style: base,
        });
        positioned.push(PositionedTextRun {
            x: line.marker_x + pdf_text_width(line.marker, base),
            y: line.y,
            text: " ",
            style: base,
        });
    }

    let mut cuts = vec![0, line.text.len()];
    for run in line.runs {
        for edge in [run.start, run.end] {
            if (line.text_start..=line.text_start + line.text.len()).contains(&edge)
                && line.text.is_char_boundary(edge - line.text_start)
            {
                cuts.push(edge - line.text_start);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();

    // Merge adjacent runs that draw alike so the PDF shaper sees the same
    // continuous text run as the editor.
    let mut pieces: Vec<(usize, usize, loom_pdf::TextStyle, bool, bool)> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let position = line.text_start + from;
        let run = line
            .runs
            .iter()
            .find(|run| run.start <= position && position < run.end);
        let mut style = base.clone();
        let (mut underline, mut strikethrough) = (false, false);
        if let Some(run) = run {
            style.bold |= is_bold_weight(run.style.weight);
            style.italic |= run.style.italic;
            style.size_pt = crate::run_size(&run.style, base.size_pt);
            underline = run.style.underline;
            strikethrough = run.style.strikethrough;
        }
        match pieces.last_mut() {
            Some(last)
                if last.1 == from
                    && last.2.bold == style.bold
                    && last.2.italic == style.italic
                    && last.2.size_pt == style.size_pt
                    && last.3 == underline
                    && last.4 == strikethrough =>
            {
                last.1 = to;
            }
            _ => pieces.push((from, to, style, underline, strikethrough)),
        }
    }

    let mut x = line.x;
    let mut underlines = Vec::new();
    for (from, to, style, underline, _) in &pieces {
        let mut ends: Vec<usize> = line
            .geometry
            .gaps
            .iter()
            .map(|(gap, _)| gap.end.saturating_sub(line.text_start))
            .filter(|end| *from < *end && *end < *to)
            .collect();
        ends.push(*to);
        ends.sort_unstable();
        ends.dedup();
        let mut start = *from;
        for end in ends {
            let piece = &line.text[start..end];
            positioned.push(PositionedTextRun {
                x,
                y: line.y,
                text: piece,
                style,
            });
            let width = pdf_text_width(piece, style);
            let expanded = line
                .geometry
                .extra_between(line.text_start + start, line.text_start + end);
            if *underline && !piece.trim().is_empty() {
                let rule_y = line.y - style.size_pt * 0.12;
                underlines.push((
                    x,
                    rule_y,
                    x + width + expanded,
                    PathStyle::stroked(style.fill_rgb, (style.size_pt / 24.0).max(0.5)),
                ));
            }
            x += width + expanded;
            start = end;
        }
    }

    pdf.draw_text_line(page, &positioned);
    for (x1, y, x2, style) in underlines {
        pdf.draw_line(page, x1, y, x2, y, style);
    }
}
