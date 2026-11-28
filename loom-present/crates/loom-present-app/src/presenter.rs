//! Presenter view: a second window with the current slide's notes, the next
//! slide's title and a running clock. It can sit on another display while the
//! slideshow fills the audience screen.
//!
//! The window is a thin view of the session. Its Next and Previous controls
//! invoke the main window's own slide callbacks, so navigation, selection and
//! refresh behave exactly as they do from the keyboard in the slideshow.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use loom_present_core::PresentationSession;
use slint::{ComponentHandle, Global, SharedString, Timer, TimerMode};

use crate::{PresentApp, PresenterWindow, Theme};

/// A pausable stopwatch. Every method takes the current instant so the logic
/// is deterministic under test.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    banked: Duration,
    running_since: Option<Instant>,
}

impl Clock {
    pub(crate) fn started(now: Instant) -> Self {
        Self {
            banked: Duration::ZERO,
            running_since: Some(now),
        }
    }

    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        self.banked
            + self
                .running_since
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    pub(crate) fn toggle(&mut self, now: Instant) {
        match self.running_since.take() {
            Some(since) => self.banked += now.saturating_duration_since(since),
            None => self.running_since = Some(now),
        }
    }

    /// Back to zero. A running clock keeps running from the new zero.
    pub(crate) fn reset(&mut self, now: Instant) {
        self.banked = Duration::ZERO;
        if self.running_since.is_some() {
            self.running_since = Some(now);
        }
    }
}

