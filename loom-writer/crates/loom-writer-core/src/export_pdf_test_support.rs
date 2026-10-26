//! Semantic readers for the PDF operators emitted by Writer's text exporters.
//!
//! This test-only reader understands `Tf`, identity `Tm`, relative `Td`/`TD`,
//! and literal-string `Tj`/`TJ` runs. It reports each run's starting position,
//! current font and size. It deliberately does not calculate glyph advances,
//! interpret non-identity text/page transforms, or claim positions for later
//! text that reuses an advanced text matrix.

use loom_pdf::inspect;

/// Exports a valid test fixture, keeping fallible production errors explicit
/// while avoiding repetitive `expect` calls at established success assertions.
pub(crate) fn export_valid_pdf(document: &crate::WriterDocument) -> Vec<u8> {
    crate::export_pdf(document).expect("valid PDF fixture")
}

/// A text run whose starting point and font state are known from the supported
/// Loom text operators.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DrawnTextRun {
    pub(crate) text: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) font: String,
    pub(crate) size: f32,
    /// Weight for the four standard Inter resources; unknown for fallbacks.
    pub(crate) bold: Option<bool>,
}

#[derive(Default)]
struct TextRunState {
    in_text: bool,
    font: Option<String>,
    size: Option<f32>,
    line_x: f32,
    line_y: f32,
    pending: Option<PendingTextRun>,
    position_known: bool,
    page_transform_known: bool,
}

struct PendingTextRun {
    text: String,
    x: f32,
    y: f32,
    font: String,
    size: f32,
}

impl TextRunState {
    fn validate_position(&self) -> Result<(), String> {
        if !self.in_text {
            return Err("text-showing operator outside BT/ET".into());
        }
        if !self.position_known || !self.page_transform_known {
            return Err("text run uses an unsupported or advanced position matrix".into());
        }
        self.font
            .as_deref()
            .ok_or_else(|| "text run has no preceding Tf font".to_owned())?;
        self.size
            .ok_or_else(|| "text run has no preceding Tf size".to_owned())?;
        Ok(())
    }

    fn append_text(&mut self, runs: &mut Vec<DrawnTextRun>, text: String) -> Result<(), String> {
        self.validate_position()?;
        let font = self.font.as_deref().expect("validated font").to_owned();
        let size = self.size.expect("validated size");
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.font != font || pending.size != size)
        {
            self.flush_run(runs);
        }
        let (x, y) = (self.line_x, self.line_y);
        let pending = self.pending.get_or_insert_with(|| PendingTextRun {
            text: String::new(),
            x,
            y,
            font,
            size,
        });
        pending.text.push_str(&text);
        self.position_known = false;
        Ok(())
    }

    fn flush_run(&mut self, runs: &mut Vec<DrawnTextRun>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if pending.text.is_empty() {
            return;
        }
        runs.push(DrawnTextRun {
            text: pending.text,
            x: pending.x,
            y: pending.y,
            bold: match pending.font.as_str() {
                "/F1" | "/F3" => Some(false),
                "/F2" | "/F4" => Some(true),
                _ => None,
            },
            font: pending.font,
            size: pending.size,
        });
    }
}

/// Reads literal text runs from the serialized PDF's human-readable content.
pub(crate) fn drawn_text_runs(pdf: &[u8]) -> Result<Vec<DrawnTextRun>, String> {
    let content = inspect::readable_content(pdf)?;
    parse_drawn_text_runs(&content)
}

