//! Core nonlinear video editing engine for Loom Video.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackType {
    Video,
    Audio,
    Title,
}

/// A nondestructive clip effect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoEffect {
    /// Stable effect kind, such as `opacity`, `transform`, or `color`.
    pub kind: String,
    /// Whether the effect participates in rendering.
    pub enabled: bool,
    /// Numeric parameters in stable key order.
    pub parameters: std::collections::BTreeMap<String, f64>,
}

impl VideoEffect {
    /// Creates an enabled effect.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            enabled: true,
            parameters: std::collections::BTreeMap::new(),
        }
    }
}

fn default_playback_rate() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

/// Visual color tag for timeline clip organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClipColorTag {
    #[default]
    Orange,
    Blue,
    Green,
    Purple,
    Yellow,
    Teal,
    Rose,
}

impl ClipColorTag {
    /// Returns the standard hex color string for this tag.
    pub fn hex_color(&self) -> &'static str {
        match self {
            ClipColorTag::Orange => "#f97316",
            ClipColorTag::Blue => "#3b82f6",
            ClipColorTag::Green => "#22c55e",
            ClipColorTag::Purple => "#a855f7",
            ClipColorTag::Yellow => "#eab308",
            ClipColorTag::Teal => "#14b8a6",
            ClipColorTag::Rose => "#f43f5e",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub start_time: f64,
    pub duration: f64,
    pub in_point: f64,
    pub out_point: f64,
    #[serde(default = "default_playback_rate")]
    pub playback_rate: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color_tag: ClipColorTag,
    #[serde(default)]
    pub proxy_path: Option<String>,
    #[serde(default)]
    pub effects: Vec<VideoEffect>,
}

impl Clip {
    pub fn new(id: impl Into<String>, name: impl Into<String>, duration: f64) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source_path: String::new(),
            start_time: 0.0,
            duration,
            in_point: 0.0,
            out_point: duration,
            playback_rate: 1.0,
            enabled: true,
            color_tag: ClipColorTag::default(),
            proxy_path: None,
            effects: Vec::new(),
        }
    }

    /// Sets the color organization label for this clip.
    pub fn set_color_tag(&mut self, tag: ClipColorTag) {
        self.color_tag = tag;
    }

    /// Returns the effective timeline duration of the clip accounting for playback_rate.
    pub fn effective_timeline_duration(&self) -> f64 {
        let rate = if self.playback_rate.abs() > 1e-4 {
            self.playback_rate.abs()
        } else {
            1.0
        };
        (self.out_point - self.in_point).max(0.0) / rate
    }

    /// Sets playback speed rate, scaling timeline duration proportionally.
    pub fn set_speed(&mut self, rate: f64) {
        if rate > 0.0 && rate.is_finite() {
            self.playback_rate = rate;
            self.duration = self.effective_timeline_duration();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub track_type: TrackType,
    pub muted: bool,
    pub locked: bool,
    #[serde(default)]
    pub solo: bool,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(id: impl Into<String>, name: impl Into<String>, track_type: TrackType) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            track_type,
            muted: false,
            locked: false,
            solo: false,
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
    }
}

/// Palette color presets for timeline markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MarkerColor {
    #[default]
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    Cyan,
}

impl MarkerColor {
    pub fn as_hex(&self) -> &'static str {
        match self {
            Self::Blue => "#3b82f6",
            Self::Red => "#ef4444",
            Self::Green => "#10b981",
            Self::Yellow => "#f59e0b",
            Self::Purple => "#8b5cf6",
            Self::Cyan => "#06b6d4",
        }
    }
}

/// Timeline marker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineMarker {
    /// Stable id.
    pub id: String,
    /// Timeline time in seconds.
    pub time: f64,
    /// User-visible label.
    pub label: String,
    /// Optional color.
    pub color: String,
}

/// Manual or generated caption cue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptionCue {
    /// Stable id.
    pub id: String,
    /// Inclusive start time.
    pub start: f64,
    /// Exclusive end time.
    pub end: f64,
    /// Caption text.
    pub text: String,
    /// BCP-47 language tag.
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoProject {
    pub id: String,
    pub name: String,
    pub frame_rate: f64,
    pub width: u32,
    pub height: u32,
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub active_track_index: usize,
    #[serde(default)]
    pub markers: Vec<TimelineMarker>,
    #[serde(default)]
    pub captions: Vec<CaptionCue>,
}

impl VideoProject {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut proj = Self {
            id: id.into(),
            name: name.into(),
            frame_rate: 30.0,
            width: 1920,
            height: 1080,
            tracks: Vec::new(),
            active_track_index: 0,
            markers: Vec::new(),
            captions: Vec::new(),
        };
        proj.tracks
            .push(Track::new("v1", "Video 1", TrackType::Video));
        proj.tracks
            .push(Track::new("a1", "Audio 1", TrackType::Audio));
        proj
    }

    pub fn add_track(&mut self, track: Track) {
        self.tracks.push(track);
    }

    pub fn select_track(&mut self, index: usize) -> bool {
        if index < self.tracks.len() {
            self.active_track_index = index;
            true
        } else {
            false
        }
    }

    pub fn total_clips(&self) -> usize {
        self.tracks.iter().map(|t| t.clips.len()).sum()
    }

    /// Finds all timeline markers falling within an inclusive time range `[start, end]`.
    pub fn find_markers_in_range(&self, start: f64, end: f64) -> Vec<&TimelineMarker> {
        self.markers
            .iter()
            .filter(|m| m.time >= start && m.time <= end)
            .collect()
    }
}

pub fn save_video_project(proj: &VideoProject) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(proj).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/project.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Video,
        id: proj.id.clone(),
        title: proj.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/project.json".into(),
            mime: MimeType::parse("application/vnd.loom.video-content")
                .map_err(|e| format!("invalid built-in video MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_video_project(bytes: &[u8]) -> Result<VideoProject, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Video {
        return Err("not a Video project".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/project.json")
        .ok_or_else(|| "missing project.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

/// One resolved timeline segment ready for a decoder/render backend.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSegment {
    /// Track id.
    pub track_id: String,
    /// Clip id.
    pub clip_id: String,
    /// Source path or URI.
    pub source_path: String,
    /// Timeline start.
    pub timeline_start: f64,
    /// Timeline end.
    pub timeline_end: f64,
    /// Source in point.
    pub source_in: f64,
    /// Source out point.
    pub source_out: f64,
    /// Playback rate.
    pub playback_rate: f64,
    /// Whether a proxy should be preferred.
    pub proxy_path: Option<String>,
}

/// Timeline edit failure.
#[derive(Debug, Clone, PartialEq)]
pub enum TimelineError {
    /// Track index was invalid.
    InvalidTrack,
    /// Clip id was not found.
    ClipNotFound,
    /// Track is locked.
    TrackLocked,
    /// Requested timing was invalid.
    InvalidTiming(String),
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTrack => write!(formatter, "track index is invalid"),
            Self::ClipNotFound => write!(formatter, "clip was not found"),
            Self::TrackLocked => write!(formatter, "track is locked"),
            Self::InvalidTiming(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for TimelineError {}

impl Clip {
    /// Effective timeline end.
    pub fn end_time(&self) -> f64 {
        self.start_time + self.duration.max(0.0)
    }

    /// Source span consumed by this clip.
    pub fn source_span(&self) -> f64 {
        (self.out_point - self.in_point).max(0.0)
    }

    /// Recomputes timeline duration from source trim and playback rate.
    pub fn sync_duration(&mut self) -> Result<(), TimelineError> {
        if !self.playback_rate.is_finite() || self.playback_rate <= 0.0 {
            return Err(TimelineError::InvalidTiming(
                "playback rate must be finite and positive".into(),
            ));
        }
        if !self.in_point.is_finite()
            || !self.out_point.is_finite()
            || self.in_point < 0.0
            || self.out_point < self.in_point
        {
            return Err(TimelineError::InvalidTiming(
                "source trim range is invalid".into(),
            ));
        }
        self.duration = self.source_span() / self.playback_rate;
        Ok(())
    }

    /// Splits a clip at an absolute timeline time.
    pub fn split(&self, timeline_time: f64) -> Result<(Clip, Clip), TimelineError> {
        if !timeline_time.is_finite()
            || timeline_time <= self.start_time
            || timeline_time >= self.end_time()
        {
            return Err(TimelineError::InvalidTiming(
                "split point must be inside the clip".into(),
            ));
        }
        let timeline_offset = timeline_time - self.start_time;
        let source_offset = timeline_offset * self.playback_rate;
        let source_split = self.in_point + source_offset;
        let mut left = self.clone();
        left.id = format!("{}-a", self.id);
        left.out_point = source_split;
        left.sync_duration()?;
        let mut right = self.clone();
        right.id = format!("{}-b", self.id);
        right.start_time = timeline_time;
        right.in_point = source_split;
        right.sync_duration()?;
        Ok((left, right))
    }

    /// Trims the source in point, moving the timeline start to preserve the out point.
    pub fn trim_in(&mut self, source_in: f64) -> Result<(), TimelineError> {
        if !source_in.is_finite() || source_in < 0.0 || source_in >= self.out_point {
            return Err(TimelineError::InvalidTiming(
                "invalid source in point".into(),
            ));
        }
        let delta = source_in - self.in_point;
        self.in_point = source_in;
        self.start_time += delta / self.playback_rate.max(f64::EPSILON);
        self.sync_duration()
    }

    /// Trims the source out point.
    pub fn trim_out(&mut self, source_out: f64) -> Result<(), TimelineError> {
        if !source_out.is_finite() || source_out <= self.in_point {
            return Err(TimelineError::InvalidTiming(
                "invalid source out point".into(),
            ));
        }
        self.out_point = source_out;
        self.sync_duration()
    }

    /// Changes speed while retaining source trim.
    pub fn set_playback_rate(&mut self, rate: f64) -> Result<(), TimelineError> {
        self.playback_rate = rate;
        self.sync_duration()
    }
}

impl Track {
    /// Timeline end of the last enabled clip, or zero for an empty track.
    pub fn duration(&self) -> f64 {
        self.clips
            .iter()
            .filter(|clip| clip.enabled)
            .map(Clip::end_time)
            .fold(0.0, f64::max)
    }

    /// Inserts a clip and sorts by timeline start.
    pub fn insert_clip(&mut self, clip: Clip) -> Result<(), TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        validate_clip_timing(&clip)?;
        if self.clips.iter().any(|existing| existing.id == clip.id) {
            return Err(TimelineError::InvalidTiming(format!(
                "duplicate clip id {}",
                clip.id
            )));
        }
        self.clips.push(clip);
        self.sort_clips();
        Ok(())
    }

    /// Sorts clips deterministically by start then id.
    pub fn sort_clips(&mut self) {
        self.clips.sort_by(|left, right| {
            left.start_time
                .total_cmp(&right.start_time)
                .then_with(|| left.id.cmp(&right.id))
        });
    }

    /// Removes a clip and optionally closes the removed timeline gap.
    pub fn remove_clip(&mut self, clip_id: &str, ripple: bool) -> Result<Clip, TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        let index = self
            .clips
            .iter()
            .position(|clip| clip.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let removed = self.clips.remove(index);
        if ripple {
            let gap_start = removed.start_time;
            let gap = removed.duration;
            for clip in &mut self.clips {
                if clip.start_time >= removed.end_time() - f64::EPSILON {
                    clip.start_time = (clip.start_time - gap).max(gap_start);
                }
            }
            self.sort_clips();
        }
        Ok(removed)
    }

    /// Returns overlapping clip-id pairs.
    pub fn overlaps(&self) -> Vec<(String, String)> {
        let mut clips: Vec<&Clip> = self.clips.iter().filter(|clip| clip.enabled).collect();
        clips.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
        let mut overlaps = Vec::new();
        for pair in clips.windows(2) {
            if pair[0].end_time() > pair[1].start_time + f64::EPSILON {
                overlaps.push((pair[0].id.clone(), pair[1].id.clone()));
            }
        }
        overlaps
    }

    /// Slips the source content of a clip by delta seconds without changing its timeline position or duration.
    pub fn slip_clip(&mut self, clip_id: &str, delta_secs: f64) -> Result<(), TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        let clip = self
            .clips
            .iter_mut()
            .find(|c| c.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let new_in = clip.in_point + delta_secs;
        let new_out = clip.out_point + delta_secs;
        if new_in < 0.0 || new_out <= new_in {
            return Err(TimelineError::InvalidTiming(
                "slip would result in negative or invalid in/out points".into(),
            ));
        }
        clip.in_point = new_in;
        clip.out_point = new_out;
        clip.sync_duration()
    }

    /// Slides a clip along the timeline by delta seconds without changing its duration.
    pub fn slide_clip(&mut self, clip_id: &str, delta_secs: f64) -> Result<(), TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        let clip = self
            .clips
            .iter_mut()
            .find(|c| c.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let new_start = clip.start_time + delta_secs;
        if new_start < 0.0 {
            return Err(TimelineError::InvalidTiming(
                "slide would result in negative start time".into(),
            ));
        }
        clip.start_time = new_start;
        self.sort_clips();
        Ok(())
    }

    /// Closes all gaps between clips on this track, aligning them contiguously starting at time 0.0.
    pub fn close_gaps(&mut self) -> Result<usize, TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        self.sort_clips();
        let mut current_time = 0.0;
        let mut moved_count = 0;
        for clip in &mut self.clips {
            if (clip.start_time - current_time).abs() > 1e-4 {
                clip.start_time = current_time;
                moved_count += 1;
            }
            current_time += clip.effective_timeline_duration();
        }
        Ok(moved_count)
    }

    /// Splits a clip at timeline time `split_time`, creating a second clip for the remaining duration.
    /// Returns the new clip ID.
    pub fn split_clip(&mut self, clip_id: &str, split_time: f64) -> Result<String, TimelineError> {
        if self.locked {
            return Err(TimelineError::TrackLocked);
        }
        let clip_idx = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;

        let clip = &self.clips[clip_idx];
        let clip_start = clip.start_time;
        let clip_duration = clip.effective_timeline_duration();
        let clip_end = clip_start + clip_duration;

        if split_time <= clip_start + 0.001 || split_time >= clip_end - 0.001 {
            return Err(TimelineError::InvalidTiming(
                "split time must be strictly within clip bounds".into(),
            ));
        }

        let rate = clip.playback_rate.max(0.001);
        let left_duration = split_time - clip_start;
        let source_delta = left_duration * rate;

        let new_id = format!("{}-split", clip.id);
        let mut second_clip = clip.clone();
        second_clip.id = new_id.clone();
        second_clip.start_time = split_time;
        second_clip.in_point += source_delta;
        second_clip.duration = (clip.out_point - second_clip.in_point) / rate;
        second_clip.out_point = second_clip.in_point + second_clip.duration * rate;

        // Resize left clip
        let first_clip = &mut self.clips[clip_idx];
        first_clip.duration = left_duration;
        first_clip.out_point = first_clip.in_point + first_clip.duration * rate;

        self.clips.push(second_clip);
        self.sort_clips();
        Ok(new_id)
    }
}

/// Computes min and max peak pairs `(min_sample, max_sample)` decimated into `target_bins` for fast timeline waveform rendering.
pub fn compute_waveform_peaks(samples: &[f32], target_bins: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || target_bins == 0 {
        return Vec::new();
    }
    let total_samples = samples.len();
    let bin_size = (total_samples as f64 / target_bins as f64).max(1.0);
    let mut peaks = Vec::with_capacity(target_bins);

    for bin in 0..target_bins {
        let start = (bin as f64 * bin_size) as usize;
        let end = (((bin + 1) as f64 * bin_size) as usize).min(total_samples);
        if start >= total_samples {
            break;
        }
        let slice = &samples[start..end.max(start + 1)];
        let mut min_val = 0.0f32;
        let mut max_val = 0.0f32;
        for &s in slice {
            if s < min_val {
                min_val = s;
            }
            if s > max_val {
                max_val = s;
            }
        }
        peaks.push((min_val, max_val));
    }
    peaks
}

/// Snaps a timeline playhead or edit coordinate to nearby clip start/end boundaries or markers.
pub fn snap_timeline_to_edit_points(
    time_secs: f64,
    tracks: &[Track],
    markers: &[TimelineMarker],
    tolerance_secs: f64,
) -> f64 {
    let mut best_snap = time_secs;
    let mut min_diff = tolerance_secs;

    for track in tracks {
        for clip in &track.clips {
            let start_diff = (time_secs - clip.start_time).abs();
            if start_diff <= min_diff {
                min_diff = start_diff;
                best_snap = clip.start_time;
            }

            let end_time = clip.start_time + clip.effective_timeline_duration();
            let end_diff = (time_secs - end_time).abs();
            if end_diff <= min_diff {
                min_diff = end_diff;
                best_snap = end_time;
            }
        }
    }

    for marker in markers {
        let marker_diff = (time_secs - marker.time).abs();
        if marker_diff <= min_diff {
            min_diff = marker_diff;
            best_snap = marker.time;
        }
    }

    best_snap
}

/// Aligns timeline clip start times to the nearest musical beat grid (BPM) if within `snap_threshold_secs`.
/// Returns the number of clips aligned.
pub fn align_clips_to_beat_grid(
    clips: &mut [Clip],
    bpm: f64,
    time_offset_secs: f64,
    snap_threshold_secs: f64,
) -> usize {
    if bpm <= 0.0 || snap_threshold_secs <= 0.0 {
        return 0;
    }

    let beat_interval = 60.0 / bpm;
    let mut aligned_count = 0;

    for clip in clips.iter_mut() {
        let relative_time = clip.start_time - time_offset_secs;
        let nearest_beat_idx = (relative_time / beat_interval).round();
        let nearest_beat_time = (time_offset_secs + nearest_beat_idx * beat_interval).max(0.0);

        let delta = (clip.start_time - nearest_beat_time).abs();
        if delta <= snap_threshold_secs && delta > 1e-4 {
            clip.start_time = nearest_beat_time;
            aligned_count += 1;
        }
    }

    aligned_count
}

/// Performs a Roll Edit between two adjacent clips, shifting the cut point by `delta_secs`.
/// The left clip's duration increases by `delta_secs` while the right clip's in_point and start_time
/// shift by `delta_secs` and its duration decreases by `delta_secs`.
pub fn roll_edit(
    left_clip: &mut Clip,
    right_clip: &mut Clip,
    delta_secs: f64,
) -> Result<(), String> {
    if left_clip.duration + delta_secs <= 0.1 {
        return Err("roll edit would make left clip too short".into());
    }
    if right_clip.duration - delta_secs <= 0.1 {
        return Err("roll edit would make right clip too short".into());
    }

    left_clip.duration += delta_secs;
    right_clip.in_point += delta_secs;
    right_clip.start_time += delta_secs;
    right_clip.duration -= delta_secs;

    Ok(())
}

/// Performs a Slip Edit on a clip, shifting its internal media window by `delta_secs`
/// while preserving its timeline start_time and timeline duration.
pub fn slip_edit(clip: &mut Clip, delta_secs: f64, media_duration: f64) -> Result<(), String> {
    let new_in = clip.in_point + delta_secs;
    if new_in < 0.0 {
        return Err("slip edit before start of media".into());
    }
    if new_in + clip.duration > media_duration {
        return Err("slip edit past end of media".into());
    }

    clip.in_point = new_in;
    Ok(())
}

/// Supported NLE video track transition types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VideoTransitionType {
    #[default]
    CrossDissolve,
    DipToBlack,
    DipToWhite,
    WipeLeft,
    WipeRight,
}

/// NLE Video Transition configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoTransition {
    pub kind: VideoTransitionType,
    pub duration_secs: f64,
    pub alignment: String,
}

impl Default for VideoTransition {
    fn default() -> Self {
        Self {
            kind: VideoTransitionType::CrossDissolve,
            duration_secs: 1.0,
            alignment: "CenterOnCut".into(),
        }
    }
}

/// Calculates timeline (start_time, end_time) bounds for a transition around a cut point.
pub fn calculate_transition_overlap(cut_point: f64, duration: f64, alignment: &str) -> (f64, f64) {
    let dur = duration.max(0.01);
    match alignment {
        "StartOnCut" => (cut_point, cut_point + dur),
        "EndOnCut" => ((cut_point - dur).max(0.0), cut_point),
        _ => {
            // Default "CenterOnCut"
            let half = dur / 2.0;
            ((cut_point - half).max(0.0), cut_point + half)
        }
    }
}

/// Volume keyframe on a clip audio envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEnvelopeKey {
    pub time_offset: f64,
    pub volume_db: f32,
}

