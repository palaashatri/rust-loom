//! Live object-level Magic Move (`TransitionKind::Morph`) slideshow playback.
//!
//! Computes a [`MorphPlan`] between consecutive slides and interpolates element
//! geometry, rotation, and opacity on a ~16 ms timer during slideshow preview,
//! snapping immediately when `Theme.tokens.reduced-motion` is enabled or when
//! leaving preview mode.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use loom_present_core::{
    compute_morph_plan, interpolate_morph_frame, ElementType, MorphPlan, PresentationDocument,
    Slide, SlideElement,
};
use slint::{ComponentHandle, Global, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::{
    element_names, element_type_index, element_type_name, object_parts, picture_view,
    refresh_without_recovery, ElementVisualMeta, GuiState, ObjectParts, PresentApp, Theme,
};

const FRAME_INTERVAL_MS: u64 = 16;
const FRAME_STEP: f32 = 0.04;

struct ActiveMorph {
    plan: MorphPlan,
    from: Slide,
    to: Slide,
    progress: f32,
    state: Weak<GuiState>,
}

struct MorphDriver {
    timer: Timer,
    active: Option<ActiveMorph>,
}

impl MorphDriver {
    fn new() -> Self {
        Self {
            timer: Timer::default(),
            active: None,
        }
    }

    fn stop(&mut self) {
        self.timer.stop();
        self.active = None;
    }
}

thread_local! {
    static MORPH: RefCell<MorphDriver> = RefCell::new(MorphDriver::new());
}

/// Cancels any in-flight Magic Move playback timer.
pub(crate) fn cancel() {
    MORPH.with(|cell| cell.borrow_mut().stop());
}

/// Returns whether a Magic Move transition timer is currently running.
#[cfg(test)]
pub(crate) fn is_running() -> bool {
    MORPH.with(|cell| cell.borrow().active.is_some())
}

fn source_element<'a>(
    from: &'a Slide,
    to: &'a Slide,
    source_idx: Option<usize>,
    target_idx: Option<usize>,
) -> Option<&'a SlideElement> {
    target_idx
        .and_then(|i| to.elements.get(i))
        .or_else(|| source_idx.and_then(|i| from.elements.get(i)))
}

fn apply_frame(
    app: &PresentApp,
    document: &PresentationDocument,
    from: &Slide,
    to: &Slide,
    plan: &MorphPlan,
    t: f32,
) {
    let items = interpolate_morph_frame(plan, t);
    let mut labels = Vec::with_capacity(items.len());
    let mut placeholders = Vec::with_capacity(items.len());
    let mut contents = Vec::with_capacity(items.len());
    let mut xs = Vec::with_capacity(items.len());
    let mut ys = Vec::with_capacity(items.len());
    let mut widths = Vec::with_capacity(items.len());
    let mut heights = Vec::with_capacity(items.len());
    let mut rotations = Vec::with_capacity(items.len());
    let mut types = Vec::with_capacity(items.len());
    let mut selected = Vec::with_capacity(items.len());
    let mut images = Vec::with_capacity(items.len());
    let mut parts = Vec::with_capacity(items.len());
    let mut metas = Vec::with_capacity(items.len());

    for item in &items {
        let backing = source_element(from, to, item.source_index, item.target_index);
        let text = if item.element_type == ElementType::Picture {
            backing
                .map(|e| picture_view::picture_name(document, e))
                .unwrap_or_else(|| "Picture".to_string())
        } else {
            item.content.clone()
        };
        labels.push(SharedString::from(element_names::label(
            element_type_name(&item.element_type),
            &text,
            false,
        )));
        placeholders.push(SharedString::new());
        contents.push(SharedString::from(text.as_str()));
        xs.push(item.x);
        ys.push(item.y);
        widths.push(item.width);
        heights.push(item.height);
        rotations.push(item.rotation_deg);
        types.push(element_type_index(&item.element_type));
        selected.push(false);
        let img = backing
            .and_then(|e| document.asset_for(e))
            .map(picture_view::image_for)
            .unwrap_or_default();
        images.push(img);
        let part_row = backing.map(object_parts::parts_for).unwrap_or_default();
        parts.push(ModelRc::new(VecModel::from(part_row)));
        let fx = backing.and_then(|e| e.effects.as_ref());
        metas.push(ElementVisualMeta {
            opacity: item.opacity,
            corner_radius: fx.map(|f| f.corner_radius).unwrap_or(0.0),
            has_shadow: fx.and_then(|f| f.shadow.as_ref()).is_some(),
            shadow_dx: fx
                .and_then(|f| f.shadow.as_ref())
                .map(|s| s.offset_x)
                .unwrap_or(0.0),
            shadow_dy: fx
                .and_then(|f| f.shadow.as_ref())
                .map(|s| s.offset_y)
                .unwrap_or(0.0),
            locked: false,
            flip_h: backing.map(|e| e.flip_h).unwrap_or(false),
            flip_v: backing.map(|e| e.flip_v).unwrap_or(false),
        });
    }

    app.set_element_labels(ModelRc::new(VecModel::from(labels)));
    app.set_element_placeholders(ModelRc::new(VecModel::from(placeholders)));
    app.set_element_contents(ModelRc::new(VecModel::from(contents)));
    app.set_element_xs(ModelRc::new(VecModel::from(xs)));
    app.set_element_ys(ModelRc::new(VecModel::from(ys)));
    app.set_element_widths(ModelRc::new(VecModel::from(widths)));
    app.set_element_heights(ModelRc::new(VecModel::from(heights)));
    app.set_element_rotations(ModelRc::new(VecModel::from(rotations)));
    app.set_element_types(ModelRc::new(VecModel::from(types)));
    app.set_element_selected(ModelRc::new(VecModel::from(selected)));
    app.set_element_images(ModelRc::new(VecModel::from(images)));
    let op = app.global::<ObjectParts>();
    op.set_main(ModelRc::new(VecModel::from(parts)));
    op.set_element_meta(ModelRc::new(VecModel::from(metas)));
    app.set_reveal(t.clamp(0.0, 1.0));
}