/// Reads the supported text subset from `inspect::readable_content` output.
pub(crate) fn parse_drawn_text_runs(content: &str) -> Result<Vec<DrawnTextRun>, String> {
    let mut lexer = Lexer::new(content);
    let mut operands = Vec::new();
    let mut runs = Vec::new();
    let mut text_state = TextRunState {
        page_transform_known: true,
        ..TextRunState::default()
    };
    let mut transform_stack = Vec::new();
    let mut announced: Option<String> = None;
    let mut replacement: Option<(String, bool)> = None;
    let mut expect_actual = false;

    while let Some(token) = lexer.next()? {
        match &token {
            Token::Name(name) if name == "/ActualText" => expect_actual = true,
            Token::Literal(text) if expect_actual => {
                announced = Some(text.clone());
                expect_actual = false;
            }
            _ => {}
        }
        let Token::Word(operator) = &token else {
            operands.push(token);
            continue;
        };

        match operator.as_str() {
            "BT" => {
                text_state.in_text = true;
                text_state.line_x = 0.0;
                text_state.line_y = 0.0;
                text_state.position_known = true;
            }
            "ET" => {
                text_state.flush_run(&mut runs);
                text_state.in_text = false;
                text_state.position_known = false;
            }
            "Tf" => {
                text_state.flush_run(&mut runs);
                let name = operands.iter().rev().find_map(|token| match token {
                    Token::Name(name) => Some(name.clone()),
                    _ => None,
                });
                let point_size = operands.iter().rev().find_map(|token| match token {
                    Token::Number(value) => Some(*value),
                    _ => None,
                });
                text_state.font = name;
                text_state.size = point_size;
            }
            "Tm" => {
                let values = last_numbers::<6>(&operands);
                if let Some([a, b, c, d, e, f]) = values {
                    if near(a, 1.0) && near(b, 0.0) && near(c, 0.0) && near(d, 1.0) {
                        text_state.line_x = e;
                        text_state.line_y = f;
                        text_state.position_known = text_state.in_text;
                    } else {
                        text_state.position_known = false;
                    }
                } else {
                    text_state.position_known = false;
                }
            }
            "Td" | "TD" => {
                if let Some([x, y]) = last_numbers::<2>(&operands) {
                    text_state.line_x += x;
                    text_state.line_y += y;
                    text_state.position_known = text_state.in_text;
                } else {
                    text_state.position_known = false;
                }
            }
            "Tj" => {
                let text = operands.iter().rev().find_map(|token| match token {
                    Token::Literal(text) => Some(text.clone()),
                    _ => None,
                });
                if let Some(text) = text {
                    text_state.validate_position()?;
                    if let Some(logical) = replacement_text(&mut replacement, text) {
                        text_state.append_text(&mut runs, logical)?;
                    } else {
                        text_state.position_known = false;
                    }
                    // Advancing by a text run needs font glyph widths and any
                    // TJ adjustments. A fresh absolute Tm is needed before the
                    // next glyph's position can be reported.
                }
            }
            "TJ" => {
                if let Some(text) = array_text(&operands) {
                    text_state.validate_position()?;
                    if let Some(logical) = replacement_text(&mut replacement, text) {
                        text_state.append_text(&mut runs, logical)?;
                    } else {
                        text_state.position_known = false;
                    }
                    // The array's numeric spacing operands are intentionally
                    // not interpreted as glyph-position measurements.
                }
            }
            "T*" => text_state.position_known = false,
            "q" => transform_stack.push(text_state.page_transform_known),
            "Q" => {
                text_state.page_transform_known = transform_stack
                    .pop()
                    .ok_or_else(|| "unbalanced Q while reading text transforms".to_owned())?;
            }
            "cm" => {
                let identity = last_numbers::<6>(&operands).is_some_and(|[a, b, c, d, e, f]| {
                    near(a, 1.0)
                        && near(b, 0.0)
                        && near(c, 0.0)
                        && near(d, 1.0)
                        && near(e, 0.0)
                        && near(f, 0.0)
                });
                // `cm` concatenates onto the current CTM; an identity matrix
                // leaves a prior nonidentity transform in place. Only `Q`
                // can restore the saved state we track here.
                text_state.page_transform_known &= identity;
            }
            "BDC" => replacement = announced.take().map(|actual| (actual, false)),
            "EMC" => {
                if let Some((actual, inserted)) = replacement.take() {
                    if !inserted {
                        if let Some(pending) = text_state.pending.as_mut() {
                            pending.text.push_str(&actual);
                        }
                    }
                }
            }
            _ => {}
        }

        if is_content_operator(operator) {
            operands.clear();
        }
    }

    Ok(runs)
}

fn replacement_text(replacement: &mut Option<(String, bool)>, shown: String) -> Option<String> {
    match replacement {
        Some((actual, inserted)) if !*inserted => {
            *inserted = true;
            Some(actual.clone())
        }
        Some(_) => None,
        None => Some(shown),
    }
}

fn last_numbers<const N: usize>(operands: &[Token]) -> Option<[f32; N]> {
    let values: Vec<f32> = operands
        .iter()
        .rev()
        .filter_map(|token| match token {
            Token::Number(value) => Some(*value),
            _ => None,
        })
        .take(N)
        .collect();
    if values.len() != N {
        return None;
    }
    Some(std::array::from_fn(|i| values[N - 1 - i]))
}

fn array_text(operands: &[Token]) -> Option<String> {
    let start = operands
        .iter()
        .rposition(|token| matches!(token, Token::ArrayStart))?;
    let mut out = String::new();
    for token in &operands[start + 1..] {
        match token {
            Token::ArrayEnd => return Some(out),
            Token::Literal(text) => out.push_str(text),
            _ => {}
        }
    }
    None
}

fn near(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.0001
}

