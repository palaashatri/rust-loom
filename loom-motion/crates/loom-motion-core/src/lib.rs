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

/// Standard composition resolution preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositionPreset {
    Fhd1080p,
    Uhd4k,
    Square1080,
    Vertical1080x1920,
    Cinema4k,
}

impl CompositionPreset {
    /// Returns `(width, height)` in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            CompositionPreset::Fhd1080p => (1920, 1080),
            CompositionPreset::Uhd4k => (3840, 2160),
            CompositionPreset::Square1080 => (1080, 1080),
            CompositionPreset::Vertical1080x1920 => (1080, 1920),
            CompositionPreset::Cinema4k => (4096, 2160),
        }
    }

    /// Aspect ratio as `(num, den)`.
    pub fn aspect_ratio(&self) -> (u32, u32) {
        match self {
            CompositionPreset::Fhd1080p | CompositionPreset::Uhd4k => (16, 9),
            CompositionPreset::Square1080 => (1, 1),
            CompositionPreset::Vertical1080x1920 => (9, 16),
            CompositionPreset::Cinema4k => (256, 135),
        }
    }
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

/// Snaps a timeline time to nearby keyframe targets or frame boundaries within a tolerance threshold.
pub fn snap_timeline_time(
    time_secs: f32,
    fps: f32,
    snap_targets: &[f32],
    tolerance_secs: f32,
) -> f32 {
    let mut best_snap = time_secs;
    let mut min_diff = tolerance_secs;
    let mut found_target = false;

    for &target in snap_targets {
        let diff = (time_secs - target).abs();
        if diff <= min_diff {
            min_diff = diff;
            best_snap = target;
            found_target = true;
        }
    }

    if !found_target && fps > 0.0 {
        let frame_duration = 1.0 / fps;
        let nearest_frame_time = (time_secs * fps).round() * frame_duration;
        let frame_diff = (time_secs - nearest_frame_time).abs();
        if frame_diff < tolerance_secs {
            best_snap = nearest_frame_time;
        }
    }

    best_snap
}

/// Interpolates linearly between two RGBA colors `[r, g, b, a]` normalized in `[0.0, 1.0]`.
pub fn interpolate_color_rgba(c1: [f32; 4], c2: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        c1[0] + (c2[0] - c1[0]) * t,
        c1[1] + (c2[1] - c1[1]) * t,
        c1[2] + (c2[2] - c1[2]) * t,
        c1[3] + (c2[3] - c1[3]) * t,
    ]
}

/// Applies layer opacity multiplier to an RGBA color.
pub fn apply_layer_opacity(color: [f32; 4], opacity: f32) -> [f32; 4] {
    let alpha = (color[3] * opacity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    [color[0], color[1], color[2], alpha]
}

/// Keyframe interpolation curve modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InterpolationMode {
    #[default]
    Linear,
    Bezier,
    Hold,
}

/// Graph editor tangent handles for Bezier keyframe curve interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TangentHandle {
    pub in_weight: f32,
    pub in_angle_deg: f32,
    pub out_weight: f32,
    pub out_angle_deg: f32,
}

impl Default for TangentHandle {
    fn default() -> Self {
        Self {
            in_weight: 0.33,
            in_angle_deg: 0.0,
            out_weight: 0.33,
            out_angle_deg: 0.0,
        }
    }
}

/// Evaluates a normalized time parameter `t` in `[0.0, 1.0]` between two values based on interpolation mode.
pub fn evaluate_keyframe_segment(v1: f32, v2: f32, t: f32, mode: InterpolationMode) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match mode {
        InterpolationMode::Linear => v1 + (v2 - v1) * t,
        InterpolationMode::Hold => {
            if t >= 1.0 {
                v2
            } else {
                v1
            }
        }
        InterpolationMode::Bezier => {
            // Smooth ease-in-out cubic bezier approximation: 3t^2 - 2t^3
            let smooth_t = t * t * (3.0 - 2.0 * t);
            v1 + (v2 - v1) * smooth_t
        }
    }
}

/// Interpolates between two sets of polygon vertices for shape morphing animations.
pub fn interpolate_polygon_points(
    p1: &[[f32; 2]],
    p2: &[[f32; 2]],
    t: f32,
) -> Result<Vec<[f32; 2]>, String> {
    if p1.len() != p2.len() {
        return Err("polygon point sets must have matching vertex counts".into());
    }
    if p1.is_empty() {
        return Ok(Vec::new());
    }
    let t = t.clamp(0.0, 1.0);
    let mut points = Vec::with_capacity(p1.len());

    for (v1, v2) in p1.iter().zip(p2.iter()) {
        let x = v1[0] + (v2[0] - v1[0]) * t;
        let y = v1[1] + (v2[1] - v1[1]) * t;
        points.push([x, y]);
    }

    Ok(points)
}

