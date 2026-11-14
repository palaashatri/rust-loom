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
        0 => vec![
            0, 3, 4, 5, 6, 7, 1, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 2, 19, 20, 21, 22, 23,
            24, 25, 26,
        ],
        1 => recents.to_vec(),
        2 => vec![0, 3, 4, 5, 6],
        3 => vec![7, 1, 8, 9, 10, 11, 12, 13, 14],
        4 => vec![15, 16, 17, 18],
        5 => vec![2, 19, 20, 21, 22],
        6 => vec![23, 24, 25, 26],
        _ => Vec::new(),
    }
}

fn business_scroll_row(row: usize, recents: &[i32], text_scale: f32) -> i32 {
    let scale = text_scale.max(1.0);
    let card_height = (110.0 + 44.0 * scale).max(160.0);
    let heading_height = 24.0 * scale;
    let body_height = 40.0 * scale;
    let inner_gap = 16.0;
    let section_gap = 24.0;
    let recent_height = heading_height
        + inner_gap
        + if recents.is_empty() {
            body_height
        } else {
            card_height
        };
    let two_row_section = heading_height + 2.0 * inner_gap + 2.0 * card_height;
    let three_row_section = heading_height + 3.0 * inner_gap + 3.0 * card_height;
    let business_section_y = recent_height
        + section_gap
        + two_row_section
        + section_gap
        + three_row_section
        + section_gap
        + two_row_section
        + section_gap;
    let row_y = business_section_y
        + heading_height
        + (row as f32 + 1.0) * inner_gap
        + (row as f32) * card_height;
    let row_step = 174.0 * scale;
    let min_pos = ((row_y + card_height) / row_step - 1.0).ceil();
    let max_pos = ((row_y + 400.0) / row_step - 1.0).floor();
    min_pos.min(max_pos).max(0.0) as i32
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
            9..=11 => 4 - i32::from(text_scale >= 1.5),
            12..=14 => 5 - i32::from(text_scale >= 1.5),
            15..=17 => 7 - large_text_adjustment,
            18 => 8 - large_text_adjustment,
            2 | 19 | 20 => business_scroll_row(0, recents, text_scale),
            21 | 22 => business_scroll_row(1, recents, text_scale),
            23..=25 => 13 - large_text_adjustment,
            26 => 14 - large_text_adjustment,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_templates_stay_within_viewport_at_large_text_scale() {
        let scale = 1.5_f32;
        let card_height = (110.0 + 44.0 * scale).max(160.0);
        let heading_height = 24.0 * scale;
        let body_height = 40.0 * scale;
        let inner_gap = 16.0;
        let section_gap = 24.0;
        let two_row = heading_height + 2.0 * inner_gap + 2.0 * card_height;
        let three_row = heading_height + 3.0 * inner_gap + 3.0 * card_height;
        let row_step = 174.0 * scale;

        for recents in [&[][..], &[9][..]] {
            let recent_height = heading_height
                + inner_gap
                + if recents.is_empty() {
                    body_height
                } else {
                    card_height
                };
            let biz_y = recent_height
                + section_gap
                + two_row
                + section_gap
                + three_row
                + section_gap
                + two_row
                + section_gap;
            for (row, id) in [(0, 19), (1, 21)] {
                let card_y = biz_y
                    + heading_height
                    + (row as f32 + 1.0) * inner_gap
                    + row as f32 * card_height;
                let pos = template_scroll_row(0, id, 0, recents, scale) as f32;
                for viewport_h in [400.0_f32, 520.0_f32] {
                    let scroll_top = (pos * row_step - (viewport_h - row_step)).max(0.0);
                    assert!(
                        card_y >= scroll_top,
                        "id={id} recents={recents:?} vh={viewport_h}: card_y={card_y} < scroll_top={scroll_top}"
                    );
                    assert!(
                        card_y + card_height <= scroll_top + viewport_h,
                        "id={id} recents={recents:?} vh={viewport_h}: bottom={} > {}",
                        card_y + card_height,
                        scroll_top + viewport_h
                    );
                }
            }
        }
    }
}
