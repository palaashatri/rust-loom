//! Multi-sheet workbook evaluation with cross-sheet references.
//!
//! `evaluate_workbook` generalizes the single-sheet topological evaluator to
//! a combined dependency graph over `(sheet, cell)` nodes. Foreign nodes such
//! as `Sheet2!A1` are rewritten to fresh synthetic locals backed by an
//! overlay map, so the existing scalar evaluator runs unchanged: no signature
//! churn, identical single-sheet semantics (including cycle handling).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    collect_refs, eval_expr, parse_formula, parse_literal, CalcError, CellRef, Expr, Sheet, Value,
};

/// A node in the combined workbook dependency graph.
type Node = (usize, CellRef);

/// Evaluate every sheet, resolving cross-sheet references across tabs.
/// Unknown sheet qualifiers evaluate to `#REF!`; empty foreign cells read as
/// empty. Sheets keep their input order in the returned vector.
pub fn evaluate_workbook(sheets: &[Sheet]) -> Vec<HashMap<CellRef, Value>> {
    let mut results: Vec<HashMap<CellRef, Value>> = vec![HashMap::new(); sheets.len()];
    if sheets.is_empty() {
        return results;
    }
    // First-wins sheet index by lowercased name.
    let mut names: HashMap<String, usize> = HashMap::new();
    for (index, sheet) in sheets.iter().enumerate() {
        names
            .entry(sheet.name.to_ascii_lowercase())
            .or_insert(index);
    }

    // First pass: literal cells (non-formula) on every sheet.
    let mut formula_cells: Vec<Node> = Vec::new();
    for (index, sheet) in sheets.iter().enumerate() {
        for (cell, raw) in &sheet.cells {
            if raw.is_formula() {
                formula_cells.push((index, *cell));
            } else {
                results[index].insert(*cell, parse_literal(&raw.raw));
            }
        }
    }

    // Parse every formula cell; failures record immediately like single-sheet.
    let mut parsed: HashMap<Node, crate::Formula> = HashMap::new();
    let mut deps: HashMap<Node, HashSet<Node>> = HashMap::new();
    for &(index, cell) in &formula_cells {
        let body = sheets[index].raw(cell).unwrap()[1..].trim().to_string();
        match parse_formula(&body) {
            Ok(formula) => {
                let mut local = HashSet::new();
                collect_refs(&formula.root, &mut local);
                let mut edges: HashSet<Node> = HashSet::new();
                for dep in local {
                    if sheets[index]
                        .cells
                        .get(&dep)
                        .map(|c| c.is_formula())
                        .unwrap_or(false)
                    {
                        edges.insert((index, dep));
                    }
                }
                for (name, dep) in collect_foreign_refs(&formula.root) {
                    if let Some(&target) = names.get(&name.to_ascii_lowercase()) {
                        if sheets[target]
                            .cells
                            .get(&dep)
                            .map(|c| c.is_formula())
                            .unwrap_or(false)
                        {
                            edges.insert((target, dep));
                        }
                    }
                }
                deps.insert((index, cell), edges);
                parsed.insert((index, cell), formula);
            }
            Err(error) => {
                results[index].insert(cell, Value::Error(error));
            }
        }
    }

    // Iterative demand-driven resolution with an explicit stack: no recursion
    // depth limits on long fill-down chains. Roots run in sorted order and
    // dependency pushes are sorted, so resolution order is deterministic;
    // memo maps never drive control flow. Spill placement can invalidate
    // earlier reads, so evaluation runs in phases until no formula read a
    // changed cell (cap = formula count + 2; leftovers become #REF!).
    let mut reads: HashMap<Node, HashMap<Node, Value>> = HashMap::new();
    let mut placed: HashSet<Node> = HashSet::new();
    let mut claims: HashMap<Node, Node> = HashMap::new();
    let mut pending: Vec<Node> = parsed.keys().cloned().collect();
    pending.sort();
    let phase_cap = pending.len() + 2;
    let mut phases = 0usize;
    enum Frame {
        Expand(Node),
        Eval(Node),
    }
    loop {
        phases += 1;
        let rerun: HashSet<Node> = pending.iter().cloned().collect();
        // Clear spill targets owned by nodes about to re-run so shifted
        // areas leave no phantom values behind.
        let stale: Vec<Node> = claims
            .iter()
            .filter(|(_, owner)| rerun.contains(owner))
            .map(|(target, _)| *target)
            .collect();
        for target in stale {
            claims.remove(&target);
            placed.remove(&target);
            if target.0 < results.len() {
                results[target.0].remove(&target.1);
            }
        }
        let mut evaluated: HashSet<Node> = HashSet::new();
        let mut visiting: HashSet<Node> = HashSet::new();
        let mut stack: Vec<Frame> = pending.iter().rev().cloned().map(Frame::Expand).collect();
        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Expand(node) => {
                    if evaluated.contains(&node) || !rerun.contains(&node) {
                        continue;
                    }
                    if !visiting.insert(node) {
                        results[node.0].insert(node.1, Value::Error(CalcError::Ref));
                        evaluated.insert(node);
                        continue;
                    }
                    stack.push(Frame::Eval(node));
                    if let Some(depset) = deps.get(&node) {
                        let mut ordered: Vec<Node> = depset.iter().cloned().collect();
                        ordered.sort();
                        for dep in ordered.iter().rev() {
                            stack.push(Frame::Expand(*dep));
                        }
                    }
                }
                Frame::Eval(node) => {
                    visiting.remove(&node);
                    if evaluated.contains(&node) || !rerun.contains(&node) {
                        continue;
                    }
                    evaluated.insert(node);
                    if let Some(formula) = parsed.get(&node) {
                        let (value, seen) =
                            eval_workbook_node(sheets, &names, &results, node.0, &formula.root);
                        reads.insert(node, seen);
                        place_spill(sheets, &mut results, &mut claims, &mut placed, node, &value);
                    }
                }
            }
        }
        // Dirty set: nodes whose recorded reads no longer match memo, plus
        // transitive readers of dirty nodes.
        let mut dirty: HashSet<Node> = HashSet::new();
        for (node, seen) in &reads {
            for (target, value) in seen {
                let current = if target.0 < results.len() {
                    results[target.0]
                        .get(&target.1)
                        .cloned()
                        .unwrap_or(Value::Empty)
                } else {
                    Value::Error(CalcError::Ref)
                };
                if current != *value {
                    dirty.insert(*node);
                    break;
                }
            }
        }
        loop {
            let mut extra = Vec::new();
            for (node, seen) in &reads {
                if dirty.contains(node) {
                    continue;
                }
                if seen.keys().any(|t| dirty.contains(t)) {
                    extra.push(*node);
                }
            }
            if extra.is_empty() {
                break;
            }
            for node in extra {
                dirty.insert(node);
            }
        }
        // Only parsed formula nodes can re-run.
        pending = dirty
            .into_iter()
            .filter(|n| parsed.contains_key(n))
            .collect();
        pending.sort();
        if pending.is_empty() {
            break;
        }
        if phases >= phase_cap {
            for node in &pending {
                results[node.0].insert(node.1, Value::Error(CalcError::Ref));
                // Drop spill targets owned by nodes that never converged so
                // no phantom values linger on empty cells.
                let owned: Vec<Node> = claims
                    .iter()
                    .filter(|(_, owner)| *owner == node)
                    .map(|(target, _)| *target)
                    .collect();
                for target in owned {
                    claims.remove(&target);
                    placed.remove(&target);
                    if target.0 < results.len() {
                        results[target.0].remove(&target.1);
                    }
                }
            }
            break;
        }
    }

    results
}

