use crate::{ElementType, PresentationDocument, PresentationSession};

/// A single text match inside a [`PresentationDocument`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub slide_index: usize,
    pub element_id: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub matched_text: String,
    pub is_notes: bool,
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
    find_matches_with_notes(doc, query, case_sensitive, false)
}

/// Finds all occurrences of `query` across all slides' text elements, and
/// optionally speaker notes when `include_notes` is true.
pub fn find_matches_with_notes(
    doc: &PresentationDocument,
    query: &str,
    case_sensitive: bool,
    include_notes: bool,
) -> Vec<TextMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (slide_index, slide) in doc.slides.iter().enumerate() {
        for elem in &slide.elements {
            if !is_searchable_text(&elem.element_type) || elem.content.is_empty() {
                continue;
            }
            for (start, end) in find_text_ranges(&elem.content, query, case_sensitive) {
                let matched_text = elem.content[start..end].to_string();
                matches.push(TextMatch {
                    slide_index,
                    element_id: elem.id.clone(),
                    byte_start: start,
                    byte_end: end,
                    matched_text,
                    is_notes: false,
                });
            }
        }
        if include_notes && !slide.speaker_notes.is_empty() {
            for (start, end) in find_text_ranges(&slide.speaker_notes, query, case_sensitive) {
                let matched_text = slide.speaker_notes[start..end].to_string();
                matches.push(TextMatch {
                    slide_index,
                    element_id: "__notes__".to_string(),
                    byte_start: start,
                    byte_end: end,
                    matched_text,
                    is_notes: true,
                });
            }
        }
    }
    matches
}

fn find_text_ranges(text: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    if query.is_empty() {
        return ranges;
    }
    if case_sensitive {
        let mut offset = 0usize;
        while let Some(pos) = text[offset..].find(query) {
            let start = offset + pos;
            let end = start + query.len();
            ranges.push((start, end));
            offset = end;
        }
        return ranges;
    }
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    if query_chars.is_empty() {
        return ranges;
    }
    let mut lowered_chars: Vec<char> = Vec::new();
    let mut char_to_orig_start: Vec<usize> = Vec::new();
    for (orig_start, ch) in text.char_indices() {
        for lc in ch.to_lowercase() {
            lowered_chars.push(lc);
            char_to_orig_start.push(orig_start);
        }
    }
    char_to_orig_start.push(text.len());
    let mut i = 0usize;
    while i + query_chars.len() <= lowered_chars.len() {
        if lowered_chars[i..i + query_chars.len()] == query_chars[..] {
            let start = char_to_orig_start[i];
            let end = char_to_orig_start[i + query_chars.len()];
            if text.is_char_boundary(start) && text.is_char_boundary(end) {
                ranges.push((start, end));
            }
            i += query_chars.len();
        } else {
            i += 1;
        }
    }
    ranges
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
    if m.is_notes {
        if m.byte_start > m.byte_end
            || m.byte_end > slide.speaker_notes.len()
            || !slide.speaker_notes.is_char_boundary(m.byte_start)
            || !slide.speaker_notes.is_char_boundary(m.byte_end)
            || slide.speaker_notes[m.byte_start..m.byte_end] != m.matched_text
        {
            return Err("Match range is no longer valid".to_string());
        }
        session.checkpoint();
        session.document.slides[m.slide_index]
            .speaker_notes
            .replace_range(m.byte_start..m.byte_end, replacement);
        return Ok(());
    }
    let elem = slide
        .elements
        .iter()
        .find(|e| e.id == m.element_id)
        .ok_or_else(|| format!("Element '{}' not found", m.element_id))?;
    if m.byte_start > m.byte_end
        || m.byte_end > elem.content.len()
        || !elem.content.is_char_boundary(m.byte_start)
        || !elem.content.is_char_boundary(m.byte_end)
        || elem.content[m.byte_start..m.byte_end] != m.matched_text
    {
        return Err("Match range is no longer valid".to_string());
    }
    session.checkpoint();
    let elem_mut = session.document.slides[m.slide_index]
        .elements
        .iter_mut()
        .find(|e| e.id == m.element_id)
        .expect("verified element");
    crate::rich_text::replace_range_preserving_runs(
        elem_mut,
        m.byte_start,
        m.byte_end,
        replacement,
    );
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
    replace_all_with_notes(session, query, replacement, case_sensitive, false)
}

/// Replaces all occurrences of `query` with `replacement` across the entire
/// deck (and optionally speaker notes) as a single undoable step.
pub fn replace_all_with_notes(
    session: &mut PresentationSession,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
    include_notes: bool,
) -> usize {
    let matches = find_matches_with_notes(&session.document, query, case_sensitive, include_notes);
    if matches.is_empty() {
        return 0;
    }
    let count = matches.len();
    session.checkpoint();
    // Apply in reverse order per element/notes so byte offsets stay valid.
    for m in matches.into_iter().rev() {
        if let Some(slide) = session.document.slides.get_mut(m.slide_index) {
            if m.is_notes {
                if m.byte_end <= slide.speaker_notes.len()
                    && slide.speaker_notes.is_char_boundary(m.byte_start)
                    && slide.speaker_notes.is_char_boundary(m.byte_end)
                {
                    slide
                        .speaker_notes
                        .replace_range(m.byte_start..m.byte_end, replacement);
                }
            } else if let Some(elem) = slide.elements.iter_mut().find(|e| e.id == m.element_id) {
                if m.byte_end <= elem.content.len()
                    && elem.content.is_char_boundary(m.byte_start)
                    && elem.content.is_char_boundary(m.byte_end)
                {
                    crate::rich_text::replace_range_preserving_runs(
                        elem,
                        m.byte_start,
                        m.byte_end,
                        replacement,
                    );
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_and_replace_with_notes_is_real_and_undoable() {
        let mut doc = PresentationDocument::new("d", "Deck");
        doc.slides[0].speaker_notes = "Speaker note about alpha and alpha".to_string();
        let mut session = PresentationSession::new(doc);

        assert!(find_matches_with_notes(&session.document, "alpha", false, false).is_empty());
        let matches = find_matches_with_notes(&session.document, "alpha", false, true);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.is_notes));

        replace_match(&mut session, &matches[0], "beta").expect("replace note match");
        assert_eq!(
            session.document.slides[0].speaker_notes,
            "Speaker note about beta and alpha"
        );

        let count = replace_all_with_notes(&mut session, "alpha", "gamma", false, true);
        assert_eq!(count, 1);
        assert_eq!(
            session.document.slides[0].speaker_notes,
            "Speaker note about beta and gamma"
        );

        assert!(session.undo());
        assert_eq!(
            session.document.slides[0].speaker_notes,
            "Speaker note about beta and alpha"
        );
    }
}
