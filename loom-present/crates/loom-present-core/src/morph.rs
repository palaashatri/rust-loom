//! Object-level Magic Move (`TransitionKind::Morph`) planning and interpolation.
//!
//! Matches slide objects between consecutive slides by:
//! 1. Exact element ID (`from.id == to.id`),
//! 2. Same `(ElementType, content)` pair,
//! 3. Same `Title` or `Subtitle` role.
//!
//! Matched objects smoothly interpolate `(x, y, width, height, rotation_deg, opacity)`
//! using shortest-arc rotation and cubic ease-in-out; unmatched outgoing objects
//! fade out in place and unmatched incoming objects fade in at their destination.

use crate::{normalize_angle_degrees, ElementType, Slide, SlideElement};

/// Geometric and opacity state of an element at one endpoint of a morph transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
}

impl MorphRect {
    fn from_element(elem: &SlideElement) -> Self {
        let opacity = elem
            .effects
            .as_ref()
            .map(|e| e.opacity.clamp(0.0, 1.0))
            .unwrap_or(1.0);
        Self {
            x: elem.x,
            y: elem.y,
            width: elem.width,
            height: elem.height,
            rotation_deg: normalize_angle_degrees(elem.rotation_deg),
            opacity,
        }
    }
}

/// How an element participates in a Magic Move transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphItemKind {
    /// Present on both slides: smoothly morphs from `from` to `to`.
    Matched,
    /// Present only on the destination slide: fades in at `to`.
    Entering,
    /// Present only on the source slide: fades out at `from`.
    Exiting,
}

/// One element transition entry inside a [`MorphPlan`].
#[derive(Debug, Clone, PartialEq)]
pub struct MorphElementPair {
    pub element_id: String,
    pub element_type: ElementType,
    pub content: String,
    pub from: MorphRect,
    pub to: MorphRect,
    pub kind: MorphItemKind,
    pub source_index: Option<usize>,
    pub target_index: Option<usize>,
}

/// Complete object-level transition plan between two slides.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphPlan {
    pub items: Vec<MorphElementPair>,
    pub from_bg: String,
    pub to_bg: String,
}

/// Interpolated visual frame state of one element at progress `t` in `0.0..=1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphFrameItem {
    pub element_id: String,
    pub element_type: ElementType,
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    pub kind: MorphItemKind,
    pub source_index: Option<usize>,
    pub target_index: Option<usize>,
}

/// Cubic ease-in-out curve matching Keynote Magic Move pacing.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Interpolates between two angles along the shortest angular arc.
fn lerp_angle_deg(from: f32, to: f32, t: f32) -> f32 {
    let delta = (to - from + 540.0).rem_euclid(360.0) - 180.0;
    normalize_angle_degrees(from + delta * t)
}

