//! Conditional number sections such as `[>=1000]0.0,"K";0` (spike C). A
//! section may start with one comparison in brackets. Sections are tried in
//! order and the first match is shown; a section without a condition takes the
//! numbers the earlier conditions did not claim. A minus sign is shown only when
//! the chosen section admits a positive number. Colour brackets (`[Red]`) are
//! dropped; other brackets, such as `[$€-407]`, stay for the tokenizer.

/// A comparison written in brackets, e.g. `[>100]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Condition {
    op: Op,
    value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Condition {
    pub(super) fn matches(self, n: f64) -> bool {
        match self.op {
            Op::Lt => n < self.value,
            Op::Le => n <= self.value,
            Op::Gt => n > self.value,
            Op::Ge => n >= self.value,
            Op::Eq => n == self.value,
            Op::Ne => n != self.value,
        }
    }
}

/// The inside of a bracket read as a comparison: `>=1000`, `<0`, `=5`.
fn parse_condition(inner: &str) -> Option<Condition> {
    // Two-character operators first, so `<=` is not read as `<` and `=`.
    const OPERATORS: [(&str, Op); 6] = [
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("<>", Op::Ne),
        ("<", Op::Lt),
        (">", Op::Gt),
        ("=", Op::Eq),
    ];
    let inner = inner.trim();
    let (op, rest) = OPERATORS
        .iter()
        .find_map(|(text, op)| inner.strip_prefix(text).map(|rest| (*op, rest)))?;
    let value: f64 = rest.trim().parse().ok()?;
    value.is_finite().then_some(Condition { op, value })
}

/// Excel's colour names and `Color n` (1-56) brackets.
fn is_colour(inner: &str) -> bool {
    let lower = inner.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "black" | "blue" | "cyan" | "green" | "magenta" | "red" | "white" | "yellow"
    ) || lower
        .strip_prefix("color")
        .is_some_and(|number| number.trim().parse::<u32>().is_ok())
}

/// Peel the leading bracket groups off a section. Returns the condition found
/// (if any) and the rest of the section, with colours removed.
pub(super) fn split_leading(section: &str) -> (Option<Condition>, &str) {
    let mut condition = None;
    let mut rest = section;
    while let Some(after) = rest.strip_prefix('[') {
        let Some(end) = after.find(']') else {
            break;
        };
        let inner = &after[..end];
        if let Some(parsed) = parse_condition(inner) {
            condition = Some(parsed);
        } else if !is_colour(inner) {
            break;
        }
        rest = &after[end + 1..];
    }
    (condition, rest)
}

/// The section that shows a number, and whether a negative number keeps its sign.
#[derive(Debug, PartialEq)]
pub(super) struct Selection {
    pub(super) index: usize,
    pub(super) show_minus: bool,
}

/// The section to show for `n` when sections carry conditions. Only the first
/// three sections (the fourth is for text) are considered. A lone conditional
/// section also shows the numbers it does not select, as its implied else.
/// `None` when no section applies.
pub(super) fn select(conditions: &[Option<Condition>], n: f64) -> Option<Selection> {
    let sections = &conditions[..conditions.len().min(3)];
    let holds = |condition: &Option<Condition>| match condition {
        Some(condition) => condition.matches(n),
        None => true,
    };
    if let Some(index) = sections.iter().position(holds) {
        // The section admits its own condition and none of the numbers an earlier one claimed.
        let mut rules: Vec<(Condition, bool)> = sections[..index]
            .iter()
            .flatten()
            .map(|earlier| (*earlier, false))
            .collect();
        if let Some(own) = sections[index] {
            rules.push((own, true));
        }
        return Some(Selection {
            index,
            show_minus: admits_positive(&rules),
        });
    }
    match sections {
        [Some(only)] => Some(Selection {
            index: 0,
            show_minus: admits_positive(&[(*only, false)]),
        }),
        _ => None,
    }
}

/// Whether some positive number meets every rule: a condition that must hold
/// (`true`) or must fail (`false`). A comparison changes truth only at its own
/// value, so probing each gap between the positive values, each value, and one
/// number past the largest is exact.
fn admits_positive(rules: &[(Condition, bool)]) -> bool {
    let mut edges: Vec<f64> = rules
        .iter()
        .map(|(condition, _)| condition.value)
        .filter(|value| *value > 0.0)
        .collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    let mut probes = Vec::with_capacity(2 * edges.len() + 1);
    let mut below = 0.0;
    for edge in edges {
        probes.push((below + edge) / 2.0);
        probes.push(edge);
        below = edge;
    }
    probes.push(below + 1.0);
    probes.into_iter().any(|x| {
        rules
            .iter()
            .all(|(condition, holds)| condition.matches(x) == *holds)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_brackets_yield_a_condition_and_colours_are_dropped() {
        let (condition, rest) = split_leading("[Red][>=1000]0.0,\"K\"");
        assert_eq!(rest, "0.0,\"K\"");
        assert!(condition.is_some_and(|c| c.matches(1000.0) && !c.matches(999.0)));
        let (none, plain) = split_leading("[$€-407]#,##0.00");
        assert!(none.is_none());
        assert_eq!(plain, "[$€-407]#,##0.00");
    }

    #[test]
    fn first_matching_section_wins_and_plain_sections_catch_the_rest() {
        let conditions = [parse_condition(">100"), parse_condition("<0"), None];
        assert_eq!(select(&conditions, 150.0).map(|s| s.index), Some(0));
        assert_eq!(select(&conditions, -3.0).map(|s| s.index), Some(1));
        assert_eq!(select(&conditions, 50.0).map(|s| s.index), Some(2));
    }

    #[test]
    fn minus_is_dropped_only_when_no_positive_number_can_reach_the_section() {
        let shows_minus = |sections: &[Option<Condition>]| {
            select(sections, -5.0).map(|selection| selection.show_minus)
        };
        assert_eq!(shows_minus(&[parse_condition("<0"), None]), Some(false));
        assert_eq!(shows_minus(&[parse_condition("<10"), None]), Some(true));
        assert_eq!(shows_minus(&[parse_condition(">0"), None]), Some(false));
        assert_eq!(shows_minus(&[parse_condition(">=1000"), None]), Some(true));
        assert_eq!(shows_minus(&[parse_condition("<=-5")]), Some(false));
        assert_eq!(shows_minus(&[parse_condition(">0")]), Some(false));
    }
}
