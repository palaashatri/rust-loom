//! Keystroke and scroll work follows what is on screen, not document length.
//! These assert amounts of work (rows built, pages materialised, recovery
//! writes), never wall-clock time.

use super::actions_tests::{test_state, text_document};
use super::frame_bench_tests::{build_document, setup};
use super::*;

fn rows_in_window(doc: &WriterDocument, viewport: PageViewport) -> projection::Projection {
    projection::project(doc, viewport, Some(800.0))
}

fn viewport_at(zoom: f32, scroll_y: f32) -> PageViewport {
    PageViewport {
        zoom,
        scroll_y,
        ..PageViewport::default()
    }
}

fn geometry(rows: &[WriterRenderBlock]) -> Vec<(f32, f32, f32, f32)> {
    rows.iter()
        .map(|row| (row.x, row.y, row.width, row.height))
        .collect()
}

#[test]
fn rows_built_for_a_view_do_not_grow_with_the_document() {
    let counts: Vec<(usize, usize)> = [20usize, 100, 400]
        .into_iter()
        .map(|pages| {
            let doc = build_document(pages);
            let all = projection::project(&doc, PageViewport::default(), None);
            let windowed = rows_in_window(&doc, PageViewport::default());
            assert_eq!(all.page_count, windowed.page_count);
            (all.rows.len(), windowed.rows.len())
        })
        .collect();
    let [(all20, win20), (all100, win100), (all400, win400)] = counts[..] else {
        unreachable!()
    };
    assert!(
        all400 > 15 * all20 / 2,
        "documents differ in size: {all20} {all100} {all400}"
    );
    assert!(
        win100.abs_diff(win20) <= 10 && win400.abs_diff(win20) <= 10,
        "rows built per view: {win20} {win100} {win400}"
    );
    assert!(win400 * 20 < all400, "{win400} of {all400} rows built");
}

#[test]
fn a_windowed_projection_is_a_slice_of_the_full_one() {
    let doc = build_document(60);
    let pitch = (doc.page.page_style().height_pt + loom_writer_core::PAGE_GAP_PT) * 1.5;
    for scroll in [0.0, 20.0 * pitch + 130.0, 55.0 * pitch] {
        let viewport = viewport_at(1.5, scroll);
        let full = projection::project(&doc, viewport, None);
        let part = rows_in_window(&doc, viewport);
        let (full_rows, part_rows) = (geometry(&full.rows), geometry(&part.rows));
        assert!(!part_rows.is_empty() && part_rows.len() < full_rows.len());
        let at = full_rows
            .windows(part_rows.len())
            .position(|window| window == part_rows.as_slice())
            .unwrap_or_else(|| panic!("scroll {scroll}: windowed rows are not a slice"));
        assert!(
            part.pages.start <= at / 30 + 1,
            "window starts near its rows"
        );
        assert_eq!(full.stack_height, part.stack_height);
        for rect in &part.comment_rects {
            assert!(part.pages.contains(&(rect.page_index as usize)));
        }
        for rect in &part.selection_rects {
            assert!(part.pages.contains(&(rect.page_index as usize)));
        }
    }
}

#[test]
fn scrolling_far_moves_the_window_and_the_selection_leaves_it() {
    let (app, state) = setup(build_document(100));
    let first = app.get_visible_page_first();
    assert_eq!(first, 0);
    assert!(app.get_visible_page_count() <= 6, "few sheets are drawn");
    assert!(app.get_page_count() > 100);
    let before = app.get_render_blocks().row_count();
    assert!(before > 0 && before < 400, "{before} rows");
    let caret_rects = app.get_selection_rects().row_count();
    assert_eq!(caret_rects, 1, "the caret on page one is drawn");

    let pitch = (app.get_page_height_pt() + app.get_page_gap_pt()) * state.viewport.borrow().zoom;
    app.invoke_page_scroll_changed(0.0, 60.0 * pitch);
    assert!(
        (56..=61).contains(&app.get_visible_page_first()),
        "window follows the scroll: {}",
        app.get_visible_page_first()
    );
    assert!(app.get_visible_page_count() <= 7);
    let rows = app.get_render_blocks();
    assert!(rows.row_count() > 0 && rows.row_count() < 400);
    let top = rows.row_data(0).expect("a row").y;
    assert!(top > 55.0 * app.get_page_height_pt(), "rows moved: {top}");
    assert_eq!(
        app.get_selection_rects().row_count(),
        0,
        "caret is off screen"
    );
}

#[test]
fn a_keystroke_in_a_long_document_projects_only_the_visible_pages() {
    for pages in [20usize, 400] {
        let (app, state) = setup(build_document(pages));
        let text = state.current.borrow().editor_text();
        let mut edited = text.clone();
        edited.insert(10, 'x');
        app.invoke_document_edited(edited.as_str().into(), 11, 11);
        assert_eq!(state.current.borrow().editor_text(), edited);
        let rows = app.get_render_blocks().row_count();
        assert!(rows < 300, "{pages} pages: {rows} rows after a keystroke");
        assert!(app.get_visible_page_count() <= 7);
        assert!(app.get_page_count() as usize >= pages);
    }
}

