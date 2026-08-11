//! Loom Sheets formula engine and workbook model — headless and testable.
//!
//! The engine implements a tokenizer, a recursive-descent parser, cell
//! reference resolution, and a dependency-graph evaluator with topological
//! ordering and cycle detection. CSV import/export is included for
//! interoperability. The GUI (a documented follow-on) consumes this engine.

use std::collections::{BTreeMap, HashMap, HashSet};

/// A cell coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    /// Row (0-based).
    pub row: u32,
    /// Column (0-based).
    pub col: u32,
}

impl CellRef {
    /// Parse an A1-style reference like "B3" (col letter(s), then row number).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_uppercase();
        let bytes = s.as_bytes();
        let mut col: u32 = 0;
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            col = col * 26 + (bytes[i] - b'A' + 1) as u32;
            i += 1;
        }
        if col == 0 {
            return None;
        }
        let row_str = &s[i..];
        if row_str.is_empty() {
            return None;
        }
        let row: u32 = row_str.parse().ok()?;
        if row == 0 {
            return None;
        }
        Some(Self {
            row: row - 1,
            col: col - 1,
        })
    }

    /// Render as A1 (e.g. `B3`).
    pub fn to_a1(self) -> String {
        let mut col = self.col + 1;
        let mut letters = String::new();
        while col > 0 {
            let rem = (col - 1) % 26;
            letters.insert(0, (b'A' + rem as u8) as char);
            col = (col - 1) / 26;
        }
        format!("{}{}", letters, self.row + 1)
    }
}

/// A cell value produced by evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number.
    Number(f64),
    /// A text string.
    Text(String),
    /// Boolean.
    Bool(bool),
    /// Empty cell.
    Empty,
    /// An error value produced by evaluation.
    Error(CalcError),
}

impl Value {
    /// Display a value for export or console.
    pub fn display(&self) -> String {
        match self {
            Self::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Self::Text(s) => s.clone(),
            Self::Bool(b) => b.to_string(),
            Self::Empty => String::new(),
            Self::Error(e) => format!("#{}", e.code()),
        }
    }
}

/// Calculation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalcError {
    /// Division by zero.
    DivZero,
    /// Value not available (reference to empty/error).
    NA,
    /// Invalid value type.
    Value,
    /// Name not recognized.
    Name,
    /// Formula references its own cell (cycle).
    Ref,
    /// Parse error.
    Parse,
}

impl CalcError {
    /// Spreadsheet-style error code.
    pub fn code(self) -> &'static str {
        match self {
            Self::DivZero => "DIV/0!",
            Self::NA => "N/A",
            Self::Value => "VALUE!",
            Self::Name => "NAME?",
            Self::Ref => "REF!",
            Self::Parse => "PARSE!",
        }
    }
}

/// A cell in the workbook.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Raw input: a formula like `=A1+B2` or a literal.
    pub raw: String,
}

impl Cell {
    /// Is this a formula? (starts with `=`)
    pub fn is_formula(&self) -> bool {
        self.raw.starts_with('=')
    }
}

/// A pending formula-bar edit whose workbook mutation happens only on commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellEditTransaction {
    original: Option<String>,
    draft: String,
}

impl CellEditTransaction {
    /// Start an edit from the selected cell's exact raw state.
    pub fn begin(original: Option<&str>) -> Self {
        let original = original.map(str::to_owned);
        let draft = original.clone().unwrap_or_default();
        Self { original, draft }
    }

    /// Replace the uncommitted text without changing the source cell.
    pub fn update(&mut self, draft: impl Into<String>) {
        self.draft = draft.into();
    }

    /// Commit the draft as one raw edit, or return `None` for an unchanged edit.
    pub fn commit(self) -> Option<RawCellEdit> {
        if self.original.as_deref() == Some(self.draft.as_str()) {
            return None;
        }
        Some(RawCellEdit {
            before: self.original,
            after: self.draft,
        })
    }

    /// Cancel the edit and return the exact original raw state.
    pub fn cancel(self) -> Option<String> {
        self.original
    }
}

/// The before/after raw values for one committed cell edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCellEdit {
    before: Option<String>,
    after: String,
}

impl RawCellEdit {
    /// Return the exact raw value before the edit, preserving absent cells.
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }

    /// Return the exact raw value written by the edit.
    pub fn after(&self) -> &str {
        &self.after
    }
}

/// A single worksheet.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// Cells keyed by coordinate.
    pub cells: BTreeMap<CellRef, Cell>,
    /// Sheet name.
    pub name: String,
}

impl Sheet {
    /// New sheet with a name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cells: BTreeMap::new(),
        }
    }

    /// Set a cell. Coordinates A1-style.
    pub fn set_str(&mut self, a1: &str, raw: &str) {
        if let Some(r) = CellRef::parse(a1) {
            self.set_raw(r, raw);
        }
    }

    /// Set a cell's exact raw input at a resolved cell reference.
    pub fn set_raw(&mut self, r: CellRef, raw: impl Into<String>) {
        self.cells.insert(r, Cell { raw: raw.into() });
    }

    /// Get raw content of a cell.
    pub fn raw(&self, r: CellRef) -> Option<&str> {
        self.cells.get(&r).map(|c| c.raw.as_str())
    }
}

/// A workbook = multiple sheets (single sheet used by engine core for now).
#[derive(Debug, Clone, Default)]
pub struct Workbook {
    /// Sheets in order.
    pub sheets: Vec<Sheet>,
}

impl Workbook {
    /// New workbook with one empty sheet.
    pub fn with_sheet(name: &str) -> Self {
        Self {
            sheets: vec![Sheet::new(name)],
        }
    }

