use super::*;
use crate::analysis::{bar_plot_geometry, line_path_commands, pie_wedge_commands};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_sheets_core::ChartKind;
use slint::{platform::Key, SharedString, VecModel};
use std::cell::Cell;
use std::ops::ControlFlow;
use std::rc::Rc;

fn chart_app(kind: &str, categories: &[String], values: &[String]) -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 720));
    app.set_chart_visible(true);
    app.set_chart_title("Quarterly revenue".into());
    app.set_chart_source_range("A1:B4".into());
    app.set_chart_series_label("Amount".into());
    app.set_chart_unit_label("USD/month".into());
    app.set_chart_kind(kind.into());
    app.set_chart_categories(Rc::new(VecModel::from(shared_strings(categories))).into());
    app.set_chart_values_display(Rc::new(VecModel::from(shared_strings(values))).into());
    let numbers: Vec<_> = values
        .iter()
        .map(|value| value.parse::<f64>().unwrap_or(0.0))
        .collect();
    let bar_geometry = bar_plot_geometry(&numbers);
    app.set_chart_bar_baseline_fraction(bar_geometry.baseline);
    let min = numbers.iter().copied().fold(f64::INFINITY, f64::min);
    let max = numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let normalized: Vec<f32> = if kind == "bar" {
        bar_geometry.signed_heights
    } else {
        numbers
            .iter()
            .map(|value| {
                if min == max {
                    1.0
                } else {
                    ((value - min) / (max - min)) as f32
                }
            })
            .collect()
    };
    app.set_chart_normalized(Rc::new(VecModel::from(normalized.clone())).into());
    app.set_chart_line_commands(line_path_commands(&normalized).into());
    app
}

fn shared_strings(values: &[String]) -> Vec<SharedString> {
    values
        .iter()
        .map(|value| SharedString::from(value.as_str()))
        .collect()
}

fn render_chart(app: &SheetsApp) {
    let _ = snapshot_component(app, 1280.0, 720.0, 1.0)
        .expect("render chart and its accessibility tree");
}

fn chart_data_list(app: &SheetsApp) -> ElementHandle {
    chart_data_list_named(app, "Quarterly revenue")
}

fn chart_data_list_named(app: &SheetsApp, title: &str) -> ElementHandle {
    let label = format!("{title} chart data");
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(app, &label).collect();
    assert_eq!(matches.len(), 1, "chart data must appear as one named list");
    let list = matches.into_iter().next().expect("list count checked");
    assert_eq!(list.accessible_role(), Some(AccessibleRole::List));
    list
}

fn chart_data_points(list: &ElementHandle) -> Vec<ElementHandle> {
    let mut points = Vec::new();
    list.visit_descendants(|element| {
        if element.accessible_role() == Some(AccessibleRole::ListItem) {
            points.push(element);
        }
        ControlFlow::<()>::Continue(())
    });
    points
}

fn point_label(app: &SheetsApp) -> String {
    let list = chart_data_list(app);
    let points = chart_data_points(&list);
    assert_eq!(
        points.len(),
        1,
        "the accessible list should expose its current point"
    );
    points[0]
        .accessible_label()
        .expect("selected data point has a category label")
        .to_string()
}

fn press(app: &SheetsApp, key: Key) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.into() });
}

fn press_text(app: &SheetsApp, text: &str) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
}

fn press_control_text(app: &SheetsApp, text: &str) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
    press_text(app, text);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
}

