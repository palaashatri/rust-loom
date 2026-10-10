//! Text preparation shared by drawing and measuring.
//!
//! [`prepare`] turns the caller's string into the characters that are actually
//! set: invisible format controls are removed, emoji sequences collapse to
//! their base character, and the rest is put in canonical (NFC) form, so
//! `e` followed by U+0301 is one precomposed glyph. Drawing and
//! [`crate::text_width_pt`] both start here, so a measured run is the run that
//! is drawn.

use std::borrow::Cow;

use crate::compose_data::{COMBINING_CLASS, PAIRS, SINGLETONS};

/// Characters that never reach the page: C0/C1 controls and the Unicode
/// default-ignorable code points (soft hyphen, zero-width space, joiners and
/// directional marks, word joiner and invisible operators, variation
/// selectors, BOM, tag characters), plus the line and paragraph separators.
/// None of them draws a glyph or takes any room.
pub(crate) fn is_not_drawn(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00..=0x1F
            | 0x7F..=0x9F
            | 0x00AD
            | 0x034F
            | 0x061C
            | 0x115F
            | 0x1160
            | 0x17B4
            | 0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFFB
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// Fitzpatrick skin-tone modifiers.
fn is_emoji_modifier(ch: char) -> bool {
    matches!(ch as u32, 0x1F3FB..=0x1F3FF)
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1F1E6..=0x1F1FF)
}

/// Symbols and pictographs that emoji sequences are built from. Arrows and
/// letterlike symbols are deliberately left out: Inter draws those itself.
fn is_pictographic(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF | 0x2B00..=0x2BFF
    )
}

/// The characters of an emoji or flag sequence reduce to one: the base emoji
/// (drawn as Inter's `.notdef` box unless a fallback font has it). Skin tones,
/// ZWJ-joined members, the second half of a flag, tag sequences and keycaps
/// add nothing visible of their own, so they are dropped rather than printed
/// as a row of boxes. Text extraction therefore sees only the base character.
fn simplify(text: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    let mut joining = false;
    let mut regional = 0usize;
    for ch in text.chars() {
        if ch == '\u{200D}' {
            joining = out
                .last()
                .is_some_and(|previous| is_pictographic(*previous));
            continue;
        }
        if is_not_drawn(ch) {
            continue;
        }
        let joined = std::mem::take(&mut joining);
        let previous = out.last().copied();
        if joined && is_pictographic(ch) {
            continue;
        }
        if is_emoji_modifier(ch) && previous.is_some_and(is_pictographic) {
            continue;
        }
        if ch == '\u{20E3}' && previous.is_some_and(|p| matches!(p, '0'..='9' | '#' | '*')) {
            continue;
        }
        if is_regional_indicator(ch) {
            regional += 1;
            if regional % 2 == 0 {
                continue;
            }
        } else {
            regional = 0;
        }
        out.push(ch);
    }
    out
}

/// Canonical combining class (0 for a starter or an unlisted mark).
fn combining_class(ch: char) -> u8 {
    let Ok(code) = u16::try_from(u32::from(ch)) else {
        return 0;
    };
    COMBINING_CLASS
        .binary_search_by_key(&code, |entry| entry.0)
        .map_or(0, |at| COMBINING_CLASS[at].1)
}

/// The precomposed character for `first` followed by `second`, if one exists.
fn composition(first: char, second: char) -> Option<char> {
    let first = u16::try_from(u32::from(first)).ok()?;
    let second = u16::try_from(u32::from(second)).ok()?;
    let at = PAIRS
        .binary_search_by_key(&(first, second), |entry| (entry.0, entry.1))
        .ok()?;
    char::from_u32(u32::from(PAIRS[at].2))
}

