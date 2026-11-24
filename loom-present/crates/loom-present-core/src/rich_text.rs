use crate::{PresentationSession, SlideElement};
use serde::{Deserialize, Serialize};

/// A contiguous run of text within a [`SlideElement`] with its own formatting.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyledTextRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl StyledTextRun {
    /// Creates a run inheriting all style properties from the parent element.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Creates a run initialized from an [`crate::ElementStyle`].
    pub fn from_element_style(text: impl Into<String>, style: &crate::ElementStyle) -> Self {
        Self {
            text: text.into(),
            font_family: style.font_family.clone(),
            font_size_pt: style.font_size_pt,
            bold: style.bold,
            italic: Some(style.italic),
            underline: Some(style.underline),
            strikethrough: Some(style.strikethrough),
            color: style.color.clone(),
        }
    }

    /// Returns true if two runs have identical formatting (ignoring `text`).
    pub fn same_style(&self, other: &Self) -> bool {
        self.font_family == other.font_family
            && self.font_size_pt == other.font_size_pt
            && self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
            && self.strikethrough == other.strikethrough
            && self.color == other.color
    }
}

/// Partial style update applied to a byte range of an element's text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextRangeStyleChange {
    pub font_family: Option<Option<String>>,
    pub font_size_pt: Option<Option<f32>>,
    pub bold: Option<Option<bool>>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub color: Option<Option<String>>,
}

impl TextRangeStyleChange {
    pub fn apply_to(&self, run: &mut StyledTextRun) {
        if let Some(ref ff) = self.font_family {
            run.font_family = ff.clone();
        }
        if let Some(fs) = self.font_size_pt {
            run.font_size_pt = fs;
        }
        if let Some(b) = self.bold {
            run.bold = b;
        }
        if let Some(i) = self.italic {
            run.italic = Some(i);
        }
        if let Some(u) = self.underline {
            run.underline = Some(u);
        }
        if let Some(s) = self.strikethrough {
            run.strikethrough = Some(s);
        }
        if let Some(ref c) = self.color {
            run.color = c.clone();
        }
    }
}

/// Coalesces adjacent runs with identical formatting and drops empty runs.
pub fn normalize_runs(runs: Vec<StyledTextRun>) -> Vec<StyledTextRun> {
    let mut out: Vec<StyledTextRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.same_style(&run) {
                last.text.push_str(&run.text);
                continue;
            }
        }
        out.push(run);
    }
    out
}

impl SlideElement {
    /// Returns the effective styled runs for this element.
    /// If `self.runs` is non-empty, returns it; otherwise synthesizes a single
    /// run from `self.content` and `self.style`.
    pub fn effective_runs(&self) -> Vec<StyledTextRun> {
        if !self.runs.is_empty() {
            return self.runs.clone();
        }
        if self.content.is_empty() {
            return Vec::new();
        }
        vec![StyledTextRun::from_element_style(
            self.content.clone(),
            &self.style,
        )]
    }

    /// Replaces `self.runs` with normalized `runs` and updates `self.content`
    /// to the concatenated text of all runs.
    pub fn set_runs(&mut self, runs: Vec<StyledTextRun>) {
        let normalized = normalize_runs(runs);
        self.content = normalized.iter().map(|r| r.text.as_str()).collect();
        self.runs = normalized;
    }

    /// Synchronizes `self.runs` after `self.content` was edited directly.
    /// Preserves the first run's style (or `self.style` if `runs` was empty)
    /// when `self.runs` no longer matches `self.content`.
    pub fn sync_runs_from_content(&mut self) {
        if self.runs.is_empty() {
            return;
        }
        let joined: String = self.runs.iter().map(|r| r.text.as_str()).collect();
        if joined == self.content {
            return;
        }
        if self.content.is_empty() {
            self.runs.clear();
            return;
        }
        let mut base = self.runs[0].clone();
        base.text = self.content.clone();
        self.runs = vec![base];
    }
}

/// Applies `change` to `[byte_start..byte_end)` of `runs` and returns the
/// updated normalized runs.
pub fn apply_style_to_range(
    runs: &[StyledTextRun],
    byte_start: usize,
    byte_end: usize,
    change: &TextRangeStyleChange,
) -> Vec<StyledTextRun> {
    if byte_start >= byte_end {
        return normalize_runs(runs.to_vec());
    }
    let mut result = Vec::with_capacity(runs.len() + 2);
    let mut cursor = 0usize;
    for run in runs {
        let len = run.text.len();
        let run_start = cursor;
        let run_end = cursor + len;
        cursor = run_end;

        if run_end <= byte_start || run_start >= byte_end {
            result.push(run.clone());
            continue;
        }

        let local_start = byte_start.saturating_sub(run_start);
        let local_end = (byte_end - run_start).min(len);

        // Snap to valid UTF-8 char boundaries.
        let mut s = local_start;
        while s > 0 && !run.text.is_char_boundary(s) {
            s -= 1;
        }
        let mut e = local_end;
        while e < len && !run.text.is_char_boundary(e) {
            e += 1;
        }

        if s > 0 {
            let mut before = run.clone();
            before.text = run.text[..s].to_string();
            result.push(before);
        }
        if s < e {
            let mut mid = run.clone();
            mid.text = run.text[s..e].to_string();
            change.apply_to(&mut mid);
            result.push(mid);
        }
        if e < len {
            let mut after = run.clone();
            after.text = run.text[e..].to_string();
            result.push(after);
        }
    }
    normalize_runs(result)
}