    /// Get a sheet by index.
    pub fn sheet(&self, i: usize) -> Option<&Sheet> {
        self.sheets.get(i)
    }
}

/// Token types for the formula lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Cell(CellRef),
    Ident(String),
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
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
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
            '"' => {
                i += 1;
                let mut s = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch == '"' {
                        if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                            s.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        s.push(ch);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(CalcError::Parse);
                }
                tokens.push(Token::String(s));
            }
            c if c.is_ascii_digit() => {
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
                let num: f64 = input[start..i].parse().map_err(|_| CalcError::Parse)?;
                tokens.push(Token::Number(num));
            }
            c if c.is_ascii_alphabetic() => {
                // Could be a cell ref (e.g. A1), function name, or bare name.
                let letters_start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                    i += 1;
                }
                let letters = &input[letters_start..i];
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

/// AST expression.
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Text(String),
    Cell(CellRef),
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
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
        let lhs = self.parse_additive()?;
        if let Some(t) = self.peek() {
            let op = match t {
                Token::Eq => Some(BinOp::Eq),
                Token::Lt => Some(BinOp::Lt),
                Token::Gt => Some(BinOp::Gt),
                Token::Le => Some(BinOp::Le),
                Token::Ge => Some(BinOp::Ge),
                Token::Ne => Some(BinOp::Ne),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_additive()?;
                return Ok(Expr::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                });
            }
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

    fn parse_unary(&mut self) -> Result<Expr, CalcError> {
        if let Some(Token::Minus) = self.peek() {
            self.pos += 1;
            let inner = self.parse_unary()?;
            return Ok(Expr::Unary(Box::new(Expr::Number(-1.0).mul(inner))));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<Expr, CalcError> {
        let lhs = self.parse_primary()?;
        if let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            let rhs = self.parse_unary()?;
            return Ok(Expr::Binary {
                lhs: Box::new(lhs),
                op: BinOp::Pow,
                rhs: Box::new(rhs),
            });
        }
        Ok(lhs)
    }

    fn parse_primary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::String(s)) => Ok(Expr::Text(s)),
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
                        _ => Err(CalcError::Name),
                    }
                }
            }
            Some(Token::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            _ => Err(CalcError::Parse),
        }
    }
}

impl Expr {
    /// Multiply helper (for unary minus construction).
    fn mul(self, rhs: Expr) -> Expr {
        Expr::Binary {
            lhs: Box::new(self),
            op: BinOp::Mul,
            rhs: Box::new(rhs),
        }
    }
}

/// A parsed formula ready for evaluation.
#[derive(Debug)]
pub struct Formula {
    root: Expr,
}

/// Parse a formula body (without leading `=`).
pub fn parse_formula(body: &str) -> Result<Formula, CalcError> {
    let tokens = lex(body)?;
    let mut p = Parser::new(tokens);
    let root = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(CalcError::Parse);
    }
    Ok(Formula { root })
}

/// Collect all cell references touched by an expression (for the dependency graph).
fn collect_refs(e: &Expr, out: &mut HashSet<CellRef>) {
    match e {
        Expr::Cell(r) => {
            out.insert(*r);
        }
        Expr::Range { start, end } => {
            for row in start.row.min(end.row)..=start.row.max(end.row) {
                for col in start.col.min(end.col)..=start.col.max(end.col) {
                    out.insert(CellRef { row, col });
                }
            }
        }
        Expr::Unary(x) => collect_refs(x, out),
        Expr::Binary { lhs, rhs, .. } => {
            collect_refs(lhs, out);
            collect_refs(rhs, out);
        }
        Expr::Func { args, .. } => {
            for a in args {
                collect_refs(a, out);
            }
        }
        _ => {}
    }
}