fn sample_categories() -> Vec<String> {
    ["North", "South", "West"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn sample_values() -> Vec<String> {
    ["100", "250", "400"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn chart_accessibility_tree_exposes_summary_and_selected_category_value_for_each_kind() {
    for (kind, spoken_kind) in [
        ("bar", "Bar chart"),
        ("line", "Line chart"),
        ("pie", "Pie chart"),
        ("scatter", "Scatter chart"),
    ] {
        let app = chart_app(kind, &sample_categories(), &sample_values());
        render_chart(&app);

        let list = chart_data_list(&app);
        let summary = list
            .accessible_description()
            .expect("chart data list has a complete summary")
            .to_string();
        for part in [
            spoken_kind,
            "A1:B4",
            "Amount",
            "USD/month",
            "Left and Right",
            "Home and End",
            "Page Up or Page Down",
            "Tab then Page Up",
            "Escape",
        ] {
            assert!(
                summary.contains(part),
                "summary missing {part:?}: {summary:?}"
            );
        }

        let points = chart_data_points(&list);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].accessible_role(), Some(AccessibleRole::ListItem));
        assert_eq!(point_label(&app), "North");
        let point_detail = points[0]
            .accessible_description()
            .expect("selected point describes series, value, and unit")
            .to_string();
        assert!(point_detail.contains("Amount"), "{point_detail:?}");
        assert!(point_detail.contains("100"), "{point_detail:?}");
        assert!(point_detail.contains("USD/month"), "{point_detail:?}");
        assert!(point_detail.contains("Point 1 of 3"), "{point_detail:?}");
    }
}

#[test]
fn f6_enters_chart_data_and_arrows_home_end_and_escape_keep_worksheet_context() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    render_chart(&app);
    let worksheet_moves = Rc::new(Cell::new(0));
    let worksheet_moves_ref = worksheet_moves.clone();
    app.on_navigate_selection(move |_, _| worksheet_moves_ref.set(worksheet_moves_ref.get() + 1));
    let edits = Rc::new(Cell::new(0));
    let edits_ref = edits.clone();
    app.on_begin_edit(move |_| edits_ref.set(edits_ref.get() + 1));
    let clears = Rc::new(Cell::new(0));
    let clears_ref = clears.clone();
    app.on_clear_selected_cells(move || clears_ref.set(clears_ref.get() + 1));
    app.set_selected_col(7);
    app.set_selected_row(9);
    app.invoke_focus_grid();

    press(&app, Key::F6);
    press(&app, Key::RightArrow);
    render_chart(&app);
    assert_eq!(point_label(&app), "South");
    assert!(
        app.get_chart_focused(),
        "F6 moves native focus into chart data"
    );
    assert_eq!(
        worksheet_moves.get(),
        0,
        "chart navigation must not move the worksheet"
    );
    assert_eq!(app.get_selected_col(), 7);
    assert_eq!(app.get_selected_row(), 9);

    press(&app, Key::UpArrow);
    press(&app, Key::Delete);
    press(&app, Key::Backspace);
    press_text(&app, "x");
    assert_eq!(
        worksheet_moves.get(),
        0,
        "unhandled chart keys stay out of the grid"
    );
    assert_eq!(
        clears.get(),
        0,
        "Delete and Backspace must not clear worksheet cells"
    );
    assert_eq!(
        edits.get(),
        0,
        "typing must not begin worksheet cell editing"
    );

    press(&app, Key::Home);
    render_chart(&app);
    assert_eq!(point_label(&app), "North");
    press(&app, Key::LeftArrow);
    render_chart(&app);
    assert_eq!(
        point_label(&app),
        "North",
        "navigation clamps at the first point"
    );

    press(&app, Key::End);
    render_chart(&app);
    assert_eq!(point_label(&app), "West");
    press(&app, Key::RightArrow);
    render_chart(&app);
    assert_eq!(
        point_label(&app),
        "West",
        "navigation clamps at the last point"
    );

    press(&app, Key::Escape);
    press(&app, Key::RightArrow);
    render_chart(&app);
    assert!(
        app.get_chart_visible(),
        "leaving data review keeps the chart open"
    );
    assert_eq!(
        point_label(&app),
        "West",
        "Escape returns keyboard focus to the worksheet"
    );
    assert!(!app.get_chart_focused(), "Escape clears chart data focus");
    assert_eq!(
        worksheet_moves.get(),
        1,
        "worksheet arrows work again after Escape"
    );
}

#[test]
fn f6_does_not_interrupt_an_active_formula_edit() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    render_chart(&app);
    app.set_formula_edit_buffer("=unsaved draft".into());
    app.set_is_editing(true);
    app.invoke_focus_grid();

    press(&app, Key::F6);

    assert!(!app.get_chart_focused());
    assert!(app.get_is_editing());
    assert_eq!(app.get_formula_edit_buffer().as_str(), "=unsaved draft");
}

#[test]
fn f6_after_cancel_does_not_steal_a_new_direct_formula_draft() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    render_chart(&app);
    app.invoke_focus_formula_bar();
    press(&app, Key::Escape);
    assert!(!app.get_is_editing(), "Escape cancels the prior edit");

    let field = ElementHandle::find_by_accessible_label(&app, "Selected cell formula or value")
        .next()
        .expect("formula text field is available");
    let position = field.absolute_position();
    let size = field.size();
    let click = slint::LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: click,
            button: slint::platform::PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: click,
            button: slint::platform::PointerEventButton::Left,
        });
    press_text(&app, "x");
    assert_eq!(app.get_formula_edit_buffer().as_str(), "x");

    press(&app, Key::F6);

    assert!(
        !app.get_chart_focused(),
        "F6 must leave the edited field focused"
    );
    assert_eq!(app.get_formula_edit_buffer().as_str(), "x");
}

#[test]
fn chart_data_focus_blocks_worksheet_edit_shortcuts_but_keeps_save_and_palette() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    render_chart(&app);
    app.invoke_focus_grid();
    press(&app, Key::F6);

    let edits = Rc::new(Cell::new(0));
    let edit_count = edits.clone();
    app.on_cut_selection(move || edit_count.set(edit_count.get() + 1));
    let edit_count = edits.clone();
    app.on_paste_selection(move || edit_count.set(edit_count.get() + 1));
    let edit_count = edits.clone();
    app.on_select_all(move || edit_count.set(edit_count.get() + 1));
    let edit_count = edits.clone();
    app.on_toggle_bold(move || edit_count.set(edit_count.get() + 1));
    let edit_count = edits.clone();
    app.on_undo(move || edit_count.set(edit_count.get() + 1));
    let saves = Rc::new(Cell::new(0));
    let save_count = saves.clone();
    app.on_save_sheet(move || save_count.set(save_count.get() + 1));

    for shortcut in ["x", "v", "a", "b", "z"] {
        press_control_text(&app, shortcut);
    }
    assert_eq!(edits.get(), 0, "worksheet editing shortcuts are isolated");

    press_control_text(&app, "s");
    assert_eq!(
        saves.get(),
        1,
        "save remains available while reviewing data"
    );
    press_control_text(&app, "k");
    assert!(
        app.get_palette_open(),
        "the command palette remains available"
    );
}

