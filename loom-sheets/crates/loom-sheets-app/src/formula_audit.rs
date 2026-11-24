//! Formula Auditing: Show Formulas mode (Cmd/Ctrl+~) + Trace Precedents/Dependents.
#![allow(dead_code)]

use loom_sheets_core::{CellRef, Sheet};

/// State for Formula Auditing overlays and Show Formulas mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditOverlayState {
    /// When `true`, cells display their raw formula text instead of evaluated values.
    pub show_formulas: bool,
    /// Cell currently being traced for precedents/dependents.
    pub traced_cell: Option<CellRef>,
    /// Direct precedent cells referenced by `traced_cell`.
    pub precedents: Vec<CellRef>,
    /// Direct dependent cells that reference `traced_cell`.
    pub dependents: Vec<CellRef>,
}

impl AuditOverlayState {
    /// Toggles Show Formulas mode (`Cmd/Ctrl+~`) and returns the new state.
    pub fn toggle_show_formulas(&mut self) -> bool {
        self.show_formulas = !self.show_formulas;
        self.show_formulas
    }

    /// Traces direct precedents of `cell` by scanning cell references in its formula.
    pub fn trace_precedents(&mut self, sheet: &Sheet, cell: CellRef) {
        self.traced_cell = Some(cell);
        self.dependents.clear();
        let mut preds = Vec::new();
        if let Some(c) = sheet.cells.get(&cell) {
            if c.is_formula() {
                preds = extract_cell_refs(&c.raw);
            }
        }
        preds.sort_by_key(|c| (c.row, c.col));
        preds.dedup();
        self.precedents = preds;
    }

    /// Traces direct dependents of `cell` by finding formulas that reference `cell`.
    pub fn trace_dependents(&mut self, sheet: &Sheet, cell: CellRef) {
        self.traced_cell = Some(cell);
        self.precedents.clear();
        let mut deps = Vec::new();
        for (coord, c) in sheet.cells.iter() {
            if c.is_formula() && extract_cell_refs(&c.raw).contains(&cell) {
                deps.push(*coord);
            }
        }
        deps.sort_by_key(|c| (c.row, c.col));
        deps.dedup();
        self.dependents = deps;
    }

    /// Clears all trace arrows/highlights.
    pub fn clear_traces(&mut self) {
        self.traced_cell = None;
        self.precedents.clear();
        self.dependents.clear();
    }
}

fn extract_cell_refs(formula: &str) -> Vec<CellRef> {
    let mut refs = Vec::new();
    let bytes = formula.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let num_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > num_start {
                if let Some(cr) = CellRef::parse(&formula[start..i]) {
                    refs.push(cr);
                }
            }
        } else {
            i += 1;
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_sheets_core::Cell;

    #[test]
    fn show_formulas_and_trace_precedents_dependents() {
        let mut sheet = Sheet::new("Sheet1");
        let cr = |row, col| CellRef { row, col };
        sheet.cells.insert(cr(0, 0), Cell::new("10"));
        sheet.cells.insert(cr(1, 0), Cell::new("=A1+5"));
        sheet.cells.insert(cr(2, 0), Cell::new("=A2*2"));

        let mut audit = AuditOverlayState::default();
        assert!(!audit.show_formulas);
        assert!(audit.toggle_show_formulas());
        assert!(audit.show_formulas);

        audit.trace_precedents(&sheet, cr(1, 0));
        assert_eq!(audit.precedents, vec![cr(0, 0)]);
        assert!(audit.dependents.is_empty());

        audit.trace_dependents(&sheet, cr(1, 0));
        assert_eq!(audit.dependents, vec![cr(2, 0)]);
        assert!(audit.precedents.is_empty());

        audit.clear_traces();
        assert!(audit.traced_cell.is_none());
    }
}
