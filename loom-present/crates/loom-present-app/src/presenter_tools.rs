use std::collections::HashMap;

use loom_present_core::{AnnotationDrawingTool, AnnotationStroke, SlideAnnotationOverlay};

/// Live presenter rehearsal state: annotation tool, per-slide strokes, laser pointer, and numeric jump buffer.
#[derive(Debug, Default)]
pub struct PresenterToolState {
    pub active_tool: AnnotationDrawingTool,
    pub overlays: HashMap<usize, SlideAnnotationOverlay>,
    pub jump_buffer: String,
    pub laser_pos: Option<(f32, f32)>,
}

impl PresenterToolState {
    pub fn set_tool(&mut self, tool: AnnotationDrawingTool) {
        self.active_tool = tool;
        if tool != AnnotationDrawingTool::LaserPointer {
            self.laser_pos = None;
        }
    }

    pub fn push_jump_digit(&mut self, ch: char) {
        if ch.is_ascii_digit() && self.jump_buffer.len() < 4 {
            self.jump_buffer.push(ch);
        }
    }

    pub fn clear_jump(&mut self) {
        self.jump_buffer.clear();
    }

    /// Parses the buffered 1-based slide number, clears the buffer, and returns a 0-based index if valid.
    pub fn commit_jump(&mut self, total_slides: usize) -> Option<usize> {
        let raw = std::mem::take(&mut self.jump_buffer);
        let one_based: usize = raw.parse().ok()?;
        if one_based == 0 || one_based > total_slides {
            return None;
        }
        Some(one_based - 1)
    }

    pub fn begin_stroke(&mut self, slide_index: usize, x: f32, y: f32) {
        if self.active_tool == AnnotationDrawingTool::LaserPointer {
            self.laser_pos = Some((x, y));
            return;
        }
        if self.active_tool == AnnotationDrawingTool::Eraser {
            self.erase_near(slide_index, x, y, 16.0);
            return;
        }
        let (color, width) = match self.active_tool {
            AnnotationDrawingTool::Pen => ("#ef4444".to_string(), 3.0),
            AnnotationDrawingTool::Highlighter => ("rgba(250, 204, 21, 0.45)".to_string(), 14.0),
            AnnotationDrawingTool::LaserPointer | AnnotationDrawingTool::Eraser => return,
        };
        let overlay = self
            .overlays
            .entry(slide_index)
            .or_insert_with(|| SlideAnnotationOverlay::new(slide_index));
        overlay.strokes.push(AnnotationStroke {
            tool: self.active_tool,
            color,
            width,
            points: vec![(x, y)],
        });
    }

    pub fn extend_stroke(&mut self, slide_index: usize, x: f32, y: f32) {
        if self.active_tool == AnnotationDrawingTool::LaserPointer {
            self.laser_pos = Some((x, y));
            return;
        }
        if self.active_tool == AnnotationDrawingTool::Eraser {
            self.erase_near(slide_index, x, y, 16.0);
            return;
        }
        if let Some(overlay) = self.overlays.get_mut(&slide_index) {
            if let Some(last) = overlay.strokes.last_mut() {
                last.points.push((x, y));
            }
        }
    }

    pub fn erase_near(&mut self, slide_index: usize, x: f32, y: f32, radius: f32) {
        let r2 = radius * radius;
        if let Some(overlay) = self.overlays.get_mut(&slide_index) {
            overlay.strokes.retain(|stroke| {
                !stroke
                    .points
                    .iter()
                    .any(|&(px, py)| (px - x) * (px - x) + (py - y) * (py - y) <= r2)
            });
        }
    }

    pub fn clear_slide(&mut self, slide_index: usize) {
        self.overlays.remove(&slide_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_buffer_parses_one_based_slide() {
        let mut state = PresenterToolState::default();
        state.push_jump_digit('2');
        state.push_jump_digit('3');
        assert_eq!(state.commit_jump(30), Some(22));
        assert!(state.jump_buffer.is_empty());
        state.push_jump_digit('0');
        assert_eq!(state.commit_jump(10), None);
    }

    #[test]
    fn pen_and_eraser_strokes() {
        let mut state = PresenterToolState::default();
        state.set_tool(AnnotationDrawingTool::Pen);
        state.begin_stroke(0, 10.0, 20.0);
        state.extend_stroke(0, 15.0, 25.0);
        assert_eq!(state.overlays[&0].strokes.len(), 1);
        state.set_tool(AnnotationDrawingTool::Eraser);
        state.erase_near(0, 12.0, 22.0, 10.0);
        assert!(state.overlays[&0].strokes.is_empty());
    }
}
