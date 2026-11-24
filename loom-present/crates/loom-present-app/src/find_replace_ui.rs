use std::rc::Rc;

use slint::ComponentHandle;

use crate::{FindReplaceState, PresentApp};

pub(crate) type QueryChangedCb = Rc<dyn Fn(&PresentApp, &str, bool, bool)>;
pub(crate) type ReplaceTextCb = Rc<dyn Fn(&PresentApp, &str)>;

pub(crate) struct FindReplaceCallbacks {
    pub on_query_changed: QueryChangedCb,
    pub on_find_prev: Rc<dyn Fn(&PresentApp)>,
    pub on_find_next: Rc<dyn Fn(&PresentApp)>,
    pub on_replace_one: ReplaceTextCb,
    pub on_replace_all: ReplaceTextCb,
}

pub(crate) fn open(app: &PresentApp) {
    app.global::<crate::WelcomeGuideState>().set_open(false);
    app.global::<crate::StartHubState>().set_open(false);
    app.global::<crate::PrintPreviewState>().set_open(false);
    let state = app.global::<FindReplaceState>();
    state.set_open(true);
    let toggled = !state.get_focus_request();
    state.set_focus_request(toggled);
    let q = state.get_query();
    if !q.is_empty() {
        state.invoke_query_changed(q);
    }
}

pub(crate) fn update_match_counts(app: &PresentApp, current: usize, total: usize) {
    let state = app.global::<FindReplaceState>();
    state.set_current_match(current as i32);
    state.set_total_matches(total as i32);
}

pub(crate) fn wire(app: &PresentApp, callbacks: FindReplaceCallbacks) {
    let state = app.global::<FindReplaceState>();

    let weak = app.as_weak();
    state.on_close_modal(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<FindReplaceState>().set_open(false);
        }
    });

    let weak = app.as_weak();
    let on_query_changed = Rc::clone(&callbacks.on_query_changed);
    state.on_query_changed(move |query| {
        if let Some(app) = weak.upgrade() {
            let s = app.global::<FindReplaceState>();
            on_query_changed(
                &app,
                query.as_str(),
                s.get_case_sensitive(),
                s.get_include_notes(),
            );
        }
    });

    let weak = app.as_weak();
    let on_find_prev = Rc::clone(&callbacks.on_find_prev);
    state.on_find_prev(move || {
        if let Some(app) = weak.upgrade() {
            on_find_prev(&app);
        }
    });

    let weak = app.as_weak();
    let on_find_next = Rc::clone(&callbacks.on_find_next);
    state.on_find_next(move || {
        if let Some(app) = weak.upgrade() {
            on_find_next(&app);
        }
    });

    let weak = app.as_weak();
    let on_replace_one = Rc::clone(&callbacks.on_replace_one);
    state.on_replace_one(move || {
        if let Some(app) = weak.upgrade() {
            let replacement = app.global::<FindReplaceState>().get_replacement();
            on_replace_one(&app, replacement.as_str());
        }
    });

    let weak = app.as_weak();
    let on_replace_all = Rc::clone(&callbacks.on_replace_all);
    state.on_replace_all(move || {
        if let Some(app) = weak.upgrade() {
            let replacement = app.global::<FindReplaceState>().get_replacement();
            on_replace_all(&app, replacement.as_str());
        }
    });
}
