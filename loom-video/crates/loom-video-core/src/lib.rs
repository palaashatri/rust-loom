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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub start_time: f64,
    pub duration: f64,
    pub in_point: f64,
    pub out_point: f64,
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
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
    }
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
}
