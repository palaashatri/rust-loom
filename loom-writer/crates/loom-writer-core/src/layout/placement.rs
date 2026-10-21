//! Putting wrapped lines on pages.
//!
//! Besides filling a page and starting the next, placement keeps a paragraph of
//! three or more lines from leaving one line alone at the bottom of a page
//! (orphan) or one line alone at the top of the next (widow), and ends a page
//! after a line that closes with a page break (form feed).

use std::ops::Range;

use super::WrappedLine;
use crate::layout::FlowLine;
use crate::PageRect;

pub(super) struct Placer {
    pub(super) lines: Vec<FlowLine>,
    pub(super) page_lines: Vec<Range<usize>>,
    pub(super) used: f32,
    usable: f32,
    first_line: usize,
    page: usize,
    break_pending: bool,
}

/// The lines of one block between two page breaks.
pub(super) struct Segment<'a> {
    pub(super) block_index: usize,
    pub(super) block_start: usize,
    pub(super) lines: &'a [WrappedLine],
    pub(super) line_height: f32,
    /// Width of every line when the block is a picture.
    pub(super) fixed_width: Option<f32>,
    /// Whether widow and orphan control applies (text, not pictures or tables).
    pub(super) controlled: bool,
    /// Whether the last line ends with a page break.
    pub(super) break_after: bool,
}

impl Placer {
    pub(super) fn new(usable: f32) -> Self {
        Self {
            lines: Vec::new(),
            page_lines: Vec::new(),
            used: 0.0,
            usable,
            first_line: 0,
            page: 0,
            break_pending: false,
        }
    }

    pub(super) fn has_content(&self) -> bool {
        self.lines.len() > self.first_line
    }

    pub(super) fn next_page(&mut self) {
        self.page_lines.push(self.first_line..self.lines.len());
        self.first_line = self.lines.len();
        self.page += 1;
        self.used = 0.0;
    }

    pub(super) fn finish(mut self) -> (Vec<FlowLine>, Vec<Range<usize>>) {
        self.page_lines.push(self.first_line..self.lines.len());
        (self.lines, self.page_lines)
    }

    /// Whether `count` more lines of `line_height` fit under what is used.
    pub(super) fn fits(&self, count: usize, line_height: f32) -> bool {
        self.fit(line_height, count) == count
    }

    /// How many of the next `most` lines fit below what the page already holds.
    fn fit(&self, line_height: f32, most: usize) -> usize {
        let mut used = self.used;
        let mut count = 0;
        while count < most && used + line_height <= self.usable + f32::EPSILON {
            used += line_height;
            count += 1;
        }
        count
    }

    pub(super) fn place(&mut self, segment: &Segment<'_>) {
        let total = segment.lines.len();
        let mut done = 0;
        while done < total {
            if std::mem::take(&mut self.break_pending) && self.has_content() {
                self.next_page();
            }
            let remaining = total - done;
            let mut take = self.fit(segment.line_height, remaining);
            if take < remaining && segment.controlled && total >= 3 {
                if remaining - take == 1 {
                    take = take.saturating_sub(1);
                    if take < 2 && done == 0 {
                        take = 0;
                    }
                }
                if done == 0 && take == 1 {
                    take = 0;
                }
            }
            if take == 0 {
                if self.has_content() {
                    self.next_page();
                    continue;
                }
                take = 1;
            }
            for line in &segment.lines[done..done + take] {
                self.lines.push(FlowLine {
                    block_index: segment.block_index,
                    start: line.start,
                    end: line.end,
                    global_start: segment.block_start + line.start,
                    page: self.page,
                    bounds: PageRect {
                        x: 0.0,
                        y: 0.0,
                        width: segment.fixed_width.unwrap_or(line.advance_pt),
                        height: 0.0,
                    },
                });
                self.used += segment.line_height;
            }
            done += take;
            if done < total {
                self.next_page();
            }
        }
        if segment.break_after {
            self.break_pending = true;
        }
    }
}
