from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:120]!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


# Media render stage accounting must be initialized before base-frame upload.
replace_once(
    "loom-core/crates/loom-media-runtime/src/lib.rs",
    '''        let mut filter = String::new();
        let (red, green, blue, alpha) = rgba_components(self.background_rgba);''',
    '''        let mut filter = String::new();
        let mut gpu_stages = Vec::new();
        let mut cpu_stages = Vec::new();
        let (red, green, blue, alpha) = rgba_components(self.background_rgba);''',
)
replace_once(
    "loom-core/crates/loom-media-runtime/src/lib.rs",
    '''        let mut gpu_stages = Vec::new();
        let mut cpu_stages = Vec::new();
        let mut current_base = "base0".to_string();''',
    '''        let mut current_base = "base0".to_string();''',
)

# Export the application snapshot coordinator and simplify destructive clear.
replace_once(
    "loom-core/crates/loom-production/src/lib.rs",
    '''#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};''',
    '''#![forbid(unsafe_code)]

/// Deduplicating full-state recovery coordination for Loom applications.
pub mod snapshot;

use serde::{Deserialize, Serialize};''',
)
replace_once(
    "loom-core/crates/loom-production/src/snapshot.rs",
    '''    pub fn clear(mut self) -> Result<(), ProductionError> {
        let directory = self.journal.directory().to_path_buf();
        self.restored_payload = None;
        self.last_payload = None;
        drop(self.journal);''',
    '''    pub fn clear(self) -> Result<(), ProductionError> {
        let directory = self.journal.directory().to_path_buf();
        drop(self.journal);''',
)

# Correct native Windows installer UpgradeCode formatting.
replace_once(
    "loom-bootstrap/packaging/release.py",
    '''upgrade_code = uuid.uuid5(uuid.NAMESPACE_URL, "https://loom.local/creator-suite").upper()''',
    '''upgrade_code = str(uuid.uuid5(uuid.NAMESPACE_URL, "https://loom.local/creator-suite")).upper()''',
)

# Complete the Video core API used by the GUI and give timeline failures a stable message.
replace_once(
    "loom-video/crates/loom-video-core/src/lib.rs",
    '''pub enum TimelineError {
    /// Track index was invalid.
    InvalidTrack,
    /// Clip id was not found.
    ClipNotFound,
    /// Track is locked.
    TrackLocked,
    /// Requested timing was invalid.
    InvalidTiming(String),
}

impl Clip {''',
    '''pub enum TimelineError {
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

impl Clip {''',
)
replace_once(
    "loom-video/crates/loom-video-core/src/lib.rs",
    '''impl Track {
    /// Inserts a clip and sorts by timeline start.
    pub fn insert_clip(&mut self, clip: Clip) -> Result<(), TimelineError> {''',
    '''impl Track {
    /// Timeline end of the last enabled clip, or zero for an empty track.
    pub fn duration(&self) -> f64 {
        self.clips
            .iter()
            .filter(|clip| clip.enabled)
            .map(Clip::end_time)
            .fold(0.0, f64::max)
    }

    /// Inserts a clip and sorts by timeline start.
    pub fn insert_clip(&mut self, clip: Clip) -> Result<(), TimelineError> {''',
)

# Accept split ids returned by the core API. Display now handles timeline errors.
replace_once(
    "loom-video/crates/loom-video-app/src/main.rs",
    '''                        match session.project.split_clip(track_index, &id, playhead) {
                            Ok(()) => app
                                .set_status_left(format!("Split clip at {:.2}s", playhead).into()),''',
    '''                        match session.project.split_clip(track_index, &id, playhead) {
                            Ok((_left_id, _right_id)) => app
                                .set_status_left(format!("Split clip at {:.2}s", playhead).into()),''',
)