#[test]
fn empty_long_and_changing_chart_data_stay_bounded_and_truthful() {
    let app = chart_app("line", &[], &[]);
    render_chart(&app);
    let list = chart_data_list(&app);
    let summary = list
        .accessible_description()
        .expect("empty chart still exposes a summary")
        .to_string();
    assert!(summary.contains("No chart data points"), "{summary:?}");
    assert!(chart_data_points(&list).is_empty());
    assert!(list
        .accessible_label()
        .expect("empty list remains named")
        .contains("Quarterly revenue"));

    let long_categories: Vec<_> = (0..256)
        .map(|index| format!("Category {index:03}"))
        .collect();
    let long_values: Vec<_> = (0..256).map(|index| (index * 10).to_string()).collect();
    app.set_chart_categories(Rc::new(VecModel::from(shared_strings(&long_categories))).into());
    app.set_chart_values_display(Rc::new(VecModel::from(shared_strings(&long_values))).into());
    render_chart(&app);
    app.invoke_focus_grid();
    press(&app, Key::F6);
    assert!(app.get_chart_focused());
    press(&app, Key::End);
    render_chart(&app);
    assert_eq!(point_label(&app), "Category 255");
    assert_eq!(chart_data_points(&chart_data_list(&app)).len(), 1);

    let shorter_categories = vec!["Only".to_owned(), "Remaining".to_owned()];
    let shorter_values = vec!["7".to_owned(), "9".to_owned()];
    app.set_chart_categories(Rc::new(VecModel::from(shared_strings(&shorter_categories))).into());
    app.set_chart_values_display(Rc::new(VecModel::from(shared_strings(&shorter_values))).into());
    render_chart(&app);
    assert_eq!(
        point_label(&app),
        "Remaining",
        "current point clamps to the new last point"
    );
    let point = chart_data_points(&chart_data_list(&app)).remove(0);
    let detail = point
        .accessible_description()
        .expect("changed values update the accessible point")
        .to_string();
    assert!(detail.contains("9"), "{detail:?}");
    assert!(detail.contains("Point 2 of 2"), "{detail:?}");

    let long_label = "Long category ".repeat(18);
    app.set_chart_categories(
        Rc::new(VecModel::from(vec![SharedString::from(
            long_label.as_str(),
        )]))
        .into(),
    );
    app.set_chart_values_display(Rc::new(VecModel::from(vec![SharedString::from("11")])).into());
    app.set_template_text_scale(1.5);
    render_chart(&app);
    assert_eq!(
        point_label(&app),
        long_label,
        "long accessible names are not truncated"
    );
}

#[test]
fn very_long_chart_category_keeps_exit_help_visible_and_scrolls() {
    let long_label = "Long category ".repeat(18);
    let app = chart_app("bar", std::slice::from_ref(&long_label), &["11".to_owned()]);
    app.window().set_size(PhysicalSize::new(1018, 728));
    app.set_template_text_scale(1.5);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
        .expect("render the scaled chart with a long category");

    let summary = chart_data_list(&app);
    let summary_position = summary.absolute_position();
    let summary_size = summary.size();
    let summary_bottom = summary_position.y + summary_size.height;
    let id = "SheetChart::point-help-text";
    let item = ElementHandle::find_by_element_id(&app, id)
        .next()
        .unwrap_or_else(|| panic!("missing visible summary item {id}"));
    let position = item.absolute_position();
    let size = item.size();
    let bottom = position.y + size.height;
    assert!(
        position.y >= summary_position.y && bottom <= summary_bottom,
        "{id} at y={}..{bottom} must remain in the visible summary y={}..{summary_bottom}",
        position.y,
        summary_position.y,
    );

    let category_text = ElementHandle::find_by_element_id(&app, "SheetChart::category-text")
        .next()
        .expect("long category text remains rendered inside its scroll viewport");
    let initial_y = category_text.absolute_position().y;
    app.invoke_focus_grid();
    press(&app, Key::F6);
    assert!(app.get_chart_focused());
    press(&app, Key::Tab);
    press(&app, Key::PageDown);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
        .expect("render after scrolling the long category");
    let scrolled_y = category_text.absolute_position().y;
    assert!(
        scrolled_y < initial_y,
        "Page Down scrolls the category text from y={initial_y} to y={scrolled_y}"
    );
    press(&app, Key::PageUp);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
        .expect("render after scrolling the category back up");
    assert_eq!(category_text.absolute_position().y, initial_y);

    press(&app, Key::Backtab);
    assert!(
        app.get_chart_focused(),
        "Backtab keeps focus within chart data"
    );
    press(&app, Key::Tab);
    assert!(app.get_chart_focused(), "Tab keeps focus within chart data");
}

