//! Loom Studio local-first DAW application.

mod audio_io;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use audio_io::{AudioIo, MidiEvent};
use loom_studio_core::{
    decode_wav, load_studio_bundle, save_studio_bundle, synthesize_notes, AudioAssetStore,
    AudioBuffer, AudioRegion, MidiNote, StudioProject, StudioSession, StudioTrack, TrackKind,
    WorkspaceMode,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "song.loomstudio";

loom_production::define_snapshot_recovery!(STUDIO_RECOVERY, "org.loom.studio", "loom.studio/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
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

fn initial_session(args: &Args) -> Result<(StudioSession, AudioAssetStore), String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|error| format!("failed to read Studio project '{path}': {error}"))?;
            let (project, assets) = load_studio_bundle(&bytes)
                .map_err(|error| format!("failed to load Studio project '{path}': {error}"))?;
            Ok((StudioSession::new(project), assets))
        }
        None => sample_session(),
    }
}

struct GuiState {
    session: RefCell<StudioSession>,
    assets: RefCell<AudioAssetStore>,
    record_arms: RefCell<Vec<bool>>,
    audio: RefCell<Option<AudioIo>>,
    midi_status: RefCell<String>,
}

fn refresh(app: &StudioApp, state: &GuiState) {
    let session = state.session.borrow();
    let project = &session.project;
    app.set_song_title(project.name.as_str().into());
    app.set_tempo_text(format!("{:.0} BPM · {} Hz", project.bpm, project.sample_rate).into());
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
            .map(|track| {
                SharedString::from(match track.kind {
                    TrackKind::Audio => "AUDIO",
                    TrackKind::Midi => "MIDI",
                    TrackKind::Drummer => "DRUMMER",
                    TrackKind::Bus => "BUS",
                })
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
    apply_theme(&app, &args.theme);
    let (session, assets) = initial_session(args)?;
    let state = GuiState {
        record_arms: RefCell::new(vec![false; session.project.tracks.len()]),
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        audio: RefCell::new(None),
        midi_status: RefCell::new("MIDI discovery unavailable in headless mode".into()),
    };
    refresh(&app, &state);
    app.set_status_left("Headless deterministic Studio workspace".into());
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn midi_event_label(event: MidiEvent) -> String {
    if event.len >= 3 {
        match event.bytes[0] & 0xF0 {
            0x90 if event.bytes[2] > 0 => {
                format!("Note on · {} · velocity {}", event.bytes[1], event.bytes[2])
            }
            0x80 | 0x90 => format!("Note off · {}", event.bytes[1]),
            0xB0 => format!("Controller {} · {}", event.bytes[1], event.bytes[2]),
            _ => format!(
                "MIDI {:02X} {:02X} {:02X}",
                event.bytes[0], event.bytes[1], event.bytes[2]
            ),
        }
    } else {
        "MIDI message".into()
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

    let app = StudioApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let recovered = initialize_snapshot_recovery()?;
    let (session, assets) = if args.open.is_some() {
        initial_session(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_studio_bundle(bytes).ok())
            .map(|(project, assets)| (StudioSession::new(project), assets))
            .unwrap_or(initial_session(&args)?)
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
        audio: RefCell::new(audio),
        midi_status: RefCell::new(if midi_ports.is_empty() {
            "No MIDI input ports".into()
        } else {
            "MIDI input available".into()
        }),
    });

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
                match sample_session() {
                    Ok((session, assets)) => {
                        *state.session.borrow_mut() = session;
                        *state.assets.borrow_mut() = assets;
                        state.record_arms.borrow_mut().clear();
                        if let Some(audio) = state.audio.borrow().as_ref() {
                            audio.stop();
                        }
                        refresh(&app, &state);
                        app.set_status_left("Created a new local Studio project".into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("New project failed: {error}").into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_open_song(move || {
            if let Some(app) = weak.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| load_studio_bundle(&bytes))
                {
                    Ok((project, assets)) => {
                        *state.session.borrow_mut() = StudioSession::new(project);
                        *state.assets.borrow_mut() = assets;
                        state.record_arms.borrow_mut().clear();
                        refresh(&app, &state);
                        app.set_status_left(format!("Opened {SAVE_FILENAME}").into());
                    }
                    Err(error) => app.set_status_left(format!("Open failed: {error}").into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_save_song(move || {
            if let Some(app) = weak.upgrade() {
                let session = state.session.borrow();
                let assets = state.assets.borrow();
                match save_studio_bundle(&session.project, &assets).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                }) {
                    Ok(()) => app.set_status_left(
                        format!("Saved {SAVE_FILENAME} with embedded audio").into(),
                    ),
                    Err(error) => app.set_status_left(format!("Save failed: {error}").into()),
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
        let number = project.tracks.len() + 1;
        project.add_track(StudioTrack::new(
            format!("track-{number}"),
            format!("Track {number}"),
            TrackKind::Audio,
        ));
        project.active_track_index = project.tracks.len() - 1;
    });
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_select_track(move |index| {
            if let Some(app) = weak.upgrade() {
                if index >= 0 {
                    state
                        .session
                        .borrow_mut()
                        .project
                        .select_track(index as usize);
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
                session.project.mode = if index == 1 {
                    WorkspaceMode::Pro
                } else {
                    WorkspaceMode::Quick
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
    app.on_toggle_metronome(|| {});
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
    for action in 0..3 {
        let state = state.clone();
        let weak = app.as_weak();
        match action {
            0 => app.on_toggle_mute(move |index| {
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
            }),
            1 => app.on_toggle_solo(move |index| {
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
            }),
            _ => app.on_toggle_rec_arm(move |index| {
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
            }),
        }
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
                let path = PathBuf::from(path.trim());
                let result = std::fs::read(&path)
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
                        let name = path
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
                        app.set_status_left(format!("Imported {}", path.display()).into());
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
                let mut output = PathBuf::from(path.trim());
                if output.as_os_str().is_empty() {
                    output = "loom-studio-mix.wav".into();
                }
                if output.extension().is_none() {
                    output.set_extension("wav");
                }
                match mix_current(&state)
                    .and_then(|mix| mix.to_wav_pcm16())
                    .and_then(|bytes| {
                        std::fs::write(&output, bytes).map_err(|error| error.to_string())
                    }) {
                    Ok(()) => app.set_status_left(format!("Exported {}", output.display()).into()),
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

    let timer_state = state.clone();
    let timer_weak = app.as_weak();
    let ui_timer = Timer::default();
    ui_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        if let Some(app) = timer_weak.upgrade() {
            if let Some(audio) = timer_state.audio.borrow().as_ref() {
                app.set_playhead_seconds(audio.position_seconds() as f32);
                app.set_is_playing(audio.is_playing());
                app.set_is_recording(audio.is_recording());
                let events = audio.drain_midi_events();
                if let Some(event) = events.last().copied() {
                    let label = midi_event_label(event);
                    *timer_state.midi_status.borrow_mut() = label.clone();
                    app.set_midi_status(label.into());
                }
            }
        }
    });

    refresh(&app, &state);
    app.set_status_left("Real-time local audio engine initialized".into());
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())?;
    drop(ui_timer);
    Ok(())
}
