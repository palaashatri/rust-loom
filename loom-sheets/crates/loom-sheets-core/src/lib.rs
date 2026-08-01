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
            self.cells.insert(
                r,
                Cell {
                    raw: raw.to_string(),
                },
            );
        }
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
    fn literal_parsing() {
        assert_eq!(parse_literal("  "), Value::Empty);
        assert_eq!(parse_literal("3.5"), Value::Number(3.5));
        assert_eq!(parse_literal("TRUE"), Value::Bool(true));
        assert_eq!(parse_literal("hello"), Value::Text("hello".to_string()));
    }
}
