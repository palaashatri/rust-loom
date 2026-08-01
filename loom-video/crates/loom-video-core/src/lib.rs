//! Core nonlinear video editing engine for Loom Video.

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

    pub fn total_clips(&self) -> usize {
        self.tracks.iter().map(|t| t.clips.len()).sum()
    }
}

pub fn save_video_project(proj: &VideoProject) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(proj).map_err(|e| e.to_string())?;
    let mut arch = loom_package::PackageArchive::new();
    arch.add("content/project.json", json)
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_video_project(bytes: &[u8]) -> Result<VideoProject, String> {
    let arch = loom_package::PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
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
    fn test_save_load_roundtrip() {
        let mut proj = VideoProject::new("v-test", "Short Film");
        proj.tracks[0].add_clip(Clip::new("c1", "Scene1.mp4", 12.0));
        let bytes = save_video_project(&proj).expect("save failed");
        let loaded = load_video_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Short Film");
        assert_eq!(loaded.total_clips(), 1);
    }
}
