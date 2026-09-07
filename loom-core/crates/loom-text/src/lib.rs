//! `loom-text` provides text model primitives: paragraph and character
//! styles, line breaking, and simple shaping hooks. It is engine-focused and
//! deterministic; rendering integration is documented in an RFC.
//!
//! We deliberately keep a small scope here (style values, measurement-free
//! wrapping decisions via grapheme / width tables, and a document line model)
//! so it compiles anywhere without external fonts or platform bindings.

/// Character style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Thin.
    Thin,
    /// Light.
    Light,
    /// Regular.
    Regular,
    /// Medium.
    Medium,
    /// SemiBold.
    Semibold,
    /// Bold.
    Bold,
    /// Black.
    Black,
}

impl FontWeight {
    /// Numeric weight (CSS-like).
    pub fn numeric(self) -> u16 {
        match self {
            Self::Thin => 100,
            Self::Light => 300,
            Self::Regular => 400,
            Self::Medium => 500,
            Self::Semibold => 600,
            Self::Bold => 700,
            Self::Black => 900,
        }
    }
}

/// Horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// Left.
    Left,
    /// Centered.
    Center,
    /// Right.
    Right,
    /// Justified.
    Justify,
}

/// Line breaking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreakRule {
    /// No breaking (single line).
    NoBreak,
    /// Break at word boundaries.
    Word,
    /// Break at any character.
    Char,
}

/// Paragraph style value object (immutable).
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphStyle {
    /// Horizontal alignment.
    pub alignment: Alignment,
    /// Space before in points.
    pub space_before: f32,
    /// Space after in points.
    pub space_after: f32,
    /// Line spacing multiplier.
    pub line_spacing: f32,
    /// Left indent in points.
    pub left_indent: f32,
    /// Right indent in points.
    pub right_indent: f32,
    /// First line indent in points.
    pub first_line_indent: f32,
    /// Break rule.
    pub break_rule: LineBreakRule,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            alignment: Alignment::Left,
            space_before: 0.0,
            space_after: 8.0,
            line_spacing: 1.15,
            left_indent: 0.0,
            right_indent: 0.0,
            first_line_indent: 0.0,
            break_rule: LineBreakRule::Word,
        }
    }
}

/// Character style value object (immutable).
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterStyle {
    /// Font family name (may be empty for inherited).
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Weight.
    pub weight: FontWeight,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Strikethrough.
    pub strikethrough: bool,
    /// Superscript.
    pub superscript: bool,
    /// Subscript.
    pub subscript: bool,
    /// Color (hex `#RRGGBBAA`).
    pub color: Option<String>,
    /// Background color hex.
    pub background: Option<String>,
}

impl Default for CharacterStyle {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: 12.0,
            weight: FontWeight::Regular,
            italic: false,
            underline: false,
            strikethrough: false,
            superscript: false,
            subscript: false,
            color: Some("#000000FF".into()),
            background: None,
        }
    }
}

/// A run of uniform character style within a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRun {
    /// Range in bytes (code points) into the paragraph text.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The style.
    pub style: CharacterStyle,
}

/// A line within a laid-out paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Byte start into paragraph.
    pub start: usize,
    /// Byte end (exclusive).
    pub end: usize,
    /// Whether a forced line break preceded this line.
    pub preceded_by_break: bool,
}

/// A paragraph with text and styles.
#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    /// Text (may include internal `\u{2028}` for soft breaks).
    pub text: String,
    /// Paragraph style.
    pub style: ParagraphStyle,
    /// Character style runs.
    pub runs: Vec<StyleRun>,
}

