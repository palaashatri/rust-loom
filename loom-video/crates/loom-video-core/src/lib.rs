//! Core nonlinear video editing engine for Loom Video.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};

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
            return Err(TimelineError::InvalidTiming("invalid source in point".into()));
        }
        let delta = source_in - self.in_point;
        self.in_point = source_in;
        self.start_time += delta / self.playback_rate.max(f64::EPSILON);
        self.sync_duration()
    }

    /// Trims the source out point.
    pub fn trim_out(&mut self, source_out: f64) -> Result<(), TimelineError> {
        if !source_out.is_finite() || source_out <= self.in_point {
            return Err(TimelineError::InvalidTiming("invalid source out point".into()));
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
}

impl VideoProject {
    /// Timeline duration across every enabled clip and caption.
    pub fn duration(&self) -> f64 {
        let clips = self
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter(|clip| clip.enabled)
            .map(Clip::end_time)
            .fold(0.0_f64, f64::max);
        let captions = self.captions.iter().map(|caption| caption.end).fold(0.0, f64::max);
        clips.max(captions)
    }

    /// Adds a marker in sorted order.
    pub fn add_marker(&mut self, marker: TimelineMarker) -> Result<(), TimelineError> {
        if !marker.time.is_finite() || marker.time < 0.0 {
            return Err(TimelineError::InvalidTiming("marker time is invalid".into()));
        }
        if self.markers.iter().any(|existing| existing.id == marker.id) {
            return Err(TimelineError::InvalidTiming(format!(
                "duplicate marker id {}",
                marker.id
            )));
        }
        self.markers.push(marker);
        self.markers.sort_by(|left, right| left.time.total_cmp(&right.time));
        Ok(())
    }

    /// Adds a caption cue in sorted order.
    pub fn add_caption(&mut self, caption: CaptionCue) -> Result<(), TimelineError> {
        if !caption.start.is_finite()
            || !caption.end.is_finite()
            || caption.start < 0.0
            || caption.end <= caption.start
        {
            return Err(TimelineError::InvalidTiming("caption range is invalid".into()));
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

}