/// Evaluate an expression against a resolved-cell lookup.
fn eval_expr(e: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Value {
    match e {
        Expr::Number(n) => Value::Number(*n),
        Expr::Text(s) => Value::Text(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Cell(r) => lookup(*r),
        Expr::Range { start, end: _ } => {
            // Range used directly where a scalar is expected -> take top-left.
            lookup(*start)
        }
        Expr::Unary(x) => {
            let v = eval_expr(x, lookup);
            match v {
                Value::Number(n) => Value::Number(-n),
                _ => Value::Error(CalcError::Value),
            }
        }
        Expr::Binary { lhs, op, rhs } => {
            let l = eval_expr(lhs, lookup);
            let r = eval_expr(rhs, lookup);
            eval_binary(&l, *op, &r)
        }
        Expr::Func { name, args } => eval_function(name, args, lookup),
    }
}

fn num(v: &Value) -> Result<f64, CalcError> {
    match v {
        Value::Number(n) => Ok(*n),
        _ => Err(CalcError::Value),
    }
}

fn eval_binary(l: &Value, op: BinOp, r: &Value) -> Value {
    // Comparison with text/bool/error semantics.
    if matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
    ) {
        if let (Ok(ln), Ok(rn)) = (num(l), num(r)) {
            let b = match op {
                BinOp::Eq => ln == rn,
                BinOp::Ne => ln != rn,
                BinOp::Lt => ln < rn,
                BinOp::Gt => ln > rn,
                BinOp::Le => ln <= rn,
                BinOp::Ge => ln >= rn,
                _ => unreachable!(),
            };
            return Value::Bool(b);
        }
        match op {
            BinOp::Eq => return Value::Bool(l == r),
            BinOp::Ne => return Value::Bool(l != r),
            _ => return Value::Error(CalcError::Value),
        }
    }
    let ln = match num(l) {
        Ok(n) => n,
        Err(_) => return Value::Error(CalcError::Value),
    };
    let rn = match num(r) {
        Ok(n) => n,
        Err(_) => return Value::Error(CalcError::Value),
    };
    match op {
        BinOp::Add => Value::Number(ln + rn),
        BinOp::Sub => Value::Number(ln - rn),
        BinOp::Mul => Value::Number(ln * rn),
        BinOp::Div => {
            if rn == 0.0 {
                Value::Error(CalcError::DivZero)
            } else {
                Value::Number(ln / rn)
            }
        }
        BinOp::Pow => Value::Number(ln.powf(rn)),
        _ => Value::Error(CalcError::Value),
    }
}

fn eval_function(name: &str, args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Value {
    // Flatten each argument: ranges expand to individual cell lookups.
    let mut values: Vec<Value> = Vec::new();
    for a in args {
        match a {
            Expr::Range { start, end } => {
                for row in start.row.min(end.row)..=start.row.max(end.row) {
                    for col in start.col.min(end.col)..=start.col.max(end.col) {
                        values.push(lookup(CellRef { row, col }));
                    }
                }
            }
            _ => values.push(eval_expr(a, lookup)),
        }
    }
    match name {
        "SUM" => {
            let mut t = 0.0;
            for v in &values {
                match v {
                    Value::Number(n) => t += n,
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            Value::Number(t)
        }
        "AVERAGE" => {
            let mut t = 0.0;
            let mut n = 0.0;
            for v in &values {
                match v {
                    Value::Number(x) => {
                        t += x;
                        n += 1.0;
                    }
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if n == 0.0 {
                Value::Error(CalcError::DivZero)
            } else {
                Value::Number(t / n)
            }
        }
        "MIN" => {
            let mut best = f64::INFINITY;
            for v in &values {
                match v {
                    Value::Number(x) => best = best.min(*x),
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if best.is_infinite() {
                Value::Error(CalcError::NA)
            } else {
                Value::Number(best)
            }
        }
        "MAX" => {
            let mut best = f64::NEG_INFINITY;
            for v in &values {
                match v {
                    Value::Number(x) => best = best.max(*x),
                    Value::Empty => {}
                    _ => return Value::Error(CalcError::Value),
                }
            }
            if best.is_infinite() {
                Value::Error(CalcError::NA)
            } else {
                Value::Number(best)
            }
        }
        "ABS" => {
            if values.len() != 1 {
                return Value::Error(CalcError::Value);
            }
            match num(&values[0]) {
                Ok(n) => Value::Number(n.abs()),
                Err(_) => Value::Error(CalcError::Value),
            }
        }
        "ROUND" => {
            if values.len() != 2 {
                return Value::Error(CalcError::Value);
            }
            match (num(&values[0]), num(&values[1])) {
                (Ok(n), Ok(d)) => {
                    let f = 10f64.powf(d);
                    Value::Number((n * f).round() / f)
                }
                _ => Value::Error(CalcError::Value),
            }
        }
        "CONCAT" => {
            let mut s = String::new();
            for v in &values {
                match v {
                    Value::Text(t) => s.push_str(t),
                    Value::Number(n) => s.push_str(&Value::Number(*n).display()),
                    Value::Bool(b) => s.push_str(&b.to_string()),
                    Value::Empty => {}
                    Value::Error(_) => return v.clone(),
                }
            }
            Value::Text(s)
        }
        _ => Value::Error(CalcError::Name),
    }
}

/// A resolved cell value plus its formula dependencies (for auditing).
#[derive(Debug, Clone)]
pub struct EvaluatedCell {
    /// Final value.
    pub value: Value,
}

/// Evaluate a sheet, returning a map of raw -> resolved value for every cell.
pub fn evaluate(sheet: &Sheet) -> HashMap<CellRef, Value> {
    let mut result: HashMap<CellRef, Value> = HashMap::new();

    // First pass: literal cells (non-formula).
    let mut formula_cells: Vec<CellRef> = Vec::new();
    for (r, c) in &sheet.cells {
        if c.is_formula() {
            formula_cells.push(*r);
        } else {
            result.insert(*r, parse_literal(&c.raw));
        }
    }

    // Build dependency graph among formula cells only.
    let mut deps: HashMap<CellRef, HashSet<CellRef>> = HashMap::new();
    let mut parsed: HashMap<CellRef, Formula> = HashMap::new();
    for r in &formula_cells {
        let body = sheet.raw(*r).unwrap()[1..].trim();
        match parse_formula(body) {
            Ok(f) => {
                let mut refs = HashSet::new();
                collect_refs(&f.root, &mut refs);
                // Only keep refs that are formula cells (literal deps need no ordering).
                let formula_refs: HashSet<CellRef> = refs
                    .iter()
                    .copied()
                    .filter(|rr| sheet.cells.get(rr).map(|c| c.is_formula()).unwrap_or(false))
                    .collect();
                deps.insert(*r, formula_refs);
                parsed.insert(*r, f);
            }
            Err(e) => {
                result.insert(*r, Value::Error(e));
            }
        }
    }

    // Topological order (Kahn's algorithm) with cycle detection.
    let mut in_degree: HashMap<CellRef, usize> = HashMap::new();
    for r in &formula_cells {
        in_degree.insert(*r, deps.get(r).map(|d| d.len()).unwrap_or(0));
    }
    let mut ready: Vec<CellRef> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(r, _)| *r)
        .collect();
    // Deterministic order.
    ready.sort();

    let mut evaluated_count = 0usize;
    let visited: &mut HashSet<CellRef> = &mut HashSet::new();
    while let Some(r) = ready.first().copied() {
        ready.remove(0);
        if visited.contains(&r) {
            continue;
        }
        visited.insert(r);
        // Evaluate.
        if let Some(f) = parsed.get(&r) {
            let lookup = |cr: CellRef| -> Value {
                if let Some(v) = result.get(&cr) {
                    v.clone()
                } else {
                    Value::Empty
                }
            };
            result.insert(r, eval_expr(&f.root, &lookup));
        }
        evaluated_count += 1;
        // Decrease in-degree of dependents.
        let dependents: Vec<CellRef> = formula_cells
            .iter()
            .copied()
            .filter(|other| deps.get(other).map(|d| d.contains(&r)).unwrap_or(false))
            .collect();
        for d in dependents {
            if let Some(deg) = in_degree.get_mut(&d) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 && !visited.contains(&d) {
                    ready.push(d);
                }
            }
        }
        ready.sort();
    }

    // Cycles or incomplete -> mark remaining formula cells as REF error.
    let total_formulas = formula_cells.len();
    if evaluated_count < total_formulas {
        for r in &formula_cells {
            if !visited.contains(r) {
                result.insert(*r, Value::Error(CalcError::Ref));
            }
        }
    }

    result
}

/// Parse a literal cell value (numbers, bools, text, empty).
pub fn parse_literal(s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        Value::Empty
    } else if let Ok(n) = s.parse::<f64>() {
        Value::Number(n)
    } else if s.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if s.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else {
        Value::Text(s.to_string())
    }
}

/// Export a sheet to CSV.
pub fn to_csv(sheet: &Sheet) -> String {
    let vals = evaluate(sheet);
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for r in sheet.cells.keys() {
        max_row = max_row.max(r.row);
        max_col = max_col.max(r.col);
    }
    let mut out = String::new();
    for row in 0..=max_row {
        let mut line = Vec::new();
        for col in 0..=max_col {
            let cr = CellRef { row, col };
            let v = vals.get(&cr).cloned().unwrap_or(Value::Empty);
            line.push(csv_escape(&v.display()));
        }
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Import a CSV into a sheet.
pub fn from_csv(name: &str, csv: &str) -> Sheet {
    let mut sheet = Sheet::new(name);
    for (row, line) in csv.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // Parse simple CSV (quoted fields, comma).
        let fields = parse_csv_line(line);
        for (col, f) in fields.iter().enumerate() {
            let cr = CellRef {
                row: row as u32,
                col: col as u32,
            };
            sheet.cells.insert(cr, Cell { raw: f.clone() });
        }
    }
    sheet
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            out.push(cur.clone());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

/// Serialize a sheet to the `.loomtable` content JSON.
pub fn sheet_to_json(sheet: &Sheet) -> String {
    let mut s = String::with_capacity(128);
    s.push('{');
    s.push_str("\"name\":\"");
    s.push_str(&sheet.name.replace('"', "\\\""));
    s.push_str("\",\"cells\":[");
    let mut first = true;
    for (r, c) in &sheet.cells {
        if !first {
            s.push(',');
        }
        first = false;
        s.push('{');
        s.push_str("\"ref\":\"");
        s.push_str(&r.to_a1());
        s.push_str("\",\"raw\":\"");
        s.push_str(
            &c.raw
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n"),
        );
        s.push_str("\"}");
    }
    s.push(']');
    s.push('}');
    s
}

/// Parse sheet JSON back.
pub fn sheet_from_json(s: &str) -> Result<Sheet, String> {
    // Extract name and cells using a minimal parser (name is up to first "cells").
    let mut name = String::new();
    if let Some(prefix) = s.split("\"cells\"").next() {
        if let Some(n) = prefix.split("\"name\":\"").nth(1) {
            // Cut at the closing quote that precedes `,"cells"`.
            let end = n.find('"').unwrap_or(n.len());
            name = n[..end].replace("\\\"", "\"").replace("\\\\", "\\");
        }
    }
    let mut sheet = Sheet::new(&name);
    // Parse each {"ref":"A1","raw":"..."}.
    let body = s.split("\"cells\":").nth(1).unwrap_or("[]");
    for frag in body.split("{\"ref\":\"") {
        if frag.is_empty() {
            continue;
        }
        let Some(end) = frag.find("\",\"raw\":\"") else {
            continue;
        };
        let a1 = &frag[..end];
        let rest = &frag[end + "\",\"raw\":\"".len()..];
        let Some(end2) = rest.find("\"}") else {
            continue;
        };
        let raw = &rest[..end2];
        let raw = raw
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n");
        sheet.set_str(a1, &raw);
    }
    Ok(sheet)
}

/// Inclusive rectangular cell range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRange {
    /// Top-left cell.
    pub start: CellRef,
    /// Bottom-right cell.
    pub end: CellRef,
}

impl CellRange {
    /// Creates a normalized range.
    pub fn new(a: CellRef, b: CellRef) -> Self {
        Self {
            start: CellRef {
                row: a.row.min(b.row),
                col: a.col.min(b.col),
            },
            end: CellRef {
                row: a.row.max(b.row),
                col: a.col.max(b.col),
            },
        }
    }

    /// Parses `A1:B4` or a single-cell `A1` range.
    pub fn parse(input: &str) -> Option<Self> {
        if let Some((left, right)) = input.split_once(':') {
            Some(Self::new(CellRef::parse(left)?, CellRef::parse(right)?))
        } else {
            let cell = CellRef::parse(input)?;
            Some(Self::new(cell, cell))
        }
    }

    /// Whether a cell is inside the range.
    pub fn contains(self, cell: CellRef) -> bool {
        cell.row >= self.start.row
            && cell.row <= self.end.row
            && cell.col >= self.start.col
            && cell.col <= self.end.col
    }

    /// Cells in row-major order.
    pub fn cells(self) -> Vec<CellRef> {
        let mut cells = Vec::new();
        for row in self.start.row..=self.end.row {
            for col in self.start.col..=self.end.col {
                cells.push(CellRef { row, col });
            }
        }
        cells
    }

    /// Renders A1 notation.
    pub fn to_a1(self) -> String {
        if self.start == self.end {
            self.start.to_a1()
        } else {
            format!("{}:{}", self.start.to_a1(), self.end.to_a1())
        }
    }
}

/// Named range available to formulas and navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedRange {
    /// Case-insensitive stable name.
    pub name: String,
    /// Target cells.
    pub range: CellRange,
}

/// Cell validation rule.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationRule {
    /// Any value is valid.
    Any,
    /// Number within optional inclusive bounds.
    Number {
        /// Minimum value.
        min: Option<f64>,
        /// Maximum value.
        max: Option<f64>,
    },
    /// Text from a fixed set.
    List(Vec<String>),
    /// Non-empty value.
    Required,
}

impl ValidationRule {
    /// Checks a resolved value.
    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            ValidationRule::Any => true,
            ValidationRule::Number { min, max } => match value {
                Value::Number(number) => {
                    min.map(|min| *number >= min).unwrap_or(true)
                        && max.map(|max| *number <= max).unwrap_or(true)
                }
                _ => false,
            },
            ValidationRule::List(options) => {
                options.iter().any(|option| option == &value.display())
            }
            ValidationRule::Required => {
                !matches!(value, Value::Empty) && !value.display().is_empty()
            }
        }
    }
}

/// Simple conditional-format comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatCondition {
    /// Numeric value is above threshold.
    GreaterThan(f64),
    /// Numeric value is below threshold.
    LessThan(f64),
    /// Display text contains substring, case-insensitive.
    TextContains(String),
    /// Cell contains any error.
    IsError,
}

impl FormatCondition {
    /// Evaluates a condition.
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            FormatCondition::GreaterThan(threshold) => {
                matches!(value, Value::Number(number) if number > threshold)
            }
            FormatCondition::LessThan(threshold) => {
                matches!(value, Value::Number(number) if number < threshold)
            }
            FormatCondition::TextContains(query) => value
                .display()
                .to_lowercase()
                .contains(&query.to_lowercase()),
            FormatCondition::IsError => matches!(value, Value::Error(_)),
        }
    }
}

