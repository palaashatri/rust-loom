//! Validated font-size edits shared by Writer's typed field and steppers.

use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::document_formatting::{set_selection_font_size, DocumentSelection};
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

const MIN_FONT_SIZE_PT: f32 = 6.0;
const MAX_FONT_SIZE_PT: f32 = 72.0;

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let app_ref = app.as_weak();
    let apply_state = state.clone();
    app.on_apply_font_size(move |text| {
        let Some(app) = app_ref.upgrade() else {
            return false;
        };
        let Some(size_pt) = parse_font_size_pt(&text) else {
            app.set_status_right("Font size must be 6-72 pt in 0.5 pt steps".into());
            return false;
        };
        apply_selected_font_size(&app, &apply_state, size_pt);
        app.set_status_right(SharedString::from(format!("Font size: {size_pt} pt")));
        true
    });

    let app_ref = app.as_weak();
    let increase_state = state.clone();
    app.on_increase_font_size(move || {
        let Some(app) = app_ref.upgrade() else { return };
        let current = current_font_size_pt(&app);
        apply_selected_font_size(&app, &increase_state, (current + 1.0).min(MAX_FONT_SIZE_PT));
    });

    let app_ref = app.as_weak();
    let decrease_state = state.clone();
    app.on_decrease_font_size(move || {
        let Some(app) = app_ref.upgrade() else { return };
        let current = current_font_size_pt(&app);
        apply_selected_font_size(&app, &decrease_state, (current - 1.0).max(MIN_FONT_SIZE_PT));
    });
}

fn current_font_size_pt(app: &WriterApp) -> f32 {
    app.get_font_size_value()
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|size| size.is_finite() && (MIN_FONT_SIZE_PT..=MAX_FONT_SIZE_PT).contains(size))
        .unwrap_or_else(|| app.get_font_size_pt() as f32)
}

fn parse_font_size_pt(text: &str) -> Option<f32> {
    let text = text.trim();
    let mut parts = text.split('.');
    let whole = parts.next()?;
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    if fraction.is_some_and(|value| value != "0" && value != "5") {
        return None;
    }
    let size_pt = text.parse::<f32>().ok()?;
    if !size_pt.is_finite()
        || !(MIN_FONT_SIZE_PT..=MAX_FONT_SIZE_PT).contains(&size_pt)
        || (size_pt * 2.0 - (size_pt * 2.0).round()).abs() > f32::EPSILON * 8.0
    {
        return None;
    }
    Some(size_pt)
}

fn apply_selected_font_size(app: &WriterApp, state: &GuiState, size_pt: f32) {
    let mut next = state.current.borrow().clone();
    let selection = next.selection();
    set_selection_font_size(
        &mut next,
        DocumentSelection::range(selection.anchor, selection.focus),
        size_pt,
    );
    crate::table_actions::table_format_actions::set_selection_font_size(
        &mut next,
        selection.clone(),
        size_pt,
    );
    next.set_selection(selection);
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
}

#[cfg(test)]
mod tests {
    use super::parse_font_size_pt;

    #[test]
    fn font_size_parser_accepts_bounded_half_points_only() {
        assert_eq!(parse_font_size_pt("6"), Some(6.0));
        assert_eq!(parse_font_size_pt("6.5"), Some(6.5));
        assert_eq!(parse_font_size_pt("72.0"), Some(72.0));
        for invalid in ["5.5", "72.5", "12.25", "", "1e1", "12 pt", "NaN"] {
            assert_eq!(parse_font_size_pt(invalid), None, "{invalid:?}");
        }
    }
}
