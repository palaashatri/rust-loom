//! Core audio engine and DAW model for Loom Studio.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
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
    #[serde(default)]
    pub active_track_index: usize,
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
            active_track_index: 0,
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

    pub fn select_track(&mut self, index: usize) -> bool {
        if index < self.tracks.len() {
            self.active_track_index = index;
            true
        } else {
            false
        }
    }

    pub fn total_regions(&self) -> usize {
        self.tracks.iter().map(|t| t.regions.len()).sum()
    }
}

pub fn save_studio_project(proj: &StudioProject) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(proj).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/studio.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Studio,
        id: proj.id.clone(),
        title: proj.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/studio.json".into(),
            mime: MimeType::parse("application/vnd.loom.studio-content")
                .map_err(|e| format!("invalid built-in studio MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_studio_project(bytes: &[u8]) -> Result<StudioProject, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Studio {
        return Err("not a Studio project".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
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
    fn test_select_track_rejects_invalid_index() {
        let mut proj = StudioProject::new("studio-1", "Acoustic Ballad");
        assert!(proj.select_track(1));
        assert!(!proj.select_track(2));
        assert_eq!(proj.active_track_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut proj = StudioProject::new("studio-test", "Synthwave Groove");
        proj.mode = WorkspaceMode::Pro;
        proj.tracks
            .push(StudioTrack::new("t3", "Analog Bass", TrackKind::Midi));
        let bytes = save_studio_project(&proj).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Studio);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_studio_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Synthwave Groove");
        assert_eq!(loaded.mode, WorkspaceMode::Pro);
        assert_eq!(loaded.tracks.len(), 3);
    }
}