/// Evaluate one formula node: foreign references resolve from the current
/// memo through a synthetic-cell overlay, then the plain scalar evaluator
/// runs. Returns the value plus every memo cell read (local and foreign) so
/// the phase driver can detect cells invalidated by later spill placement.
fn eval_workbook_node(
    sheets: &[Sheet],
    names: &HashMap<String, usize>,
    results: &[HashMap<CellRef, Value>],
    index: usize,
    root: &Expr,
) -> (Value, HashMap<Node, Value>) {
    let seen = RefCell::new(HashMap::<Node, Value>::new());
    let foreign = collect_foreign_refs(root);
    if foreign.is_empty() {
        let lookup = |cell: CellRef| -> Value {
            let value = results[index].get(&cell).cloned().unwrap_or(Value::Empty);
            seen.borrow_mut().insert((index, cell), value.clone());
            value
        };
        let value = eval_expr(root, &lookup);
        return (value, seen.into_inner());
    }
    // Resolve every foreign cell up front.
    let mut overlay: HashMap<CellRef, Value> = HashMap::new();
    let mut taken: BTreeSet<(u32, u32)> =
        sheets[index].cells.keys().map(|c| (c.row, c.col)).collect();
    // Group contiguous foreign ranges so one synthetic block serves each.
    let mut range_blocks: Vec<(String, CellRef, CellRef)> = Vec::new();
    let mut single_cells: Vec<(String, CellRef)> = Vec::new();
    collect_foreign_nodes(root, &mut range_blocks, &mut single_cells);
    let mut rewritten = root.clone();
    for (name, start, end) in range_blocks {
        let min_row = start.row.min(end.row);
        let max_row = start.row.max(end.row);
        let min_col = start.col.min(end.col);
        let max_col = start.col.max(end.col);
        let height = (max_row - min_row + 1) as usize;
        let width = (max_col - min_col + 1) as usize;
        let (block_row, block_col) = claim_block(&mut taken, width, height);
        for dr in 0..height {
            for dc in 0..width {
                let row = min_row + dr as u32;
                let col = min_col + dc as u32;
                let value = resolve_foreign(names, results, &name, CellRef { row, col });
                if let Some(&target) = names.get(&name.to_ascii_lowercase()) {
                    seen.borrow_mut()
                        .insert((target, CellRef { row, col }), value.clone());
                }
                overlay.insert(
                    CellRef {
                        row: block_row + dr as u32,
                        col: block_col + dc as u32,
                    },
                    value,
                );
            }
        }
        let new_start = CellRef {
            row: block_row,
            col: block_col,
        };
        let new_end = CellRef {
            row: block_row + height as u32 - 1,
            col: block_col + width as u32 - 1,
        };
        rewritten = substitute_range(&rewritten, &name, &start, &end, new_start, new_end);
    }
    for (name, cell) in single_cells {
        // Substitution replaces every matching node at once, so a repeated
        // cell simply finds nothing left to replace on later passes.
        let value = resolve_foreign(names, results, &name, cell);
        if let Some(&target) = names.get(&name.to_ascii_lowercase()) {
            seen.borrow_mut().insert((target, cell), value.clone());
        }
        let fresh = claim_cell(&mut taken);
        overlay.insert(fresh, value);
        rewritten = substitute_cell(&rewritten, &name, &cell, fresh);
    }
    let lookup = |cell: CellRef| -> Value {
        if let Some(value) = overlay.get(&cell) {
            return value.clone();
        }
        let value = results[index].get(&cell).cloned().unwrap_or(Value::Empty);
        seen.borrow_mut().insert((index, cell), value.clone());
        value
    };
    let value = eval_expr(&rewritten, &lookup);
    (value, seen.into_inner())
}

