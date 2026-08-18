//! Core nonlinear video editing engine for Loom Video.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            proxy_path: None,
            effects: Vec::new(),
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    if max_width == 0 || max_height == 0 {
        return Err("preview dimensions must be non-zero".into());
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
    let output = Command::new(&tools.ffmpeg)
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
        .output()
        .map_err(|error| format!("start FFmpeg preview decoder: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let expected = width as usize * height as usize * 4;
    if output.stdout.len() != expected {
        return Err(format!(
            "decoded frame has {} bytes; expected {expected}",
            output.stdout.len()
        ));
    }
    Ok(VideoFrame {
        width,
        height,
        pixels: output.stdout,
    })
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
pub fn execute_timeline_export<F>(plan: &TimelineExportPlan, mut progress: F) -> Result<(), String>
where
    F: FnMut(f32),
{
    if let Some(parent) = plan.output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    let mut child = Command::new(&plan.executable)
        .args(&plan.arguments)
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
    let mut last = 0.0;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| error.to_string())?;
        if let Some(value) = line
            .strip_prefix("out_time_us=")
            .and_then(|value| value.parse::<f64>().ok())
        {
            last = (value / 1_000_000.0 / plan.duration.max(0.001)).clamp(0.0, 0.999) as f32;
            progress(last);
        } else if line == "progress=end" {
            last = 1.0;
            progress(1.0);
        }
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let stderr = stderr_reader
        .join()
        .unwrap_or_else(|_| "FFmpeg stderr reader panicked".into());
    if status.success() {
        if last < 1.0 {
            progress(1.0);
        }
        Ok(())
    } else {
        Err(stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
