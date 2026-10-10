//! LET(name1, value1, [name2, value2, ...], calculation) (spike C). Names are
//! bound in order and each value sees the names bound before it; a later
//! binding of a name shadows an earlier one. A value that is a reference stays
//! a reference, so ranges keep working. Any other value is computed once and
//! bound as a constant, so a volatile value such as RAND is drawn only once.

use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// The names bound so far with their replacements; a later entry shadows an
/// earlier one of the same name.
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
        bindings.retain(|(earlier, _)| earlier != bound);
        bindings.push((bound.clone(), replacement));
    }
    eval_expr(&substitute(calculation, &bindings), lookup)
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
