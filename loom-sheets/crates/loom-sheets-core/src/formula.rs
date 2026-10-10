//! Formula lexer, parser and expression tree. `parse_formula` turns the text of a
//! cell into an `Expr` tree; the evaluator in `lib.rs` and the function modules walk it.

use crate::functions::let_binding::repeats_a_name;
use crate::{CalcError, CellRef, Value};

/// Token types for the formula lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Cell(CellRef),
    Ident(String),
    /// Quoted sheet name for cross-sheet references (`'My Sheet'!A1`).
    SheetName(String),
    /// `!` separating a sheet name from a cell reference.
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Comma,
    Colon,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
    Amp,
    Percent,
    /// An error literal such as `#N/A`.
    Error(CalcError),
}

/// The error an Excel error literal (`#N/A`, `#REF!`, ...) names.
fn error_literal(text: &str) -> Option<CalcError> {
    Some(match text.to_ascii_uppercase().as_str() {
        "#DIV/0!" => CalcError::DivZero,
        "#N/A" => CalcError::NA,
        "#VALUE!" => CalcError::Value,
        "#NAME?" => CalcError::Name,
        "#REF!" => CalcError::Ref,
        "#NUM!" => CalcError::Num,
        _ => return None,
    })
}

fn lex(input: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Amp);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '!' => {
                tokens.push(Token::Bang);
                i += 1;
            }
            '\'' => {
                // Quoted sheet name for cross-sheet references: `'My Sheet'!A1`
                // with '' as an escaped quote. Only meaningful directly before
                // a `!`; anything else fails at parse time.
                i += 1;
                let mut name = String::new();
                let mut closed = false;
                while let Some(ch) = input[i..].chars().next() {
                    if ch == '\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            name.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        name.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed || name.is_empty() {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::SheetName(name));
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            // Absolute-reference marker: `$A$1` pins column/row during
            // fill/copy shifting (see `shift_formula_references`). The marker
            // carries no evaluation semantics, so only a `$` directly
            // attached to a cell coordinate is accepted here.
            '$' => {
                if i + 1 < bytes.len() && (bytes[i + 1] as char).is_ascii_alphabetic() {
                    i += 1;
                } else {
                    return Err(CalcError::Parse);
                }
            }
            '=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Eq);
                    i += 2;
                } else {
                    tokens.push(Token::Eq);
                    i += 1;
                }
            }
            '<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '#' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'/' | b'!' | b'?')))
                    .map_or(bytes.len(), |p| i + 1 + p);
                let literal = error_literal(&input[i..end]).ok_or(CalcError::Parse)?;
                tokens.push(Token::Error(literal));
                i = end;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                let mut closed = false;
                while let Some(ch) = input[i..].chars().next() {
                    if ch == '"' {
                        if bytes.get(i + 1) == Some(&b'"') {
                            s.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        s.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::String(s));
            }
            c if c.is_ascii_digit()
                || (c == '.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                let start = i;
                let mut saw_decimal = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' && !saw_decimal {
                        saw_decimal = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                i = scan_exponent(bytes, i);
                let num: f64 = input[start..i].parse().map_err(|_| CalcError::Parse)?;
                tokens.push(Token::Number(num));
            }
            c if c.is_ascii_alphabetic() => {
                // Could be a cell ref (e.g. A1, $A$1), function name, or bare name.
                let letters_start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                    i += 1;
                }
                let letters = &input[letters_start..i];
                // A name with digits, dots or underscores is a function when a
                // `(` follows (`LOG10(`, `STDEV.S(`) and a sheet qualifier when
                // a `!` does (`Sheet2!A1`); otherwise it may still be a cell.
                let mut word_end = i;
                while word_end < bytes.len()
                    && (bytes[word_end].is_ascii_alphanumeric()
                        || bytes[word_end] == b'_'
                        || bytes[word_end] == b'.')
                {
                    word_end += 1;
                }
                if word_end > i {
                    let after = input[word_end..].trim_start();
                    if after.starts_with('(') || input[word_end..].starts_with('!') {
                        tokens.push(Token::Ident(
                            input[letters_start..word_end].to_ascii_uppercase(),
                        ));
                        i = word_end;
                        continue;
                    }
                }
                // Optional row-absolute marker between column and row (`A$1`).
                if i + 1 < bytes.len()
                    && bytes[i] == b'$'
                    && (bytes[i + 1] as char).is_ascii_digit()
                {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    // A cell reference: letters followed by digits, e.g. A1 or AA10.
                    let num_start = i;
                    while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                    let full = format!("{letters}{}", &input[num_start..i]);
                    if let Some(cr) = CellRef::parse(&full) {
                        tokens.push(Token::Cell(cr));
                    } else {
                        return Err(CalcError::Name);
                    }
                } else {
                    tokens.push(Token::Ident(letters.to_ascii_uppercase()));
                }
            }
            _ => return Err(CalcError::Parse),
        }
    }
    Ok(tokens)
}