/// Realtime playback clock and transport timebase for motion compositions.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionClock {
    pub fps: f64,
    pub current_frame: u64,
    pub in_frame: u64,
    pub out_frame: u64,
    pub is_playing: bool,
    pub loop_playback: bool,
}

impl CompositionClock {
    pub fn new(fps: f64, out_frame: u64) -> Self {
        Self {
            fps: if fps > 0.0 { fps } else { 60.0 },
            current_frame: 0,
            in_frame: 0,
            out_frame: out_frame.max(1),
            is_playing: false,
            loop_playback: true,
        }
    }

    /// Converts frame number to timestamp in seconds.
    pub fn frame_to_seconds(&self, frame: u64) -> f64 {
        frame as f64 / self.fps
    }

    /// Converts seconds to closest frame number.
    pub fn seconds_to_frame(&self, seconds: f64) -> u64 {
        (seconds.max(0.0) * self.fps).round() as u64
    }

    /// Steps forward by 1 frame, respecting in/out points and loop mode.
    pub fn step_forward(&mut self) -> u64 {
        if self.current_frame >= self.out_frame {
            if self.loop_playback {
                self.current_frame = self.in_frame;
            }
        } else {
            self.current_frame += 1;
        }
        self.current_frame
    }

    /// Steps backward by 1 frame, bounded by in point.
    pub fn step_backward(&mut self) -> u64 {
        if self.current_frame > self.in_frame {
            self.current_frame -= 1;
        }
        self.current_frame
    }

    /// Seeks to a specific frame number clamped within composition bounds.
    pub fn seek_frame(&mut self, frame: u64) {
        self.current_frame = frame.clamp(self.in_frame, self.out_frame);
    }

    /// Seeks to a timestamp in seconds.
    pub fn seek_seconds(&mut self, seconds: f64) {
        let frame = self.seconds_to_frame(seconds);
        self.seek_frame(frame);
    }
}

/// Generates a smooth, curved spatial motion path through control waypoints using Catmull-Rom splines.
pub fn smooth_spatial_path(
    waypoints: &[[f64; 2]],
    subdivisions: usize,
) -> Result<Vec<[f64; 2]>, String> {
    if waypoints.len() < 2 {
        return Err("at least 2 waypoints are required for spatial path".into());
    }
    if subdivisions == 0 {
        return Ok(waypoints.to_vec());
    }

    let mut result = Vec::new();
    let n = waypoints.len();

    for i in 0..n - 1 {
        let p0 = if i == 0 {
            waypoints[0]
        } else {
            waypoints[i - 1]
        };
        let p1 = waypoints[i];
        let p2 = waypoints[i + 1];
        let p3 = if i + 2 < n {
            waypoints[i + 2]
        } else {
            waypoints[i + 1]
        };

        for step in 0..subdivisions {
            let t = step as f64 / subdivisions as f64;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = 0.5
                * ((2.0 * p1[0])
                    + (-p0[0] + p2[0]) * t
                    + (2.0 * p0[0] - 5.0 * p1[0] + 4.0 * p2[0] - p3[0]) * t2
                    + (-p0[0] + 3.0 * p1[0] - 3.0 * p2[0] + p3[0]) * t3);

            let y = 0.5
                * ((2.0 * p1[1])
                    + (-p0[1] + p2[1]) * t
                    + (2.0 * p0[1] - 5.0 * p1[1] + 4.0 * p2[1] - p3[1]) * t2
                    + (-p0[1] + 3.0 * p1[1] - 3.0 * p2[1] + p3[1]) * t3);

            result.push([x, y]);
        }
    }

    // Add final waypoint
    result.push(*waypoints.last().unwrap());
    Ok(result)
}

/// Motion blur shutter angle and sub-frame temporal sampling configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ShutterConfig {
    /// Shutter opening angle in degrees [0.0, 360.0]. Standard cinema is 180.0°.
    pub shutter_angle_deg: f32,
    /// Shutter phase angle in degrees [-180.0, 180.0]. Default is -90.0° (centered).
    pub shutter_phase_deg: f32,
    /// Number of sub-frame temporal samples evaluated for motion blur accumulation.
    pub samples_per_frame: usize,
    /// Enable or disable motion blur simulation for this composition.
    pub enabled: bool,
}

impl Default for ShutterConfig {
    fn default() -> Self {
        Self {
            shutter_angle_deg: 180.0,
            shutter_phase_deg: -90.0,
            samples_per_frame: 16,
            enabled: false,
        }
    }
}

impl ShutterConfig {
    /// Calculates the effective camera sensor exposure duration in seconds.
    pub fn exposure_duration_seconds(&self, fps: f64) -> f64 {
        if fps <= 0.0 {
            return 0.0;
        }
        (self.shutter_angle_deg as f64 / 360.0) / fps
    }