impl Paragraph {
    /// New empty paragraph.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            style: ParagraphStyle::default(),
            runs: Vec::new(),
        }
    }

    /// Segment text into lines based on a width table function.
    ///
    /// `char_width` returns a display width per char (e.g. 2 for CJK).
    /// `available_width` is in the same units.
    pub fn layout_lines<F>(&self, available_width: f32, char_width: F) -> Vec<Line>
    where
        F: Fn(char) -> f32,
    {
        let mut lines = Vec::new();
        let chars: Vec<(usize, char)> = self.text.char_indices().collect();
        match self.style.break_rule {
            LineBreakRule::NoBreak => {
                if !chars.is_empty() {
                    lines.push(Line {
                        start: 0,
                        end: self.text.len(),
                        preceded_by_break: false,
                    });
                }
                lines
            }
            LineBreakRule::Char => {
                let mut cur_start = 0usize;
                let mut width = 0.0f32;
                let mut preceded = false;
                for (i, c) in &chars {
                    let w = char_width(*c);
                    if width + w > available_width && width > 0.0 {
                        lines.push(Line {
                            start: cur_start,
                            end: *i,
                            preceded_by_break: preceded,
                        });
                        cur_start = *i;
                        width = w;
                        preceded = false;
                    } else {
                        width += w;
                    }
                }
                if cur_start < self.text.len() {
                    lines.push(Line {
                        start: cur_start,
                        end: self.text.len(),
                        preceded_by_break: preceded,
                    });
                }
                lines
            }
            LineBreakRule::Word => {
                // Break at whitespace and at explicit soft/hard breaks.
                let mut cur_start = 0usize;
                let mut width = 0.0f32;
                let mut last_space_idx: Option<usize> = None; // byte index of space
                let mut preceded = false;
                let mut i = 0usize;
                while i < chars.len() {
                    let (byte_idx, c) = chars[i];
                    // Handle explicit breaks.
                    if c == '\n' || c == '\u{2028}' {
                        if cur_start < byte_idx {
                            lines.push(Line {
                                start: cur_start,
                                end: byte_idx,
                                preceded_by_break: preceded,
                            });
                        }
                        cur_start = byte_idx + c.len_utf8();
                        width = 0.0;
                        last_space_idx = None;
                        preceded = true;
                        i += 1;
                        continue;
                    }
                    let w = char_width(c);
                    if c.is_whitespace() {
                        // Whitespace ends a word but stays in the current line.
                        last_space_idx = Some(byte_idx);
                    }
                    if width + w > available_width && width > 0.0 {
                        // Wrap at last space if it exists and is after cur_start,
                        // otherwise hard-wrap at current char.
                        if let Some(sp) = last_space_idx {
                            if sp > cur_start {
                                lines.push(Line {
                                    start: cur_start,
                                    end: sp,
                                    preceded_by_break: preceded,
                                });
                                cur_start = sp + 1; // skip the space
                                width = 0.0;
                                last_space_idx = None;
                                i += 1;
                                continue;
                            }
                        }
                        // Hard wrap (no space).
                        lines.push(Line {
                            start: cur_start,
                            end: byte_idx,
                            preceded_by_break: preceded,
                        });
                        cur_start = byte_idx;
                        width = w;
                        last_space_idx = None;
                        preceded = false;
                    } else {
                        width += w;
                    }
                    i += 1;
                }
                if cur_start < self.text.len() {
                    lines.push(Line {
                        start: cur_start,
                        end: self.text.len(),
                        preceded_by_break: preceded,
                    });
                }
                lines
            }
        }
    }
}

impl Default for Paragraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Bidi algorithm helpers: detect RTL start, logical mirroring placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    /// Left-to-right.
    L,
    /// Right-to-left.
    R,
    /// Arabic Letter.
    AL,
    /// European Number.
    EN,
    /// European Separator.
    ES,
    /// European Terminator.
    ET,
    /// Common Separator.
    CS,
    /// Other Neutral.
    ON,
}

/// Simplified bidi class detection for the Unicode characters we document.
pub fn bidi_class(c: char) -> BidiClass {
    let cp = c as u32;
    // Hebrew / Arabic ranges.
    if (0x0590..=0x08FF).contains(&cp) {
        // Some are numbers/digits but simplify.
        if (0x05D0..=0x05EA).contains(&cp)
            || (0x0620..=0x064A).contains(&cp)
            || (0x0671..=0x06D3).contains(&cp)
            || (0x06FA..=0x06FC).contains(&cp)
            || (0x0750..=0x077F).contains(&cp)
            || (0x08A0..=0x08FF).contains(&cp)
        {
            if (0x0600..=0x0605).contains(&cp) {
                return BidiClass::AL;
            }
            if (0x0660..=0x0669).contains(&cp) || (0x06F0..=0x06F9).contains(&cp) {
                return BidiClass::EN;
            }
            return BidiClass::AL;
        }
        BidiClass::R
    } else if c.is_ascii_digit() {
        BidiClass::EN
    } else {
        BidiClass::L
    }
}

/// Determine if a paragraph starts right-to-left.
pub fn starts_rtl(s: &str) -> bool {
    for c in s.chars() {
        let cls = bidi_class(c);
        if matches!(cls, BidiClass::R | BidiClass::AL) {
            return true;
        }
        if matches!(cls, BidiClass::L) {
            return false;
        }
    }
    false
}