/// Extend a number token over an exponent (`E3`, `e-7`) when one follows.
fn scan_exponent(bytes: &[u8], i: usize) -> usize {
    if !bytes.get(i).is_some_and(|b| *b == b'E' || *b == b'e') {
        return i;
    }
    let mut j = i + 1;
    if bytes.get(j).is_some_and(|b| *b == b'+' || *b == b'-') {
        j += 1;
    }
    if !bytes.get(j).is_some_and(u8::is_ascii_digit) {
        return i;
    }
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
    }
    j
}

/// AST expression.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Number(f64),
    Text(String),
    Cell(CellRef),
    /// Cross-sheet cell reference (`Sheet2!A1`); sheet name as written.
    SheetCell {
        sheet: String,
        cell: CellRef,
    },
    /// Cross-sheet range (`Sheet2!A1:B2`).
    SheetRange {
        sheet: String,
        start: CellRef,
        end: CellRef,
    },
    Unary(Box<Expr>),
    Binary {
        lhs: Box<Expr>,
        op: BinOp,
        rhs: Box<Expr>,
    },
    Func {
        name: String,
        args: Vec<Expr>,
    },
    Range {
        start: CellRef,
        end: CellRef,
    },
    Bool(bool),
    /// An error literal typed into the formula.
    Error(CalcError),
    /// A bare name that is not a function or a constant. It is `#NAME?`
    /// unless a `LET` binds it.
    Name(String),
    /// A value already computed by `LET`, standing in for a bound name.
    Constant(Value),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
}