/// Builds a deterministic Magic Move plan from `from` to `to`.
pub fn compute_morph_plan(from: &Slide, to: &Slide) -> MorphPlan {
    let from_len = from.elements.len();
    let to_len = to.elements.len();
    let mut from_matched = vec![false; from_len];
    let mut to_matched = vec![false; to_len];
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    // Pass 1: exact element ID match.
    for (ti, target) in to.elements.iter().enumerate() {
        if let Some(fi) = from
            .elements
            .iter()
            .enumerate()
            .position(|(i, src)| !from_matched[i] && src.id == target.id)
        {
            from_matched[fi] = true;
            to_matched[ti] = true;
            pairs.push((fi, ti));
        }
    }

    // Pass 2: identical (ElementType, content) match (trimmed non-empty content,
    // or matching geometric shape type with identical content).
    for (ti, target) in to.elements.iter().enumerate() {
        if to_matched[ti] {
            continue;
        }
        let target_text = target.content.trim();
        let allow_empty = !matches!(
            target.element_type,
            ElementType::Title | ElementType::Subtitle | ElementType::BodyText
        );
        if target_text.is_empty() && !allow_empty {
            continue;
        }
        if let Some(fi) = from.elements.iter().enumerate().position(|(i, src)| {
            !from_matched[i]
                && src.element_type == target.element_type
                && src.content.trim() == target_text
        }) {
            from_matched[fi] = true;
            to_matched[ti] = true;
            pairs.push((fi, ti));
        }
    }

    // Pass 3: Title / Subtitle semantic role match.
    for (ti, target) in to.elements.iter().enumerate() {
        if to_matched[ti]
            || !matches!(
                target.element_type,
                ElementType::Title | ElementType::Subtitle
            )
        {
            continue;
        }
        if let Some(fi) = from
            .elements
            .iter()
            .enumerate()
            .position(|(i, src)| !from_matched[i] && src.element_type == target.element_type)
        {
            from_matched[fi] = true;
            to_matched[ti] = true;
            pairs.push((fi, ti));
        }
    }

    // Emit exiting elements first (behind matched/entering), then target-order matched/entering.
    let mut items = Vec::with_capacity(from_len + to_len);
    for (fi, src) in from.elements.iter().enumerate() {
        if !from_matched[fi] {
            let from_rect = MorphRect::from_element(src);
            let mut to_rect = from_rect;
            to_rect.opacity = 0.0;
            items.push(MorphElementPair {
                element_id: src.id.clone(),
                element_type: src.element_type.clone(),
                content: src.content.clone(),
                from: from_rect,
                to: to_rect,
                kind: MorphItemKind::Exiting,
                source_index: Some(fi),
                target_index: None,
            });
        }
    }

    let match_for_target: std::collections::HashMap<usize, usize> =
        pairs.into_iter().map(|(fi, ti)| (ti, fi)).collect();

    for (ti, target) in to.elements.iter().enumerate() {
        let to_rect = MorphRect::from_element(target);
        if let Some(&fi) = match_for_target.get(&ti) {
            let src = &from.elements[fi];
            let from_rect = MorphRect::from_element(src);
            items.push(MorphElementPair {
                element_id: target.id.clone(),
                element_type: target.element_type.clone(),
                content: target.content.clone(),
                from: from_rect,
                to: to_rect,
                kind: MorphItemKind::Matched,
                source_index: Some(fi),
                target_index: Some(ti),
            });
        } else {
            let mut from_rect = to_rect;
            from_rect.opacity = 0.0;
            items.push(MorphElementPair {
                element_id: target.id.clone(),
                element_type: target.element_type.clone(),
                content: target.content.clone(),
                from: from_rect,
                to: to_rect,
                kind: MorphItemKind::Entering,
                source_index: None,
                target_index: Some(ti),
            });
        }
    }

    MorphPlan {
        items,
        from_bg: from.bg_color.clone(),
        to_bg: to.bg_color.clone(),
    }
}