/// Read a foreign cell: evaluated value when available, empty for blank
/// cells, `#REF!` for unknown sheets.
fn resolve_foreign(
    names: &HashMap<String, usize>,
    results: &[HashMap<CellRef, Value>],
    name: &str,
    cell: CellRef,
) -> Value {
    match names.get(&name.to_ascii_lowercase()) {
        Some(&target) => results[target].get(&cell).cloned().unwrap_or(Value::Empty),
        None => Value::Error(CalcError::Ref),
    }
}

/// Store one node's value, spilling array results into neighboring empty
/// cells. Every spill target must be free of raw content and of other
/// owners' claims; otherwise the owner becomes `#SPILL!` and nothing is
/// placed. All checks run before any claim so a blocked spill leaves no
/// partial footprint.
fn place_spill(
    sheets: &[Sheet],
    results: &mut [HashMap<CellRef, Value>],
    claims: &mut HashMap<Node, Node>,
    placed: &mut HashSet<Node>,
    owner: Node,
    value: &Value,
) {
    let (values, width, height) = match value {
        Value::Array(values, width, height) => (values, *width, *height),
        _ => {
            results[owner.0].insert(owner.1, value.clone());
            return;
        }
    };
    if width == 0 || height == 0 || values.is_empty() {
        results[owner.0].insert(owner.1, value.clone());
        return;
    }
    let mut targets: Vec<(Node, Value)> = Vec::new();
    let mut blocked = false;
    for dr in 0..height {
        for dc in 0..width {
            if dr == 0 && dc == 0 {
                continue;
            }
            let (Some(row), Some(col)) = (
                owner.1.row.checked_add(dr as u32),
                owner.1.col.checked_add(dc as u32),
            ) else {
                blocked = true;
                break;
            };
            let cell = CellRef { row, col };
            let target = (owner.0, cell);
            let occupied = sheets[owner.0]
                .raw(cell)
                .map(|raw| !raw.trim().is_empty())
                .unwrap_or(false);
            let claimed = claims.get(&target).map(|o| *o != owner).unwrap_or(false);
            if occupied || claimed {
                blocked = true;
                break;
            }
            let element = values.get(dr * width + dc).cloned().unwrap_or(Value::Empty);
            targets.push((target, element));
        }
        if blocked {
            break;
        }
    }
    if blocked {
        results[owner.0].insert(owner.1, Value::Error(CalcError::Spill));
        return;
    }
    results[owner.0].insert(owner.1, value.clone());
    for (target, element) in targets {
        claims.insert(target, owner);
        placed.insert(target);
        results[target.0].insert(target.1, element);
    }
}