#[test]
fn very_long_series_value_and_unit_have_a_keyboard_reading_path() {
    let long_series = "Quarterly regional revenue ".repeat(8);
    let long_value = "1234567890".repeat(8);
    let long_unit = "million dollars per quarter ".repeat(7);
    let app = chart_app("bar", &["Q1".to_owned()], std::slice::from_ref(&long_value));
    app.window().set_size(PhysicalSize::new(900, 680));
    app.set_chart_series_label(long_series.clone().into());
    app.set_chart_unit_label(long_unit.clone().into());
    app.set_template_text_scale(2.0);
    let _ = snapshot_component(&app, 900.0, 680.0, 1.0)
        .expect("render the two-times chart with a long selected value");

    let data_list = chart_data_list(&app);
    let point = chart_data_points(&data_list).remove(0);
    let detail = point
        .accessible_description()
        .expect("selected point exposes its complete value metadata");
    for complete_value in [&long_series, &long_value, &long_unit] {
        assert!(
            detail.contains(complete_value),
            "missing full text in {detail:?}"
        );
    }
    let workspace = ElementHandle::find_by_element_id(&app, "SheetsApp::workspace")
        .next()
        .expect("the app exposes its workspace bounds");
    let help = ElementHandle::find_by_element_id(&app, "SheetChart::point-help-text")
        .next()
        .expect("point navigation and exit help remains visible");
    let help_y = help.absolute_position().y;
    assert!(
        help_y + help.size().height <= workspace.absolute_position().y + workspace.size().height,
        "chart navigation help remains inside the workspace"
    );
    assert!(
        help_y + help.size().height <= data_list.absolute_position().y + data_list.size().height,
        "chart navigation help remains inside the chart data panel"
    );
    let point_y = point.absolute_position().y;
    assert!(
        help_y >= point_y && help_y + help.size().height <= point_y + point.size().height,
        "chart navigation help remains inside the focused point card"
    );
    assert!(
        data_list.absolute_position().y + data_list.size().height
            <= workspace.absolute_position().y + workspace.size().height,
        "the scaled chart data panel remains inside the workspace"
    );
    let value_text = ElementHandle::find_by_element_id(&app, "SheetChart::point-value-text")
        .next()
        .expect("selected value remains rendered inside the summary viewport");
    let initial_y = value_text.absolute_position().y;

    app.invoke_focus_grid();
    press(&app, Key::F6);
    assert!(app.get_chart_focused());
    press(&app, Key::Tab);
    press(&app, Key::PageDown);
    let scrolled_y = value_text.absolute_position().y;
    assert!(
        scrolled_y < initial_y,
        "Page Down must scroll the long value into view from y={initial_y} to y={scrolled_y}"
    );
}

#[test]
fn chart_summary_stays_inside_a_short_window_at_large_text_scale() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    app.window().set_size(PhysicalSize::new(1018, 728));
    app.set_template_text_scale(1.5);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
        .expect("render the scaled chart in a short window");

    let summary = chart_data_list(&app);
    let position = summary.absolute_position();
    let size = summary.size();
    let bottom = position.y + size.height;
    let workspace = ElementHandle::find_by_element_id(&app, "SheetsApp::workspace")
        .next()
        .expect("the app exposes its workspace bounds");
    let workspace_position = workspace.absolute_position();
    let workspace_bottom = workspace_position.y + workspace.size().height;
    assert!(
        bottom <= workspace_bottom,
        "chart summary ends at y={bottom}, beyond the workspace ending at y={workspace_bottom}"
    );
}

#[test]
fn normal_scale_chart_point_value_fits_inside_its_clipped_viewport() {
    let app = chart_app("bar", &sample_categories(), &sample_values());
    app.window().set_size(PhysicalSize::new(1018, 728));
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
        .expect("render the chart at the native window size");

    let value = ElementHandle::find_by_element_id(&app, "SheetChart::point-value-text")
        .next()
        .expect("selected point exposes its value line");
    let viewport = ElementHandle::find_by_element_id(&app, "SheetChart::category-viewport")
        .next()
        .expect("selected point details have a clipped viewport");
    let value_position = value.absolute_position();
    let value_size = value.size();
    let viewport_position = viewport.absolute_position();
    let viewport_size = viewport.size();
    let value_bottom = value_position.y + value_size.height;
    let viewport_bottom = viewport_position.y + viewport_size.height;

    assert!(
        value_position.y >= viewport_position.y && value_bottom + 4.0 <= viewport_bottom,
        "normal point value {value_position:?} + {value_size:?} must fit with four pixels of slack inside the visible details viewport {viewport_position:?} + {viewport_size:?}"
    );
}

#[test]
fn long_chart_categories_do_not_expand_bar_columns() {
    let categories = vec![
        "Long category ".repeat(18),
        "South".to_owned(),
        "West".to_owned(),
        "East".to_owned(),
        "North".to_owned(),
    ];
    let values = vec!["100", "250", "400", "175", "325"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let app = chart_app("bar", &categories, &values);
    render_chart(&app);

    let columns: Vec<_> =
        ElementHandle::find_by_element_id(&app, "SheetChart::bar-column").collect();
    assert_eq!(
        columns.len(),
        categories.len(),
        "one equal slot per category"
    );
    let widths: Vec<_> = columns.iter().map(|column| column.size().width).collect();
    let smallest = widths.iter().copied().fold(f32::INFINITY, f32::min);
    let largest = widths.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        largest - smallest <= 1.0,
        "long category text must be elided within equal-width columns, got {widths:?}"
    );
}