/// Starts a Magic Move transition from `from` to `to` if in preview mode and
/// reduced-motion is disabled; otherwise snaps immediately to `to`.
pub(crate) fn start(app: &PresentApp, state: &Rc<GuiState>, from: Slide, to: Slide) {
    cancel();
    if !app.get_is_preview_mode() || Theme::get(app).get_tokens().reduced_motion {
        app.set_reveal(1.0);
        return;
    }
    let plan = compute_morph_plan(&from, &to);
    {
        let session = state.session.borrow();
        apply_frame(app, &session.document, &from, &to, &plan, 0.0);
    }
    let app_weak = app.as_weak();
    let state_weak = Rc::downgrade(state);
    MORPH.with(|cell| {
        let mut driver = cell.borrow_mut();
        driver.active = Some(ActiveMorph {
            plan,
            from,
            to,
            progress: 0.0,
            state: state_weak,
        });
        driver.timer.start(
            TimerMode::Repeated,
            Duration::from_millis(FRAME_INTERVAL_MS),
            move || {
                tick(&app_weak);
            },
        );
    });
}

fn tick(app_weak: &slint::Weak<PresentApp>) {
    let Some(app) = app_weak.upgrade() else {
        cancel();
        return;
    };
    let reduced_motion = Theme::get(&app).get_tokens().reduced_motion;
    if !app.get_is_preview_mode() || reduced_motion {
        let state_opt = MORPH.with(|cell| {
            let mut driver = cell.borrow_mut();
            let s = driver.active.as_ref().and_then(|a| a.state.upgrade());
            driver.stop();
            s
        });
        app.set_reveal(1.0);
        if let Some(state) = state_opt {
            refresh_without_recovery(&app, &state);
        }
        return;
    }

    struct ApplyFrame {
        state: Rc<GuiState>,
        plan: MorphPlan,
        from: Slide,
        to: Slide,
        t: f32,
    }

    enum StepOutcome {
        Apply(Box<ApplyFrame>),
        Finish(Option<Rc<GuiState>>),
        None,
    }

    let outcome = MORPH.with(|cell| {
        let mut driver = cell.borrow_mut();
        let Some(active) = driver.active.as_mut() else {
            return StepOutcome::None;
        };
        active.progress = (active.progress + FRAME_STEP).min(1.0);
        if active.progress >= 1.0 {
            let st = active.state.upgrade();
            driver.stop();
            StepOutcome::Finish(st)
        } else if let Some(st) = active.state.upgrade() {
            StepOutcome::Apply(Box::new(ApplyFrame {
                state: st,
                plan: active.plan.clone(),
                from: active.from.clone(),
                to: active.to.clone(),
                t: active.progress,
            }))
        } else {
            driver.stop();
            StepOutcome::None
        }
    });

    match outcome {
        StepOutcome::Apply(frame) => {
            if let Ok(session) = frame.state.session.try_borrow() {
                apply_frame(
                    &app,
                    &session.document,
                    &frame.from,
                    &frame.to,
                    &frame.plan,
                    frame.t,
                );
            }
        }
        StepOutcome::Finish(state_opt) => {
            app.set_reveal(1.0);
            if let Some(state) = state_opt {
                refresh_without_recovery(&app, &state);
            }
        }
        StepOutcome::None => {}
    }
}

/// Evaluates the currently active morph at explicit progress `t` for deterministic tests.
#[cfg(test)]
pub(crate) fn step_to_for_test(app: &PresentApp, state: &Rc<GuiState>, t: f32) -> bool {
    let snapshot = MORPH.with(|cell| {
        let driver = cell.borrow();
        driver
            .active
            .as_ref()
            .map(|a| (a.plan.clone(), a.from.clone(), a.to.clone()))
    });
    let Some((plan, from, to)) = snapshot else {
        return false;
    };
    let session = state.session.borrow();
    apply_frame(app, &session.document, &from, &to, &plan, t);
    true
}