impl PresentationSession {
    /// Applies `change` to `[byte_start..byte_end)` of the specified element's
    /// text as a single undoable step.
    pub fn style_text_range(
        &mut self,
        element_id: &str,
        byte_start: usize,
        byte_end: usize,
        change: TextRangeStyleChange,
    ) -> Result<(), String> {
        let slide_idx = self.document.active_index;
        let mut updated = None;
        {
            let slide = self
                .document
                .slides
                .get(slide_idx)
                .ok_or_else(|| "Active slide index out of range".to_string())?;
            let elem = slide
                .elements
                .iter()
                .find(|e| e.id == element_id)
                .ok_or_else(|| format!("Element '{element_id}' not found"))?;
            let base_runs = elem.effective_runs();
            let new_runs = apply_style_to_range(&base_runs, byte_start, byte_end, &change);
            if new_runs != base_runs {
                updated = Some(new_runs);
            }
        }
        if let Some(new_runs) = updated {
            self.checkpoint();
            if let Some(slide) = self.document.slides.get_mut(slide_idx) {
                if let Some(elem) = slide.elements.iter_mut().find(|e| e.id == element_id) {
                    elem.set_runs(new_runs);
                }
            }
        }
        Ok(())
    }
}

/// Replaces `byte_start..byte_end` in `elem.content` while preserving `elem.runs`
/// styling for text outside the replaced span and inheriting the start run's style
/// for the replacement text.
pub fn replace_range_preserving_runs(
    elem: &mut crate::SlideElement,
    byte_start: usize,
    byte_end: usize,
    replacement: &str,
) {
    if byte_start > byte_end || byte_end > elem.content.len() {
        return;
    }
    if elem.runs.is_empty() {
        let mut new_content = elem.content.clone();
        new_content.replace_range(byte_start..byte_end, replacement);
        elem.content = new_content;
        return;
    }
    let runs = elem.effective_runs();
    let mut out: Vec<StyledTextRun> = Vec::with_capacity(runs.len() + 1);
    let mut cursor = 0usize;
    let mut inserted = false;
    for run in runs {
        let len = run.text.len();
        let run_start = cursor;
        let run_end = cursor + len;
        cursor = run_end;
        if run_end <= byte_start || run_start >= byte_end {
            if !inserted && run_start >= byte_start {
                if !replacement.is_empty() {
                    let mut rep = run.clone();
                    rep.text = replacement.to_string();
                    out.push(rep);
                }
                inserted = true;
            }
            out.push(run);
            continue;
        }
        let prefix_end = byte_start.saturating_sub(run_start).min(len);
        let suffix_start = byte_end.saturating_sub(run_start).min(len);
        if prefix_end > 0 {
            let mut prefix = run.clone();
            prefix.text = run.text[..prefix_end].to_string();
            out.push(prefix);
        }
        if !inserted {
            if !replacement.is_empty() {
                let mut rep = run.clone();
                rep.text = replacement.to_string();
                out.push(rep);
            }
            inserted = true;
        }
        if suffix_start < len {
            let mut suffix = run.clone();
            suffix.text = run.text[suffix_start..].to_string();
            out.push(suffix);
        }
    }
    if !inserted && !replacement.is_empty() {
        out.push(StyledTextRun::plain(replacement));
    }
    elem.set_runs(out);
}

/// Replaces `elem.content` with `new_text` while preserving `elem.runs`
/// styling for the unchanged common prefix and suffix.
pub fn replace_content_preserving_runs(elem: &mut crate::SlideElement, new_text: &str) {
    if elem.content == new_text {
        return;
    }
    let old = elem.content.clone();
    let prefix = old
        .char_indices()
        .zip(new_text.chars())
        .take_while(|((_, a), b)| a == b)
        .last()
        .map_or(0, |((i, a), _)| i + a.len_utf8());
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new_text[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    replace_range_preserving_runs(
        elem,
        prefix,
        old.len() - suffix,
        &new_text[prefix..new_text.len() - suffix],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_false_range_style_overrides_element_style() {
        let style = crate::ElementStyle {
            underline: true,
            ..Default::default()
        };
        let base = vec![StyledTextRun::from_element_style("hello", &style)];
        let updated = apply_style_to_range(
            &base,
            0,
            5,
            &TextRangeStyleChange {
                underline: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(updated[0].underline, Some(false));
    }

    #[test]
    fn replace_content_preserving_runs_keeps_tail_style() {
        let mut elem = crate::SlideElement {
            id: "e1".to_string(),
            element_type: crate::ElementType::BodyText,
            content: "Hello world".to_string(),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
            rotation_deg: 0.0,
            action: None,
            effects: None,
            group_id: None,
            locked: false,
            flip_h: false,
            flip_v: false,
            paragraphs: Vec::new(),
            style: Default::default(),
            runs: vec![
                StyledTextRun {
                    text: "Hello ".to_string(),
                    bold: Some(true),
                    ..Default::default()
                },
                StyledTextRun {
                    text: "world".to_string(),
                    color: Some("#FF0000".to_string()),
                    ..Default::default()
                },
            ],
        };
        replace_content_preserving_runs(&mut elem, "Hello world!");
        assert_eq!(elem.content, "Hello world!");
        assert_eq!(elem.runs[1].color.as_deref(), Some("#FF0000"));
    }
}