#[test]
fn bars_share_the_zero_baseline_and_scale_from_zero() {
    let categories = ["One", "Two", "Three", "Four"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let values = ["100", "450", "1200", "150"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let app = chart_app("bar", &categories, &values);
    render_chart(&app);

    let bars: Vec<_> = ElementHandle::find_by_element_id(&app, "SheetChart::bar-shape").collect();
    assert_eq!(bars.len(), values.len());
    let baselines: Vec<_> = bars
        .iter()
        .map(|bar| bar.absolute_position().y + bar.size().height)
        .collect();
    let minimum = baselines.iter().copied().fold(f32::INFINITY, f32::min);
    let maximum = baselines.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        maximum - minimum <= 1.0,
        "bar baselines differ: {baselines:?}"
    );
    assert!(
        (bars[0].size().height / bars[2].size().height - (100.0 / 1200.0)).abs() < 0.02,
        "bars should scale from zero: heights={:?}",
        bars.iter().map(|bar| bar.size().height).collect::<Vec<_>>()
    );

    let mixed_categories = vec!["Negative".to_owned(), "Positive".to_owned()];
    let mixed_values = vec!["-100".to_owned(), "100".to_owned()];
    let mixed = chart_app("bar", &mixed_categories, &mixed_values);
    render_chart(&mixed);
    let mixed_bars: Vec<_> =
        ElementHandle::find_by_element_id(&mixed, "SheetChart::bar-shape").collect();
    assert_eq!(mixed_bars.len(), 2);
    let negative = mixed_bars[0].absolute_position();
    let positive = mixed_bars[1].absolute_position();
    let positive_size = mixed_bars[1].size();
    assert!(
        (negative.y - (positive.y + positive_size.height)).abs() <= 1.0,
        "positive and negative bars must meet at the same zero baseline: negative={negative:?}, positive={positive:?}, positive_size={positive_size:?}"
    );
}

#[test]
fn bars_keep_finite_extreme_values_visible_without_overflowing_the_range() {
    let geometry = bar_plot_geometry(&[-1.0e308, 1.0e308]);
    assert!((geometry.baseline - 0.5).abs() < 1.0e-6);
    assert!((geometry.signed_heights[0] + 0.5).abs() < 1.0e-6);
    assert!((geometry.signed_heights[1] - 0.5).abs() < 1.0e-6);
}

#[test]
fn line_markers_share_the_path_viewbox_geometry() {
    let app = chart_app("line", &sample_categories(), &sample_values());
    render_chart(&app);
    let plot = ElementHandle::find_by_element_id(&app, "SheetChart::line-plot")
        .next()
        .expect("line plot has measurable bounds");
    let plot_position = plot.absolute_position();
    let plot_size = plot.size();
    let markers: Vec<_> =
        ElementHandle::find_by_element_id(&app, "SheetChart::line-point-marker").collect();
    assert_eq!(markers.len(), sample_values().len());
    for (index, marker) in markers.iter().enumerate() {
        let marker_position = marker.absolute_position();
        let marker_size = marker.size();
        let nx = index as f32 / (markers.len() - 1) as f32;
        let ny = [0.0_f32, 0.5, 1.0][index];
        let expected_x = plot_position.x + 6.0 + (plot_size.width - 12.0) * nx;
        let expected_y =
            plot_position.y + 6.0 + (plot_size.height - 12.0) * ((58.0 - 54.0 * ny) / 60.0);
        assert!(
            (marker_position.x + marker_size.width / 2.0 - expected_x).abs() <= 1.0,
            "marker {index} x does not match path: {marker_position:?} {marker_size:?} expected={expected_x}"
        );
        assert!(
            (marker_position.y + marker_size.height / 2.0 - expected_y).abs() <= 1.0,
            "marker {index} y does not match path: {marker_position:?} {marker_size:?} expected={expected_y}"
        );
    }
}

#[test]
fn scatter_markers_use_numeric_x_coordinates() {
    let categories = vec!["1".to_owned(), "2".to_owned(), "100".to_owned()];
    let values = vec!["10".to_owned(), "20".to_owned(), "30".to_owned()];
    let app = chart_app("scatter", &categories, &values);
    app.set_chart_normalized(
        Rc::new(VecModel::from(vec![0.0, 0.0, 0.5, 1.0 / 99.0, 1.0, 1.0])).into(),
    );
    render_chart(&app);

    let plot = ElementHandle::find_by_element_id(&app, "SheetChart::line-plot")
        .next()
        .expect("scatter plot has measurable bounds");
    let plot_position = plot.absolute_position();
    let markers: Vec<_> =
        ElementHandle::find_by_element_id(&app, "SheetChart::line-point-marker").collect();
    assert_eq!(markers.len(), 3);
    let center_x =
        |marker: &ElementHandle| marker.absolute_position().x + marker.size().width / 2.0;
    assert!(
        center_x(&markers[1]) - center_x(&markers[0]) < 10.0,
        "X=2 must be near X=1, not halfway across the plot"
    );
    assert!(
        center_x(&markers[2]) > plot_position.x + plot.size().width - 20.0,
        "X=100 must be at the right edge of the plot"
    );
    let point = chart_data_points(&chart_data_list(&app)).remove(0);
    assert!(point
        .accessible_description()
        .expect("scatter point is fully described")
        .contains("X value: 1"));
}

#[test]
fn production_scatter_projection_uses_numeric_x_values_and_discards_invalid_pairs() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 720));
    let mut sheet = Sheet::new("Scatter");
    sheet.set_str("A1", "Elapsed seconds");
    sheet.set_str("B1", "Response (ms)");
    sheet.set_str("A2", "1");
    sheet.set_str("B2", "10");
    sheet.set_str("A3", "2");
    sheet.set_str("B3", "20");
    sheet.set_str("A4", "not numeric");
    sheet.set_str("B4", "25");
    sheet.set_str("A5", "1,2");
    sheet.set_str("B5", "26");
    sheet.set_str("A6", "100");
    sheet.set_str("B6", "30");
    sheet.set_str("A7", "1,234");
    sheet.set_str("B7", "33");
    sheet.set_str("A8", "1.2,3");
    sheet.set_str("B8", "34");
    sheet.set_str("A9", "1e1,2");
    sheet.set_str("B9", "35");
    sheet.chart = Some(loom_sheets_core::SheetChart {
        kind: ChartKind::Scatter,
        title: "Latency samples".into(),
        cat_col: 0,
        val_col: 1,
        start_row: 1,
        end_row: Some(8),
    });
    app.set_chart_visible(true);
    sync_chart_to_app(&app, &sheet);

    assert_eq!(
        app.get_chart_categories().row_count(),
        4,
        "a point without numeric X must be discarded with its Y value"
    );
    render_chart(&app);
    let plot = ElementHandle::find_by_element_id(&app, "SheetChart::line-plot")
        .next()
        .expect("scatter plot has measurable bounds");
    let markers: Vec<_> =
        ElementHandle::find_by_element_id(&app, "SheetChart::line-point-marker").collect();
    assert_eq!(markers.len(), 4);
    let center_x =
        |marker: &ElementHandle| marker.absolute_position().x + marker.size().width / 2.0;
    assert!(center_x(&markers[1]) - center_x(&markers[0]) < 10.0);
    assert!(center_x(&markers[3]) > plot.absolute_position().x + plot.size().width - 20.0);
    let point = chart_data_points(&chart_data_list_named(&app, "Latency samples")).remove(0);
    assert_eq!(point.accessible_label().as_deref(), Some("1"));
    assert!(point
        .accessible_description()
        .expect("scatter point announces its x value")
        .contains("X value: 1"));
}