/// Simple display width (EUC-KR / East Asian widths). CJK = 2, else 1.
pub fn default_char_width(c: char) -> f32 {
    let cp = c as u32;
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FFFD).contains(&cp)
    {
        2.0
    } else {
        1.0
    }
}

/// A run-length-encoded UTF-16 mapping for cursor movement correctness.
#[derive(Debug, Clone)]
pub struct Utf16Map {
    /// Byte offset -> UTF-16 unit offset for each char boundary.
    boundaries: Vec<(usize, usize)>,
}

impl Utf16Map {
    /// Build the mapping from a string.
    pub fn build(s: &str) -> Self {
        let mut boundaries = Vec::with_capacity(s.chars().count() + 1);
        let mut byte = 0usize;
        let mut unit = 0usize;
        boundaries.push((0, 0));
        for c in s.chars() {
            byte += c.len_utf8();
            unit += c.len_utf16();
            boundaries.push((byte, unit));
        }
        Self { boundaries }
    }

    /// Byte offset for a UTF-16 unit offset.
    pub fn to_byte(&self, utf16: usize) -> usize {
        // Binary search for the largest boundary <= utf16.
        let pos = self.boundaries.partition_point(|&(_, u)| u <= utf16);
        let idx = pos.saturating_sub(1);
        self.boundaries[idx].0
    }

    /// UTF-16 unit offset for a byte offset.
    pub fn to_utf16(&self, byte: usize) -> usize {
        let pos = self.boundaries.partition_point(|&(b, _)| b <= byte);
        let idx = pos.saturating_sub(1);
        self.boundaries[idx].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    fn width_latin(_c: char) -> f32 {
        1.0
    }

    #[test]
    fn paragraph_defaults() {
        let p = Paragraph::new();
        assert_eq!(p.style.alignment, Alignment::Left);
        let cs = CharacterStyle::default();
        assert_eq!(cs.font_size, 12.0);
    }

    #[test]
    fn word_wrap_simple() {
        let mut p = Paragraph::new();
        p.text = "hello world foo bar".into();
        p.style.break_rule = LineBreakRule::Word;
        let lines = p.layout_lines(6.0, width_latin);
        assert!(lines.len() >= 2);
        let first: Vec<char> = p.text[lines[0].start..lines[0].end].chars().collect();
        assert_eq!(first.iter().collect::<String>(), "hello");
    }

    #[test]
    fn word_wrap_unbreakable_word() {
        let mut p = Paragraph::new();
        p.text = "abcdefghij".into();
        p.style.break_rule = LineBreakRule::Word;
        // No spaces, must hard-wrap.
        let lines = p.layout_lines(3.0, width_latin);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].end - lines[0].start, 3);
    }

    #[test]
    fn char_wrap() {
        let mut p = Paragraph::new();
        p.text = "abcd".into();
        p.style.break_rule = LineBreakRule::Char;
        let lines = p.layout_lines(2.0, width_latin);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn explicit_newline() {
        let mut p = Paragraph::new();
        p.text = "a\nb".into();
        p.style.break_rule = LineBreakRule::Word;
        let lines = p.layout_lines(1000.0, width_latin);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].preceded_by_break);
    }

    #[test]
    fn no_break() {
        let mut p = Paragraph::new();
        p.text = "aaaaaa".into();
        p.style.break_rule = LineBreakRule::NoBreak;
        let lines = p.layout_lines(2.0, width_latin);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].end - lines[0].start, 6);
    }

    #[test]
    fn cjk_width() {
        assert_eq!(default_char_width('中'), 2.0);
        assert_eq!(default_char_width('a'), 1.0);
    }

    #[test]
    fn bidi_detection() {
        assert!(starts_rtl("\u{05D0}bc"));
        assert!(!starts_rtl("abc"));
        assert!(starts_rtl("\u{0627}long"));
        assert!(!starts_rtl("123"));
    }

    #[test]
    fn utf16_mapping() {
        let s = "a\u{1F600}b"; // a(1), emoji(2 utf16 units), b(1)
        let m = Utf16Map::build(s);
        assert_eq!(m.to_byte(0), 0);
        assert_eq!(m.to_byte(1), 1); // after 'a'
        assert_eq!(m.to_byte(3), 5); // after emoji (5 bytes)
        assert_eq!(m.to_utf16(5), 3);
        assert_eq!(m.to_byte(6), 6); // end
    }

    #[test]
    fn style_flags_preserved() {
        let s = CharacterStyle {
            italic: true,
            weight: FontWeight::Bold,
            ..Default::default()
        };
        assert!(s.italic);
        assert_eq!(s.weight.numeric(), 700);
    }
}