#[test]
fn the_dirty_check_ignores_the_caret_and_sees_every_content_change() {
    let doc = build_document(3);
    let mut same = doc.clone();
    same.set_selection(TextSelection::range(4, 40));
    assert!(document_content_equal(&doc, &same));
    assert_ne!(doc.to_content_json(), same.to_content_json());

    let mut retitled = doc.clone();
    retitled.title = "Other".into();
    let mut retyped = doc.clone();
    retyped.blocks[2].text = Text::from_str("changed");
    let mut restyled = doc.clone();
    restyled.blocks[1].kind = "heading2".into();
    let mut commented = doc.clone();
    let id = commented.blocks[0].id;
    commented
        .add_comment_thread(id, 0, 3, "note")
        .expect("comment");
    let mut resized = doc.clone();
    resized.page.paper = loom_writer_core::PaperSize::Letter;
    let mut bolded = doc.clone();
    loom_writer_core::set_selection_bold(&mut bolded, TextSelection::range(2, 8), true);
    for (name, changed) in [
        ("title", retitled),
        ("text", retyped),
        ("kind", restyled),
        ("comment", commented),
        ("paper", resized),
        ("bold", bolded),
    ] {
        assert!(!document_content_equal(&doc, &changed), "{name}");
        assert_ne!(
            doc.to_content_json(),
            changed.to_content_json(),
            "{name} is real content"
        );
    }
}

#[test]
fn history_sizes_are_measured_once_per_entry() {
    let mut history = EditorHistory::with_budget(32, usize::MAX);
    let mut doc = text_document("alpha");
    for step in 0..5u64 {
        let mut next = doc.clone();
        next.replace_paragraphs(&format!("alpha {}", "beta ".repeat(step as usize + 1)));
        history.record(doc, next.clone(), HistoryKind::DocumentAction, step);
        doc = next;
    }
    for entry in history.undo.iter() {
        assert_eq!(entry.before_bytes, document_bytes(&entry.before));
        assert_eq!(entry.after_bytes, document_bytes(&entry.after));
    }
    let total: usize = history.undo.iter().map(HistoryEntry::memory_bytes).sum();
    assert_eq!(history.total_bytes(), total);

    // Typing that coalesces into one entry re-measures only the new "after".
    let mut typing = EditorHistory::with_budget(32, usize::MAX);
    let first = text_document("a");
    let second = text_document("ab");
    let third = text_document("abc and more text");
    typing.record(first, second.clone(), HistoryKind::Typing, 0);
    typing.record(second, third.clone(), HistoryKind::Typing, 100);
    assert_eq!(typing.undo_len(), 1);
    assert_eq!(typing.undo[0].after_bytes, document_bytes(&third));
    assert_eq!(typing.total_bytes(), typing.undo[0].memory_bytes());
}

#[test]
fn deferred_recovery_arms_one_timer_per_window_and_settles() {
    let pending = recovery::DeferredWrite::default();
    assert!(pending.note_edit(), "the first keystroke arms the timer");
    for _ in 0..50 {
        assert!(!pending.note_edit(), "later keystrokes share it");
    }
    assert!(pending.fire(), "the timer finds the draft stale");
    assert!(!pending.fire(), "and only once");
    assert!(pending.note_edit(), "the next burst arms a new timer");
    pending.settled();
    assert!(!pending.fire(), "a synchronous write already covered it");
}

#[test]
fn typing_defers_the_recovery_write_and_every_other_action_writes_at_once() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("first line"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    let writes = || recovery::WRITES.with(Cell::get);
    let baseline = writes();

    let mut text = state.current.borrow().editor_text();
    for key in 0..30 {
        text.push('x');
        let caret = text.len() as i32;
        app.invoke_document_edited(text.as_str().into(), caret, caret);
        assert_eq!(writes(), baseline, "keystroke {key} wrote no draft");
    }
    assert!(recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));
    assert!(
        state.history.borrow().can_undo(),
        "typing is still undoable"
    );
    assert!(
        app.get_document_dirty(),
        "typing marks the document unsaved at once"
    );

    flush_deferred_recovery(&app, &state);
    assert_eq!(writes(), baseline + 1, "one write covers all 30 keystrokes");
    assert!(!recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));
    flush_deferred_recovery(&app, &state);
    assert_eq!(
        writes(),
        baseline + 1,
        "nothing is stale, nothing is written"
    );

    // Any other change writes the draft before returning, and settles a
    // pending typing write instead of leaving a second one behind.
    text.push('y');
    let caret = text.len() as i32;
    app.invoke_document_edited(text.as_str().into(), caret, caret);
    assert!(recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));
    let mut formatted = state.current.borrow().clone();
    loom_writer_core::set_selection_heading(&mut formatted, TextSelection::caret(0), 1);
    apply_with_history(&app, &state, formatted, HistoryKind::DocumentAction);
    assert_eq!(writes(), baseline + 2, "formatting writes at once");
    assert!(!recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));
}

#[test]
fn a_synchronous_write_invalidates_a_draft_still_being_serialized() {
    let pending = recovery::DeferredWrite::default();
    let in_flight = pending.epoch();
    pending.settled();
    assert_ne!(
        pending.epoch(),
        in_flight,
        "the worker's result is dropped when it arrives"
    );
}
