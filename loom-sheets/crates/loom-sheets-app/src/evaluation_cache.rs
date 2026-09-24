use loom_sheets_core::{CellRef, Value};
use std::collections::HashMap;
use std::rc::Rc;

/// Calculated values for the active tab. Navigation can reuse them until an
/// edit tells the app to calculate again.
#[derive(Default)]
pub(crate) struct EvaluationCache {
    active_sheet: Option<usize>,
    values: Option<Rc<HashMap<CellRef, Value>>>,
}

impl EvaluationCache {
    /// Return the last accepted worker values, or an empty projection while a
    /// calculation for this tab is still running. This path never evaluates.
    pub(crate) fn cached_or_empty(&mut self, active_sheet: usize) -> Rc<HashMap<CellRef, Value>> {
        if self.active_sheet == Some(active_sheet) {
            if let Some(values) = &self.values {
                return Rc::clone(values);
            }
        }
        self.set_values(active_sheet, HashMap::new())
    }

    /// Install calculated values returned by the workbook worker.
    pub(crate) fn set_values(
        &mut self,
        active_sheet: usize,
        values: HashMap<CellRef, Value>,
    ) -> Rc<HashMap<CellRef, Value>> {
        let values = Rc::new(values);
        self.active_sheet = Some(active_sheet);
        self.values = Some(Rc::clone(&values));
        values
    }

    /// Return the saved values, calculating once when the cache is empty.
    pub(crate) fn get_or_calculate(
        &mut self,
        active_sheet: usize,
        calculate: impl FnOnce() -> HashMap<CellRef, Value>,
    ) -> Rc<HashMap<CellRef, Value>> {
        if self.active_sheet == Some(active_sheet) {
            if let Some(values) = &self.values {
                return Rc::clone(values);
            }
        }
        self.refresh(active_sheet, calculate)
    }

    /// Replace old values after a workbook edit.
    pub(crate) fn refresh(
        &mut self,
        active_sheet: usize,
        calculate: impl FnOnce() -> HashMap<CellRef, Value>,
    ) -> Rc<HashMap<CellRef, Value>> {
        let values = Rc::new(calculate());
        self.active_sheet = Some(active_sheet);
        self.values = Some(Rc::clone(&values));
        values
    }
}
