//! Loom Studio local-first DAW application.

mod audio_io;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use audio_io::AudioIo;
use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_studio_core::{
    bounce_mix_with_cancel, decode_wav, load_studio_bundle, save_studio_bundle, synthesize_notes,
    AudioAssetStore, AudioBuffer, AudioRegion, MidiNote, StudioCancellation, StudioEditError,
    StudioJobState, StudioProject, StudioSession, StudioTrack, TrackKind, WorkspaceMode,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, Timer, TimerMode, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);

loom_production::define_snapshot_recovery!(STUDIO_RECOVERY, "org.loom.studio", "loom.studio/1");

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

fn empty_session() -> (StudioSession, AudioAssetStore) {
    (
        StudioSession::new(StudioProject::new("untitled-studio", "Untitled Session")),
        AudioAssetStore::default(),
    )
}

fn compact_layout_for_width(app: &StudioApp, width: u32) -> bool {
    let policy = ResponsivePolicy::get(app);
    (width as f32) < policy.get_priority_1_icon_only_below()
}

fn configure_responsive_layout(app: &StudioApp, width: u32) {
    app.set_compact_layout(compact_layout_for_width(app, width));
}

fn configure_direction(app: &StudioApp, rtl: bool) {
    app.set_rtl(rtl);
}

fn wire_responsive_layout(app: &StudioApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_layout(&app, width.max(0.0) as u32);
        }
    });
}

fn sample_session() -> Result<(StudioSession, AudioAssetStore), String> {
    let mut project = StudioProject::new("studio-sample", "Loom Studio Session");
    project.bpm = 118.0;
    project.tracks.clear();

    let mut vocal = StudioTrack::new("track-vocal", "Vocal Guide", TrackKind::Audio);
    vocal.add_region(AudioRegion {
        id: "region-vocal".into(),
        name: "Vocal Guide.wav".into(),
        start_sample: 0,
        length_samples: 48_000 * 10,
    });
    let mut guitar = StudioTrack::new("track-guitar", "Acoustic Guitar", TrackKind::Audio);
    guitar.volume_db = -5.0;
    guitar.pan = -0.18;
    guitar.add_region(AudioRegion {
        id: "region-guitar".into(),
        name: "Acoustic Guitar.wav".into(),
        start_sample: 48_000,
        length_samples: 48_000 * 8,
    });
    let mut synth = StudioTrack::new("track-synth", "Copper Keys", TrackKind::Midi);
    synth.volume_db = -8.0;
    synth.pan = 0.16;
    synth.add_region(AudioRegion {
        id: "region-synth".into(),
        name: "Copper Keys.wav".into(),
        start_sample: 48_000 * 2,
        length_samples: 48_000 * 8,
    });
    project.tracks = vec![vocal, guitar, synth];

    let mut assets = AudioAssetStore::default();
    assets.insert(
        "Vocal Guide.wav",
        AudioBuffer::sine(48_000, 2, 220.0, 10.0, 0.11)?,
    )?;
    assets.insert(
        "Acoustic Guitar.wav",
        AudioBuffer::sine(48_000, 2, 329.63, 8.0, 0.09)?,
    )?;
    assets.insert(
        "Copper Keys.wav",
        synthesize_notes(
            48_000,
            &[
                MidiNote {
                    key: 60,
                    start_secs: 0.0,
                    duration_secs: 1.8,
                    velocity: 0.45,
                },
                MidiNote {
                    key: 64,
                    start_secs: 2.0,
                    duration_secs: 1.8,
                    velocity: 0.42,
                },
                MidiNote {
                    key: 67,
                    start_secs: 4.0,
                    duration_secs: 1.8,
                    velocity: 0.40,
                },
                MidiNote {
                    key: 72,
                    start_secs: 6.0,
                    duration_secs: 1.8,
                    velocity: 0.38,
                },
            ],
            0.2,
        )?,
    )?;
    Ok((StudioSession::new(project), assets))
}

fn initial_session(
    args: &Args,
) -> Result<((StudioSession, AudioAssetStore), Option<PathBuf>), String> {
    match args.open.as_deref() {
        Some(path) => {
            let p = PathBuf::from(path);
            let bytes = std::fs::read(&p)
                .map_err(|error| format!("failed to read Studio project '{path}': {error}"))?;
            let (project, assets) = load_studio_bundle(&bytes)
                .map_err(|error| format!("failed to load Studio project '{path}': {error}"))?;
            Ok(((StudioSession::new(project), assets), Some(p)))
        }
        None => Ok((sample_session()?, None)),
    }
}

struct GuiState {
    session: RefCell<StudioSession>,
    assets: RefCell<AudioAssetStore>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    audio: RefCell<Option<AudioIo>>,
    midi_status: RefCell<String>,
    metronome_enabled: Cell<bool>,
    audio_error: RefCell<Option<String>>,
    gesture: RefCell<Option<RegionGesture>>,
    job: RefCell<JobUiState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GestureKind {
    Move,
    TrimStart,
    TrimEnd,
}

#[derive(Debug, Clone)]
struct RegionGesture {
    baseline: StudioProject,
    track_index: usize,
    region_id: String,
    kind: GestureKind,
}

#[derive(Debug)]
struct JobUiState {
    state: StudioJobState,
    cancellation: Option<StudioCancellation>,
    receiver: Option<Receiver<StudioJobEvent>>,
}

impl Default for JobUiState {
    fn default() -> Self {
        Self {
            state: StudioJobState::Idle,
            cancellation: None,
            receiver: None,
        }
    }
}

/// Events emitted by the bounded import/bounce workers.  The receiver is
/// drained only on the Slint thread, so all UI and document mutation remains
/// deterministic and single-threaded.
#[derive(Debug)]
enum StudioJobEvent {
    Progress(f32),
    ImportFinished {
        path: PathBuf,
        name: String,
        result: Result<AudioBuffer, String>,
    },
    BounceFinished {
        destination: PathBuf,
        result: Result<(), String>,
    },
}

fn studio_filter() -> FileFilter {
    FileFilter {
        name: "Loom Studio Project (*.loomstudio)".into(),
        extensions: vec!["loomstudio".into()],
    }
}

fn audio_filter() -> FileFilter {
    FileFilter {
        name: "Audio Files (*.wav, *.aif, *.flac, *.mp3)".into(),
        extensions: vec![
            "wav".into(),
            "aif".into(),
            "aiff".into(),
            "flac".into(),
            "mp3".into(),
        ],
    }
}

fn open_studio_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Studio Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![studio_filter()],
    }
}

fn save_studio_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Save Studio Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled.loomstudio".into())),
        filters: vec![studio_filter()],
    }
}

fn open_audio_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Import Audio".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![audio_filter()],
    }
}

fn export_audio_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Audio Mix".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: Some("Mixdown.wav".into()),
        filters: vec![FileFilter {
            name: "WAV Audio (*.wav)".into(),
            extensions: vec!["wav".into()],
        }],
    }
}

