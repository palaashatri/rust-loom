//! Loom Video desktop application with local FFmpeg media workflows.

use std::collections::VecDeque;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{
    process::{Child, Command, Stdio},
    thread,
};

use loom_desktop::{
    build_standard_menu_bar, CommandAction, FileDialogService, FileFilter, Menu, MenuActionSink,
    MenuBar, MenuBarService, MenuItem, MenuShortcut, NativeFileDialogs, NativeMenuBar,
    OpenFileRequest, SaveFileRequest, ScriptedFileDialogs,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::PaletteProbe;
#[cfg(test)]
use loom_video_core::decode_preview_frame;
use loom_video_core::{
    build_timeline_export_plan, decode_audio_waveform_with_cancel,
    decode_preview_frame_with_cancel, discover_media_tools, execute_timeline_export_with_cancel,
    load_video_project, probe_media, save_video_project, snap_timeline_to_edit_points, Clip,
    ExportCancellation, MediaTools, PreviewCancellation, TimelineMarker, VideoFrame, VideoProject,
    VideoSession,
};
use slint::{
    ComponentHandle, Image, Model, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);

loom_production::define_snapshot_recovery!(VIDEO_RECOVERY, "org.loom.video", "loom.video/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    rtl: bool,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "dark".into(),
        rtl: false,
        open: None,
    };
    let mut iterator = std::env::args().skip(1);
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--screenshot" => {
                args.screenshot = Some(iterator.next().ok_or("--screenshot needs a path")?)
            }
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(
                    iterator
                        .next()
                        .ok_or("--journey needs an output directory")?,
                );
            }
            "--size" => {
                let value = iterator.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad width")?,
                    height.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => args.theme = iterator.next().ok_or("--theme needs a name")?,
            "--rtl" => args.rtl = true,
            "--open" => args.open = Some(iterator.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn empty_project() -> VideoProject {
    VideoProject::new("untitled-project", "Untitled Project")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResponsiveToolbarState {
    icon_only: bool,
    overflow: bool,
    labeled: bool,
}

fn responsive_toolbar_state(app: &VideoApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32;
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
    }
}

#[cfg(test)]
fn compact_layout_for_width(app: &VideoApp, width: u32) -> bool {
    responsive_toolbar_state(app, width).icon_only
}

#[cfg(test)]
fn compact_layout_for_breakpoint(width: u32, breakpoint: f32) -> bool {
    (width as f32) < breakpoint
}

fn configure_responsive_layout(app: &VideoApp, width: u32) {
    let state = responsive_toolbar_state(app, width);
    app.set_compact_layout(state.icon_only);
}

fn configure_direction(app: &VideoApp, rtl: bool) {
    app.set_rtl(rtl);
}

fn wire_responsive_layout(app: &VideoApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_layout(&app, width.max(0.0) as u32);
        }
    });
}

fn sample_project() -> VideoProject {
    let mut project = VideoProject::new("video-sample", "Documentary Assembly");
    let mut first = Clip::new("clip-1", "Opening Scene", 6.0);
    first.start_time = 0.0;
    let mut second = Clip::new("clip-2", "Interview Select", 10.5);
    second.start_time = 6.0;
    project.tracks[0].add_clip(first);
    project.tracks[0].add_clip(second);
    project
}

fn initial_session(args: &Args) -> Result<(VideoSession, Option<PathBuf>), String> {
    match args.open.as_deref() {
        Some(path) => {
            let p = PathBuf::from(path);
            let bytes = std::fs::read(&p)
                .map_err(|error| format!("failed to read video project '{path}': {error}"))?;
            let project = load_video_project(&bytes)?;
            Ok((VideoSession::new(project), Some(p)))
        }
        None => Ok((VideoSession::new(sample_project()), None)),
    }
}

struct AppState {
    session: Mutex<VideoSession>,
    save_path: Mutex<Option<PathBuf>>,
    dialogs: Arc<dyn FileDialogService>,
    selected_clip: Mutex<usize>,
    preview: Mutex<Option<VideoFrame>>,
    preview_synthetic: AtomicBool,
    tools: Option<MediaTools>,
    exporting: AtomicBool,
    export_cancel: ExportCancellation,
    preview_generation: PreviewGeneration,
    preview_cancel: Mutex<Option<PreviewCancellation>>,
    preview_in_flight: AtomicBool,
    preview_cache: Mutex<PreviewCache>,
    preview_cache_hits: AtomicU64,
    waveform_cache_hits: AtomicU64,
    gesture: Mutex<Option<TimelineGesture>>,
    playback_clock: Mutex<Option<PlaybackClock>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheState {
    Pending,
    Ready,
    Failed,
}

#[derive(Debug, Clone)]
struct PreviewCacheEntry {
    clip_id: String,
    source_identity: String,
    frame_time: f64,
    thumbnail: CacheState,
    waveform: CacheState,
    frame: Option<VideoFrame>,
    waveform_peaks: Option<Vec<(f32, f32)>>,
}

#[derive(Debug, Default)]
struct PreviewCache {
    entries: VecDeque<PreviewCacheEntry>,
}

impl PreviewCache {
    const LIMIT: usize = 8;

    fn mark_pending_at(&mut self, clip_id: &str, source_identity: &str, frame_time: f64) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.clip_id == clip_id && entry.source_identity == source_identity)
        {
            entry.frame_time = frame_time;
            entry.thumbnail = CacheState::Pending;
            entry.frame = None;
            return;
        }
        self.entries.retain(|entry| entry.clip_id != clip_id);
        self.entries.push_front(PreviewCacheEntry {
            clip_id: clip_id.to_string(),
            source_identity: source_identity.to_string(),
            frame_time,
            thumbnail: CacheState::Pending,
            waveform: CacheState::Pending,
            frame: None,
            waveform_peaks: None,
        });
        self.entries.truncate(Self::LIMIT);
    }

    fn mark_thumbnail_ready_at(
        &mut self,
        clip_id: &str,
        source_identity: &str,
        frame_time: f64,
        frame: VideoFrame,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.clip_id == clip_id && entry.source_identity == source_identity)
        {
            entry.frame_time = frame_time;
            entry.thumbnail = CacheState::Ready;
            entry.frame = Some(frame);
            return;
        }
        self.entries.push_front(PreviewCacheEntry {
            clip_id: clip_id.to_string(),
            source_identity: source_identity.to_string(),
            frame_time,
            thumbnail: CacheState::Ready,
            waveform: CacheState::Pending,
            frame: Some(frame),
            waveform_peaks: None,
        });
        self.entries.truncate(Self::LIMIT);
    }

    fn mark_thumbnail_failed(&mut self, clip_id: &str, source_identity: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.clip_id == clip_id && entry.source_identity == source_identity)
        {
            entry.thumbnail = CacheState::Failed;
            entry.frame = None;
        }
    }

    fn mark_waveform_ready(
        &mut self,
        clip_id: &str,
        source_identity: &str,
        peaks: Vec<(f32, f32)>,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.clip_id == clip_id && entry.source_identity == source_identity)
        {
            entry.waveform = CacheState::Ready;
            entry.waveform_peaks = Some(peaks);
        }
    }

    fn mark_waveform_failed(&mut self, clip_id: &str, source_identity: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.clip_id == clip_id && entry.source_identity == source_identity)
        {
            entry.waveform = CacheState::Failed;
            entry.waveform_peaks = None;
        }
    }

    fn cached_frame(
        &mut self,
        clip_id: &str,
        source_identity: &str,
        frame_time: f64,
    ) -> Option<VideoFrame> {
        let index = self.entries.iter().position(|entry| {
            entry.clip_id == clip_id
                && entry.source_identity == source_identity
                && entry.thumbnail == CacheState::Ready
                && entry
                    .frame
                    .as_ref()
                    .is_some_and(|_| (entry.frame_time - frame_time).abs() <= 1e-3)
        })?;
        let entry = self.entries.remove(index)?;
        let frame = entry.frame.clone();
        self.entries.push_front(entry);
        self.entries.truncate(Self::LIMIT);
        frame
    }

    fn cached_waveform(&mut self, clip_id: &str, source_identity: &str) -> Option<Vec<(f32, f32)>> {
        // Keep the immutable lookup as the single source of truth for both
        // status and production reads, then touch the entry for LRU serving.
        let peaks = self.waveform_for(clip_id, source_identity)?.to_vec();
        let index = self.entries.iter().position(|entry| {
            entry.clip_id == clip_id && entry.source_identity == source_identity
        })?;
        let entry = self.entries.remove(index)?;
        self.entries.push_front(entry);
        self.entries.truncate(Self::LIMIT);
        Some(peaks)
    }

    fn waveform_for(&self, clip_id: &str, source_identity: &str) -> Option<&[(f32, f32)]> {
        self.entries
            .iter()
            .find(|entry| {
                entry.clip_id == clip_id
                    && entry.source_identity == source_identity
                    && entry.waveform == CacheState::Ready
            })
            .and_then(|entry| entry.waveform_peaks.as_deref())
            .filter(|peaks| !peaks.is_empty())
    }

    fn status_for(&self, clip: &Clip) -> String {
        let source = clip.source_path.trim();
        if source.is_empty() {
            return "Offline sample · synthetic preview".to_string();
        }
        if !Path::new(source).is_file() {
            return "Source missing · relink required".to_string();
        }
        let identity = source_identity(Path::new(source));
        match self
            .entries
            .iter()
            .find(|entry| entry.clip_id == clip.id && entry.source_identity == identity)
        {
            Some(entry) => {
                let waveform_state = if self.waveform_for(&clip.id, &identity).is_some() {
                    CacheState::Ready
                } else {
                    entry.waveform
                };
                match (entry.thumbnail, waveform_state) {
                    (CacheState::Ready, CacheState::Ready) => {
                        "Thumbnail ready · waveform ready".into()
                    }
                    (CacheState::Ready, CacheState::Pending) => {
                        "Thumbnail ready · waveform pending".into()
                    }
                    (CacheState::Ready, CacheState::Failed) => {
                        "Thumbnail ready · waveform failed · retry on demand".into()
                    }
                    (CacheState::Pending, CacheState::Ready) => {
                        "Thumbnail pending · waveform ready".into()
                    }
                    (CacheState::Pending, CacheState::Failed) => {
                        "Thumbnail pending · waveform failed".into()
                    }
                    (CacheState::Failed, CacheState::Ready) => {
                        "Thumbnail failed · waveform ready · retry on demand".into()
                    }
                    (CacheState::Failed, CacheState::Failed) => {
                        "Thumbnail failed · waveform failed · retry on demand".into()
                    }
                    (CacheState::Failed, CacheState::Pending) => {
                        "Thumbnail failed · waveform pending · retry on demand".into()
                    }
                    (CacheState::Pending, CacheState::Pending) => {
                        "Thumbnail pending · waveform pending".into()
                    }
                }
            }
            None => "Thumbnail pending · waveform pending".into(),
        }
    }
}

