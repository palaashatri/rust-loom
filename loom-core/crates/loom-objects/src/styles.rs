//! Table style gallery and formatting resolver with 12 built-in styles.

use crate::table::{Border, CellBorders, HorizontalAlign, TableCell};
use serde::{Deserialize, Serialize};

/// Style category for grouping in UI galleries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableStyleCategory {
    Light,
    Medium,
    Dark,
}

/// A complete table style definition mapping roles to cell formats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStyle {
    pub id: String,
    pub name: String,
    pub category: TableStyleCategory,
    pub header_row: CellFormat,
    pub total_row: CellFormat,
    pub first_column: CellFormat,
    pub last_column: CellFormat,
    pub odd_row: CellFormat,
    pub even_row: CellFormat,
    pub default_cell: CellFormat,
}

/// Cell formatting properties within a table style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default)]
    pub borders: CellBorders,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_align: Option<HorizontalAlign>,
}

/// Fully resolved cell presentation after applying style hierarchy and overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCellFormat {
    pub background_color: Option<String>,
    pub text_color: Option<String>,
    pub bold: bool,
    pub borders: CellBorders,
    pub horizontal_align: HorizontalAlign,
}

/// Table formatting configuration flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableStyleOptions {
    pub header_row: bool,
    pub total_row: bool,
    pub first_column: bool,
    pub last_column: bool,
    pub banded_rows: bool,
    pub banded_columns: bool,
}

impl Default for TableStyleOptions {
    fn default() -> Self {
        Self {
            header_row: true,
            total_row: false,
            first_column: false,
            last_column: false,
            banded_rows: true,
            banded_columns: false,
        }
    }
}

impl TableStyle {
    /// Resolves final formatting for a cell at (row, col) considering table options and cell overrides.
    pub fn resolve_cell(
        &self,
        row: usize,
        col: usize,
        total_rows: usize,
        total_cols: usize,
        options: &TableStyleOptions,
        cell_override: Option<&TableCell>,
    ) -> ResolvedCellFormat {
        // 1. Determine base role format
        let base_format = if options.header_row && row == 0 {
            &self.header_row
        } else if options.total_row && total_rows > 1 && row == total_rows - 1 {
            &self.total_row
        } else if options.banded_rows {
            if row % 2 == 1 {
                &self.odd_row
            } else {
                &self.even_row
            }
        } else {
            &self.default_cell
        };

        // 2. Start with base values
        let mut bg = base_format.background_color.clone();
        let mut fg = base_format.text_color.clone();
        let mut bold = base_format.bold.unwrap_or(false);
        let mut borders = base_format.borders.clone();
        let mut align = base_format
            .horizontal_align
            .unwrap_or(HorizontalAlign::Left);

        // 3. First column emphasis
        if options.first_column && col == 0 && (!options.header_row || row > 0) {
            if let Some(c_bg) = &self.first_column.background_color {
                bg = Some(c_bg.clone());
            }
            if let Some(c_fg) = &self.first_column.text_color {
                fg = Some(c_fg.clone());
            }
            if let Some(c_bold) = self.first_column.bold {
                bold = c_bold;
            }
            merge_borders(&mut borders, &self.first_column.borders);
        }

        // 4. Last column emphasis
        if options.last_column && col + 1 == total_cols && (!options.header_row || row > 0) {
            if let Some(c_bg) = &self.last_column.background_color {
                bg = Some(c_bg.clone());
            }
            if let Some(c_fg) = &self.last_column.text_color {
                fg = Some(c_fg.clone());
            }
            if let Some(c_bold) = self.last_column.bold {
                bold = c_bold;
            }
            merge_borders(&mut borders, &self.last_column.borders);
        }

        // 5. Apply explicit cell overrides if present
        if let Some(cell) = cell_override {
            if let Some(override_style) = &cell.style_override {
                if let Some(ov_bg) = &override_style.background_color {
                    bg = Some(ov_bg.clone());
                }
                if let Some(ov_align) = override_style.horizontal_align {
                    align = ov_align;
                }
                if let Some(ov_borders) = &override_style.borders {
                    merge_borders(&mut borders, ov_borders);
                }
            }
        }

        ResolvedCellFormat {
            background_color: bg,
            text_color: fg,
            bold,
            borders,
            horizontal_align: align,
        }
    }
}

