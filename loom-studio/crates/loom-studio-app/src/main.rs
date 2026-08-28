//! Loom Studio local-first DAW application.

mod audio_io;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use audio_io::AudioIo;
use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_studio_core::{
    decode_wav, load_studio_bundle, save_studio_bundle, synthesize_notes, AudioAssetStore,
    AudioBuffer, AudioRegion, MidiNote, StudioProject, StudioSession, StudioTrack, TrackKind,
    WorkspaceMode,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, Timer, TimerMode, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const COMPACT_LAYOUT_MAX_WIDTH: u32 = 1220;

loom_production::define_snapshot_recovery!(STUDIO_RECOVERY, "org.loom.studio", "loom.studio/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
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

fn compact_layout_for_width(width: u32) -> bool {
    width < COMPACT_LAYOUT_MAX_WIDTH
}

fn configure_responsive_layout(app: &StudioApp, width: u32) {
    app.set_compact_layout(compact_layout_for_width(width));
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
    record_arms: RefCell<Vec<bool>>,
    session: RefCell<StudioSession>,
    assets: RefCell<AudioAssetStore>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    audio: RefCell<Option<AudioIo>>,
    midi_status: RefCell<String>,
    metronome_enabled: Cell<bool>,
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
    app.set_track_region_starts(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| {
                track
                    .regions
                    .first()
                    .map(|region| region.start_sample as f32 / project.sample_rate.max(1) as f32)
                    .unwrap_or(0.0)
            })
            .collect::<Vec<_>>(),
    )));
    app.set_track_region_durations(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| {
                track
                    .regions
                    .first()
                    .map(|region| region.length_samples as f32 / project.sample_rate.max(1) as f32)
                    .unwrap_or(0.0)
            })
            .collect::<Vec<_>>(),
    )));
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
    let mut arms = state.record_arms.borrow_mut();
    arms.resize(project.tracks.len(), false);
    app.set_track_arms(ModelRc::new(VecModel::from(arms.clone())));
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
        app.set_output_device("No audio output device".into());
        app.set_input_device("No audio input device".into());
        app.set_is_playing(false);
        app.set_is_recording(false);
        app.set_status_right("Audio unavailable".into());
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
    if let Ok(bytes) = save_studio_bundle(project, &state.assets.borrow()) {
        let _ = record_snapshot_recovery("studio state", bytes);
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
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let ((session, assets), save_path) = initial_session(args)?;
    let state = GuiState {
        record_arms: RefCell::new(vec![false; session.project.tracks.len()]),
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(save_path),
        dialogs: Rc::new(NativeFileDialogs),
        audio: RefCell::new(None),
        midi_status: RefCell::new("MIDI discovery unavailable in headless mode".into()),
        metronome_enabled: Cell::new(true),
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

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = StudioApp::new().map_err(|error| error.to_string())?;
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let ((session, assets), save_path) = initial_session(args)?;
    let state = GuiState {
        record_arms: RefCell::new(vec![false; session.project.tracks.len()]),
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(save_path),
        dialogs: Rc::new(NativeFileDialogs),
        audio: RefCell::new(None),
        midi_status: RefCell::new("MIDI discovery unavailable in headless mode".into()),
        metronome_enabled: Cell::new(true),
    };
    refresh(&app, &state);
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
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "studio", Path::new(out_dir), "workspace")
        .map_err(|error| format!("journey failed: {error}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
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
                    session.checkpoint();
                    ($body)(&mut session.project);
                    drop(session);
                    refresh(&app, &state);
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
                state.record_arms.borrow_mut().clear();
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
                            state.record_arms.borrow_mut().clear();
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
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
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
        let weak = app.as_weak();
        app.on_set_workspace_mode(move |index| {
            if let Some(app) = weak.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                session.project.mode = match index {
                    1 => WorkspaceMode::Pro,
                    _ => WorkspaceMode::Quick,
                };
                drop(session);
                refresh(&app, &state);
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
                session.checkpoint();
                session.project.bpm = value.clamp(40.0, 240.0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_mute(move |index| {
            if let Some(app) = weak.upgrade() {
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.mute = !track.mute;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_solo(move |index| {
            if let Some(app) = weak.upgrade() {
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.solo = !track.solo;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_toggle_rec_arm(move |index| {
            if let Some(app) = weak.upgrade() {
                if index >= 0 {
                    let mut arms = state.record_arms.borrow_mut();
                    if let Some(armed) = arms.get_mut(index as usize) {
                        *armed = !*armed;
                    }
                    drop(arms);
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
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.volume_db = value.clamp(-60.0, 6.0);
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_pan_changed(move |index, value| {
            if let Some(app) = weak.upgrade() {
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.pan = value.clamp(-1.0, 1.0);
                    }
                    drop(session);
                    refresh(&app, &state);
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

                let result = std::fs::read(&chosen_path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| decode_wav(&bytes))
                    .and_then(|buffer| {
                        if !matches!(buffer.channels, 1 | 2) {
                            return Err("only mono and stereo WAV files are supported".into());
                        }
                        let target_rate = state.session.borrow().project.sample_rate;
                        buffer.resample_linear(target_rate)
                    });
                match result {
                    Ok(buffer) => {
                        let name = chosen_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Imported Audio.wav")
                            .to_string();
                        let frames = buffer.frames();
                        if let Err(error) = state.assets.borrow_mut().insert(name.clone(), buffer) {
                            app.set_status_left(format!("Import failed: {error}").into());
                            return;
                        }
                        let mut session = state.session.borrow_mut();
                        session.checkpoint();
                        let active = session.project.active_track_index;
                        if let Some(track) = session.project.tracks.get_mut(active) {
                            track.add_region(AudioRegion {
                                id: format!("import-{}", track.regions.len() + 1),
                                name,
                                start_sample: 0,
                                length_samples: frames,
                            });
                        }
                        drop(session);
                        refresh(&app, &state);
                        app.set_status_left(format!("Imported {}", chosen_path.display()).into());
                    }
                    Err(error) => app.set_status_left(format!("Import failed: {error}").into()),
                }
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

                let result = mix_current(&state).and_then(|mix| {
                    let wav_bytes = mix.to_wav_pcm16()?;
                    loom_storage::atomic_write(&chosen_path, &wav_bytes)
                        .map_err(|error| error.to_string())
                });
                match result {
                    Ok(()) => app.set_status_left(
                        format!(
                            "Exported mix to {}",
                            chosen_path.file_name().unwrap().to_string_lossy()
                        )
                        .into(),
                    ),
                    Err(error) => app.set_status_left(format!("Export failed: {error}").into()),
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
    let audio = AudioIo::open_default().ok();
    let midi_ports = AudioIo::midi_ports().unwrap_or_default();
    app.set_midi_ports(ModelRc::new(VecModel::from(
        midi_ports
            .iter()
            .map(|port| SharedString::from(port.as_str()))
            .collect::<Vec<_>>(),
    )));
    let state = Rc::new(GuiState {
        record_arms: RefCell::new(vec![false; session.project.tracks.len()]),
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
    });

    wire_application(&app, state.clone());

    let ui_timer = Timer::default();
    {
        let weak = app.as_weak();
        let state = state.clone();
        ui_timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
            if let Some(app) = weak.upgrade() {
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
            record_arms: RefCell::new(vec![false; session.project.tracks.len()]),
            session: RefCell::new(session),
            assets: RefCell::new(assets),
            save_path: RefCell::new(None),
            dialogs: Rc::new(scripted),
            audio: RefCell::new(None),
            midi_status: RefCell::new("Test MIDI harness".into()),
            metronome_enabled: Cell::new(true),
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
        assert!(compact_layout_for_width(1024));
        assert!(compact_layout_for_width(1219));
        assert!(!compact_layout_for_width(1220));
        assert!(!compact_layout_for_width(1440));
    }
}
