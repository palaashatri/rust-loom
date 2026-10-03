//! Page presentation: every page is its own sheet with a gap between sheets,
//! and every consumer of vertical positions (text rows, selection and comment
//! rectangles, the caret, pointer hit-testing, scrolling, find) still agrees
//! with the pagination model across page boundaries.

use super::actions_tests::{test_state, text_document};
use super::*;

fn long_text(paragraphs: usize) -> String {
    (1..=paragraphs)
        .map(|n| {
            format!("Paragraph number {n} lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn multi_page() -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(&long_text(120)), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    // Push the page metrics (count, stack height, gap) to the window.
    let document = state.current.borrow().clone();
    refresh_writer_render_projection(&app, &document, *state.viewport.borrow());
    (app, state)
}

fn unit_viewport(style: &PageStyle) -> PageViewport {
    PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }
}

/// Document-space top and bottom (points, from the top of the first sheet) of
/// the sheet that page `index` is drawn on.
fn sheet_span(style: &PageStyle, index: usize) -> (f32, f32) {
    let top = index as f32 * (style.height_pt + loom_writer_core::PAGE_GAP_PT);
    (top, top + style.height_pt)
}

/// Where the editor draws a projected rectangle, measured from the first
/// sheet's top edge (what the canvas shows at zoom 1 and no scroll).
fn drawn_y(style: &PageStyle, projected_y: f32) -> f32 {
    projected_y + style.margin_top_pt
}

fn caret_page_and_y(doc: &WriterDocument, offset: usize) -> (usize, f32, f32) {
    let style = doc.page.page_style();
    let mut doc = doc.clone();
    doc.set_selection(TextSelection::caret(offset));
    let (_, rects, _) = writer_render_projection(&doc, unit_viewport(&style));
    let caret = rects.iter().find(|r| r.caret).expect("caret rect");
    (caret.page_index as usize, caret.y, caret.height)
}

#[test]
fn the_page_gap_comes_from_the_one_core_constant() {
    let (app, _state) = multi_page();
    assert_eq!(app.get_page_gap_pt(), loom_writer_core::PAGE_GAP_PT);
    assert_eq!(loom_writer_core::PAGE_GAP_PT, 24.0);
}

/// Lengths and starts of the runs of pure paper pixels down one column.
struct Pixels {
    raw: Vec<u8>,
    width: u32,
    height: u32,
}

impl Pixels {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.raw[i],
            self.raw[i + 1],
            self.raw[i + 2],
            self.raw[i + 3],
        ]
    }
}

/// Read the renderer image without naming its (foreign) type.
macro_rules! pixels {
    ($image:expr) => {{
        let image = $image;
        let (width, height) = (image.width(), image.height());
        Pixels {
            raw: image.into_raw(),
            width,
            height,
        }
    }};
}

fn paper_runs(image: &Pixels) -> Vec<(u32, u32)> {
    let paper = |x: u32, y: u32| {
        let p = image.at(x, y);
        p[0] == 255 && p[1] == 255 && p[2] == 255
    };
    // The column through the left margin has the most paper pixels: no text.
    let mut best = (0, 0u32);
    for x in 0..image.width {
        let count = (0..image.height).filter(|&y| paper(x, y)).count() as u32;
        if count > best.1 {
            best = (x, count);
        }
    }
    let x = best.0;
    let mut runs = Vec::new();
    let mut start = None;
    for y in 0..image.height {
        match (paper(x, y), start) {
            (true, None) => start = Some(y),
            (false, Some(s)) => {
                runs.push((s, y - s));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, image.height - s));
    }
    runs
}

fn sheet_runs(runs: &[(u32, u32)], sheet_height: f32) -> Vec<(u32, u32)> {
    runs.iter()
        .copied()
        .filter(|(_, len)| (*len as f32 - sheet_height).abs() <= 4.0)
        .collect()
}