/// Conditional-format rule carrying a semantic style id.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalFormatRule {
    /// Target range.
    pub range: CellRange,
    /// Predicate.
    pub condition: FormatCondition,
    /// Stable style identifier resolved by the UI.
    pub style_id: String,
}

/// Row filter.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterPredicate {
    /// Keep every row.
    All,
    /// Display value contains text.
    Contains(String),
    /// Numeric value is at least threshold.
    NumberAtLeast(f64),
    /// Resolved value is non-empty.
    NonEmpty,
}

impl FilterPredicate {
    fn matches(&self, value: &Value) -> bool {
        match self {
            FilterPredicate::All => true,
            FilterPredicate::Contains(query) => value
                .display()
                .to_lowercase()
                .contains(&query.to_lowercase()),
            FilterPredicate::NumberAtLeast(threshold) => {
                matches!(value, Value::Number(number) if number >= threshold)
            }
            FilterPredicate::NonEmpty => !matches!(value, Value::Empty),
        }
    }
}

/// Higher-level worksheet features layered over the formula engine.
#[derive(Debug, Clone)]
pub struct SheetModel {
    /// Cell data.
    pub sheet: Sheet,
    /// Named ranges keyed by uppercase name.
    pub named_ranges: BTreeMap<String, CellRange>,
    /// Validation rules.
    pub validations: Vec<(CellRange, ValidationRule)>,
    /// Conditional formatting.
    pub conditional_formats: Vec<ConditionalFormatRule>,
    /// Hidden row indexes.
    pub hidden_rows: HashSet<u32>,
    /// Frozen row count.
    pub frozen_rows: u32,
    /// Frozen column count.
    pub frozen_columns: u32,
}

