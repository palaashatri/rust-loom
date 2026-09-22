//! Keyboard selection and scrolling for the Sheets template chooser.

use slint::{ComponentHandle, Model};

use crate::SheetsApp;

/// Connect the chooser's left/right keys to the currently visible template list.
pub(crate) fn wire_template_navigation(app: &SheetsApp) {
    let app_ref = app.as_weak();
    app.on_navigate_template_selection(move |direction| {
        if let Some(app) = app_ref.upgrade() {
            move_template_selection(&app, direction);
        }
    });
}

fn template_ids_for_category(category: i32, recents: &[i32]) -> Vec<i32> {
    match category {
        0 => vec![0, 3, 4, 5, 6, 7, 1, 8, 9, 10, 2],
        1 => recents.to_vec(),
        2 => vec![0, 3, 4, 5, 6],
        3 => vec![7, 1, 8, 9, 10],
        4 => vec![1, 8, 10],
        5 => vec![2, 4, 5],
        6 => vec![6, 8, 4],
        _ => Vec::new(),
    }
}

fn template_scroll_row(
    category: i32,
    selected: i32,
    position: usize,
    recents: &[i32],
    text_scale: f32,
) -> i32 {
    if category == 0 {
        if recents.contains(&selected) {
            return 0;
        }

        // Keep the selected card below the top edge as text grows. The scroll
        // viewport is shorter than the content at compact sizes, so using the
        // same row estimate at 1.5x can scroll the selected preview offscreen.
        let large_text_adjustment = if text_scale >= 1.5 {
            2
        } else if text_scale >= 1.25 {
            1
        } else {
            0
        };
        return match selected {
            0 | 3 | 4 => 0,
            5 | 6 => 1,
            7 | 1 | 8 => 3 - i32::from(text_scale >= 1.5),
            9 | 10 => 4 - i32::from(text_scale >= 1.5),
            2 => 7 - large_text_adjustment,
            _ => 0,
        };
    }
    if category == 1 {
        return 0;
    }
    (position / 3) as i32
}

fn move_template_selection(app: &SheetsApp, direction: i32) {
    let category = app.get_template_category();
    let recents: Vec<i32> = app.get_template_recents().iter().collect();
    let templates = template_ids_for_category(category, &recents);
    if templates.is_empty() {
        return;
    }

    let selected = app.get_template_selected();
    let position = templates
        .iter()
        .position(|template| *template == selected)
        .unwrap_or(if direction < 0 {
            0
        } else {
            templates.len() - 1
        });
    let step = if direction < 0 { -1 } else { 1 };
    let next = (position as i32 + step).rem_euclid(templates.len() as i32) as usize;
    let selected = templates[next];
    app.set_template_selected(selected);
    app.set_template_scroll_position(template_scroll_row(
        category,
        selected,
        next,
        &recents,
        app.get_template_text_scale(),
    ));
}