fn is_content_operator(word: &str) -> bool {
    matches!(
        word,
        "w" | "J"
            | "j"
            | "M"
            | "d"
            | "ri"
            | "i"
            | "gs"
            | "q"
            | "Q"
            | "cm"
            | "m"
            | "l"
            | "c"
            | "v"
            | "y"
            | "h"
            | "re"
            | "S"
            | "s"
            | "f"
            | "F"
            | "f*"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "n"
            | "W"
            | "W*"
            | "BT"
            | "ET"
            | "Tc"
            | "Tw"
            | "Tz"
            | "TL"
            | "Tf"
            | "Tr"
            | "Ts"
            | "Td"
            | "TD"
            | "Tm"
            | "T*"
            | "Tj"
            | "TJ"
            | "'"
            | "\""
            | "d0"
            | "d1"
            | "CS"
            | "cs"
            | "SC"
            | "SCN"
            | "sc"
            | "scn"
            | "G"
            | "g"
            | "RG"
            | "rg"
            | "K"
            | "k"
            | "sh"
            | "Do"
            | "BI"
            | "ID"
            | "EI"
            | "BMC"
            | "BDC"
            | "EMC"
            | "BX"
            | "EX"
    )
}

#[derive(Debug)]
enum Token {
    Name(String),
    Number(f32),
    Literal(String),
    ArrayStart,
    ArrayEnd,
    Word(String),
}