#[test]
fn each_page_is_drawn_as_its_own_sheet_with_a_gap_at_every_zoom() {
    let style = PageStyle::default();
    for (zoom, height, scroll) in [(0.5_f32, 1700.0_f32, 0.0_f32), (1.0, 2000.0, 0.0)] {
        let (app, state) = multi_page();
        app.invoke_page_zoom_changed(zoom);
        app.invoke_page_scroll_changed(0.0, scroll);
        let shot = snapshot_component(&app, 1280.0, height, 1.0).expect("render");
        let _ = std::fs::create_dir_all(std::env::temp_dir().join("loom-writer-pages"));
        let _ = shot.save(std::env::temp_dir().join(format!("loom-writer-pages/zoom-{zoom}.png")));
        let image = pixels!(shot);
        let pages = state
            .current
            .borrow()
            .layout(&style, unit_viewport(&style))
            .unwrap()
            .pages
            .len();
        assert!(pages >= 3, "fixture must span several pages, got {pages}");

        let runs = paper_runs(&image);
        let sheets = sheet_runs(&runs, style.height_pt * zoom);
        assert!(
            sheets.len() >= 2,
            "zoom {zoom}: expected separate sheets of {}px, runs were {runs:?}",
            style.height_pt * zoom
        );
        let pitch = (style.height_pt + loom_writer_core::PAGE_GAP_PT) * zoom;
        for pair in sheets.windows(2) {
            let step = pair[1].0 as f32 - pair[0].0 as f32;
            assert!(
                (step - pitch).abs() <= 3.0,
                "zoom {zoom}: sheets are {step}px apart, the pagination pitch is {pitch}px"
            );
        }
    }
}

#[test]
fn the_gap_between_sheets_is_empty_canvas_with_no_text() {
    let style = PageStyle::default();
    let (app, _state) = multi_page();
    let image = pixels!(snapshot_component(&app, 1280.0, 2000.0, 1.0).expect("render"));
    let runs = paper_runs(&image);
    let sheets = sheet_runs(&runs, style.height_pt);
    assert!(sheets.len() >= 2, "runs: {runs:?}");
    let gap_top = sheets[0].0 + sheets[0].1;
    let gap_bottom = sheets[1].0;
    // The gap holds nothing but canvas colour: every pixel across its full
    // width (outside the sheets' borders and shadow rows) is one flat colour.
    let reference = image.at(2, (gap_top + gap_bottom) / 2);
    for x in 0..image.width {
        assert_eq!(
            image.at(x, (gap_top + gap_bottom) / 2),
            reference,
            "no text or ink in the gap at x={x}"
        );
    }
    // And it is not paper: the gap row away from the label is canvas coloured.
    let x = image.width - 3;
    let mid = image.at(x, (gap_top + gap_bottom) / 2);
    assert!(mid[0] < 255, "the gap is not paper");
}

#[test]
fn text_on_every_page_sits_inside_its_own_sheet() {
    let (_app, state) = multi_page();
    let doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let layout = doc.layout(&style, unit_viewport(&style)).unwrap();
    assert!(layout.pages.len() >= 4);
    let mut last_pages_seen = std::collections::BTreeSet::new();
    let text_len = doc.editor_text().len();
    let mut offset = 0;
    while offset <= text_len {
        if doc.editor_text().is_char_boundary(offset) {
            let (page, y, height) = caret_page_and_y(&doc, offset);
            let (top, bottom) = sheet_span(&style, page);
            let drawn = drawn_y(&style, y);
            assert!(
                drawn >= top + style.margin_top_pt - 0.5
                    && drawn + height <= bottom - style.margin_bottom_pt + 0.5,
                "offset {offset}: caret on page {page} drawn at {drawn}..{} outside sheet {top}..{bottom}",
                drawn + height
            );
            last_pages_seen.insert(page);
        }
        offset += 7;
    }
    assert!(last_pages_seen.len() >= 4, "{last_pages_seen:?}");
}

#[test]
fn a_caret_drawn_on_a_later_page_is_hit_at_the_same_offset() {
    let (_app, state) = multi_page();
    let mut doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let viewport = unit_viewport(&style);
    let layout = doc.layout(&style, viewport).unwrap();
    // First and last line of every page, plus the middle: the boundary lines.
    let mut targets = Vec::new();
    for page in &layout.pages {
        let block_starts: Vec<usize> = {
            let mut cursor = 0;
            doc.blocks
                .iter()
                .map(|b| {
                    let s = cursor;
                    cursor += b.text.as_str().len() + 1;
                    s
                })
                .collect()
        };
        for fragment in [page.fragments.first(), page.fragments.last()]
            .into_iter()
            .flatten()
        {
            let index = doc.blocks.iter().position(|b| b.id == fragment.block_id);
            let base = block_starts[index.unwrap()];
            targets.push((page.index, base + fragment.start + 2));
        }
    }
    assert!(targets.iter().any(|(page, _)| *page >= 2));
    for (page, offset) in targets {
        doc.set_selection(TextSelection::caret(offset));
        let layout = doc.layout(&style, viewport).unwrap();
        let base = layout.page_bounds[0];
        let caret = layout
            .selection_rects
            .iter()
            .find(|r| r.start == r.end)
            .expect("caret");
        assert_eq!(caret.page_index, page, "offset {offset} is on page {page}");
        let x = caret.rect.x - base.x - style.margin_left_pt;
        let y = caret.rect.y - base.y - style.margin_top_pt + caret.rect.height / 2.0;
        assert_eq!(
            writer_pointer_offset(&doc, viewport, x, y),
            Some(offset),
            "a click on page {page} at the caret for {offset} must land there"
        );
    }
}

