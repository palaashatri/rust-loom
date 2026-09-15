//! Worksheet objects anchored to cells.
//!
//! Objects are deliberately small, serializable model values.  The desktop
//! app decides how to render them, while the sheet owns their position and
//! undoable/persisted identity.

use std::path::Path;

use crate::style::FillColor;
use crate::CellRef;

/// The two non-cell object kinds currently supported by Sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetObjectKind {
    /// A labeled, filled vector rectangle.
    Shape,
    /// A local raster/vector image referenced by path.
    Image,
}

impl SheetObjectKind {
    /// Stable lower-case identifier used by persistence and the UI bridge.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Image => "image",
        }
    }

    /// Parse a persisted/UI identifier. Unknown objects are rejected by the
    /// persistence layer rather than silently becoming shapes.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shape" => Some(Self::Shape),
            "image" => Some(Self::Image),
            _ => None,
        }
    }
}

/// A drawing or image placed over a worksheet at a cell anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetObject {
    /// Object kind.
    pub kind: SheetObjectKind,
    /// Top-left worksheet cell used as the placement anchor.
    pub anchor: CellRef,
    /// Rendered width in logical pixels.
    pub width: u32,
    /// Rendered height in logical pixels.
    pub height: u32,
    /// Accessible/display label. For images this is also the fallback label.
    pub label: String,
    /// Local source path for images; empty for shapes.
    pub path: String,
    /// Embedded source bytes loaded from a `.loomtable` asset entry.
    pub embedded: Option<Vec<u8>>,
    /// Package-relative asset entry containing `embedded`, when persisted.
    pub asset: Option<String>,
    /// Shape fill. Images ignore this value.
    pub fill: FillColor,
}

impl SheetObject {
    /// Create a default filled shape at `anchor`.
    pub fn shape(anchor: CellRef, label: impl Into<String>) -> Self {
        Self {
            kind: SheetObjectKind::Shape,
            anchor,
            width: 240,
            height: 112,
            label: label.into(),
            path: String::new(),
            embedded: None,
            asset: None,
            fill: FillColor::Blue,
        }
    }

    /// Create an image attachment at `anchor` after validating its path.
    pub fn image(anchor: CellRef, path: impl Into<String>) -> Result<Self, String> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err("image path must not be empty".to_string());
        }
        let label = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Image")
            .to_string();
        Ok(Self {
            kind: SheetObjectKind::Image,
            anchor,
            width: 320,
            height: 200,
            label,
            path,
            embedded: None,
            asset: None,
            fill: FillColor::None,
        })
    }

    /// Shift the anchor after a row deletion, or return `None` when the
    /// object was anchored on the deleted row.
    pub fn after_deleted_row(&self, deleted_row: u32) -> Option<Self> {
        if self.anchor.row == deleted_row {
            None
        } else {
            let mut shifted = self.clone();
            if self.anchor.row > deleted_row {
                shifted.anchor.row -= 1;
            }
            Some(shifted)
        }
    }

    /// Shift the anchor after a column deletion, or return `None` when the
    /// object was anchored on the deleted column.
    pub fn after_deleted_col(&self, deleted_col: u32) -> Option<Self> {
        if self.anchor.col == deleted_col {
            None
        } else {
            let mut shifted = self.clone();
            if self.anchor.col > deleted_col {
                shifted.anchor.col -= 1;
            }
            Some(shifted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_objects_require_a_path_and_derive_an_accessible_label() {
        let image = SheetObject::image(CellRef { row: 2, col: 3 }, "/tmp/hero.png")
            .expect("image path is valid");
        assert_eq!(image.kind, SheetObjectKind::Image);
        assert_eq!(image.label, "hero.png");
        assert_eq!(image.path, "/tmp/hero.png");
        assert!(SheetObject::image(CellRef { row: 0, col: 0 }, " ").is_err());
    }

    #[test]
    fn image_objects_can_carry_an_embedded_payload() {
        let mut image = SheetObject::image(CellRef { row: 0, col: 0 }, "hero.png")
            .expect("image path is valid");
        image.embedded = Some(vec![0x89, b'P', b'N', b'G']);
        assert_eq!(
            image.embedded.as_deref(),
            Some(&[0x89, b'P', b'N', b'G'][..])
        );
    }

    #[test]
    fn object_anchors_follow_row_and_column_deletions() {
        let shape = SheetObject::shape(CellRef { row: 3, col: 4 }, "Callout");
        assert_eq!(
            shape.after_deleted_row(1).unwrap().anchor,
            CellRef { row: 2, col: 4 }
        );
        assert_eq!(
            shape.after_deleted_col(1).unwrap().anchor,
            CellRef { row: 3, col: 3 }
        );
        assert!(shape.after_deleted_row(3).is_none());
        assert!(shape.after_deleted_col(4).is_none());
    }
}