struct Lexer<'a> {
    input: &'a str,
    at: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, at: 0 }
    }

    fn next(&mut self) -> Result<Option<Token>, String> {
        self.skip_space_and_comments();
        let bytes = self.input.as_bytes();
        let Some(&first) = bytes.get(self.at) else {
            return Ok(None);
        };
        let token = match first {
            b'(' => Token::Literal(self.literal_string()?),
            b'/' => Token::Name(self.name()),
            b'[' => {
                self.at += 1;
                Token::ArrayStart
            }
            b']' => {
                self.at += 1;
                Token::ArrayEnd
            }
            b'<' => {
                // Hex strings and dictionaries are opaque to this reader.
                self.opaque_angle_token();
                Token::Word("<opaque>".into())
            }
            _ => {
                let word = self.word();
                if let Ok(number) = word.parse::<f32>() {
                    Token::Number(number)
                } else {
                    Token::Word(word)
                }
            }
        };
        Ok(Some(token))
    }

    fn skip_space_and_comments(&mut self) {
        let bytes = self.input.as_bytes();
        loop {
            while bytes.get(self.at).is_some_and(|byte| is_space(*byte)) {
                self.at += 1;
            }
            if bytes.get(self.at) != Some(&b'%') {
                return;
            }
            while bytes
                .get(self.at)
                .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
            {
                self.at += 1;
            }
        }
    }

    fn name(&mut self) -> String {
        self.at += 1;
        let start = self.at;
        while self
            .input
            .as_bytes()
            .get(self.at)
            .is_some_and(|byte| !is_delimiter(*byte))
        {
            self.at += 1;
        }
        format!("/{}", &self.input[start..self.at])
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while self
            .input
            .as_bytes()
            .get(self.at)
            .is_some_and(|byte| !is_delimiter(*byte))
        {
            self.at += 1;
        }
        if self.at == start {
            self.at += 1;
        }
        self.input[start..self.at].to_owned()
    }

    fn opaque_angle_token(&mut self) {
        let bytes = self.input.as_bytes();
        if bytes.get(self.at + 1) == Some(&b'<') {
            self.at += 2;
            while self.at + 1 < bytes.len() && &bytes[self.at..self.at + 2] != b">>" {
                self.at += 1;
            }
            self.at = (self.at + 2).min(bytes.len());
        } else {
            self.at += 1;
            while self.at < bytes.len() && bytes[self.at] != b'>' {
                self.at += 1;
            }
            self.at = (self.at + 1).min(bytes.len());
        }
    }

    fn literal_string(&mut self) -> Result<String, String> {
        let bytes = self.input.as_bytes();
        self.at += 1; // opening parenthesis
        let mut depth = 1usize;
        let mut decoded = Vec::new();
        while let Some(&byte) = bytes.get(self.at) {
            self.at += 1;
            match byte {
                b'\\' => {
                    let Some(&escaped) = bytes.get(self.at) else {
                        return Err("trailing escape in literal PDF string".into());
                    };
                    self.at += 1;
                    match escaped {
                        b'n' => decoded.push(b'\n'),
                        b'r' => decoded.push(b'\r'),
                        b't' => decoded.push(b'\t'),
                        b'b' => decoded.push(0x08),
                        b'f' => decoded.push(0x0C),
                        b'(' | b')' | b'\\' => decoded.push(escaped),
                        b'\r' => {
                            if bytes.get(self.at) == Some(&b'\n') {
                                self.at += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut value = u16::from(escaped - b'0');
                            for _ in 0..2 {
                                let Some(next @ b'0'..=b'7') = bytes.get(self.at).copied() else {
                                    break;
                                };
                                self.at += 1;
                                value = value * 8 + u16::from(next - b'0');
                            }
                            decoded.push(value as u8);
                        }
                        other => decoded.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    decoded.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return String::from_utf8(decoded)
                            .map_err(|error| format!("literal PDF text is not UTF-8: {error}"));
                    }
                    decoded.push(byte);
                }
                _ => decoded.push(byte),
            }
        }
        Err("unterminated literal PDF string".into())
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    is_space(byte) || matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_pdf::{text_lines::PositionedTextRun, PdfDocument, TextStyle};

    #[test]
    fn reader_matches_td_and_tm_and_keeps_multiple_absolute_fragments() {
        let style = TextStyle {
            size_pt: 13.5,
            bold: true,
            ..Default::default()
        };
        let first = "escaped (literal) and \\ path";
        let second = "second (fragment)";

        let mut pdf = PdfDocument::new();
        let page = pdf.add_page(200.0, 120.0);
        pdf.draw_text(page, 15.0, 90.0, first, &style);
        pdf.draw_text_line(
            page,
            &[
                PositionedTextRun {
                    x: 15.0,
                    y: 90.0,
                    text: first,
                    style: &style,
                },
                PositionedTextRun {
                    x: 82.0,
                    y: 41.0,
                    text: second,
                    style: &style,
                },
            ],
        );

        let bytes = pdf.serialize();
        let runs = drawn_text_runs(&bytes).expect("read Loom's text operators");
        assert_eq!(runs.len(), 3, "one Td run plus two Tm fragments: {runs:?}");
        assert_eq!(runs[0], runs[1], "Td and Tm report the same run semantics");
        assert_eq!(runs[0].text, first, "escaped literal text is decoded");
        assert_eq!((runs[2].x, runs[2].y), (82.0, 41.0));
        assert_eq!(runs[2].text, second);
        for run in &runs {
            assert_eq!(run.font, "/F2");
            assert_eq!(run.size, 13.5);
            assert_eq!(run.bold, Some(true));
        }
        inspect::verify(&bytes).expect("regression PDF is structurally valid");
    }

    #[test]
    fn reader_groups_positioned_glyphs_and_applies_actualtext_once() {
        let content = concat!(
            "BT /F1 12 Tf ",
            "/Span << /ActualText (fi) >> BDC ",
            "1 0 0 1 10 20 Tm (f) Tj ",
            "1 0 0 1 16 20 Tm (i) Tj EMC ET",
        );
        let runs = parse_drawn_text_runs(content).expect("read shaped glyph run");
        assert_eq!(runs.len(), 1, "glyphs in one Tf segment form one run");
        assert_eq!(runs[0].text, "fi", "ActualText is applied once per span");
        assert_eq!((runs[0].x, runs[0].y), (10.0, 20.0));
    }

    #[test]
    fn reader_refuses_unknown_matrix_and_advanced_following_run_positions() {
        let transformed = "BT /F1 12 Tf 0 1 -1 0 10 20 Tm (rotated) Tj ET";
        assert!(parse_drawn_text_runs(transformed)
            .unwrap_err()
            .contains("unsupported or advanced position matrix"));

        let advanced = "BT /F1 12 Tf 1 0 0 1 10 20 Tm (first) Tj (second) Tj ET";
        assert!(parse_drawn_text_runs(advanced)
            .unwrap_err()
            .contains("unsupported or advanced position matrix"));
    }

    #[test]
    fn identity_cm_does_not_clear_transformed_ctm_until_q_restores_it() {
        let transformed_scope = concat!(
            "q ",
            "0 1 -1 0 10 20 cm ",
            "1 0 0 1 0 0 cm ",
            "BT /F1 12 Tf 1 0 0 1 10 20 Tm (inside transformed scope) Tj ET ",
            "Q"
        );
        assert!(parse_drawn_text_runs(transformed_scope)
            .unwrap_err()
            .contains("unsupported or advanced position matrix"));

        let restored_scope = concat!(
            "q 0 1 -1 0 10 20 cm Q ",
            "BT /F1 12 Tf 1 0 0 1 10 20 Tm (after Q restore) Tj ET"
        );
        let runs =
            parse_drawn_text_runs(restored_scope).expect("Q restores the identity page transform");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "after Q restore");
        assert_eq!((runs[0].x, runs[0].y), (10.0, 20.0));
    }

    #[test]
    fn reader_decodes_nested_literals_escaped_parentheses_and_octal() {
        let content = r"BT /F1 12 Tf 1 0 0 1 3 4 Tm (outer (inner) \(paren\) \\ \101) Tj ET";
        let runs = parse_drawn_text_runs(content).expect("read escaped literal");
        assert_eq!(runs[0].text, "outer (inner) (paren) \\ A");
    }
}