/// `MM:SS`, or `H:MM:SS` once an hour has passed.
pub(crate) fn format_elapsed(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

struct Presenter {
    window: PresenterWindow,
    _ticker: Timer,
    slide_count: Rc<std::cell::Cell<usize>>,
    active_slide: Rc<std::cell::Cell<usize>>,
    tools: Rc<RefCell<crate::presenter_tools::PresenterToolState>>,
}

fn sync_annotations(
    win: &PresenterWindow,
    st: &crate::presenter_tools::PresenterToolState,
    slide_idx: usize,
) {
    let mut dots = Vec::new();
    if let Some(overlay) = st.overlays.get(&slide_idx) {
        for stroke in &overlay.strokes {
            let color = match stroke.tool {
                loom_present_core::AnnotationDrawingTool::Highlighter => {
                    slint::Color::from_argb_u8(115, 250, 204, 21)
                }
                _ => slint::Color::from_argb_u8(255, 239, 68, 68),
            };
            let size = stroke.width.max(2.0);
            for &(px, py) in &stroke.points {
                dots.push(crate::PresenterStrokeDot {
                    x: px,
                    y: py,
                    color,
                    size,
                });
            }
        }
    }
    win.set_stroke_dots(slint::ModelRc::new(slint::VecModel::from(dots)));
    if let Some((lx, ly)) = st.laser_pos {
        win.set_has_laser(true);
        win.set_laser_x(lx);
        win.set_laser_y(ly);
    } else {
        win.set_has_laser(false);
    }
}

thread_local! {
    static PRESENTER: RefCell<Option<Presenter>> = const { RefCell::new(None) };
}

/// Show the presenter window, creating it on first use. Reopening after a
/// close keeps the same clock so the talk's running time is not lost.
pub(crate) fn open(app: &PresentApp, session: &PresentationSession) -> Result<(), String> {
    let already_built = PRESENTER.with(|cell| cell.borrow().is_some());
    if !already_built {
        let presenter = build(app)?;
        PRESENTER.with(|cell| *cell.borrow_mut() = Some(presenter));
    }
    sync(session);
    PRESENTER.with(|cell| {
        let cell = cell.borrow();
        let presenter = cell.as_ref().expect("presenter was just built");
        presenter.window.show().map_err(|error| error.to_string())
    })
}

/// Hide the presenter window if it is open.
pub(crate) fn close() {
    PRESENTER.with(|cell| {
        if let Some(presenter) = cell.borrow().as_ref() {
            let _ = presenter.window.hide();
        }
    });
}

/// Push the session's current slide into the presenter window, if one exists.
pub(crate) fn sync(session: &PresentationSession) {
    PRESENTER.with(|cell| {
        let cell = cell.borrow();
        let Some(presenter) = cell.as_ref() else {
            return;
        };
        let document = &session.document;
        presenter.slide_count.set(document.len());
        presenter.active_slide.set(document.active_index);
        let index = document.active_index;
        let window = &presenter.window;
        window.set_position_text(format!("Slide {} of {}", index + 1, document.len()).into());
        match document.active_slide() {
            Some(slide) => {
                window.set_slide_title(slide.title.as_str().into());
                window.set_notes(slide.speaker_notes.trim().into());
            }
            None => {
                window.set_slide_title(SharedString::new());
                window.set_notes(SharedString::new());
            }
        }
        window.set_current_rows(crate::picture_view::rows_for(
            document,
            document.active_slide(),
        ));
        window.set_next_rows(crate::presenter_thumbs::next_rows(document));
        let next = crate::presenter_thumbs::next_index(index, document.len())
            .and_then(|next| document.slides.get(next));
        window.set_has_next(next.is_some());
        window.set_has_previous(index > 0);
        window.set_next_title(
            next.map_or_else(SharedString::new, |slide| slide.title.as_str().into()),
        );
        sync_annotations(window, &presenter.tools.borrow(), index);
    });
}

/// Gives the presenter window the same appearance as the main window. Its own
/// `Theme` follows the operating system while `System` is chosen.
fn copy_theme(app: &PresentApp, window: &PresenterWindow) {
    let source = Theme::get(app);
    let theme = Theme::get(window);
    theme.set_appearance(source.get_appearance());
    theme.set_active_theme(source.get_active_theme());
    theme.set_ui_font_family(source.get_ui_font_family());
}

/// Pushes the active live dynamic-background frame to an open presenter window.
pub(crate) fn update_dynamic_bg(has_bg: bool, image: slint::Image) {
    PRESENTER.with(|cell| {
        if let Some(presenter) = cell.borrow().as_ref() {
            presenter.window.set_has_dynamic_bg(has_bg);
            presenter.window.set_dynamic_bg_image(image);
        }
    });
}

/// Re-applies the main window's appearance to an open presenter window.
pub(crate) fn mirror_theme(app: &PresentApp) {
    PRESENTER.with(|cell| {
        if let Some(presenter) = cell.borrow().as_ref() {
            copy_theme(app, &presenter.window);
        }
    });
}

fn build(app: &PresentApp) -> Result<Presenter, String> {
    let window = PresenterWindow::new().map_err(|error| error.to_string())?;
    copy_theme(app, &window);
    Theme::get(&window).set_text_scale(Theme::get(app).get_text_scale());
    let clock = Rc::new(RefCell::new(Clock::started(Instant::now())));

    let show_clock = {
        let clock = clock.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                let clock = clock.borrow();
                window.set_elapsed(format_elapsed(clock.elapsed(Instant::now())).into());
                window.set_timer_running(clock.is_running());
            }
        }
    };
    show_clock();

    let ticker = Timer::default();
    ticker.start(
        TimerMode::Repeated,
        Duration::from_millis(250),
        show_clock.clone(),
    );

    {
        let clock = clock.clone();
        let show_clock = show_clock.clone();
        window.on_toggle_timer(move || {
            clock.borrow_mut().toggle(Instant::now());
            show_clock();
        });
    }
    {
        let clock = clock.clone();
        let show_clock = show_clock.clone();
        window.on_reset_timer(move || {
            clock.borrow_mut().reset(Instant::now());
            show_clock();
        });
    }
    {
        let app = app.as_weak();
        window.on_next_slide(move || {
            if let Some(app) = app.upgrade() {
                app.invoke_next_slide();
            }
        });
    }
    {
        let app = app.as_weak();
        window.on_previous_slide(move || {
            if let Some(app) = app.upgrade() {
                app.invoke_prev_slide();
            }
        });
    }
    window.on_close_presenter(close);
    let slide_count = Rc::new(std::cell::Cell::new(1));
    let active_slide = Rc::new(std::cell::Cell::new(0));
    let tools = Rc::new(RefCell::new(
        crate::presenter_tools::PresenterToolState::default(),
    ));
    {
        let tools = tools.clone();
        window.on_select_tool(move |idx| {
            let tool = match idx {
                1 => loom_present_core::AnnotationDrawingTool::Highlighter,
                2 => loom_present_core::AnnotationDrawingTool::LaserPointer,
                3 => loom_present_core::AnnotationDrawingTool::Eraser,
                _ => loom_present_core::AnnotationDrawingTool::Pen,
            };
            tools.borrow_mut().set_tool(tool);
        });
    }
    {
        let tools = tools.clone();
        let active_slide = active_slide.clone();
        let w = window.as_weak();
        window.on_clear_annotations(move || {
            tools.borrow_mut().clear_slide(active_slide.get());
            if let Some(win) = w.upgrade() {
                sync_annotations(&win, &tools.borrow(), active_slide.get());
            }
        });
    }
    {
        let tools = tools.clone();
        let active_slide = active_slide.clone();
        let w = window.as_weak();
        let tools_b = tools.clone();
        let active_b = active_slide.clone();
        let w_b = w.clone();
        window.on_stroke_begin(move |fx, fy| {
            let mut st = tools_b.borrow_mut();
            let idx = active_b.get();
            st.begin_stroke(idx, fx, fy);
            if let Some(win) = w_b.upgrade() {
                sync_annotations(&win, &st, idx);
            }
        });
        let tools_m = tools.clone();
        let active_m = active_slide.clone();
        window.on_stroke_move(move |fx, fy| {
            let mut st = tools_m.borrow_mut();
            let idx = active_m.get();
            st.extend_stroke(idx, fx, fy);
            if let Some(win) = w.upgrade() {
                sync_annotations(&win, &st, idx);
            }
        });
    }
    {
        let tools = tools.clone();
        let w = window.as_weak();
        window.on_jump_digit(move |s| {
            if let Some(ch) = s.chars().next() {
                tools.borrow_mut().push_jump_digit(ch);
                if let Some(win) = w.upgrade() {
                    win.set_jump_buffer(tools.borrow().jump_buffer.clone().into());
                }
            }
        });
    }
    {
        let tools = tools.clone();
        let w = window.as_weak();
        let app = app.as_weak();
        let slide_count = slide_count.clone();
        window.on_commit_jump(move || {
            let target = tools.borrow_mut().commit_jump(slide_count.get());
            if let Some(target) = target {
                if let Some(a) = app.upgrade() {
                    a.invoke_select_slide(target as i32);
                }
            }
            if let Some(win) = w.upgrade() {
                win.set_jump_buffer("".into());
            }
        });
    }
    window
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    Ok(Presenter {
        window,
        _ticker: ticker,
        slide_count,
        active_slide,
        tools,
    })
}