    /// Generates temporal sub-frame sample offsets in seconds relative to current frame time.
    pub fn sample_offsets(&self, fps: f64) -> Vec<f64> {
        let n = self.samples_per_frame.max(1);
        let exposure = self.exposure_duration_seconds(fps);
        let phase_offset = (self.shutter_phase_deg as f64 / 360.0) / fps;

        let mut offsets = Vec::with_capacity(n);
        if n == 1 {
            offsets.push(phase_offset);
            return offsets;
        }

        for i in 0..n {
            let t = i as f64 / (n - 1) as f64;
            offsets.push(phase_offset + (t - 0.5) * exposure);
        }
        offsets
    }
}

/// Generated audio envelope amplitude keyframe for driving visual motion and effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAmplitudeKeyframe {
    pub time_seconds: f64,
    pub amplitude: f32,
}

/// Extracts frame-accurate audio amplitude keyframes from PCM samples to automate motion properties.
pub fn generate_audio_driven_keyframes(
    samples: &[f32],
    sample_rate: u32,
    target_fps: f64,
    smoothing_factor: f32,
) -> Vec<AudioAmplitudeKeyframe> {
    if samples.is_empty() || sample_rate == 0 || target_fps <= 0.0 {
        return Vec::new();
    }

    let samples_per_frame = (sample_rate as f64 / target_fps).round().max(1.0) as usize;
    let num_frames = samples.len() / samples_per_frame;
    let mut keyframes = Vec::with_capacity(num_frames);

    let smooth = smoothing_factor.clamp(0.0, 0.95);
    let mut current_envelope = 0.0f32;

    for frame_idx in 0..num_frames {
        let start = frame_idx * samples_per_frame;
        let end = (start + samples_per_frame).min(samples.len());
        let frame_samples = &samples[start..end];

        let sum_sq: f32 = frame_samples.iter().map(|s| s * s).sum();
        let rms = (sum_sq / frame_samples.len() as f32).sqrt();

        // One-pole low-pass envelope smoothing
        current_envelope = current_envelope * smooth + rms * (1.0 - smooth);

        keyframes.push(AudioAmplitudeKeyframe {
            time_seconds: frame_idx as f64 / target_fps,
            amplitude: current_envelope.clamp(0.0, 1.0),
        });
    }

    keyframes
}

/// Calculates layer auto-orientation rotation angle in degrees from spatial velocity vector (dx, dy).
pub fn auto_orient_along_path(tangent_x: f32, tangent_y: f32) -> f32 {
    let rad = tangent_y.atan2(tangent_x);
    let deg = rad.to_degrees();
    if deg < 0.0 {
        deg + 360.0
    } else {
        deg
    }
}

/// Generates rotation heading angles in degrees for each point along a discrete spatial motion path.
pub fn calculate_path_headings(path: &[[f32; 2]]) -> Vec<f32> {
    if path.is_empty() {
        return Vec::new();
    }
    if path.len() == 1 {
        return vec![0.0];
    }

    let mut headings = Vec::with_capacity(path.len());
    for i in 0..path.len() {
        let (dx, dy) = if i + 1 < path.len() {
            (path[i + 1][0] - path[i][0], path[i + 1][1] - path[i][1])
        } else {
            (path[i][0] - path[i - 1][0], path[i][1] - path[i - 1][1])
        };
        headings.push(auto_orient_along_path(dx, dy));
    }
    headings
}

/// Procedural motion turbulence and organic wiggle generator parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WiggleConfig {
    /// Jitter oscillation rate in hertz (cycles per second).
    pub frequency_hz: f32,
    /// Maximum displacement magnitude.
    pub amplitude: f32,
    /// Detail harmonic octaves (1 to 4).
    pub octaves: u32,
    /// Seed phase offset.
    pub seed: u64,
}

impl Default for WiggleConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 2.0,
            amplitude: 50.0,
            octaves: 2,
            seed: 42,
        }
    }
}

/// Generates 1D procedural harmonic organic jitter displacement for keyframed parameters.
pub fn wiggle_1d(time_seconds: f64, config: &WiggleConfig) -> f32 {
    let mut total = 0.0f32;
    let mut freq = config.frequency_hz;
    let mut amp = config.amplitude;
    let seed_phase = (config.seed as f32 * 1.6180339) % (2.0 * std::f32::consts::PI);

    let octaves = config.octaves.clamp(1, 4);
    for octave in 0..octaves {
        let t = time_seconds as f32 * freq * 2.0 * std::f32::consts::PI
            + seed_phase
            + (octave as f32 * 1.7);
        total += t.sin() * amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    total
}

/// Generates 2D procedural organic position jitter displacement (dx, dy).
pub fn wiggle_2d(
    time_seconds: f64,
    config_x: &WiggleConfig,
    config_y: &WiggleConfig,
) -> (f32, f32) {
    (
        wiggle_1d(time_seconds, config_x),
        wiggle_1d(time_seconds, config_y),
    )
}

/// Spring overshoot and inertial bounce damping parameters for keyframe arrivals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InertialBounceConfig {
    /// Initial overshoot amplitude factor (e.g. 0.1 for 10% bounce).
    pub amplitude: f32,
    /// Oscillation frequency in hertz.
    pub frequency_hz: f32,
    /// Damping decay rate per second.
    pub decay: f32,
}