#[test]
fn pie_with_one_positive_value_uses_a_two_arc_full_circle() {
    let wedge = pie_wedge_commands(&[100.0, 0.0]);
    assert_eq!(wedge.len(), 2);
    assert_eq!(wedge[0].matches('A').count(), 2, "full circle: {wedge:?}");
    assert!(wedge[1].is_empty(), "zero-value points have no wedge");
}

#[test]
fn pie_wedges_are_empty_when_values_are_nonpositive() {
    assert!(pie_wedge_commands(&[0.0, -5.0]).is_empty());
}

#[test]
fn pie_omits_nonpositive_categories_when_positive_slices_exist() {
    let wedges = pie_wedge_commands(&[5.0, -3.0, 0.0]);
    assert_eq!(wedges.len(), 3);
    assert!(!wedges[0].is_empty());
    assert!(wedges[1].is_empty());
    assert!(wedges[2].is_empty());
}

#[test]
fn pie_dominant_slice_does_not_collapse_at_rounded_arc_endpoints() {
    let wedges = pie_wedge_commands(&[10_000.0, 1.0]);
    assert_eq!(
        wedges[0].matches('A').count(),
        2,
        "dominant wedge: {wedges:?}"
    );
}

#[test]
fn pie_scales_large_finite_values_before_summing() {
    let wedges = pie_wedge_commands(&[1.0e308, 1.0e308]);
    assert_eq!(wedges.len(), 2);
    assert!(wedges.iter().all(|wedge| wedge.matches('A').count() == 1));
}

#[test]
fn pie_does_not_duplicate_a_full_circle_after_ratio_underflow() {
    let wedges = pie_wedge_commands(&[1.0e308, 1.0e-308]);
    assert_eq!(wedges.len(), 2);
    assert_eq!(wedges[0].matches('A').count(), 2);
    assert!(
        wedges[1].is_empty(),
        "a ratio below floating-point/rendering precision cannot become another full circle: {wedges:?}"
    );
}