#[test]
fn a_click_on_page_two_text_through_the_window_places_the_caret_there() {
    let (app, state) = multi_page();
    let doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let layout = doc.layout(&style, unit_viewport(&style)).unwrap();
    let fragment = &layout.pages[1].fragments[0];
    let mut cursor = 0;
    let mut start = 0;
    for block in &doc.blocks {
        if block.id == fragment.block_id {
            start = cursor + fragment.start;
        }
        cursor += block.text.as_str().len() + 1;
    }
    let target = start + 4;
    let (page, y, height) = caret_page_and_y(&doc, target);
    assert_eq!(page, 1);
    let x = {
        let mut probe = doc.clone();
        probe.set_selection(TextSelection::caret(target));
        let rects = writer_render_projection(&probe, unit_viewport(&style)).1;
        rects.iter().find(|r| r.caret).unwrap().x
    };
    app.invoke_pointer_pressed(x, y + height / 2.0, false);
    app.invoke_pointer_released(x, y + height / 2.0);
    assert_eq!(state.current.borrow().selection().focus, target);
}

#[test]
fn a_selection_across_a_page_boundary_has_rectangles_on_both_sheets() {
    let (_app, state) = multi_page();
    let mut doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let layout = doc.layout(&style, unit_viewport(&style)).unwrap();
    let first_on_two = &layout.pages[1].fragments[0];
    let mut cursor = 0;
    let mut start_two = 0;
    for block in &doc.blocks {
        if block.id == first_on_two.block_id {
            start_two = cursor + first_on_two.start;
        }
        cursor += block.text.as_str().len() + 1;
    }
    doc.set_selection(TextSelection::range(start_two - 30, start_two + 30));
    let (_, rects, _) = writer_render_projection(&doc, unit_viewport(&style));
    let selected: Vec<_> = rects.iter().filter(|r| !r.caret).collect();
    let pages: std::collections::BTreeSet<i32> = selected.iter().map(|r| r.page_index).collect();
    assert!(pages.contains(&0) && pages.contains(&1), "{pages:?}");
    for rect in selected {
        let (top, bottom) = sheet_span(&style, rect.page_index as usize);
        let drawn = drawn_y(&style, rect.y);
        assert!(
            drawn >= top && drawn + rect.height <= bottom,
            "a rectangle for page {} drawn at {drawn} is off its sheet {top}..{bottom}",
            rect.page_index
        );
    }
}

#[test]
fn comment_highlights_on_a_later_page_sit_on_that_sheet() {
    let (app, state) = multi_page();
    let target = {
        let doc = state.current.borrow();
        let style = doc.page.page_style();
        let layout = doc.layout(&style, unit_viewport(&style)).unwrap();
        let fragment = &layout.pages[2].fragments[0];
        let mut cursor = 0;
        let mut start = 0;
        for block in &doc.blocks {
            if block.id == fragment.block_id {
                start = cursor + fragment.start;
            }
            cursor += block.text.as_str().len() + 1;
        }
        start
    };
    app.invoke_selection_changed(target as i32, (target + 9) as i32);
    app.invoke_add_comment(SharedString::from("On page three"));
    let doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let (_, _, comments) = writer_render_projection(&doc, *state.viewport.borrow());
    assert!(!comments.is_empty());
    for rect in comments {
        assert_eq!(rect.page_index, 2);
        let (top, bottom) = sheet_span(&style, 2);
        let drawn = drawn_y(&style, rect.y);
        assert!(
            drawn >= top && drawn + rect.height <= bottom,
            "comment drawn at {drawn} off sheet {top}..{bottom}"
        );
    }
}

