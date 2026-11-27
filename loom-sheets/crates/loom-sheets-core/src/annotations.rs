use std::collections::BTreeMap;

use crate::conditional_format::{ColorScaleRule, DataBarRule};
use crate::{CellRange, CellRef};

/// Per-sheet annotations, conditional visual rules, and print settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetAnnotations {
    pub hyperlinks: BTreeMap<CellRef, String>,
    pub notes: BTreeMap<CellRef, String>,
    pub data_bars: Vec<DataBarRule>,
    pub color_scales: Vec<ColorScaleRule>,
    pub print_area: Option<CellRange>,
    pub print_header: String,
    pub print_footer: String,
}