/// Evaluates `plan` at normalized progress `t` in `0.0..=1.0`.
pub fn interpolate_morph_frame(plan: &MorphPlan, t: f32) -> Vec<MorphFrameItem> {
    let eased = ease_in_out_cubic(t);
    plan.items
        .iter()
        .map(|pair| MorphFrameItem {
            element_id: pair.element_id.clone(),
            element_type: pair.element_type.clone(),
            content: pair.content.clone(),
            x: lerp(pair.from.x, pair.to.x, eased),
            y: lerp(pair.from.y, pair.to.y, eased),
            width: lerp(pair.from.width, pair.to.width, eased).max(1.0),
            height: lerp(pair.from.height, pair.to.height, eased).max(1.0),
            rotation_deg: lerp_angle_deg(pair.from.rotation_deg, pair.to.rotation_deg, eased),
            opacity: lerp(pair.from.opacity, pair.to.opacity, eased).clamp(0.0, 1.0),
            kind: pair.kind,
            source_index: pair.source_index,
            target_index: pair.target_index,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElementStyle;

    fn elem(
        id: &str,
        element_type: ElementType,
        content: &str,
        rect: (f32, f32, f32, f32),
        rot: f32,
    ) -> SlideElement {
        SlideElement {
            id: id.into(),
            element_type,
            content: content.into(),
            x: rect.0,
            y: rect.1,
            width: rect.2,
            height: rect.3,
            rotation_deg: rot,
            action: None,
            style: ElementStyle::default(),
            runs: Vec::new(),
            effects: None,
            group_id: None,
            locked: false,
            flip_h: false,
            flip_v: false,
            paragraphs: Vec::new(),
        }
    }

    #[test]
    fn matches_by_id_then_type_and_content_then_title_role() {
        let mut from = Slide::new("s1", "One", "content");
        from.elements = vec![
            elem(
                "hero",
                ElementType::ShapeCircle,
                "",
                (100.0, 100.0, 80.0, 80.0),
                350.0,
            ),
            elem(
                "a",
                ElementType::BodyText,
                "Shared point",
                (60.0, 200.0, 300.0, 60.0),
                0.0,
            ),
            elem(
                "t1",
                ElementType::Title,
                "Chapter 1",
                (60.0, 40.0, 800.0, 70.0),
                0.0,
            ),
            elem(
                "gone",
                ElementType::BodyText,
                "Only on slide 1",
                (60.0, 360.0, 200.0, 50.0),
                0.0,
            ),
        ];

        let mut to = Slide::new("s2", "Two", "content");
        to.elements = vec![
            elem(
                "hero",
                ElementType::ShapeCircle,
                "",
                (500.0, 240.0, 160.0, 160.0),
                10.0,
            ),
            elem(
                "b",
                ElementType::BodyText,
                "Shared point",
                (420.0, 200.0, 360.0, 80.0),
                0.0,
            ),
            elem(
                "t2",
                ElementType::Title,
                "Chapter 2",
                (120.0, 80.0, 700.0, 70.0),
                0.0,
            ),
            elem(
                "fresh",
                ElementType::StatCard,
                "99%",
                (700.0, 320.0, 180.0, 100.0),
                0.0,
            ),
        ];

        let plan = compute_morph_plan(&from, &to);
        assert_eq!(plan.items.len(), 5);
        assert_eq!(
            plan.items
                .iter()
                .filter(|i| i.kind == MorphItemKind::Matched)
                .count(),
            3
        );
        assert_eq!(
            plan.items
                .iter()
                .filter(|i| i.kind == MorphItemKind::Exiting)
                .count(),
            1
        );
        assert_eq!(
            plan.items
                .iter()
                .filter(|i| i.kind == MorphItemKind::Entering)
                .count(),
            1
        );

        let mid = interpolate_morph_frame(&plan, 0.5);
        let hero_mid = mid.iter().find(|i| i.element_id == "hero").unwrap();
        assert!((hero_mid.x - 300.0).abs() < 0.1);
        assert!((hero_mid.y - 170.0).abs() < 0.1);
        assert!((hero_mid.width - 120.0).abs() < 0.1);
        assert!((hero_mid.height - 120.0).abs() < 0.1);
        // Shortest-arc rotation from 350 deg -> 10 deg passes through 0 deg at t=0.5!
        assert!(
            hero_mid.rotation_deg.abs() < 0.5 || (hero_mid.rotation_deg - 360.0).abs() < 0.5,
            "expected ~0 deg midpoint, got {}",
            hero_mid.rotation_deg
        );

        let gone_mid = mid.iter().find(|i| i.element_id == "gone").unwrap();
        let fresh_mid = mid.iter().find(|i| i.element_id == "fresh").unwrap();
        assert!((gone_mid.opacity - 0.5).abs() < 0.05);
        assert!((fresh_mid.opacity - 0.5).abs() < 0.05);

        let end = interpolate_morph_frame(&plan, 1.0);
        let gone_end = end.iter().find(|i| i.element_id == "gone").unwrap();
        let fresh_end = end.iter().find(|i| i.element_id == "fresh").unwrap();
        assert_eq!(gone_end.opacity, 0.0);
        assert_eq!(fresh_end.opacity, 1.0);
    }
}