impl SheetModel {
    /// Wraps a sheet.
    pub fn new(sheet: Sheet) -> Self {
        Self {
            sheet,
            named_ranges: BTreeMap::new(),
            validations: Vec::new(),
            conditional_formats: Vec::new(),
            hidden_rows: HashSet::new(),
            frozen_rows: 0,
            frozen_columns: 0,
        }
    }

    /// Adds or replaces a named range.
    pub fn set_named_range(&mut self, name: &str, range: CellRange) -> Result<(), String> {
        if !valid_named_range(name) {
            return Err(format!("invalid named range {name:?}"));
        }
        self.named_ranges.insert(name.to_ascii_uppercase(), range);
        Ok(())
    }

    /// Returns a temporary sheet whose formulas have named ranges expanded to A1 syntax.
    pub fn resolved_sheet(&self) -> Sheet {
        let mut sheet = self.sheet.clone();
        for cell in sheet.cells.values_mut() {
            if cell.is_formula() {
                cell.raw = expand_named_ranges(&cell.raw, &self.named_ranges);
            }
        }
        sheet
    }

    /// Evaluates formulas with named ranges.
    pub fn evaluate(&self) -> HashMap<CellRef, Value> {
        evaluate(&self.resolved_sheet())
    }

    /// Sets one cell only when all matching validation rules accept it.
    pub fn set_validated(&mut self, cell: CellRef, raw: &str) -> Result<(), String> {
        let mut preview = self.sheet.clone();
        preview.set_raw(cell, raw);
        let values = evaluate(&preview);
        let value = values.get(&cell).cloned().unwrap_or(Value::Empty);
        for (range, rule) in &self.validations {
            if range.contains(cell) && !rule.accepts(&value) {
                return Err(format!(
                    "value {:?} violates validation for {}",
                    value,
                    range.to_a1()
                ));
            }
        }
        self.sheet = preview;
        Ok(())
    }