fn source_identity(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let metadata = std::fs::metadata(path).ok();
    let size = metadata.as_ref().map(std::fs::Metadata::len).unwrap_or(0);
    let modified = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| format!("{}.{:09}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_default();
    format!("{}:{size}:{modified}", canonical.to_string_lossy())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GestureKind {
    Move,
    TrimIn,
    TrimOut,
}

#[derive(Debug)]
struct TimelineGesture {
    clip_id: String,
    track_index: usize,
    kind: GestureKind,
    baseline: VideoProject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockSource {
    AudioMaster,
    MonotonicFallback,
}

const AUDIO_CONSUMER_CHUNK_SAMPLES: usize = 2048;
const AUDIO_CONSUMER_QUEUE_CHUNKS: usize = 8;
const AUDIO_CONSUMER_TICK_HZ: u64 = 30;

/// Bounded local PCM decoder used as the audio-master source when no realtime
/// output device is available. The worker decodes mono f32 samples through
/// FFmpeg and sends only bounded sample-count chunks; the playback timer
/// consumes one fixed sample budget per tick.
#[derive(Debug)]
struct DecodedAudioSampleConsumer {
    sample_rate: u32,
    tools: MediaTools,
    path: PathBuf,
    start_time: f64,
    pending_samples: u64,
    receiver: Receiver<u64>,
    cancellation: PreviewCancellation,
    process: Arc<Mutex<Child>>,
    worker: Option<thread::JoinHandle<()>>,
    terminal: bool,
}

impl DecodedAudioSampleConsumer {
    fn spawn(
        tools: &MediaTools,
        path: &Path,
        start_time: f64,
        sample_rate: u32,
    ) -> Result<Self, String> {
        if sample_rate == 0 {
            return Err("decoded audio sample rate must be non-zero".into());
        }
        if !path.is_file() {
            return Err(format!("audio source does not exist: {}", path.display()));
        }
        let mut child = Command::new(&tools.ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{:.6}", start_time.max(0.0)),
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-ac",
                "1",
                "-ar",
                &sample_rate.to_string(),
                "-f",
                "f32le",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("start FFmpeg audio consumer: {error}"))?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("FFmpeg audio consumer stdout was not captured".into());
            }
        };
        let process = Arc::new(Mutex::new(child));
        let worker_process = Arc::clone(&process);
        let (sender, receiver) = mpsc::sync_channel(AUDIO_CONSUMER_QUEUE_CHUNKS);
        let cancellation = PreviewCancellation::default();
        let worker_cancellation = cancellation.clone();
        let worker = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut bytes = vec![0_u8; AUDIO_CONSUMER_CHUNK_SAMPLES * 4];
            let mut carry = [0_u8; std::mem::size_of::<f32>()];
            let mut carry_len = 0_usize;
            loop {
                if worker_cancellation.is_cancelled() {
                    if let Ok(mut process) = worker_process.lock() {
                        let _ = process.kill();
                    }
                    break;
                }
                let read = match reader.read(&mut bytes) {
                    Ok(read) => read,
                    Err(_) => break,
                };
                if read == 0 {
                    break;
                }
                let mut sample_count = 0_u64;
                let mut offset = 0_usize;
                if carry_len > 0 {
                    let needed = carry.len() - carry_len;
                    let copied = needed.min(read);
                    carry[carry_len..carry_len + copied].copy_from_slice(&bytes[..copied]);
                    carry_len += copied;
                    offset = copied;
                    if carry_len == carry.len() {
                        sample_count += 1;
                        carry_len = 0;
                    }
                }
                while offset + carry.len() <= read {
                    sample_count += 1;
                    offset += carry.len();
                }
                if offset < read {
                    // A valid FFmpeg f32le stream is four-byte aligned. Keep
                    // the tail until the next read instead of counting a
                    // partial sample; if the process ends with a tail it is
                    // intentionally ignored as malformed output.
                    carry[..read - offset].copy_from_slice(&bytes[offset..read]);
                    carry_len = read - offset;
                }
                let mut samples = sample_count;
                while samples > 0 {
                    let chunk = samples.min(AUDIO_CONSUMER_CHUNK_SAMPLES as u64);
                    loop {
                        match sender.try_send(chunk) {
                            Ok(()) => break,
                            Err(TrySendError::Full(value)) => {
                                samples = value;
                                if worker_cancellation.is_cancelled() {
                                    if let Ok(mut process) = worker_process.lock() {
                                        let _ = process.kill();
                                    }
                                    return;
                                }
                                thread::sleep(Duration::from_millis(2));
                            }
                            Err(TrySendError::Disconnected(_)) => {
                                if let Ok(mut process) = worker_process.lock() {
                                    let _ = process.kill();
                                }
                                return;
                            }
                        }
                    }
                    samples = samples.saturating_sub(chunk);
                }
            }
            if let Ok(mut process) = worker_process.lock() {
                let _ = process.wait();
            }
        });
        Ok(Self {
            sample_rate,
            tools: tools.clone(),
            path: path.to_path_buf(),
            start_time: start_time.max(0.0),
            pending_samples: 0,
            receiver,
            cancellation,
            process,
            worker: Some(worker),
            terminal: false,
        })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start_time(&self) -> f64 {
        self.start_time
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn consume_samples(&mut self, budget: u64) -> u64 {
        if budget == 0 {
            return 0;
        }
        while self.pending_samples < budget {
            match self.receiver.try_recv() {
                Ok(samples) => {
                    self.pending_samples = self.pending_samples.saturating_add(samples);
                }
                // The decoder runs asynchronously. If no chunk is ready for
                // this timer tick, leave the audio-master clock parked until
                // a later tick observes decoded samples. A disconnected
                // worker is terminal unless cancellation is replacing it
                // (for example during a seek), in which case the replacement
                // consumer owns the next samples.
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.cancellation.is_cancelled() {
                        self.terminal = true;
                    }
                    break;
                }
            }
        }
        let consumed = self.pending_samples.min(budget);
        self.pending_samples -= consumed;
        consumed
    }

    fn stop_worker(&mut self) {
        self.cancellation.cancel();
        if let Ok(mut process) = self.process.lock() {
            let _ = process.kill();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn seek(&mut self, position: f64) -> bool {
        let tools = self.tools.clone();
        let path = self.path.clone();
        let sample_rate = self.sample_rate;
        self.stop_worker();
        match Self::spawn(&tools, &path, position, sample_rate) {
            Ok(replacement) => {
                *self = replacement;
                true
            }
            Err(_) => {
                self.pending_samples = 0;
                self.terminal = false;
                false
            }
        }
    }
}

impl Drop for DecodedAudioSampleConsumer {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

fn spawn_decoded_audio_consumer(
    tools: &MediaTools,
    path: &Path,
    start_time: f64,
    sample_rate: u32,
) -> Result<DecodedAudioSampleConsumer, String> {
    DecodedAudioSampleConsumer::spawn(tools, path, start_time, sample_rate)
}

#[derive(Debug)]
struct AudioSampleSource {
    sample_rate: u32,
    timeline_cursor: f64,
    playback_rate: f64,
    fractional_source_samples: f64,
    consumer: DecodedAudioSampleConsumer,
}

impl AudioSampleSource {
    fn new(position: f64, consumer: DecodedAudioSampleConsumer, playback_rate: f64) -> Self {
        let sample_rate = consumer.sample_rate().max(1);
        Self {
            sample_rate,
            timeline_cursor: position.max(0.0) * f64::from(sample_rate),
            playback_rate: playback_rate.max(0.001),
            fractional_source_samples: 0.0,
            consumer,
        }
    }

    fn position(&self) -> f64 {
        self.timeline_cursor / f64::from(self.sample_rate)
    }

    fn consume_for_tick(&mut self) -> u64 {
        let exact_source_samples = f64::from(self.sample_rate) / AUDIO_CONSUMER_TICK_HZ as f64
            * self.playback_rate
            + self.fractional_source_samples;
        let source_budget = exact_source_samples.floor() as u64;
        self.fractional_source_samples = exact_source_samples - source_budget as f64;
        let consumed = self.consumer.consume_samples(source_budget);
        self.timeline_cursor += consumed as f64 / self.playback_rate;
        consumed
    }

    fn seek(&mut self, timeline_position: f64, source_position: f64) -> bool {
        self.timeline_cursor = timeline_position.max(0.0) * f64::from(self.sample_rate);
        self.fractional_source_samples = 0.0;
        self.consumer.seek(source_position)
    }
}

#[derive(Debug)]
struct PlaybackClock {
    source: ClockSource,
    anchor: Instant,
    anchor_position: f64,
    audio: Option<AudioSampleSource>,
    audio_clip_id: Option<String>,
    active_clip_id: Option<String>,
    audio_ended: bool,
    audio_status: &'static str,
}

impl PlaybackClock {
    #[cfg(test)]
    fn start(position: f64) -> Self {
        Self::start_for_clip_with_status(
            position,
            None,
            "No audio stream · monotonic fallback clock",
        )
    }

    fn start_for_clip_with_status(
        position: f64,
        clip_id: Option<String>,
        audio_status: &'static str,
    ) -> Self {
        Self {
            source: ClockSource::MonotonicFallback,
            anchor: Instant::now(),
            anchor_position: position.max(0.0),
            audio: None,
            audio_clip_id: None,
            active_clip_id: clip_id,
            audio_ended: false,
            audio_status,
        }
    }

    #[cfg(test)]
    fn start_audio(position: f64, consumer: DecodedAudioSampleConsumer) -> Self {
        Self::start_audio_for_clip(position, consumer, None, 1.0)
    }

    fn start_audio_for_clip(
        position: f64,
        consumer: DecodedAudioSampleConsumer,
        clip_id: Option<String>,
        playback_rate: f64,
    ) -> Self {
        Self {
            source: ClockSource::AudioMaster,
            anchor: Instant::now(),
            anchor_position: position.max(0.0),
            audio: Some(AudioSampleSource::new(position, consumer, playback_rate)),
            audio_clip_id: clip_id.clone(),
            active_clip_id: clip_id,
            audio_ended: false,
            audio_status:
                "Audio stream detected · output device unavailable; clock is audio-master",
        }
    }

    fn source(&self) -> ClockSource {
        self.source
    }

    fn is_audio_master_for_clip(&self, clip_id: Option<&str>) -> bool {
        clip_id.is_some_and(|clip_id| {
            self.source == ClockSource::AudioMaster
                && self.audio.is_some()
                && self.audio_clip_id.as_deref() == Some(clip_id)
        })
    }

    #[cfg(test)]
    fn audio_clip_id(&self) -> Option<&str> {
        self.audio_clip_id.as_deref()
    }

    fn active_clip_id(&self) -> Option<&str> {
        self.active_clip_id.as_deref()
    }

    fn audio_ended(&self) -> bool {
        self.audio_ended
    }

    fn audio_status(&self) -> &'static str {
        self.audio_status
    }

    fn position(&self) -> f64 {
        self.audio
            .as_ref()
            .map(AudioSampleSource::position)
            .unwrap_or_else(|| self.anchor_position + self.anchor.elapsed().as_secs_f64())
    }

    fn tick(&mut self) -> f64 {
        if let Some(audio) = self.audio.as_mut() {
            audio.consume_for_tick();
            if audio.consumer.is_terminal() {
                self.audio_ended = true;
            }
        }
        self.position()
    }

    #[cfg(test)]
    fn seek(&mut self, position: f64) {
        self.seek_with_source(position, position);
    }

    fn seek_with_source(&mut self, timeline_position: f64, source_position: f64) {
        let audio_seeked = if let Some(audio) = self.audio.as_mut() {
            audio.seek(timeline_position, source_position)
        } else {
            self.anchor = Instant::now();
            self.anchor_position = timeline_position.max(0.0);
            true
        };
        if audio_seeked {
            self.audio_ended = false;
        }
        if !audio_seeked {
            let active_clip_id = self.active_clip_id.clone();
            self.audio = None;
            self.source = ClockSource::MonotonicFallback;
            self.audio_clip_id = None;
            self.active_clip_id = active_clip_id;
            self.audio_ended = false;
            self.audio_status = "Audio seek unavailable · monotonic fallback clock";
            self.anchor = Instant::now();
            self.anchor_position = timeline_position.max(0.0);
        }
    }
}

#[derive(Debug)]
struct PreviewGeneration(AtomicU64);

impl Default for PreviewGeneration {
    fn default() -> Self {
        Self(AtomicU64::new(0))
    }
}

impl PreviewGeneration {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_current(&self, generation: u64) -> bool {
        self.0.load(Ordering::Acquire) == generation
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn invalidate_preview(state: &AppState) {
    state.preview_generation.next();
    if let Some(cancel) = lock(&state.preview_cancel).take() {
        cancel.cancel();
    }
}

fn video_track_index(project: &VideoProject) -> Option<usize> {
    project
        .tracks
        .iter()
        .position(|track| matches!(track.track_type, loom_video_core::TrackType::Video))
}

fn selected_clip_id(project: &VideoProject, track_index: usize, index: usize) -> Option<String> {
    project
        .tracks
        .get(track_index)
        .and_then(|track| track.clips.get(index))
        .map(|clip| clip.id.clone())
}

fn video_filter() -> FileFilter {
    FileFilter {
        name: "Loom Video Project (*.loomvideo)".into(),
        extensions: vec!["loomvideo".into()],
    }
}

fn media_filter() -> FileFilter {
    FileFilter {
        name: "Supported Media (*.mp4, *.mov, *.mkv, *.avi, *.wav, *.mp3)".into(),
        extensions: vec![
            "mp4".into(),
            "mov".into(),
            "mkv".into(),
            "avi".into(),
            "wav".into(),
            "mp3".into(),
        ],
    }
}

fn open_video_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Video Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![video_filter()],
    }
}

fn save_video_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Save Video Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled.loomvideo".into())),
        filters: vec![video_filter()],
    }
}

fn save_video_export_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Video Timeline".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled-export.mp4".into())),
        filters: vec![FileFilter {
            name: "MPEG-4 Video (*.mp4)".into(),
            extensions: vec!["mp4".into()],
        }],
    }
}

fn open_media_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Import Media".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![media_filter()],
    }
}

fn timeline_clips(project: &VideoProject) -> Vec<&Clip> {
    project
        .tracks
        .iter()
        .find(|track| matches!(track.track_type, loom_video_core::TrackType::Video))
        .map(|track| track.clips.iter().collect())
        .unwrap_or_default()
}

fn timeline_duration(project: &VideoProject) -> f64 {
    timeline_clips(project)
        .iter()
        .map(|clip| clip.end_time())
        .fold(0.0, f64::max)
        .max(0.01)
}

fn clip_display_name(clip: &Clip) -> String {
    if clip.source_path.trim().is_empty() {
        format!("{} · offline sample", clip.name)
    } else {
        clip.name.clone()
    }
}

fn procedural_preview() -> VideoFrame {
    let (width, height) = (640, 360);
    let mut pixels = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let nx = x as f32 / width as f32;
            let ny = y as f32 / height as f32;
            pixels[index] = (18.0 + nx * 95.0) as u8;
            pixels[index + 1] = (28.0 + ny * 48.0) as u8;
            pixels[index + 2] = (46.0 + (1.0 - nx) * 68.0) as u8;
            pixels[index + 3] = 255;
        }
    }
    VideoFrame {
        width: width as u32,
        height: height as u32,
        pixels,
    }
}

fn frame_image(frame: &VideoFrame) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &frame.pixels,
        frame.width,
        frame.height,
    ))
}

fn timecode(seconds: f64, frame_rate: f64) -> String {
    let seconds = seconds.max(0.0);
    let hours = (seconds / 3600.0).floor() as u64;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u64;
    let whole = (seconds % 60.0).floor() as u64;
    let frames = ((seconds.fract() * frame_rate.max(1.0)).round() as u64)
        .min(frame_rate.max(1.0) as u64 - 1);
    format!("{hours:02}:{minutes:02}:{whole:02}:{frames:02}")
}

