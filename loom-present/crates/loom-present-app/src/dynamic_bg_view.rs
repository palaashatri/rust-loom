//! Live procedural Dynamic Background renderer, thumbnail cache, and inspector wiring.
//!
//! Drives a lightweight ~33 ms Slint `Timer` ONLY while the active slide has a
//! `DynamicBackground` with `animate == true` and `!Theme.tokens.reduced-motion`.
//! Reuses a single 320x180 RGBA8 scratch buffer so idle slides incur zero work.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use loom_present_core::{render_dynamic_frame, DynamicBackground, DynamicBgPreset};
use slint::{ComponentHandle, Global, Image, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode};

use crate::{refresh, set_status, ElementInk, GuiState, PresentApp, Theme};

const LIVE_WIDTH: u32 = 320;
const LIVE_HEIGHT: u32 = 180;
const THUMB_WIDTH: u32 = 160;
const THUMB_HEIGHT: u32 = 90;
const FRAME_INTERVAL_MS: u64 = 33;
const FRAME_STEP_SECS: f32 = 0.033;
const MAX_THUMB_CACHE_ENTRIES: usize = 32;

const SPEED_SLOW: f32 = 0.5;
const SPEED_CALM: f32 = 1.0;
const SPEED_LIVELY: f32 = 1.8;

struct LiveDriver {
    timer: Timer,
    running: bool,
    phase_secs: f32,
    scratch: Vec<u8>,
    last_static_key: Option<String>,
    thumb_cache: HashMap<String, Image>,
    bound_state: Option<Weak<GuiState>>,
}

impl LiveDriver {
    fn new() -> Self {
        Self {
            timer: Timer::default(),
            running: false,
            phase_secs: 0.0,
            scratch: Vec::new(),
            last_static_key: None,
            thumb_cache: HashMap::new(),
            bound_state: None,
        }
    }

    fn stop(&mut self) {
        if self.running {
            self.timer.stop();
            self.running = false;
        }
    }
}

thread_local! {
    static DRIVER: RefCell<LiveDriver> = RefCell::new(LiveDriver::new());
}

fn speed_to_index(speed: f32) -> i32 {
    if speed <= 0.75 {
        0
    } else if speed >= 1.4 {
        2
    } else {
        1
    }
}

fn index_to_speed(idx: i32) -> f32 {
    match idx {
        0 => SPEED_SLOW,
        2 => SPEED_LIVELY,
        _ => SPEED_CALM,
    }
}

fn render_image_into(
    bg: &DynamicBackground,
    phase_secs: f32,
    width: u32,
    height: u32,
    scratch: &mut Vec<u8>,
) -> Image {
    render_dynamic_frame(bg, phase_secs, width, height, scratch);
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(scratch, width, height);
    Image::from_rgba8(buffer)
}

/// Returns a cached static (`phase = 0.0`) thumbnail image for `bg`.
pub(crate) fn thumbnail_image(bg: &DynamicBackground) -> Image {
    let key = bg.cache_key();
    DRIVER.with(|cell| {
        let mut driver = cell.borrow_mut();
        if let Some(img) = driver.thumb_cache.get(&key) {
            return img.clone();
        }
        let mut buf = Vec::new();
        let img = render_image_into(bg, 0.0, THUMB_WIDTH, THUMB_HEIGHT, &mut buf);
        if driver.thumb_cache.len() >= MAX_THUMB_CACHE_ENTRIES {
            driver.thumb_cache.clear();
        }
        driver.thumb_cache.insert(key, img.clone());
        img
    })
}

/// Returns whether the ~33 ms live animation timer is currently active.
#[cfg(test)]
pub(crate) fn is_timer_running() -> bool {
    DRIVER.with(|cell| cell.borrow().running)
}

/// Stops any active dynamic background timer (used between tests / on shutdown).
#[allow(dead_code)]
pub(crate) fn stop_timer() {
    DRIVER.with(|cell| cell.borrow_mut().stop());
}

/// Synchronizes `ElementInk` dynamic background properties, active canvas/presenter
/// raster, and the ~33 ms animation timer with the current slide and reduced-motion flag.
pub(crate) fn sync(app: &PresentApp, state: &GuiState) {
    let active_bg = {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .and_then(|s| s.dynamic_bg.clone())
    };
    let ink = app.global::<ElementInk>();
    let Some(bg) = active_bg else {
        ink.set_has_dynamic_bg(false);
        DRIVER.with(|cell| {
            let mut driver = cell.borrow_mut();
            driver.stop();
            driver.last_static_key = None;
        });
        crate::presenter::update_dynamic_bg(false, Image::default());
        return;
    };

    ink.set_has_dynamic_bg(true);
    ink.set_dynamic_bg_preset(bg.preset.index() as i32);
    ink.set_dynamic_bg_animate(bg.animate);
    ink.set_dynamic_bg_speed(speed_to_index(bg.speed));

    let reduced_motion = Theme::get(app).get_tokens().reduced_motion;
    let should_animate = bg.animate && !reduced_motion;

    let frame = DRIVER.with(|cell| {
        let mut driver = cell.borrow_mut();
        if !should_animate {
            driver.stop();
            let key = bg.cache_key();
            if driver.last_static_key.as_deref() == Some(key.as_str()) {
                return None;
            }
            driver.last_static_key = Some(key);
            let mut static_bg = bg.clone();
            static_bg.animate = false;
            let img = render_image_into(
                &static_bg,
                0.0,
                LIVE_WIDTH,
                LIVE_HEIGHT,
                &mut driver.scratch,
            );
            return Some(img);
        }

        driver.last_static_key = None;
        let phase = driver.phase_secs;
        let img = render_image_into(&bg, phase, LIVE_WIDTH, LIVE_HEIGHT, &mut driver.scratch);

        if !driver.running {
            if let Some(state_weak) = driver.bound_state.clone() {
                let app_weak = app.as_weak();
                driver.running = true;
                driver.timer.start(
                    TimerMode::Repeated,
                    Duration::from_millis(FRAME_INTERVAL_MS),
                    move || {
                        tick(&app_weak, &state_weak);
                    },
                );
            }
        }
        Some(img)
    });

    if let Some(img) = frame {
        ink.set_dynamic_bg_image(img.clone());
        crate::presenter::update_dynamic_bg(true, img);
    }
}

