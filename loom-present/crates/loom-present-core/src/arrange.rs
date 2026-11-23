use crate::{PresentationSession, Slide, SlideElement, SLIDE_HEIGHT, SLIDE_WIDTH};

/// Alignment mode for [`PresentationSession::align_selected`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// Distribution axis for [`PresentationSession::distribute_selected`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
}

impl Slide {
    /// Aligns the given element IDs to their rightmost right edge.
    pub fn align_right(&mut self, ids: &[String]) -> bool {
        let max_right = self
            .elements
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| e.x + e.width)
            .fold(f32::NEG_INFINITY, f32::max);
        if !max_right.is_finite() {
            return false;
        }
        let mut changed = false;
        for elem in &mut self.elements {
            if ids.contains(&elem.id) {
                let target_x = max_right - elem.width;
                if (elem.x - target_x).abs() > f32::EPSILON {
                    elem.x = target_x;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Aligns the given element IDs to their bottommost bottom edge.
    pub fn align_bottom(&mut self, ids: &[String]) -> bool {
        let max_bottom = self
            .elements
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| e.y + e.height)
            .fold(f32::NEG_INFINITY, f32::max);
        if !max_bottom.is_finite() {
            return false;
        }
        let mut changed = false;
        for elem in &mut self.elements {
            if ids.contains(&elem.id) {
                let target_y = max_bottom - elem.height;
                if (elem.y - target_y).abs() > f32::EPSILON {
                    elem.y = target_y;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Aligns the given element IDs to their average vertical center.
    pub fn align_middle(&mut self, ids: &[String]) -> bool {
        let mut sum = 0.0_f32;
        let mut count = 0usize;
        for elem in &self.elements {
            if ids.contains(&elem.id) {
                sum += elem.y + elem.height * 0.5;
                count += 1;
            }
        }
        if count < 2 {
            return false;
        }
        let center_y = sum / count as f32;
        let mut changed = false;
        for elem in &mut self.elements {
            if ids.contains(&elem.id) {
                let target_y = center_y - elem.height * 0.5;
                if (elem.y - target_y).abs() > f32::EPSILON {
                    elem.y = target_y;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Duplicates the elements matching `ids` with a small cascade offset and
    /// returns the new element IDs.
    pub fn duplicate_elements(&mut self, ids: &[String]) -> Vec<String> {
        let mut new_ids = Vec::new();
        let mut copies: Vec<SlideElement> = Vec::new();
        let mut serial = self.elements.len() + 1;
        for elem in &self.elements {
            if ids.contains(&elem.id) {
                let mut copy = elem.clone();
                let mut candidate = format!("{}-copy-{serial}", elem.id);
                while self.elements.iter().any(|e| e.id == candidate)
                    || copies.iter().any(|e| e.id == candidate)
                {
                    serial += 1;
                    candidate = format!("{}-copy-{serial}", elem.id);
                }
                serial += 1;
                copy.id = candidate.clone();
                copy.x = (copy.x + 20.0).min((SLIDE_WIDTH - copy.width).max(0.0));
                copy.y = (copy.y + 20.0).min((SLIDE_HEIGHT - copy.height).max(0.0));
                new_ids.push(candidate);
                copies.push(copy);
            }
        }
        self.elements.extend(copies);
        new_ids
    }
}

impl PresentationSession {
    /// Brings the selected elements to the front of the z-order (1 undo step).
    pub fn bring_to_front(&mut self) -> bool {
        let ids = self.selected_elements.clone();
        if ids.is_empty() {
            return false;
        }
        let Some(slide) = self.document.slides.get(self.document.active_index) else {
            return false;
        };
        let already_front = slide
            .elements
            .iter()
            .rev()
            .take(ids.len())
            .all(|e| ids.contains(&e.id));
        if already_front {
            return false;
        }
        self.checkpoint();
        if let Some(slide) = self.document.slides.get_mut(self.document.active_index) {
            let mut front = Vec::new();
            slide.elements.retain(|e| {
                if ids.contains(&e.id) {
                    front.push(e.clone());
                    false
                } else {
                    true
                }
            });
            slide.elements.extend(front);
        }
        true
    }

    /// Brings the first selected element forward by one z-order step (1 undo step).
    pub fn bring_forward(&mut self) -> bool {
        let Some(id) = self.selected_elements.first().cloned() else {
            return false;
        };
        let idx = self
            .document
            .slides
            .get(self.document.active_index)
            .and_then(|s| s.elements.iter().position(|e| e.id == id));
        let Some(pos) = idx else {
            return false;
        };
        let len = self.document.slides[self.document.active_index]
            .elements
            .len();
        if pos + 1 >= len {
            return false;
        }
        self.checkpoint();
        self.document.slides[self.document.active_index]
            .elements
            .swap(pos, pos + 1);
        true
    }

    /// Sends the first selected element backward by one z-order step (1 undo step).
    pub fn send_backward(&mut self) -> bool {
        let Some(id) = self.selected_elements.first().cloned() else {
            return false;
        };
        let idx = self
            .document
            .slides
            .get(self.document.active_index)
            .and_then(|s| s.elements.iter().position(|e| e.id == id));
        let Some(pos) = idx else {
            return false;
        };
        if pos == 0 {
            return false;
        }
        self.checkpoint();
        self.document.slides[self.document.active_index]
            .elements
            .swap(pos, pos - 1);
        true
    }

    /// Sends the selected elements to the back of the z-order (1 undo step).
    pub fn send_to_back(&mut self) -> bool {
        let ids = self.selected_elements.clone();
        if ids.is_empty() {
            return false;
        }
        let Some(slide) = self.document.slides.get(self.document.active_index) else {
            return false;
        };
        let already_back = slide
            .elements
            .iter()
            .take(ids.len())
            .all(|e| ids.contains(&e.id));
        if already_back {
            return false;
        }
        self.checkpoint();
        if let Some(slide) = self.document.slides.get_mut(self.document.active_index) {
            let mut back = Vec::new();
            slide.elements.retain(|e| {
                if ids.contains(&e.id) {
                    back.push(e.clone());
                    false
                } else {
                    true
                }
            });
            back.append(&mut slide.elements);
            slide.elements = back;
        }
        true
    }

    /// Aligns selected elements on the active slide (1 undo step).
    pub fn align_selected(&mut self, mode: AlignMode) -> bool {
        let ids = self.selected_elements.clone();
        if ids.len() < 2 {
            return false;
        }
        self.checkpoint();
        let Some(slide) = self.document.slides.get_mut(self.document.active_index) else {
            return false;
        };
        match mode {
            AlignMode::Right => slide.align_right(&ids),
            AlignMode::Bottom => slide.align_bottom(&ids),
            AlignMode::Middle => slide.align_middle(&ids),
            AlignMode::Left => {
                let min_x = slide
                    .elements
                    .iter()
                    .filter(|e| ids.contains(&e.id))
                    .map(|e| e.x)
                    .fold(f32::INFINITY, f32::min);
                for elem in &mut slide.elements {
                    if ids.contains(&elem.id) {
                        elem.x = min_x;
                    }
                }
                true
            }
            AlignMode::Center => {
                let mut sum = 0.0_f32;
                let mut count = 0usize;
                for elem in &slide.elements {
                    if ids.contains(&elem.id) {
                        sum += elem.x + elem.width * 0.5;
                        count += 1;
                    }
                }
                let cx = sum / count.max(1) as f32;
                for elem in &mut slide.elements {
                    if ids.contains(&elem.id) {
                        elem.x = cx - elem.width * 0.5;
                    }
                }
                true
            }
            AlignMode::Top => {
                let min_y = slide
                    .elements
                    .iter()
                    .filter(|e| ids.contains(&e.id))
                    .map(|e| e.y)
                    .fold(f32::INFINITY, f32::min);
                for elem in &mut slide.elements {
                    if ids.contains(&elem.id) {
                        elem.y = min_y;
                    }
                }
                true
            }
        }
    }

    /// Distributes selected elements evenly along `axis` (1 undo step).
    pub fn distribute_selected(&mut self, axis: DistributeAxis) -> bool {
        let ids = self.selected_elements.clone();
        if ids.len() < 3 {
            return false;
        }
        self.checkpoint();
        let Some(slide) = self.document.slides.get_mut(self.document.active_index) else {
            return false;
        };
        let mut indices: Vec<usize> = slide
            .elements
            .iter()
            .enumerate()
            .filter_map(|(i, e)| if ids.contains(&e.id) { Some(i) } else { None })
            .collect();
        if indices.len() < 3 {
            return false;
        }
        match axis {
            DistributeAxis::Horizontal => {
                indices.sort_by(|&a, &b| {
                    slide.elements[a]
                        .x
                        .partial_cmp(&slide.elements[b].x)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let first_x = slide.elements[indices[0]].x;
                let last = &slide.elements[*indices.last().unwrap()];
                let last_right = last.x + last.width;
                let total_w: f32 = indices.iter().map(|&i| slide.elements[i].width).sum();
                let gap = ((last_right - first_x) - total_w) / (indices.len() - 1) as f32;
                let mut cursor = first_x;
                for &i in &indices {
                    slide.elements[i].x = cursor;
                    cursor += slide.elements[i].width + gap;
                }
            }
            DistributeAxis::Vertical => {
                indices.sort_by(|&a, &b| {
                    slide.elements[a]
                        .y
                        .partial_cmp(&slide.elements[b].y)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let first_y = slide.elements[indices[0]].y;
                let last = &slide.elements[*indices.last().unwrap()];
                let last_bottom = last.y + last.height;
                let total_h: f32 = indices.iter().map(|&i| slide.elements[i].height).sum();
                let gap = ((last_bottom - first_y) - total_h) / (indices.len() - 1) as f32;
                let mut cursor = first_y;
                for &i in &indices {
                    slide.elements[i].y = cursor;
                    cursor += slide.elements[i].height + gap;
                }
            }
        }
        true
    }

    /// Rotates selected elements by `delta_deg` degrees (1 undo step).
    pub fn rotate_selected_by(&mut self, delta_deg: f32) -> bool {
        let ids = self.selected_elements.clone();
        if ids.is_empty() || delta_deg.abs() <= f32::EPSILON {
            return false;
        }
        self.checkpoint();
        if let Some(slide) = self.document.slides.get_mut(self.document.active_index) {
            for elem in &mut slide.elements {
                if ids.contains(&elem.id) {
                    elem.rotation_deg = (elem.rotation_deg + delta_deg).rem_euclid(360.0);
                }
            }
        }
        true
    }

    /// Duplicates selected elements on the active slide and selects the copies (1 undo step).
    pub fn duplicate_selected(&mut self) -> Vec<String> {
        let ids = self.selected_elements.clone();
        if ids.is_empty() {
            return Vec::new();
        }
        self.checkpoint();
        let Some(slide) = self.document.slides.get_mut(self.document.active_index) else {
            return Vec::new();
        };
        let new_ids = slide.duplicate_elements(&ids);
        if !new_ids.is_empty() {
            self.selected_elements = new_ids.clone();
        }
        new_ids
    }

    /// Sets the active slide's background color (1 undo step).
    pub fn set_slide_background(&mut self, color: impl Into<String>) -> bool {
        let color = color.into();
        let Some(slide) = self.document.slides.get(self.document.active_index) else {
            return false;
        };
        if slide.bg_color == color {
            return false;
        }
        self.checkpoint();
        if let Some(slide) = self.document.slides.get_mut(self.document.active_index) {
            slide.bg_color = color;
        }
        true
    }

    /// Sets the deck's widescreen (16:9) vs standard (4:3) aspect ratio metadata (1 undo step).
    pub fn set_aspect_ratio_wide(&mut self, wide: bool) {
        self.checkpoint();
        self.document.aspect_ratio_wide = wide;
    }
}