/// Parser produces an expression from tokens.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, t: &Token) -> Result<(), CalcError> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(CalcError::Parse)
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, CalcError> {
        // Handle comparison at top level.
        let mut lhs = self.parse_concat()?;
        // Comparisons chain left to right: `2>1>0` is `(2>1)>0`.
        while let Some(t) = self.peek() {
            let op = match t {
                Token::Eq => BinOp::Eq,
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,
                Token::Ne => BinOp::Ne,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_concat()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    /// `&` binds looser than `+`/`-` and tighter than comparisons.
    fn parse_concat(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_additive()?;
        while self.peek() == Some(&Token::Amp) {
            self.pos += 1;
            let rhs = self.parse_additive()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Concat,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => Some(BinOp::Add),
                Some(Token::Minus) => Some(BinOp::Sub),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_multiplicative()?;
                lhs = Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => Some(BinOp::Mul),
                Some(Token::Slash) => Some(BinOp::Div),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_unary()?;
                lhs = Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    /// Excel: unary minus binds tighter than `^` (`-2^2` is 4).
    fn parse_unary(&mut self) -> Result<Expr, CalcError> {
        self.parse_power()
    }

    fn parse_negated(&mut self) -> Result<Expr, CalcError> {
        if let Some(Token::Minus) = self.peek() {
            self.pos += 1;
            let inner = self.parse_negated()?;
            return Ok(Expr::Unary(Box::new(inner)));
        }
        let mut operand = self.parse_primary()?;
        // Postfix percent binds tighter than `^` and unary minus: `-50%` is -0.5.
        while let Some(Token::Percent) = self.peek() {
            self.pos += 1;
            operand = Expr::Binary {
                lhs: Box::new(operand),
                op: BinOp::Div,
                rhs: Box::new(Expr::Number(100.0)),
            };
        }
        Ok(operand)
    }

    fn parse_power(&mut self) -> Result<Expr, CalcError> {
        let mut lhs = self.parse_negated()?;
        while let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            let rhs = self.parse_negated()?;
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Pow,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_primary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::String(s)) => Ok(Expr::Text(s)),
            Some(Token::Error(error)) => Ok(Expr::Error(error)),
            Some(Token::Cell(r)) => {
                // Check for range.
                if let Some(Token::Colon) = self.peek() {
                    self.pos += 1;
                    let end = match self.next() {
                        Some(Token::Cell(e)) => e,
                        _ => return Err(CalcError::Parse),
                    };
                    return Ok(Expr::Range { start: r, end });
                }
                Ok(Expr::Cell(r))
            }
            Some(Token::Ident(name)) => {
                // Cross-sheet reference: Name!A1 or Name!A1:B2.
                if let Some(Token::Bang) = self.peek() {
                    self.pos += 1;
                    return self.parse_sheet_ref(name);
                }
                // Function call or bare name.
                if let Some(Token::LParen) = self.peek() {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if let Some(Token::RParen) = self.peek() {
                        self.pos += 1;
                    } else {
                        loop {
                            let arg = self.parse_expr()?;
                            args.push(arg);
                            match self.next() {
                                Some(Token::Comma) => continue,
                                Some(Token::RParen) => break,
                                _ => return Err(CalcError::Parse),
                            }
                        }
                    }
                    Ok(Expr::Func { name, args })
                } else {
                    // Bare ident: could be TRUE/FALSE or named error.
                    match name.as_str() {
                        "TRUE" => Ok(Expr::Bool(true)),
                        "FALSE" => Ok(Expr::Bool(false)),
                        "NA" | "ERROR" => Err(CalcError::NA),
                        _ => Ok(Expr::Name(name.clone())),
                    }
                }
            }
            Some(Token::SheetName(name)) => {
                // Quoted cross-sheet reference: 'My Sheet'!A1.
                if let Some(Token::Bang) = self.peek() {
                    self.pos += 1;
                    return self.parse_sheet_ref(name);
                }
                Err(CalcError::Parse)
            }
            Some(Token::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            _ => Err(CalcError::Parse),
        }
    }

    /// Parse the cell or range half of a cross-sheet reference after the
    /// sheet name and `!` were consumed.
    fn parse_sheet_ref(&mut self, sheet: String) -> Result<Expr, CalcError> {
        let start = match self.next() {
            Some(Token::Cell(cell)) => cell,
            _ => return Err(CalcError::Parse),
        };
        if let Some(Token::Colon) = self.peek() {
            self.pos += 1;
            let end = match self.next() {
                Some(Token::Cell(cell)) => cell,
                _ => return Err(CalcError::Parse),
            };
            return Ok(Expr::SheetRange { sheet, start, end });
        }
        Ok(Expr::SheetCell { sheet, cell: start })
    }
}

/// A parsed formula ready for evaluation.
#[derive(Debug)]
pub struct Formula {
    pub(crate) root: Expr,
}

/// Parse a formula body (without leading `=`).
pub fn parse_formula(body: &str) -> Result<Formula, CalcError> {
    let tokens = lex(body)?;
    let mut p = Parser::new(tokens);
    let root = p.parse_expr()?;
    // Excel refuses to enter a LET that binds one name twice, so that is a parse error.
    if p.pos != p.tokens.len() || repeats_a_name(&root) {
        return Err(CalcError::Parse);
    }
    Ok(Formula { root })
}