    /// Style ids matching a cell's resolved value.
    pub fn conditional_style_ids(&self, cell: CellRef) -> Vec<&str> {
        let values = self.evaluate();
        let value = values.get(&cell).cloned().unwrap_or(Value::Empty);
        self.conditional_formats
            .iter()
            .filter(|rule| rule.range.contains(cell) && rule.condition.matches(&value))
            .map(|rule| rule.style_id.as_str())
            .collect()
    }

    /// Filters rows in `range` by a column relative to the range start.
    pub fn filter_rows(
        &mut self,
        range: CellRange,
        relative_column: u32,
        predicate: &FilterPredicate,
    ) -> Result<Vec<u32>, String> {
        let column = range
            .start
            .col
            .checked_add(relative_column)
            .ok_or_else(|| "filter column overflow".to_string())?;
        if column > range.end.col {
            return Err("filter column is outside the range".into());
        }
        let values = self.evaluate();
        let mut hidden = Vec::new();
        for row in range.start.row..=range.end.row {
            let value = values
                .get(&CellRef { row, col: column })
                .cloned()
                .unwrap_or(Value::Empty);
            if predicate.matches(&value) {
                self.hidden_rows.remove(&row);
            } else {
                self.hidden_rows.insert(row);
                hidden.push(row);
            }
        }
        Ok(hidden)
    }

    /// Sorts complete rows in a range by a relative column.
    pub fn sort_rows(
        &mut self,
        range: CellRange,
        relative_column: u32,
        ascending: bool,
    ) -> Result<(), String> {
        let sort_column = range
            .start
            .col
            .checked_add(relative_column)
            .ok_or_else(|| "sort column overflow".to_string())?;
        if sort_column > range.end.col {
            return Err("sort column is outside the range".into());
        }
        let values = self.evaluate();
        let mut rows: Vec<u32> = (range.start.row..=range.end.row).collect();
        rows.sort_by(|left, right| {
            let left_value = values
                .get(&CellRef {
                    row: *left,
                    col: sort_column,
                })
                .cloned()
                .unwrap_or(Value::Empty);
            let right_value = values
                .get(&CellRef {
                    row: *right,
                    col: sort_column,
                })
                .cloned()
                .unwrap_or(Value::Empty);
            let ordering = compare_values(&left_value, &right_value);
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
        let original = self.sheet.cells.clone();
        for (destination_offset, source_row) in rows.into_iter().enumerate() {
            let destination_row = range.start.row + destination_offset as u32;
            for col in range.start.col..=range.end.col {
                let source = CellRef {
                    row: source_row,
                    col,
                };
                let destination = CellRef {
                    row: destination_row,
                    col,
                };
                match original.get(&source) {
                    Some(cell) => {
                        self.sheet.cells.insert(destination, cell.clone());
                    }
                    None => {
                        self.sheet.cells.remove(&destination);
                    }
                }
            }
        }
        Ok(())
    }
}

fn valid_named_range(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && CellRef::parse(name).is_none()
}

fn expand_named_ranges(formula: &str, ranges: &BTreeMap<String, CellRange>) -> String {
    let mut output = String::with_capacity(formula.len());
    let bytes = formula.as_bytes();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        let character = bytes[index] as char;
        if character == '"' {
            quoted = !quoted;
            output.push(character);
            index += 1;
            continue;
        }
        if !quoted && (character.is_ascii_alphabetic() || character == '_') {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                if next.is_ascii_alphanumeric() || next == '_' {
                    index += 1;
                } else {
                    break;
                }
            }
            let token = &formula[start..index];
            if let Some(range) = ranges.get(&token.to_ascii_uppercase()) {
                output.push_str(&range.to_a1());
            } else {
                output.push_str(token);
            }
        } else {
            output.push(character);
            index += 1;
        }
    }
    output
}

fn compare_values(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.total_cmp(right),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Empty, Value::Empty) => std::cmp::Ordering::Equal,
        (Value::Empty, _) => std::cmp::Ordering::Greater,
        (_, Value::Empty) => std::cmp::Ordering::Less,
        _ => left
            .display()
            .to_lowercase()
            .cmp(&right.display().to_lowercase()),
    }
}

/// Cached dependency graph and values for incremental recalculation.
#[derive(Debug, Clone, Default)]
pub struct CalculationCache {
    /// Last resolved values.
    pub values: HashMap<CellRef, Value>,
    dependencies: HashMap<CellRef, HashSet<CellRef>>,
    dependents: HashMap<CellRef, HashSet<CellRef>>,
}

impl CalculationCache {
    /// Performs a full calculation and dependency-graph rebuild.
    pub fn rebuild(&mut self, sheet: &Sheet) {
        self.values = evaluate(sheet);
        let (dependencies, dependents) = dependency_graph(sheet);
        self.dependencies = dependencies;
        self.dependents = dependents;
    }

