//! Truthful warnings for XLSX export when Loom model features cannot be
//! represented losslessly in OOXML.

use std::fmt;

use crate::Sheet;

/// Warning emitted when exporting a workbook to XLSX loses or approximates
/// Loom-specific features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxExportWarning {
    pub sheet: Option<String>,
    pub feature: String,
    pub message: String,
}

impl fmt::Display for XlsxExportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.sheet {
            Some(sheet) => write!(f, "[{sheet}] {}: {}", self.feature, self.message),
            None => write!(f, "{}: {}", self.feature, self.message),
        }
    }
}

/// Collect truthful warnings for any sheet features that are approximated or
/// not losslessly representable in OOXML.
pub(super) fn collect_export_warnings(sheets: &[Sheet]) -> Vec<XlsxExportWarning> {
    let mut warnings = Vec::new();
    for sheet in sheets {
        if sheet.tables.len() > 1 {
            warnings.push(XlsxExportWarning {
                sheet: Some(sheet.name.clone()),
                feature: "multi-table canvas".to_string(),
                message: format!(
                    "{} floating tables exported as OOXML tables; freeform canvas geometry is not preserved.",
                    sheet.tables.len()
                ),
            });
        }
        for table in &sheet.tables {
            if table.header_cols > 0 || table.header_rows > 1 || !table.show_title {
                warnings.push(XlsxExportWarning {
                    sheet: Some(sheet.name.clone()),
                    feature: "table layout".to_string(),
                    message: format!(
                        "Table '{}' uses Loom-specific header/title options (header_rows={}, header_cols={}, show_title={}) approximated in OOXML.",
                        table.name, table.header_rows, table.header_cols, table.show_title
                    ),
                });
            }
        }
    }
    warnings
}