fn tick(app_weak: &slint::Weak<PresentApp>, state_weak: &Weak<GuiState>) {
    let (Some(app), Some(state)) = (app_weak.upgrade(), state_weak.upgrade()) else {
        stop_timer();
        return;
    };
    let reduced_motion = Theme::get(&app).get_tokens().reduced_motion;
    let active_bg = state.session.try_borrow().ok().and_then(|s| {
        s.document
            .active_slide()
            .and_then(|sl| sl.dynamic_bg.clone())
    });
    let Some(bg) = active_bg else {
        stop_timer();
        return;
    };
    if !bg.animate || reduced_motion {
        stop_timer();
        return;
    }
    let img = DRIVER.with(|cell| {
        let mut driver = cell.borrow_mut();
        driver.phase_secs = (driver.phase_secs + FRAME_STEP_SECS) % 3600.0;
        let phase = driver.phase_secs;
        render_image_into(&bg, phase, LIVE_WIDTH, LIVE_HEIGHT, &mut driver.scratch)
    });
    app.global::<ElementInk>().set_dynamic_bg_image(img.clone());
    crate::presenter::update_dynamic_bg(true, img);
}

/// Connects the Document inspector's Dynamic Background controls to undoable session mutations.
pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    DRIVER.with(|cell| {
        let mut driver = cell.borrow_mut();
        driver.stop();
        driver.last_static_key = None;
        driver.bound_state = Some(Rc::downgrade(state));
    });
    let ink = app.global::<ElementInk>();

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_dynamic_bg_mode_changed(move |enable| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let changed = {
                let mut session = state.session.borrow_mut();
                if enable {
                    let preset_idx =
                        app.global::<ElementInk>().get_dynamic_bg_preset().max(0) as usize;
                    let preset = DynamicBgPreset::from_index(preset_idx)
                        .unwrap_or(DynamicBgPreset::AuroraVeil);
                    session.set_slide_dynamic_bg(Some(DynamicBackground::new(preset)))
                } else {
                    session.set_slide_dynamic_bg(None)
                }
            };
            if changed {
                refresh(&app, &state);
                set_status(
                    &app,
                    if enable {
                        "Enabled Dynamic Background"
                    } else {
                        "Switched to Solid Background"
                    },
                );
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_dynamic_bg_preset_picked(move |idx| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let Some(preset) = DynamicBgPreset::from_index(idx.max(0) as usize) else {
                return;
            };
            let changed = {
                let mut session = state.session.borrow_mut();
                let mut next = session
                    .document
                    .active_slide()
                    .and_then(|s| s.dynamic_bg.clone())
                    .unwrap_or_else(|| DynamicBackground::new(preset));
                next.preset = preset;
                next.custom_colors.clear();
                session.set_slide_dynamic_bg(Some(next))
            };
            if changed {
                refresh(&app, &state);
                set_status(
                    &app,
                    format!("Dynamic Background: {}", preset.display_name()),
                );
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_dynamic_bg_animate_toggled(move |animate| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let changed = {
                let mut session = state.session.borrow_mut();
                let current = session
                    .document
                    .active_slide()
                    .and_then(|s| s.dynamic_bg.clone());
                if let Some(mut bg) = current {
                    bg.animate = animate;
                    session.set_slide_dynamic_bg(Some(bg))
                } else {
                    false
                }
            };
            if changed {
                refresh(&app, &state);
                set_status(
                    &app,
                    if animate {
                        "Dynamic Background animation on"
                    } else {
                        "Dynamic Background animation paused"
                    },
                );
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_dynamic_bg_speed_picked(move |idx| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let speed = index_to_speed(idx);
            let changed = {
                let mut session = state.session.borrow_mut();
                let current = session
                    .document
                    .active_slide()
                    .and_then(|s| s.dynamic_bg.clone());
                if let Some(mut bg) = current {
                    bg.speed = speed;
                    session.set_slide_dynamic_bg(Some(bg))
                } else {
                    false
                }
            };
            if changed {
                refresh(&app, &state);
                let label = match idx {
                    0 => "Slow",
                    2 => "Lively",
                    _ => "Calm",
                };
                set_status(&app, format!("Dynamic Background speed: {label}"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_dynamic_bg_apply_all(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let changed = {
                let mut session = state.session.borrow_mut();
                let current = session
                    .document
                    .active_slide()
                    .and_then(|s| s.dynamic_bg.clone());
                session.apply_dynamic_bg_to_all_slides(current)
            };
            if changed {
                refresh(&app, &state);
                set_status(&app, "Applied Dynamic Background to all slides");
            }
        });
    }
}