#[test]
fn chart_summary_explains_nonpositive_pie_values() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("Losses");
    sheet.set_str("A1", "Category");
    sheet.set_str("B1", "Amount (USD)");
    sheet.set_str("A2", "Zero");
    sheet.set_str("B2", "0");
    sheet.set_str("A3", "Loss");
    sheet.set_str("B3", "-5");
    sheet.chart = Some(loom_sheets_core::SheetChart {
        kind: ChartKind::Pie,
        title: "Losses pie".into(),
        cat_col: 0,
        val_col: 1,
        start_row: 1,
        end_row: Some(2),
    });
    app.set_chart_visible(true);
    sync_chart_to_app(&app, &sheet);
    render_chart(&app);

    let summary = chart_data_list_named(&app, "Losses pie")
        .accessible_description()
        .expect("pie summary explains how nonpositive values are rendered")
        .to_ascii_lowercase();
    assert!(summary.contains("no positive values"), "{summary:?}");
    let guidance = ElementHandle::find_by_element_id(&app, "SheetChart::chart-guidance-text")
        .next()
        .expect("visible pie guidance is rendered");
    assert!(guidance
        .accessible_label()
        .expect("pie guidance is exposed to assistive technology")
        .to_ascii_lowercase()
        .contains("no positive values"));

    sheet.set_str("B2", "10");
    sync_chart_to_app(&app, &sheet);
    render_chart(&app);
    let mixed_summary = chart_data_list_named(&app, "Losses pie")
        .accessible_description()
        .expect("pie summary discloses omitted nonpositive values")
        .to_ascii_lowercase();
    assert!(
        mixed_summary.contains("zero and negative values are omitted"),
        "{mixed_summary:?}"
    );
    assert!(guidance
        .accessible_label()
        .expect("mixed pie guidance remains accessible")
        .contains("omitted from pie slices"));
}

#[test]
fn long_series_and_unit_metadata_do_not_push_the_data_panel_out_of_the_workspace() {
    let app = chart_app("bar", &["Q1".to_owned()], &["100".to_owned()]);
    app.window().set_size(PhysicalSize::new(900, 680));
    app.set_chart_series_label("Quarterly regional revenue ".repeat(8).into());
    app.set_chart_unit_label("million dollars per customer per quarter ".repeat(7).into());
    app.set_template_text_scale(2.0);
    let _ = snapshot_component(&app, 900.0, 680.0, 1.0)
        .expect("render long chart metadata in a short scaled window");

    let summary = chart_data_list(&app);
    let workspace = ElementHandle::find_by_element_id(&app, "SheetsApp::workspace")
        .next()
        .expect("the app exposes its workspace bounds");
    let summary_bottom = summary.absolute_position().y + summary.size().height;
    let workspace_bottom = workspace.absolute_position().y + workspace.size().height;
    assert!(
        summary_bottom <= workspace_bottom,
        "chart data panel ends at y={summary_bottom}, beyond workspace y={workspace_bottom}"
    );
    let description = summary
        .accessible_description()
        .expect("the full metadata remains available to assistive technology");
    assert!(description.contains("Quarterly regional revenue"));
    assert!(description.contains("million dollars per customer per quarter"));
}