    /// Recalculates changed cells and their transitive dependents.
    ///
    /// Formula parsing for the graph is linear in the sheet's formula count,
    /// while evaluation is restricted to the affected subgraph.
    pub fn recalculate(&mut self, sheet: &Sheet, changed: &[CellRef]) -> HashSet<CellRef> {
        let (dependencies, dependents) = dependency_graph(sheet);
        self.dependencies = dependencies;
        self.dependents = dependents;
        let mut affected: HashSet<CellRef> = changed.iter().copied().collect();
        let mut queue: Vec<CellRef> = changed.to_vec();
        while let Some(cell) = queue.pop() {
            if let Some(next) = self.dependents.get(&cell) {
                for dependent in next {
                    if affected.insert(*dependent) {
                        queue.push(*dependent);
                    }
                }
            }
        }
        for cell in &affected {
            self.values.remove(cell);
        }
        let mut remaining = affected.clone();
        let mut progress = true;
        while progress && !remaining.is_empty() {
            progress = false;
            let ready: Vec<CellRef> = remaining
                .iter()
                .copied()
                .filter(|cell| {
                    self.dependencies
                        .get(cell)
                        .map(|deps| deps.iter().all(|dep| !remaining.contains(dep)))
                        .unwrap_or(true)
                })
                .collect();
            for cell in ready {
                let value = match sheet.cells.get(&cell) {
                    None => Value::Empty,
                    Some(raw) if !raw.is_formula() => parse_literal(&raw.raw),
                    Some(raw) => match parse_formula(raw.raw[1..].trim()) {
                        Ok(formula) => {
                            let lookup = |reference: CellRef| {
                                self.values.get(&reference).cloned().unwrap_or(Value::Empty)
                            };
                            eval_expr(&formula.root, &lookup)
                        }
                        Err(error) => Value::Error(error),
                    },
                };
                self.values.insert(cell, value);
                remaining.remove(&cell);
                progress = true;
            }
        }
        for cell in remaining {
            self.values.insert(cell, Value::Error(CalcError::Ref));
        }
        affected
    }
}