impl Default for InertialBounceConfig {
    fn default() -> Self {
        Self {
            amplitude: 0.1,
            frequency_hz: 3.5,
            decay: 6.0,
        }
    }
}

/// Computes the damped harmonic spring overshoot offset following a keyframe landing.
pub fn calculate_inertial_bounce(
    time_past_keyframe: f64,
    delta_value: f32,
    config: &InertialBounceConfig,
) -> f32 {
    if time_past_keyframe <= 0.0 || delta_value.abs() < 1e-5 {
        return 0.0;
    }
    let t = time_past_keyframe as f32;
    let decay_factor = (-config.decay * t).exp();
    let oscillation = (2.0 * std::f32::consts::PI * config.frequency_hz * t).sin();
    delta_value * config.amplitude * oscillation * decay_factor
}

/// A single simulated particle.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// Remaining lifetime in seconds.
    pub life: f32,
    /// Initial lifetime in seconds.
    pub max_life: f32,
}

/// Emission and force configuration for a particle emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleEmitterConfig {
    /// Particles emitted per second.
    pub emission_rate: f32,
    /// Emission direction in degrees (0 = +x axis, 90 = up/-y screen space).
    pub direction_degrees: f32,
    /// Half-angle cone spread in degrees.
    pub spread_degrees: f32,
    /// Initial speed in px/s.
    pub speed: f32,
    /// Downward acceleration px/s^2 (screen space, where +y points down).
    pub gravity: f32,
    /// Particle lifetime seconds.
    pub life: f32,
}

impl Default for ParticleEmitterConfig {
    fn default() -> Self {
        Self {
            emission_rate: 50.0,
            direction_degrees: 90.0,
            spread_degrees: 25.0,
            speed: 120.0,
            gravity: 200.0,
            life: 1.5,
        }
    }
}

/// Advances the particle simulation by `dt` seconds and returns the new population count.
///
/// Emission is quantized per step: `round(emission_rate * dt)` particles are spawned at the
/// origin `(0, 0)` each call with no carried accumulator between steps, so callers stepping at
/// uneven rates should expect quantized emission totals; callers translate spawned particles.
/// Per-particle cone spread randomness is derived deterministically by integer-hashing
/// `(frame_index, spawn index)`, so identical inputs always produce identical states without an
/// RNG crate. Each step integrates velocities and downward gravity, decrements lifetimes, and
/// removes dead particles.
pub fn step_particles(
    particles: &mut Vec<Particle>,
    config: &ParticleEmitterConfig,
    dt: f32,
    frame_index: u64,
) -> usize {
    let dt = dt.max(0.0);
    let emit_count = if config.emission_rate.is_finite() && config.emission_rate > 0.0 && dt > 0.0 {
        (config.emission_rate * dt).round().max(0.0) as usize
    } else {
        0
    };

    let base_angle = config.direction_degrees.to_radians();
    let spread = config.spread_degrees.to_radians();
    let life = config.life.max(0.0);

    for spawn_index in 0..emit_count {
        // SplitMix64-style finalizer keeps emission deterministic without an RNG crate.
        let mut hash = frame_index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (spawn_index as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
        hash ^= hash >> 30;
        hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        hash ^= hash >> 27;
        hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
        hash ^= hash >> 31;
        let unit = (hash >> 40) as f32 / (1u64 << 24) as f32;
        let angle = base_angle + (unit - 0.5) * 2.0 * spread;

        particles.push(Particle {
            x: 0.0,
            y: 0.0,
            vx: config.speed * angle.cos(),
            // Negate sin so positive degrees launch upward against screen-space +y.
            vy: -config.speed * angle.sin(),
            life,
            max_life: life,
        });
    }

    for particle in particles.iter_mut() {
        particle.vy += config.gravity * dt;
        particle.x += particle.vx * dt;
        particle.y += particle.vy * dt;
        particle.life -= dt;
    }

    particles.retain(|particle| particle.life > 0.0);
    particles.len()
}

/// Produces the sub-frame sample times (seconds relative to the current frame time, all < 0,
/// i.e. trailing the frame) used to accumulate one motion-blurred frame, given shutter angle
/// in degrees (360 = full frame exposure), fps, and samples-per-frame count (>=1).
/// Sample i = -(i / samples) * (angle/360)/fps seconds for i in 0..samples.
///
/// Returns an empty vector when `fps <= 0` or non-finite, when `angle_degrees` is outside
/// `(0, 360]` (including non-finite), or when `samples == 0`.
pub fn shutter_sample_offsets(angle_degrees: f64, fps: f64, samples: usize) -> Vec<f64> {
    let angle_ok = angle_degrees > 0.0 && angle_degrees <= 360.0;
    let fps_ok = fps.is_finite() && fps > 0.0;
    if !angle_ok || !fps_ok || samples == 0 {
        return Vec::new();
    }
    (0..samples)
        .map(|i| -(i as f64 / samples as f64) * (angle_degrees / 360.0) / fps)
        .collect()
}

/// Averages N sampled transform positions into one accumulated blur position (simple mean).
/// An empty input yields `(0.0, 0.0)`.
pub fn accumulate_motion_samples(positions: &[(f32, f32)]) -> (f32, f32) {
    if positions.is_empty() {
        return (0.0, 0.0);
    }
    let count = positions.len() as f32;
    let sum_x: f32 = positions.iter().map(|position| position.0).sum();
    let sum_y: f32 = positions.iter().map(|position| position.1).sum();
    (sum_x / count, sum_y / count)
}

/// Handheld camera simulation parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct HandheldConfig {
    /// Maximum positional offset magnitude in pixels at amplitude 1.
    pub amplitude_px: f32,
    /// Low-frequency sway rate Hz.
    pub sway_hz: f32,
    /// Higher-frequency jitter rate Hz.
    pub jitter_hz: f32,
    /// Fraction of jitter vs sway in [0, 1].
    pub jitter_mix: f32,
}

