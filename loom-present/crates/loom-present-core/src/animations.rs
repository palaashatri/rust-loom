use crate::Slide;
use serde::{Deserialize, Serialize};

/// Phase of an object animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationPhase {
    #[default]
    BuildIn,
    Action,
    BuildOut,
}

/// Trigger condition for an object animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BuildTrigger {
    #[default]
    OnClick,
    WithPrevious,
    AfterPrevious,
}

/// Animation effect presets for slide elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationEffect {
    #[default]
    Appear,
    FadeIn,
    FlyFromLeft,
    FlyFromRight,
    ZoomIn,
    FadeOut,
    FlyToLeft,
    FlyToRight,
    ZoomOut,
    Pulse,
}

/// One object animation entry bound to a slide element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationEntry {
    pub element_id: String,
    pub effect: AnimationEffect,
    #[serde(default)]
    pub phase: AnimationPhase,
    #[serde(default)]
    pub trigger: BuildTrigger,
    /// Zero-based position in the click-build sequence.
    pub build_order: usize,
    pub duration_ms: u32,
    pub delay_ms: u32,
}

/// Sorts animation entries into their build order (stable).
pub fn sort_animation_builds(entries: &mut [AnimationEntry]) {
    entries.sort_by_key(|entry| entry.build_order);
}

/// Computes normalized eased progress 0.0..=1.0 of an entry at elapsed ms (after delay), using
/// smoothstep easing (3t^2 - 2t^3) clamped to [0,1]. Before delay elapses => 0.0; past end => 1.0.
pub fn animation_progress(entry: &AnimationEntry, elapsed_ms: u32) -> f64 {
    if elapsed_ms < entry.delay_ms {
        return 0.0;
    }
    if entry.duration_ms == 0 || elapsed_ms - entry.delay_ms >= entry.duration_ms {
        return 1.0;
    }
    let t = f64::from(elapsed_ms - entry.delay_ms) / f64::from(entry.duration_ms);
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Appends a new animation entry to `slide` and normalizes `build_order`.
pub fn add_animation(
    slide: &mut Slide,
    element_id: impl Into<String>,
    effect: AnimationEffect,
    phase: AnimationPhase,
    trigger: BuildTrigger,
    duration_ms: u32,
) {
    let build_order = slide.animations.len();
    slide.animations.push(AnimationEntry {
        element_id: element_id.into(),
        effect,
        phase,
        trigger,
        build_order,
        duration_ms,
        delay_ms: 0,
    });
}

/// Removes the animation at `index` and renumbers `build_order`.
pub fn remove_animation(slide: &mut Slide, index: usize) -> Option<AnimationEntry> {
    if index >= slide.animations.len() {
        return None;
    }
    let removed = slide.animations.remove(index);
    for (i, entry) in slide.animations.iter_mut().enumerate() {
        entry.build_order = i;
    }
    Some(removed)
}

/// Counts how many OnClick build steps exist on `slide`.
pub fn click_build_count(slide: &Slide) -> usize {
    slide
        .animations
        .iter()
        .filter(|a| a.trigger == BuildTrigger::OnClick)
        .count()
}

/// Returns the next non-hidden slide index after `current`, or `None`.
pub fn next_visible_slide(slides: &[Slide], current: usize) -> Option<usize> {
    ((current + 1)..slides.len()).find(|&i| !slides[i].hidden)
}

/// Returns the previous non-hidden slide index before `current`, or `None`.
pub fn prev_visible_slide(slides: &[Slide], current: usize) -> Option<usize> {
    (0..current).rev().find(|&i| !slides[i].hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_slide_skip_navigation() {
        let mut s0 = Slide::new("s0", "Zero", "title");
        let mut s1 = Slide::new("s1", "One", "title_content");
        s1.hidden = true;
        let s2 = Slide::new("s2", "Two", "title_content");
        s0.hidden = false;
        let slides = vec![s0, s1, s2];
        assert_eq!(next_visible_slide(&slides, 0), Some(2));
        assert_eq!(prev_visible_slide(&slides, 2), Some(0));
    }

    #[test]
    fn add_and_remove_animation_renumbers_order() {
        let mut s = Slide::new("s0", "Zero", "title");
        add_animation(
            &mut s,
            "e1",
            AnimationEffect::FadeIn,
            AnimationPhase::BuildIn,
            BuildTrigger::OnClick,
            300,
        );
        add_animation(
            &mut s,
            "e2",
            AnimationEffect::ZoomIn,
            AnimationPhase::BuildOut,
            BuildTrigger::WithPrevious,
            200,
        );
        assert_eq!(click_build_count(&s), 1);
        remove_animation(&mut s, 0);
        assert_eq!(s.animations.len(), 1);
        assert_eq!(s.animations[0].build_order, 0);
    }
}
