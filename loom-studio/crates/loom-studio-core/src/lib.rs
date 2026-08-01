//! Core audio engine and DAW model for Loom Studio.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceMode {
    Quick,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackKind {
    Audio,
    Midi,
    Drummer,
    Bus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRegion {
    pub id: String,
    pub name: String,
    pub start_sample: u64,
    pub length_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioTrack {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub regions: Vec<AudioRegion>,
}

impl StudioTrack {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            regions: Vec::new(),
        }
    }

    pub fn add_region(&mut self, region: AudioRegion) {
        self.regions.push(region);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioProject {
    pub id: String,
    pub name: String,
    pub bpm: f32,
    pub sample_rate: u32,
    pub mode: WorkspaceMode,
    pub tracks: Vec<StudioTrack>,
}

impl StudioProject {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut proj = Self {
            id: id.into(),
            name: name.into(),
            bpm: 120.0,
            sample_rate: 48000,
            mode: WorkspaceMode::Quick,
            tracks: Vec::new(),
        };
        let mut t1 = StudioTrack::new("track-1", "Vocal Guide", TrackKind::Audio);
        t1.add_region(AudioRegion {
            id: "r1".to_string(),
            name: "Vocal_Take1.wav".to_string(),
            start_sample: 0,
            length_samples: 48000 * 10,
        });
        proj.tracks.push(t1);
        proj.tracks.push(StudioTrack::new(
            "track-2",
            "Acoustic Guitar",
            TrackKind::Audio,
        ));
        proj
    }

    pub fn add_track(&mut self, track: StudioTrack) {
        self.tracks.push(track);
    }

    pub fn total_regions(&self) -> usize {
        self.tracks.iter().map(|t| t.regions.len()).sum()
    }
}

pub fn save_studio_project(proj: &StudioProject) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(proj).map_err(|e| e.to_string())?;
    let mut arch = loom_package::PackageArchive::new();
    arch.add("content/studio.json", json)
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_studio_project(bytes: &[u8]) -> Result<StudioProject, String> {
    let arch = loom_package::PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let content = arch
        .get("content/studio.json")
        .ok_or_else(|| "missing studio.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_studio_creation() {
        let proj = StudioProject::new("studio-1", "Indie Rock Song");
        assert_eq!(proj.bpm, 120.0);
        assert_eq!(proj.sample_rate, 48000);
        assert_eq!(proj.tracks.len(), 2);
    }

    #[test]
    fn test_track_regions() {
        let mut proj = StudioProject::new("studio-1", "Acoustic Ballad");
        proj.tracks[1].add_region(AudioRegion {
            id: "r2".to_string(),
            name: "Guitar_Rhythm.wav".to_string(),
            start_sample: 48000 * 2,
            length_samples: 48000 * 8,
        });
        assert_eq!(proj.total_regions(), 2);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut proj = StudioProject::new("studio-test", "Synthwave Groove");
        proj.mode = WorkspaceMode::Pro;
        proj.tracks
            .push(StudioTrack::new("t3", "Analog Bass", TrackKind::Midi));
        let bytes = save_studio_project(&proj).expect("save failed");
        let loaded = load_studio_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Synthwave Groove");
        assert_eq!(loaded.mode, WorkspaceMode::Pro);
        assert_eq!(loaded.tracks.len(), 3);
    }
}