impl Default for HandheldConfig {
    fn default() -> Self {
        Self {
            amplitude_px: 12.0,
            sway_hz: 0.8,
            jitter_hz: 4.5,
            jitter_mix: 0.35,
        }
    }
}

/// Per-seed phase offset in radians `[0, 2*pi)` for one handheld shake channel, derived by
/// integer hashing (SplitMix64-style finalizer) so identical seeds always produce identical
/// motion without an RNG crate.
fn handheld_channel_phase(seed: u64, channel: u64) -> f32 {
    let mut hash = seed ^ channel.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 31;
    (hash >> 40) as f32 / (1u64 << 24) as f32 * 2.0 * std::f32::consts::PI
}

/// Computes the layered handheld offset at time `t` seconds for `seed`: two phase-shifted
/// sines per axis (sway + jitter) with per-seed phase offsets derived from integer hashing;
/// deterministic. Returns `(dx, dy)` each bounded by `+-amplitude_px`.
///
/// Non-finite times and non-positive amplitudes yield `(0.0, 0.0)`; negative rates are
/// treated as zero and `jitter_mix` is clamped to `[0, 1]`.
pub fn calculate_handheld_offset(t: f64, seed: u64, config: &HandheldConfig) -> (f32, f32) {
    let amplitude = config.amplitude_px.max(0.0);
    if !t.is_finite() || amplitude == 0.0 {
        return (0.0, 0.0);
    }
    let sway_hz = config.sway_hz.max(0.0);
    let jitter_hz = config.jitter_hz.max(0.0);
    let jitter_mix = config.jitter_mix.clamp(0.0, 1.0);
    let sway_weight = 1.0 - jitter_mix;

    let time = t as f32;
    let two_pi = 2.0 * std::f32::consts::PI;
    let sway_angle = two_pi * sway_hz * time;
    let jitter_angle = two_pi * jitter_hz * time;

    let dx_raw = sway_weight * (sway_angle + handheld_channel_phase(seed, 0)).sin()
        + jitter_mix * (jitter_angle + handheld_channel_phase(seed, 1)).sin();
    let dy_raw = sway_weight * (sway_angle + handheld_channel_phase(seed, 2)).sin()
        + jitter_mix * (jitter_angle + handheld_channel_phase(seed, 3)).sin();

    (
        (dx_raw * amplitude).clamp(-amplitude, amplitude),
        (dy_raw * amplitude).clamp(-amplitude, amplitude),
    )
}