fn refresh(app: &StudioApp, state: &GuiState) {
    let session = state.session.borrow();
    let project = &session.project;
    app.set_song_title(project.name.as_str().into());
    app.set_bpm_val(project.bpm);
    let (mode, mode_index) = match project.mode {
        WorkspaceMode::Quick => ("Quick", 0),
        WorkspaceMode::Pro => ("Pro", 1),
    };
    app.set_workspace_mode(mode.into());
    app.set_workspace_mode_index(mode_index);
    app.set_track_labels(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| SharedString::from(track.name.as_str()))
            .collect::<Vec<_>>(),
    )));
    app.set_track_kinds(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| match track.kind {
                TrackKind::Audio => SharedString::from("Audio"),
                TrackKind::Midi => SharedString::from("MIDI"),
                TrackKind::Drummer => SharedString::from("Drummer"),
                TrackKind::Bus => SharedString::from("Bus"),
            })
            .collect::<Vec<_>>(),
    )));
    app.set_track_region_counts(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.regions.len() as i32)
            .collect::<Vec<_>>(),
    )));
    let mut region_ids = Vec::new();
    let mut region_names = Vec::new();
    let mut region_track_indices = Vec::new();
    let mut region_starts = Vec::new();
    let mut region_durations = Vec::new();
    let mut region_selected = Vec::new();
    for (track_index, track) in project.tracks.iter().enumerate() {
        for region in &track.regions {
            region_ids.push(SharedString::from(region.id.as_str()));
            region_names.push(SharedString::from(region.name.as_str()));
            region_track_indices.push(track_index as i32);
            region_starts.push(region.start_sample as f32 / project.sample_rate.max(1) as f32);
            region_durations.push(region.length_samples as f32 / project.sample_rate.max(1) as f32);
            region_selected.push(
                session.selection.track_index == Some(track_index)
                    && session.selection.region_id.as_deref() == Some(region.id.as_str()),
            );
        }
    }
    app.set_region_ids(ModelRc::new(VecModel::from(region_ids)));
    app.set_region_names(ModelRc::new(VecModel::from(region_names)));
    app.set_region_track_indices(ModelRc::new(VecModel::from(region_track_indices)));
    app.set_region_start_seconds(ModelRc::new(VecModel::from(region_starts)));
    app.set_region_durations(ModelRc::new(VecModel::from(region_durations)));
    app.set_region_selected_flags(ModelRc::new(VecModel::from(region_selected)));
    app.set_track_mutes(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.mute)
            .collect::<Vec<_>>(),
    )));
    app.set_track_solos(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.solo)
            .collect::<Vec<_>>(),
    )));
    app.set_track_arms(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.record_arm)
            .collect::<Vec<_>>(),
    )));
    app.set_track_volumes(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.volume_db)
            .collect::<Vec<_>>(),
    )));
    app.set_track_pans(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.pan)
            .collect::<Vec<_>>(),
    )));
    app.set_active_track_index(project.active_track_index as i32);
    app.set_duration_seconds(project.duration_samples() as f32 / project.sample_rate.max(1) as f32);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    app.set_metronome_on(state.metronome_enabled.get());

    let (selection_label, selected_region_id) = match session.selected_region() {
        Some((track_index, region)) => (
            format!(
                "Selected {} · {} · {:.2}s · {:.2}s",
                region.name,
                project
                    .tracks
                    .get(track_index)
                    .map(|track| track.name.as_str())
                    .unwrap_or("Track"),
                region.start_sample as f32 / project.sample_rate.max(1) as f32,
                region.length_samples as f32 / project.sample_rate.max(1) as f32
            ),
            region.id.clone(),
        ),
        None => match session.selection.track_index {
            Some(index) => (
                format!(
                    "Selected track {}",
                    project
                        .tracks
                        .get(index)
                        .map(|track| track.name.as_str())
                        .unwrap_or("Unknown")
                ),
                String::new(),
            ),
            None => ("No region selected".into(), String::new()),
        },
    };
    app.set_selection_label(selection_label.into());
    app.set_selected_region_id(selected_region_id.into());

    let audio = state.audio.borrow();
    if let Some(audio) = audio.as_ref() {
        app.set_audio_available(true);
        app.set_input_available(audio.has_input());
        app.set_output_device(audio.output_device_name().into());
        app.set_input_device(
            audio
                .input_device_name()
                .unwrap_or("No input device")
                .into(),
        );
        app.set_is_playing(audio.is_playing());
        app.set_is_recording(audio.is_recording());
        app.set_status_right("Local CPAL audio".into());
    } else {
        app.set_audio_available(false);
        app.set_input_available(false);
        app.set_output_device(
            state
                .audio_error
                .borrow()
                .as_deref()
                .map(|error| format!("Unavailable · {error}"))
                .unwrap_or_else(|| "No audio output device".into())
                .into(),
        );
        app.set_input_device("No audio input device".into());
        app.set_is_playing(false);
        app.set_is_recording(false);
        app.set_status_right(
            state
                .audio_error
                .borrow()
                .as_deref()
                .map(|error| format!("Audio unavailable · {error}"))
                .unwrap_or_else(|| "Audio unavailable".into())
                .into(),
        );
    }
    app.set_midi_status(state.midi_status.borrow().as_str().into());
    let path_label = state
        .save_path
        .borrow()
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into());
    app.set_status_left(
        format!(
            "{path_label} · {} tracks · {} audio assets",
            project.tracks.len(),
            state.assets.borrow().names().count()
        )
        .into(),
    );
    let job = state.job.borrow();
    let (job_state, job_progress, job_cancellable) = match &job.state {
        StudioJobState::Idle => ("Idle".to_string(), 0.0, false),
        StudioJobState::Running { kind, progress } => (kind.clone(), *progress, true),
        StudioJobState::Completed { kind } => (format!("{kind} complete"), 1.0, false),
        StudioJobState::Cancelled { kind } => (format!("{kind} cancelled"), 0.0, false),
        StudioJobState::Failed { kind, message } => {
            (format!("{kind} failed: {message}"), 0.0, false)
        }
    };
    app.set_job_state(job_state.into());
    app.set_job_progress(job_progress);
    app.set_job_cancellable(job_cancellable);
    if let Ok(bytes) = save_studio_bundle(project, &state.assets.borrow()) {
        let _ = record_snapshot_recovery("studio state", bytes);
    }
}

fn format_edit_error(operation: &str, error: &StudioEditError) -> String {
    format!("{operation} failed: {error}")
}

/// Applies an indexed mixer edit and makes that channel the active selection
/// only after the edit succeeds. Failed and no-op edits therefore leave both
/// the document and the prior selection untouched.
fn apply_mixer_edit<F>(
    session: &mut StudioSession,
    track_index: i32,
    edit: F,
) -> Result<(), StudioEditError>
where
    F: FnOnce(&mut StudioSession, i32) -> Result<(), StudioEditError>,
{
    edit(session, track_index)?;
    session.select_track(track_index)
}

fn delta_to_samples(delta_seconds: f32, sample_rate: u32) -> Result<i64, StudioEditError> {
    if !delta_seconds.is_finite() {
        return Err(StudioEditError::InvalidTiming(
            "timeline delta must be finite".into(),
        ));
    }
    let samples = f64::from(delta_seconds) * f64::from(sample_rate);
    if !samples.is_finite() || samples < i64::MIN as f64 || samples > i64::MAX as f64 {
        return Err(StudioEditError::InvalidTiming(
            "timeline delta is outside the supported range".into(),
        ));
    }
    Ok(samples.round() as i64)
}

fn region_target_sample(
    session: &StudioSession,
    track_index: i32,
    region_id: &str,
    kind: GestureKind,
    delta_seconds: f32,
) -> Result<i64, StudioEditError> {
    let track_index =
        usize::try_from(track_index).map_err(|_| StudioEditError::InvalidTrackIndex)?;
    let track = session
        .project
        .tracks
        .get(track_index)
        .ok_or(StudioEditError::InvalidTrackIndex)?;
    let region = track
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .ok_or(StudioEditError::RegionNotFound)?;
    let base = match kind {
        GestureKind::Move | GestureKind::TrimStart => region.start_sample,
        GestureKind::TrimEnd => region.end_sample(),
    };
    let base = i64::try_from(base).map_err(|_| {
        StudioEditError::InvalidTiming("region position exceeds the supported range".into())
    })?;
    base.checked_add(delta_to_samples(
        delta_seconds,
        session.project.sample_rate,
    )?)
    .ok_or_else(|| StudioEditError::InvalidTiming("region position overflowed".into()))
}

fn mutate_region_at(
    project: &mut StudioProject,
    track_index: i32,
    region_id: &str,
    kind: GestureKind,
    target_sample: i64,
) -> Result<(), StudioEditError> {
    let target_sample = u64::try_from(target_sample).map_err(|_| {
        if matches!(kind, GestureKind::Move | GestureKind::TrimStart) {
            StudioEditError::NegativePosition
        } else {
            StudioEditError::InvalidTiming("region end must not be negative".into())
        }
    })?;
    let track = project
        .tracks
        .get_mut(usize::try_from(track_index).map_err(|_| StudioEditError::InvalidTrackIndex)?)
        .ok_or(StudioEditError::InvalidTrackIndex)?;
    let region = track
        .regions
        .iter_mut()
        .find(|region| region.id == region_id)
        .ok_or(StudioEditError::RegionNotFound)?;
    match kind {
        GestureKind::Move => {
            if region.start_sample == target_sample {
                return Err(StudioEditError::NoOp);
            }
            region.start_sample = target_sample;
            track.regions.sort_by_key(|region| region.start_sample);
        }
        GestureKind::TrimStart => {
            let end = region.end_sample();
            if target_sample >= end {
                return Err(StudioEditError::InvalidTiming(
                    "trim start must be before region end".into(),
                ));
            }
            if target_sample == region.start_sample {
                return Err(StudioEditError::NoOp);
            }
            region.start_sample = target_sample;
            region.length_samples = end - target_sample;
        }
        GestureKind::TrimEnd => {
            if target_sample <= region.start_sample {
                return Err(StudioEditError::InvalidTiming(
                    "trim end must be after region start".into(),
                ));
            }
            if target_sample == region.end_sample() {
                return Err(StudioEditError::NoOp);
            }
            region.length_samples = target_sample - region.start_sample;
        }
    }
    Ok(())
}