fn refresh(app: &VideoApp, state: &AppState) {
    let session = lock(&state.session);
    let project = &session.project;
    app.set_project_name(project.name.as_str().into());
    app.set_project_format(
        format!(
            "{} × {} · {:.3} fps",
            project.width, project.height, project.frame_rate
        )
        .into(),
    );
    app.set_track_labels(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| SharedString::from(format!("{} · {:?}", track.name, track.track_type)))
            .collect::<Vec<_>>(),
    )));
    app.set_track_mutes(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.muted)
            .collect::<Vec<_>>(),
    )));
    app.set_track_solos(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.solo)
            .collect::<Vec<_>>(),
    )));
    app.set_active_track_index(project.active_track_index as i32);
    let clips = timeline_clips(project);
    app.set_clip_labels(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| SharedString::from(clip_display_name(clip)))
            .collect::<Vec<_>>(),
    )));
    app.set_clip_starts(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.start_time as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_durations(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.effective_timeline_duration() as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_in_points(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.in_point as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_out_points(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.out_point as f32)
            .collect::<Vec<_>>(),
    )));
    let cache_statuses = {
        let preview_cache = lock(&state.preview_cache);
        clips
            .iter()
            .map(|clip| SharedString::from(preview_cache.status_for(clip)))
            .collect::<Vec<_>>()
    };
    app.set_clip_cache_status(ModelRc::new(VecModel::from(cache_statuses)));
    let selected = (*lock(&state.selected_clip)).min(clips.len().saturating_sub(1));
    *lock(&state.selected_clip) = selected;
    app.set_active_clip_index(selected as i32);
    let duration = timeline_duration(project);
    app.set_timeline_duration(duration as f32);
    let zoom = f64::from(app.get_timeline_zoom()).clamp(1.0, 4.0);
    app.set_timeline_zoom(zoom as f32);
    let visible_duration = (duration / zoom).min(duration);
    let max_scroll = (duration - visible_duration).max(0.0);
    let scroll = f64::from(app.get_timeline_scroll()).clamp(0.0, max_scroll);
    app.set_timeline_scroll(scroll as f32);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    let media = project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .filter(|clip| !clip.source_path.is_empty())
        .map(|clip| {
            SharedString::from(
                Path::new(&clip.source_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&clip.name),
            )
        })
        .collect::<Vec<_>>();
    app.set_media_bin_items(ModelRc::new(VecModel::from(media)));
    app.set_backend_available(state.tools.is_some());
    app.set_backend_version(
        state
            .tools
            .as_ref()
            .map(|tools| SharedString::from(tools.version.as_str()))
            .unwrap_or_else(|| "Install FFmpeg, FFprobe and FFplay on PATH".into()),
    );
    let preview_synthetic = state.preview_synthetic.load(Ordering::Relaxed);
    app.set_preview_synthetic(preview_synthetic);
    app.set_status_right(
        if state.tools.is_some() {
            if preview_synthetic {
                "Local FFmpeg · synthetic preview"
            } else {
                "Local FFmpeg media"
            }
        } else {
            "Media backend unavailable"
        }
        .into(),
    );
    let (clock_source, clock_status) = {
        let clock = lock(&state.playback_clock);
        (
            clock.as_ref().map(PlaybackClock::source),
            clock
                .as_ref()
                .map(PlaybackClock::audio_status)
                .unwrap_or("No audio stream · monotonic fallback clock"),
        )
    };
    match clock_source {
        Some(ClockSource::AudioMaster) => {
            app.set_playback_clock_source("Audio master".into());
            app.set_audio_output_status(
                "Audio stream detected · output device unavailable; clock is audio-master".into(),
            );
        }
        Some(ClockSource::MonotonicFallback) => {
            app.set_playback_clock_source("Monotonic fallback".into());
            app.set_audio_output_status(clock_status.into());
        }
        None => {
            app.set_playback_clock_source("No clock".into());
            app.set_audio_output_status("Audio output unavailable".into());
        }
    }
    let path_label = lock(&state.save_path)
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into());
    app.set_status_left(
        format!(
            "{path_label} · {} tracks · {} clips · {} markers",
            project.tracks.len(),
            project.total_clips(),
            project.markers.len()
        )
        .into(),
    );
    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
    }
    if let Ok(bytes) = save_video_project(project) {
        let _ = record_snapshot_recovery("video state", bytes);
    }
}

/// Refreshes preview/cache projections without persisting a recovery snapshot.
/// Playback can request frames several times per second, so it must not route
/// through the full document refresh path (which checkpoints recovery state).
fn refresh_preview_surface(app: &VideoApp, state: &AppState) {
    let session = lock(&state.session);
    let clips = timeline_clips(&session.project);
    let cache_statuses = {
        let preview_cache = lock(&state.preview_cache);
        clips
            .iter()
            .map(|clip| SharedString::from(preview_cache.status_for(clip)))
            .collect::<Vec<_>>()
    };
    app.set_clip_cache_status(ModelRc::new(VecModel::from(cache_statuses)));
    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
    }
    let preview_synthetic = state.preview_synthetic.load(Ordering::Acquire);
    app.set_preview_synthetic(preview_synthetic);
    app.set_status_right(
        if state.tools.is_some() {
            if preview_synthetic {
                "Local FFmpeg · synthetic preview"
            } else {
                "Local FFmpeg media"
            }
        } else {
            "Media backend unavailable"
        }
        .into(),
    );
}

fn apply_theme(app: &VideoApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let (initial_proj, initial_path) = initial_session(args)?;
    let state = AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        preview_synthetic: AtomicBool::new(true),
        tools: discover_media_tools().ok(),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        preview_cancel: Mutex::new(None),
        preview_in_flight: AtomicBool::new(false),
        preview_cache: Mutex::new(PreviewCache::default()),
        preview_cache_hits: AtomicU64::new(0),
        waveform_cache_hits: AtomicU64::new(0),
        gesture: Mutex::new(None),
        playback_clock: Mutex::new(None),
    };
    refresh(&app, &state);
    if args.palette {
        app.set_palette_query(SharedString::from("pr"));
        rebuild_palette(&app, "pr");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn capture_workflow_step(
    app: &VideoApp,
    out_dir: &Path,
    name: &str,
    detail: &str,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let image = snapshot_component(app, 1280.0, 800.0, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let path = out_dir.join(format!("video-workflow-{name}.png"));
    loom_test_support::png::save_png(&path, &image)
        .map_err(|error| format!("save {}: {error}", path.display()))?;
    steps.push(format!("{name}: {detail} | artifact={}", path.display()));
    Ok(())
}

fn create_workflow_media(tools: &MediaTools, out_dir: &Path) -> Result<PathBuf, String> {
    let path = out_dir.join("video-workflow-source.mp4");
    let output = Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "4",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-pix_fmt",
            "yuv420p",
            "-shortest",
        ])
        .arg(&path)
        .output()
        .map_err(|error| format!("start journey media generator: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "journey media generator failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(path)
}

fn wait_for_export(state: &AppState, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while state.exporting.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if state.exporting.load(Ordering::Acquire) {
        Err("timed out waiting for timeline export worker".into())
    } else {
        Ok(())
    }
}

fn wait_for_decoded_preview(
    app: &VideoApp,
    state: &AppState,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while state.preview_synthetic.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if state.preview_synthetic.load(Ordering::Acquire) {
        return Err(format!(
            "timed out waiting for decoded in-window preview (status={})",
            app.get_status_left()
        ));
    }
    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
        app.set_preview_synthetic(false);
        app.set_status_right("Local FFmpeg media".into());
    }
    Ok(())
}

/// Execute the controller-backed import/edit/save/play/export workflow with
/// per-step screenshots and assertions tied to `AppState` and persisted data.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir).map_err(|error| format!("create journey output: {error}"))?;
    let tools = discover_media_tools()
        .map_err(|error| format!("Video journey requires local FFmpeg/FFprobe: {error}"))?;
    let source_path = create_workflow_media(&tools, out_dir)?;
    let probe = probe_media(&tools, &source_path)?;
    if !probe.has_audio || probe.width == 0 || probe.height == 0 || probe.duration <= 0.0 {
        return Err(format!(
            "journey source probe did not produce a video+audio stream: {probe:?}"
        ));
    }

    let project_path = out_dir.join("video-workflow.loomvideo");
    let export_path = out_dir.join("video-workflow-export.mp4");
    let cancel_path = out_dir.join("video-workflow-cancelled.mp4");
    let _ = std::fs::remove_file(&project_path);
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&cancel_path);

    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let (initial_proj, initial_path) = initial_session(args)?;
    let dialogs = ScriptedFileDialogs::new([Some(project_path.clone())], []);
    let state = Arc::new(AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(dialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        preview_synthetic: AtomicBool::new(true),
        tools: Some(tools.clone()),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        preview_cancel: Mutex::new(None),
        preview_in_flight: AtomicBool::new(false),
        preview_cache: Mutex::new(PreviewCache::default()),
        preview_cache_hits: AtomicU64::new(0),
        waveform_cache_hits: AtomicU64::new(0),
        gesture: Mutex::new(None),
        playback_clock: Mutex::new(None),
    });
    wire_application(&app, state.clone());
    wire_palette(&app);
    rebuild_palette(&app, "");
    let menu_bar = build_standard_menu_bar(
        "Loom Video",
        vec![MenuItem::action_with_shortcut(
            "file.export_video",
            "Export Timeline...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![Menu::new(
            "Clip",
            vec![
                MenuItem::action_with_shortcut(
                    "clip.split",
                    "Split Clip at Playhead",
                    MenuShortcut::primary("B"),
                ),
                MenuItem::action("clip.delete", "Ripple Delete Selected Clip"),
            ],
        )],
    );
    let _menu_service = install_video_menu(&app, menu_bar);
    refresh(&app, &state);

    let mut steps = Vec::new();
    capture_workflow_step(
        &app,
        out_dir,
        "00-initial",
        "sample project loaded",
        &mut steps,
    )?;

    app.invoke_import_media(source_path.to_string_lossy().into_owned().into());
    let imported_index = *lock(&state.selected_clip);
    let imported = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .get(imported_index)
            .map(|clip| (*clip).clone())
            .ok_or("import callback did not add a clip")?
    };
    if imported.source_path != source_path.to_string_lossy()
        || imported.duration <= 0.0
        || !app.get_can_undo()
    {
        return Err("import did not update the controller session and undo state".into());
    }
    app.invoke_select_clip(imported_index as i32);
    wait_for_decoded_preview(&app, &state, Duration::from_secs(8))?;
    capture_workflow_step(
        &app,
        out_dir,
        "01-import",
        "imported probed video+audio and decoded an in-window preview",
        &mut steps,
    )?;

    let original_start = imported.start_time;
    let original_duration = imported.duration;
    app.invoke_trim_selected(0.25, 0.0);
    let trimmed = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .get(imported_index)
            .map(|clip| (*clip).clone())
            .ok_or("trim removed the selected clip")?
    };
    if trimmed.start_time <= original_start
        || trimmed.duration >= original_duration
        || !app.get_can_undo()
    {
        return Err("trim callback did not produce a reversible timeline edit".into());
    }
    let trimmed_start = trimmed.start_time;
    app.invoke_move_clip(imported_index as i32, 0.5);
    let moved_start = timeline_clips(&lock(&state.session).project)
        .get(imported_index)
        .map(|clip| clip.start_time)
        .ok_or("move removed the selected clip")?;
    if (moved_start - (trimmed_start + 0.5)).abs() > 1e-6 {
        return Err(format!(
            "move callback landed at {moved_start}, expected {}",
            trimmed_start + 0.5
        ));
    }
    app.invoke_undo();
    let undone_start = timeline_clips(&lock(&state.session).project)
        .get(imported_index)
        .map(|clip| clip.start_time)
        .ok_or("undo removed the selected clip")?;
    if (undone_start - trimmed_start).abs() > 1e-6 {
        return Err("undo did not restore the pre-move timeline position".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "02-edit-undo",
        "trimmed, moved, and undid the move through controller callbacks",
        &mut steps,
    )?;

    *lock(&state.save_path) = Some(project_path.clone());
    app.invoke_save_project();
    if !project_path.is_file() {
        return Err("save callback did not write the journey project".into());
    }
    let persisted = load_video_project(
        &std::fs::read(&project_path)
            .map_err(|error| format!("read saved journey project: {error}"))?,
    )?;
    if persisted.tracks[0].clips.len() != timeline_clips(&lock(&state.session).project).len() {
        return Err("saved project clip count differs from the live controller".into());
    }
    app.invoke_open_project();
    let reopened = lock(&state.session).project.clone();
    if reopened.tracks[0].clips.len() != persisted.tracks[0].clips.len()
        || (reopened.tracks[0].clips[imported_index].in_point
            - persisted.tracks[0].clips[imported_index].in_point)
            .abs()
            > 1e-6
    {
        return Err("open callback did not restore the saved trim state".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "03-save-reopen",
        "saved to and reopened the package through the dialog-backed controller",
        &mut steps,
    )?;

    let playback_start = reopened.tracks[0].clips[imported_index].start_time + 0.5;
    app.invoke_seek(playback_start as f32);
    wait_for_decoded_preview(&app, &state, Duration::from_secs(8))?;
    app.invoke_play_pause();
    if !app.get_is_playing()
        || app.get_playback_clock_source().as_str() != "Audio master"
        || lock(&state.playback_clock)
            .as_ref()
            .map(PlaybackClock::source)
            != Some(ClockSource::AudioMaster)
    {
        return Err("playback did not select the audio-master clock".into());
    }
    app.invoke_seek((playback_start + 0.25) as f32);
    let seek_timeline = playback_start + 0.25;
    let expected_source_time = reopened.tracks[0].clips[imported_index].in_point
        + (seek_timeline - reopened.tracks[0].clips[imported_index].start_time)
            * reopened.tracks[0].clips[imported_index].playback_rate;
    let (seek_position, consumer_source_time) = {
        let clock = lock(&state.playback_clock);
        let clock = clock
            .as_ref()
            .ok_or("seek cleared the active playback clock")?;
        let consumer_source_time = clock
            .audio
            .as_ref()
            .map(|audio| audio.consumer.start_time())
            .ok_or("source-aware seek dropped the audio consumer")?;
        (clock.position(), consumer_source_time)
    };
    if (seek_position - playback_start - 0.25).abs() > 0.08 {
        return Err(format!("seek position drifted to {seek_position:.3}"));
    }
    if (consumer_source_time - expected_source_time).abs() > 0.08 {
        return Err(format!(
            "source-aware seek restarted audio at {consumer_source_time:.3}, expected {expected_source_time:.3}"
        ));
    }
    let seek_deadline = Instant::now() + Duration::from_secs(4);
    let mut resumed_position = seek_position;
    while Instant::now() < seek_deadline {
        resumed_position = lock(&state.playback_clock)
            .as_mut()
            .map(PlaybackClock::tick)
            .unwrap_or(seek_position);
        if resumed_position > seek_position {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if resumed_position <= seek_position {
        return Err("audio-master clock did not resume after source-aware seek".into());
    }
    app.invoke_play_pause();
    capture_workflow_step(
        &app,
        out_dir,
        "04-play-seek",
        "played the decoded preview with audio-master timing and sought",
        &mut steps,
    )?;

    app.set_export_path(export_path.to_string_lossy().into_owned().into());
    app.invoke_export_timeline(export_path.to_string_lossy().into_owned().into());
    wait_for_export(&state, Duration::from_secs(20))?;
    if !export_path.is_file() {
        return Err(format!(
            "completed export did not produce an output file (status={})",
            app.get_status_left()
        ));
    }
    capture_workflow_step(
        &app,
        out_dir,
        "05-export",
        "exported the edited timeline through the local FFmpeg worker",
        &mut steps,
    )?;

    let cancellation_destination = b"existing destination before cancellation";
    std::fs::write(&cancel_path, cancellation_destination)
        .map_err(|error| format!("seed cancellation destination: {error}"))?;
    app.invoke_export_timeline(cancel_path.to_string_lossy().into_owned().into());
    app.invoke_cancel_export();
    wait_for_export(&state, Duration::from_secs(20))?;
    if !state.export_cancel.is_cancelled() {
        return Err("cancel callback did not signal the export worker".into());
    }
    let destination_preserved = std::fs::read(&cancel_path)
        .map(|bytes| bytes == cancellation_destination)
        .unwrap_or(false);
    if !destination_preserved {
        return Err("cancelled export overwrote the existing destination".into());
    }
    let temporary_left = std::fs::read_dir(out_dir)
        .map_err(|error| format!("read cancellation artifacts: {error}"))?
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains(".loom-video-"));
    if temporary_left {
        return Err("cancelled export left a temporary output behind".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "06-export-cancel",
        "cancelled the second export while preserving its prior destination and removing partial output",
        &mut steps,
    )?;

    let report_path = out_dir.join("video-workflow.txt");
    let mut report = format!(
        "Video controller workflow: PASS\nsource={}\nproject={}\nexport={}\n\n",
        source_path.display(),
        project_path.display(),
        export_path.display()
    );
    report.push_str(&steps.join("\n"));
    report.push('\n');
    report.push_str(
        "Evidence limits: preview and export use local FFmpeg; no realtime audio device output is claimed.\n",
    );
    std::fs::write(&report_path, report)
        .map_err(|error| format!("write journey report {}: {error}", report_path.display()))?;
    println!("video workflow journey: PASS ({})", report_path.display());
    Ok(())
}

impl PaletteProbe for VideoApp {
    fn palette_open(&self) -> bool {
        self.get_palette_open()
    }

    fn palette_commands(&self) -> usize {
        self.get_palette_commands().row_count()
    }

    fn palette_selected(&self) -> i32 {
        self.get_palette_selected()
    }

    fn palette_query(&self) -> String {
        self.get_palette_query().to_string()
    }

    fn open_palette(&self) {
        self.invoke_open_palette();
    }
}

fn request_preview(state: Arc<AppState>, weak: slint::Weak<VideoApp>, timeline_time: f64) {
    request_preview_internal(state, weak, timeline_time, true);
}

fn request_playback_preview(state: Arc<AppState>, weak: slint::Weak<VideoApp>, timeline_time: f64) {
    request_preview_internal(state, weak, timeline_time, false);
}

fn request_preview_internal(
    state: Arc<AppState>,
    weak: slint::Weak<VideoApp>,
    timeline_time: f64,
    force: bool,
) {
    if !force && state.preview_in_flight.load(Ordering::Acquire) {
        return;
    }
    let generation = state.preview_generation.next();
    if let Some(previous) = lock(&state.preview_cancel).take() {
        previous.cancel();
    }
    let cancellation = PreviewCancellation::default();
    *lock(&state.preview_cancel) = Some(cancellation.clone());
    state.preview_in_flight.store(true, Ordering::Release);
    let source = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .into_iter()
            .find(|clip| timeline_time >= clip.start_time && timeline_time < clip.end_time())
            .map(|clip| {
                (
                    clip.id.clone(),
                    PathBuf::from(&clip.source_path),
                    clip.in_point + (timeline_time - clip.start_time) * clip.playback_rate,
                )
            })
    };
    let Some((clip_id, path, source_time)) = source.filter(|(_, path, _)| path.is_file()) else {
        state.preview_in_flight.store(false, Ordering::Release);
        let _ = lock(&state.preview_cancel).take();
        show_synthetic_preview(&state, &weak);
        return;
    };
    let cache_identity = source_identity(&path);
    let cached_waveform_bins = {
        let mut preview_cache = lock(&state.preview_cache);
        let cached = preview_cache.cached_waveform(&clip_id, &cache_identity);
        if let Some(peaks) = cached.as_ref() {
            state.waveform_cache_hits.fetch_add(1, Ordering::AcqRel);
            Some(peaks.len())
        } else {
            None
        }
    };
    let cached_frame = {
        let mut preview_cache = lock(&state.preview_cache);
        preview_cache.cached_frame(&clip_id, &cache_identity, source_time)
    };
    if let Some(frame) = cached_frame {
        let _ = lock(&state.preview_cancel).take();
        state.preview_in_flight.store(false, Ordering::Release);
        state.preview_cache_hits.fetch_add(1, Ordering::AcqRel);
        *lock(&state.preview) = Some(frame.clone());
        state.preview_synthetic.store(false, Ordering::Release);
        if let Some(app) = weak.upgrade() {
            refresh_preview_surface(&app, &state);
            app.set_preview_image(frame_image(&frame));
            app.set_has_preview(true);
            app.set_preview_synthetic(false);
            let waveform_status = cached_waveform_bins
                .map(|bins| format!(" · waveform {bins} bins"))
                .unwrap_or_default();
            app.set_status_left(
                format!("Preview cache hit at {timeline_time:.2}s{waveform_status}").into(),
            );
        }
        return;
    }
    let Some(tools) = state.tools.clone() else {
        state.preview_in_flight.store(false, Ordering::Release);
        let _ = lock(&state.preview_cancel).take();
        show_synthetic_preview(&state, &weak);
        return;
    };
    lock(&state.preview_cache).mark_pending_at(&clip_id, &cache_identity, source_time);
    if let Some(app) = weak.upgrade() {
        refresh_preview_surface(&app, &state);
    }
    let callback_state = state.clone();
    std::thread::spawn(move || {
        let result =
            decode_preview_frame_with_cancel(&tools, &path, source_time, 960, 540, &cancellation);
        match result {
            Ok(frame) => {
                if !state.preview_generation.is_current(generation) {
                    return;
                }
                *lock(&state.preview) = Some(frame.clone());
                lock(&state.preview_cache).mark_thumbnail_ready_at(
                    &clip_id,
                    &cache_identity,
                    source_time,
                    frame.clone(),
                );
                state.preview_synthetic.store(false, Ordering::Relaxed);
                let waveform_bins = if let Some(bins) = cached_waveform_bins {
                    Some(bins)
                } else {
                    let waveform_result =
                        decode_audio_waveform_with_cancel(&tools, &path, 256, &cancellation);
                    if !state.preview_generation.is_current(generation) {
                        return;
                    }
                    if let Ok(peaks) = waveform_result {
                        let bins = peaks.len();
                        lock(&state.preview_cache).mark_waveform_ready(
                            &clip_id,
                            &cache_identity,
                            peaks,
                        );
                        Some(bins)
                    } else {
                        lock(&state.preview_cache).mark_waveform_failed(&clip_id, &cache_identity);
                        None
                    }
                };
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if !callback_state.preview_generation.is_current(generation) {
                        return;
                    }
                    refresh_preview_surface(&app, &callback_state);
                    app.set_preview_image(frame_image(&frame));
                    app.set_has_preview(true);
                    app.set_preview_synthetic(false);
                    let waveform_status = waveform_bins
                        .map(|bins| format!(" · waveform {bins} bins"))
                        .unwrap_or_default();
                    app.set_status_left(
                        format!("Decoded preview at {timeline_time:.2}s{waveform_status}").into(),
                    );
                    callback_state
                        .preview_in_flight
                        .store(false, Ordering::Release);
                });
            }
            Err(error) => {
                if !state.preview_generation.is_current(generation) {
                    return;
                }
                lock(&state.preview_cache).mark_thumbnail_failed(&clip_id, &cache_identity);
                let frame = procedural_preview();
                *lock(&state.preview) = Some(frame);
                state.preview_synthetic.store(true, Ordering::Release);
                state.preview_in_flight.store(false, Ordering::Release);
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if callback_state.preview_generation.is_current(generation) {
                        refresh_preview_surface(&app, &callback_state);
                        app.set_status_left(format!("Preview decode failed: {error}").into());
                    }
                });
            }
        }
        if state.preview_generation.is_current(generation) {
            *lock(&state.preview_cancel) = None;
            state.preview_in_flight.store(false, Ordering::Release);
        }
    });
}