/// Claim one address absent from the sheet for synthetic overlay use.
fn claim_cell(taken: &mut BTreeSet<(u32, u32)>) -> CellRef {
    let (row, col) = claim_block(taken, 1, 1);
    CellRef { row, col }
}

/// Claim a `width` x `height` block absent from the sheet, scanning down
/// from the top of the address space (collisions are near-impossible and
/// the scan terminates on the first free block).
fn claim_block(taken: &mut BTreeSet<(u32, u32)>, width: usize, height: usize) -> (u32, u32) {
    let width = width.max(1) as u32;
    let height = height.max(1) as u32;
    let mut row = u32::MAX.saturating_sub(height);
    loop {
        let mut col: u32 = 0;
        loop {
            // Keep col + dc in bounds for every dc below.
            if col > u32::MAX - width {
                break;
            }
            let free =
                (0..height).all(|dr| (0..width).all(|dc| !taken.contains(&(row + dr, col + dc))));
            if free {
                for dr in 0..height {
                    for dc in 0..width {
                        taken.insert((row + dr, col + dc));
                    }
                }
                return (row, col);
            }
            match col.checked_add(width) {
                Some(next) => col = next,
                None => break,
            }
        }
        match row.checked_sub(height) {
            Some(prev) => row = prev,
            None => return (u32::MAX.saturating_sub(height), 0),
        }
    }
}

/// Collect every foreign cell touched by an expression, ranges expanded.
fn collect_foreign_refs(root: &Expr) -> Vec<(String, CellRef)> {
    let mut ranges = Vec::new();
    let mut singles = Vec::new();
    collect_foreign_nodes(root, &mut ranges, &mut singles);
    let mut out = singles;
    for (_, start, end) in ranges {
        for row in start.row.min(end.row)..=start.row.max(end.row) {
            for col in start.col.min(end.col)..=start.col.max(end.col) {
                // Name lookup happens per range below; placeholder replaced.
                out.push((String::new(), CellRef { row, col }));
            }
        }
    }
    out
}

/// Collect foreign range and single-cell nodes with their sheet names.
fn collect_foreign_nodes(
    root: &Expr,
    ranges: &mut Vec<(String, CellRef, CellRef)>,
    singles: &mut Vec<(String, CellRef)>,
) {
    match root {
        Expr::SheetCell { sheet, cell } => singles.push((sheet.clone(), *cell)),
        Expr::SheetRange { sheet, start, end } => ranges.push((sheet.clone(), *start, *end)),
        Expr::Unary(inner) => collect_foreign_nodes(inner, ranges, singles),
        Expr::Binary { lhs, rhs, .. } => {
            collect_foreign_nodes(lhs, ranges, singles);
            collect_foreign_nodes(rhs, ranges, singles);
        }
        Expr::Func { args, .. } => {
            for arg in args {
                collect_foreign_nodes(arg, ranges, singles);
            }
        }
        _ => {}
    }
}