fn merge_borders(target: &mut CellBorders, source: &CellBorders) {
    if source.top.is_some() {
        target.top = source.top.clone();
    }
    if source.bottom.is_some() {
        target.bottom = source.bottom.clone();
    }
    if source.left.is_some() {
        target.left = source.left.clone();
    }
    if source.right.is_some() {
        target.right = source.right.clone();
    }
}

/// Catalog of standard built-in table styles.
pub struct TableStyleCatalog;

impl TableStyleCatalog {
    /// Returns the complete list of 12 built-in styles.
    pub fn builtin() -> Vec<TableStyle> {
        vec![
            // --- Light 1 to 4 ---
            Self::light_1(),
            Self::light_2(),
            Self::light_3(),
            Self::light_4(),
            // --- Medium 1 to 4 ---
            Self::medium_1(),
            Self::medium_2(),
            Self::medium_3(),
            Self::medium_4(),
            // --- Dark 1 to 4 ---
            Self::dark_1(),
            Self::dark_2(),
            Self::dark_3(),
            Self::dark_4(),
        ]
    }

    /// Looks up a built-in style by ID.
    pub fn get(id: &str) -> Option<TableStyle> {
        Self::builtin().into_iter().find(|s| s.id == id)
    }

    /// Default table style used for new tables.
    pub fn default_style() -> TableStyle {
        Self::medium_1()
    }