/// Dynamic audio volume automation curve across a clip.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AudioEnvelope {
    pub keys: Vec<AudioEnvelopeKey>,
}

impl AudioEnvelope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or updates an automation key at a relative clip offset.
    pub fn add_key(&mut self, time_offset: f64, volume_db: f32) {
        if let Some(pos) = self
            .keys
            .iter()
            .position(|k| (k.time_offset - time_offset).abs() < 1e-4)
        {
            self.keys[pos].volume_db = volume_db;
        } else {
            self.keys.push(AudioEnvelopeKey {
                time_offset,
                volume_db,
            });
            self.keys.sort_by(|a, b| {
                a.time_offset
                    .partial_cmp(&b.time_offset)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    /// Evaluates interpolated decibel volume gain at a relative clip time offset.
    pub fn evaluate_volume_db_at(&self, time_offset: f64) -> f32 {
        if self.keys.is_empty() {
            return 0.0; // 0 dB unity gain default
        }
        if time_offset <= self.keys[0].time_offset {
            return self.keys[0].volume_db;
        }
        if time_offset >= self.keys.last().unwrap().time_offset {
            return self.keys.last().unwrap().volume_db;
        }

        // Find surrounding keys for linear interpolation
        for i in 0..self.keys.len() - 1 {
            let k1 = &self.keys[i];
            let k2 = &self.keys[i + 1];
            if time_offset >= k1.time_offset && time_offset <= k2.time_offset {
                let span = k2.time_offset - k1.time_offset;
                if span.abs() < 1e-5 {
                    return k1.volume_db;
                }
                let t = ((time_offset - k1.time_offset) / span) as f32;
                return k1.volume_db + (k2.volume_db - k1.volume_db) * t;
            }
        }

        0.0
    }
}

/// Ken Burns / dynamic pan-and-zoom motion effect across an image or video clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KenBurnsEffect {
    /// Normalized starting crop box (x, y, width, height) in `[0.0, 1.0]`.
    pub start_rect: (f32, f32, f32, f32),
    /// Normalized ending crop box (x, y, width, height) in `[0.0, 1.0]`.
    pub end_rect: (f32, f32, f32, f32),
}

impl Default for KenBurnsEffect {
    fn default() -> Self {
        Self {
            start_rect: (0.0, 0.0, 1.0, 1.0),
            end_rect: (0.1, 0.1, 0.8, 0.8),
        }
    }
}

impl KenBurnsEffect {
    /// Computes interpolated crop rectangle at normalized time progress `t` in `[0.0, 1.0]` with smoothstep easing.
    pub fn interpolate_crop_rect(&self, progress: f64) -> (f32, f32, f32, f32) {
        let t = progress.clamp(0.0, 1.0) as f32;
        let smooth_t = t * t * (3.0 - 2.0 * t); // smoothstep

        let (sx, sy, sw, sh) = self.start_rect;
        let (ex, ey, ew, eh) = self.end_rect;

        (
            sx + (ex - sx) * smooth_t,
            sy + (ey - sy) * smooth_t,
            sw + (ew - sw) * smooth_t,
            sh + (eh - sh) * smooth_t,
        )
    }
}

/// Multitrack audio mixing parameters for video timeline audio tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackAudioConfig {
    /// Fader volume in decibels (0.0 dB = unity gain).
    pub volume_db: f32,
    /// Stereo panning position in `[-1.0, 1.0]` (-1.0 = full left, 1.0 = full right).
    pub pan: f32,
    /// Track mute status.
    pub is_muted: bool,
    /// Track solo status.
    pub is_solo: bool,
}

impl Default for TrackAudioConfig {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            is_muted: false,
            is_solo: false,
        }
    }
}

impl TrackAudioConfig {
    /// Computes effective (left, right) linear amplitude gains accounting for mute and pan law.
    pub fn stereo_linear_gains(&self) -> (f32, f32) {
        if self.is_muted {
            return (0.0, 0.0);
        }

        let linear_vol = 10.0f32.powf(self.volume_db / 20.0);
        let pan = self.pan.clamp(-1.0, 1.0);

        // Constant-power sinusoidal pan law
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let left_pan = angle.cos();
        let right_pan = angle.sin();

        (linear_vol * left_pan, linear_vol * right_pan)
    }
}

/// Brickwall peak limiter for project master audio output preventing digital clipping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterAudioLimiter {
    /// True peak output ceiling in decibels (e.g. -0.5 dBTP).
    pub ceiling_db: f32,
    /// Limiter engagement threshold in decibels (e.g. -1.0 dB).
    pub threshold_db: f32,
    /// Release recovery time in milliseconds.
    pub release_ms: f32,
    /// Limiter enabled state.
    pub enabled: bool,
}

impl Default for MasterAudioLimiter {
    fn default() -> Self {
        Self {
            ceiling_db: -0.5,
            threshold_db: -1.0,
            release_ms: 50.0,
            enabled: true,
        }
    }
}

impl MasterAudioLimiter {
    /// Applies lookahead brickwall peak limiting and saturation prevention to interleaved stereo channels.
    pub fn process_stereo_samples(&self, left: &mut [f32], right: &mut [f32], sample_rate: u32) {
        if !self.enabled || left.is_empty() || sample_rate == 0 {
            return;
        }

        let ceiling_lin = 10.0f32.powf(self.ceiling_db / 20.0);
        let release_coeff = (-1.0 / (self.release_ms.max(1.0) * 0.001 * sample_rate as f32)).exp();
        let mut gain = 1.0f32;

        let len = left.len().min(right.len());
        for i in 0..len {
            let peak = left[i].abs().max(right[i].abs());
            let target_gain = if peak > ceiling_lin && peak > 0.0 {
                ceiling_lin / peak
            } else {
                1.0
            };

            if target_gain < gain {
                gain = target_gain; // Instant attack
            } else {
                gain = target_gain + release_coeff * (gain - target_gain); // Exponential release
            }

            left[i] = (left[i] * gain).clamp(-ceiling_lin, ceiling_lin);
            right[i] = (right[i] * gain).clamp(-ceiling_lin, ceiling_lin);
        }
    }
}

/// SMPTE timecode representation (HH:MM:SS:FF).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timecode {
    /// Hours [0..].
    pub hours: u32,
    /// Minutes [0..=59].
    pub minutes: u32,
    /// Seconds [0..=59].
    pub seconds: u32,
    /// Frames [0..frame_rate).
    pub frames: u32,
}

impl Timecode {
    /// Formats seconds to a standard SMPTE timecode `HH:MM:SS:FF` given a frame rate.
    pub fn from_seconds(seconds: f64, frame_rate: f64) -> Self {
        let fps = if frame_rate > 0.0 { frame_rate } else { 30.0 };
        let total_frames = (seconds.max(0.0) * fps).round() as u64;
        let fps_u64 = fps.round().max(1.0) as u64;
        let frames = (total_frames % fps_u64) as u32;
        let total_seconds = total_frames / fps_u64;
        let sec = (total_seconds % 60) as u32;
        let total_minutes = total_seconds / 60;
        let min = (total_minutes % 60) as u32;
        let hrs = (total_minutes / 60) as u32;
        Timecode {
            hours: hrs,
            minutes: min,
            seconds: sec,
            frames,
        }
    }

    /// Converts this timecode to total seconds given a frame rate.
    pub fn to_seconds(&self, frame_rate: f64) -> f64 {
        let fps = if frame_rate > 0.0 { frame_rate } else { 30.0 };
        let total_seconds =
            (self.hours as f64) * 3600.0 + (self.minutes as f64) * 60.0 + (self.seconds as f64);
        total_seconds + (self.frames as f64) / fps
    }

