use crate::{inspector_tab_index, SheetsApp};
use slint::ComponentHandle;

pub(crate) fn inspector_section_visibility(query: &str) -> (bool, bool) {
    let query = query.trim().to_ascii_lowercase();
    let matches = |labels: &[&str]| {
        query.is_empty() || labels.iter().any(|label| label.contains(query.as_str()))
    };
    (
        matches(&["worksheet", "worksheet name", "name", "rows", "columns"]),
        matches(&[
            "cell",
            "selection",
            "selected range",
            "value",
            "displayed value",
            "formula",
            "raw formula",
            "font style",
            "bold",
            "italic",
            "underline",
            "font size",
            "font size in pixels",
            "pixels",
            "text colour",
            "text color",
            "wrap text",
            "border",
            "borders",
            "borders & fill",
            "fill",
            "data format",
            "general",
            "number",
            "currency",
            "percent",
            "percentage",
            "decimal places",
            "decimals",
            "decimals count",
            "alignment",
            "left",
            "center",
            "right",
            "row height",
            "selected row height",
            "column width",
            "selected column width",
        ]),
    )
}

pub(crate) fn wire(app: &SheetsApp) {
    {
        let app_ref = app.as_weak();
        app.on_inspector_search_edited(move |query| {
            if let Some(app) = app_ref.upgrade() {
                let (table, cell) = inspector_section_visibility(query.as_str());
                app.set_inspector_show_table(table);
                app.set_inspector_show_cell(cell);
                // Search is also a context switch when only one inspector
                // section matches. This keeps a query such as "formula"
                // useful even when the Worksheet tab was previously selected.
                if table && !cell {
                    app.set_inspector_tab(0);
                } else if cell && !table {
                    app.set_inspector_tab(1);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_inspector_context_changed(move |index| {
            if let Some(app) = app_ref.upgrade() {
                // TabStrip owns the visual selection; clamp the mirrored
                // state so keyboard/programmatic activation cannot address a
                // context that has no inspector section.
                app.set_inspector_tab(inspector_tab_index(index));
                let (table, cell) =
                    inspector_section_visibility(app.get_inspector_search().as_str());
                app.set_inspector_show_table(table);
                app.set_inspector_show_cell(cell);
            }
        });
    }
}