#[cfg(test)]
pub(crate) fn with_window<T>(read: impl FnOnce(&PresenterWindow) -> T) -> Option<T> {
    PRESENTER.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|presenter| read(&presenter.window))
    })
}

#[cfg(test)]
pub(crate) fn forget() {
    PRESENTER.with(|cell| *cell.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_formats_as_minutes_then_hours() {
        assert_eq!(format_elapsed(Duration::ZERO), "00:00");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "00:59");
        assert_eq!(format_elapsed(Duration::from_secs(61)), "01:01");
        assert_eq!(format_elapsed(Duration::from_secs(3599)), "59:59");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(
            format_elapsed(Duration::from_secs(3 * 3600 + 7 * 60 + 9)),
            "3:07:09"
        );
    }

    #[test]
    fn the_clock_pauses_resumes_and_resets() {
        let t0 = Instant::now();
        let at = |seconds: u64| t0 + Duration::from_secs(seconds);
        let mut clock = Clock::started(t0);
        assert_eq!(clock.elapsed(at(10)), Duration::from_secs(10));

        clock.toggle(at(10));
        assert!(!clock.is_running());
        assert_eq!(
            clock.elapsed(at(500)),
            Duration::from_secs(10),
            "paused time does not count"
        );

        clock.toggle(at(500));
        assert!(clock.is_running());
        assert_eq!(
            clock.elapsed(at(505)),
            Duration::from_secs(15),
            "resume continues from 10 s"
        );

        clock.reset(at(505));
        assert_eq!(clock.elapsed(at(505)), Duration::ZERO);
        assert_eq!(
            clock.elapsed(at(508)),
            Duration::from_secs(3),
            "a running clock keeps running after reset"
        );

        clock.toggle(at(508));
        clock.reset(at(600));
        assert!(!clock.is_running(), "reset does not restart a paused clock");
        assert_eq!(clock.elapsed(at(700)), Duration::ZERO);
    }
}