/// A canonical singleton replacement (U+2126 OHM SIGN is U+03A9, and so on).
fn singleton(ch: char) -> Option<char> {
    let code = u16::try_from(u32::from(ch)).ok()?;
    let at = SINGLETONS
        .binary_search_by_key(&code, |entry| entry.0)
        .ok()?;
    char::from_u32(u32::from(SINGLETONS[at].1))
}

/// NFC for the Basic Multilingual Plane: singleton replacement, canonical
/// reordering of mark runs, then the standard blocked-composition pass over
/// the primary composition pairs (UAX #15). Hangul syllable composition and
/// compositions involving supplementary-plane characters are not applied.
fn compose(chars: Vec<char>) -> Vec<char> {
    let mut chars: Vec<char> = chars
        .into_iter()
        .map(|ch| singleton(ch).unwrap_or(ch))
        .collect();
    let mut at = 0;
    while at < chars.len() {
        if combining_class(chars[at]) == 0 {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && combining_class(chars[at]) != 0 {
            at += 1;
        }
        chars[start..at].sort_by_key(|ch| combining_class(*ch));
    }
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut starter: Option<usize> = None;
    let mut last_class = 0u8;
    for ch in chars {
        let class = combining_class(ch);
        if let Some(at) = starter {
            if let Some(composed) = composition(out[at], ch) {
                if last_class < class || last_class == 0 {
                    out[at] = composed;
                    continue;
                }
            }
        }
        if class == 0 {
            starter = Some(out.len());
        }
        last_class = class;
        out.push(ch);
    }
    out
}

/// The characters of `text` that are set: see the module notes.
pub(crate) fn prepare(text: &str) -> Cow<'_, str> {
    if text.bytes().all(|b| (0x20..0x7F).contains(&b)) {
        return Cow::Borrowed(text);
    }
    let prepared: String = compose(simplify(text)).into_iter().collect();
    if prepared == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(prepared)
    }
}