fn dependency_graph(
    sheet: &Sheet,
) -> (
    HashMap<CellRef, HashSet<CellRef>>,
    HashMap<CellRef, HashSet<CellRef>>,
) {
    let mut dependencies = HashMap::new();
    let mut dependents: HashMap<CellRef, HashSet<CellRef>> = HashMap::new();
    for (cell_ref, cell) in &sheet.cells {
        if !cell.is_formula() {
            dependencies.insert(*cell_ref, HashSet::new());
            continue;
        }
        let mut refs = HashSet::new();
        if let Ok(formula) = parse_formula(cell.raw[1..].trim()) {
            collect_refs(&formula.root, &mut refs);
        }
        for dependency in &refs {
            dependents.entry(*dependency).or_default().insert(*cell_ref);
        }
        dependencies.insert(*cell_ref, refs);
    }
    (dependencies, dependents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_ref_parse_render() {
        let a1 = CellRef::parse("A1").unwrap();
        assert_eq!(a1, CellRef { row: 0, col: 0 });
        assert_eq!(a1.to_a1(), "A1");
        assert_eq!(CellRef::parse("B3").unwrap().to_a1(), "B3");
        assert_eq!(CellRef::parse("AA10").unwrap(), CellRef { row: 9, col: 26 });
        assert!(CellRef::parse("").is_none());
        assert!(CellRef::parse("1A").is_none());
        assert!(CellRef::parse("A0").is_none());
    }

    #[test]
    fn basic_arithmetic() {
        let f = parse_formula("1+2*3").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        let v = eval_expr(&f.root, &lookup);
        assert_eq!(v, Value::Number(7.0));
    }

    #[test]
    fn precedence_and_parens() {
        let f = parse_formula("(1+2)*3").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        assert_eq!(eval_expr(&f.root, &lookup), Value::Number(9.0));
        let f2 = parse_formula("2^3^2").unwrap();
        assert_eq!(eval_expr(&f2.root, &lookup), Value::Number(512.0));
    }

    #[test]
    fn division_by_zero() {
        let f = parse_formula("1/0").unwrap();
        let lookup = |_: CellRef| Value::Empty;
        assert_eq!(
            eval_expr(&f.root, &lookup),
            Value::Error(CalcError::DivZero)
        );
    }

    #[test]
    fn cell_references_resolve() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "5");
        sheet.set_str("C1", "=A1+B1*2");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("C1").unwrap()),
            Some(&Value::Number(20.0))
        );
    }

    #[test]
    fn dependency_order_and_cycle() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=B1+1");
        sheet.set_str("B1", "=C1+1");
        sheet.set_str("C1", "5");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(6.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Number(7.0))
        );

        // Cycle A1 <-> B1.
        let mut cyc = Sheet::new("c");
        cyc.set_str("A1", "=B1");
        cyc.set_str("B1", "=A1");
        let vals = evaluate(&cyc);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Ref))
        );
    }

    #[test]
    fn functions() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "2");
        sheet.set_str("A3", "3");
        sheet.set_str("B1", "=SUM(A1:A3)");
        sheet.set_str("B2", "=AVERAGE(A1:A3)");
        sheet.set_str("B3", "=ABS(-42)");
        sheet.set_str("B4", "=ROUND(1.567,2)");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(6.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B2").unwrap()),
            Some(&Value::Number(2.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B3").unwrap()),
            Some(&Value::Number(42.0))
        );
        assert_eq!(
            vals.get(&CellRef::parse("B4").unwrap()),
            Some(&Value::Number(1.57))
        );
    }

    #[test]
    fn comparison_and_text() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=1<2");
        sheet.set_str("A2", "=CONCAT(\"loom\",\"-\",\"sheets\")");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            vals.get(&CellRef::parse("A2").unwrap()),
            Some(&Value::Text("loom-sheets".to_string()))
        );
    }

    #[test]
    fn parse_error_reports() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "=1+");
        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Parse))
        );
    }

    #[test]
    fn csv_roundtrip() {
        let mut sheet = Sheet::new("data");
        sheet.set_str("A1", "10");
        sheet.set_str("B1", "\"a,b\"");
        sheet.set_str("A2", "=A1*2");
        let csv = to_csv(&sheet);
        assert!(csv.contains("10"));
        assert!(csv.contains("\"a,b\""));
        let loaded = from_csv("data", &csv);
        let vals = evaluate(&loaded);
        // A1 = 10 (literal), A2 self-referential cycle? No: A2 depends on A1 literal.
        assert_eq!(
            vals.get(&CellRef::parse("A2").unwrap()),
            Some(&Value::Number(20.0))
        );
    }

    #[test]
    fn json_roundtrip() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("B2", "=A1+1");
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        assert_eq!(back.name, "t");
        assert_eq!(back.raw(CellRef::parse("B2").unwrap()), Some("=A1+1"));
        let vals = evaluate(&back);
        assert_eq!(
            vals.get(&CellRef::parse("B2").unwrap()),
            Some(&Value::Number(2.0))
        );
    }

    #[test]
    fn editing_a_cell_retains_formula_and_empty_raw_semantics() {
        let mut sheet = Sheet::new("t");
        let cell = CellRef::parse("B1").unwrap();
        sheet.set_str("A1", "2");

        sheet.set_raw(cell, "=A1+1");
        assert_eq!(sheet.raw(cell), Some("=A1+1"));
        assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Number(3.0)));

        sheet.set_raw(cell, "");
        assert_eq!(sheet.raw(cell), Some(""));
        assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Empty));
    }

    #[test]
    fn formula_edit_transaction_commits_once_and_cancels_without_mutation() {
        let mut canceled = CellEditTransaction::begin(Some("=A1+1"));
        canceled.update("=A1+12");
        assert_eq!(canceled.cancel(), Some("=A1+1".to_string()));

        let mut committed = CellEditTransaction::begin(Some("=A1+1"));
        committed.update("=A1+12");
        let edit = committed.commit().expect("changed draft commits");
        assert_eq!(edit.before(), Some("=A1+1"));
        assert_eq!(edit.after(), "=A1+12");

        let mut unchanged = CellEditTransaction::begin(Some("=A1+1"));
        unchanged.update("=A1+1");
        assert!(unchanged.commit().is_none());

        let mut empty = CellEditTransaction::begin(Some("=A1+1"));
        empty.update("");
        let empty_edit = empty.commit().expect("empty text is a raw edit");
        assert_eq!(empty_edit.after(), "");

        let mut new_empty = CellEditTransaction::begin(None);
        new_empty.update("");
        let new_empty_edit = new_empty
            .commit()
            .expect("empty text differs from absent raw");
        assert_eq!(new_empty_edit.before(), None);
        assert_eq!(new_empty_edit.after(), "");
    }

    #[test]
    fn literal_parsing() {
        assert_eq!(parse_literal("  "), Value::Empty);
        assert_eq!(parse_literal("3.5"), Value::Number(3.5));
        assert_eq!(parse_literal("TRUE"), Value::Bool(true));
        assert_eq!(parse_literal("hello"), Value::Text("hello".to_string()));
    }

    #[test]
    fn named_ranges_validation_and_conditional_formatting_work() {
        let mut sheet = Sheet::new("model");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("B1", "=SUM(DATA)");
        let mut model = SheetModel::new(sheet);
        model
            .set_named_range("DATA", CellRange::parse("A1:A2").unwrap())
            .unwrap();
        model.validations.push((
            CellRange::parse("A1:A2").unwrap(),
            ValidationRule::Number {
                min: Some(0.0),
                max: Some(100.0),
            },
        ));
        model.conditional_formats.push(ConditionalFormatRule {
            range: CellRange::parse("A1:A2").unwrap(),
            condition: FormatCondition::GreaterThan(15.0),
            style_id: "high".into(),
        });
        assert_eq!(
            model.evaluate().get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(30.0))
        );
        assert!(model
            .set_validated(CellRef::parse("A1").unwrap(), "-1")
            .is_err());
        assert_eq!(
            model.conditional_style_ids(CellRef::parse("A2").unwrap()),
            vec!["high"]
        );
    }

    #[test]
    fn rows_sort_filter_and_ranges_are_deterministic() {
        let mut sheet = Sheet::new("rows");
        sheet.set_str("A1", "b");
        sheet.set_str("B1", "2");
        sheet.set_str("A2", "a");
        sheet.set_str("B2", "1");
        let mut model = SheetModel::new(sheet);
        let range = CellRange::parse("A1:B2").unwrap();
        model.sort_rows(range, 1, true).unwrap();
        assert_eq!(model.sheet.raw(CellRef::parse("A1").unwrap()), Some("a"));
        let hidden = model
            .filter_rows(range, 0, &FilterPredicate::Contains("a".into()))
            .unwrap();
        assert_eq!(hidden, vec![1]);
        assert_eq!(range.cells().len(), 4);
    }

    #[test]
    fn calculation_cache_recalculates_transitive_dependents() {
        let mut sheet = Sheet::new("incremental");
        sheet.set_str("A1", "1");
        sheet.set_str("B1", "=A1+1");
        sheet.set_str("C1", "=B1+1");
        sheet.set_str("Z1", "99");
        let mut cache = CalculationCache::default();
        cache.rebuild(&sheet);
        sheet.set_str("A1", "10");
        let affected = cache.recalculate(&sheet, &[CellRef::parse("A1").unwrap()]);
        assert!(affected.contains(&CellRef::parse("C1").unwrap()));
        assert!(!affected.contains(&CellRef::parse("Z1").unwrap()));
        assert_eq!(
            cache.values.get(&CellRef::parse("C1").unwrap()),
            Some(&Value::Number(12.0))
        );
    }
}