#[test]
fn pie_wedges_remain_inside_their_common_circular_plot_bounds() {
    let categories = ["North", "South", "West", "East", "Central"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let values = ["100", "250", "400", "175", "325"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let bar_app = chart_app("bar", &categories, &values);
    let bar_image = snapshot_component(&bar_app, 1280.0, 720.0, 1.0)
        .expect("render matching bar-chart background");

    let pie_app = chart_app("pie", &categories, &values);
    let wedges = pie_wedge_commands(&[100.0, 250.0, 400.0, 175.0, 325.0])
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>();
    pie_app.set_chart_pie_commands(Rc::new(VecModel::from(wedges)).into());
    pie_app
        .set_chart_pie_opacities(Rc::new(VecModel::from(vec![1.0, 0.85, 0.7, 0.55, 0.4])).into());
    let pie_image = snapshot_component(&pie_app, 1280.0, 720.0, 1.0).expect("render pie chart");
    let plot = ElementHandle::find_by_element_id(&pie_app, "SheetChart::pie-plot")
        .next()
        .expect("pie plot has a measurable square bound");
    let position = plot.absolute_position();
    let size = plot.size();
    assert!(
        size.width > 16.0 && size.height > 16.0,
        "plot bounds: {size:?}"
    );

    let left = position.x as u32 + 8;
    let right = (position.x + size.width) as u32 - 8;
    let top = position.y as u32 + 8;
    for x in [left, right] {
        assert_eq!(
            pie_image.get_pixel(x, top),
            bar_image.get_pixel(x, top),
            "a pie wedge must not spill into the circular plot's outside corner at ({x}, {top})"
        );
    }
}

#[test]
fn production_chart_sync_clears_stale_empty_data_and_announces_chart_hiding() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut populated = Sheet::new("Sales");
    populated.set_str("A1", "Quarter");
    populated.set_str("B1", "Revenue (USD)");
    populated.set_str("A2", "Q1");
    populated.set_str("B2", "15000");
    populated.chart = Some(loom_sheets_core::SheetChart {
        title: "Sales Chart".into(),
        ..Default::default()
    });
    app.set_chart_visible(true);
    sync_chart_to_app(&app, &populated);
    assert_eq!(app.get_chart_categories().row_count(), 1);

    let mut emptied = Sheet::new("Sales");
    emptied.set_str("A1", "Quarter");
    emptied.set_str("B1", "Revenue (USD)");
    emptied.chart = populated.chart.clone();
    app.set_chart_focused(true);
    sync_chart_to_app(&app, &emptied);

    assert!(!app.get_chart_visible(), "empty source data hides the plot");
    assert_eq!(app.get_chart_categories().row_count(), 0);
    assert_eq!(app.get_chart_values_display().row_count(), 0);
    assert_eq!(app.get_chart_normalized().row_count(), 0);
    assert_eq!(app.get_chart_pie_commands().row_count(), 0);
    assert_eq!(app.get_chart_pie_opacities().row_count(), 0);
    assert!(app.get_chart_line_commands().is_empty());
    assert!(app.get_status_left().contains("no plottable data points"));
    assert!(
        !app.get_chart_focused(),
        "hiding the chart restores grid focus"
    );

    app.set_status_left("Save failed: disk full".into());
    render_chart(&app);
    assert_eq!(
        ElementHandle::find_by_accessible_label(&app, "Save failed: disk full").count(),
        1,
        "a later operation status must replace the chart-hiding announcement"
    );

    let mut invalid = populated;
    invalid
        .chart
        .as_mut()
        .expect("chart spec exists")
        .title
        .clear();
    app.set_chart_visible(true);
    app.set_chart_focused(true);
    sync_chart_to_app(&app, &invalid);
    assert!(!app.get_chart_visible());
    assert_eq!(app.get_chart_categories().row_count(), 0);
    assert_eq!(app.get_chart_values_display().row_count(), 0);
    assert!(app.get_status_left().contains("cannot be plotted"));
    assert!(!app.get_chart_focused());
}

#[test]
fn hiding_an_empty_chart_does_not_steal_formula_bar_focus() {
    let app = chart_app("line", &sample_categories(), &sample_values());
    app.set_formula_edit_buffer("draft".into());
    app.set_is_editing(true);
    app.invoke_focus_formula_bar();
    app.set_chart_visible(true);

    let navigations = Rc::new(Cell::new(0));
    let navigations_ref = navigations.clone();
    app.on_navigate_selection(move |_, _| navigations_ref.set(navigations_ref.get() + 1));
    press(&app, Key::RightArrow);
    assert_eq!(
        navigations.get(),
        0,
        "the focused formula editor owns arrow keys"
    );
    app.invoke_focus_formula_bar();

    let mut empty = Sheet::new("Sales");
    empty.set_str("A1", "Quarter");
    empty.set_str("B1", "Revenue (USD)");
    empty.chart = Some(loom_sheets_core::SheetChart {
        title: "Sales Chart".into(),
        ..Default::default()
    });
    sync_chart_to_app(&app, &empty);
    assert!(!app.get_chart_visible());

    press(&app, Key::RightArrow);
    assert_eq!(
        navigations.get(),
        0,
        "automatic chart hiding must leave the formula editor focused instead of routing arrows to the grid"
    );
}

/// UI-41: the chart advertised a `Chart resize handle` corner affordance with no
/// `picked`/`nudged` action behind it, and the grid had an equally inert
/// `Resize table handle`. Both announced themselves as buttons and swallowed
/// arrow and Enter keys while changing nothing.
///
/// A chart resize needs geometry in the core `SheetChart` model, persistence,
/// and the XLSX round trip, which is new feature work and therefore locked
/// during the audit-repair phase. The bounded honest repair is to remove the
/// affordance rather than keep a placebo, and this guard proves no canvas handle
/// in the Sheets UI can be reintroduced unwired.
#[test]
fn every_visible_canvas_handle_in_sheets_wires_its_action() {
    let sources = [
        ("ui/chart.slint", include_str!("../../ui/chart.slint")),
        (
            "ui/components.slint",
            include_str!("../../ui/components.slint"),
        ),
        ("ui/objects.slint", include_str!("../../ui/objects.slint")),
    ];

    let mut handles = 0usize;
    for (name, source) in sources {
        let mut cursor = 0usize;
        while let Some(offset) = source[cursor..].find("LoomCanvasHandle {") {
            let start = cursor + offset;
            // Take the whole brace-balanced instance body.
            let mut depth = 0i32;
            let mut end = source.len() - 1;
            for (index, byte) in source[start..].bytes().enumerate() {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + index;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &source[start..=end];
            handles += 1;
            assert!(
                body.contains("picked =>"),
                "{name} exposes a LoomCanvasHandle with no action:\n{body}"
            );
            cursor = end + 1;
        }
    }
    assert!(
        handles > 0,
        "the guard must actually find a handle, or it proves nothing"
    );
}

#[test]
fn neither_the_chart_nor_the_grid_advertises_an_inert_resize_handle() {
    set_platform();
    let app = chart_app("bar", &["One".to_owned()], &["1".to_owned()]);
    let _ = snapshot_component(&app, 1280.0, 720.0, 1.0).expect("render a chart with a grid");

    for label in ["Chart resize handle", "Resize table handle"] {
        let handles: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert!(
            handles.is_empty(),
            "{label:?} is an enabled affordance with no operation behind it"
        );
    }
}