/// Plain characters a symbol Inter lacks is shown as: the typographic
/// ligature code points (Inter ligates through OpenType features but has no
/// glyphs for the presentation forms), the fixed-width spaces, the leader
/// dots and the full-width ASCII block. Used only when no installed fallback
/// font covers the character; the result keeps the text legible and
/// searchable instead of printing a box.
pub(crate) fn compat_expansion(ch: char) -> Option<([char; 3], usize)> {
    const NUL: char = '\0';
    let code = ch as u32;
    Some(match code {
        0xFB00 => (['f', 'f', NUL], 2),
        0xFB01 => (['f', 'i', NUL], 2),
        0xFB02 => (['f', 'l', NUL], 2),
        0xFB03 => (['f', 'f', 'i'], 3),
        0xFB04 => (['f', 'f', 'l'], 3),
        0xFB05 | 0xFB06 => (['s', 't', NUL], 2),
        0x2000..=0x200A | 0x202F | 0x205F | 0x3000 => ([' ', NUL, NUL], 1),
        0x2024 => (['.', NUL, NUL], 1),
        0x2025 => (['.', '.', NUL], 2),
        0xFF01..=0xFF5E => ([char::from_u32(code - 0xFEE0)?, NUL, NUL], 1),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(text: &str) -> String {
        prepare(text).into_owned()
    }

    #[test]
    fn ascii_is_returned_untouched_without_allocating() {
        assert!(matches!(prepare("Hello, world 123"), Cow::Borrowed(_)));
        assert!(matches!(prepare("caf\u{e9}"), Cow::Borrowed(_)));
    }

    #[test]
    fn invisible_format_controls_are_removed() {
        for ch in [
            '\u{ad}',
            '\u{200b}',
            '\u{200c}',
            '\u{200e}',
            '\u{200f}',
            '\u{202a}',
            '\u{202e}',
            '\u{2060}',
            '\u{2064}',
            '\u{2066}',
            '\u{2069}',
            '\u{2028}',
            '\u{2029}',
            '\u{fe0f}',
            '\u{feff}',
            '\u{e0020}',
            '\u{e0100}',
        ] {
            assert_eq!(prepared(&format!("a{ch}b")), "ab", "U+{:04X}", ch as u32);
            assert!(is_not_drawn(ch));
        }
        assert!(!is_not_drawn('a'));
        assert!(!is_not_drawn('\u{a0}'), "no-break space is drawn");
        assert!(!is_not_drawn('\u{202f}'), "narrow no-break space is drawn");
    }

    #[test]
    fn a_base_letter_and_its_mark_become_one_precomposed_character() {
        assert_eq!(prepared("e\u{301}"), "\u{e9}");
        assert_eq!(prepared("A\u{30a}"), "\u{c5}");
        assert_eq!(prepared("n\u{303}"), "\u{f1}");
        // Greek tonos and Cyrillic breve.
        assert_eq!(prepared("\u{3b1}\u{301}"), "\u{3ac}");
        assert_eq!(prepared("\u{438}\u{306}"), "\u{439}");
        // Marks are put in canonical order first: dot below sorts before
        // circumflex, so both spellings give the same Vietnamese letter.
        assert_eq!(prepared("a\u{302}\u{323}"), "\u{1ead}");
        assert_eq!(prepared("a\u{323}\u{302}"), "\u{1ead}");
        // A precomposed character plus another mark composes again.
        assert_eq!(prepared("\u{1ea1}\u{302}"), "\u{1ead}");
        // Polytonic Greek needs a second composition step.
        assert_eq!(prepared("\u{3b1}\u{313}\u{301}"), "\u{1f04}");
    }

    #[test]
    fn a_mark_that_cannot_compose_is_kept_after_its_base() {
        assert_eq!(prepared("x\u{301}"), "x\u{301}");
        // Two marks of the same class on one base: the second is blocked.
        assert_eq!(prepared("e\u{301}\u{301}"), "\u{e9}\u{301}");
    }

    #[test]
    fn canonical_singletons_take_their_preferred_form() {
        assert_eq!(prepared("\u{2126}"), "\u{3a9}", "ohm sign is Omega");
        assert_eq!(prepared("\u{212b}"), "\u{c5}", "angstrom is A with ring");
        assert_eq!(prepared("\u{212a}"), "K", "kelvin sign is K");
        assert_eq!(prepared("\u{37e}"), ";", "Greek question mark");
    }

    #[test]
    fn emoji_sequences_collapse_to_their_base() {
        // Skin tone, ZWJ family, flag, subdivision-tag flag and keycap.
        assert_eq!(prepared("\u{1f44d}\u{1f3fd}"), "\u{1f44d}");
        assert_eq!(
            prepared("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}"),
            "\u{1f468}"
        );
        assert_eq!(prepared("\u{1f1ee}\u{1f1f3}"), "\u{1f1ee}");
        assert_eq!(
            prepared("\u{1f1ee}\u{1f1f3}\u{1f1fa}\u{1f1f8}"),
            "\u{1f1ee}\u{1f1fa}"
        );
        assert_eq!(
            prepared("\u{1f3f4}\u{e0067}\u{e0062}\u{e007f}"),
            "\u{1f3f4}"
        );
        assert_eq!(prepared("1\u{fe0f}\u{20e3}"), "1");
        // A joiner between letters is not an emoji sequence: only the joiner goes.
        assert_eq!(prepared("a\u{200d}b"), "ab");
    }

    #[test]
    fn compat_expansions_cover_ligatures_spaces_and_fullwidth_forms() {
        let text = |ch: char| {
            let (chars, len) = compat_expansion(ch).expect("expands");
            chars[..len].iter().collect::<String>()
        };
        assert_eq!(text('\u{fb01}'), "fi");
        assert_eq!(text('\u{fb03}'), "ffi");
        assert_eq!(text('\u{2003}'), " ");
        assert_eq!(text('\u{ff21}'), "A");
        assert!(compat_expansion('a').is_none());
    }
}
