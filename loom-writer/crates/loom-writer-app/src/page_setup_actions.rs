use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp, WriterPageGeometry};
use loom_writer_core::{PageSetup, WriterDocument};

const PT_PER_INCH: f32 = 72.0;
const MM_PER_INCH: f32 = 25.4;

pub(crate) fn project(app: &WriterApp, document: &WriterDocument) {
    app.set_page_appearance(document.page.appearance.as_str().into());
    let page = document.page.page_style();
    let millimeters = |points: f32| (points * MM_PER_INCH / PT_PER_INCH * 100.0).round() / 100.0;
    app.set_page_geometry(WriterPageGeometry {
        width_mm: millimeters(page.width_pt),
        height_mm: millimeters(page.height_pt),
        top_mm: millimeters(page.margin_top_pt),
        bottom_mm: millimeters(page.margin_bottom_pt),
        left_mm: millimeters(page.margin_left_pt),
        right_mm: millimeters(page.margin_right_pt),
        margin_top_pt: page.margin_top_pt,
        margin_bottom_pt: page.margin_bottom_pt,
        margin_left_pt: page.margin_left_pt,
        margin_right_pt: page.margin_right_pt,
    });
}

pub(crate) fn register(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_edit_page_geometry(move |field, value| {
        let Some(app) = app_ref.upgrade() else {
            return false;
        };
        let Ok(millimeters) = value.as_str().trim().parse::<f32>() else {
            app.set_status_right(SharedString::from("Enter a valid millimeter value"));
            return false;
        };
        let mut next = state.current.borrow().clone();
        if let Err(error) = apply_measurement(&mut next.page, field.as_str(), millimeters) {
            app.set_status_right(SharedString::from(error));
            return false;
        }
        if let Err(error) = next
            .header_footer
            .validate_for_page_style(&next.page.page_style())
        {
            app.set_status_right(SharedString::from(error));
            return false;
        }
        if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
            return false;
        }
        app.set_status_right(SharedString::from("Custom page geometry updated"));
        true
    });
}

fn apply_measurement(
    page: &mut PageSetup,
    field: &str,
    millimeters: f32,
) -> Result<(), &'static str> {
    if !millimeters.is_finite() {
        return Err("Page geometry must be a finite number");
    }
    let points = millimeters * PT_PER_INCH / MM_PER_INCH;
    let style = page.page_style();
    match field {
        "width" => page.set_custom_dimensions_points(points, style.height_pt),
        "height" => page.set_custom_dimensions_points(style.width_pt, points),
        "top" => page.set_custom_margins_points(
            points,
            style.margin_bottom_pt,
            style.margin_left_pt,
            style.margin_right_pt,
        ),
        "bottom" => page.set_custom_margins_points(
            style.margin_top_pt,
            points,
            style.margin_left_pt,
            style.margin_right_pt,
        ),
        "left" => page.set_custom_margins_points(
            style.margin_top_pt,
            style.margin_bottom_pt,
            points,
            style.margin_right_pt,
        ),
        "right" => page.set_custom_margins_points(
            style.margin_top_pt,
            style.margin_bottom_pt,
            style.margin_left_pt,
            points,
        ),
        _ => Err("Unknown page geometry field"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_writer_core::{PageMarginsPreset, PaperSize};

    #[test]
    fn numeric_page_edits_preserve_other_settings_and_reject_invalid_values() {
        let mut page = PageSetup::default();
        apply_measurement(&mut page, "width", 216.0).expect("valid width");
        assert_eq!(page.paper, PaperSize::Custom);
        assert_eq!(page.margins, PageMarginsPreset::Normal);
        assert_eq!(page.page_style().margin_top_pt, 72.0);

        apply_measurement(&mut page, "left", 12.7).expect("valid custom margin");
        assert_eq!(page.margins, PageMarginsPreset::Custom);
        let before = page;
        assert!(apply_measurement(&mut page, "width", 20.0).is_err());
        assert_eq!(page, before);
        assert!(apply_measurement(&mut page, "unknown", 20.0).is_err());
    }
}