    /// Formats as a display string `HH:MM:SS:FF`.
    pub fn format_smpte(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}:{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

impl VideoProject {
    /// Returns the current timecode at the given playhead time in seconds.
    pub fn timecode_at(&self, time_secs: f64) -> Timecode {
        Timecode::from_seconds(time_secs, self.frame_rate)
    }

    /// Converts timeline seconds to pixel coordinate based on zoom level (pixels per second).
    pub fn seconds_to_pixels(seconds: f64, pixels_per_second: f64) -> f64 {
        seconds.max(0.0) * pixels_per_second.max(1.0)
    }

    /// Converts timeline pixel coordinate to seconds based on zoom level (pixels per second).
    pub fn pixels_to_seconds(pixels: f64, pixels_per_second: f64) -> f64 {
        pixels.max(0.0) / pixels_per_second.max(1.0)
    }

    /// Timeline duration across every enabled clip and caption.
    pub fn duration(&self) -> f64 {
        let clips = self
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter(|clip| clip.enabled)
            .map(Clip::end_time)
            .fold(0.0_f64, f64::max);
        let captions = self
            .captions
            .iter()
            .map(|caption| caption.end)
            .fold(0.0, f64::max);
        clips.max(captions)
    }

    /// Adds a marker in sorted order.
    pub fn add_marker(&mut self, marker: TimelineMarker) -> Result<(), TimelineError> {
        if !marker.time.is_finite() || marker.time < 0.0 {
            return Err(TimelineError::InvalidTiming(
                "marker time is invalid".into(),
            ));
        }
        if self.markers.iter().any(|existing| existing.id == marker.id) {
            return Err(TimelineError::InvalidTiming(format!(
                "duplicate marker id {}",
                marker.id
            )));
        }
        self.markers.push(marker);
        self.markers
            .sort_by(|left, right| left.time.total_cmp(&right.time));
        Ok(())
    }

    /// Adds a caption cue in sorted order.
    pub fn add_caption(&mut self, caption: CaptionCue) -> Result<(), TimelineError> {
        if !caption.start.is_finite()
            || !caption.end.is_finite()
            || caption.start < 0.0
            || caption.end <= caption.start
        {
            return Err(TimelineError::InvalidTiming(
                "caption range is invalid".into(),
            ));
        }
        self.captions.push(caption);
        self.captions
            .sort_by(|left, right| left.start.total_cmp(&right.start));
        Ok(())
    }

    /// Splits one clip in-place.
    pub fn split_clip(
        &mut self,
        track_index: usize,
        clip_id: &str,
        timeline_time: f64,
    ) -> Result<(String, String), TimelineError> {
        let track = self
            .tracks
            .get_mut(track_index)
            .ok_or(TimelineError::InvalidTrack)?;
        if track.locked {
            return Err(TimelineError::TrackLocked);
        }
        let index = track
            .clips
            .iter()
            .position(|clip| clip.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let (left, right) = track.clips[index].split(timeline_time)?;
        let ids = (left.id.clone(), right.id.clone());
        track.clips.splice(index..=index, [left, right]);
        Ok(ids)
    }

    /// Moves a clip to a track/time, optionally rippling later clips on the destination.
    pub fn move_clip(
        &mut self,
        from_track: usize,
        to_track: usize,
        clip_id: &str,
        start_time: f64,
        ripple: bool,
    ) -> Result<(), TimelineError> {
        if !start_time.is_finite() || start_time < 0.0 {
            return Err(TimelineError::InvalidTiming("start time is invalid".into()));
        }
        if from_track >= self.tracks.len() || to_track >= self.tracks.len() {
            return Err(TimelineError::InvalidTrack);
        }
        if self.tracks[from_track].locked || self.tracks[to_track].locked {
            return Err(TimelineError::TrackLocked);
        }
        let index = self.tracks[from_track]
            .clips
            .iter()
            .position(|clip| clip.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let mut clip = self.tracks[from_track].clips.remove(index);
        clip.start_time = start_time;
        if ripple {
            for existing in &mut self.tracks[to_track].clips {
                if existing.start_time >= start_time {
                    existing.start_time += clip.duration;
                }
            }
        }
        self.tracks[to_track].clips.push(clip);
        self.tracks[to_track].sort_clips();
        Ok(())
    }

    /// Deletes a clip from a track without rippling.
    pub fn delete_clip(
        &mut self,
        track_index: usize,
        clip_id: &str,
    ) -> Result<Clip, TimelineError> {
        let track = self
            .tracks
            .get_mut(track_index)
            .ok_or(TimelineError::InvalidTrack)?;
        if track.locked {
            return Err(TimelineError::TrackLocked);
        }
        let pos = track
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        Ok(track.clips.remove(pos))
    }

    /// Ripple deletes a clip from a track, closing the gap by shifting following clips left.
    pub fn ripple_delete_clip(
        &mut self,
        track_index: usize,
        clip_id: &str,
    ) -> Result<Clip, TimelineError> {
        let track = self
            .tracks
            .get_mut(track_index)
            .ok_or(TimelineError::InvalidTrack)?;
        if track.locked {
            return Err(TimelineError::TrackLocked);
        }
        let pos = track
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or(TimelineError::ClipNotFound)?;
        let removed = track.clips.remove(pos);
        let removed_duration = removed.duration;
        for following in &mut track.clips[pos..] {
            following.start_time = (following.start_time - removed_duration).max(0.0);
        }
        Ok(removed)
    }

    /// Removes a timeline marker by id.
    pub fn remove_marker(&mut self, marker_id: &str) -> bool {
        if let Some(pos) = self.markers.iter().position(|m| m.id == marker_id) {
            self.markers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Builds the decoder/render plan for all enabled, unlocked media clips.
    pub fn render_plan(&self) -> Vec<RenderSegment> {
        let solo_active = self.tracks.iter().any(|track| track.solo);
        let mut plan = Vec::new();
        for track in &self.tracks {
            if track.muted || (solo_active && !track.solo) {
                continue;
            }
            for clip in &track.clips {
                if !clip.enabled {
                    continue;
                }
                plan.push(RenderSegment {
                    track_id: track.id.clone(),
                    clip_id: clip.id.clone(),
                    source_path: clip.source_path.clone(),
                    timeline_start: clip.start_time,
                    timeline_end: clip.end_time(),
                    source_in: clip.in_point,
                    source_out: clip.out_point,
                    playback_rate: clip.playback_rate,
                    proxy_path: clip.proxy_path.clone(),
                });
            }
        }
        plan.sort_by(|left, right| {
            left.timeline_start
                .total_cmp(&right.timeline_start)
                .then_with(|| left.track_id.cmp(&right.track_id))
        });
        plan
    }

    /// Exports a simple, documented edit decision list for interchange/debugging.
    pub fn to_edl(&self) -> String {
        let mut output = format!("TITLE: {}\nFCM: NON-DROP FRAME\n", self.name);
        for (index, segment) in self.render_plan().iter().enumerate() {
            output.push_str(&format!(
                "{:03}  {}  V  C  {:.3} {:.3} {:.3} {:.3}\n* FROM CLIP NAME: {}\n",
                index + 1,
                segment.track_id,
                segment.source_in,
                segment.source_out,
                segment.timeline_start,
                segment.timeline_end,
                segment.clip_id
            ));
        }
        output
    }
}

fn validate_clip_timing(clip: &Clip) -> Result<(), TimelineError> {
    if !clip.start_time.is_finite()
        || !clip.duration.is_finite()
        || !clip.in_point.is_finite()
        || !clip.out_point.is_finite()
        || !clip.playback_rate.is_finite()
        || clip.start_time < 0.0
        || clip.duration < 0.0
        || clip.in_point < 0.0
        || clip.out_point < clip.in_point
        || clip.playback_rate <= 0.0
    {
        return Err(TimelineError::InvalidTiming(format!(
            "clip {} has invalid timing",
            clip.id
        )));
    }
    Ok(())
}

/// Undoable editing session for a video project.
#[derive(Debug, Clone)]
pub struct VideoSession {
    /// Current project.
    pub project: VideoProject,
    undo: Vec<VideoProject>,
    redo: Vec<VideoProject>,
    history_limit: usize,
}

impl VideoSession {
    /// Creates a new session.
    pub fn new(project: VideoProject) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            history_limit: 64,
        }
    }

    /// Records a project snapshot before mutation.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Returns whether undo is possible.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns whether redo is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Restores the previous project.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut self.project, previous));
        true
    }

    /// Reapplies the next project.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.project, next));
        true
    }

    /// Applies an edit to a cloned project and records history only when the
    /// edit succeeds. This keeps failed/cancelled commands out of undo.
    pub fn apply_edit<E, F>(&mut self, edit: F) -> Result<(), E>
    where
        F: FnOnce(&mut VideoProject) -> Result<(), E>,
    {
        let mut candidate = self.project.clone();
        edit(&mut candidate)?;
        self.checkpoint();
        self.project = candidate;
        Ok(())
    }

    /// Applies an edit without creating a history entry. Gesture updates use
    /// this while the pointer is down and commit one checkpoint on release.
    pub fn apply_edit_without_history<E, F>(&mut self, edit: F) -> Result<(), E>
    where
        F: FnOnce(&mut VideoProject) -> Result<(), E>,
    {
        let mut candidate = self.project.clone();
        edit(&mut candidate)?;
        self.project = candidate;
        Ok(())
    }

    /// Commits a gesture baseline as one undo entry when the document changed.
    pub fn commit_gesture(&mut self, baseline: VideoProject) -> bool {
        if baseline == self.project {
            return false;
        }
        self.undo.push(baseline);
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    /// Rolls the current project back to a gesture baseline without touching
    /// existing history.
    pub fn rollback_gesture(&mut self, baseline: VideoProject) {
        self.project = baseline;
    }
}

/// Cooperative cancellation shared by export workers and their UI command.
#[derive(Debug, Clone, Default)]
pub struct ExportCancellation(Arc<AtomicBool>);

impl ExportCancellation {
    pub fn reset(&self) {
        self.0.store(false, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Cooperative cancellation shared by preview decoder workers.
#[derive(Debug, Clone, Default)]
pub struct PreviewCancellation(Arc<AtomicBool>);

impl PreviewCancellation {
    /// Requests cancellation of the associated preview worker.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Local FFmpeg/FFprobe/FFplay toolchain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTools {
    /// FFmpeg executable.
    pub ffmpeg: PathBuf,
    /// FFprobe executable.
    pub ffprobe: PathBuf,
    /// FFplay executable.
    pub ffplay: PathBuf,
    /// FFmpeg version line.
    pub version: String,
}

/// Probed media metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaProbe {
    /// Original path.
    pub path: PathBuf,
    /// Duration in seconds.
    pub duration: f64,
    /// Primary video width, or zero for audio-only files.
    pub width: u32,
    /// Primary video height, or zero for audio-only files.
    pub height: u32,
    /// Primary video frame rate.
    pub frame_rate: f64,
    /// Whether at least one audio stream exists.
    pub has_audio: bool,
    /// Sample rate of the first audio stream, when present.
    #[serde(default)]
    pub audio_sample_rate: Option<u32>,
}

/// Decoded RGBA preview frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Row-major RGBA8 bytes.
    pub pixels: Vec<u8>,
}

/// A deterministic FFmpeg sequence-export command.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineExportPlan {
    /// Executable.
    pub executable: PathBuf,
    /// Arguments excluding the executable.
    pub arguments: Vec<String>,
    /// Output path.
    pub output: PathBuf,
    /// Expected output duration.
    pub duration: f64,
}

/// Discovers a fully local media toolchain.
pub fn discover_media_tools() -> Result<MediaTools, String> {
    let ffmpeg = PathBuf::from(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let output = Command::new(&ffmpeg)
        .arg("-version")
        .output()
        .map_err(|error| format!("start FFmpeg: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("ffmpeg")
        .to_string();
    Ok(MediaTools {
        ffmpeg,
        ffprobe: PathBuf::from(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }),
        ffplay: PathBuf::from(if cfg!(windows) {
            "ffplay.exe"
        } else {
            "ffplay"
        }),
        version,
    })
}

#[derive(Debug, Deserialize)]
struct ProbeDocument {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}
#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    sample_rate: Option<String>,
}
#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

fn parse_ratio(value: &str) -> f64 {
    let Some((numerator, denominator)) = value.split_once('/') else {
        return value.parse().unwrap_or(0.0);
    };
    let numerator = numerator.parse::<f64>().unwrap_or(0.0);
    let denominator = denominator.parse::<f64>().unwrap_or(0.0);
    if denominator.abs() <= f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

fn parse_sample_rate(value: &str) -> Option<u32> {
    let rate = value.parse::<f64>().ok()?;
    if rate.is_finite() && rate > 0.0 && rate <= f64::from(u32::MAX) {
        Some(rate.round() as u32)
    } else {
        None
    }
}

/// Probes a local media file through FFprobe JSON output.
pub fn probe_media(tools: &MediaTools, path: &Path) -> Result<MediaProbe, String> {
    if !path.is_file() {
        return Err(format!("media does not exist: {}", path.display()));
    }
    let output = Command::new(&tools.ffprobe)
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("start FFprobe: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let document: ProbeDocument = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse FFprobe JSON: {error}"))?;
    let video = document
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"));
    let has_audio = document
        .streams
        .iter()
        .any(|stream| stream.codec_type.as_deref() == Some("audio"));
    let audio_sample_rate = document
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .and_then(|stream| stream.sample_rate.as_deref())
        .and_then(parse_sample_rate);
    let duration = document
        .format
        .and_then(|format| format.duration)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    Ok(MediaProbe {
        path: path.to_path_buf(),
        duration,
        width: video.and_then(|stream| stream.width).unwrap_or(0),
        height: video.and_then(|stream| stream.height).unwrap_or(0),
        frame_rate: video
            .and_then(|stream| stream.r_frame_rate.as_deref())
            .map(parse_ratio)
            .unwrap_or(0.0),
        has_audio,
        audio_sample_rate,
    })
}

/// Decodes one scaled RGBA preview frame through FFmpeg.
pub fn decode_preview_frame(
    tools: &MediaTools,
    path: &Path,
    time_secs: f64,
    max_width: u32,
    max_height: u32,
) -> Result<VideoFrame, String> {
    decode_preview_frame_with_cancel(
        tools,
        path,
        time_secs,
        max_width,
        max_height,
        &PreviewCancellation::default(),
    )
}

/// Decodes one scaled RGBA preview frame while allowing the worker to be
/// cancelled without waiting for FFmpeg to finish.
pub fn decode_preview_frame_with_cancel(
    tools: &MediaTools,
    path: &Path,
    time_secs: f64,
    max_width: u32,
    max_height: u32,
    cancel: &PreviewCancellation,
) -> Result<VideoFrame, String> {
    if max_width == 0 || max_height == 0 {
        return Err("preview dimensions must be non-zero".into());
    }
    if cancel.is_cancelled() {
        return Err("preview decode cancelled".into());
    }
    let probe = probe_media(tools, path)?;
    if probe.width == 0 || probe.height == 0 {
        return Err("media has no video stream".into());
    }
    let scale = (max_width as f64 / probe.width as f64)
        .min(max_height as f64 / probe.height as f64)
        .min(1.0);
    let width = ((probe.width as f64 * scale).round() as u32).max(2) & !1;
    let height = ((probe.height as f64 * scale).round() as u32).max(2) & !1;
    let mut child = Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.6}", time_secs.max(0.0)),
            "-i",
        ])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={width}:{height}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start FFmpeg preview decoder: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "FFmpeg preview stdout was not captured".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "FFmpeg preview stderr was not captured".to_string())?;
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = std::io::Read::read_to_end(&mut BufReader::new(stdout), &mut bytes);
        result.map(|_| bytes).map_err(|error| error.to_string())
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let result = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut text);
        result.map(|_| text).map_err(|error| error.to_string())
    });
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("preview decode cancelled".into());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for FFmpeg preview decoder: {error}"))?
        {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let pixels = stdout_reader
        .join()
        .map_err(|_| "FFmpeg preview stdout reader panicked".to_string())?
        .map_err(|error| format!("read FFmpeg preview output: {error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "FFmpeg preview stderr reader panicked".to_string())?
        .map_err(|error| format!("read FFmpeg preview errors: {error}"))?;
    if !status.success() {
        return Err(stderr);
    }
    let expected = width as usize * height as usize * 4;
    if pixels.len() != expected {
        return Err(format!(
            "decoded frame has {} bytes; expected {expected}",
            pixels.len()
        ));
    }
    Ok(VideoFrame {
        width,
        height,
        pixels,
    })
}

/// Decodes the first audio stream into bounded mono waveform peaks.
///
/// The raw PCM is generated by the local FFmpeg process and reduced before it
/// reaches the application cache. The cancellation token lets a newer seek or
/// project load stop the worker without leaving a decoder process behind.
const WAVEFORM_READ_BUFFER_BYTES: usize = 64 * 1024;
const WAVEFORM_STDERR_LIMIT_BYTES: u64 = 64 * 1024;

fn drain_reader_with_prefix<R: Read>(mut reader: R, limit: usize) -> std::io::Result<String> {
    let mut retained = Vec::with_capacity(limit.min(WAVEFORM_READ_BUFFER_BYTES));
    let mut bytes = [0_u8; WAVEFORM_READ_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        if retained.len() < limit {
            let keep = (limit - retained.len()).min(read);
            retained.extend_from_slice(&bytes[..keep]);
        }
    }
    Ok(String::from_utf8_lossy(&retained).into_owned())
}

pub fn decode_audio_waveform_with_cancel(
    tools: &MediaTools,
    path: &Path,
    target_bins: usize,
    cancel: &PreviewCancellation,
) -> Result<Vec<(f32, f32)>, String> {
    if target_bins == 0 {
        return Err("waveform bins must be non-zero".into());
    }
    if cancel.is_cancelled() {
        return Err("waveform decode cancelled".into());
    }
    let probe = probe_media(tools, path)?;
    let sample_rate = probe
        .audio_sample_rate
        .ok_or_else(|| "media has no audio stream with a sample rate".to_string())?;
    // FFprobe's format duration gives us a stable sample-to-bin projection
    // without retaining the source PCM. If duration is unavailable, use a
    // bounded one-sample-per-bin estimate and clamp any excess into the final
    // bin; the returned allocation remains bounded by target_bins in either case.
    let estimated_total_samples = if probe.duration.is_finite() && probe.duration > 0.0 {
        let estimate = probe.duration * f64::from(sample_rate);
        if estimate.is_finite() && estimate >= 1.0 && estimate <= u64::MAX as f64 {
            estimate.ceil() as u64
        } else {
            target_bins as u64
        }
    } else {
        target_bins as u64
    }
    .max(1);
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-vn", "-ac", "1", "-ar"])
        .arg(sample_rate.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start FFmpeg waveform decoder: {error}"))?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("FFmpeg waveform stdout was not captured".into());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("FFmpeg waveform stderr was not captured".into());
        }
    };
    let worker_cancel = cancel.clone();
    let stdout_reader = std::thread::spawn(move || -> Result<Vec<(f32, f32)>, String> {
        let mut reader = BufReader::new(stdout);
        let mut bytes = [0_u8; WAVEFORM_READ_BUFFER_BYTES];
        let mut carry = [0_u8; std::mem::size_of::<f32>()];
        let mut carry_len = 0_usize;
        let mut sample_index = 0_u64;
        let mut peaks = vec![(f32::INFINITY, f32::NEG_INFINITY); target_bins];

        let mut accept_sample = |sample: f32| {
            if !sample.is_finite() {
                return;
            }
            let bin = (sample_index.saturating_mul(target_bins as u64) / estimated_total_samples)
                .min(target_bins.saturating_sub(1) as u64) as usize;
            let (min_value, max_value) = &mut peaks[bin];
            *min_value = (*min_value).min(sample);
            *max_value = (*max_value).max(sample);
            sample_index = sample_index.saturating_add(1);
        };

        loop {
            if worker_cancel.is_cancelled() {
                return Err("waveform decode cancelled".into());
            }
            let read = reader
                .read(&mut bytes)
                .map_err(|error| format!("read FFmpeg waveform output: {error}"))?;
            if read == 0 {
                break;
            }
            let mut offset = 0_usize;
            if carry_len > 0 {
                let needed = carry.len() - carry_len;
                let copied = needed.min(read);
                carry[carry_len..carry_len + copied].copy_from_slice(&bytes[..copied]);
                carry_len += copied;
                offset = copied;
                if carry_len == carry.len() {
                    accept_sample(f32::from_le_bytes(carry));
                    carry_len = 0;
                }
            }
            while offset + carry.len() <= read {
                let sample = f32::from_le_bytes(
                    bytes[offset..offset + carry.len()]
                        .try_into()
                        .expect("f32 sample chunk has four bytes"),
                );
                accept_sample(sample);
                offset += carry.len();
            }
            if offset < read {
                carry[..read - offset].copy_from_slice(&bytes[offset..read]);
                carry_len = read - offset;
            }
        }
        if carry_len != 0 {
            return Err("waveform decoder emitted an incomplete f32 sample".into());
        }
        if sample_index == 0 {
            return Err("waveform decoder produced no samples".into());
        }
        let peaks = peaks
            .into_iter()
            .filter(|(min_value, max_value)| min_value.is_finite() && max_value.is_finite())
            .collect::<Vec<_>>();
        if peaks.is_empty() {
            Err("waveform decoder produced no finite samples".into())
        } else {
            Ok(peaks)
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        drain_reader_with_prefix(stderr, WAVEFORM_STDERR_LIMIT_BYTES as usize)
            .map_err(|error| error.to_string())
    });
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("waveform decode cancelled".into());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for FFmpeg waveform decoder: {error}"))?
        {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let peaks = stdout_reader
        .join()
        .map_err(|_| "FFmpeg waveform stdout reader panicked".to_string())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "FFmpeg waveform stderr reader panicked".to_string())??;
    if cancel.is_cancelled() {
        return Err("waveform decode cancelled".into());
    }
    if !status.success() {
        return Err(stderr);
    }
    Ok(peaks)
}

/// Starts an external local preview player for a source range.
pub fn spawn_preview_player(
    tools: &MediaTools,
    path: &Path,
    start: f64,
    duration: Option<f64>,
) -> Result<Child, String> {
    if !path.is_file() {
        return Err(format!("media does not exist: {}", path.display()));
    }
    let mut command = Command::new(&tools.ffplay);
    command.args([
        "-hide_banner",
        "-autoexit",
        "-loglevel",
        "warning",
        "-ss",
        &format!("{:.6}", start.max(0.0)),
    ]);
    if let Some(duration) = duration.filter(|duration| *duration > 0.0) {
        command.args(["-t", &format!("{duration:.6}")]);
    }
    command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start FFplay: {error}"))
}

/// Creates a deterministic video-only sequence export plan from the first video track.
pub fn build_timeline_export_plan(
    project: &VideoProject,
    tools: &MediaTools,
    output: impl Into<PathBuf>,
) -> Result<TimelineExportPlan, String> {
    let clips = project
        .tracks
        .iter()
        .filter(|track| track.track_type == TrackType::Video && !track.muted)
        .flat_map(|track| track.clips.iter())
        .filter(|clip| clip.enabled && !clip.source_path.trim().is_empty())
        .collect::<Vec<_>>();
    if clips.is_empty() {
        return Err("timeline contains no enabled video clips with source paths".into());
    }
    let output = output.into();
    let mut arguments = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
    for clip in &clips {
        if !Path::new(&clip.source_path).is_file() {
            return Err(format!("missing clip source: {}", clip.source_path));
        }
        arguments.extend([
            "-ss".into(),
            format!("{:.6}", clip.in_point),
            "-t".into(),
            format!("{:.6}", clip.source_span()),
            "-i".into(),
            clip.source_path.clone(),
        ]);
    }
    let mut filters = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        filters.push(format!(
            "[{index}:v]setpts=(PTS-STARTPTS)/{:.8},scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,fps={:.6}[v{index}]",
            clip.playback_rate, project.width, project.height, project.width, project.height, project.frame_rate
        ));
    }
    let inputs = (0..clips.len())
        .map(|index| format!("[v{index}]"))
        .collect::<String>();
    filters.push(format!("{inputs}concat=n={}:v=1:a=0[vout]", clips.len()));
    let duration = clips.iter().map(|clip| clip.duration).sum();
    arguments.extend([
        "-filter_complex".into(),
        filters.join(";"),
        "-map".into(),
        "[vout]".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        output.to_string_lossy().into_owned(),
    ]);
    Ok(TimelineExportPlan {
        executable: tools.ffmpeg.clone(),
        arguments,
        output,
        duration,
    })
}

/// Executes a timeline export and reports normalized progress.
pub fn execute_timeline_export<F>(plan: &TimelineExportPlan, progress: F) -> Result<(), String>
where
    F: FnMut(f32),
{
    execute_timeline_export_with_cancel(plan, progress, &ExportCancellation::default())
}

static NEXT_EXPORT_TEMP: AtomicU64 = AtomicU64::new(0);

fn export_temp_path(output: &Path) -> PathBuf {
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("loom-video-export.mp4");
    // FFmpeg selects its muxer from the output suffix. Keep the temporary
    // marker in the stem while retaining the destination extension so the
    // worker can encode successfully before the atomic rename.
    let extension = output
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("mp4");
    let nonce = NEXT_EXPORT_TEMP.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{file_name}.loom-video-{}-{nonce}.{extension}",
        std::process::id(),
    ))
}

