//! Work per edit, per slide switch and per refresh must not grow with the
//! length of the deck: counts of thumbnails built and drafts serialized, not
//! wall-clock time.

use super::frame_bench_tests::{build_session, setup};
use super::*;
use slint::Model;

fn built() -> usize {
    picture_view::ROWS_BUILT.with(std::cell::Cell::get)
}

fn jobs() -> usize {
    recovery_deferred::JOBS.with(std::cell::Cell::get)
}

fn populated(app: &PresentApp) -> Vec<usize> {
    app.get_slide_thumbs()
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.key.is_empty())
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn an_edit_rebuilds_one_thumbnail_whatever_the_deck_length() {
    let mut per_edit = Vec::new();
    for slides in [20, 300] {
        let (app, _state) = setup(build_session(slides));
        app.invoke_select_element(0);
        let before = built();
        app.invoke_update_element_content("Edited title".into());
        per_edit.push(built() - before);
    }
    assert_eq!(
        per_edit,
        vec![1, 1],
        "one slide changed, one thumbnail built"
    );
}

#[test]
fn switching_slides_rebuilds_no_thumbnail_whatever_the_deck_length() {
    for slides in [20, 300] {
        let (app, _state) = setup(build_session(slides));
        let before = built();
        for target in [3, 7, 11, 5] {
            app.invoke_select_slide(target);
        }
        assert_eq!(
            built() - before,
            0,
            "{slides} slides: nothing changed on any slide"
        );
    }
}

#[test]
fn only_the_strip_window_has_thumbnails_and_scrolling_fills_the_next_one() {
    let (app, _state) = setup(build_session(300));
    let window = populated(&app);
    assert_eq!(
        window,
        (0..24).collect::<Vec<_>>(),
        "the default window only"
    );
    assert_eq!(app.get_slide_thumbs().row_count(), 300);

    app.invoke_strip_window_changed(150, 10);
    let window = populated(&app);
    assert!(
        (150..160).all(|index| window.contains(&index)),
        "the scrolled-to slides are filled"
    );
    // Rows that were filled before stay valid; nothing outside is built.
    assert!(window.len() <= 24 + 10);
}

#[test]
fn refreshing_never_serializes_the_deck_until_the_timer_flushes() {
    let (app, state) = setup(build_session(300));
    let before = jobs();
    for _ in 0..50 {
        refresh(&app, &state);
    }
    assert_eq!(jobs(), before, "50 refreshes serialized nothing");
    assert!(recovery_deferred::is_stale());
    app.invoke_recovery_flush();
    assert_eq!(jobs(), before + 1, "one draft covers all of them");
    assert!(!recovery_deferred::is_stale());
    app.invoke_recovery_flush();
    assert_eq!(jobs(), before + 1, "nothing stale, nothing written");
}

#[test]
fn a_save_checkpoint_cancels_a_pending_draft() {
    let (app, state) = setup(build_session(20));
    refresh(&app, &state);
    assert!(recovery_deferred::is_stale());
    recovery_deferred::invalidate();
    let before = jobs();
    app.invoke_recovery_flush();
    assert_eq!(jobs(), before, "the checkpoint already covers the draft");
}