fn show_synthetic_preview(state: &AppState, weak: &slint::Weak<VideoApp>) {
    let frame = procedural_preview();
    let _ = lock(&state.preview_cancel).take();
    state.preview_in_flight.store(false, Ordering::Release);
    *lock(&state.preview) = Some(frame.clone());
    state.preview_synthetic.store(true, Ordering::Release);
    if let Some(app) = weak.upgrade() {
        app.set_preview_image(frame_image(&frame));
        app.set_has_preview(true);
        app.set_preview_synthetic(true);
        app.set_status_right(
            if state.tools.is_some() {
                "Local FFmpeg · synthetic preview"
            } else {
                "Media backend unavailable"
            }
            .into(),
        );
    }
}

#[derive(Debug)]
struct PlaybackClockSetup {
    clock: PlaybackClock,
    source_label: &'static str,
    audio_status: &'static str,
}

fn build_playback_clock(state: &AppState, playhead: f64) -> PlaybackClockSetup {
    let playhead = if playhead.is_finite() {
        playhead.max(0.0)
    } else {
        0.0
    };
    let source = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .into_iter()
            .find(|clip| playhead >= clip.start_time && playhead < clip.end_time())
            .map(|clip| {
                (
                    clip.id.clone(),
                    clip.source_path.clone(),
                    clip.in_point + (playhead - clip.start_time) * clip.playback_rate,
                    clip.playback_rate,
                )
            })
    };
    let source_clip_id = source.as_ref().map(|source| source.0.clone());
    let audio_probe = source.as_ref().and_then(|(_, path, _, _)| {
        state.tools.as_ref().and_then(|tools| {
            let path = Path::new(path);
            path.is_file()
                .then(|| probe_media(tools, path).ok())
                .flatten()
        })
    });
    let has_audio = audio_probe.as_ref().is_some_and(|probe| probe.has_audio);
    let audio_sample_rate = audio_probe
        .as_ref()
        .filter(|probe| probe.has_audio)
        .and_then(|probe| probe.audio_sample_rate);
    let fallback = |audio_status| PlaybackClockSetup {
        clock: PlaybackClock::start_for_clip_with_status(
            playhead,
            source_clip_id.clone(),
            audio_status,
        ),
        source_label: "Monotonic fallback",
        audio_status,
    };

    match (
        audio_sample_rate,
        source.as_ref(),
        state.tools.as_ref(),
    ) {
        (Some(sample_rate), Some((clip_id, path, source_time, playback_rate)), Some(tools)) => {
            match spawn_decoded_audio_consumer(tools, Path::new(path), *source_time, sample_rate) {
                Ok(consumer) => PlaybackClockSetup {
                    clock: PlaybackClock::start_audio_for_clip(
                        playhead,
                        consumer,
                        Some(clip_id.clone()),
                        *playback_rate,
                    ),
                    source_label: "Audio master",
                    audio_status:
                        "Audio stream detected · output device unavailable; clock is audio-master",
                },
                Err(_) => fallback(
                    "Audio stream detected · decoded consumer unavailable; monotonic fallback clock",
                ),
            }
        }
        _ if has_audio => fallback(
            "Audio stream detected · sample rate unavailable; monotonic fallback clock",
        ),
        _ => fallback("No audio stream · monotonic fallback clock"),
    }
}

fn apply_playback_clock_setup(app: &VideoApp, setup: &PlaybackClockSetup) {
    app.set_playback_clock_source(setup.source_label.into());
    app.set_audio_output_status(setup.audio_status.into());
}

fn clip_id_at(project: &VideoProject, position: f64) -> Option<String> {
    timeline_clips(project)
        .into_iter()
        .find(|clip| position >= clip.start_time && position < clip.end_time())
        .map(|clip| clip.id.clone())
}

fn advance_playback_tick(app: &VideoApp, state: &Arc<AppState>) -> bool {
    let (position, audio_ended) = {
        let mut clock = lock(&state.playback_clock);
        let Some(clock) = clock.as_mut() else {
            return false;
        };
        let position = clock.tick();
        (position, clock.audio_ended())
    };
    if audio_ended {
        let active_clip_id = lock(&state.playback_clock)
            .as_ref()
            .and_then(|clock| clock.active_clip_id().map(str::to_owned));
        *lock(&state.playback_clock) = Some(PlaybackClock::start_for_clip_with_status(
            position,
            active_clip_id,
            "Audio stream ended · monotonic fallback clock",
        ));
        app.set_playback_clock_source("Monotonic fallback".into());
        app.set_audio_output_status("Audio stream ended · monotonic fallback clock".into());
    }

    let current_clip_id = {
        let session = lock(&state.session);
        clip_id_at(&session.project, position)
    };
    let clip_changed = lock(&state.playback_clock)
        .as_ref()
        .is_some_and(|clock| clock.active_clip_id() != current_clip_id.as_deref());
    if clip_changed {
        let setup = build_playback_clock(state, position);
        apply_playback_clock_setup(app, &setup);
        *lock(&state.playback_clock) = Some(setup.clock);
    }

    let duration = timeline_duration(&lock(&state.session).project);
    if position >= duration {
        app.set_playhead_seconds(duration as f32);
        app.set_timecode_display(
            timecode(duration, lock(&state.session).project.frame_rate).into(),
        );
        app.invoke_stop_playback();
        false
    } else {
        app.set_playhead_seconds(position as f32);
        app.set_timecode_display(
            timecode(position, lock(&state.session).project.frame_rate).into(),
        );
        request_playback_preview(state.clone(), app.as_weak(), position);
        true
    }
}