fn export_arguments_with_output(plan: &TimelineExportPlan, temporary: &Path) -> Vec<String> {
    let mut arguments = plan.arguments.clone();
    let final_output = plan.output.to_string_lossy();
    let temporary = temporary.to_string_lossy().into_owned();
    if arguments
        .last()
        .is_some_and(|argument| argument == final_output.as_ref())
    {
        if let Some(argument) = arguments.last_mut() {
            *argument = temporary;
        }
    } else {
        arguments.push(temporary);
    }
    arguments
}

struct TemporaryExport {
    path: PathBuf,
    committed: bool,
}

impl TemporaryExport {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }
}

impl Drop for TemporaryExport {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Executes a timeline export with cooperative cancellation.
pub fn execute_timeline_export_with_cancel<F>(
    plan: &TimelineExportPlan,
    mut progress: F,
    cancel: &ExportCancellation,
) -> Result<(), String>
where
    F: FnMut(f32),
{
    if cancel.is_cancelled() {
        return Err("timeline export cancelled".into());
    }
    if let Some(parent) = plan.output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    let temporary = export_temp_path(&plan.output);
    let _ = std::fs::remove_file(&temporary);
    let mut temporary_output = TemporaryExport::new(temporary.clone());
    let arguments = export_arguments_with_output(plan, &temporary);
    let mut child = Command::new(&plan.executable)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start timeline export: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "FFmpeg stdout was not captured".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "FFmpeg stderr was not captured".to_string())?;
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = std::io::Read::read_to_string(&mut reader, &mut text);
        text
    });
    let (line_sender, line_receiver) = std::sync::mpsc::channel();
    let stdout_reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line_sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut last = 0.0;
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("timeline export cancelled".into());
        }
        match line_receiver.recv_timeout(std::time::Duration::from_millis(20)) {
            Ok(line_result) => {
                let line = match line_result {
                    Ok(line) => line,
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stdout_reader.join();
                        let _ = stderr_reader.join();
                        return Err(error.to_string());
                    }
                };
                if let Some(value) = line
                    .strip_prefix("out_time_us=")
                    .and_then(|value| value.parse::<f64>().ok())
                {
                    last =
                        (value / 1_000_000.0 / plan.duration.max(0.001)).clamp(0.0, 0.999) as f32;
                    progress(last);
                } else if line == "progress=end" {
                    last = 1.0;
                    progress(1.0);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(format!("wait for FFmpeg export: {error}"));
                }
            },
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("wait for FFmpeg export: {error}"));
            }
        }
    };
    let _ = stdout_reader.join();
    let stderr = stderr_reader
        .join()
        .unwrap_or_else(|_| "FFmpeg stderr reader panicked".into());
    if cancel.is_cancelled() {
        Err("timeline export cancelled".into())
    } else if status.success() {
        if last < 1.0 {
            progress(1.0);
        }
        if !temporary.is_file() {
            return Err("FFmpeg completed without producing an output file".into());
        }
        std::fs::rename(&temporary, &plan.output)
            .map_err(|error| format!("commit timeline export: {error}"))?;
        temporary_output.committed = true;
        Ok(())
    } else {
        let _ = std::fs::remove_file(&temporary);
        Err(stderr)
    }
}

/// One camera angle available to a multicam clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MulticamAngle {
    /// Identifier of the source clip backing this angle.
    pub clip_id: String,
    pub label: String,
}

/// A cut switching the active multicam angle at a timeline time (seconds).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MulticamCut {
    pub timeline_time: f64,
    pub angle_index: usize,
}

/// Returns the index of the active camera angle at `time` given ordered or unordered cuts.
/// Before the first cut, angle 0 is active. Returns None when `angles` is empty.
pub fn active_angle_at(angles: &[MulticamAngle], cuts: &[MulticamCut], time: f64) -> Option<usize> {
    if angles.is_empty() {
        return None;
    }
    cuts.iter()
        .filter(|cut| cut.timeline_time <= time)
        .min_by(|a, b| {
            b.timeline_time
                .partial_cmp(&a.timeline_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|cut| cut.angle_index)
        .filter(|index| *index < angles.len())
        .or(Some(0))
}

/// Parameters for generating a ducking automation envelope over a music clip.
#[derive(Debug, Clone, PartialEq)]
pub struct DuckingConfig {
    /// Amount of gain reduction applied while dialogue is active, in decibels (positive number, e.g. 12).
    pub reduction_db: f64,
    /// Attack time in seconds (ramp into full reduction).
    pub attack_seconds: f64,
    /// Release time in seconds (ramp back to unity).
    pub release_seconds: f64,
    /// Extra padding added before/after each dialogue region, seconds >= 0.
    pub padding_seconds: f64,
}

/// Generates envelope keys `(time_seconds, gain_db)` implementing a dialogue-ducking curve over
/// `dialogue_regions` given as `(start, end)` pairs. Regions may be unsorted but must satisfy
/// `start < end`, otherwise an error is returned. Regions are processed sorted by start without
/// merging overlaps; when two regions' keys land on identical times, the later region's key
/// overwrites the earlier one. Each padded region contributes four keys: attack ramp start at
/// 0 dB, padded region start at full reduction, padded region end holding full reduction, and
/// the release ramp end back at 0 dB. Output is sorted by time ascending.
pub fn generate_ducking_envelope(
    config: &DuckingConfig,
    dialogue_regions: &[(f64, f64)],
) -> Result<Vec<(f64, f64)>, String> {
    if config.reduction_db.is_nan() || config.reduction_db <= 0.0 {
        return Err("reduction_db must be greater than 0".to_string());
    }
    if config.attack_seconds.is_nan() || config.attack_seconds < 0.0 {
        return Err("attack_seconds must be >= 0".to_string());
    }
    if config.release_seconds.is_nan() || config.release_seconds < 0.0 {
        return Err("release_seconds must be >= 0".to_string());
    }
    if config.padding_seconds.is_nan() || config.padding_seconds < 0.0 {
        return Err("padding_seconds must be >= 0".to_string());
    }

    let mut regions: Vec<(f64, f64)> = dialogue_regions.to_vec();
    regions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Keys are pushed region by region (sorted by start), then stably sorted by time.
    // dedup_by keeps the last of each identical-time run, so a later region's key
    // overwrites an earlier one at the same time, matching AudioEnvelope::add_key's
    // replace-at-same-offset behaviour.
    let mut keys: Vec<(f64, f64)> = Vec::new();
    for (start, end) in regions {
        if start.is_nan() || end.is_nan() || start >= end {
            return Err(format!(
                "dialogue region must satisfy start < end, got ({start}, {end})"
            ));
        }
        let padded_start = start - config.padding_seconds;
        let padded_end = end + config.padding_seconds;
        keys.push((padded_start - config.attack_seconds, 0.0));
        keys.push((padded_start, -config.reduction_db));
        keys.push((padded_end, -config.reduction_db));
        keys.push((padded_end + config.release_seconds, 0.0));
    }

    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    keys.dedup_by(|a, b| a.0 == b.0);
    Ok(keys)
}

/// A proxy (edit-friendly stand-in) linked to a source media clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyLink {
    /// Source clip identifier this proxy stands in for.
    pub source_clip_id: String,
    pub proxy_path: String,
    /// Proxy resolution scale relative to source (e.g. 0.5).
    pub scale: f32,
    /// Codec used by the proxy file (informational, e.g. "prores_proxy").
    pub codec: String,
}

/// Registry of proxy links within a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProxyRegistry {
    pub links: Vec<ProxyLink>,
}

impl ProxyRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self { links: Vec::new() }
    }

    /// Adds or replaces the link for a source clip id. Scale must be in (0, 1] and paths
    /// non-empty else Err.
    pub fn set_link(&mut self, link: ProxyLink) -> Result<(), String> {
        if link.source_clip_id.is_empty() {
            return Err("source_clip_id must not be empty".to_string());
        }
        if link.proxy_path.is_empty() {
            return Err("proxy_path must not be empty".to_string());
        }
        if !(link.scale > 0.0 && link.scale <= 1.0) {
            return Err(format!("scale must be in (0, 1], got {}", link.scale));
        }
        match self
            .links
            .iter_mut()
            .find(|l| l.source_clip_id == link.source_clip_id)
        {
            Some(existing) => *existing = link,
            None => self.links.push(link),
        }
        Ok(())
    }

    /// Removes the link for a source clip; true when removed.
    pub fn remove_link(&mut self, source_clip_id: &str) -> bool {
        let before = self.links.len();
        self.links
            .retain(|link| link.source_clip_id != source_clip_id);
        self.links.len() != before
    }

    /// The active proxy path for a source clip, if any.
    pub fn proxy_for(&self, source_clip_id: &str) -> Option<&str> {
        self.links
            .iter()
            .find(|link| link.source_clip_id == source_clip_id)
            .map(|link| link.proxy_path.as_str())
    }

    /// Marks missing proxies: given a predicate answering whether a path exists on disk,
    /// returns ids of clips whose proxy path fails the predicate.
    pub fn stale_links<F: Fn(&str) -> bool>(&self, path_exists: F) -> Vec<String> {
        self.links
            .iter()
            .filter(|link| !path_exists(&link.proxy_path))
            .map(|link| link.source_clip_id.clone())
            .collect()
    }
}

/// The playback transport direction of a shuttle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShuttleDirection {
    #[default]
    Forward,
    Reverse,
}

/// J/K/L-style shuttle transport state: each press of the shuttle key steps through the
/// speed ladder, `Stop` resets to paused. Speeds are playback-rate multipliers.
#[derive(Debug, Clone, PartialEq)]
pub struct ShuttleState {
    pub direction: ShuttleDirection,
    /// Zero-based index into the speed ladder; paused when stopped.
    pub step: usize,
}

/// The standard shuttle speed ladder in playback-rate multiples.
pub const SHUTTLE_SPEED_LADDER: &[f64] = &[0.5, 1.0, 2.0, 4.0, 8.0];