fn parse_gesture_kind(kind: &str) -> Option<GestureKind> {
    match kind {
        "move" => Some(GestureKind::Move),
        "trim-start" => Some(GestureKind::TrimStart),
        "trim-end" => Some(GestureKind::TrimEnd),
        _ => None,
    }
}

fn apply_region_delta(
    session: &mut StudioSession,
    track_index: i32,
    region_id: &str,
    kind: GestureKind,
    delta_seconds: f32,
    gesture_update: bool,
) -> Result<(), StudioEditError> {
    let target_sample = region_target_sample(session, track_index, region_id, kind, delta_seconds)?;
    if gesture_update {
        session.apply_edit_without_history(|project| {
            mutate_region_at(project, track_index, region_id, kind, target_sample)
        })
    } else {
        session.apply_edit(|project| {
            mutate_region_at(project, track_index, region_id, kind, target_sample)
        })
    }
}

/// Drains worker events and applies their terminal result on the UI thread.
/// Returns whether any event was consumed, allowing headless journeys and the
/// live timer to share exactly the same job lifecycle.
fn poll_job(app: &StudioApp, state: &GuiState) -> bool {
    let events = {
        let job = state.job.borrow();
        job.receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
    };
    let Some(events) = events else {
        return false;
    };
    if events.is_empty() {
        return false;
    }

    let mut terminal = None;
    {
        let mut job = state.job.borrow_mut();
        for event in events {
            match event {
                StudioJobEvent::Progress(progress) => {
                    if let StudioJobState::Running {
                        progress: current, ..
                    } = &mut job.state
                    {
                        *current = progress.clamp(0.0, 1.0);
                    }
                }
                event @ (StudioJobEvent::ImportFinished { .. }
                | StudioJobEvent::BounceFinished { .. }) => {
                    terminal = Some(event);
                }
            }
        }
        if terminal.is_some() {
            job.receiver = None;
            job.cancellation = None;
        }
    }

    if let Some(event) = terminal {
        let kind = match &state.job.borrow().state {
            StudioJobState::Running { kind, .. } => kind.clone(),
            _ => "Studio job".into(),
        };
        match event {
            StudioJobEvent::ImportFinished { path, name, result } => match result {
                Ok(buffer) => {
                    let frames = buffer.frames();
                    if frames == 0 {
                        state.job.borrow_mut().state = StudioJobState::Failed {
                            kind: kind.clone(),
                            message: "imported audio is empty".into(),
                        };
                        refresh(app, state);
                        app.set_status_left("Import failed: imported audio is empty".into());
                        return true;
                    }
                    let previous_asset = state.assets.borrow().get(&name).cloned();
                    let insert_result = state.assets.borrow_mut().insert(name.clone(), buffer);
                    let edit_result = insert_result.and_then(|()| {
                        let mut session = state.session.borrow_mut();
                        let active = i32::try_from(session.project.active_track_index)
                            .map_err(|_| "active track index is too large".to_string())?;
                        let region_id = next_region_id(&session.project, "import");
                        let selected_region_id = region_id.clone();
                        session
                            .apply_edit(|project| {
                                let track = project
                                    .tracks
                                    .get_mut(
                                        usize::try_from(active)
                                            .map_err(|_| StudioEditError::InvalidTrackIndex)?,
                                    )
                                    .ok_or(StudioEditError::InvalidTrackIndex)?;
                                track.add_region(AudioRegion {
                                    id: region_id,
                                    name: name.clone(),
                                    start_sample: 0,
                                    length_samples: frames,
                                });
                                Ok(())
                            })
                            .map_err(|error| error.to_string())
                            .map(|()| {
                                session.selection = loom_studio_core::TimelineSelection::region(
                                    usize::try_from(active).unwrap_or_default(),
                                    selected_region_id,
                                );
                            })
                    });
                    match edit_result {
                        Ok(()) => {
                            state.job.borrow_mut().state =
                                StudioJobState::Completed { kind: kind.clone() };
                            refresh(app, state);
                            app.set_status_left(format!("Imported {}", path.display()).into());
                        }
                        Err(error) => {
                            if let Some(previous_asset) = previous_asset {
                                let _ = state
                                    .assets
                                    .borrow_mut()
                                    .insert(name.clone(), previous_asset);
                            } else {
                                let _ = state.assets.borrow_mut().remove(&name);
                            }
                            state.job.borrow_mut().state = StudioJobState::Failed {
                                kind: kind.clone(),
                                message: error.clone(),
                            };
                            refresh(app, state);
                            app.set_status_left(format!("Import failed: {error}").into());
                        }
                    }
                }
                Err(error) if error.contains("cancel") => {
                    state.job.borrow_mut().state = StudioJobState::Cancelled { kind };
                    refresh(app, state);
                    app.set_status_left("Import cancelled; no region was added".into());
                }
                Err(error) => {
                    state.job.borrow_mut().state = StudioJobState::Failed {
                        kind,
                        message: error.clone(),
                    };
                    refresh(app, state);
                    app.set_status_left(format!("Import failed: {error}").into());
                }
            },
            StudioJobEvent::BounceFinished {
                destination,
                result,
            } => match result {
                Ok(()) => {
                    state.job.borrow_mut().state = StudioJobState::Completed { kind: kind.clone() };
                    refresh(app, state);
                    app.set_status_left(
                        format!("Exported mix to {}", destination.display()).into(),
                    );
                }
                Err(error) if error.contains("cancel") => {
                    state.job.borrow_mut().state = StudioJobState::Cancelled { kind };
                    refresh(app, state);
                    app.set_status_left(
                        "Bounce cancelled; existing destination was preserved".into(),
                    );
                }
                Err(error) => {
                    state.job.borrow_mut().state = StudioJobState::Failed {
                        kind,
                        message: error.clone(),
                    };
                    refresh(app, state);
                    app.set_status_left(format!("Bounce failed: {error}").into());
                }
            },
            StudioJobEvent::Progress(_) => unreachable!("progress is consumed above"),
        }
    } else {
        refresh(app, state);
    }
    true
}

fn next_region_id(project: &StudioProject, prefix: &str) -> String {
    let mut index = 1_u64;
    loop {
        let candidate = format!("{prefix}-{index}");
        if !project
            .tracks
            .iter()
            .flat_map(|track| track.regions.iter())
            .any(|region| region.id == candidate)
        {
            return candidate;
        }
        index += 1;
    }
}

fn mix_current(state: &GuiState) -> Result<AudioBuffer, String> {
    let session = state.session.borrow();
    let assets = state.assets.borrow();
    let result = session.project.mix(&assets)?;
    if result.audio.frames() == 0 {
        return Err("the project contains no renderable audio regions".into());
    }
    Ok(result.audio)
}

