//! Core motion graphics and compositing engine for Loom Motion.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time_secs: f32,
    pub value: f32,
    pub easing: String,
}

/// Vector shape geometry for motion graphic shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VectorShape {
    /// Axis-aligned rectangle with corner radius.
    Rectangle {
        width: f32,
        height: f32,
        corner_radius: f32,
    },
    /// Circle or ellipse.
    Ellipse { radius_x: f32, radius_y: f32 },
    /// Regular polygon with N sides.
    Polygon { sides: u32, radius: f32 },
    /// Star with points, outer radius, and inner radius.
    Star {
        points: u32,
        outer_radius: f32,
        inner_radius: f32,
    },
}

impl VectorShape {
    /// Computes approximate bounding box `(width, height)`.
    pub fn bounding_box(&self) -> (f32, f32) {
        match self {
            VectorShape::Rectangle { width, height, .. } => (*width, *height),
            VectorShape::Ellipse { radius_x, radius_y } => (radius_x * 2.0, radius_y * 2.0),
            VectorShape::Polygon { radius, .. } => (radius * 2.0, radius * 2.0),
            VectorShape::Star { outer_radius, .. } => (outer_radius * 2.0, outer_radius * 2.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionLayer {
    pub id: String,
    pub name: String,
    pub layer_type: String,
    pub start_time: f32,
    pub duration: f32,
    pub position_x_keys: Vec<Keyframe>,
    pub position_y_keys: Vec<Keyframe>,
    pub opacity_keys: Vec<Keyframe>,
    pub scale_keys: Vec<Keyframe>,
    #[serde(default)]
    pub rotation_keys: Vec<Keyframe>,
}

impl MotionLayer {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        layer_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            layer_type: layer_type.into(),
            start_time: 0.0,
            duration: 10.0,
            position_x_keys: Vec::new(),
            position_y_keys: Vec::new(),
            opacity_keys: Vec::new(),
            scale_keys: Vec::new(),
            rotation_keys: Vec::new(),
        }
    }

    pub fn add_keyframe(&mut self, property: &str, time: f32, val: f32) {
        let kf = Keyframe {
            time_secs: time,
            value: val,
            easing: "ease-in-out".to_string(),
        };
        let keys = match property {
            "x" => Some(&mut self.position_x_keys),
            "y" => Some(&mut self.position_y_keys),
            "opacity" => Some(&mut self.opacity_keys),
            "scale" => Some(&mut self.scale_keys),
            "rotation" => Some(&mut self.rotation_keys),
            _ => None,
        };
        if let Some(keys) = keys {
            keys.retain(|existing| (existing.time_secs - time).abs() > f32::EPSILON);
            keys.push(kf);
            keys.sort_by(|left, right| left.time_secs.total_cmp(&right.time_secs));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositionDocument {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub duration_secs: f32,
    pub layers: Vec<MotionLayer>,
    #[serde(default)]
    pub active_layer_index: usize,
}

impl CompositionDocument {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            width: 1920,
            height: 1080,
            frame_rate: 60.0,
            duration_secs: 10.0,
            layers: Vec::new(),
            active_layer_index: 0,
        }
    }

    pub fn add_layer(&mut self, layer: MotionLayer) {
        self.layers.push(layer);
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn select_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.active_layer_index = index;
        true
    }

    pub fn duplicate_layer(&mut self, index: usize) -> Option<usize> {
        if index < self.layers.len() {
            let mut dup = self.layers[index].clone();
            dup.id = format!("{}-copy", dup.id);
            dup.name = format!("{} Copy", dup.name);
            let new_index = index + 1;
            self.layers.insert(new_index, dup);
            self.active_layer_index = new_index;
            Some(new_index)
        } else {
            None
        }
    }
}

pub fn save_motion(doc: &CompositionDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/motion.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Motion,
        id: doc.id.clone(),
        title: doc.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/motion.json".into(),
            mime: MimeType::parse("application/vnd.loom.motion-content")
                .map_err(|e| format!("invalid built-in motion MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_motion(bytes: &[u8]) -> Result<CompositionDocument, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Motion {
        return Err("not a Motion composition".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/motion.json")
        .ok_or_else(|| "missing motion.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

/// Sampled transform for one layer at one composition time.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerSample {
    /// Layer id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// X position in composition pixels.
    pub x: f32,
    /// Y position in composition pixels.
    pub y: f32,
    /// Opacity in `[0, 1]`.
    pub opacity: f32,
    /// Uniform scale, where `1` is original size.
    pub scale: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Whether the layer is active at this time.
    pub visible: bool,
}

/// Deterministic scene state for one composition frame.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionFrame {
    /// Requested time in seconds, clamped to the composition duration.
    pub time_secs: f32,
    /// Zero-based frame index.
    pub frame_index: u64,
    /// Sampled layers in document order.
    pub layers: Vec<LayerSample>,
}

/// Validation issue in a motion document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionIssue {
    /// Layer id, when the issue belongs to a layer.
    pub layer_id: Option<String>,
    /// Human-readable description.
    pub message: String,
}

/// Render range for a composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    /// Inclusive first frame.
    pub start: u64,
    /// Exclusive end frame.
    pub end: u64,
}

impl FrameRange {
    /// Number of frames in the range.
    pub fn len(self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// Whether no frames are selected.
    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

impl MotionLayer {
    /// Removes a keyframe at the exact timestamp from a property track.
    pub fn remove_keyframe(&mut self, property: &str, time: f32) -> bool {
        let keys = match property {
            "x" => Some(&mut self.position_x_keys),
            "y" => Some(&mut self.position_y_keys),
            "opacity" => Some(&mut self.opacity_keys),
            "scale" => Some(&mut self.scale_keys),
            "rotation" => Some(&mut self.rotation_keys),
            _ => None,
        };
        let Some(keys) = keys else {
            return false;
        };
        let before = keys.len();
        keys.retain(|key| (key.time_secs - time).abs() > f32::EPSILON);
        before != keys.len()
    }

    /// Samples all animated properties at absolute composition time.
    pub fn sample(&self, time_secs: f32) -> LayerSample {
        let local_time = time_secs - self.start_time;
        let visible = local_time >= 0.0 && local_time <= self.duration.max(0.0);
        LayerSample {
            id: self.id.clone(),
            name: self.name.clone(),
            x: sample_keys(&self.position_x_keys, local_time, 0.0),
            y: sample_keys(&self.position_y_keys, local_time, 0.0),
            opacity: sample_keys(&self.opacity_keys, local_time, 1.0).clamp(0.0, 1.0),
            scale: sample_keys(&self.scale_keys, local_time, 1.0).max(0.0),
            rotation: sample_keys(&self.rotation_keys, local_time, 0.0),
            visible,
        }
    }
}

impl CompositionDocument {
    /// Number of output frames using round-to-nearest duration semantics.
    pub fn duration_frames(&self) -> u64 {
        if !self.duration_secs.is_finite() || !self.frame_rate.is_finite() {
            return 0;
        }
        (self.duration_secs.max(0.0) * self.frame_rate.max(0.0)).round() as u64
    }

    /// Converts a frame index to seconds.
    pub fn frame_time(&self, frame_index: u64) -> f32 {
        if self.frame_rate <= 0.0 || !self.frame_rate.is_finite() {
            0.0
        } else {
            frame_index as f32 / self.frame_rate
        }
    }

    /// Samples the entire composition at one time.
    pub fn frame_at(&self, time_secs: f32) -> CompositionFrame {
        let time_secs = time_secs.max(0.0).min(self.duration_secs.max(0.0));
        let frame_index = if self.frame_rate > 0.0 && self.frame_rate.is_finite() {
            (time_secs * self.frame_rate).round() as u64
        } else {
            0
        };
        CompositionFrame {
            time_secs,
            frame_index,
            layers: self
                .layers
                .iter()
                .map(|layer| layer.sample(time_secs))
                .collect(),
        }
    }

    /// Samples a frame by index.
    pub fn frame(&self, frame_index: u64) -> CompositionFrame {
        self.frame_at(self.frame_time(frame_index))
    }

    /// Reorders one layer.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return false;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        self.active_layer_index = to;
        true
    }

    /// Removes a layer while preserving a valid active index.
    pub fn remove_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.layers.remove(index);
        self.active_layer_index = self
            .active_layer_index
            .min(self.layers.len().saturating_sub(1));
        true
    }

    /// Counts total keyframes across all properties of all layers in the composition.
    pub fn total_keyframes(&self) -> usize {
        self.layers
            .iter()
            .map(|l| {
                l.position_x_keys.len()
                    + l.position_y_keys.len()
                    + l.opacity_keys.len()
                    + l.scale_keys.len()
                    + l.rotation_keys.len()
            })
            .sum()
    }

    /// Validates timing, ids, dimensions, and keyframe values.
    pub fn validate(&self) -> Vec<MotionIssue> {
        let mut issues = Vec::new();
        if self.width == 0 || self.height == 0 {
            issues.push(MotionIssue {
                layer_id: None,
                message: "composition dimensions must be non-zero".into(),
            });
        }
        if !self.frame_rate.is_finite() || self.frame_rate <= 0.0 {
            issues.push(MotionIssue {
                layer_id: None,
                message: "frame rate must be finite and positive".into(),
            });
        }
        if !self.duration_secs.is_finite() || self.duration_secs <= 0.0 {
            issues.push(MotionIssue {
                layer_id: None,
                message: "duration must be finite and positive".into(),
            });
        }
        if !self.layers.is_empty() && self.active_layer_index >= self.layers.len() {
            issues.push(MotionIssue {
                layer_id: None,
                message: "active layer index is out of bounds".into(),
            });
        }
        let mut ids = std::collections::HashSet::new();
        for layer in &self.layers {
            if !ids.insert(&layer.id) {
                issues.push(MotionIssue {
                    layer_id: Some(layer.id.clone()),
                    message: "duplicate layer id".into(),
                });
            }
            if !layer.start_time.is_finite()
                || !layer.duration.is_finite()
                || layer.start_time < 0.0
                || layer.duration < 0.0
            {
                issues.push(MotionIssue {
                    layer_id: Some(layer.id.clone()),
                    message: "invalid layer timing".into(),
                });
            }
            for (property, keys) in [
                ("x", &layer.position_x_keys),
                ("y", &layer.position_y_keys),
                ("opacity", &layer.opacity_keys),
                ("scale", &layer.scale_keys),
                ("rotation", &layer.rotation_keys),
            ] {
                if keys
                    .iter()
                    .any(|key| !key.time_secs.is_finite() || !key.value.is_finite())
                {
                    issues.push(MotionIssue {
                        layer_id: Some(layer.id.clone()),
                        message: format!("{property} track contains a non-finite keyframe"),
                    });
                }
                if keys
                    .windows(2)
                    .any(|pair| pair[0].time_secs >= pair[1].time_secs)
                {
                    issues.push(MotionIssue {
                        layer_id: Some(layer.id.clone()),
                        message: format!("{property} keyframes are not strictly ordered"),
                    });
                }
            }
        }
        issues
    }

    /// Resolves a bounded render range.
    pub fn render_range(&self, start: Option<u64>, end: Option<u64>) -> FrameRange {
        let duration = self.duration_frames();
        let start = start.unwrap_or(0).min(duration);
        let end = end.unwrap_or(duration).min(duration).max(start);
        FrameRange { start, end }
    }
}

fn sample_keys(keys: &[Keyframe], time: f32, default: f32) -> f32 {
    let Some(first) = keys.first() else {
        return default;
    };
    if time <= first.time_secs {
        return first.value;
    }
    let Some(last) = keys.last() else {
        return default;
    };
    if time >= last.time_secs {
        return last.value;
    }
    for pair in keys.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        if time >= left.time_secs && time <= right.time_secs {
            let duration = right.time_secs - left.time_secs;
            if duration <= f32::EPSILON {
                return right.value;
            }
            let progress = ((time - left.time_secs) / duration).clamp(0.0, 1.0);
            let eased = easing_progress(&left.easing, progress);
            return left.value + (right.value - left.value) * eased;
        }
    }
    last.value
}

fn easing_progress(name: &str, progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    match name.trim().to_ascii_lowercase().as_str() {
        "linear" => t,
        "ease-in" | "quad-in" => t * t,
        "ease-out" | "quad-out" => 1.0 - (1.0 - t) * (1.0 - t),
        "cubic-in" => t * t * t,
        "cubic-out" => 1.0 - (1.0 - t).powi(3),
        "expo-in" => {
            if t == 0.0 {
                0.0
            } else {
                2.0_f32.powf(10.0 * (t - 1.0))
            }
        }
        "expo-out" => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        }
        "hold" | "step" => 0.0,
        _ => {
            // Smoothstep: deterministic ease-in-out without an external spline crate.
            t * t * (3.0 - 2.0 * t)
        }
    }
}

/// Evaluates a 1D cubic Bézier curve: B(t) = (1-t)^3*p0 + 3(1-t)^2*t*p1 + 3(1-t)*t^2*p2 + t^3*p3.
pub fn cubic_bezier_1d(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let one_minus_t = 1.0 - t;
    let b0 = one_minus_t.powi(3) * p0;
    let b1 = 3.0 * one_minus_t.powi(2) * t * p1;
    let b2 = 3.0 * one_minus_t * t.powi(2) * p2;
    let b3 = t.powi(3) * p3;
    b0 + b1 + b2 + b3
}

/// Evaluates a 2D cubic Bézier spatial path position: (x(t), y(t)).
pub fn cubic_bezier_2d(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    (
        cubic_bezier_1d(p0.0, p1.0, p2.0, p3.0, t),
        cubic_bezier_1d(p0.1, p1.1, p2.1, p3.1, t),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_creation() {
        let doc = CompositionDocument::new("comp-1", "Logo Intro Animation");
        assert_eq!(doc.frame_rate, 60.0);
        assert!(doc.is_empty());
    }

    #[test]
    fn test_motion_keyframes() {
        let mut layer = MotionLayer::new("l1", "Shape", "VectorShape");
        layer.add_keyframe("x", 0.0, 100.0);
        layer.add_keyframe("x", 2.0, 500.0);
        assert_eq!(layer.position_x_keys.len(), 2);
    }

    #[test]
    fn test_select_layer_rejects_invalid_index() {
        let mut doc = CompositionDocument::new("comp-1", "Logo Intro Animation");
        doc.add_layer(MotionLayer::new("l2", "Background", "VectorShape"));
        assert!(doc.select_layer(0));
        assert!(!doc.select_layer(1));
        assert_eq!(doc.active_layer_index, 0);
    }

    #[test]
    fn test_load_legacy_motion_without_active_layer_index() {
        let mut expected = CompositionDocument::new("comp-legacy", "Legacy Title");
        expected.add_layer(MotionLayer::new("l2", "Background Ribbon", "VectorShape"));

        let mut legacy_payload = serde_json::to_value(&expected).expect("serialize payload");
        legacy_payload
            .as_object_mut()
            .expect("document payload must be an object")
            .remove("active_layer_index");
        let content = serde_json::to_vec_pretty(&legacy_payload).expect("serialize legacy payload");

        let mut archive = PackageArchive::new();
        archive
            .add("content/motion.json", content.clone())
            .expect("add motion content");
        let manifest = Manifest {
            schema: SchemaVersion::CURRENT,
            kind: PackageKind::Motion,
            id: expected.id.clone(),
            title: expected.name.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            entries: vec![ManifestEntry {
                path: "content/motion.json".into(),
                mime: MimeType::parse("application/vnd.loom.motion-content").unwrap(),
                size: content.len() as u64,
                sha256: Checksum::from_bytes(zip::sha256(&content)),
            }],
        };
        archive
            .add("manifest.json", pkg_json::write(&manifest).into_bytes())
            .expect("add manifest");

        let loaded = load_motion(&archive.to_bytes().expect("serialize archive"))
            .expect("legacy package should load");

        assert_eq!(loaded.active_layer_index, 0);
        assert_eq!(loaded.id, expected.id);
        assert_eq!(loaded.name, expected.name);
        assert_eq!(
            serde_json::to_value(&loaded.layers).expect("serialize loaded layers"),
            serde_json::to_value(&expected.layers).expect("serialize expected layers")
        );
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = CompositionDocument::new("comp-test", "Title Lower Third");
        doc.add_layer(MotionLayer::new("l2", "Background Ribbon", "VectorShape"));
        let bytes = save_motion(&doc).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Motion);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_motion(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Title Lower Third");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded, doc);
        let loaded_again = load_motion(&bytes).expect("second load failed");
        assert_eq!(loaded_again, loaded);
    }

    #[test]
    fn malformed_motion_package_is_rejected() {
        let error = load_motion(b"not a Loom package").expect_err("malformed package loaded");
        assert!(!error.trim().is_empty());
    }

    #[test]
    fn unsupported_motion_schema_is_rejected() {
        let document = CompositionDocument::new("future-motion", "Future Motion");
        let content = serde_json::to_vec_pretty(&document).expect("serialize content");
        let mut archive = PackageArchive::new();
        archive
            .add("content/motion.json", content.clone())
            .expect("add content");
        let manifest = Manifest {
            schema: SchemaVersion::new(1, 0, 0),
            kind: PackageKind::Motion,
            id: document.id.clone(),
            title: document.name.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            entries: vec![ManifestEntry {
                path: "content/motion.json".into(),
                mime: MimeType::parse("application/vnd.loom.motion-content").expect("valid MIME"),
                size: content.len() as u64,
                sha256: Checksum::from_bytes(zip::sha256(&content)),
            }],
        };
        archive
            .add("manifest.json", pkg_json::write(&manifest).into_bytes())
            .expect("add manifest");

        let error = load_motion(&archive.to_bytes().expect("serialize package"))
            .expect_err("future schema loaded");
        assert!(error.contains("unsupported schema version"), "{error}");
    }

    #[test]
    fn keyframes_are_sorted_replaced_and_sampled() {
        let mut layer = MotionLayer::new("layer", "Layer", "Text");
        layer.add_keyframe("x", 2.0, 200.0);
        layer.add_keyframe("x", 0.0, 0.0);
        layer.add_keyframe("x", 2.0, 300.0);
        assert_eq!(layer.position_x_keys.len(), 2);
        assert_eq!(layer.position_x_keys[0].time_secs, 0.0);
        assert!((layer.sample(1.0).x - 150.0).abs() < 0.001);
        assert!(layer.remove_keyframe("x", 2.0));
        assert_eq!(layer.sample(10.0).x, 0.0);
    }

    #[test]
    fn composition_frame_and_render_range_are_bounded() {
        let doc = CompositionDocument::new("comp-frame", "Frame Test");
        assert_eq!(doc.duration_frames(), 600);
        let frame = doc.frame(60);
        assert!((frame.time_secs - 1.0).abs() < 0.001);
        assert_eq!(frame.frame_index, 60);
        assert_eq!(
            doc.render_range(Some(590), Some(900)),
            FrameRange {
                start: 590,
                end: 600
            }
        );
    }

    #[test]
    fn motion_validation_reports_bad_documents() {
        let mut doc = CompositionDocument::new("comp-invalid", "Invalid");
        doc.frame_rate = 0.0;
        doc.add_layer(MotionLayer::new("layer-invalid", "Invalid Layer", "Text"));
        doc.layers[0].position_x_keys = vec![
            Keyframe {
                time_secs: 1.0,
                value: 0.0,
                easing: "linear".into(),
            },
            Keyframe {
                time_secs: 0.5,
                value: 1.0,
                easing: "linear".into(),
            },
        ];
        let issues = doc.validate();
        assert!(issues
            .iter()
            .any(|issue| issue.message.contains("frame rate")));
        assert!(issues
            .iter()
            .any(|issue| issue.message.contains("not strictly ordered")));
    }

    #[test]
    fn layer_duplicate_move_remove_operations() {
        let mut doc = CompositionDocument::new("comp-layers", "Layers Test");
        doc.add_layer(MotionLayer::new("l1", "Layer 1", "Shape"));
        doc.add_layer(MotionLayer::new("l2", "Layer 2", "Text"));
        assert_eq!(doc.len(), 2);

        // Duplicate layer 0
        let dup_idx = doc.duplicate_layer(0).unwrap();
        assert_eq!(dup_idx, 1);
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.layers[1].name, "Layer 1 Copy");

        // Move layer 0 to 2
        assert!(doc.move_layer(0, 2));
        assert_eq!(doc.active_layer_index, 2);

        // Remove layer 1
        assert!(doc.remove_layer(1));
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn vector_shape_bounding_box_calculations() {
        let rect = VectorShape::Rectangle {
            width: 200.0,
            height: 100.0,
            corner_radius: 8.0,
        };
        assert_eq!(rect.bounding_box(), (200.0, 100.0));

        let ellipse = VectorShape::Ellipse {
            radius_x: 50.0,
            radius_y: 30.0,
        };
        assert_eq!(ellipse.bounding_box(), (100.0, 60.0));
    }

    #[test]
    fn easing_progress_curves_evaluate_expected_values() {
        assert_eq!(easing_progress("linear", 0.5), 0.5);
        assert_eq!(easing_progress("ease-in", 0.5), 0.25);
        assert_eq!(easing_progress("cubic-in", 0.5), 0.125);
        assert_eq!(easing_progress("cubic-out", 0.5), 0.875);
        assert_eq!(easing_progress("hold", 0.5), 0.0);
    }

    #[test]
    fn total_keyframes_counts_all_layer_properties() {
        let mut doc = CompositionDocument::new("comp-k", "Keyframes Count");
        let mut l1 = MotionLayer::new("l1", "Layer 1", "VectorShape");
        l1.add_keyframe("x", 0.0, 10.0);
        l1.add_keyframe("x", 1.0, 50.0);
        l1.add_keyframe("y", 0.0, 0.0);
        doc.add_layer(l1);

        assert_eq!(doc.total_keyframes(), 3);
    }

    #[test]
    fn cubic_bezier_curves_and_paths() {
        // Start: t=0 -> p0
        assert_eq!(cubic_bezier_1d(0.0, 10.0, 20.0, 30.0, 0.0), 0.0);
        // End: t=1 -> p3
        assert_eq!(cubic_bezier_1d(0.0, 10.0, 20.0, 30.0, 1.0), 30.0);
        // Midpoint: t=0.5 of symmetric curve [0, 100, 100, 0] -> 75.0
        assert_eq!(cubic_bezier_1d(0.0, 100.0, 100.0, 0.0, 0.5), 75.0);

        // 2D Path
        let p_start = (0.0, 0.0);
        let p_end = (100.0, 200.0);
        let (x, y) = cubic_bezier_2d(p_start, (30.0, 50.0), (70.0, 150.0), p_end, 0.5);
        assert_eq!(x, 50.0);
        assert_eq!(y, 100.0);
    }
}