impl Default for ShuttleState {
    fn default() -> Self {
        Self::stopped()
    }
}

impl ShuttleState {
    /// Paused shuttle at the base of the forward ladder.
    pub fn stopped() -> Self {
        Self {
            direction: ShuttleDirection::Forward,
            step: 0,
        }
    }

    /// True when the shuttle is fully stopped (step 0 regardless of direction).
    pub fn is_stopped(&self) -> bool {
        self.step == 0
    }

    /// Signed playback rate: positive forward, negative reverse; zero when stopped.
    pub fn rate(&self) -> f64 {
        if self.is_stopped() {
            0.0
        } else {
            let magnitude = SHUTTLE_SPEED_LADDER
                .get(self.step - 1)
                .copied()
                .unwrap_or(*SHUTTLE_SPEED_LADDER.last().unwrap());
            match self.direction {
                ShuttleDirection::Forward => magnitude,
                ShuttleDirection::Reverse => -magnitude,
            }
        }
    }

    /// Advances one rung up the ladder in the current direction.
    pub fn press_shuttle_forward(&mut self) {
        self.direction = ShuttleDirection::Forward;
        self.step = (self.step + 1).min(SHUTTLE_SPEED_LADDER.len());
    }

    /// Advances one rung up the reverse ladder.
    pub fn press_shuttle_reverse(&mut self) {
        self.direction = ShuttleDirection::Reverse;
        self.step = (self.step + 1).min(SHUTTLE_SPEED_LADDER.len());
    }

    /// Stops playback and returns to the neutral paused state.
    pub fn press_stop(&mut self) {
        *self = Self::stopped();
    }
}

/// Analyzes a clip's samples and returns the gain in decibels needed to bring the loudest
/// sample up to `target_peak` (0..=1). Returns Ok(0.0) for silent or empty input. A positive
/// result means the clip should be amplified; negative means attenuated.
pub fn suggest_normalize_gain(samples: &[f32], target_peak: f32) -> Result<f64, String> {
    if !(0.0..=1.0).contains(&target_peak) {
        return Err("target peak must be within [0, 1]".into());
    }
    let peak = samples
        .iter()
        .fold(0.0f32, |max, s| max.max(s.abs()))
        .clamp(0.0, 1.0);
    if peak <= 1e-6 {
        return Ok(0.0);
    }
    let linear = (target_peak as f64 / peak as f64).min(f64::from(u16::MAX));
    Ok(20.0 * linear.log10())
}

/// Clamps a suggested gain into the safe range applied before preview playback.
pub fn clamp_gain_db(gain_db: f64, max_boost_db: f64) -> f64 {
    gain_db.clamp(-60.0, max_boost_db.max(0.0))
}

/// One timed subtitle cue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleCue {
    /// Zero-based sequential cue number.
    pub index: usize,
    /// Start time in seconds from the media start.
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// Cue text; multi-line cues keep their interior newlines.
    pub text: String,
}

/// Parses SRT subtitle content into cues. Accepts \r\n and \n. Timestamps use
/// HH:MM:SS,mmm (also tolerate '.' as millisecond separator). Blocks separated by one or
/// more blank lines. Returns Err naming the malformed block when timestamps or structure are invalid.
pub fn parse_srt(content: &str) -> Result<Vec<SubtitleCue>, String> {
    let normalized = content.replace("\r\n", "\n");
    let mut cues = Vec::new();
    let mut block_lines: Vec<&str> = Vec::new();
    let mut block_number = 0usize;

    let mut flush_block = |block_lines: &mut Vec<&str>| -> Result<(), String> {
        if block_lines.is_empty() {
            return Ok(());
        }
        block_number += 1;
        let block = block_lines.join("\n");
        block_lines.clear();
        cues.push(parse_srt_block(&block, block_number)?);
        Ok(())
    };

    for line in normalized.split('\n') {
        if line.trim().is_empty() {
            flush_block(&mut block_lines)?;
        } else {
            block_lines.push(line);
        }
    }
    flush_block(&mut block_lines)?;
    Ok(cues)
}

/// Parses a single blank-line-delimited SRT block into one cue.
fn parse_srt_block(block: &str, block_number: usize) -> Result<SubtitleCue, String> {
    let lines: Vec<&str> = block.lines().collect();
    if lines.len() < 2 {
        return Err(format!(
            "subtitle block {block_number}: expected an index line and a timestamp line, found {} line(s)",
            lines.len()
        ));
    }

    let first_trimmed = lines[0].trim();
    let looks_like_index = !first_trimmed.is_empty()
        && first_trimmed.chars().all(|c| c.is_ascii_digit())
        && lines[1].contains("-->");
    let body_start = if looks_like_index {
        1
    } else if lines[0].contains("-->") {
        0
    } else {
        return Err(format!(
            "subtitle block {block_number}: unexpected opening line {first_trimmed:?}; expected a cue index or a timestamp"
        ));
    };
    let index = if looks_like_index {
        first_trimmed.parse::<usize>().map_err(|_| {
            format!("subtitle block {block_number}: invalid cue index {first_trimmed:?}")
        })?
    } else {
        block_number.saturating_sub(1)
    };

    let (start_seconds, end_seconds) = parse_srt_time_line(lines[body_start], block_number)?;
    if end_seconds <= start_seconds {
        return Err(format!(
            "subtitle block {block_number}: end timestamp {end_seconds}s must be greater than start {start_seconds}s"
        ));
    }
    let text = lines[body_start + 1..].join("\n");
    Ok(SubtitleCue {
        index,
        start_seconds,
        end_seconds,
        text,
    })
}

/// Parses an `HH:MM:SS,mmm --> HH:MM:SS,mmm` timing line into start/end seconds.
fn parse_srt_time_line(line: &str, block_number: usize) -> Result<(f64, f64), String> {
    const ARROW: &str = "-->";
    let arrow = line.find(ARROW).ok_or_else(|| {
        format!("subtitle block {block_number}: missing '{ARROW}' separator in {line:?}")
    })?;
    let start = parse_srt_timestamp(&line[..arrow], block_number)?;
    let end = parse_srt_timestamp(&line[arrow + ARROW.len()..], block_number)?;
    Ok((start, end))
}

/// Parses one `HH:MM:SS,mmm` (or `HH:MM:SS.mmm`) timestamp into seconds.
fn parse_srt_timestamp(raw: &str, block_number: usize) -> Result<f64, String> {
    let text = raw.trim();
    let invalid = || format!("subtitle block {block_number}: invalid timestamp {text:?}");

    let (clock, fraction) = match text.split_once([',', '.']) {
        Some((clock, fraction)) => (clock, fraction),
        None => (text, ""),
    };

    let mut clock_parts = clock.split(':');
    let hours: u64 = clock_parts
        .next()
        .unwrap_or_default()
        .trim()
        .parse()
        .map_err(|_| invalid())?;
    let minutes: u64 = clock_parts
        .next()
        .ok_or_else(invalid)?
        .trim()
        .parse()
        .map_err(|_| invalid())?;
    let seconds: u64 = clock_parts
        .next()
        .ok_or_else(invalid)?
        .trim()
        .parse()
        .map_err(|_| invalid())?;
    if clock_parts.next().is_some() {
        return Err(invalid());
    }

    let millis = if fraction.is_empty() {
        0
    } else {
        let digits = fraction.trim();
        if digits.len() > 3 || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        digits.parse::<u64>().map_err(|_| invalid())? * 10u64.pow(3 - digits.len() as u32)
    };

    Ok((hours * 3600 + minutes * 60 + seconds) as f64 + millis as f64 / 1000.0)
}

/// Formats seconds as an `HH:MM:SS,mmm` SRT timestamp, rounded to the nearest millisecond.
fn format_srt_timestamp(seconds: f64) -> String {
    let total_millis = (seconds.max(0.0) * 1000.0).round() as u64;
    let millis = total_millis % 1000;
    let whole_seconds = total_millis / 1000;
    format!(
        "{:02}:{:02}:{:02},{millis:03}",
        whole_seconds / 3600,
        (whole_seconds / 60) % 60,
        whole_seconds % 60
    )
}

/// Serializes cues back to standard SRT text with \r\n line endings and trailing newline.
pub fn write_srt(cues: &[SubtitleCue]) -> String {
    let mut out = String::new();
    for cue in cues {
        out.push_str(&cue.index.to_string());
        out.push_str("\r\n");
        out.push_str(&format_srt_timestamp(cue.start_seconds));
        out.push_str(" --> ");
        out.push_str(&format_srt_timestamp(cue.end_seconds));
        out.push_str("\r\n");
        out.push_str(&cue.text.replace('\n', "\r\n"));
        out.push_str("\r\n\r\n");
    }
    out
}

/// One caption placed on the caption lane.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionEntry {
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// Caption text; multi-line cue text collapses to single spaces.
    pub text: String,
}

/// Converts cues into sorted, non-overlapping caption entries. Overlapping cues are rejected
/// with Err naming the conflict; zero/negative durations err; text is whitespace-collapsed.
pub fn captions_from_cues(cues: &[SubtitleCue]) -> Result<Vec<CaptionEntry>, String> {
    let mut ordered: Vec<&SubtitleCue> = cues.iter().collect();
    ordered.sort_by(|a, b| {
        a.start_seconds
            .partial_cmp(&b.start_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.end_seconds
                    .partial_cmp(&b.end_seconds)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.index.cmp(&b.index))
    });

    let mut entries = Vec::with_capacity(ordered.len());
    let mut previous: Option<&SubtitleCue> = None;
    for cue in ordered {
        if !cue.start_seconds.is_finite()
            || !cue.end_seconds.is_finite()
            || cue.end_seconds <= cue.start_seconds
        {
            return Err(format!(
                "caption cue {}: invalid range [{:.3}s .. {:.3}s]; end must be after start",
                cue.index, cue.start_seconds, cue.end_seconds
            ));
        }
        if let Some(previous_cue) = previous {
            if cue.start_seconds < previous_cue.end_seconds {
                return Err(format!(
                    "caption cue {} [{:.3}s .. {:.3}s] overlaps cue {} [{:.3}s .. {:.3}s]",
                    cue.index,
                    cue.start_seconds,
                    cue.end_seconds,
                    previous_cue.index,
                    previous_cue.start_seconds,
                    previous_cue.end_seconds
                ));
            }
        }
        entries.push(CaptionEntry {
            start_seconds: cue.start_seconds,
            end_seconds: cue.end_seconds,
            text: cue.text.split_whitespace().collect::<Vec<_>>().join(" "),
        });
        previous = Some(cue);
    }
    Ok(entries)
}

/// Finds the active caption at a playhead time (binary search); None between captions.
pub fn active_caption_at(captions: &[CaptionEntry], time_seconds: f64) -> Option<&CaptionEntry> {
    let up_to = captions.partition_point(|entry| entry.start_seconds <= time_seconds);
    let candidate = captions.get(up_to.checked_sub(1)?)?;
    (candidate.end_seconds > time_seconds).then_some(candidate)
}

/// Host-side reference to an installed motion template with resolved parameter values.
///
/// This is the Video-side consumption shape for Motion templates (M9): the timeline stores
/// plain local data and never depends on the Motion crate itself. Parameter values are
/// strings because the template host performs final typed coercion at render time.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionTemplateBinding {
    /// Installed template identifier from the template library.
    pub template_id: String,
    /// Template schema version the binding was authored against.
    pub schema_version: u32,
    /// Resolved parameter name/value pairs (already validated against the template).
    pub parameters: Vec<(String, String)>,
    /// Timeline placement seconds.
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

impl MotionTemplateBinding {
    /// Validates the binding: non-empty `template_id`, positive duration, non-negative
    /// start, and parameter names that are unique and non-empty. Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.template_id.is_empty() {
            return Err("template_id must be non-empty".to_string());
        }
        if !self.duration_seconds.is_finite() || self.duration_seconds <= 0.0 {
            return Err(format!(
                "duration_seconds {} must be positive",
                self.duration_seconds
            ));
        }
        if !self.start_seconds.is_finite() || self.start_seconds < 0.0 {
            return Err(format!(
                "start_seconds {} must be non-negative",
                self.start_seconds
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for (name, _) in &self.parameters {
            if name.is_empty() {
                return Err("parameter names must be non-empty".to_string());
            }
            if !seen.insert(name.as_str()) {
                return Err(format!("duplicate parameter name {name:?}"));
            }
        }
        Ok(())
    }

    /// The value bound to a parameter name, if present.
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// True when this binding was authored against a different schema version than the
    /// installed template now provides (needs migration review).
    pub fn needs_migration(&self, installed_schema_version: u32) -> bool {
        self.schema_version != installed_schema_version
    }
}

/// One CMX3600 edit record derived from a timeline clip.
#[derive(Debug, Clone, PartialEq)]
pub struct EdlRecord {
    /// 1-based event number.
    pub event_number: u32,
    /// Reel identifier derived from the clip source name.
    pub reel: String,
    pub source_in: String,
    pub source_out: String,
    pub record_in: String,
    pub record_out: String,
    pub clip_name: String,
}

/// Derives an EDL reel name from a clip name: uppercased, truncated to 8 characters, with
/// non-alphanumeric characters replaced by '_'.
fn edl_reel_name(clip_name: &str) -> String {
    let upper = clip_name.to_uppercase();
    let mut reel: String = upper
        .chars()
        .take(8)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if reel.is_empty() {
        reel.push('_');
    }
    reel
}

/// Builds CMX3600-style records from ordered timeline clips. Each tuple is
/// (clip_name, source_offset_seconds, start_seconds, duration_seconds). Source in equals the
/// clip's source offset; source out adds the duration; record positions come from the
/// timeline. fps must be positive and finite else Err.
pub fn build_edl_records(
    clips: &[(String, f64, f64, f64)],
    fps: f64,
) -> Result<Vec<EdlRecord>, String> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err("frame rate must be positive and finite".into());
    }
    let mut records = Vec::with_capacity(clips.len());
    for (index, (clip_name, source_offset, start, duration)) in clips.iter().enumerate() {
        let source_in = Timecode::from_seconds(*source_offset, fps).format_smpte();
        let source_out =
            Timecode::from_seconds(source_offset + duration.max(0.0), fps).format_smpte();
        let record_in = Timecode::from_seconds(*start, fps).format_smpte();
        let record_out = Timecode::from_seconds(start + duration.max(0.0), fps).format_smpte();
        records.push(EdlRecord {
            event_number: index as u32 + 1,
            reel: edl_reel_name(clip_name),
            source_in,
            source_out,
            record_in,
            record_out,
            clip_name: clip_name.clone(),
        });
    }
    Ok(records)
}

/// Serializes records into CMX3600 text: one event line with fixed-width columns followed by
/// a `* FROM CLIP NAME:` comment per record, each line ending with \r\n.
pub fn write_edl(records: &[EdlRecord]) -> String {
    let mut output = String::new();
    for record in records {
        output.push_str(&format!(
            "{:03}  {:<8}  V  C        {:<11} {:<11} {:<11} {:<11}\r\n",
            record.event_number,
            record.reel,
            record.source_in,
            record.source_out,
            record.record_in,
            record.record_out
        ));
        output.push_str(&format!("* FROM CLIP NAME: {}\r\n\r\n", record.clip_name));
    }
    output
}