fn apply_theme(app: &StudioApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = StudioApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let ((session, assets), save_path) = initial_session(args)?;
    let state = GuiState {
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(save_path),
        dialogs: Rc::new(NativeFileDialogs),
        audio: RefCell::new(None),
        // Keep the compact sidebar status readable at the reference width.
        // The full headless context is already communicated by the status
        // bar, while this pill should remain a concise device-state label.
        midi_status: RefCell::new("MIDI unavailable (headless)".into()),
        metronome_enabled: Cell::new(true),
        audio_error: RefCell::new(Some("headless harness".into())),
        gesture: RefCell::new(None),
        job: RefCell::new(JobUiState::default()),
    };
    refresh(&app, &state);
    if args.palette {
        app.set_palette_query(SharedString::from("so"));
        rebuild_palette(&app, "so");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    app.set_status_left("Headless deterministic Studio workspace".into());
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn capture_studio_journey_step(
    app: &StudioApp,
    state: &GuiState,
    out_dir: &Path,
    name: &str,
    steps: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    let image = snapshot_component(
        app,
        app.window().size().width as f32,
        app.window().size().height as f32,
        1.0,
    )
    .map_err(|error| format!("capture {name}: {error}"))?;
    let file_name = format!("studio-journey-{name}.png");
    loom_test_support::png::save_png(&out_dir.join(&file_name), &image)
        .map_err(|error| format!("save {file_name}: {error}"))?;
    let session = state.session.borrow();
    let job = state.job.borrow();
    steps.push(serde_json::json!({
        "name": name,
        "screenshot": file_name,
        "status_left": app.get_status_left().to_string(),
        "selection": app.get_selection_label().to_string(),
        "region_count": session.project.total_regions(),
        "session_digest": session.session_digest(),
        "can_undo": session.can_undo(),
        "can_redo": session.can_redo(),
        "job_state": format!("{:?}", job.state),
    }));
    Ok(())
}

fn wait_for_job(app: &StudioApp, state: &GuiState, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let _ = poll_job(app, state);
        if !matches!(state.job.borrow().state, StudioJobState::Running { .. }) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Studio job did not finish before the journey timeout".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Run an executable Studio workflow and retain state assertions, screenshots,
/// and a JSON transcript under the requested evidence directory.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|error| error.to_string())?;
    let app = StudioApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let ((session, assets), save_path) = initial_session(args)?;
    let project_path = out_dir.join("studio-journey.loomstudio");
    let source_path = out_dir.join("studio-journey-source.wav");
    // A 30-second fixture keeps the bounce worker active long enough for the
    // cancellation callback to race it deterministically in headless runs.
    let source = AudioBuffer::sine(48_000, 1, 330.0, 30.0, 0.2)
        .map_err(|error| format!("create journey source: {error}"))?;
    let source_bytes = source
        .to_wav_pcm16()
        .map_err(|error| format!("encode journey source: {error}"))?;
    std::fs::write(&source_path, source_bytes)
        .map_err(|error| format!("write journey source: {error}"))?;
    let dialogs = loom_desktop::ScriptedFileDialogs::new(
        vec![Some(project_path.clone())],
        vec![Some(project_path.clone())],
    );
    let state = Rc::new(GuiState {
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(save_path),
        dialogs: Rc::new(dialogs),
        audio: RefCell::new(None),
        midi_status: RefCell::new("MIDI unavailable (headless)".into()),
        metronome_enabled: Cell::new(true),
        audio_error: RefCell::new(Some("headless harness".into())),
        gesture: RefCell::new(None),
        job: RefCell::new(JobUiState::default()),
    });
    wire_application(&app, state.clone());
    wire_palette(&app);
    rebuild_palette(&app, "");
    refresh(&app, &state);
    let mut steps = Vec::new();
    capture_studio_journey_step(&app, &state, &out_dir, "initial", &mut steps)?;

    // Import through the same callback used by the file panel/drop target.
    app.invoke_import_audio(source_path.to_string_lossy().into_owned().into());
    wait_for_job(&app, &state, Duration::from_secs(5))?;
    if !matches!(state.job.borrow().state, StudioJobState::Completed { .. }) {
        return Err(format!(
            "import journey step failed: {:?}",
            state.job.borrow().state
        ));
    }
    capture_studio_journey_step(&app, &state, &out_dir, "imported", &mut steps)?;

    // Select, coalesce a pointer gesture, and prove undo/redo restore the
    // exact arrangement state before the subsequent trims/split/delete.
    app.invoke_select_region(0, "region-vocal".into());
    let before_move = state.session.borrow().session_digest();
    app.invoke_begin_region_gesture(0, "region-vocal".into(), "move".into());
    app.invoke_move_region(0, "region-vocal".into(), 0.10);
    app.invoke_move_region(0, "region-vocal".into(), 0.10);
    app.invoke_end_region_gesture();
    if !state.session.borrow().can_undo() {
        return Err("move gesture did not create an undo entry".into());
    }
    app.invoke_undo();
    if state.session.borrow().session_digest() != before_move {
        return Err("undo did not restore the pre-gesture arrangement".into());
    }
    app.invoke_redo();
    app.invoke_trim_region_start(0, "region-vocal".into(), 0.05);
    app.invoke_trim_region_end(0, "region-vocal".into(), -0.05);
    app.invoke_split_region(0, "region-vocal".into());
    app.invoke_delete_region(0, "region-vocal-b".into());
    capture_studio_journey_step(&app, &state, &out_dir, "edited", &mut steps)?;

    // Save and reopen through the scripted native dialog harness.
    app.invoke_save_song();
    if !project_path.is_file() {
        return Err("save journey step did not create the project package".into());
    }
    let saved_digest = state.session.borrow().session_digest();
    app.invoke_open_song();
    if state.session.borrow().session_digest() != saved_digest {
        return Err("reopen journey step changed the durable project".into());
    }
    capture_studio_journey_step(&app, &state, &out_dir, "reopened", &mut steps)?;

    // Headless mode intentionally has no CPAL output; the failure is visible
    // and recoverable rather than pretending to play audio.
    app.invoke_play_pause();
    if !app.get_status_left().contains("No audio output device") {
        return Err("device failure did not reach the visible status state".into());
    }
    capture_studio_journey_step(&app, &state, &out_dir, "device-failure", &mut steps)?;

    // Bounce to a pre-existing destination and cancel cooperatively.  The
    // atomic worker must preserve the sentinel and leave no temporary file.
    let bounce_path = out_dir.join("studio-journey-cancelled.wav");
    std::fs::write(&bounce_path, b"existing mix").map_err(|error| error.to_string())?;
    app.invoke_export_mix(bounce_path.to_string_lossy().into_owned().into());
    app.invoke_cancel_job();
    wait_for_job(&app, &state, Duration::from_secs(10))?;
    if !matches!(state.job.borrow().state, StudioJobState::Cancelled { .. }) {
        return Err(format!(
            "bounce cancellation was not observed: {:?}",
            state.job.borrow().state
        ));
    }
    if std::fs::read(&bounce_path).map_err(|error| error.to_string())? != b"existing mix" {
        return Err("cancelled bounce replaced the existing destination".into());
    }
    let temporary_count = std::fs::read_dir(&out_dir)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".loom-studio-")
        })
        .count();
    if temporary_count != 0 {
        return Err("cancelled bounce left a temporary output".into());
    }
    capture_studio_journey_step(&app, &state, &out_dir, "bounce-cancelled", &mut steps)?;

    let palette = record_keyboard_palette_journey(&app, "studio-palette", &out_dir, "so")
        .map_err(|error| format!("palette journey failed: {error}"))?;
    if !palette.passed {
        return Err("keyboard palette journey invariants failed".into());
    }
    let transcript = serde_json::json!({
        "app": "studio",
        "workflow": "import -> select/edit -> undo/redo -> save/reopen -> device failure -> bounce/cancel",
        "passed": true,
        "steps": steps,
        "palette_report": "studio-palette.json",
        "project": project_path.file_name().and_then(|name| name.to_str()),
        "source": source_path.file_name().and_then(|name| name.to_str()),
        "cancelled_output": bounce_path.file_name().and_then(|name| name.to_str()),
    });
    std::fs::write(
        out_dir.join("studio-workflow.json"),
        serde_json::to_vec_pretty(&transcript).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!("studio journey: PASS ({})", out_dir.display());
    Ok(())
}