/// Convenience: samples offsets across `[start, end]` at uniform `dt` producing a `Vec` of
/// `(t, dx, dy)` with sample times `t_i = start + i * dt`. The end time is included only
/// when `(end - start)` lands exactly on a `dt` multiple; otherwise sampling stops at the
/// last interior sample. Returns an empty vector unless `dt > 0` and `end >= start`
/// (both finite).
pub fn handheld_offset_track(
    start: f64,
    end: f64,
    dt: f64,
    seed: u64,
    config: &HandheldConfig,
) -> Vec<(f64, f32, f32)> {
    if !dt.is_finite() || dt <= 0.0 || !start.is_finite() || !end.is_finite() || end < start {
        return Vec::new();
    }
    let steps = ((end - start) / dt).floor().max(0.0) as u64;
    let mut track = Vec::new();
    for i in 0..=steps {
        let t = start + i as f64 * dt;
        if t > end {
            break;
        }
        let (dx, dy) = calculate_handheld_offset(t, seed, config);
        track.push((t, dx, dy));
    }
    track
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

    #[test]
    fn composition_preset_dimensions_and_aspect_ratios() {
        assert_eq!(CompositionPreset::Fhd1080p.dimensions(), (1920, 1080));
        assert_eq!(CompositionPreset::Fhd1080p.aspect_ratio(), (16, 9));

        assert_eq!(CompositionPreset::Square1080.dimensions(), (1080, 1080));
        assert_eq!(CompositionPreset::Square1080.aspect_ratio(), (1, 1));

        assert_eq!(
            CompositionPreset::Vertical1080x1920.dimensions(),
            (1080, 1920)
        );
        assert_eq!(CompositionPreset::Vertical1080x1920.aspect_ratio(), (9, 16));
    }

    #[test]
    fn timeline_time_snapping() {
        // Snap to target at 2.0s with tolerance 0.1s
        let targets = vec![1.0, 2.0, 3.5];
        let snapped = snap_timeline_time(2.04, 30.0, &targets, 0.1);
        assert_eq!(snapped, 2.0);

        // Snap to nearest frame at 30fps (frame duration ~ 0.0333s)
        // 1.035s is closer to frame 31 (1.0333s) than to no target
        let empty_targets: Vec<f32> = Vec::new();
        let frame_snapped = snap_timeline_time(1.035, 30.0, &empty_targets, 0.05);
        assert!((frame_snapped - 1.033333).abs() < 1e-4);
    }

    #[test]
    fn color_interpolation_and_opacity() {
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];

        let mid = interpolate_color_rgba(red, blue, 0.5);
        assert_eq!(mid, [0.5, 0.0, 0.5, 1.0]);

        let transparent_red = apply_layer_opacity(red, 0.4);
        assert_eq!(transparent_red, [1.0, 0.0, 0.0, 0.4]);
    }

    #[test]
    fn keyframe_interpolation_evaluation() {
        // Linear
        let lin = evaluate_keyframe_segment(10.0, 20.0, 0.5, InterpolationMode::Linear);
        assert_eq!(lin, 15.0);

        // Hold
        let hold_mid = evaluate_keyframe_segment(10.0, 20.0, 0.5, InterpolationMode::Hold);
        assert_eq!(hold_mid, 10.0);
        let hold_end = evaluate_keyframe_segment(10.0, 20.0, 1.0, InterpolationMode::Hold);
        assert_eq!(hold_end, 20.0);

        // Bezier ease
        let bez = evaluate_keyframe_segment(0.0, 100.0, 0.5, InterpolationMode::Bezier);
        assert_eq!(bez, 50.0);

        let tangent = TangentHandle::default();
        assert_eq!(tangent.in_weight, 0.33);
    }

    #[test]
    fn polygon_points_interpolation() {
        let square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let shifted = vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]];

        let mid = interpolate_polygon_points(&square, &shifted, 0.5).unwrap();
        assert_eq!(mid[0], [5.0, 5.0]);
        assert_eq!(mid[1], [15.0, 5.0]);
        assert_eq!(mid[2], [15.0, 15.0]);
        assert_eq!(mid[3], [5.0, 15.0]);

        // Mismatched lengths should error
        let triangle = vec![[0.0, 0.0], [5.0, 10.0], [10.0, 0.0]];
        assert!(interpolate_polygon_points(&square, &triangle, 0.5).is_err());
    }

    #[test]
    fn composition_clock_transport() {
        let mut clock = CompositionClock::new(60.0, 120);
        assert_eq!(clock.frame_to_seconds(60), 1.0);
        assert_eq!(clock.seconds_to_frame(2.0), 120);

        assert_eq!(clock.step_forward(), 1);
        assert_eq!(clock.step_forward(), 2);
        assert_eq!(clock.step_backward(), 1);

        clock.seek_frame(120);
        assert_eq!(clock.current_frame, 120);

        // Step forward from out_frame with loop_playback = true should wrap to in_frame (0)
        assert_eq!(clock.step_forward(), 0);
    }

    #[test]
    fn smooth_spatial_motion_path() {
        let waypoints = vec![[0.0, 0.0], [50.0, 100.0], [100.0, 0.0]];
        let smoothed = smooth_spatial_path(&waypoints, 4).unwrap();

        // 2 segments * 4 steps + 1 endpoint = 9 points
        assert_eq!(smoothed.len(), 9);
        assert_eq!(smoothed[0], [0.0, 0.0]);
        assert_eq!(smoothed[8], [100.0, 0.0]);

        // Error with fewer than 2 points
        assert!(smooth_spatial_path(&[[0.0, 0.0]], 4).is_err());
    }

    #[test]
    fn shutter_angle_motion_blur_sampling() {
        let shutter = ShutterConfig {
            shutter_angle_deg: 180.0,
            shutter_phase_deg: 0.0,
            samples_per_frame: 5,
            enabled: true,
        };

        // At 24 fps, 180° = 1/48th of a second
        let exp = shutter.exposure_duration_seconds(24.0);
        assert!((exp - (1.0 / 48.0)).abs() < 1e-6);

        let offsets = shutter.sample_offsets(24.0);
        assert_eq!(offsets.len(), 5);
        // Middle sample should be at 0.0 offset
        assert!(offsets[2].abs() < 1e-6);
        // First and last should be symmetric
        assert!((offsets[0] + offsets[4]).abs() < 1e-6);
    }

    #[test]
    fn audio_amplitude_keyframe_extraction() {
        // 1 second of audio at 48000 Hz: 0.5s silence, 0.5s full volume sine burst
        let mut samples = vec![0.0f32; 48000];
        for (i, sample) in samples[24000..48000].iter_mut().enumerate() {
            *sample = ((24000 + i) as f32 * 0.1).sin();
        }

        let keyframes = generate_audio_driven_keyframes(&samples, 48000, 30.0, 0.3);
        assert_eq!(keyframes.len(), 30); // 30 frames for 1 second

        // Early frames should have ~0 amplitude
        assert!(keyframes[0].amplitude < 0.05);

        // Later frames during sine burst should have substantial amplitude
        assert!(keyframes[25].amplitude > 0.4);
    }

    #[test]
    fn auto_orient_motion_path_headings() {
        // Moving right (+X) -> 0 degrees
        assert_eq!(auto_orient_along_path(10.0, 0.0), 0.0);

        // Moving down (+Y) -> 90 degrees
        assert_eq!(auto_orient_along_path(0.0, 10.0), 90.0);

        // Moving left (-X) -> 180 degrees
        assert_eq!(auto_orient_along_path(-10.0, 0.0), 180.0);

        // Moving up (-Y) -> 270 degrees
        assert_eq!(auto_orient_along_path(0.0, -10.0), 270.0);

        let path = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let headings = calculate_path_headings(&path);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0], 0.0); // moving right
        assert_eq!(headings[1], 90.0); // moving down
    }

    #[test]
    fn procedural_wiggle_jitter_generation() {
        let config = WiggleConfig {
            frequency_hz: 2.0,
            amplitude: 100.0,
            octaves: 2,
            seed: 12345,
        };

        // Time 0.0 vs Time 0.25 should produce smooth but differing values
        let w0 = wiggle_1d(0.0, &config);
        let w1 = wiggle_1d(0.25, &config);
        assert_ne!(w0, w1);

        // Max magnitude should remain bounded by ~1.5 * amplitude (due to octaves)
        assert!(w0.abs() <= 150.0);
        assert!(w1.abs() <= 150.0);

        let config_y = WiggleConfig {
            frequency_hz: 1.5,
            amplitude: 50.0,
            octaves: 1,
            seed: 67890,
        };
        let (dx, dy) = wiggle_2d(0.5, &config, &config_y);
        assert!(dx.abs() <= 150.0);
        assert!(dy.abs() <= 75.0);
    }

    #[test]
    fn inertial_bounce_damping_physics() {
        let config = InertialBounceConfig {
            amplitude: 0.1,
            frequency_hz: 2.0,
            decay: 4.0,
        };

        // At t = 0 (exact keyframe), bounce offset is 0.0
        assert_eq!(calculate_inertial_bounce(0.0, 500.0, &config), 0.0);

        // At t = 0.125 (quarter wave of 2Hz -> sin(pi/2) = 1.0), bounce is peak positive
        let peak = calculate_inertial_bounce(0.125, 500.0, &config);
        assert!(peak > 20.0); // 500 * 0.1 * 1.0 * exp(-4 * 0.125) = 50 * exp(-0.5) ~ 30.32

        // At t = 2.0s (after decay), bounce has decayed to near 0
        let late = calculate_inertial_bounce(2.0, 500.0, &config);
        assert!(late.abs() < 0.1);
    }

    #[test]
    fn particle_simulation_is_deterministic() {
        let config = ParticleEmitterConfig::default();
        let mut run_a = Vec::new();
        let mut run_b = Vec::new();

        let mut populations = Vec::new();
        for frame in 0..10u64 {
            step_particles(&mut run_a, &config, 1.0 / 60.0, frame);
            step_particles(&mut run_b, &config, 1.0 / 60.0, frame);
            populations.push(run_a.len());
        }

        // Identical inputs must produce bit-identical simulation states.
        assert_eq!(run_a, run_b);

        // 50 Hz at 1/60 s rounds to one spawn per step; nothing expires within
        // 1/6 s of a 1.5 s lifetime, so the population grows monotonically.
        assert_eq!(populations[0], 1);
        assert!(populations.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(run_a.len(), 10);

        // Direction 90 launches upward: every initial vy is negative (+y is down).
        assert!(run_a.iter().all(|particle| particle.vy < 0.0));

        // Gravity accelerates particles downward: vy rises toward +y every step
        // even while the emitter is silent (rate 0).
        let vys_before_quiet: Vec<f32> = run_a.iter().map(|particle| particle.vy).collect();
        let quiet = ParticleEmitterConfig {
            emission_rate: 0.0,
            ..config.clone()
        };
        step_particles(&mut run_a, &quiet, 1.0 / 60.0, 10);
        step_particles(&mut run_a, &quiet, 1.0 / 60.0, 11);
        assert_eq!(run_a.len(), vys_before_quiet.len());
        for (before, particle) in vys_before_quiet.iter().zip(run_a.iter()) {
            assert!(particle.vy > *before);
            assert!(particle.life > 0.0);
        }

        // Simulating past max life removes every particle and the emitter stays silent.
        for frame in 12..32u64 {
            step_particles(&mut run_a, &quiet, 0.1, frame);
        }
        assert!(run_a.is_empty());
        assert_eq!(step_particles(&mut run_a, &quiet, 0.1, 99), 0);
        assert!(run_a.is_empty());
    }

    #[test]
    fn motion_blur_sampling_and_accumulation() {
        // 180° shutter at 24 fps over 2 samples:
        // offset_i = -(i / samples) * (angle / 360) / fps
        let offsets = shutter_sample_offsets(180.0, 24.0, 2);
        assert_eq!(offsets.len(), 2);
        assert!((offsets[0] - 0.0).abs() < 1e-9);
        assert!((offsets[1] - (-(1.0 / 2.0) * (180.0 / 360.0) / 24.0)).abs() < 1e-9);

        // Invalid inputs yield no samples: fps must be positive, samples >= 1.
        assert!(shutter_sample_offsets(180.0, 0.0, 2).is_empty());
        assert!(shutter_sample_offsets(180.0, 24.0, 0).is_empty());

        // Accumulation is the simple mean of sampled positions.
        let blurred = accumulate_motion_samples(&[(0.0, 0.0), (10.0, 20.0), (30.0, 40.0)]);
        assert!((blurred.0 - (40.0f32 / 3.0)).abs() < 1e-4);
        assert!((blurred.1 - 20.0).abs() < 1e-9);

        // A single sample accumulates to itself.
        assert_eq!(accumulate_motion_samples(&[(-5.5, 12.25)]), (-5.5, 12.25));
    }

    #[test]
    fn handheld_shake_deterministic_and_bounded() {
        let config = HandheldConfig::default();
        assert_eq!(
            HandheldConfig::default(),
            HandheldConfig {
                amplitude_px: 12.0,
                sway_hz: 0.8,
                jitter_hz: 4.5,
                jitter_mix: 0.35,
            }
        );

        // Same seed produces bit-identical tracks across calls.
        let track_a = handheld_offset_track(0.0, 1.0, 0.125, 12345, &config);
        let track_b = handheld_offset_track(0.0, 1.0, 0.125, 12345, &config);
        assert_eq!(track_a, track_b);

        // Known range/dt: floor(1.0 / 0.125) = 8 steps -> 9 samples including both endpoints.
        assert_eq!(track_a.len(), 9);
        assert_eq!(track_a.first().unwrap().0, 0.0);
        assert_eq!(track_a.last().unwrap().0, 1.0);

        // Point sampling agrees with track sampling at shared timestamps.
        for (t, dx, dy) in &track_a {
            let (expected_dx, expected_dy) = calculate_handheld_offset(*t, 12345, &config);
            assert_eq!((*dx, *dy), (expected_dx, expected_dy));
        }

        // Different seeds produce different motion over the same window.
        let track_other_seed = handheld_offset_track(0.0, 1.0, 0.125, 999, &config);
        assert_ne!(track_a, track_other_seed);

        // Every offset stays within +-amplitude_px.
        let amplitude = config.amplitude_px;
        for (_, dx, dy) in &track_other_seed {
            assert!(dx.abs() <= amplitude);
            assert!(dy.abs() <= amplitude);
        }
        // Bounds also hold far outside a single period.
        for t in [7.3f64, 61.25, 404.5] {
            let (dx, dy) = calculate_handheld_offset(t, 7, &config);
            assert!(dx.abs() <= amplitude && dy.abs() <= amplitude);
        }

        // Non-positive dt yields no samples.
        assert!(handheld_offset_track(0.0, 1.0, 0.0, 1, &config).is_empty());
        assert!(handheld_offset_track(0.0, 1.0, -0.05, 1, &config).is_empty());

        // Inverted ranges yield no samples.
        assert!(handheld_offset_track(1.0, 0.0, 0.125, 1, &config).is_empty());
    }
}