/// FNV-1a 64-bit hash over bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl VideoProject {
    /// Stable digest over project frame rate and every track/clip's identity, timing,
    /// speed, and source offset in order. Uses [`fnv1a64`].
    ///
    /// Feeds `"proj:<frame_rate>"`, then per track `"track:<id>:<name>"`, then per clip
    /// `"clip:<id>:<name>:<start>:<dur>:<rate>:<in>:<out>"` in stored order, plus one
    /// `"marker:<id>:<time>:<label>"` line per timeline marker. Markers participate by
    /// choice: they are user-authored annotations added through [`VideoProject::add_marker`],
    /// so adding, moving, or removing one must change the digest. Derived caches such as
    /// clip `proxy_path` and presentation-only metadata (`color_tag`, effect parameters,
    /// caption cues) deliberately do not participate.
    pub fn timeline_digest(&self) -> u64 {
        let mut feed = format!("proj:{fps}\n", fps = self.frame_rate);
        for track in &self.tracks {
            feed.push_str(&format!(
                "track:{t_id}:{t_name}\n",
                t_id = track.id,
                t_name = track.name
            ));
            for clip in &track.clips {
                feed.push_str(&format!(
                    "clip:{c_id}:{c_name}:{start}:{dur}:{rate}:{inp}:{outp}\n",
                    c_id = clip.id,
                    c_name = clip.name,
                    start = clip.start_time,
                    dur = clip.duration,
                    rate = clip.playback_rate,
                    inp = clip.in_point,
                    outp = clip.out_point
                ));
            }
        }
        for marker in &self.markers {
            feed.push_str(&format!(
                "marker:{m_id}:{m_time}:{m_label}\n",
                m_id = marker.id,
                m_time = marker.time,
                m_label = marker.label
            ));
        }
        fnv1a64(feed.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn video_timeline_digest_stability() {
        let mut project = VideoProject::new("proj-digest", "Digest Edit");
        let mut intro = Clip::new("c1", "Intro.mov", 6.0);
        intro.source_path = "intro.mov".into();
        intro.set_playback_rate(2.0).unwrap();
        let mut broll = Clip::new("c2", "B-Roll.mov", 4.0);
        broll.in_point = 1.0;
        broll.out_point = 5.0;
        broll.sync_duration().unwrap();
        project.tracks[0].insert_clip(intro).unwrap();
        project.tracks[0].insert_clip(broll).unwrap();

        // Stable across repeated calls.
        let baseline = project.timeline_digest();
        assert_eq!(baseline, project.timeline_digest());

        // Trimming a clip changes the digest.
        let mut trimmed = project.clone();
        trimmed.tracks[0].clips[0].trim_out(4.0).unwrap();
        assert_ne!(trimmed.timeline_digest(), baseline);

        // Splitting a clip changes the digest.
        let mut split = project.clone();
        split.split_clip(0, "c2", 2.0).unwrap();
        assert_ne!(split.timeline_digest(), baseline);

        // Adding a marker changes the digest: markers are user-authored annotations
        // (added through VideoProject::add_marker), a choice documented on
        // timeline_digest itself.
        let mut marked = project.clone();
        marked
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 1.0,
                label: "Cut Here".into(),
                color: "#ef4444".into(),
            })
            .unwrap();
        assert_ne!(marked.timeline_digest(), baseline);

        // An empty project digests differently from a populated one.
        let empty = VideoProject::new("proj-empty", "Empty");
        assert_ne!(empty.timeline_digest(), baseline);
    }

    #[test]
    fn stderr_prefix_reader_drains_beyond_retained_limit() {
        let limit = WAVEFORM_STDERR_LIMIT_BYTES as usize;
        let mut input = vec![b'x'; limit + WAVEFORM_READ_BUFFER_BYTES + 17];
        input.extend_from_slice(b"tail");
        let mut reader = Cursor::new(input.clone());

        let retained = drain_reader_with_prefix(&mut reader, limit).unwrap();

        assert_eq!(reader.position() as usize, input.len());
        assert_eq!(retained.len(), limit);
        assert!(retained.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn test_video_creation() {
        let proj = VideoProject::new("v-1", "Documentary Edit");
        assert_eq!(proj.frame_rate, 30.0);
        assert_eq!(proj.tracks.len(), 2);
    }

    #[test]
    fn test_add_clips() {
        let mut proj = VideoProject::new("v-1", "Documentary Edit");
        proj.tracks[0].add_clip(Clip::new("clip-1", "B-Roll Shot 1.mp4", 5.0));
        assert_eq!(proj.total_clips(), 1);
    }

    #[test]
    fn test_select_track_rejects_invalid_index() {
        let mut proj = VideoProject::new("v-1", "Documentary Edit");
        assert!(proj.select_track(1));
        assert!(!proj.select_track(2));
        assert_eq!(proj.active_track_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut proj = VideoProject::new("v-test", "Short Film");
        proj.tracks[0].add_clip(Clip::new("c1", "Scene1.mp4", 12.0));
        let bytes = save_video_project(&proj).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Video);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_video_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Short Film");
        assert_eq!(loaded.total_clips(), 1);
    }

    #[test]
    fn split_trim_move_and_ripple_edits_are_consistent() {
        let mut project = VideoProject::new("video-edit", "Edit Test");
        let mut clip = Clip::new("clip", "Source", 10.0);
        clip.source_path = "source.mov".into();
        project.tracks[0].insert_clip(clip).unwrap();
        let (left, right) = project.split_clip(0, "clip", 4.0).unwrap();
        assert_eq!(project.tracks[0].clips.len(), 2);
        assert_eq!(project.tracks[0].clips[0].duration, 4.0);
        project.move_clip(0, 0, &right, 8.0, false).unwrap();
        assert_eq!(project.duration(), 14.0);
        project.tracks[0].remove_clip(&left, true).unwrap();
        assert_eq!(project.tracks[0].clips[0].start_time, 4.0);
    }

    #[test]
    fn playback_rate_render_plan_and_edl_are_real() {
        let mut project = VideoProject::new("video-plan", "Plan");
        let mut clip = Clip::new("c1", "Shot", 8.0);
        clip.source_path = "shot.mov".into();
        clip.set_playback_rate(2.0).unwrap();
        project.tracks[0].insert_clip(clip).unwrap();
        assert_eq!(project.duration(), 4.0);
        let plan = project.render_plan();
        assert_eq!(plan[0].playback_rate, 2.0);
        assert!(project.to_edl().contains("FROM CLIP NAME: c1"));
    }

    #[test]
    fn captions_markers_and_overlap_detection_are_sorted() {
        let mut project = VideoProject::new("video-meta", "Metadata");
        project
            .add_marker(TimelineMarker {
                id: "m2".into(),
                time: 2.0,
                label: "Second".into(),
                color: "#fff".into(),
            })
            .unwrap();
        project
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 1.0,
                label: "First".into(),
                color: "#fff".into(),
            })
            .unwrap();
        project
            .add_caption(CaptionCue {
                id: "cap".into(),
                start: 0.0,
                end: 1.5,
                text: "Hello".into(),
                language: "en".into(),
            })
            .unwrap();
        assert_eq!(project.markers[0].id, "m1");
        assert_eq!(project.duration(), 1.5);
        let mut first = Clip::new("a", "A", 2.0);
        first.start_time = 0.0;
        let mut second = Clip::new("b", "B", 2.0);
        second.start_time = 1.0;
        project.tracks[0].insert_clip(first).unwrap();
        project.tracks[0].insert_clip(second).unwrap();
        assert_eq!(project.tracks[0].overlaps(), vec![("a".into(), "b".into())]);
    }

    #[test]
    fn session_history_restores_timeline_mutations() {
        let mut session = VideoSession::new(VideoProject::new("video", "Video"));
        session.checkpoint();
        session.project.tracks[0].add_clip(Clip::new("clip", "Clip", 2.0));
        assert!(session.can_undo());
        assert!(session.undo());
        assert_eq!(session.project.total_clips(), 0);
        assert!(session.redo());
        assert_eq!(session.project.total_clips(), 1);
    }

    #[test]
    fn export_plan_maps_real_sources_and_concat_filter() {
        let directory =
            std::env::temp_dir().join(format!("loom-video-plan-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.mov");
        std::fs::write(&source, b"fixture").unwrap();
        let mut project = VideoProject::new("video", "Video");
        let mut clip = Clip::new("clip", "Clip", 2.0);
        clip.source_path = source.to_string_lossy().into_owned();
        project.tracks[0].add_clip(clip);
        let tools = MediaTools {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            ffplay: "ffplay".into(),
            version: "test".into(),
        };
        let plan = build_timeline_export_plan(&project, &tools, directory.join("out.mp4")).unwrap();
        assert!(plan
            .arguments
            .iter()
            .any(|argument| argument.contains("concat=n=1")));
        assert!(plan.arguments.iter().any(|argument| argument == "libx264"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn slip_and_slide_clip_operations() {
        let mut track = Track::new("v1", "Video", TrackType::Video);
        let mut clip = Clip::new("c1", "Clip 1", 5.0);
        clip.start_time = 2.0;
        clip.in_point = 1.0;
        clip.out_point = 6.0;
        track.insert_clip(clip).unwrap();

        // Slip by +1.0 sec
        track.slip_clip("c1", 1.0).unwrap();
        let c = &track.clips[0];
        assert_eq!(c.start_time, 2.0);
        assert_eq!(c.in_point, 2.0);
        assert_eq!(c.out_point, 7.0);

        // Slide by +3.0 sec
        track.slide_clip("c1", 3.0).unwrap();
        let c = &track.clips[0];
        assert_eq!(c.start_time, 5.0);
        assert_eq!(c.in_point, 2.0);
        assert_eq!(c.out_point, 7.0);
    }

    #[test]
    fn clip_delete_and_ripple_delete_operations() {
        let mut project = VideoProject::new("v-test", "Video Test");
        let mut c1 = Clip::new("c1", "Clip 1", 3.0);
        c1.start_time = 0.0;
        let mut c2 = Clip::new("c2", "Clip 2", 4.0);
        c2.start_time = 3.0;
        let mut c3 = Clip::new("c3", "Clip 3", 2.0);
        c3.start_time = 7.0;

        project.tracks[0].clips.clear();
        project.tracks[0].add_clip(c1);
        project.tracks[0].add_clip(c2);
        project.tracks[0].add_clip(c3);
        assert_eq!(project.tracks[0].clips.len(), 3);

        // Ripple delete middle clip (c2, duration 4.0)
        let removed = project.ripple_delete_clip(0, "c2").unwrap();
        assert_eq!(removed.id, "c2");
        assert_eq!(project.tracks[0].clips.len(), 2);
        // c3 start_time should ripple from 7.0 to 3.0
        assert_eq!(project.tracks[0].clips[1].id, "c3");
        assert_eq!(project.tracks[0].clips[1].start_time, 3.0);

        // Delete first clip without rippling
        let removed1 = project.delete_clip(0, "c1").unwrap();
        assert_eq!(removed1.id, "c1");
        assert_eq!(project.tracks[0].clips.len(), 1);

        // Marker removal
        project
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 2.5,
                label: "Cut".into(),
                color: "#ff0000".into(),
            })
            .unwrap();
        assert_eq!(project.markers.len(), 1);
        assert!(project.remove_marker("m1"));
        assert_eq!(project.markers.len(), 0);
    }

    #[test]
    fn timecode_formatting_and_smpte_conversions() {
        let tc = Timecode::from_seconds(3665.5, 30.0);
        assert_eq!(tc.hours, 1);
        assert_eq!(tc.minutes, 1);
        assert_eq!(tc.seconds, 5);
        assert_eq!(tc.frames, 15);
        assert_eq!(tc.format_smpte(), "01:01:05:15");

        let secs = tc.to_seconds(30.0);
        assert!((secs - 3665.5).abs() < 1e-4);
    }

    #[test]
    fn clip_speed_and_effective_duration_scaling() {
        let mut clip = Clip::new("c1", "Shot 1", 10.0);
        assert_eq!(clip.duration, 10.0);
        assert_eq!(clip.effective_timeline_duration(), 10.0);

        // 2x Fast Forward
        clip.set_speed(2.0);
        assert_eq!(clip.playback_rate, 2.0);
        assert_eq!(clip.duration, 5.0);

        // 0.5x Slow Motion
        clip.set_speed(0.5);
        assert_eq!(clip.playback_rate, 0.5);
        assert_eq!(clip.duration, 20.0);
    }

    #[test]
    fn timeline_zoom_and_clip_count() {
        let mut project = VideoProject::new("proj-zoom", "Zoom Project");
        assert_eq!(project.total_clips(), 0);

        project.tracks[0].add_clip(Clip::new("c1", "Shot 1", 5.0));
        assert_eq!(project.total_clips(), 1);

        // 100 pixels per second
        assert_eq!(VideoProject::seconds_to_pixels(5.5, 100.0), 550.0);
        assert_eq!(VideoProject::pixels_to_seconds(550.0, 100.0), 5.5);
    }

    #[test]
    fn timeline_track_close_gaps() {
        let mut track = Track::new("v1", "Video 1", TrackType::Video);
        let mut c1 = Clip::new("c1", "Intro", 3.0);
        c1.start_time = 2.0; // gap of 2s before c1
        let mut c2 = Clip::new("c2", "Main", 5.0);
        c2.start_time = 8.0; // gap of 3s between c1 and c2

        track.add_clip(c1);
        track.add_clip(c2);

        let moved = track.close_gaps().unwrap();
        assert_eq!(moved, 2);
        assert_eq!(track.clips[0].start_time, 0.0);
        assert_eq!(track.clips[1].start_time, 3.0);
    }

    #[test]
    fn waveform_peak_decimation() {
        let samples = vec![0.1, -0.8, 0.9, -0.2, 0.5, -0.5, 0.3, -0.1];
        let peaks = compute_waveform_peaks(&samples, 2);
        assert_eq!(peaks.len(), 2);

        // First half: min is -0.8, max is 0.9
        assert_eq!(peaks[0].0, -0.8);
        assert_eq!(peaks[0].1, 0.9);

        // Second half: min is -0.5, max is 0.5
        assert_eq!(peaks[1].0, -0.5);
        assert_eq!(peaks[1].1, 0.5);
    }

    #[test]
    fn timeline_split_clip_operation() {
        let mut track = Track::new("t1", "Video 1", TrackType::Video);
        let mut clip = Clip::new("c1", "Media", 10.0);
        clip.start_time = 0.0;
        track.add_clip(clip);

        // Split clip at 4.0s
        let new_id = track.split_clip("c1", 4.0).unwrap();
        assert_eq!(track.clips.len(), 2);

        // First clip: 0.0 -> 4.0, duration 4.0
        assert_eq!(track.clips[0].id, "c1");
        assert_eq!(track.clips[0].start_time, 0.0);
        assert_eq!(track.clips[0].duration, 4.0);

        // Second clip: 4.0 -> 10.0, duration 6.0
        assert_eq!(track.clips[1].id, new_id);
        assert_eq!(track.clips[1].start_time, 4.0);
        assert_eq!(track.clips[1].duration, 6.0);
        assert_eq!(track.clips[1].in_point, 4.0);
    }

    #[test]
    fn timeline_split_clip_respects_playback_rate() {
        for (rate, split_time, expected_left_duration, expected_right_duration) in
            [(2.0, 2.0, 2.0, 3.0), (0.5, 4.0, 4.0, 16.0)]
        {
            let mut track = Track::new("t1", "Video 1", TrackType::Video);
            let mut clip = Clip::new("c1", "Media", 10.0);
            clip.set_playback_rate(rate).unwrap();
            track.add_clip(clip);

            let new_id = track.split_clip("c1", split_time).unwrap();
            let left = track.clips.iter().find(|clip| clip.id == "c1").unwrap();
            let right = track.clips.iter().find(|clip| clip.id == new_id).unwrap();

            assert!((left.start_time - 0.0).abs() < 1e-9);
            assert!((left.duration - expected_left_duration).abs() < 1e-9);
            assert!((left.in_point - 0.0).abs() < 1e-9);
            assert!((left.out_point - (split_time * rate)).abs() < 1e-9);
            assert!((right.start_time - split_time).abs() < 1e-9);
            assert!((right.duration - expected_right_duration).abs() < 1e-9);
            assert!((right.in_point - (split_time * rate)).abs() < 1e-9);
            assert!((right.out_point - 10.0).abs() < 1e-9);
            assert!((track.duration() - 10.0 / rate).abs() < 1e-9);
        }
    }

    #[test]
    fn timeline_edit_point_snapping() {
        let mut track = Track::new("t1", "Video 1", TrackType::Video);
        let mut clip = Clip::new("c1", "Media", 5.0);
        clip.start_time = 2.0; // spans 2.0 -> 7.0
        track.add_clip(clip);

        let marker = TimelineMarker {
            id: "m1".into(),
            time: 12.0,
            label: "Intro".into(),
            color: "blue".into(),
        };

        // Snapping near start boundary (2.0s)
        let snapped_start = snap_timeline_to_edit_points(
            2.05,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_start, 2.0);

        // Snapping near end boundary (7.0s)
        let snapped_end = snap_timeline_to_edit_points(
            6.98,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_end, 7.0);

        // Snapping near marker (12.0s)
        let snapped_marker = snap_timeline_to_edit_points(
            12.04,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_marker, 12.0);
    }

    #[test]
    fn roll_and_slip_editing() {
        let mut left = Clip::new("c1", "MediaA", 5.0);
        left.start_time = 0.0;
        let mut right = Clip::new("c2", "MediaB", 5.0);
        right.start_time = 5.0;

        // Roll cut by +1.0s
        roll_edit(&mut left, &mut right, 1.0).unwrap();
        assert_eq!(left.duration, 6.0);
        assert_eq!(right.start_time, 6.0);
        assert_eq!(right.in_point, 1.0);
        assert_eq!(right.duration, 4.0);

        // Slip edit right clip by +2.0s within 10.0s media
        slip_edit(&mut right, 2.0, 10.0).unwrap();
        assert_eq!(right.in_point, 3.0);
        assert_eq!(right.duration, 4.0); // duration unchanged
        assert_eq!(right.start_time, 6.0); // start_time unchanged
    }

    #[test]
    fn video_transition_overlap_calculation() {
        let (center_start, center_end) = calculate_transition_overlap(10.0, 2.0, "CenterOnCut");
        assert_eq!(center_start, 9.0);
        assert_eq!(center_end, 11.0);

        let (start_on_cut, start_end) = calculate_transition_overlap(10.0, 2.0, "StartOnCut");
        assert_eq!(start_on_cut, 10.0);
        assert_eq!(start_end, 12.0);

        let (end_on_cut, end_end) = calculate_transition_overlap(10.0, 2.0, "EndOnCut");
        assert_eq!(end_on_cut, 8.0);
        assert_eq!(end_end, 10.0);
    }

    #[test]
    fn timeline_markers_and_color_presets() {
        let mut proj = VideoProject::new("vp1", "Test Video");
        assert_eq!(MarkerColor::Blue.as_hex(), "#3b82f6");
        assert_eq!(MarkerColor::Red.as_hex(), "#ef4444");

        let m1 = TimelineMarker {
            id: "m1".into(),
            time: 5.0,
            label: "Cut 1".into(),
            color: MarkerColor::Red.as_hex().into(),
        };
        let m2 = TimelineMarker {
            id: "m2".into(),
            time: 15.0,
            label: "Cut 2".into(),
            color: MarkerColor::Green.as_hex().into(),
        };

        proj.add_marker(m1).unwrap();
        proj.add_marker(m2).unwrap();

        // Sorted by time
        assert_eq!(proj.markers[0].id, "m1");
        assert_eq!(proj.markers[1].id, "m2");

        let in_range = proj.find_markers_in_range(0.0, 10.0);
        assert_eq!(in_range.len(), 1);
        assert_eq!(in_range[0].id, "m1");

        assert!(proj.remove_marker("m1"));
        assert_eq!(proj.markers.len(), 1);
    }

    #[test]
    fn audio_envelope_volume_automation() {
        let mut env = AudioEnvelope::new();
        assert_eq!(env.evaluate_volume_db_at(2.5), 0.0);

        // Fade in from -24dB at 0.0s to 0dB at 2.0s
        env.add_key(0.0, -24.0);
        env.add_key(2.0, 0.0);

        assert_eq!(env.evaluate_volume_db_at(-1.0), -24.0);
        assert_eq!(env.evaluate_volume_db_at(0.0), -24.0);
        assert_eq!(env.evaluate_volume_db_at(1.0), -12.0); // halfway
        assert_eq!(env.evaluate_volume_db_at(2.0), 0.0);
        assert_eq!(env.evaluate_volume_db_at(5.0), 0.0);
    }

    #[test]
    fn ken_burns_pan_and_zoom_interpolation() {
        let effect = KenBurnsEffect {
            start_rect: (0.0, 0.0, 1.0, 1.0),
            end_rect: (0.2, 0.2, 0.6, 0.6),
        };

        let start = effect.interpolate_crop_rect(0.0);
        assert_eq!(start, (0.0, 0.0, 1.0, 1.0));

        let end = effect.interpolate_crop_rect(1.0);
        assert_eq!(end, (0.2, 0.2, 0.6, 0.6));

        let mid = effect.interpolate_crop_rect(0.5);
        // At t=0.5, smoothstep(0.5) = 0.5
        assert_eq!(mid, (0.1, 0.1, 0.8, 0.8));
    }

    #[test]
    fn track_audio_mixer_pan_and_volume() {
        let mut track_audio = TrackAudioConfig::default();
        let (left, right) = track_audio.stereo_linear_gains();
        // At center pan, gains should be equal and nonzero (~0.707)
        assert!((left - right).abs() < 1e-4);
        assert!(left > 0.7);

        // Muted track should produce silence
        track_audio.is_muted = true;
        assert_eq!(track_audio.stereo_linear_gains(), (0.0, 0.0));

        // Panned hard left
        track_audio.is_muted = false;
        track_audio.pan = -1.0;
        let (left_pan, right_pan) = track_audio.stereo_linear_gains();
        assert!(left_pan > 0.99);
        assert!(right_pan < 1e-4);
    }

    #[test]
    fn master_audio_limiter_brickwall_processing() {
        let limiter = MasterAudioLimiter {
            ceiling_db: -1.0, // ceiling ~ 0.891
            threshold_db: -1.0,
            release_ms: 20.0,
            enabled: true,
        };

        let mut left = vec![1.5f32, 2.0f32, 0.5f32];
        let mut right = vec![1.2f32, 1.8f32, 0.4f32];

        limiter.process_stereo_samples(&mut left, &mut right, 48000);

        let ceiling_lin = 10.0f32.powf(-1.0 / 20.0);
        // All limited samples must not exceed ceiling
        for s in left {
            assert!(s <= ceiling_lin + 1e-5);
            assert!(s >= -ceiling_lin - 1e-5);
        }
        for s in right {
            assert!(s <= ceiling_lin + 1e-5);
            assert!(s >= -ceiling_lin - 1e-5);
        }
    }

    #[test]
    fn clip_color_tag_labeling() {
        let mut clip = Clip::new("c1", "Intro B-Roll", 5.0);
        assert_eq!(clip.color_tag, ClipColorTag::Orange);
        assert_eq!(clip.color_tag.hex_color(), "#f97316");

        clip.set_color_tag(ClipColorTag::Teal);
        assert_eq!(clip.color_tag, ClipColorTag::Teal);
        assert_eq!(clip.color_tag.hex_color(), "#14b8a6");
    }

    #[test]
    fn align_clips_to_beat_grid_bpm() {
        // At 120 BPM, 1 beat = 0.5s. Beats occur at 0.0, 0.5, 1.0, 1.5, 2.0...
        let mut clips = vec![
            Clip {
                start_time: 0.53, // within 0.05s of 0.5s beat -> snaps to 0.5
                ..Clip::new("c1", "Clip 1", 2.0)
            },
            Clip {
                start_time: 1.25, // 0.25s away from 1.0 and 1.5 -> does NOT snap with 0.1s threshold
                ..Clip::new("c2", "Clip 2", 2.0)
            },
            Clip {
                start_time: 1.98, // within 0.05s of 2.0s beat -> snaps to 2.0
                ..Clip::new("c3", "Clip 3", 2.0)
            },
        ];

        let aligned = align_clips_to_beat_grid(&mut clips, 120.0, 0.0, 0.05);
        assert_eq!(aligned, 2);
        assert_eq!(clips[0].start_time, 0.5);
        assert_eq!(clips[1].start_time, 1.25);
        assert_eq!(clips[2].start_time, 2.0);
    }

    #[test]
    fn multicam_active_angle_switching() {
        let angles = vec![
            MulticamAngle {
                clip_id: "wide".into(),
                label: "Wide".into(),
            },
            MulticamAngle {
                clip_id: "closeup".into(),
                label: "Close-Up".into(),
            },
            MulticamAngle {
                clip_id: "overhead".into(),
                label: "Overhead".into(),
            },
        ];
        // Deliberately unsorted; the function must not mutate its input.
        let cuts = [
            MulticamCut {
                timeline_time: 5.5,
                angle_index: 2,
            },
            MulticamCut {
                timeline_time: 2.0,
                angle_index: 1,
            },
        ];

        assert_eq!(active_angle_at(&angles, &cuts, 0.0), Some(0));
        assert_eq!(active_angle_at(&angles, &cuts, 1.999), Some(0));
        assert_eq!(active_angle_at(&angles, &cuts, 2.0), Some(1));
        assert_eq!(active_angle_at(&angles, &cuts, 5.499), Some(1));
        assert_eq!(active_angle_at(&angles, &cuts, 5.5), Some(2));
        assert_eq!(active_angle_at(&angles, &cuts, 120.0), Some(2));
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].timeline_time, 5.5);

        assert_eq!(active_angle_at(&[], &cuts, 3.0), None);

        let out_of_range = [MulticamCut {
            timeline_time: 1.0,
            angle_index: 7,
        }];
        assert_eq!(active_angle_at(&angles, &out_of_range, 3.0), Some(0));
    }

    #[test]
    fn ducking_envelope_generation() {
        let config = DuckingConfig {
            reduction_db: 12.0,
            attack_seconds: 0.5,
            release_seconds: 0.8,
            padding_seconds: 1.0,
        };

        // Single dialogue region [10, 15]: padded to [9, 16], attack ramp from 8.5,
        // full reduction across the padded region, release ramp ending at 16.8.
        let keys = generate_ducking_envelope(&config, &[(10.0, 15.0)]).unwrap();
        let expected = [(8.5, 0.0), (9.0, -12.0), (16.0, -12.0), (16.8, 0.0)];
        assert_eq!(keys.len(), expected.len());
        for ((time, gain), (want_time, want_gain)) in keys.iter().zip(expected.iter()) {
            assert!(
                (time - want_time).abs() < 1e-9,
                "time {time} != {want_time}"
            );
            assert!(
                (gain - want_gain).abs() < 1e-9,
                "gain {gain} != {want_gain}"
            );
        }

        // Invalid parameters are rejected.
        for bad in [
            DuckingConfig {
                reduction_db: 0.0,
                ..config.clone()
            },
            DuckingConfig {
                reduction_db: -3.0,
                ..config.clone()
            },
            DuckingConfig {
                attack_seconds: -0.5,
                ..config.clone()
            },
            DuckingConfig {
                release_seconds: -1.0,
                ..config.clone()
            },
            DuckingConfig {
                padding_seconds: -0.25,
                ..config.clone()
            },
        ] {
            assert!(generate_ducking_envelope(&bad, &[(10.0, 15.0)]).is_err());
        }

        // Inverted region is rejected.
        assert!(generate_ducking_envelope(&config, &[(15.0, 10.0)]).is_err());

        // Two unsorted regions produce sorted ascending output with no merging.
        let keys = generate_ducking_envelope(&config, &[(20.0, 25.0), (5.0, 8.0)]).unwrap();
        assert_eq!(keys.len(), 8);
        for pair in keys.windows(2) {
            assert!(pair[0].0 < pair[1].0, "keys not sorted: {keys:?}");
        }
        assert!((keys[0].0 - 3.5).abs() < 1e-9);
        assert!((keys[7].0 - 26.8).abs() < 1e-9);
    }

    #[test]
    fn proxy_registry_lifecycle() {
        let mut registry = ProxyRegistry::new();
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-a".into(),
                proxy_path: "/media/proxies/a.mov".into(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-b".into(),
                proxy_path: "/media/proxies/b.mov".into(),
                scale: 0.25,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        assert_eq!(registry.links.len(), 2);

        // Replacing the link for an existing source clip updates in place.
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-a".into(),
                proxy_path: "/media/proxies/a_v2.mov".into(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        assert_eq!(registry.links.len(), 2);
        assert_eq!(
            registry.proxy_for("clip-a"),
            Some("/media/proxies/a_v2.mov")
        );
        assert_eq!(registry.proxy_for("clip-b"), Some("/media/proxies/b.mov"));
        assert_eq!(registry.proxy_for("clip-missing"), None);

        // Removal succeeds once, then reports false.
        assert!(registry.remove_link("clip-b"));
        assert!(!registry.remove_link("clip-b"));
        assert_eq!(registry.links.len(), 1);

        // Only clip-a's proxy is reported stale when its path fails the predicate.
        let stale = registry.stale_links(|path| path != "/media/proxies/a_v2.mov");
        assert_eq!(stale, vec!["clip-a".to_string()]);

        // Validation errors.
        for bad_scale in [0.0_f32, 1.5] {
            let result = registry.set_link(ProxyLink {
                source_clip_id: "clip-c".into(),
                proxy_path: "/media/proxies/c.mov".into(),
                scale: bad_scale,
                codec: "prores_proxy".into(),
            });
            assert!(result.is_err(), "scale {bad_scale} must be rejected");
        }
        assert!(registry
            .set_link(ProxyLink {
                source_clip_id: "clip-c".into(),
                proxy_path: String::new(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .is_err());
        assert_eq!(registry.links.len(), 1);
    }

    #[test]
    fn shuttle_transport_ladder() {
        // Stopped state has zero rate
        let mut s = ShuttleState::stopped();
        assert!(s.is_stopped());
        assert_eq!(s.rate(), 0.0);

        // Forward ladder: 0.5x, 1x, 2x, 4x, 8x then saturates
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 0.5);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 1.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 2.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 4.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 8.0);
        let top = s.clone();
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 8.0, "ladder saturates at maximum speed");
        assert_eq!(s.step, SHUTTLE_SPEED_LADDER.len());

        // Stop resets to neutral paused state
        s.press_stop();
        assert!(s.is_stopped());
        assert_eq!(s.rate(), 0.0);

        // Reverse ladder mirrors with negative rates
        let mut r = ShuttleState::default();
        assert!(r.is_stopped());
        r.press_shuttle_reverse();
        assert_eq!(r.rate(), -0.5);
        r.press_shuttle_reverse();
        assert_eq!(r.rate(), -1.0);
        r.press_shuttle_reverse();
        assert_eq!(r.direction, ShuttleDirection::Reverse);
        assert!((r.rate() - (-2.0)).abs() < 1e-9);

        // Direction switch from reverse keeps the step magnitude
        r.press_shuttle_forward();
        assert_eq!(r.direction, ShuttleDirection::Forward);
        assert!((r.rate() - 4.0).abs() < 1e-9);

        // Determinism: identical sequences produce identical states
        let mut a = ShuttleState::stopped();
        let mut b = ShuttleState::stopped();
        for _ in 0..3 {
            a.press_shuttle_forward();
            b.press_shuttle_forward();
        }
        assert_eq!(a, b);
        assert_eq!(top.direction, ShuttleDirection::Forward);
    }

    #[test]
    fn normalize_gain_analysis() {
        // A clip peaking at 0.25 needs +12.04 dB to hit 1.0
        let clip = [0.0, 0.1, -0.25, 0.05];
        let gain = suggest_normalize_gain(&clip, 1.0).unwrap();
        assert!((gain - (20.0 * 4.0f64.log10())).abs() < 1e-9);

        // Halving the target halves the linear ratio (+6.02 dB from 0.25)
        let half = suggest_normalize_gain(&clip, 0.5).unwrap();
        assert!((half - (20.0 * 2.0f64.log10())).abs() < 1e-9);

        // Already-peaked clip needs no gain
        assert!((suggest_normalize_gain(&[1.0, -0.5], 1.0).unwrap()).abs() < 1e-9);
        // Over-peaked input clamps and attenuates: 0.5 ratio is -6.02 dB
        let attenuation = suggest_normalize_gain(&[2.0], 0.5).unwrap();
        assert!((attenuation - (-6.020_599_913_279_624)).abs() < 1e-6);

        // Silence and empty inputs need zero gain
        assert_eq!(suggest_normalize_gain(&[], 1.0).unwrap(), 0.0);
        assert_eq!(suggest_normalize_gain(&[0.0; 10], 0.8).unwrap(), 0.0);

        // Invalid target peaks are rejected
        assert!(suggest_normalize_gain(&[0.5], -0.1).is_err());
        assert!(suggest_normalize_gain(&[0.5], 1.5).is_err());
        assert!(suggest_normalize_gain(&[0.5], f32::NAN).is_err());

        // Gain clamping bounds boost but allows attenuation
        assert_eq!(clamp_gain_db(30.0, 12.0), 12.0);
        assert_eq!(clamp_gain_db(-90.0, 12.0), -60.0);
        assert_eq!(clamp_gain_db(3.0, 12.0), 3.0);
    }

    #[test]
    fn srt_round_trip_parsing() {
        let sample = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello there.\r\n\r\n\
                      2\r\n00:00:03.250 --> 00:00:05,750\r\nSecond cue,\r\nkeeps interior newlines.\r\n";
        let cues = parse_srt(sample).expect("sample should parse");
        assert_eq!(cues.len(), 2);
        assert_eq!(
            cues[0],
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 2.5,
                text: "Hello there.".into(),
            }
        );
        assert_eq!(cues[1].start_seconds, 3.25);
        assert_eq!(cues[1].end_seconds, 5.75);
        assert_eq!(cues[1].text, "Second cue,\nkeeps interior newlines.");

        let written = write_srt(&cues);
        assert!(written.contains("00:00:01,000 --> 00:00:02,500"));
        assert!(written.contains("00:00:03,250 --> 00:00:05,750"));
        let reparsed = parse_srt(&written).expect("written SRT should re-parse");
        assert_eq!(reparsed, cues);

        // Malformed timestamp block names the offending block.
        let malformed = parse_srt("1\r\n00:00:01,000 --> 00:00:02,500\r\nFine.\r\n\r\n2\r\n00:00:zz --> 00:00:02,500\r\nBad.\r\n");
        assert!(malformed.is_err());
        assert!(malformed.unwrap_err().contains("block 2"));

        // End not after start is rejected.
        assert!(parse_srt("1\r\n00:00:02,000 --> 00:00:01,000\r\nBackwards.\r\n").is_err());
        assert!(parse_srt("1\r\n00:00:01,000 --> 00:00:01,000\r\nZero length.\r\n").is_err());
    }

    #[test]
    fn caption_lane_entries_and_lookup() {
        // Input intentionally out of chronological order; cue 2 is multi-line.
        let cues = vec![
            SubtitleCue {
                index: 2,
                start_seconds: 4.0,
                end_seconds: 6.0,
                text: "Second\tcue,\n keeps   interior \r newlines.".into(),
            },
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 2.5,
                text: "Hello there.".into(),
            },
            SubtitleCue {
                index: 3,
                start_seconds: 7.0,
                end_seconds: 8.0,
                text: " Final. ".into(),
            },
        ];
        let entries = captions_from_cues(&cues).expect("valid cues convert");
        assert_eq!(
            entries,
            vec![
                CaptionEntry {
                    start_seconds: 1.0,
                    end_seconds: 2.5,
                    text: "Hello there.".into(),
                },
                CaptionEntry {
                    start_seconds: 4.0,
                    end_seconds: 6.0,
                    text: "Second cue, keeps interior newlines.".into(),
                },
                CaptionEntry {
                    start_seconds: 7.0,
                    end_seconds: 8.0,
                    text: "Final.".into(),
                },
            ]
        );

        // Hits inside every caption, including the inclusive start boundary.
        assert_eq!(active_caption_at(&entries, 1.5), Some(&entries[0]));
        assert_eq!(active_caption_at(&entries, 4.0), Some(&entries[1]));
        assert_eq!(active_caption_at(&entries, 7.25), Some(&entries[2]));

        // Misses before all, exclusive ends, gaps, and after all.
        assert_eq!(active_caption_at(&entries, 0.5), None);
        assert_eq!(active_caption_at(&entries, 2.5), None);
        assert_eq!(active_caption_at(&entries, 3.0), None);
        assert_eq!(active_caption_at(&entries, 6.5), None);
        assert_eq!(active_caption_at(&entries, 9.0), None);

        // Empty lane has no active caption at any time.
        assert_eq!(active_caption_at(&[], 1.0), None);

        // Overlapping cues are rejected with an error naming the conflict.
        let overlapping = vec![
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 3.0,
                text: "A".into(),
            },
            SubtitleCue {
                index: 2,
                start_seconds: 2.5,
                end_seconds: 4.0,
                text: "B".into(),
            },
        ];
        let err = captions_from_cues(&overlapping).unwrap_err();
        assert!(
            err.contains("overlap") && err.contains("cue 2") && err.contains("cue 1"),
            "error must name the conflict: {err}"
        );

        // Zero and negative durations are rejected.
        let zero = vec![SubtitleCue {
            index: 1,
            start_seconds: 1.0,
            end_seconds: 1.0,
            text: "Zero".into(),
        }];
        let err = captions_from_cues(&zero).unwrap_err();
        assert!(err.contains("end must be after start"), "unexpected: {err}");
        let backwards = vec![SubtitleCue {
            index: 1,
            start_seconds: 2.0,
            end_seconds: 1.0,
            text: "Backwards".into(),
        }];
        assert!(captions_from_cues(&backwards).is_err());
    }

    #[test]
    fn motion_template_binding_validation() {
        let valid = MotionTemplateBinding {
            template_id: "loom.motion.lower-third".into(),
            schema_version: 2,
            parameters: vec![
                ("title".into(), "Interview".into()),
                ("accent_color".into(), "#3b82f6".into()),
            ],
            start_seconds: 4.5,
            duration_seconds: 6.0,
        };
        valid.validate().expect("valid binding must validate");

        // Parameter lookup hits and misses.
        assert_eq!(valid.parameter("title"), Some("Interview"));
        assert_eq!(valid.parameter("accent_color"), Some("#3b82f6"));
        assert_eq!(valid.parameter("missing"), None);
        assert_eq!(valid.parameter(""), None);

        // Schema migration review is required only on version mismatch.
        assert!(!valid.needs_migration(2));
        assert!(valid.needs_migration(3));
        assert!(valid.needs_migration(1));

        // Empty template id names its rule.
        let no_id = MotionTemplateBinding {
            template_id: String::new(),
            ..valid.clone()
        };
        let err = no_id.validate().unwrap_err();
        assert!(err.contains("template_id"), "unexpected error: {err}");

        // Zero and negative durations are rejected.
        for bad_duration in [0.0_f64, -1.5] {
            let bad = MotionTemplateBinding {
                duration_seconds: bad_duration,
                ..valid.clone()
            };
            let err = bad.validate().unwrap_err();
            assert!(
                err.contains("duration_seconds"),
                "duration {bad_duration} not rejected: {err}"
            );
        }

        // Negative start is rejected.
        let negative_start = MotionTemplateBinding {
            start_seconds: -0.25,
            ..valid.clone()
        };
        let err = negative_start.validate().unwrap_err();
        assert!(
            err.contains("start_seconds") && !err.contains("duration_seconds"),
            "unexpected error: {err}"
        );

        // Duplicate parameter names are rejected.
        let duplicated = MotionTemplateBinding {
            parameters: vec![
                ("title".into(), "First".into()),
                ("title".into(), "Second".into()),
            ],
            ..valid.clone()
        };
        let err = duplicated.validate().unwrap_err();
        assert!(err.contains("duplicate"), "unexpected error: {err}");
    }

    #[test]
    fn edl_export_record_structure() {
        // Three clips at 25 fps. Timecode arithmetic: 1 s = 25 frames.
        // Clip A: source offset 10 s, timeline 0..5 s
        //   src in 00:00:10:00, src out 00:00:15:00, rec in 00:00:00:00, rec out 00:00:05:00
        // Clip B ("interview b-roll!"): source offset 60 s, timeline 5..8 s
        //   reel truncates to 8 chars uppercased with '_' -> "INTERVIE"
        //   src in 00:01:00:00, src out 00:01:03:00, rec in 00:00:05:00, rec out 00:00:08:00
        let clips = vec![
            ("Clip A".to_string(), 10.0, 0.0, 5.0),
            ("interview b-roll!".to_string(), 60.0, 5.0, 3.0),
        ];

        let records = build_edl_records(&clips, 25.0).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].event_number, 1);
        assert_eq!(records[0].reel, "CLIP_A");
        assert_eq!(records[0].source_in, "00:00:10:00");
        assert_eq!(records[0].source_out, "00:00:15:00");
        assert_eq!(records[0].record_in, "00:00:00:00");
        assert_eq!(records[0].record_out, "00:00:05:00");

        assert_eq!(records[1].event_number, 2);
        assert_eq!(records[1].reel, "INTERVIE");
        assert_eq!(records[1].source_in, "00:01:00:00");
        assert_eq!(records[1].source_out, "00:01:03:00");
        assert_eq!(records[1].record_in, "00:00:05:00");
        assert_eq!(records[1].record_out, "00:00:08:00");

        // Serialization carries event lines and clip-name comments in order
        let text = write_edl(&records);
        assert!(text.contains("001  CLIP_A"), "event line missing: {text}");
        assert!(text.contains("* FROM CLIP NAME: interview b-roll!\r\n"));
        assert!(text.ends_with("\r\n"));

        // Empty input serializes to an empty string
        assert_eq!(write_edl(&[]), "");

        // Invalid frame rates are rejected
        assert!(build_edl_records(&clips, 0.0).is_err());
        assert!(build_edl_records(&clips, -25.0).is_err());
        assert!(build_edl_records(&clips, f64::NAN).is_err());

        // Zero-length clips still produce valid ordered ranges
        let zero = build_edl_records(&[("z".to_string(), 4.0, 2.0, 0.0)], 25.0).unwrap();
        assert_eq!(zero[0].source_in, "00:00:04:00");
        assert_eq!(zero[0].source_out, "00:00:04:00");
    }

    #[test]
    fn apply_edit_checkpoints_only_after_successful_validation() {
        let mut session = VideoSession::new(VideoProject::new("video", "Edit"));
        let error = session
            .apply_edit(|project| project.split_clip(0, "missing", 1.0).map(|_| ()))
            .unwrap_err();
        assert!(matches!(error, TimelineError::ClipNotFound));
        assert!(!session.can_undo());

        let mut clip = Clip::new("clip", "Clip", 4.0);
        clip.source_path = "clip.mp4".into();
        session.project.tracks[0].insert_clip(clip).unwrap();
        session
            .apply_edit(|project| project.tracks[0].clips[0].trim_out(3.0))
            .unwrap();
        assert!(session.can_undo());
        assert_eq!(session.project.tracks[0].clips[0].duration, 3.0);
        assert!(session.undo());
        assert_eq!(session.project.tracks[0].clips[0].duration, 4.0);
    }

    #[test]
    fn gesture_edits_commit_once_or_restore_without_history() {
        let mut session = VideoSession::new(VideoProject::new("video", "Gesture"));
        session.project.tracks[0].add_clip(Clip::new("clip", "Clip", 4.0));
        let baseline = session.project.clone();

        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 1.0, false))
            .unwrap();
        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 2.0, false))
            .unwrap();
        assert!(
            !session.can_undo(),
            "gesture updates must not create history"
        );
        assert!(session.commit_gesture(baseline.clone()));
        assert!(session.can_undo());
        assert!(session.undo());
        assert_eq!(session.project, baseline);
        assert!(
            !session.can_undo(),
            "one gesture should produce one undo entry"
        );

        let cancel_baseline = session.project.clone();
        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 3.0, false))
            .unwrap();
        session.rollback_gesture(cancel_baseline.clone());
        assert_eq!(session.project, cancel_baseline);
        assert!(
            !session.can_undo(),
            "cancelled gestures must not create history"
        );
    }

    #[test]
    fn timeline_coordinate_conversion_is_zoom_stable() {
        assert_eq!(VideoProject::seconds_to_pixels(2.5, 100.0), 250.0);
        assert_eq!(VideoProject::pixels_to_seconds(250.0, 100.0), 2.5);
        assert_eq!(VideoProject::seconds_to_pixels(-1.0, 100.0), 0.0);
        assert_eq!(VideoProject::pixels_to_seconds(100.0, 0.0), 100.0);
    }

    #[test]
    fn export_cancellation_token_is_cooperative() {
        let token = ExportCancellation::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        assert!(execute_timeline_export_with_cancel(
            &TimelineExportPlan {
                executable: "definitely-not-a-real-ffmpeg".into(),
                arguments: Vec::new(),
                output: "out.mp4".into(),
                duration: 1.0,
            },
            |_| {},
            &token,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_export_removes_partial_temp_and_preserves_destination() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "loom-video-export-cancel-{}-{}",
            std::process::id(),
            NEXT_EXPORT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create export test directory");
        let script = dir.join("fake-ffmpeg.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nmarker=\"$1\"\noutput=\"$2\"\nprintf started > \"$marker\"\nprintf partial > \"$output\"\nwhile :; do sleep 0.01; done\n",
        )
        .expect("write fake ffmpeg");
        let mut permissions = std::fs::metadata(&script)
            .expect("stat fake ffmpeg")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake ffmpeg executable");

        let destination = dir.join("render.mp4");
        std::fs::write(&destination, b"existing destination").expect("seed destination");
        let marker = dir.join("started");
        let plan = TimelineExportPlan {
            executable: script,
            arguments: vec![marker.to_string_lossy().into_owned()],
            output: destination.clone(),
            duration: 1.0,
        };
        let cancel = ExportCancellation::default();
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            execute_timeline_export_with_cancel(&plan, |_| {}, &worker_cancel)
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !marker.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(marker.is_file(), "fake worker did not start");
        cancel.cancel();
        assert!(worker.join().expect("join export worker").is_err());
        assert_eq!(
            std::fs::read(&destination).expect("read existing destination"),
            b"existing destination"
        );
        let temporary_left = std::fs::read_dir(&dir)
            .expect("read export directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".loom-video-"));
        assert!(!temporary_left, "temporary export output was not removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn failed_export_removes_partial_temp_and_preserves_destination() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "loom-video-export-failure-{}-{}",
            std::process::id(),
            NEXT_EXPORT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create export test directory");
        let script = dir.join("fake-ffmpeg.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nmarker=\"$1\"\noutput=\"$2\"\nprintf started > \"$marker\"\nprintf partial > \"$output\"\nprintf failed >&2\nexit 17\n",
        )
        .expect("write fake ffmpeg");
        let mut permissions = std::fs::metadata(&script)
            .expect("stat fake ffmpeg")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake ffmpeg executable");

        let destination = dir.join("render.mp4");
        std::fs::write(&destination, b"existing destination").expect("seed destination");
        let marker = dir.join("started");
        let plan = TimelineExportPlan {
            executable: script,
            arguments: vec![marker.to_string_lossy().into_owned()],
            output: destination.clone(),
            duration: 1.0,
        };
        let result =
            execute_timeline_export_with_cancel(&plan, |_| {}, &ExportCancellation::default());
        assert!(result.is_err(), "fake FFmpeg must fail");
        assert!(marker.is_file(), "fake worker did not start");
        assert_eq!(
            std::fs::read(&destination).expect("read existing destination"),
            b"existing destination"
        );
        let temporary_left = std::fs::read_dir(&dir)
            .expect("read export directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".loom-video-"));
        assert!(!temporary_left, "temporary export output was not removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