impl PaletteProbe for StudioApp {
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

fn wire_application(app: &StudioApp, state: Rc<GuiState>) {
    macro_rules! edit_project {
        ($callback:ident, $body:expr) => {{
            let state = state.clone();
            let weak = app.as_weak();
            app.$callback(move || {
                if let Some(app) = weak.upgrade() {
                    let mut session = state.session.borrow_mut();
                    let result = session.apply_edit(|project| {
                        ($body)(project);
                        Ok(())
                    });
                    drop(session);
                    match result {
                        Ok(()) => refresh(&app, &state),
                        Err(StudioEditError::NoOp) => {}
                        Err(error) => {
                            app.set_status_left(format_edit_error("Edit", &error).into());
                        }
                    }
                }
            });
        }};
    }

    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_new_song(move || {
            if let Some(app) = weak.upgrade() {
                let (session, assets) = empty_session();
                *state.session.borrow_mut() = session;
                *state.assets.borrow_mut() = assets;
                *state.save_path.borrow_mut() = None;
                if let Some(audio) = state.audio.borrow().as_ref() {
                    audio.stop();
                }
                refresh(&app, &state);
                app.set_status_left("Created a new untitled Studio project".into());
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_open_song(move || {
            if let Some(app) = weak.upgrade() {
                let current_path = state.save_path.borrow().clone();
                let request = open_studio_request(current_path.as_deref());
                match state.dialogs.open_file(&request) {
                    Ok(Some(path)) => match std::fs::read(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| load_studio_bundle(&bytes))
                    {
                        Ok((project, assets)) => {
                            *state.session.borrow_mut() = StudioSession::new(project);
                            *state.assets.borrow_mut() = assets;
                            *state.save_path.borrow_mut() = Some(path.clone());
                            refresh(&app, &state);
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
        let weak = app.as_weak();
        app.on_save_song(move || {
            if let Some(app) = weak.upgrade() {
                let current_path = state.save_path.borrow().clone();
                let path_to_save = match current_path {
                    Some(p) => Some(p),
                    None => {
                        let req = save_studio_request(None);
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
                    let session = state.session.borrow();
                    let assets = state.assets.borrow();
                    match save_studio_bundle(&session.project, &assets).and_then(|bytes| {
                        loom_storage::atomic_write(&path, &bytes)
                            .map_err(|error| error.to_string())
                            .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    }) {
                        Ok(()) => {
                            drop(session);
                            drop(assets);
                            *state.save_path.borrow_mut() = Some(path.clone());
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
        let weak = app.as_weak();
        app.on_save_as_song(move || {
            if let Some(app) = weak.upgrade() {
                let current_path = state.save_path.borrow().clone();
                let req = save_studio_request(current_path.as_deref());
                match state.dialogs.save_file(&req) {
                    Ok(Some(path)) => {
                        let session = state.session.borrow();
                        let assets = state.assets.borrow();
                        match save_studio_bundle(&session.project, &assets).and_then(|bytes| {
                            loom_storage::atomic_write(&path, &bytes)
                                .map_err(|error| error.to_string())
                                .and_then(|_| checkpoint_snapshot_recovery(bytes))
                        }) {
                            Ok(()) => {
                                drop(session);
                                drop(assets);
                                *state.save_path.borrow_mut() = Some(path.clone());
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
        let weak = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = weak.upgrade() {
                state.session.borrow_mut().undo();
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = weak.upgrade() {
                state.session.borrow_mut().redo();
                refresh(&app, &state);
            }
        });
    }

    edit_project!(on_add_track, |project: &mut StudioProject| {
        let count = project.tracks.len() + 1;
        project.add_track(StudioTrack::new(
            format!("track-{count}"),
            format!("Track {count}"),
            TrackKind::Audio,
        ));
    });

    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_select_track(move |index| {
            if let Some(app) = weak.upgrade() {
                let result = state.session.borrow_mut().select_track(index);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(error) => {
                        app.set_status_left(format_edit_error("Select track", &error).into())
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_select_region(move |track_index, region_id| {
            if let Some(app) = weak.upgrade() {
                let result = state
                    .session
                    .borrow_mut()
                    .select_region(track_index, region_id.as_str());
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(error) => {
                        app.set_status_left(format_edit_error("Select region", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_set_workspace_mode(move |index| {
            if let Some(app) = weak.upgrade() {
                let mut session = state.session.borrow_mut();
                let mode = match index {
                    1 => WorkspaceMode::Pro,
                    _ => WorkspaceMode::Quick,
                };
                let result = session.apply_edit(|project| {
                    if project.mode == mode {
                        return Err(StudioEditError::NoOp);
                    }
                    project.mode = mode;
                    Ok(())
                });
                drop(session);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => {
                        app.set_status_left(format_edit_error("Workspace mode", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_begin_region_gesture(move |track_index, region_id, kind| {
            if let Some(app) = weak.upgrade() {
                let Some(kind) = parse_gesture_kind(kind.as_str()) else {
                    app.set_status_left("Region gesture is not supported".into());
                    return;
                };
                let mut session = state.session.borrow_mut();
                let Ok(track) = usize::try_from(track_index) else {
                    app.set_status_left("Region gesture failed: track index is invalid".into());
                    return;
                };
                let Some(track_state) = session.project.tracks.get(track) else {
                    app.set_status_left("Region gesture failed: track index is invalid".into());
                    return;
                };
                if !track_state
                    .regions
                    .iter()
                    .any(|region| region.id == region_id.as_str())
                {
                    app.set_status_left("Region gesture failed: region was not found".into());
                    return;
                }
                session.project.active_track_index = track;
                session.selection =
                    loom_studio_core::TimelineSelection::region(track, region_id.to_string());
                let baseline = session.project.clone();
                *state.gesture.borrow_mut() = Some(RegionGesture {
                    baseline,
                    track_index: track,
                    region_id: region_id.to_string(),
                    kind,
                });
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_move_region(move |track_index, region_id, delta_seconds| {
            if let Some(app) = weak.upgrade() {
                let gesture = state.gesture.borrow().clone();
                let active_gesture = gesture.as_ref().filter(|gesture| {
                    gesture.track_index == usize::try_from(track_index).unwrap_or(usize::MAX)
                        && gesture.region_id == region_id.as_str()
                        && gesture.kind == GestureKind::Move
                });
                let mut session = state.session.borrow_mut();
                let result = if active_gesture.is_some() {
                    apply_region_delta(
                        &mut session,
                        track_index,
                        region_id.as_str(),
                        GestureKind::Move,
                        delta_seconds,
                        true,
                    )
                } else {
                    apply_region_delta(
                        &mut session,
                        track_index,
                        region_id.as_str(),
                        GestureKind::Move,
                        delta_seconds,
                        false,
                    )
                };
                drop(session);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => {
                        app.set_status_left(format_edit_error("Move region", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_trim_region_start(move |track_index, region_id, delta_seconds| {
            if let Some(app) = weak.upgrade() {
                let gesture = state.gesture.borrow().clone();
                let active_gesture = gesture.as_ref().filter(|gesture| {
                    gesture.track_index == usize::try_from(track_index).unwrap_or(usize::MAX)
                        && gesture.region_id == region_id.as_str()
                        && gesture.kind == GestureKind::TrimStart
                });
                let mut session = state.session.borrow_mut();
                let result = apply_region_delta(
                    &mut session,
                    track_index,
                    region_id.as_str(),
                    GestureKind::TrimStart,
                    delta_seconds,
                    active_gesture.is_some(),
                );
                drop(session);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => {
                        app.set_status_left(format_edit_error("Trim region start", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_trim_region_end(move |track_index, region_id, delta_seconds| {
            if let Some(app) = weak.upgrade() {
                let gesture = state.gesture.borrow().clone();
                let active_gesture = gesture.as_ref().filter(|gesture| {
                    gesture.track_index == usize::try_from(track_index).unwrap_or(usize::MAX)
                        && gesture.region_id == region_id.as_str()
                        && gesture.kind == GestureKind::TrimEnd
                });
                let mut session = state.session.borrow_mut();
                let result = apply_region_delta(
                    &mut session,
                    track_index,
                    region_id.as_str(),
                    GestureKind::TrimEnd,
                    delta_seconds,
                    active_gesture.is_some(),
                );
                drop(session);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => {
                        app.set_status_left(format_edit_error("Trim region end", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_end_region_gesture(move || {
            if let Some(app) = weak.upgrade() {
                let gesture = state.gesture.borrow_mut().take();
                let Some(gesture) = gesture else {
                    return;
                };
                let changed = state.session.borrow_mut().commit_gesture(gesture.baseline);
                if changed {
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_cancel_region_gesture(move || {
            if let Some(app) = weak.upgrade() {
                let gesture = state.gesture.borrow_mut().take();
                let Some(gesture) = gesture else {
                    return;
                };
                state
                    .session
                    .borrow_mut()
                    .rollback_gesture(gesture.baseline);
                refresh(&app, &state);
                app.set_status_left("Region edit cancelled".into());
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_split_region(move |track_index, region_id| {
            if let Some(app) = weak.upgrade() {
                let split = {
                    let session = state.session.borrow();
                    session
                        .project
                        .tracks
                        .get(usize::try_from(track_index).unwrap_or(usize::MAX))
                        .and_then(|track| {
                            track
                                .regions
                                .iter()
                                .find(|region| region.id == region_id.as_str())
                        })
                        .map(|region| {
                            region
                                .start_sample
                                .saturating_add(region.length_samples / 2)
                        })
                };
                let result = split
                    .ok_or(StudioEditError::RegionNotFound)
                    .and_then(|split| {
                        state.session.borrow_mut().split_region(
                            track_index,
                            region_id.as_str(),
                            i64::try_from(split).map_err(|_| {
                                StudioEditError::InvalidTiming("split position is too large".into())
                            })?,
                        )
                    });
                match result {
                    Ok((_left, _right)) => refresh(&app, &state),
                    Err(error) => {
                        app.set_status_left(format_edit_error("Split region", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_delete_region(move |track_index, region_id| {
            if let Some(app) = weak.upgrade() {
                let result = state
                    .session
                    .borrow_mut()
                    .delete_region(track_index, region_id.as_str());
                match result {
                    Ok(_) => refresh(&app, &state),
                    Err(error) => {
                        app.set_status_left(format_edit_error("Delete region", &error).into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = weak.upgrade() {
                let audio_ref = state.audio.borrow();
                let Some(audio) = audio_ref.as_ref() else {
                    app.set_status_left("No audio output device is available".into());
                    return;
                };
                if audio.is_playing() {
                    audio.pause();
                    app.set_status_left("Playback paused".into());
                } else {
                    match mix_current(&state).and_then(|mix| audio.load(&mix)) {
                        Ok(()) => {
                            audio.set_looping(app.get_is_looping());
                            audio.seek_seconds(app.get_playhead_seconds() as f64);
                            audio.play();
                            app.set_status_left("Playing the real local project mix".into());
                        }
                        Err(error) => {
                            app.set_status_left(format!("Playback failed: {error}").into())
                        }
                    }
                }
                app.set_is_playing(audio.is_playing());
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_stop(move || {
            if let Some(app) = weak.upgrade() {
                if let Some(audio) = state.audio.borrow().as_ref() {
                    audio.stop();
                }
                app.set_playhead_seconds(0.0);
                app.set_is_playing(false);
                app.set_status_left("Playback stopped".into());
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_record(move || {
            if let Some(app) = weak.upgrade() {
                let audio_ref = state.audio.borrow();
                let Some(audio) = audio_ref.as_ref() else {
                    app.set_status_left("No audio input device is available".into());
                    return;
                };
                if audio.is_recording() {
                    match audio.stop_recording() {
                        Ok((buffer, overruns)) => {
                            let target_rate = state.session.borrow().project.sample_rate;
                            match buffer.resample_linear(target_rate) {
                                Ok(buffer) => {
                                    let name = format!(
                                        "Recording-{}.wav",
                                        state.assets.borrow().names().count() + 1
                                    );
                                    let frames = buffer.frames();
                                    if let Err(error) =
                                        state.assets.borrow_mut().insert(name.clone(), buffer)
                                    {
                                        app.set_status_left(
                                            format!("Recording failed: {error}").into(),
                                        );
                                        return;
                                    }
                                    let start_sample = (app.get_playhead_seconds().max(0.0)
                                        * target_rate as f32)
                                        as u64;
                                    let mut session = state.session.borrow_mut();
                                    session.checkpoint();
                                    let active = session.project.active_track_index;
                                    if let Some(track) = session.project.tracks.get_mut(active) {
                                        track.add_region(AudioRegion {
                                            id: format!("recording-{}", track.regions.len() + 1),
                                            name,
                                            start_sample,
                                            length_samples: frames,
                                        });
                                    }
                                    drop(session);
                                    refresh(&app, &state);
                                    app.set_status_left(
                                        format!(
                                            "Recorded {:.2} seconds · {overruns} dropped samples",
                                            frames as f64 / target_rate as f64
                                        )
                                        .into(),
                                    );
                                }
                                Err(error) => app.set_status_left(
                                    format!("Recording conversion failed: {error}").into(),
                                ),
                            }
                        }
                        Err(error) => {
                            app.set_status_left(format!("Stop recording failed: {error}").into())
                        }
                    }
                } else {
                    match audio.start_recording() {
                        Ok(()) => {
                            app.set_is_recording(true);
                            app.set_status_left(
                                "Recording from the default local input device".into(),
                            );
                        }
                        Err(error) => app.set_status_left(format!("Record failed: {error}").into()),
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_loop(move || {
            if let Some(app) = weak.upgrade() {
                if let Some(audio) = state.audio.borrow().as_ref() {
                    audio.set_looping(app.get_is_looping());
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_metronome(move || {
            if let Some(app) = weak.upgrade() {
                let next = !state.metronome_enabled.get();
                state.metronome_enabled.set(next);
                app.set_metronome_on(next);
                app.set_status_left(if next {
                    "Metronome enabled".into()
                } else {
                    "Metronome disabled".into()
                });
            }
        });
    }
    {
        let state = state.clone();
        app.on_seek(move |seconds| {
            if let Some(audio) = state.audio.borrow().as_ref() {
                audio.seek_seconds(seconds as f64);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_bpm_changed(move |value| {
            if let Some(app) = weak.upgrade() {
                let mut session = state.session.borrow_mut();
                let bpm = value.clamp(40.0, 240.0);
                let result = session.apply_edit(|project| {
                    if (project.bpm - bpm).abs() <= f32::EPSILON {
                        return Err(StudioEditError::NoOp);
                    }
                    project.bpm = bpm;
                    Ok(())
                });
                drop(session);
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => app.set_status_left(format_edit_error("Tempo", &error).into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_mute(move |index| {
            if let Some(app) = weak.upgrade() {
                let result = {
                    let mut session = state.session.borrow_mut();
                    apply_mixer_edit(&mut session, index, |session, index| {
                        session.toggle_mute(index)
                    })
                };
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(error) => app.set_status_left(format_edit_error("Mute", &error).into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_solo(move |index| {
            if let Some(app) = weak.upgrade() {
                let result = {
                    let mut session = state.session.borrow_mut();
                    apply_mixer_edit(&mut session, index, |session, index| {
                        session.toggle_solo(index)
                    })
                };
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(error) => app.set_status_left(format_edit_error("Solo", &error).into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_rec_arm(move |index| {
            if let Some(app) = weak.upgrade() {
                let result = {
                    let mut session = state.session.borrow_mut();
                    apply_mixer_edit(&mut session, index, |session, index| {
                        session.toggle_record_arm(index)
                    })
                };
                if let Err(error) = result {
                    app.set_status_left(format!("Record arm failed: {error}").into());
                } else {
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_volume_changed(move |index, value| {
            if let Some(app) = weak.upgrade() {
                let result = {
                    let mut session = state.session.borrow_mut();
                    apply_mixer_edit(&mut session, index, |session, index| {
                        session.set_volume(index, value)
                    })
                };
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => app.set_status_left(format_edit_error("Volume", &error).into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_pan_changed(move |index, value| {
            if let Some(app) = weak.upgrade() {
                let result = {
                    let mut session = state.session.borrow_mut();
                    apply_mixer_edit(&mut session, index, |session, index| {
                        session.set_pan(index, value)
                    })
                };
                match result {
                    Ok(()) => refresh(&app, &state),
                    Err(StudioEditError::NoOp) => {}
                    Err(error) => app.set_status_left(format_edit_error("Pan", &error).into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_import_audio(move |path| {
            if let Some(app) = weak.upgrade() {
                let chosen_path = if path.trim().is_empty() {
                    let current_dir = state.save_path.borrow().clone();
                    let req = open_audio_request(current_dir.as_deref());
                    match state.dialogs.open_file(&req) {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            app.set_status_left("Audio import cancelled".into());
                            return;
                        }
                        Err(e) => {
                            app.set_status_left(format!("Audio import dialog failed: {e}").into());
                            return;
                        }
                    }
                } else {
                    PathBuf::from(path.trim())
                };

                if matches!(state.job.borrow().state, StudioJobState::Running { .. }) {
                    app.set_status_left("A Studio job is already running".into());
                    return;
                }
                let target_rate = state.session.borrow().project.sample_rate;
                let name = chosen_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Imported Audio.wav")
                    .to_string();
                let (sender, receiver) = mpsc::channel();
                let cancellation = StudioCancellation::default();
                let worker_cancellation = cancellation.clone();
                let worker_path = chosen_path.clone();
                let worker_name = name.clone();
                let worker = move || {
                    let _ = sender.send(StudioJobEvent::Progress(0.05));
                    let result = (|| {
                        if worker_cancellation.is_cancelled() {
                            return Err("import cancelled".to_string());
                        }
                        let bytes =
                            std::fs::read(&worker_path).map_err(|error| error.to_string())?;
                        let _ = sender.send(StudioJobEvent::Progress(0.35));
                        if worker_cancellation.is_cancelled() {
                            return Err("import cancelled".to_string());
                        }
                        let buffer = decode_wav(&bytes)?;
                        if !matches!(buffer.channels, 1 | 2) {
                            return Err("only mono and stereo WAV files are supported".into());
                        }
                        let buffer = buffer.resample_linear(target_rate)?;
                        if worker_cancellation.is_cancelled() {
                            return Err("import cancelled".to_string());
                        }
                        Ok(buffer)
                    })();
                    let _ = sender.send(StudioJobEvent::ImportFinished {
                        path: worker_path,
                        name: worker_name,
                        result,
                    });
                };
                {
                    let mut job = state.job.borrow_mut();
                    job.state = StudioJobState::Running {
                        kind: "Import WAV".into(),
                        progress: 0.0,
                    };
                    job.cancellation = Some(cancellation);
                    job.receiver = Some(receiver);
                }
                refresh(&app, &state);
                app.set_status_left(format!("Importing {}…", chosen_path.display()).into());
                thread::spawn(worker);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_export_mix(move |path| {
            if let Some(app) = weak.upgrade() {
                let chosen_path = if path.trim().is_empty() {
                    let current_dir = state.save_path.borrow().clone();
                    let req = export_audio_request(current_dir.as_deref());
                    match state.dialogs.save_file(&req) {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            app.set_status_left("Export cancelled".into());
                            return;
                        }
                        Err(e) => {
                            app.set_status_left(format!("Export dialog failed: {e}").into());
                            return;
                        }
                    }
                } else {
                    let mut output = PathBuf::from(path.trim());
                    if output.extension().is_none() {
                        output.set_extension("wav");
                    }
                    output
                };

                if matches!(state.job.borrow().state, StudioJobState::Running { .. }) {
                    app.set_status_left("A Studio job is already running".into());
                    return;
                }
                let project = state.session.borrow().project.clone();
                let assets = state.assets.borrow().clone();
                let (sender, receiver) = mpsc::channel();
                let cancellation = StudioCancellation::default();
                let worker_cancellation = cancellation.clone();
                let worker_destination = chosen_path.clone();
                let worker = move || {
                    // Give the UI a bounded cancellation window after the
                    // running state becomes visible (also makes headless
                    // cancellation journeys deterministic).
                    thread::sleep(Duration::from_millis(25));
                    let result = bounce_mix_with_cancel(
                        &project,
                        &assets,
                        &worker_destination,
                        &worker_cancellation,
                        |progress| {
                            let _ = sender.send(StudioJobEvent::Progress(progress));
                        },
                    )
                    .map(|_| ());
                    let _ = sender.send(StudioJobEvent::BounceFinished {
                        destination: worker_destination,
                        result,
                    });
                };
                {
                    let mut job = state.job.borrow_mut();
                    job.state = StudioJobState::Running {
                        kind: "Bounce mix".into(),
                        progress: 0.0,
                    };
                    job.cancellation = Some(cancellation);
                    job.receiver = Some(receiver);
                }
                refresh(&app, &state);
                app.set_status_left(format!("Bouncing {}…", chosen_path.display()).into());
                thread::spawn(worker);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_cancel_job(move || {
            if let Some(app) = weak.upgrade() {
                let cancellation = state.job.borrow().cancellation.clone();
                if let Some(cancellation) = cancellation {
                    cancellation.cancel();
                    app.set_status_left("Cancelling Studio job…".into());
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_connect_midi(move |index| {
            if let Some(app) = weak.upgrade() {
                let result = state
                    .audio
                    .borrow_mut()
                    .as_mut()
                    .ok_or_else(|| "audio engine is unavailable".to_string())
                    .and_then(|audio| audio.connect_midi(index.max(0) as usize));
                match result {
                    Ok(name) => {
                        *state.midi_status.borrow_mut() = format!("Connected · {name}");
                        app.set_midi_status(state.midi_status.borrow().as_str().into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("MIDI connect failed: {error}").into())
                    }
                }
            }
        });
    }
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-studio-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = StudioApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    wire_responsive_layout(&app);
    let recovered = initialize_snapshot_recovery()?;
    let ((session, assets), save_path) = if args.open.is_some() {
        initial_session(&args)?
    } else {
        match recovered
            .as_deref()
            .and_then(|bytes| load_studio_bundle(bytes).ok())
        {
            Some((project, assets)) => ((StudioSession::new(project), assets), None),
            None => initial_session(&args)?,
        }
    };
    let (audio, audio_error) = match AudioIo::open_default() {
        Ok(audio) => (Some(audio), None),
        Err(error) => (None, Some(error)),
    };
    let midi_ports = AudioIo::midi_ports().unwrap_or_default();
    app.set_midi_ports(ModelRc::new(VecModel::from(
        midi_ports
            .iter()
            .map(|port| SharedString::from(port.as_str()))
            .collect::<Vec<_>>(),
    )));
    let state = Rc::new(GuiState {
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(save_path),
        dialogs: Rc::new(NativeFileDialogs),
        audio: RefCell::new(audio),
        midi_status: RefCell::new(if midi_ports.is_empty() {
            "No MIDI input ports".into()
        } else {
            "MIDI input available".into()
        }),
        metronome_enabled: Cell::new(true),
        audio_error: RefCell::new(audio_error),
        gesture: RefCell::new(None),
        job: RefCell::new(JobUiState::default()),
    });

    wire_application(&app, state.clone());

    let ui_timer = Timer::default();
    {
        let weak = app.as_weak();
        let state = state.clone();
        ui_timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
            if let Some(app) = weak.upgrade() {
                let _ = poll_job(&app, &state);
                let audio = state.audio.borrow();
                if let Some(audio) = audio.as_ref() {
                    let playing = audio.is_playing();
                    let recording = audio.is_recording();
                    if app.get_is_playing() != playing {
                        app.set_is_playing(playing);
                    }
                    if app.get_is_recording() != recording {
                        app.set_is_recording(recording);
                    }
                    let pos = audio.position_seconds();
                    app.set_playhead_seconds(pos as f32);
                }
            }
        });
    }

    let menu_bar = build_standard_menu_bar(
        "Loom Studio",
        vec![MenuItem::action_with_shortcut(
            "file.export_wav",
            "Export Mix as WAV...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![
            Menu::new(
                "Track",
                vec![
                    MenuItem::action_with_shortcut(
                        "track.new",
                        "New Audio Track",
                        MenuShortcut::primary_shift("N"),
                    ),
                    MenuItem::action("track.duplicate", "Duplicate Track"),
                    MenuItem::action("track.delete", "Delete Track"),
                ],
            ),
            Menu::new(
                "Transport",
                vec![
                    MenuItem::action_with_shortcut(
                        "transport.play_pause",
                        "Play / Pause",
                        MenuShortcut::primary("Space"),
                    ),
                    MenuItem::action("transport.record", "Record"),
                    MenuItem::action("transport.metronome", "Toggle Metronome"),
                ],
            ),
        ],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    wire_palette(&app);
    refresh(&app, &state);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Commands exposed through the command palette.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewSong,
    OpenSong,
    SaveSong,
    SaveAsSong,
    Undo,
    Redo,
    AddTrack,
    PlayPause,
    Stop,
    ToggleRecord,
    ToggleLoop,
    ToggleMetronome,
    SetWorkspaceMode(i32),
    ImportAudio,
    ExportMix,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

const PALETTE_IMPORT_WAV: &str = "";

fn master_palette(app: &StudioApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewSong,
            id: "studio.new",
            label: "New Song",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenSong,
            id: "studio.open",
            label: "Open Song",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveSong,
            id: "studio.save",
            label: "Save Song",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsSong,
            id: "studio.save-as",
            label: "Save Song As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "studio.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "studio.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddTrack,
            id: "studio.track.add",
            label: "Add Track",
            shortcut: "Ctrl+T",
        },
        PaletteCommand {
            action: PaletteAction::PlayPause,
            id: "studio.play-pause",
            label: "Play / Pause",
            shortcut: "Space",
        },
        PaletteCommand {
            action: PaletteAction::Stop,
            id: "studio.stop",
            label: "Stop",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ToggleRecord,
            id: "studio.record",
            label: "Toggle Record",
            shortcut: "Ctrl+R",
        },
        PaletteCommand {
            action: PaletteAction::ToggleLoop,
            id: "studio.loop",
            label: "Toggle Loop",
            shortcut: "L",
        },
        PaletteCommand {
            action: PaletteAction::ToggleMetronome,
            id: "studio.metronome",
            label: "Toggle Metronome",
            shortcut: "M",
        },
        PaletteCommand {
            action: PaletteAction::SetWorkspaceMode(0),
            id: "studio.workspace.quick",
            label: "Quick Workspace",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetWorkspaceMode(1),
            id: "studio.workspace.pro",
            label: "Pro Workspace",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ImportAudio,
            id: "studio.import-audio",
            label: "Import Audio",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::ExportMix,
            id: "studio.export",
            label: "Export Mix",
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

fn rebuild_palette(app: &StudioApp, query: &str) {
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

fn wire_palette(app: &StudioApp) {
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
                        PaletteAction::NewSong => app.invoke_new_song(),
                        PaletteAction::OpenSong => app.invoke_open_song(),
                        PaletteAction::SaveSong => app.invoke_save_song(),
                        PaletteAction::SaveAsSong => app.invoke_save_as_song(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddTrack => app.invoke_add_track(),
                        PaletteAction::PlayPause => app.invoke_play_pause(),
                        PaletteAction::Stop => app.invoke_stop(),
                        PaletteAction::ToggleRecord => app.invoke_toggle_record(),
                        PaletteAction::ToggleLoop => app.invoke_toggle_loop(),
                        PaletteAction::ToggleMetronome => app.invoke_toggle_metronome(),
                        PaletteAction::SetWorkspaceMode(index) => {
                            app.invoke_set_workspace_mode(index)
                        }
                        PaletteAction::ImportAudio => {
                            app.invoke_import_audio(PALETTE_IMPORT_WAV.into())
                        }
                        PaletteAction::ExportMix => app.invoke_export_mix(app.get_export_path()),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (StudioApp, Rc<GuiState>) {
        set_platform();
        let app = StudioApp::new().expect("create StudioApp");
        let (session, assets) = sample_session().expect("sample_session");
        let state = Rc::new(GuiState {
            session: RefCell::new(session),
            assets: RefCell::new(assets),
            save_path: RefCell::new(None),
            dialogs: Rc::new(scripted),
            audio: RefCell::new(None),
            midi_status: RefCell::new("Test MIDI harness".into()),
            metronome_enabled: Cell::new(true),
            audio_error: RefCell::new(Some("test harness".into())),
            gesture: RefCell::new(None),
            job: RefCell::new(JobUiState::default()),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
    }

    #[test]
    fn toggle_metronome_updates_state_and_ui() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        assert!(state.metronome_enabled.get());
        assert!(app.get_metronome_on());

        app.invoke_toggle_metronome();
        assert!(!state.metronome_enabled.get());
        assert!(!app.get_metronome_on());

        app.invoke_toggle_metronome();
        assert!(state.metronome_enabled.get());
        assert!(app.get_metronome_on());
    }

    #[test]
    fn new_song_creates_untitled_clean_state() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        *state.save_path.borrow_mut() = Some(PathBuf::from("/tmp/existing.loomstudio"));

        app.invoke_new_song();
        assert_eq!(*state.save_path.borrow(), None);
        assert_eq!(state.session.borrow().project.name, "Untitled Session");
        assert_eq!(app.get_song_title().as_str(), "Untitled Session");
    }

    #[test]
    fn open_song_with_dialog_loads_path_and_updates_state() {
        let dir = std::env::temp_dir().join(format!("loom-studio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("open_test.loomstudio");

        let mut proj = StudioProject::new("loaded-proj", "Loaded Studio Project");
        proj.tracks.clear();
        let assets = AudioAssetStore::default();
        let bytes = save_studio_bundle(&proj, &assets).unwrap();
        std::fs::write(&file, bytes).unwrap();

        let scripted = ScriptedFileDialogs::new(vec![Some(file.clone())], vec![]);

        let (app, state) = test_app_and_state(scripted);
        app.invoke_open_song();

        assert_eq!(*state.save_path.borrow(), Some(file));
        assert_eq!(state.session.borrow().project.name, "Loaded Studio Project");
        assert_eq!(app.get_song_title().as_str(), "Loaded Studio Project");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_open_leaves_current_session_untouched() {
        let scripted = ScriptedFileDialogs::new(vec![None], vec![]);

        let (app, state) = test_app_and_state(scripted);
        let original_name = state.session.borrow().project.name.clone();

        app.invoke_open_song();
        assert_eq!(state.session.borrow().project.name, original_name);
        assert_eq!(app.get_status_left().as_str(), "Open cancelled");
    }

    #[test]
    fn save_untitled_prompts_dialog_and_writes_file() {
        let dir = std::env::temp_dir().join(format!("loom-studio-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved_project.loomstudio");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file.clone())]);

        let (app, state) = test_app_and_state(scripted);
        assert_eq!(*state.save_path.borrow(), None);

        app.invoke_save_song();

        assert_eq!(*state.save_path.borrow(), Some(file.clone()));
        assert!(file.is_file());
        let read_bytes = std::fs::read(&file).unwrap();
        let (loaded_proj, _) = load_studio_bundle(&read_bytes).unwrap();
        assert_eq!(loaded_proj.name, "Loom Studio Session");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_prompts_dialog_and_updates_path() {
        let dir = std::env::temp_dir().join(format!("loom-studio-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_v1 = dir.join("v1.loomstudio");
        let file_v2 = dir.join("v2.loomstudio");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file_v2.clone())]);

        let (app, state) = test_app_and_state(scripted);
        *state.save_path.borrow_mut() = Some(file_v1);

        app.invoke_save_as_song();

        assert_eq!(*state.save_path.borrow(), Some(file_v2.clone()));
        assert!(file_v2.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compact_layout_boundary_keeps_reference_width_stable() {
        set_platform();
        let app = StudioApp::new().expect("create StudioApp");
        assert!(compact_layout_for_width(&app, 1024));
        assert!(compact_layout_for_width(&app, 1179));
        assert!(!compact_layout_for_width(&app, 1180));
        assert!(!compact_layout_for_width(&app, 1440));
    }

    #[test]
    fn export_job_can_be_cancelled_and_preserves_destination() {
        let dir =
            std::env::temp_dir().join(format!("loom-studio-app-bounce-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("existing.wav");
        std::fs::write(&destination, b"existing").unwrap();
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        app.invoke_export_mix(destination.to_string_lossy().into_owned().into());
        assert!(matches!(
            state.job.borrow().state,
            StudioJobState::Running { .. }
        ));
        app.invoke_cancel_job();
        assert!(state
            .job
            .borrow()
            .cancellation
            .as_ref()
            .map(StudioCancellation::is_cancelled)
            .unwrap_or(false));
        wait_for_job(&app, &state, Duration::from_secs(5)).unwrap();
        assert!(matches!(
            state.job.borrow().state,
            StudioJobState::Cancelled { .. }
        ));
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arrangement_selection_gesture_coalesces_and_cancel_rolls_back() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        app.invoke_select_region(0, "region-vocal".into());
        assert_eq!(
            state.session.borrow().selection.region_id.as_deref(),
            Some("region-vocal")
        );
        assert_eq!(app.get_selected_region_id().as_str(), "region-vocal");
        let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
        app.invoke_begin_region_gesture(0, "region-vocal".into(), "move".into());
        app.invoke_move_region(0, "region-vocal".into(), 0.25);
        app.invoke_move_region(0, "region-vocal".into(), 0.25);
        app.invoke_end_region_gesture();
        assert_eq!(
            state.session.borrow().project.tracks[0].regions[0].start_sample,
            original_start + 24_000
        );
        assert!(state.session.borrow().can_undo());
        app.invoke_undo();
        assert_eq!(
            state.session.borrow().project.tracks[0].regions[0].start_sample,
            original_start
        );

        app.invoke_begin_region_gesture(0, "region-vocal".into(), "move".into());
        app.invoke_move_region(0, "region-vocal".into(), 0.5);
        app.invoke_cancel_region_gesture();
        assert_eq!(
            state.session.borrow().project.tracks[0].regions[0].start_sample,
            original_start
        );
        assert_eq!(app.get_status_left().as_str(), "Region edit cancelled");
    }

    #[test]
    fn mixer_and_record_arm_callbacks_update_projection_and_reject_invalid_edits() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        app.invoke_toggle_rec_arm(1);
        assert!(state.session.borrow().project.tracks[1].record_arm);
        assert!(app.get_track_arms().row_data(1).unwrap());
        assert_eq!(app.get_active_track_index(), 1);
        app.invoke_volume_changed(0, -9.0);
        app.invoke_pan_changed(0, 0.4);
        assert_eq!(state.session.borrow().project.tracks[0].volume_db, -9.0);
        assert_eq!(state.session.borrow().project.tracks[0].pan, 0.4);
        assert_eq!(app.get_active_track_index(), 0);
        let history_len_before = state.session.borrow().can_undo();
        app.invoke_pan_changed(0, 4.0);
        assert_eq!(state.session.borrow().project.tracks[0].pan, 0.4);
        assert!(history_len_before);
        app.invoke_toggle_mute(-1);
        assert!(app.get_status_left().contains("track index is invalid"));
        app.invoke_toggle_rec_arm(-1);
        assert!(app.get_status_left().contains("track index is invalid"));
    }

    #[test]
    fn import_job_adds_a_selected_region_and_device_failure_is_visible() {
        let dir =
            std::env::temp_dir().join(format!("loom-studio-app-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("input.wav");
        let bytes = AudioBuffer::sine(48_000, 1, 440.0, 0.05, 0.2)
            .unwrap()
            .to_wav_pcm16()
            .unwrap();
        std::fs::write(&source, bytes).unwrap();
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        let before = state.session.borrow().project.total_regions();
        app.invoke_import_audio(source.to_string_lossy().into_owned().into());
        wait_for_job(&app, &state, Duration::from_secs(5)).unwrap();
        assert_eq!(state.session.borrow().project.total_regions(), before + 1);
        assert!(state.session.borrow().selection.is_region());
        app.invoke_play_pause();
        assert!(app.get_status_left().contains("No audio output device"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