    // 1. Light 1: Clean, subtle horizontal lines, no cell fill
    fn light_1() -> TableStyle {
        let border_light = Border::solid(0.75, "#D0D0D0");
        let border_header = Border::solid(1.5, "#404040");
        let border_total = Border::solid(1.5, "#404040");

        TableStyle {
            id: "table-style-light-1".to_string(),
            name: "Table Style Light 1".to_string(),
            category: TableStyleCategory::Light,
            header_row: CellFormat {
                background_color: None,
                text_color: Some("#202020".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(border_header),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: None,
                text_color: Some("#202020".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(border_total),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_light.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_light.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_light),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 2. Light 2: Soft blue tint header and subtle zebra striping
    fn light_2() -> TableStyle {
        let border_grid = Border::solid(0.5, "#D8E2EC");
        TableStyle {
            id: "table-style-light-2".to_string(),
            name: "Table Style Light 2".to_string(),
            category: TableStyleCategory::Light,
            header_row: CellFormat {
                background_color: Some("#EBF2FA".to_string()),
                text_color: Some("#1B365D".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.25, "#1B365D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#EBF2FA".to_string()),
                text_color: Some("#1B365D".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.25, "#1B365D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#F6F9FC".to_string()),
                borders: CellBorders {
                    bottom: Some(border_grid.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#FFFFFF".to_string()),
                borders: CellBorders {
                    bottom: Some(border_grid.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_grid),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 3. Light 3: Neutral gray header, clean grid
    fn light_3() -> TableStyle {
        let border_gray = Border::solid(0.5, "#E0E0E0");
        TableStyle {
            id: "table-style-light-3".to_string(),
            name: "Table Style Light 3".to_string(),
            category: TableStyleCategory::Light,
            header_row: CellFormat {
                background_color: Some("#EEEEEE".to_string()),
                text_color: Some("#333333".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.0, "#B0B0B0")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#EEEEEE".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.0, "#B0B0B0")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_gray.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_gray.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_gray),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 4. Light 4: Crisp blue top and bottom accent rules
    fn light_4() -> TableStyle {
        let accent = Border::solid(2.0, "#2B579A");
        let divider = Border::solid(0.5, "#E8E8E8");
        TableStyle {
            id: "table-style-light-4".to_string(),
            name: "Table Style Light 4".to_string(),
            category: TableStyleCategory::Light,
            header_row: CellFormat {
                background_color: None,
                text_color: Some("#2B579A".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(accent.clone()),
                    bottom: Some(Border::solid(1.0, "#2B579A")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: None,
                text_color: Some("#2B579A".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.0, "#2B579A")),
                    bottom: Some(accent),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(divider.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                borders: CellBorders {
                    bottom: Some(divider.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(divider),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 5. Medium 1: Primary Office Blue with bold white text and zebra rows
    fn medium_1() -> TableStyle {
        let border_blue = Border::solid(0.5, "#C8D7E8");
        TableStyle {
            id: "table-style-medium-1".to_string(),
            name: "Table Style Medium 1".to_string(),
            category: TableStyleCategory::Medium,
            header_row: CellFormat {
                background_color: Some("#2B579A".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#1B365D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#2B579A".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#1B365D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#F2F6FB".to_string()),
                borders: CellBorders {
                    bottom: Some(border_blue.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#FFFFFF".to_string()),
                borders: CellBorders {
                    bottom: Some(border_blue.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_blue),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 6. Medium 2: Steel Charcoal header, cool gray rows
    fn medium_2() -> TableStyle {
        let border_slate = Border::solid(0.5, "#D0D7DE");
        TableStyle {
            id: "table-style-medium-2".to_string(),
            name: "Table Style Medium 2".to_string(),
            category: TableStyleCategory::Medium,
            header_row: CellFormat {
                background_color: Some("#34495E".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#1A252F")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#34495E".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#1A252F")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#F4F6F7".to_string()),
                borders: CellBorders {
                    bottom: Some(border_slate.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#FFFFFF".to_string()),
                borders: CellBorders {
                    bottom: Some(border_slate.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_slate),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 7. Medium 3: Forest Green header, soft mint zebra
    fn medium_3() -> TableStyle {
        let border_green = Border::solid(0.5, "#C8E6C9");
        TableStyle {
            id: "table-style-medium-3".to_string(),
            name: "Table Style Medium 3".to_string(),
            category: TableStyleCategory::Medium,
            header_row: CellFormat {
                background_color: Some("#2E7D32".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#1B5E20")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#2E7D32".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#1B5E20")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#F1F8E9".to_string()),
                borders: CellBorders {
                    bottom: Some(border_green.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#FFFFFF".to_string()),
                borders: CellBorders {
                    bottom: Some(border_green.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_green),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 8. Medium 4: Warm Terracotta header, cream zebra
    fn medium_4() -> TableStyle {
        let border_amber = Border::solid(0.5, "#EDBB99");
        TableStyle {
            id: "table-style-medium-4".to_string(),
            name: "Table Style Medium 4".to_string(),
            category: TableStyleCategory::Medium,
            header_row: CellFormat {
                background_color: Some("#D35400".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#A04000")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#D35400".to_string()),
                text_color: Some("#FFFFFF".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#A04000")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#FDF2E9".to_string()),
                borders: CellBorders {
                    bottom: Some(border_amber.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#FFFFFF".to_string()),
                borders: CellBorders {
                    bottom: Some(border_amber.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                borders: CellBorders {
                    bottom: Some(border_amber),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 9. Dark 1: Midnight Navy with crisp contrasting lines
    fn dark_1() -> TableStyle {
        let border_dark = Border::solid(0.5, "#415A77");
        TableStyle {
            id: "table-style-dark-1".to_string(),
            name: "Table Style Dark 1".to_string(),
            category: TableStyleCategory::Dark,
            header_row: CellFormat {
                background_color: Some("#0D1B2A".to_string()),
                text_color: Some("#E0E1DD".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#778DA9")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#0D1B2A".to_string()),
                text_color: Some("#E0E1DD".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#778DA9")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#1B263B".to_string()),
                text_color: Some("#E0E1DD".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#243048".to_string()),
                text_color: Some("#E0E1DD".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                background_color: Some("#1B263B".to_string()),
                text_color: Some("#E0E1DD".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 10. Dark 2: Deep Charcoal Slate
    fn dark_2() -> TableStyle {
        let border_dark = Border::solid(0.5, "#495057");
        TableStyle {
            id: "table-style-dark-2".to_string(),
            name: "Table Style Dark 2".to_string(),
            category: TableStyleCategory::Dark,
            header_row: CellFormat {
                background_color: Some("#121416".to_string()),
                text_color: Some("#F8F9FA".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#6C757D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#121416".to_string()),
                text_color: Some("#F8F9FA".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#6C757D")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#212529".to_string()),
                text_color: Some("#F8F9FA".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#2C3035".to_string()),
                text_color: Some("#F8F9FA".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                background_color: Some("#212529".to_string()),
                text_color: Some("#F8F9FA".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 11. Dark 3: Deep Teal / Emerald
    fn dark_3() -> TableStyle {
        let border_dark = Border::solid(0.5, "#004D40");
        TableStyle {
            id: "table-style-dark-3".to_string(),
            name: "Table Style Dark 3".to_string(),
            category: TableStyleCategory::Dark,
            header_row: CellFormat {
                background_color: Some("#00251A".to_string()),
                text_color: Some("#E0F2F1".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#80CBC4")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#00251A".to_string()),
                text_color: Some("#E0F2F1".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#80CBC4")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#004D40".to_string()),
                text_color: Some("#E0F2F1".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#00695C".to_string()),
                text_color: Some("#E0F2F1".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                background_color: Some("#004D40".to_string()),
                text_color: Some("#E0F2F1".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    // 12. Dark 4: Deep Plum / Wine
    fn dark_4() -> TableStyle {
        let border_dark = Border::solid(0.5, "#6A1B9A");
        TableStyle {
            id: "table-style-dark-4".to_string(),
            name: "Table Style Dark 4".to_string(),
            category: TableStyleCategory::Dark,
            header_row: CellFormat {
                background_color: Some("#38006B".to_string()),
                text_color: Some("#F3E5F5".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    bottom: Some(Border::solid(1.5, "#BA68C8")),
                    ..Default::default()
                },
                ..Default::default()
            },
            total_row: CellFormat {
                background_color: Some("#38006B".to_string()),
                text_color: Some("#F3E5F5".to_string()),
                bold: Some(true),
                borders: CellBorders {
                    top: Some(Border::solid(1.5, "#BA68C8")),
                    ..Default::default()
                },
                ..Default::default()
            },
            first_column: CellFormat {
                bold: Some(true),
                ..Default::default()
            },
            last_column: CellFormat::default(),
            odd_row: CellFormat {
                background_color: Some("#4A148C".to_string()),
                text_color: Some("#F3E5F5".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            even_row: CellFormat {
                background_color: Some("#6A1B9A".to_string()),
                text_color: Some("#F3E5F5".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            default_cell: CellFormat {
                background_color: Some("#4A148C".to_string()),
                text_color: Some("#F3E5F5".to_string()),
                borders: CellBorders {
                    bottom: Some(border_dark),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_at_least_12_styles() {
        let styles = TableStyleCatalog::builtin();
        assert!(
            styles.len() >= 12,
            "expected at least 12 styles, got {}",
            styles.len()
        );
        assert!(styles
            .iter()
            .any(|s| s.category == TableStyleCategory::Light));
        assert!(styles
            .iter()
            .any(|s| s.category == TableStyleCategory::Medium));
        assert!(styles
            .iter()
            .any(|s| s.category == TableStyleCategory::Dark));
    }

    #[test]
    fn resolve_header_row_applies_header_style() {
        let style = TableStyleCatalog::medium_1();
        let options = TableStyleOptions {
            header_row: true,
            ..Default::default()
        };
        let resolved = style.resolve_cell(0, 0, 3, 3, &options, None);
        assert_eq!(resolved.background_color.as_deref(), Some("#2B579A"));
        assert_eq!(resolved.text_color.as_deref(), Some("#FFFFFF"));
        assert!(resolved.bold);
    }

    #[test]
    fn resolve_banded_rows_alternates_backgrounds() {
        let style = TableStyleCatalog::medium_1();
        let options = TableStyleOptions {
            header_row: true,
            banded_rows: true,
            ..Default::default()
        };
        let r1 = style.resolve_cell(1, 0, 4, 3, &options, None);
        let r2 = style.resolve_cell(2, 0, 4, 3, &options, None);
        assert_ne!(r1.background_color, r2.background_color);
        assert_eq!(r1.background_color.as_deref(), Some("#F2F6FB"));
        assert_eq!(r2.background_color.as_deref(), Some("#FFFFFF"));
    }

    #[test]
    fn cell_overrides_take_precedence() {
        let style = TableStyleCatalog::medium_1();
        let options = TableStyleOptions::default();
        let cell = TableCell {
            style_override: Some(crate::table::CellStyleOverride {
                background_color: Some("#FF0000".to_string()),
                horizontal_align: Some(HorizontalAlign::Right),
                ..Default::default()
            }),
            ..Default::default()
        };

        let resolved = style.resolve_cell(1, 0, 3, 3, &options, Some(&cell));
        assert_eq!(resolved.background_color.as_deref(), Some("#FF0000"));
        assert_eq!(resolved.horizontal_align, HorizontalAlign::Right);
    }
}
