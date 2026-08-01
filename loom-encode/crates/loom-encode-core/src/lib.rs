//! Core batch media encoding and transcoding queue engine for Loom Encode.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Queued,
    Encoding { progress: f32 },
    Complete,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodePreset {
    pub name: String,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub bitrate_kbps: u32,
}

impl EncodePreset {
    pub fn h264_1080p() -> Self {
        Self {
            name: "H.264 Web 1080p".to_string(),
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            bitrate_kbps: 8000,
        }
    }

    pub fn prores_master() -> Self {
        Self {
            name: "ProRes 422 HQ Master".to_string(),
            container: "mov".to_string(),
            video_codec: "prores".to_string(),
            audio_codec: "pcm_s24le".to_string(),
            bitrate_kbps: 220000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeJob {
    pub id: String,
    pub source_file: String,
    pub output_file: String,
    pub preset: EncodePreset,
    pub status: JobStatus,
}

impl EncodeJob {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        output: impl Into<String>,
        preset: EncodePreset,
    ) -> Self {
        Self {
            id: id.into(),
            source_file: source.into(),
            output_file: output.into(),
            preset,
            status: JobStatus::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeQueue {
    pub id: String,
    pub name: String,
    pub jobs: Vec<EncodeJob>,
}

impl EncodeQueue {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut q = Self {
            id: id.into(),
            name: name.into(),
            jobs: Vec::new(),
        };
        q.jobs.push(EncodeJob::new(
            "job-1",
            "Master_Cut_01.mov",
            "Master_Cut_01_Web.mp4",
            EncodePreset::h264_1080p(),
        ));
        q
    }

    pub fn add_job(&mut self, job: EncodeJob) {
        self.jobs.push(job);
    }

    pub fn pending_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Queued))
            .count()
    }
}

pub fn save_encode_queue(q: &EncodeQueue) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(q).map_err(|e| e.to_string())?;
    let mut arch = loom_package::PackageArchive::new();
    arch.add("content/queue.json", json)
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_encode_queue(bytes: &[u8]) -> Result<EncodeQueue, String> {
    let arch = loom_package::PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let content = arch
        .get("content/queue.json")
        .ok_or_else(|| "missing queue.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_creation() {
        let q = EncodeQueue::new("q-1", "Broadcast Delivery");
        assert_eq!(q.jobs.len(), 1);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut q = EncodeQueue::new("q-test", "Daily Dailies");
        q.add_job(EncodeJob::new(
            "j-2",
            "Day1_Take2.mov",
            "Day1_Take2_ProRes.mov",
            EncodePreset::prores_master(),
        ));
        let bytes = save_encode_queue(&q).expect("save failed");
        let loaded = load_encode_queue(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Daily Dailies");
        assert_eq!(loaded.jobs.len(), 2);
    }
}
