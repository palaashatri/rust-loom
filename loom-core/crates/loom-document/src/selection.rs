//! Shared selection vocabulary and direct-manipulation state across Loom applications.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeSet;

/// Text caret affinity relative to line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaretAffinity {
    /// Upstream (end of previous visual line).
    #[default]
    Upstream,
    /// Downstream (beginning of next visual line).
    Downstream,
}

/// Text range selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    /// Anchor position where selection began (byte offset).
    pub anchor: usize,
    /// Active focus/caret position (byte offset).
    pub focus: usize,
    /// Caret affinity.
    pub affinity: CaretAffinity,
}

impl TextSelection {
    /// Create a collapsed caret selection at `pos`.
    pub fn caret(pos: usize) -> Self {
        Self {
            anchor: pos,
            focus: pos,
            affinity: CaretAffinity::default(),
        }
    }

    /// Create a range selection from `anchor` to `focus`.
    pub fn range(anchor: usize, focus: usize) -> Self {
        Self {
            anchor,
            focus,
            affinity: CaretAffinity::default(),
        }
    }

    /// Whether the selection is a collapsed single caret.
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    /// Return normalized `(start, end)` byte range where `start <= end`.
    pub fn normalized_range(&self) -> (usize, usize) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }
}

/// 2D cell coordinate in a spreadsheet grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellCoord {
    /// 0-indexed column.
    pub col: usize,
    /// 0-indexed row.
    pub row: usize,
}

/// Rectangular range of grid cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    /// Top-left start cell.
    pub start: CellCoord,
    /// Bottom-right end cell.
    pub end: CellCoord,
}

impl CellRange {
    /// Create a normalized range between two cell coordinates.
    pub fn new(a: CellCoord, b: CellCoord) -> Self {
        Self {
            start: CellCoord {
                col: a.col.min(b.col),
                row: a.row.min(b.row),
            },
            end: CellCoord {
                col: a.col.max(b.col),
                row: a.row.max(b.row),
            },
        }
    }

    /// Whether a cell is contained within this range.
    pub fn contains(&self, cell: CellCoord) -> bool {
        cell.col >= self.start.col
            && cell.col <= self.end.col
            && cell.row >= self.start.row
            && cell.row <= self.end.row
    }
}

/// Spreadsheet grid selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GridSelection {
    /// Anchor cell where selection started.
    pub anchor: Option<CellCoord>,
    /// Focus / active cell cursor.
    pub active_cell: Option<CellCoord>,
    /// Selected rectangular cell ranges.
    pub ranges: Vec<CellRange>,
    /// Entire columns selected.
    pub selected_cols: BTreeSet<usize>,
    /// Entire rows selected.
    pub selected_rows: BTreeSet<usize>,
}

/// Direct-manipulation transform handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformHandle {
    /// Top-left resize handle.
    TopLeft,
    /// Top-middle resize handle.
    TopMiddle,
    /// Top-right resize handle.
    TopRight,
    /// Middle-left resize handle.
    MiddleLeft,
    /// Middle-right resize handle.
    MiddleRight,
    /// Bottom-left resize handle.
    BottomLeft,
    /// Bottom-middle resize handle.
    BottomMiddle,
    /// Bottom-right resize handle.
    BottomRight,
    /// Rotation handle.
    Rotation,
}

/// Canvas / presentation scene selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SceneSelection {
    /// Set of selected node / object identifiers.
    pub selected_nodes: BTreeSet<String>,
    /// Primary active node (for single-item inspector binding).
    pub primary_node: Option<String>,
    /// Active direct-manipulation handle if dragged.
    pub active_handle: Option<TransformHandle>,
}

/// Timeline track and clip selection for video and motion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineSelection {
    /// Selected clip/element identifiers.
    pub selected_clips: BTreeSet<String>,
    /// Selected track indices.
    pub selected_tracks: BTreeSet<usize>,
    /// In point (start time in milliseconds).
    pub in_point_ms: Option<u64>,
    /// Out point (end time in milliseconds).
    pub out_point_ms: Option<u64>,
    /// Playhead position in milliseconds.
    pub playhead_ms: u64,
}

/// Layer selection for Photo and Motion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayerSelection {
    /// Selected layer identifiers.
    pub selected_layers: BTreeSet<String>,
    /// Active primary layer identifier.
    pub active_layer: Option<String>,
}

/// Audio region and channel selection for Studio.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioSelection {
    /// Selected audio track indices.
    pub selected_tracks: BTreeSet<usize>,
    /// Selected audio region identifiers.
    pub selected_regions: BTreeSet<String>,
    /// Time selection range in audio samples.
    pub sample_range: Option<(u64, u64)>,
}

/// Batch queue selection for Encode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueSelection {
    /// Selected queue job identifiers.
    pub selected_jobs: BTreeSet<u64>,
}

/// Unified application selection enum.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    /// No selection.
    #[default]
    None,
    /// Text selection in Writer, text boxes, or code editors.
    Text(TextSelection),
    /// Grid selection in Sheets.
    Grid(GridSelection),
    /// 2D scene object selection in Present and canvas tools.
    Scene(SceneSelection),
    /// Timeline selection in Video and Motion.
    Timeline(TimelineSelection),
    /// Layer selection in Photo and Motion.
    Layers(LayerSelection),
    /// Audio track/region selection in Studio.
    Audio(AudioSelection),
    /// Job queue selection in Encode.
    Queue(QueueSelection),
}

impl Selection {
    /// Whether there is any active selection.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Text(t) => t.is_collapsed(),
            Self::Grid(g) => {
                g.ranges.is_empty() && g.selected_cols.is_empty() && g.selected_rows.is_empty()
            }
            Self::Scene(s) => s.selected_nodes.is_empty(),
            Self::Timeline(t) => t.selected_clips.is_empty() && t.selected_tracks.is_empty(),
            Self::Layers(l) => l.selected_layers.is_empty(),
            Self::Audio(a) => a.selected_regions.is_empty() && a.selected_tracks.is_empty(),
            Self::Queue(q) => q.selected_jobs.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_selection_normalization() {
        let sel = TextSelection::range(10, 5);
        assert!(!sel.is_collapsed());
        assert_eq!(sel.normalized_range(), (5, 10));

        let caret = TextSelection::caret(7);
        assert!(caret.is_collapsed());
        assert_eq!(caret.normalized_range(), (7, 7));
    }

    #[test]
    fn grid_range_containment() {
        let range = CellRange::new(CellCoord { col: 1, row: 2 }, CellCoord { col: 4, row: 5 });
        assert!(range.contains(CellCoord { col: 2, row: 3 }));
        assert!(range.contains(CellCoord { col: 1, row: 2 }));
        assert!(!range.contains(CellCoord { col: 5, row: 3 }));
    }

    #[test]
    fn scene_selection_set() {
        let mut scene = SceneSelection::default();
        scene.selected_nodes.insert("shape_1".into());
        scene.selected_nodes.insert("shape_2".into());
        scene.primary_node = Some("shape_2".into());

        let sel = Selection::Scene(scene);
        assert!(!sel.is_empty());
    }
}