/// Replace one foreign range node with a synthetic local range.
fn substitute_range(
    root: &Expr,
    name: &str,
    start: &CellRef,
    end: &CellRef,
    new_start: CellRef,
    new_end: CellRef,
) -> Expr {
    match root {
        Expr::SheetRange {
            sheet,
            start: s,
            end: e,
        } if sheet == name && s == start && e == end => Expr::Range {
            start: new_start,
            end: new_end,
        },
        Expr::Unary(inner) => Expr::Unary(Box::new(substitute_range(
            inner, name, start, end, new_start, new_end,
        ))),
        Expr::Binary { lhs, op, rhs } => Expr::Binary {
            lhs: Box::new(substitute_range(lhs, name, start, end, new_start, new_end)),
            rhs: Box::new(substitute_range(rhs, name, start, end, new_start, new_end)),
            op: *op,
        },
        Expr::Func { name: func, args } => Expr::Func {
            name: func.clone(),
            args: args
                .iter()
                .map(|arg| substitute_range(arg, name, start, end, new_start, new_end))
                .collect(),
        },
        other => other.clone(),
    }
}

/// Replace one foreign cell node with a synthetic local cell.
fn substitute_cell(root: &Expr, name: &str, cell: &CellRef, fresh: CellRef) -> Expr {
    match root {
        Expr::SheetCell { sheet, cell: c } if sheet == name && c == cell => Expr::Cell(fresh),
        Expr::Unary(inner) => Expr::Unary(Box::new(substitute_cell(inner, name, cell, fresh))),
        Expr::Binary { lhs, op, rhs } => Expr::Binary {
            lhs: Box::new(substitute_cell(lhs, name, cell, fresh)),
            rhs: Box::new(substitute_cell(rhs, name, cell, fresh)),
            op: *op,
        },
        Expr::Func { name: func, args } => Expr::Func {
            name: func.clone(),
            args: args
                .iter()
                .map(|arg| substitute_cell(arg, name, cell, fresh))
                .collect(),
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sheet;

    fn workbook() -> Vec<Sheet> {
        let mut data = Sheet::new("Data");
        data.set_str("A1", "10");
        data.set_str("A2", "20");
        data.set_str("B1", "=A1*2");
        let mut report = Sheet::new("Report");
        report.set_str("A1", "=Data!B1+5");
        report.set_str("A2", "=SUM(Data!A1:A2)");
        report.set_str("A3", "='Data'!A1");
        report.set_str("A4", "=Missing!A1");
        report.set_str("A5", "=Data!B1*2");
        vec![data, report]
    }

    #[test]
    fn cross_sheet_values_and_ranges_resolve() {
        let evaluated = evaluate_workbook(&workbook());
        assert_eq!(evaluated.len(), 2);
        let report = &evaluated[1];
        let get = |a1: &str| report.get(&CellRef::parse(a1).unwrap()).unwrap().display();
        assert_eq!(get("A1"), "25");
        assert_eq!(get("A2"), "30");
        assert_eq!(get("A3"), "10");
        assert_eq!(get("A4"), "#REF!");
        assert_eq!(get("A5"), "40");
    }

    #[test]
    fn single_sheet_delegation_matches_evaluate() {
        let mut sheet = Sheet::new("Solo");
        sheet.set_str("A1", "3");
        sheet.set_str("B1", "=A1*3");
        sheet.set_str("C1", "=IF(B1>5,\"big\",\"small\")");
        let direct = crate::evaluate(&sheet);
        let via_workbook = evaluate_workbook(std::slice::from_ref(&sheet));
        assert_eq!(via_workbook.len(), 1);
        assert_eq!(via_workbook[0], direct);
    }

    #[test]
    fn cross_sheet_cycles_become_ref_errors() {
        let mut first = Sheet::new("First");
        first.set_str("A1", "=Second!A1+1");
        let mut second = Sheet::new("Second");
        second.set_str("A1", "=First!A1+1");
        let evaluated = evaluate_workbook(&[first, second]);
        assert_eq!(
            evaluated[0].get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Ref))
        );
        assert_eq!(
            evaluated[1].get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Error(CalcError::Ref))
        );
    }

    #[test]
    fn self_qualified_and_case_insensitive_names_resolve() {
        let mut solo = Sheet::new("Budget");
        solo.set_str("A1", "7");
        solo.set_str("B1", "=BUDGET!A1*2");
        solo.set_str("C1", "='budget'!A1");
        let evaluated = evaluate_workbook(std::slice::from_ref(&solo));
        assert_eq!(
            evaluated[0]
                .get(&CellRef::parse("B1").unwrap())
                .unwrap()
                .display(),
            "14"
        );
        assert_eq!(
            evaluated[0]
                .get(&CellRef::parse("C1").unwrap())
                .unwrap()
                .display(),
            "7"
        );
    }
}
