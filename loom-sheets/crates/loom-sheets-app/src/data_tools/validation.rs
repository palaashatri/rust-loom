//! Cell Data Validation callback registration for Data Tools.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, DataValidationRule, ValidationCriteria};
use slint::{ComponentHandle, SharedString};

use crate::{selection_from_app, DataToolsState, GuiState, SheetsApp};

use super::commit_sheet_metadata_edit;

pub(super) fn register_callbacks(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let dt = app.global::<DataToolsState>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_apply_validation(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let selection = selection_from_app(&app).range();
                let kind = dt.get_val_kind();
                if kind == 0 {
                    commit_sheet_metadata_edit(&app, &state, &menu_service, |sheet| {
                        sheet.clear_validation(selection);
                        true
                    });
                    dt.set_val_feedback("Validation rule cleared".into());
                    dt.set_val_feedback_error(false);
                    return;
                }
                let criteria = match kind {
                    1 => {
                        let items: Vec<String> = dt
                            .get_val_list_or_range()
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if items.is_empty() {
                            dt.set_val_feedback("Enter comma-separated list items".into());
                            dt.set_val_feedback_error(true);
                            return;
                        }
                        ValidationCriteria::List(items)
                    }
                    2 => {
                        let Some(src) =
                            CellRange::parse(dt.get_val_list_or_range().as_str().trim())
                        else {
                            dt.set_val_feedback("Invalid source range (e.g. E1:E5)".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::RangeList(src)
                    }
                    3 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<i64>(),
                            dt.get_val_max().trim().parse::<i64>(),
                        ) else {
                            dt.set_val_feedback("Enter integer Min and Max".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::WholeNumberBetween(min.min(max), min.max(max))
                    }
                    4 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<f64>(),
                            dt.get_val_max().trim().parse::<f64>(),
                        ) else {
                            dt.set_val_feedback("Enter numeric Min and Max".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::DecimalBetween(min.min(max), min.max(max))
                    }
                    5 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<usize>(),
                            dt.get_val_max().trim().parse::<usize>(),
                        ) else {
                            dt.set_val_feedback("Enter non-negative Min and Max length".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::TextLengthBetween(min.min(max), min.max(max))
                    }
                    _ => return,
                };
                let err_msg = {
                    let msg = dt.get_val_error_message().to_string();
                    if msg.trim().is_empty() {
                        "Invalid value".to_string()
                    } else {
                        msg
                    }
                };
                let mut rule = DataValidationRule::new(criteria, err_msg);
                rule.allow_blank = dt.get_val_allow_blank();
                commit_sheet_metadata_edit(&app, &state, &menu_service, |sheet| {
                    sheet.set_validation(selection, rule);
                    true
                });
                dt.set_val_feedback(SharedString::from(format!(
                    "Validation applied to {}",
                    selection.to_a1()
                )));
                dt.set_val_feedback_error(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_clear_validation(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let selection = selection_from_app(&app).range();
                commit_sheet_metadata_edit(&app, &state, &menu_service, |sheet| {
                    sheet.clear_validation(selection);
                    true
                });
                let dt = app.global::<DataToolsState>();
                dt.set_val_feedback("Validation rule cleared".into());
                dt.set_val_feedback_error(false);
            }
        });
    }
}
