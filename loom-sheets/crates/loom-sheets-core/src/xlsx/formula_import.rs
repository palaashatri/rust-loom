//! Turn the formula text Excel stores in `<f>` into Loom formula text.
//!
//! Excel writes functions it added after 2007 as `_xlfn.NAME(` (and
//! `_xlfn._xlws.NAME(` for worksheet-scoped ones); Loom knows the plain name.
//! A workbook-level defined name that points at a plain cell or range is
//! replaced by that reference so the formula keeps calculating; the name
//! itself is not kept (the import warning says so).

use std::collections::BTreeMap;

use super::cell_refs::parse_cell_ref;

/// Workbook-level names whose target is a plain reference, keyed by the
/// lower-case name.
#[derive(Debug, Default, Clone)]
pub(super) struct DefinedNames {
    targets: BTreeMap<String, String>,
}

fn is_plain_reference(target: &str) -> bool {
    let Some((sheet, cells)) = target.rsplit_once('!') else {
        return false;
    };
    let quoted_sheet = sheet.starts_with('\'') && sheet.ends_with('\'') && sheet.len() > 2;
    let bare_sheet = !sheet.is_empty()
        && sheet
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    if !quoted_sheet && !bare_sheet {
        return false;
    }
    cells
        .split(':')
        .all(|part| parse_cell_ref(&part.replace('$', "")).is_some())
        && cells.split(':').count() <= 2
}

impl DefinedNames {
    /// `entries` are `(name, refersTo, has_sheet_scope)` from `workbook.xml`.
    pub(super) fn from_entries(entries: impl IntoIterator<Item = (String, String, bool)>) -> Self {
        let mut targets = BTreeMap::new();
        for (name, refers_to, sheet_scoped) in entries {
            let target = refers_to.trim().trim_start_matches('=').to_string();
            if sheet_scoped || name.starts_with("_xlnm.") || !is_plain_reference(&target) {
                continue;
            }
            targets.insert(name.to_ascii_lowercase(), target);
        }
        Self { targets }
    }
}

fn is_ident_start(character: char) -> bool {
    character.is_alphabetic() || character == '_' || character == '\\'
}

fn is_ident_part(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '.'
}

/// `excel_body` has no leading `=`; the result has none either.
pub(super) fn loom_formula(excel_body: &str, names: &DefinedNames) -> String {
    let characters: Vec<char> = excel_body.chars().collect();
    let mut out = String::with_capacity(excel_body.len());
    let mut position = 0;
    while position < characters.len() {
        let character = characters[position];
        if character == '"' || character == '\'' {
            // String literal or quoted sheet name; a doubled quote is escaped.
            out.push(character);
            position += 1;
            while position < characters.len() {
                out.push(characters[position]);
                if characters[position] == character {
                    if characters.get(position + 1) == Some(&character) {
                        out.push(character);
                        position += 2;
                        continue;
                    }
                    position += 1;
                    break;
                }
                position += 1;
            }
            continue;
        }
        if is_ident_start(character) {
            let start = position;
            while position < characters.len() && is_ident_part(characters[position]) {
                position += 1;
            }
            let word: String = characters[start..position].iter().collect();
            let previous = out.chars().next_back();
            let next = characters.get(position).copied();
            let stripped = word
                .strip_prefix("_xlfn.")
                .map(|rest| rest.strip_prefix("_xlws.").unwrap_or(rest));
            if let Some(rest) = stripped {
                out.push_str(rest);
                continue;
            }
            let is_function = next == Some('(');
            let is_reference_part =
                matches!(previous, Some('!' | '$' | ':')) || matches!(next, Some('!' | '$' | ':'));
            if !is_function && !is_reference_part {
                if let Some(target) = names.targets.get(&word.to_ascii_lowercase()) {
                    out.push_str(target);
                    continue;
                }
            }
            out.push_str(&word);
            continue;
        }
        out.push(character);
        position += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> DefinedNames {
        DefinedNames::from_entries([
            ("Rate".to_string(), "Styled!$B$1".to_string(), false),
            (
                "Block".to_string(),
                "'My Data'!$A$1:$A$9".to_string(),
                false,
            ),
            ("Local".to_string(), "Styled!$B$2".to_string(), true),
            (
                "Complex".to_string(),
                "OFFSET(Styled!A1,1,1)".to_string(),
                false,
            ),
            (
                "_xlnm.Print_Area".to_string(),
                "Styled!$A$1:$B$2".to_string(),
                false,
            ),
        ])
    }

    #[test]
    fn future_function_prefixes_are_removed_outside_text() {
        let none = DefinedNames::default();
        assert_eq!(
            loom_formula("_xlfn.CONCAT(A1,\"_xlfn.x\")", &none),
            "CONCAT(A1,\"_xlfn.x\")"
        );
        assert_eq!(
            loom_formula("_xlfn._xlws.SORT(_xlfn.UNIQUE(A1:A9))", &none),
            "SORT(UNIQUE(A1:A9))"
        );
        assert_eq!(
            loom_formula("SUM('_xlfn.odd'!A1:A2)", &none),
            "SUM('_xlfn.odd'!A1:A2)"
        );
    }

    #[test]
    fn simple_names_become_references_and_other_names_stay() {
        let names = names();
        assert_eq!(loom_formula("Rate*2", &names), "Styled!$B$1*2");
        assert_eq!(
            loom_formula("SUM(Block)+rate", &names),
            "SUM('My Data'!$A$1:$A$9)+Styled!$B$1"
        );
        // Not a name: a function, a cell address, text, a sheet-scoped name.
        assert_eq!(loom_formula("Rate(1)+A1+$B$1", &names), "Rate(1)+A1+$B$1");
        assert_eq!(
            loom_formula("\"Rate\"&Local&Complex", &names),
            "\"Rate\"&Local&Complex"
        );
        assert_eq!(loom_formula("Sheet!Rate", &names), "Sheet!Rate");
    }
}