fn wire_application(app: &VideoApp, state: Arc<AppState>) {
    let timer = Rc::new(slint::Timer::default());
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                timer.stop();
                invalidate_preview(&state);
                *lock(&state.playback_clock) = None;
                app.set_is_playing(false);
                *lock(&state.session) = VideoSession::new(empty_project());
                *lock(&state.save_path) = None;
                *lock(&state.selected_clip) = 0;
                *lock(&state.preview) = Some(procedural_preview());
                state.preview_synthetic.store(true, Ordering::Relaxed);
                refresh(&app, &state);
                app.set_status_left("New untitled project created".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = lock(&state.save_path).clone();
                let request = open_video_request(current_path.as_deref());
                match state.dialogs.open_file(&request) {
                    Ok(Some(path)) => match std::fs::read(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| load_video_project(&bytes))
                    {
                        Ok(project) => {
                            timer.stop();
                            invalidate_preview(&state);
                            *lock(&state.playback_clock) = None;
                            app.set_is_playing(false);
                            *lock(&state.session) = VideoSession::new(project);
                            *lock(&state.save_path) = Some(path.clone());
                            state.preview_synthetic.store(true, Ordering::Relaxed);
                            refresh(&app, &state);
                            let preview_time = {
                                let session = lock(&state.session);
                                timeline_clips(&session.project)
                                    .get(*lock(&state.selected_clip))
                                    .map(|clip| clip.start_time)
                            };
                            if let Some(preview_time) = preview_time {
                                request_preview(state.clone(), app.as_weak(), preview_time);
                            }
                            app.set_status_left(
                                format!("Opened {}", path.file_name().unwrap().to_string_lossy())
                                    .into(),
                            );
                        }
                        Err(error) => app.set_status_left(format!("Open failed: {error}").into()),
                    },
                    Ok(None) => {
                        app.set_status_left("Open cancelled".into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("Open dialog failed: {error}").into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let target_path = lock(&state.save_path).clone();
                let path_to_save = match target_path {
                    Some(p) => Some(p),
                    None => {
                        let req = save_video_request(None);
                        match state.dialogs.save_file(&req) {
                            Ok(Some(p)) => Some(p),
                            Ok(None) => {
                                app.set_status_left("Save cancelled".into());
                                return;
                            }
                            Err(error) => {
                                app.set_status_left(format!("Save dialog failed: {error}").into());
                                return;
                            }
                        }
                    }
                };

                if let Some(path) = path_to_save {
                    let result =
                        save_video_project(&lock(&state.session).project).and_then(|bytes| {
                            loom_storage::atomic_write(&path, &bytes)
                                .map_err(|error| error.to_string())
                                .and_then(|_| checkpoint_snapshot_recovery(bytes))
                        });
                    match result {
                        Ok(()) => {
                            *lock(&state.save_path) = Some(path.clone());
                            refresh(&app, &state);
                            app.set_status_left(
                                format!("Saved {}", path.file_name().unwrap().to_string_lossy())
                                    .into(),
                            );
                        }
                        Err(error) => {
                            app.set_status_left(format!("Save failed: {error}").into());
                        }
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = lock(&state.save_path).clone();
                let req = save_video_request(current_path.as_deref());
                match state.dialogs.save_file(&req) {
                    Ok(Some(path)) => {
                        let result =
                            save_video_project(&lock(&state.session).project).and_then(|bytes| {
                                loom_storage::atomic_write(&path, &bytes)
                                    .map_err(|error| error.to_string())
                                    .and_then(|_| checkpoint_snapshot_recovery(bytes))
                            });
                        match result {
                            Ok(()) => {
                                *lock(&state.save_path) = Some(path.clone());
                                refresh(&app, &state);
                                app.set_status_left(
                                    format!(
                                        "Saved As {}",
                                        path.file_name().unwrap().to_string_lossy()
                                    )
                                    .into(),
                                );
                            }
                            Err(error) => {
                                app.set_status_left(format!("Save As failed: {error}").into());
                            }
                        }
                    }
                    Ok(None) => {
                        app.set_status_left("Save As cancelled".into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("Save As dialog failed: {error}").into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                invalidate_preview(&state);
                lock(&state.session).undo();
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                invalidate_preview(&state);
                lock(&state.session).redo();
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_import_media(move |path| {
            if let Some(app) = app_ref.upgrade() {
                let Some(tools) = state.tools.as_ref() else {
                    app.set_status_left("FFmpeg tools are unavailable".into());
                    return;
                };
                let chosen_path = if path.trim().is_empty() {
                    let current_dir = lock(&state.save_path).clone();
                    let req = open_media_request(current_dir.as_deref());
                    match state.dialogs.open_file(&req) {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            app.set_status_left("Import cancelled".into());
                            return;
                        }
                        Err(e) => {
                            app.set_status_left(format!("Import dialog failed: {e}").into());
                            return;
                        }
                    }
                } else {
                    PathBuf::from(path.trim())
                };

                match probe_media(tools, &chosen_path) {
                    Ok(probe) => {
                        let mut session = lock(&state.session);
                        let Some(track_index) = session.project.tracks.iter().position(|track| {
                            matches!(track.track_type, loom_video_core::TrackType::Video)
                        }) else {
                            app.set_status_left("Import failed: project has no video track".into());
                            return;
                        };
                        let start = session.project.tracks[track_index].duration();
                        let count = session.project.total_clips() + 1;
                        let mut clip = Clip::new(
                            format!("clip-{count}"),
                            chosen_path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Imported Clip"),
                            probe.duration.max(0.001),
                        );
                        clip.source_path = chosen_path.to_string_lossy().into_owned();
                        clip.start_time = start;
                        let result: Result<(), loom_video_core::TimelineError> = session
                            .apply_edit(|project| {
                                project.width = probe.width.max(1);
                                project.height = probe.height.max(1);
                                if probe.frame_rate > 0.0 {
                                    project.frame_rate = probe.frame_rate;
                                }
                                project.tracks[track_index].insert_clip(clip)
                            });
                        match result {
                            Ok(()) => {
                                *lock(&state.selected_clip) = session.project.tracks[track_index]
                                    .clips
                                    .len()
                                    .saturating_sub(1);
                                let preview_time = session.project.tracks[track_index]
                                    .clips
                                    .last()
                                    .map(|clip| clip.start_time);
                                invalidate_preview(&state);
                                state.preview_synthetic.store(true, Ordering::Relaxed);
                                drop(session);
                                refresh(&app, &state);
                                if let Some(preview_time) = preview_time {
                                    request_preview(state.clone(), app.as_weak(), preview_time);
                                }
                                app.set_status_left(
                                    format!(
                                        "Imported {}",
                                        chosen_path.file_name().unwrap().to_string_lossy()
                                    )
                                    .into(),
                                );
                            }
                            Err(error) => {
                                app.set_status_left(format!("Import failed: {error}").into());
                            }
                        }
                    }
                    Err(error) => app.set_status_left(format!("Probe failed: {error}").into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_track(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = lock(&state.session);
                    if (index as usize) < session.project.tracks.len() {
                        session.project.active_track_index = index as usize;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_track_mute(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = lock(&state.session);
                    let result: Result<(), loom_video_core::TimelineError> =
                        session.apply_edit(|project| {
                            let track = project
                                .tracks
                                .get_mut(index as usize)
                                .ok_or(loom_video_core::TimelineError::InvalidTrack)?;
                            track.muted = !track.muted;
                            Ok(())
                        });
                    drop(session);
                    if result.is_ok() {
                        refresh(&app, &state);
                    } else {
                        app.set_status_left("Mute failed: invalid track".into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_track_solo(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = lock(&state.session);
                    let result: Result<(), loom_video_core::TimelineError> =
                        session.apply_edit(|project| {
                            let track = project
                                .tracks
                                .get_mut(index as usize)
                                .ok_or(loom_video_core::TimelineError::InvalidTrack)?;
                            track.solo = !track.solo;
                            Ok(())
                        });
                    drop(session);
                    if result.is_ok() {
                        refresh(&app, &state);
                    } else {
                        app.set_status_left("Solo failed: invalid track".into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_clip(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    *lock(&state.selected_clip) = index as usize;
                    let time = {
                        let session = lock(&state.session);
                        timeline_clips(&session.project)
                            .get(index as usize)
                            .map(|clip| clip.start_time)
                            .unwrap_or(0.0)
                    };
                    app.set_playhead_seconds(time as f32);
                    app.set_timecode_display(
                        timecode(time, lock(&state.session).project.frame_rate).into(),
                    );
                    refresh(&app, &state);
                    request_preview(state.clone(), app.as_weak(), time);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_begin_clip_gesture(move |index, kind| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let kind = match kind.as_str() {
                    "Move" => GestureKind::Move,
                    "TrimIn" => GestureKind::TrimIn,
                    "TrimOut" => GestureKind::TrimOut,
                    _ => return,
                };
                let session = lock(&state.session);
                let Some(track_index) = video_track_index(&session.project) else {
                    app.set_status_left("Gesture unavailable: project has no video track".into());
                    return;
                };
                let Some(clip_id) = selected_clip_id(&session.project, track_index, index as usize)
                else {
                    app.set_status_left("Gesture unavailable: selected clip is unavailable".into());
                    return;
                };
                let baseline = session.project.clone();
                *lock(&state.gesture) = Some(TimelineGesture {
                    clip_id,
                    track_index,
                    kind,
                    baseline,
                });
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_end_clip_gesture(move || {
            if let Some(app) = app_ref.upgrade() {
                let Some(gesture) = lock(&state.gesture).take() else {
                    return;
                };
                let mut session = lock(&state.session);
                let changed = session.commit_gesture(gesture.baseline);
                let selected_after = session.project.tracks[gesture.track_index]
                    .clips
                    .iter()
                    .position(|clip| clip.id == gesture.clip_id);
                if let Some(index) = selected_after {
                    *lock(&state.selected_clip) = index;
                }
                drop(session);
                if changed {
                    invalidate_preview(&state);
                }
                refresh(&app, &state);
                app.set_status_left(if changed {
                    "Committed timeline gesture".into()
                } else {
                    "Timeline gesture cancelled (no changes)".into()
                });
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cancel_clip_gesture(move || {
            if let Some(app) = app_ref.upgrade() {
                let Some(gesture) = lock(&state.gesture).take() else {
                    return;
                };
                let mut session = lock(&state.session);
                session.rollback_gesture(gesture.baseline);
                let selected_after = session.project.tracks[gesture.track_index]
                    .clips
                    .iter()
                    .position(|clip| clip.id == gesture.clip_id)
                    .unwrap_or(0);
                *lock(&state.selected_clip) = selected_after;
                drop(session);
                invalidate_preview(&state);
                refresh(&app, &state);
                app.set_status_left("Timeline gesture cancelled".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_move_clip(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                let selected = index as usize;
                let mut session = lock(&state.session);
                let gesture = lock(&state.gesture);
                let gesture_target = gesture
                    .as_ref()
                    .filter(|gesture| gesture.kind == GestureKind::Move)
                    .map(|gesture| (gesture.track_index, gesture.clip_id.clone()));
                drop(gesture);
                let track_index = gesture_target
                    .as_ref()
                    .map(|(track_index, _)| *track_index)
                    .or_else(|| video_track_index(&session.project));
                let Some(track_index) = track_index else {
                    app.set_status_left("Move failed: project has no video track".into());
                    return;
                };
                let clip_id = gesture_target
                    .map(|(_, clip_id)| clip_id)
                    .or_else(|| selected_clip_id(&session.project, track_index, selected));
                let Some(clip_id) = clip_id else {
                    app.set_status_left("Move failed: selected clip is unavailable".into());
                    return;
                };
                let gesture_active = lock(&state.gesture)
                    .as_ref()
                    .is_some_and(|gesture| gesture.kind == GestureKind::Move);
                let snap_enabled = app.get_snap_enabled();
                let edit = |project: &mut VideoProject| {
                    let clip = project.tracks[track_index]
                        .clips
                        .iter()
                        .find(|clip| clip.id == clip_id)
                        .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                    let mut target = clip.start_time + f64::from(delta);
                    if snap_enabled {
                        target = snap_timeline_to_edit_points(
                            target,
                            &project.tracks,
                            &project.markers,
                            0.1,
                        );
                    }
                    project.move_clip(track_index, track_index, &clip_id, target, false)
                };
                let result = if gesture_active {
                    session.apply_edit_without_history(edit)
                } else {
                    session.apply_edit(edit)
                };
                if let Err(error) = result {
                    app.set_status_left(format!("Move failed: {error}").into());
                    if gesture_active {
                        drop(session);
                        app.invoke_cancel_clip_gesture();
                    }
                } else {
                    let selected_after = session.project.tracks[track_index]
                        .clips
                        .iter()
                        .position(|clip| clip.id == clip_id)
                        .unwrap_or(selected);
                    *lock(&state.selected_clip) = selected_after;
                    drop(session);
                    invalidate_preview(&state);
                    refresh(&app, &state);
                    if !gesture_active {
                        app.set_status_left("Moved selected clip".into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_remove_clip(move || {
            if let Some(app) = app_ref.upgrade() {
                let selected = *lock(&state.selected_clip);
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if let Some(id) = session.project.tracks[track_index]
                        .clips
                        .get(selected)
                        .map(|clip| clip.id.clone())
                    {
                        let result = session.apply_edit(|project| {
                            project.tracks[track_index]
                                .remove_clip(&id, true)
                                .map(|_| ())
                        });
                        if result.is_ok() {
                            *lock(&state.selected_clip) = selected.saturating_sub(1);
                            invalidate_preview(&state);
                        } else {
                            app.set_status_left(
                                "Remove failed: selected clip is unavailable".into(),
                            );
                        }
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_split_clip(move || {
            if let Some(app) = app_ref.upgrade() {
                let selected = *lock(&state.selected_clip);
                let playhead = app.get_playhead_seconds() as f64;
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if let Some(id) = session.project.tracks[track_index]
                        .clips
                        .get(selected)
                        .map(|clip| clip.id.clone())
                    {
                        let result = session.apply_edit(|project| {
                            project.split_clip(track_index, &id, playhead).map(|_| ())
                        });
                        match result {
                            Ok(()) => {
                                invalidate_preview(&state);
                                app.set_status_left(
                                    format!("Split clip at {:.2}s", playhead).into(),
                                );
                            }
                            Err(error) => {
                                app.set_status_left(format!("Split failed: {error}").into())
                            }
                        }
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_trim_selected(move |in_delta, out_delta| {
            if let Some(app) = app_ref.upgrade() {
                if !in_delta.is_finite()
                    || !out_delta.is_finite()
                    || (in_delta.abs() < f32::EPSILON && out_delta.abs() < f32::EPSILON)
                {
                    return;
                }
                let selected = *lock(&state.selected_clip);
                let mut session = lock(&state.session);
                let gesture = lock(&state.gesture);
                let gesture_target = gesture
                    .as_ref()
                    .filter(|gesture| {
                        matches!(gesture.kind, GestureKind::TrimIn | GestureKind::TrimOut)
                    })
                    .map(|gesture| (gesture.track_index, gesture.clip_id.clone(), gesture.kind));
                drop(gesture);
                let track_index = gesture_target
                    .as_ref()
                    .map(|(track_index, _, _)| *track_index)
                    .or_else(|| video_track_index(&session.project));
                if let Some(track_index) = track_index {
                    let clip_id = gesture_target
                        .as_ref()
                        .map(|(_, clip_id, _)| clip_id.clone())
                        .or_else(|| selected_clip_id(&session.project, track_index, selected));
                    if let Some(clip_id) = clip_id {
                        let gesture_active = gesture_target.is_some();
                        let snap_enabled = app.get_snap_enabled();
                        let edit = |project: &mut VideoProject| {
                            let mut snap_start = None;
                            let mut snap_end = None;
                            if in_delta != 0.0 {
                                let clip = project.tracks[track_index]
                                    .clips
                                    .iter_mut()
                                    .find(|clip| clip.id == clip_id)
                                    .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                                clip.trim_in((clip.in_point + f64::from(in_delta)).max(0.0))?;
                                snap_start = Some(clip.start_time);
                            } else if out_delta != 0.0 {
                                let clip = project.tracks[track_index]
                                    .clips
                                    .iter_mut()
                                    .find(|clip| clip.id == clip_id)
                                    .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                                clip.trim_out(
                                    (clip.out_point + f64::from(out_delta))
                                        .max(clip.in_point + 0.01),
                                )?;
                                snap_end = Some(clip.end_time());
                            }
                            if snap_enabled {
                                if let Some(start) = snap_start {
                                    let snapped = snap_timeline_to_edit_points(
                                        start,
                                        &project.tracks,
                                        &project.markers,
                                        0.1,
                                    );
                                    let delta = snapped - start;
                                    if delta.abs() > f64::EPSILON && snapped >= 0.0 {
                                        let clip = project.tracks[track_index]
                                            .clips
                                            .iter_mut()
                                            .find(|clip| clip.id == clip_id)
                                            .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                                        clip.start_time = snapped;
                                        clip.in_point += delta * clip.playback_rate;
                                        clip.sync_duration()?;
                                    }
                                }
                                if let Some(end) = snap_end {
                                    let snapped = snap_timeline_to_edit_points(
                                        end,
                                        &project.tracks,
                                        &project.markers,
                                        0.1,
                                    );
                                    let clip = project.tracks[track_index]
                                        .clips
                                        .iter_mut()
                                        .find(|clip| clip.id == clip_id)
                                        .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                                    if snapped > clip.start_time + 0.01 {
                                        clip.out_point +=
                                            (snapped - clip.end_time()) * clip.playback_rate;
                                        clip.sync_duration()?;
                                    }
                                }
                            }
                            project.tracks[track_index].sort_clips();
                            Ok(())
                        };
                        let result: Result<(), loom_video_core::TimelineError> = if gesture_active {
                            session.apply_edit_without_history(edit)
                        } else {
                            session.apply_edit(edit)
                        };
                        if let Err(error) = result {
                            app.set_status_left(format!("Trim failed: {error}").into());
                            if gesture_active {
                                drop(session);
                                app.invoke_cancel_clip_gesture();
                            }
                        } else {
                            let selected_after = session.project.tracks[track_index]
                                .clips
                                .iter()
                                .position(|clip| clip.id == clip_id)
                                .unwrap_or(selected);
                            *lock(&state.selected_clip) = selected_after;
                            drop(session);
                            invalidate_preview(&state);
                            refresh(&app, &state);
                        }
                    }
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_select_nle_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(format!("{} tool active", tool.as_str()).into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_snap(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(
                    format!(
                        "Timeline snapping {}",
                        if app.get_snap_enabled() { "on" } else { "off" }
                    )
                    .into(),
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_trim_clip_in(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                *lock(&state.selected_clip) = index as usize;
                app.set_active_clip_index(index);
                app.invoke_trim_selected(delta, 0.0);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_trim_clip_out(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                *lock(&state.selected_clip) = index as usize;
                app.set_active_clip_index(index);
                app.invoke_trim_selected(0.0, delta);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_seek(move |seconds| {
            if let Some(app) = app_ref.upgrade() {
                let duration = timeline_duration(&lock(&state.session).project);
                let seconds = f64::from(seconds).clamp(0.0, duration);
                app.set_playhead_seconds(seconds as f32);
                app.set_timecode_display(
                    timecode(seconds, lock(&state.session).project.frame_rate).into(),
                );
                let sought_source = {
                    let session = lock(&state.session);
                    timeline_clips(&session.project)
                        .into_iter()
                        .find(|clip| seconds >= clip.start_time && seconds < clip.end_time())
                        .map(|clip| {
                            (
                                clip.id.clone(),
                                clip.in_point + (seconds - clip.start_time) * clip.playback_rate,
                            )
                        })
                };
                if let Some(clock) = lock(&state.playback_clock).as_mut() {
                    let sought_clip_id =
                        sought_source.as_ref().map(|(clip_id, _)| clip_id.as_str());
                    let in_place_audio_seek = clock.is_audio_master_for_clip(sought_clip_id);
                    let identity_free_fallback = clock.active_clip_id().is_none()
                        && clock.source() == ClockSource::MonotonicFallback;
                    let clip_matches = match (clock.active_clip_id(), sought_clip_id) {
                        (Some(active_clip_id), Some(sought_clip_id)) => {
                            active_clip_id == sought_clip_id
                        }
                        (Some(_), None) => false,
                        // A clock without a clip identity is only used by
                        // focused unit tests; let it seek in place rather
                        // than inventing a source mismatch.
                        (None, _) => true,
                    };
                    if in_place_audio_seek || identity_free_fallback {
                        let was_audio_master = clock.source() == ClockSource::AudioMaster;
                        clock.seek_with_source(
                            seconds,
                            sought_source
                                .as_ref()
                                .map(|(_, source_time)| *source_time)
                                .unwrap_or(seconds),
                        );
                        if was_audio_master && clock.source() != ClockSource::AudioMaster {
                            app.set_playback_clock_source("Monotonic fallback".into());
                            app.set_audio_output_status(
                                "Audio seek unavailable · monotonic fallback clock".into(),
                            );
                        }
                    } else if clip_matches {
                        let setup = build_playback_clock(&state, seconds);
                        apply_playback_clock_setup(&app, &setup);
                        *clock = setup.clock;
                    } else {
                        // Keep the target unresolved so the next playback
                        // tick probes its own media and can attach the right
                        // audio consumer. Persist the diagnostic in the
                        // clock so refreshes before that tick stay honest.
                        *clock = PlaybackClock::start_for_clip_with_status(
                            seconds,
                            None,
                            "Audio source changed · monotonic fallback clock",
                        );
                        app.set_playback_clock_source("Monotonic fallback".into());
                        app.set_audio_output_status(
                            "Audio source changed · monotonic fallback clock".into(),
                        );
                    }
                }
                request_preview(state.clone(), app.as_weak(), seconds);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_playing() {
                    timer.stop();
                    let position = lock(&state.playback_clock)
                        .as_mut()
                        .map(PlaybackClock::tick)
                        .unwrap_or_else(|| f64::from(app.get_playhead_seconds()));
                    *lock(&state.playback_clock) = None;
                    app.set_playhead_seconds(position as f32);
                    app.set_is_playing(false);
                    invalidate_preview(&state);
                    refresh(&app, &state);
                    app.set_status_left("Playback paused".into());
                    return;
                }
                let playhead = f64::from(app.get_playhead_seconds());
                let setup = build_playback_clock(&state, playhead);
                apply_playback_clock_setup(&app, &setup);
                *lock(&state.playback_clock) = Some(setup.clock);
                let source_path = timeline_clips(&lock(&state.session).project)
                    .into_iter()
                    .find(|clip| playhead >= clip.start_time && playhead < clip.end_time())
                    .map(|clip| clip.source_path.clone());
                app.set_is_playing(true);
                app.set_status_left(
                    if source_path
                        .as_deref()
                        .is_some_and(|path| Path::new(path).is_file())
                    {
                        "Playing decoded preview in-window"
                    } else {
                        "Playing in-window preview · offline sample"
                    }
                    .into(),
                );
                request_preview(state.clone(), app.as_weak(), playhead);
                let weak = app.as_weak();
                let timer_state = state.clone();
                timer.start(
                    slint::TimerMode::Repeated,
                    std::time::Duration::from_millis(33),
                    move || {
                        if let Some(app) = weak.upgrade() {
                            let _ = advance_playback_tick(&app, &timer_state);
                        }
                    },
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_stop_playback(move || {
            if let Some(app) = app_ref.upgrade() {
                timer.stop();
                *lock(&state.playback_clock) = None;
                app.set_is_playing(false);
                invalidate_preview(&state);
                refresh(&app, &state);
                app.set_status_left("Playback stopped".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_marker(move || {
            if let Some(app) = app_ref.upgrade() {
                let time = app.get_playhead_seconds() as f64;
                let mut session = lock(&state.session);
                let count = session.project.markers.len() + 1;
                let result = session.apply_edit(|project| {
                    project.add_marker(TimelineMarker {
                        id: format!("marker-{count}"),
                        time,
                        label: format!("Marker {count}"),
                        color: "#d97745".into(),
                    })
                });
                drop(session);
                if result.is_ok() {
                    refresh(&app, &state);
                } else {
                    app.set_status_left("Marker failed: invalid playhead".into());
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cancel_export(move || {
            if state.exporting.load(Ordering::Acquire) {
                state.export_cancel.cancel();
                if let Some(app) = app_ref.upgrade() {
                    app.set_status_left("Cancelling export…".into());
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_export_timeline(move |path| {
            let mut output = if path.trim().is_empty() {
                let current_path = lock(&state.save_path).clone();
                match state
                    .dialogs
                    .save_file(&save_video_export_request(current_path.as_deref()))
                {
                    Ok(Some(path)) => path,
                    Ok(None) => {
                        if let Some(app) = weak.upgrade() {
                            app.set_status_left("Export cancelled".into());
                        }
                        return;
                    }
                    Err(error) => {
                        if let Some(app) = weak.upgrade() {
                            app.set_status_left(format!("Export dialog failed: {error}").into());
                        }
                        return;
                    }
                }
            } else {
                PathBuf::from(path.trim())
            };
            if output.extension().is_none() {
                output.set_extension("mp4");
            }
            if state.exporting.swap(true, Ordering::SeqCst) {
                return;
            }
            state.export_cancel.reset();
            let Some(tools) = state.tools.clone() else {
                state.exporting.store(false, Ordering::SeqCst);
                if let Some(app) = weak.upgrade() {
                    app.set_status_left("FFmpeg/FFprobe are unavailable".into());
                }
                return;
            };
            let project = lock(&state.session).project.clone();
            let worker_state = state.clone();
            let worker_weak = weak.clone();
            let cancel = state.export_cancel.clone();
            let display_output = output.clone();
            std::thread::spawn(move || {
                let plan = build_timeline_export_plan(&project, &tools, &output);
                let result = match plan {
                    Ok(plan) => execute_timeline_export_with_cancel(
                        &plan,
                        {
                            let progress_weak = worker_weak.clone();
                            move |progress| {
                                let _ = progress_weak.upgrade_in_event_loop(move |app| {
                                    app.set_export_progress(progress * 100.0);
                                    app.set_status_left(
                                        format!("Rendering timeline · {:.0}%", progress * 100.0)
                                            .into(),
                                    );
                                });
                            }
                        },
                        &cancel,
                    ),
                    Err(error) => Err(error),
                };
                worker_state.exporting.store(false, Ordering::SeqCst);
                let _ = worker_weak.upgrade_in_event_loop(move |app| {
                    app.set_exporting(false);
                    app.set_status_left(
                        match result {
                            Ok(()) => format!("Exported {}", output.display()),
                            Err(error) if error.contains("cancel") => {
                                "Export cancelled; no completed file was produced".into()
                            }
                            Err(error) => format!("Export failed: {error}"),
                        }
                        .into(),
                    );
                });
            });
            if let Some(app) = weak.upgrade() {
                app.set_export_path(display_output.to_string_lossy().into_owned().into());
                app.set_exporting(true);
                app.set_export_progress(0.0);
            }
        });
    }
}

fn install_video_menu(app: &VideoApp, mut menu_bar: MenuBar) -> NativeMenuBar {
    menu_bar.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "edit.undo",
        "edit.redo",
        "app.palette",
        "file.export_video",
        "clip.split",
        "clip.delete",
        "view.inspector",
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
        "app.quit",
        "window.minimize",
        "window.zoom",
        "window.bring_all_to_front",
        "help.documentation",
        "help.shortcuts",
        "help.feedback",
    ]);
    let service = NativeMenuBar::new();
    let _ = service.install_menu_bar(&menu_bar);
    let weak = app.as_weak();
    let sink: MenuActionSink = Arc::new(move |action: CommandAction| {
        let Some(app) = weak.upgrade() else {
            return Ok(());
        };
        match action.id.as_str() {
            "file.new" => app.invoke_new_project(),
            "file.open" => app.invoke_open_project(),
            "file.save" => app.invoke_save_project(),
            "file.save_as" => app.invoke_save_as_project(),
            "edit.undo" => app.invoke_undo(),
            "edit.redo" => app.invoke_redo(),
            "app.palette" => app.invoke_open_palette(),
            "file.export_video" => app.invoke_export_timeline(app.get_export_path()),
            "clip.split" => app.invoke_split_clip(),
            "clip.delete" => app.invoke_remove_clip(),
            "view.inspector" => app.set_show_inspector(!app.get_show_inspector()),
            "view.zoom_in" => app.set_timeline_zoom((app.get_timeline_zoom() + 0.5).min(4.0)),
            "view.zoom_out" => app.set_timeline_zoom((app.get_timeline_zoom() - 0.5).max(1.0)),
            "view.zoom_actual" => app.set_timeline_zoom(1.0),
            "help.documentation" => app.set_status_left("Loom Video documentation is local".into()),
            "help.shortcuts" => app.set_status_left("Use Ctrl/Cmd+K for commands".into()),
            "help.feedback" => app.set_status_left("Feedback is unavailable offline".into()),
            _ => {}
        }
        Ok(())
    });
    let _ = service.register_action_sink(sink);
    service
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-video-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    wire_responsive_layout(&app);
    let recovered = initialize_snapshot_recovery()?;
    let (initial_proj, initial_path) = if args.open.is_some() {
        initial_session(&args)?
    } else {
        match recovered
            .as_deref()
            .and_then(|bytes| load_video_project(bytes).ok())
        {
            Some(p) => (VideoSession::new(p), None),
            None => initial_session(&args)?,
        }
    };
    let state = Arc::new(AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        preview_synthetic: AtomicBool::new(true),
        tools: discover_media_tools().ok(),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        preview_cancel: Mutex::new(None),
        preview_in_flight: AtomicBool::new(false),
        preview_cache: Mutex::new(PreviewCache::default()),
        preview_cache_hits: AtomicU64::new(0),
        waveform_cache_hits: AtomicU64::new(0),
        gesture: Mutex::new(None),
        playback_clock: Mutex::new(None),
    });

    wire_application(&app, state.clone());
    let menu_bar = build_standard_menu_bar(
        "Loom Video",
        vec![MenuItem::action_with_shortcut(
            "file.export_video",
            "Export Timeline...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![Menu::new(
            "Clip",
            vec![
                MenuItem::action_with_shortcut(
                    "clip.split",
                    "Split Clip at Playhead",
                    MenuShortcut::primary("B"),
                ),
                MenuItem::action("clip.delete", "Ripple Delete Selected Clip"),
            ],
        )],
    );
    let _menu_service = install_video_menu(&app, menu_bar);

    wire_palette(&app);
    refresh(&app, &state);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Commands exposed through the command palette.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewProject,
    OpenProject,
    SaveProject,
    SaveAsProject,
    Undo,
    Redo,
    ImportMedia,
    SplitClip,
    RemoveClip,
    PlayPause,
    Stop,
    SelectClip(i32),
    Export,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

const PALETTE_IMPORT_SOURCE: &str = "";

fn master_palette(app: &VideoApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewProject,
            id: "video.new",
            label: "New Project",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenProject,
            id: "video.open",
            label: "Open Project",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveProject,
            id: "video.save",
            label: "Save Project",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsProject,
            id: "video.save-as",
            label: "Save Project As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "video.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "video.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::ImportMedia,
            id: "video.import",
            label: "Import Media",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::SplitClip,
            id: "video.split",
            label: "Split Clip",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::RemoveClip,
            id: "video.remove-clip",
            label: "Remove Clip",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::PlayPause,
            id: "video.play-pause",
            label: "Play / Pause",
            shortcut: "Space",
        },
        PaletteCommand {
            action: PaletteAction::Stop,
            id: "video.stop",
            label: "Stop Playback",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SelectClip(0),
            id: "video.select-clip",
            label: "Select Clip 1",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Export,
            id: "video.export",
            label: "Export Timeline",
            shortcut: "Ctrl+E",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &VideoApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| {
            query_lower.is_empty()
                || c.label.to_lowercase().contains(&query_lower)
                || c.id.to_lowercase().contains(&query_lower)
        })
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
}

fn wire_palette(app: &VideoApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| match c.action {
                        PaletteAction::Undo => app.get_can_undo(),
                        PaletteAction::Redo => app.get_can_redo(),
                        _ => true,
                    })
                    .filter(|c| {
                        let q = app.get_palette_query().trim().to_lowercase();
                        q.is_empty()
                            || c.label.to_lowercase().contains(&q)
                            || c.id.to_lowercase().contains(&q)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewProject => app.invoke_new_project(),
                        PaletteAction::OpenProject => app.invoke_open_project(),
                        PaletteAction::SaveProject => app.invoke_save_project(),
                        PaletteAction::SaveAsProject => app.invoke_save_as_project(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::ImportMedia => {
                            app.invoke_import_media(PALETTE_IMPORT_SOURCE.into())
                        }
                        PaletteAction::SplitClip => app.invoke_split_clip(),
                        PaletteAction::RemoveClip => app.invoke_remove_clip(),
                        PaletteAction::PlayPause => app.invoke_play_pause(),
                        PaletteAction::Stop => app.invoke_stop_playback(),
                        PaletteAction::SelectClip(index) => app.invoke_select_clip(index),
                        PaletteAction::Export => app.invoke_export_timeline(app.get_export_path()),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::{DesktopError, SaveFileRequest, ScriptedFileDialogs};

    #[derive(Default)]
    struct RecordingDialogs {
        save_results: Mutex<VecDeque<Option<PathBuf>>>,
        save_requests: Mutex<Vec<SaveFileRequest>>,
    }

    impl RecordingDialogs {
        fn with_save_results(results: impl IntoIterator<Item = Option<PathBuf>>) -> Self {
            Self {
                save_results: Mutex::new(results.into_iter().collect()),
                save_requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl FileDialogService for RecordingDialogs {
        fn open_file(&self, _request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
            Ok(None)
        }

        fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
            lock(&self.save_requests).push(request.clone());
            lock(&self.save_results)
                .pop_front()
                .ok_or(DesktopError::ScriptExhausted("save_file"))
        }
    }

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (VideoApp, Arc<AppState>) {
        test_app_and_state_with_dialogs(Arc::new(scripted))
    }

    fn test_app_and_state_with_dialogs(
        dialogs: Arc<dyn FileDialogService>,
    ) -> (VideoApp, Arc<AppState>) {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(sample_project())),
            save_path: Mutex::new(None),
            dialogs,
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: None,
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
    }

    fn test_app_and_state_with_project(
        project: VideoProject,
        tools: MediaTools,
    ) -> (VideoApp, Arc<AppState>) {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(project)),
            save_path: Mutex::new(None),
            dialogs: Arc::new(ScriptedFileDialogs::default()),
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: Some(tools),
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
    }

    #[test]
    fn new_project_creates_untitled_clean_state() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(PathBuf::from("/tmp/existing.loomvideo"));

        app.invoke_new_project();
        assert_eq!(*lock(&state.save_path), None);
        assert_eq!(lock(&state.session).project.name, "Untitled Project");
        assert_eq!(app.get_project_name().as_str(), "Untitled Project");
    }

    #[test]
    fn offline_sample_clips_are_labelled_without_source_paths() {
        let mut offline = Clip::new("clip", "Opening Scene", 2.0);
        assert_eq!(
            clip_display_name(&offline),
            "Opening Scene · offline sample"
        );
        offline.source_path = "/tmp/source.mov".into();
        assert_eq!(clip_display_name(&offline), "Opening Scene");
    }

    #[test]
    fn open_project_with_dialog_loads_path_and_updates_state() {
        let dir = std::env::temp_dir().join(format!("loom-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("open_test.loomvideo");

        let mut proj = VideoProject::new("loaded-proj", "Loaded Video Project");
        proj.tracks[0].clips.clear();
        let bytes = save_video_project(&proj).unwrap();
        std::fs::write(&file, bytes).unwrap();

        let scripted = ScriptedFileDialogs::new(vec![Some(file.clone())], vec![]);

        let (app, state) = test_app_and_state(scripted);
        app.invoke_open_project();

        assert_eq!(*lock(&state.save_path), Some(file));
        assert_eq!(lock(&state.session).project.name, "Loaded Video Project");
        assert_eq!(app.get_project_name().as_str(), "Loaded Video Project");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_open_leaves_current_session_untouched() {
        let scripted = ScriptedFileDialogs::new(vec![None], vec![]); // User clicked Cancel in native open dialog

        let (app, state) = test_app_and_state(scripted);
        let original_name = lock(&state.session).project.name.clone();

        app.invoke_open_project();
        assert_eq!(lock(&state.session).project.name, original_name);
        assert_eq!(app.get_status_left().as_str(), "Open cancelled");
    }

    #[test]
    fn save_untitled_prompts_dialog_and_writes_file() {
        let dir = std::env::temp_dir().join(format!("loom-video-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved_project.loomvideo");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file.clone())]);

        let (app, state) = test_app_and_state(scripted);
        assert_eq!(*lock(&state.save_path), None);

        app.invoke_save_project();

        assert_eq!(*lock(&state.save_path), Some(file.clone()));
        assert!(file.is_file());
        let read_bytes = std::fs::read(&file).unwrap();
        let loaded = load_video_project(&read_bytes).unwrap();
        assert_eq!(loaded.name, "Documentary Assembly");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_prompts_dialog_and_updates_path() {
        let dir = std::env::temp_dir().join(format!("loom-video-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_v1 = dir.join("v1.loomvideo");
        let file_v2 = dir.join("v2.loomvideo");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file_v2.clone())]);

        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(file_v1);

        app.invoke_save_as_project();

        assert_eq!(*lock(&state.save_path), Some(file_v2.clone()));
        assert!(file_v2.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timeline_callbacks_select_trim_move_split_and_undo() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());

        app.invoke_select_clip(1);
        assert_eq!(*lock(&state.selected_clip), 1);
        assert_eq!(app.get_active_clip_index(), 1);
        assert_eq!(app.get_clip_in_points().row_count(), 2);

        let original = lock(&state.session).project.tracks[0].clips[1].clone();
        app.invoke_trim_selected(0.5, 0.0);
        let trimmed = lock(&state.session).project.tracks[0].clips[1].clone();
        assert!(trimmed.start_time > original.start_time);
        assert!(trimmed.duration < original.duration);

        app.invoke_move_clip(1, 0.75);
        let moved = lock(&state.session).project.tracks[0].clips[1].start_time;
        assert!((moved - (trimmed.start_time + 0.75)).abs() < 1e-6);
        app.invoke_undo();
        let restored = lock(&state.session).project.tracks[0].clips[1].start_time;
        assert!((restored - trimmed.start_time).abs() < 1e-6);

        app.set_playhead_seconds(8.0);
        app.invoke_split_clip();
        assert_eq!(lock(&state.session).project.tracks[0].clips.len(), 3);
        assert!(app.get_can_undo());
    }

    #[test]
    fn timeline_gesture_commits_once_and_cancel_restores_baseline() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        app.set_snap_enabled(false);
        let baseline = lock(&state.session).project.clone();

        app.invoke_begin_clip_gesture(1, "Move".into());
        app.invoke_move_clip(1, 0.25);
        app.invoke_move_clip(1, 0.25);
        assert!(!lock(&state.session).can_undo());
        assert_ne!(lock(&state.session).project, baseline);

        app.invoke_end_clip_gesture();
        assert!(lock(&state.session).can_undo());
        app.invoke_undo();
        assert_eq!(lock(&state.session).project, baseline);
        assert!(!lock(&state.session).can_undo());

        app.invoke_begin_clip_gesture(1, "Move".into());
        app.invoke_move_clip(1, 0.5);
        app.invoke_cancel_clip_gesture();
        assert_eq!(lock(&state.session).project, baseline);
        assert!(!lock(&state.session).can_undo());
    }

    #[test]
    fn moving_clip_across_neighbor_keeps_clip_selected_after_sort() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());

        app.invoke_select_clip(0);
        let moved_id = lock(&state.session).project.tracks[0].clips[0].id.clone();
        app.invoke_move_clip(0, 10.0);

        let session = lock(&state.session);
        let clips = &session.project.tracks[0].clips;
        let moved_index = clips
            .iter()
            .position(|clip| clip.id == moved_id)
            .expect("moved clip remains in timeline");
        assert_eq!(moved_index, 1);
        assert_eq!(*lock(&state.selected_clip), moved_index);
        assert_eq!(app.get_active_clip_index(), moved_index as i32);
    }

    #[test]
    fn selecting_offline_clip_restores_synthetic_preview() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        state.preview_synthetic.store(false, Ordering::Release);
        app.set_preview_synthetic(false);

        app.invoke_select_clip(0);

        assert!(state.preview_synthetic.load(Ordering::Acquire));
        assert!(app.get_preview_synthetic());
        assert!(app.get_has_preview());
    }

    #[test]
    fn zero_delta_trim_does_not_create_history() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        assert!(!lock(&state.session).can_undo());
        app.invoke_trim_selected(0.0, 0.0);
        assert!(!lock(&state.session).can_undo());
    }

    #[test]
    fn preview_cache_reports_thumbnail_progress_and_failure() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-preview-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mp4");
        std::fs::write(&source, b"fixture").unwrap();
        let mut clip = Clip::new("clip", "Clip", 2.0);
        clip.source_path = source.to_string_lossy().into_owned();
        let identity = source_identity(&source);
        let mut cache = PreviewCache::default();
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail pending · waveform pending"
        );
        cache.mark_pending_at(&clip.id, &identity, 0.0);
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail pending · waveform pending"
        );
        cache.mark_thumbnail_ready_at(
            &clip.id,
            &identity,
            0.0,
            VideoFrame {
                width: 2,
                height: 2,
                pixels: vec![255; 16],
            },
        );
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail ready · waveform pending"
        );
        cache.mark_thumbnail_failed(&clip.id, &identity);
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail failed · waveform pending · retry on demand"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_export_path_uses_mp4_save_dialog() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-export-dialog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("chosen.mp4");
        let dialogs = Arc::new(RecordingDialogs::with_save_results([Some(output)]));
        let (app, _state) = test_app_and_state_with_dialogs(dialogs.clone());

        app.invoke_export_timeline("".into());

        let requests = lock(&dialogs.save_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].title, "Export Video Timeline");
        assert_eq!(
            requests[0].suggested_name.as_deref(),
            Some("Untitled-export.mp4")
        );
        assert_eq!(requests[0].filters[0].extensions, ["mp4"]);
        assert_eq!(
            app.get_status_left().as_str(),
            "FFmpeg/FFprobe are unavailable"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_reopen_preserves_trimmed_clip_state() {
        let dir = std::env::temp_dir().join(format!("loom-video-reopen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("edited.loomvideo");
        let scripted = ScriptedFileDialogs::new([Some(file.clone())], []);
        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(file.clone());

        app.invoke_select_clip(0);
        app.invoke_trim_selected(0.5, 0.0);
        let expected_in = lock(&state.session).project.tracks[0].clips[0].in_point;
        app.invoke_save_project();
        assert!(file.is_file());
        app.invoke_open_project();
        let reopened_in = lock(&state.session).project.tracks[0].clips[0].in_point;
        assert!((reopened_in - expected_in).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_export_callback_signals_active_worker() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        state.exporting.store(true, Ordering::Release);
        app.invoke_cancel_export();
        assert!(state.export_cancel.is_cancelled());
        state.exporting.store(false, Ordering::Release);
    }

    #[test]
    fn compact_layout_boundary_keeps_reference_width_stable() {
        // The breakpoint is owned by the shared Slint policy; the pure helper
        // keeps the boundary test independent from AppKit's main-thread window
        // requirement on macOS.
        assert!(compact_layout_for_breakpoint(1024, 1180.0));
        assert!(compact_layout_for_breakpoint(1179, 1180.0));
        assert!(!compact_layout_for_breakpoint(1180, 1180.0));
        assert!(!compact_layout_for_breakpoint(1440, 1180.0));
    }

    #[test]
    fn responsive_policy_transition_probes_are_exact() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let expected = [
            (1179, true, true, false),
            (1180, false, true, false),
            (1279, false, true, false),
            (1280, false, true, false),
            (1319, false, true, false),
            (1320, false, false, true),
        ];
        for (width, icon_only, overflow, labeled) in expected {
            assert_eq!(
                responsive_toolbar_state(&app, width),
                ResponsiveToolbarState {
                    icon_only,
                    overflow,
                    labeled,
                }
            );
            assert_eq!(compact_layout_for_width(&app, width), icon_only);
        }
    }

    #[test]
    fn monotonic_fallback_clock_exposes_seek_position() {
        let mut clock = PlaybackClock::start(2.0);
        assert_eq!(clock.source(), ClockSource::MonotonicFallback);
        assert!(clock.position() >= 2.0);
        clock.seek(4.5);
        assert!((clock.position() - 4.5).abs() < 0.05);
    }

    #[test]
    fn cross_clip_seek_keeps_source_change_status_through_refresh() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        *lock(&state.playback_clock) = Some(PlaybackClock::start_for_clip_with_status(
            0.0,
            Some("clip-1".into()),
            "Audio seek unavailable · monotonic fallback clock",
        ));

        app.invoke_seek(6.0);

        {
            let clock = lock(&state.playback_clock);
            let clock = clock.as_ref().expect("seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::MonotonicFallback);
            assert_eq!(clock.active_clip_id(), None);
            assert_eq!(
                clock.audio_status(),
                "Audio source changed · monotonic fallback clock"
            );
        }
        assert_eq!(
            app.get_audio_output_status().as_str(),
            "Audio source changed · monotonic fallback clock"
        );

        refresh(&app, &state);
        assert_eq!(
            app.get_audio_output_status().as_str(),
            "Audio source changed · monotonic fallback clock"
        );
        app.invoke_stop_playback();
    }

    #[test]
    fn preview_generation_rejects_stale_results() {
        let generation = PreviewGeneration::default();
        let first = generation.next();
        let second = generation.next();
        assert!(generation.is_current(second));
        assert!(!generation.is_current(first));
    }

    #[test]
    fn audio_master_clock_tick_consumes_decoded_samples_and_stalls_without_them() {
        let tools = discover_media_tools().expect("FFmpeg is required for audio clock test");
        let dir =
            std::env::temp_dir().join(format!("loom-video-audio-clock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let probe = probe_media(&tools, &source).expect("probe audio/video fixture");
        assert!(probe.has_audio);
        let sample_rate = probe
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let stalled_consumer = spawn_decoded_audio_consumer(&tools, &source, 999.0, sample_rate)
            .expect("spawn bounded local audio consumer");
        let mut stalled_clock = PlaybackClock::start_audio(0.0, stalled_consumer);
        std::thread::sleep(Duration::from_millis(100));
        let stalled_position = stalled_clock.position();
        assert_eq!(stalled_clock.tick(), stalled_position);

        let consumer = spawn_decoded_audio_consumer(&tools, &source, 0.0, sample_rate)
            .expect("spawn bounded local audio consumer");
        let mut clock = PlaybackClock::start_audio(0.0, consumer);
        assert_eq!(clock.source(), ClockSource::AudioMaster);
        let first_frame = decode_preview_frame(&tools, &source, clock.position(), 320, 180)
            .expect("decode first preview frame");
        let initial_position = clock.position();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut advanced_position = initial_position;
        while Instant::now() < deadline {
            advanced_position = clock.tick();
            if advanced_position > initial_position {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            advanced_position > initial_position,
            "decoded sample consumer never advanced the audio clock"
        );
        let second_frame = decode_preview_frame(&tools, &source, clock.position(), 320, 180)
            .expect("decode advanced preview frame");
        assert_ne!(first_frame.pixels, second_frame.pixels);
        clock.seek_with_source(1.0, 0.5);
        assert_eq!(clock.source(), ClockSource::AudioMaster);
        assert!((clock.position() - 1.0).abs() < 1e-6);
        assert!(
            (clock
                .audio
                .as_ref()
                .expect("audio clock should retain its consumer after seek")
                .consumer
                .start_time()
                - 0.5)
                .abs()
                < 1e-6
        );
        let seek_position = clock.position();
        let seek_deadline = Instant::now() + Duration::from_secs(4);
        let mut seek_advanced = seek_position;
        while Instant::now() < seek_deadline {
            seek_advanced = clock.tick();
            if seek_advanced > seek_position {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            seek_advanced > seek_position,
            "audio clock did not resume after source-aware seek"
        );
        drop(clock);
        drop(stalled_clock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_audio_eof_switches_playing_controller_to_monotonic_fallback() {
        let tools = discover_media_tools().expect("FFmpeg is required for audio clock test");
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-eof-controller-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let sample_rate = probe_media(&tools, &source)
            .expect("probe audio/video fixture")
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        project.tracks[0].clips[0].duration = 6.0;
        project.tracks[0].clips[0].out_point = 6.0;
        let (app, state) = test_app_and_state_with_project(project, tools);
        let consumer = spawn_decoded_audio_consumer(
            state.tools.as_ref().unwrap(),
            &source,
            999.0,
            sample_rate,
        )
        .expect("spawn bounded local audio consumer");
        *lock(&state.playback_clock) = Some(PlaybackClock::start_audio_for_clip(
            0.0,
            consumer,
            Some("clip-1".into()),
            1.0,
        ));
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .is_some_and(|clock| clock.source() == ClockSource::AudioMaster)
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }

        assert!(app.get_is_playing());
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .map(PlaybackClock::source),
            Some(ClockSource::MonotonicFallback)
        );
        assert_eq!(
            app.get_playback_clock_source().as_str(),
            "Monotonic fallback"
        );
        assert!(app
            .get_audio_output_status()
            .to_string()
            .contains("Audio stream ended"));
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_clip_fallback_seek_rebuilds_audio_clock_and_preserves_unavailable_fallback() {
        let tools = discover_media_tools().expect("FFmpeg is required for audio clock test");
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-fallback-seek-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let sample_rate = probe_media(&tools, &source)
            .expect("probe audio/video fixture")
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        let (app, state) = test_app_and_state_with_project(project.clone(), tools.clone());
        let consumer = spawn_decoded_audio_consumer(
            state.tools.as_ref().unwrap(),
            &source,
            999.0,
            sample_rate,
        )
        .expect("spawn bounded local audio consumer");
        *lock(&state.playback_clock) = Some(PlaybackClock::start_audio_for_clip(
            0.0,
            consumer,
            Some("clip-1".into()),
            1.0,
        ));
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .is_some_and(|clock| clock.source() == ClockSource::AudioMaster)
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .map(PlaybackClock::source),
            Some(ClockSource::MonotonicFallback)
        );
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .and_then(|clock| clock.active_clip_id()),
            Some("clip-1")
        );

        app.invoke_seek(0.75);

        {
            let clock = lock(&state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("same-clip seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::AudioMaster);
            assert_eq!(clock.audio_clip_id(), Some("clip-1"));
            assert_eq!(clock.active_clip_id(), Some("clip-1"));
            assert!((clock.position() - 0.75).abs() < 1e-6);
            assert!(
                (clock
                    .audio
                    .as_ref()
                    .expect("same-clip fallback seek should reattach audio")
                    .consumer
                    .start_time()
                    - 0.75)
                    .abs()
                    < 1e-6
            );
        }
        assert_eq!(app.get_playback_clock_source().as_str(), "Audio master");

        let failed_tools = MediaTools {
            ffmpeg: dir.join("missing-ffmpeg"),
            ..tools
        };
        let (fallback_app, fallback_state) = test_app_and_state_with_project(project, failed_tools);
        *lock(&fallback_state.playback_clock) = Some(PlaybackClock::start_for_clip_with_status(
            0.0,
            Some("clip-1".into()),
            "Audio stream ended · monotonic fallback clock",
        ));

        fallback_app.invoke_seek(0.75);

        {
            let clock = lock(&fallback_state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("failed consumer seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::MonotonicFallback);
            assert_eq!(clock.active_clip_id(), Some("clip-1"));
            assert_eq!(
                clock.audio_status(),
                "Audio stream detected · decoded consumer unavailable; monotonic fallback clock"
            );
        }
        assert_eq!(
            fallback_app.get_audio_output_status().as_str(),
            "Audio stream detected · decoded consumer unavailable; monotonic fallback clock"
        );

        fallback_app.invoke_stop_playback();
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_master_boundary_rebuilds_consumer_for_next_clip() {
        let tools = discover_media_tools().expect("FFmpeg is required for audio clock test");
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-boundary-controller-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");

        let mut project = sample_project();
        let source_path = source.to_string_lossy().into_owned();
        let first = &mut project.tracks[0].clips[0];
        first.source_path = source_path.clone();
        first.duration = 0.1;
        first.in_point = 0.0;
        first.out_point = 0.1;
        let second = &mut project.tracks[0].clips[1];
        second.source_path = source_path;
        second.start_time = 0.1;
        second.duration = 0.2;
        second.in_point = 1.25;
        second.out_point = 1.65;
        second.playback_rate = 2.0;
        let (app, state) = test_app_and_state_with_project(project, tools);
        let setup = build_playback_clock(&state, 0.0);
        assert_eq!(setup.clock.source(), ClockSource::AudioMaster);
        *lock(&state.playback_clock) = Some(setup.clock);
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .and_then(PlaybackClock::audio_clip_id)
                != Some("clip-2")
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }

        {
            let clock = lock(&state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("boundary should retain playback clock");
            assert_eq!(clock.source(), ClockSource::AudioMaster);
            assert_eq!(clock.audio_clip_id(), Some("clip-2"));
            let position = clock.position();
            let expected_source_time = 1.25 + (position - 0.1) * 2.0;
            let actual_source_time = clock
                .audio
                .as_ref()
                .expect("next clip should have decoded audio")
                .consumer
                .start_time();
            assert!((actual_source_time - expected_source_time).abs() < 0.08);
        }
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waveform_cache_survives_thumbnail_seek_and_invalidates_source() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-waveform-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source_a = dir.join("source-a.mp4");
        let source_b = dir.join("source-b.mp4");
        std::fs::write(&source_a, b"a").unwrap();
        std::fs::write(&source_b, b"b").unwrap();
        let identity_a = source_identity(&source_a);
        let identity_b = source_identity(&source_b);
        let mut cache = PreviewCache::default();
        let peaks = vec![(-0.5, 0.5), (-0.25, 0.25)];

        cache.mark_pending_at("clip", &identity_a, 0.0);
        cache.mark_waveform_ready("clip", &identity_a, peaks.clone());
        assert_eq!(
            cache.cached_waveform("clip", &identity_a),
            Some(peaks.clone())
        );

        cache.mark_pending_at("clip", &identity_a, 0.75);
        assert_eq!(cache.cached_waveform("clip", &identity_a), Some(peaks));

        cache.mark_pending_at("clip", &identity_b, 0.0);
        assert!(cache.cached_waveform("clip", &identity_b).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_request_generates_waveform_and_serves_cached_frame() {
        set_platform();
        let tools = discover_media_tools().expect("FFmpeg is required for preview cache test");
        let dir = std::env::temp_dir().join(format!(
            "loom-video-preview-cache-hit-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create preview fixture");
        let app = VideoApp::new().expect("create VideoApp");
        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        project.tracks[0].clips[0].duration = 4.0;
        project.tracks[0].clips[0].out_point = 4.0;
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(project)),
            save_path: Mutex::new(None),
            dialogs: Arc::new(ScriptedFileDialogs::default()),
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: Some(tools),
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        let clip = lock(&state.session).project.tracks[0].clips[0].clone();

        request_preview(state.clone(), app.as_weak(), 0.0);
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let ready = lock(&state.preview_cache)
                .entries
                .iter()
                .find(|entry| entry.clip_id == clip.id)
                .map(|entry| {
                    entry.thumbnail == CacheState::Ready
                        && entry.waveform == CacheState::Ready
                        && entry
                            .waveform_peaks
                            .as_ref()
                            .is_some_and(|peaks| !peaks.is_empty())
                })
                .unwrap_or(false);
            if ready || Instant::now() >= deadline {
                assert!(ready, "preview cache did not become ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let cached = lock(&state.preview_cache)
            .cached_frame(&clip.id, &source_identity(&source), 0.0)
            .expect("generated preview frame should be cached");
        assert!(!cached.pixels.is_empty());

        let identity = source_identity(&source);
        let first_peaks = lock(&state.preview_cache)
            .waveform_for(&clip.id, &identity)
            .map(|peaks| peaks.to_vec())
            .expect("generated waveform peaks should be available");
        assert!(!first_peaks.is_empty());

        request_preview_internal(state.clone(), app.as_weak(), 0.75, true);
        let waveform_hit_deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let ready = lock(&state.preview_cache)
                .entries
                .iter()
                .find(|entry| entry.clip_id == clip.id && entry.source_identity == identity)
                .map(|entry| {
                    entry.thumbnail == CacheState::Ready
                        && (entry.frame_time - 0.75).abs() <= 1e-3
                        && entry.waveform == CacheState::Ready
                })
                .unwrap_or(false);
            if ready || Instant::now() >= waveform_hit_deadline {
                assert!(ready, "seeked preview did not become ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(
            lock(&state.preview_cache)
                .waveform_for(&clip.id, &identity)
                .expect("waveform should survive thumbnail seek"),
            first_peaks.as_slice()
        );
        assert_eq!(state.waveform_cache_hits.load(Ordering::Acquire), 1);

        // A second request at the same time serves both projections directly
        // from cache; the status text is the user-visible waveform projection.
        request_preview_internal(state.clone(), app.as_weak(), 0.75, true);
        assert_eq!(state.preview_cache_hits.load(Ordering::Acquire), 1);
        assert_eq!(state.waveform_cache_hits.load(Ordering::Acquire), 2);
        assert!(!state.preview_in_flight.load(Ordering::Acquire));
        let served = lock(&state.preview)
            .as_ref()
            .cloned()
            .expect("cached seek frame should be served");
        assert_ne!(served.pixels, cached.pixels);
        assert!(app
            .get_status_left()
            .to_string()
            .contains(&format!("waveform {} bins", first_peaks.len())));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