#[test]
fn wheel_scrolling_moves_the_visible_pages_across_boundaries() {
    let (app, state) = multi_page();
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let style = state.current.borrow().page.page_style();
    let visible_pages = |state: &GuiState| {
        let viewport = *state.viewport.borrow();
        let layout = state
            .current
            .borrow()
            .layout(
                &style,
                PageViewport {
                    width: style.width_pt,
                    height: app.get_page_view_height().max(300.0),
                    ..viewport
                },
            )
            .unwrap();
        layout
            .visible_ranges
            .iter()
            .map(|r| r.page_index)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(visible_pages(&state).contains(&0));
    let pitch = style.height_pt + loom_writer_core::PAGE_GAP_PT;
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..4 {
        app.invoke_page_scroll_changed(0.0, 24.0 + step as f32 * pitch);
        assert!((state.viewport.borrow().scroll_y - (24.0 + step as f32 * pitch)).abs() < 0.5);
        seen.extend(visible_pages(&state));
    }
    assert!(
        seen.contains(&3),
        "scrolling four pitches reaches page four, saw {seen:?}"
    );
}

#[test]
fn page_down_walks_across_page_boundaries_and_keeps_the_caret_visible() {
    let (app, state) = multi_page();
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let style = state.current.borrow().page.page_style();
    app.invoke_selection_changed(0, 0);
    let mut pages = std::collections::BTreeSet::new();
    for _ in 0..40 {
        app.invoke_page_move(1, false);
        let focus = state.current.borrow().selection().focus;
        let doc = state.current.borrow().clone();
        let (page, y, height) = caret_page_and_y(&doc, focus);
        pages.insert(page);
        let (top, bottom) = sheet_span(&style, page);
        let drawn = drawn_y(&style, y);
        assert!(drawn >= top && drawn + height <= bottom);
        // Visible: below the scroll offset and above its bottom edge.
        let scroll = state.viewport.borrow().scroll_y;
        let abs = drawn + caret_scroll::PAGE_TOP_INSET;
        assert!(
            abs >= scroll - 1.0 && abs <= scroll + app.get_page_view_height() + 1.0,
            "caret at {abs} not within view {scroll}..{}",
            scroll + app.get_page_view_height()
        );
    }
    assert!(
        pages.len() >= 3 && pages.contains(&1),
        "Page Down crosses page boundaries one page at a time, saw {pages:?}"
    );
    let max = *pages.iter().max().unwrap();
    assert_eq!(pages.len(), max + 1, "no page is skipped: {pages:?}");
}

#[test]
fn a_find_match_on_page_three_is_selected_and_revealed() {
    let (app, state) = multi_page();
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let style = state.current.borrow().page.page_style();
    let bar = app.global::<FindBar>();
    bar.invoke_open_requested(false);
    // "number 60" first occurs well past page two.
    bar.set_query("number 60 ".into());
    bar.invoke_query_changed("number 60 ".into());
    let doc = state.current.borrow().clone();
    let selection = doc.selection();
    let (low, high) = (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    );
    assert_eq!(&doc.editor_text()[low..high], "number 60 ");
    let (page, y, height) = caret_page_and_y(&doc, low);
    assert!(page >= 2, "the match is on page {}", page + 1);
    let scroll = state.viewport.borrow().scroll_y;
    let abs = drawn_y(&style, y) + caret_scroll::PAGE_TOP_INSET;
    assert!(
        scroll > 0.0 && abs >= scroll && abs + height <= scroll + app.get_page_view_height() + 1.0,
        "the match at {abs} must be revealed in view {scroll}..{}",
        scroll + app.get_page_view_height()
    );
}

#[test]
fn a_click_in_the_gap_between_pages_goes_to_the_nearest_page() {
    let (_app, state) = multi_page();
    let doc = state.current.borrow().clone();
    let style = doc.page.page_style();
    let viewport = unit_viewport(&style);
    let layout = doc.layout(&style, viewport).unwrap();
    let last_of_page_one = layout.pages[0].fragments.last().unwrap();
    let first_of_page_two = layout.pages[1].fragments.first().unwrap();
    let offset_of = |block_id, within: usize| {
        let mut cursor = 0;
        for block in &doc.blocks {
            if block.id == block_id {
                return cursor + within;
            }
            cursor += block.text.as_str().len() + 1;
        }
        unreachable!()
    };
    let end_one = offset_of(last_of_page_one.block_id, last_of_page_one.end);
    let start_two = offset_of(first_of_page_two.block_id, first_of_page_two.start);
    // Gap rows in the editor's coordinates: just under the first sheet's
    // bottom edge and just above the second sheet's top edge.
    let sheet_bottom = style.height_pt - style.margin_top_pt;
    let near_first = writer_pointer_offset(&doc, viewport, 40.0, sheet_bottom + 4.0);
    let near_second = writer_pointer_offset(
        &doc,
        viewport,
        40.0,
        sheet_bottom + loom_writer_core::PAGE_GAP_PT + style.margin_top_pt - 4.0,
    );
    let (near_first, near_second) = (near_first.expect("hit"), near_second.expect("hit"));
    assert!(
        near_first <= end_one,
        "{near_first} should stay on page one (<= {end_one})"
    );
    assert!(
        near_second >= start_two,
        "{near_second} should reach page two (>= {start_two})"
    );
}
