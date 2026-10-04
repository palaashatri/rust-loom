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
    });
}

fn build(app: &PresentApp) -> Result<Presenter, String> {
    let window = PresenterWindow::new().map_err(|error| error.to_string())?;
    Theme::get(&window).set_active_theme(Theme::get(app).get_active_theme());
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
    window
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    Ok(Presenter {
        window,
        _ticker: ticker,
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
