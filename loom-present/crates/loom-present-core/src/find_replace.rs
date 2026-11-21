use crate::{ElementType, PresentationDocument, PresentationSession};

/// A single text match inside a [`PresentationDocument`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub slide_index: usize,
    pub element_id: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub matched_text: String,
}

fn is_searchable_text(kind: &ElementType) -> bool {
    !matches!(
        kind,
        ElementType::Picture | ElementType::Table | ElementType::Chart
    )
}

/// Finds all occurrences of `query` across all slides' text elements.
pub fn find_matches(
    doc: &PresentationDocument,
    query: &str,
    case_sensitive: bool,
) -> Vec<TextMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut matches = Vec::new();
    for (slide_index, slide) in doc.slides.iter().enumerate() {
        for elem in &slide.elements {
            if !is_searchable_text(&elem.element_type) || elem.content.is_empty() {
                continue;
            }
            let haystack = if case_sensitive {
                elem.content.clone()
            } else {
                elem.content.to_lowercase()
            };
            let mut offset = 0usize;
            while let Some(pos) = haystack[offset..].find(&needle) {
                let start = offset + pos;
                let end = start + needle.len();
                let matched_text = elem.content.get(start..end).unwrap_or("").to_string();
                matches.push(TextMatch {
                    slide_index,
                    element_id: elem.id.clone(),
                    byte_start: start,
                    byte_end: end,
                    matched_text,
                });
                offset = end.max(offset + 1);
            }
        }
    }
    matches
}

/// Replaces a single [`TextMatch`] with `replacement` as a single undoable step.
pub fn replace_match(
    session: &mut PresentationSession,
    m: &TextMatch,
    replacement: &str,
) -> Result<(), String> {
    let slide = session
        .document
        .slides
        .get(m.slide_index)
        .ok_or_else(|| "Slide index out of range".to_string())?;
    let elem = slide
        .elements
        .iter()
        .find(|e| e.id == m.element_id)
        .ok_or_else(|| format!("Element '{}' not found", m.element_id))?;
    if m.byte_end > elem.content.len()
        || !elem.content.is_char_boundary(m.byte_start)
        || !elem.content.is_char_boundary(m.byte_end)
    {
        return Err("Match range is no longer valid".to_string());
    }
    session.checkpoint();
    let elem_mut = session.document.slides[m.slide_index]
        .elements
        .iter_mut()
        .find(|e| e.id == m.element_id)
        .expect("verified element");
    elem_mut
        .content
        .replace_range(m.byte_start..m.byte_end, replacement);
    elem_mut.sync_runs_from_content();
    Ok(())
}

/// Replaces all occurrences of `query` with `replacement` across the entire
/// deck as a single undoable step, returning the number of replacements made.
pub fn replace_all(
    session: &mut PresentationSession,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) -> usize {
    let matches = find_matches(&session.document, query, case_sensitive);
    if matches.is_empty() {
        return 0;
    }
    let count = matches.len();
    session.checkpoint();
    // Apply in reverse order per element so byte offsets stay valid.
    for m in matches.into_iter().rev() {
        if let Some(slide) = session.document.slides.get_mut(m.slide_index) {
            if let Some(elem) = slide.elements.iter_mut().find(|e| e.id == m.element_id) {
                if m.byte_end <= elem.content.len()
                    && elem.content.is_char_boundary(m.byte_start)
                    && elem.content.is_char_boundary(m.byte_end)
                {
                    elem.content
                        .replace_range(m.byte_start..m.byte_end, replacement);
                    elem.sync_runs_from_content();
                }
            }
        }
    }
    count
}
