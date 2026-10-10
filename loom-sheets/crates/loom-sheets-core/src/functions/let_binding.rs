//! LET(name1, value1, [name2, value2, ...], calculation) (spike C). Names are
//! bound in order and each value sees the names bound before it. A name bound
//! twice in one LET is rejected when the formula is parsed, as Excel refuses it
//! on entry (see `repeats_a_name`). A value that is a reference stays a
//! reference, so ranges keep working. Any other value is computed once and bound
//! as a constant, so a volatile value such as RAND is drawn only once.

use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// The names bound so far with their replacements. A LET binds each name once.
type Bindings = Vec<(String, Expr)>;

/// Dispatch LET from the main evaluator.
pub(crate) fn eval_let_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    if name != "LET" {
        return None;
    }
    Some(let_function(args, lookup))
}

fn let_function(args: &[Expr], lookup: Lookup) -> Value {
    // Name/value pairs followed by one calculation: an odd count of at least three.
    if args.len() < 3 || args.len() % 2 == 0 {
        return Value::Error(CalcError::Value);
    }
    let Some((calculation, pairs)) = args.split_last() else {
        return Value::Error(CalcError::Value);
    };
    let mut bindings: Bindings = Vec::new();
    for pair in pairs.chunks(2) {
        let Expr::Name(bound) = &pair[0] else {
            return Value::Error(CalcError::Value);
        };
        let value = substitute(&pair[1], &bindings);
        let replacement = if is_reference(&value) {
            value
        } else {
            Expr::Constant(eval_expr(&value, lookup))
        };
        bindings.push((bound.clone(), replacement));
    }
    eval_expr(&substitute(calculation, &bindings), lookup)
}

/// True when a LET in `expr`, at any depth, binds the same name twice. Excel refuses
/// such a formula on entry, so the parser rejects it even in a branch never evaluated.
pub(crate) fn repeats_a_name(expr: &Expr) -> bool {
    match expr {
        Expr::Func { name, args } => {
            (name == "LET" && bound_names_repeat(args)) || args.iter().any(repeats_a_name)
        }
        Expr::Unary(inner) => repeats_a_name(inner),
        Expr::Binary { lhs, rhs, .. } => repeats_a_name(lhs) || repeats_a_name(rhs),
        _ => false,
    }
}

/// Whether the names bound by one LET's argument list (name and value pairs, then
/// the calculation) include a name twice. Names are case-insensitive; the lexer
/// stores them in upper case, so an exact comparison is enough.
fn bound_names_repeat(args: &[Expr]) -> bool {
    let Some((_, pairs)) = args.split_last() else {
        return false;
    };
    let names: Vec<&String> = pairs
        .chunks(2)
        .filter_map(|pair| match pair.first() {
            Some(Expr::Name(name)) => Some(name),
            _ => None,
        })
        .collect();
    names
        .iter()
        .enumerate()
        .any(|(index, name)| names[..index].contains(name))
}

fn is_reference(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Range { .. } | Expr::Cell(_) | Expr::SheetRange { .. } | Expr::SheetCell { .. }
    )
}

/// `expr` with every bound name replaced by its binding.
fn substitute(expr: &Expr, bindings: &Bindings) -> Expr {
    match expr {
        Expr::Name(name) => bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound == name)
            .map_or_else(|| expr.clone(), |(_, replacement)| replacement.clone()),
        Expr::Unary(inner) => Expr::Unary(Box::new(substitute(inner, bindings))),
        Expr::Binary { lhs, op, rhs } => Expr::Binary {
            lhs: Box::new(substitute(lhs, bindings)),
            op: *op,
            rhs: Box::new(substitute(rhs, bindings)),
        },
        Expr::Func { name, args } if name == "LET" => Expr::Func {
            name: name.clone(),
            args: substitute_nested_let(args, bindings),
        },
        Expr::Func { name, args } => Expr::Func {
            name: name.clone(),
            args: args.iter().map(|arg| substitute(arg, bindings)).collect(),
        },
        other => other.clone(),
    }
}

/// Substitute inside a LET nested in a value or calculation. Its own names are
/// kept as written: each of its values sees the outer bindings not yet
/// shadowed, and the names it binds hide outer bindings for the rest of it.
fn substitute_nested_let(args: &[Expr], bindings: &Bindings) -> Vec<Expr> {
    let Some((calculation, pairs)) = args.split_last() else {
        return Vec::new();
    };
    let mut scope = bindings.clone();
    let mut out = Vec::with_capacity(args.len());
    for pair in pairs.chunks(2) {
        out.push(pair[0].clone());
        if let [_, value] = pair {
            out.push(substitute(value, &scope));
        }
        if let Expr::Name(bound) = &pair[0] {
            scope.retain(|(earlier, _)| earlier != bound);
        }
    }
    out.push(substitute(calculation, &scope));
    out
}
