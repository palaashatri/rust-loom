from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:160]!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


for path, anchor in [
    ("loom-motion/crates/loom-motion-app/Cargo.toml", 'loom-motion-core = { path = "../loom-motion-core" }\n'),
    ("loom-video/crates/loom-video-app/Cargo.toml", 'loom-video-core = { path = "../loom-video-core" }\n'),
    ("loom-studio/crates/loom-studio-app/Cargo.toml", 'loom-studio-core = { path = "../loom-studio-core" }\n'),
    ("loom-encode/crates/loom-encode-app/Cargo.toml", 'loom-encode-core = { path = "../loom-encode-core" }\n'),
]:
    replace_once(
        path,
        anchor,
        anchor + 'loom-production = { path = "../../../loom-core/crates/loom-production" }\n',
    )

# Motion native composition recovery.
motion = "loom-motion/crates/loom-motion-app/src/main.rs"
replace_once(
    motion,
    '''const SAVE_FILENAME: &str = "comp.loommotion";
''',
    '''const SAVE_FILENAME: &str = "comp.loommotion";

loom_production::define_snapshot_recovery!(
    MOTION_RECOVERY,
    "org.loom.motion",
    "loom.motion/1"
);
''',
)
replace_once(
    motion,
    '''    app.set_status_right("Offline".into());
}''',
    '''    app.set_status_right("Offline".into());
    if let Ok(bytes) = save_motion(doc) {
        let _ = record_snapshot_recovery("motion state", bytes);
    }
}''',
)
replace_once(
    motion,
    '''    let state = Rc::new(GuiState {
        current: RefCell::new(initial_motion(&args)?),
    });''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_motion(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_motion(bytes).ok())
            .unwrap_or(initial_motion(&args)?)
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
    });''',
)
replace_once(
    motion,
    '''                if let Ok(bytes) = save_motion(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }''',
    '''                match save_motion(&state.current.borrow()) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => app.set_status_left(SharedString::from(format!(
                            "Saved {SAVE_FILENAME}"
                        ))),
                        Err(error) => app.set_status_left(SharedString::from(format!(
                            "Save/checkpoint failed: {error}"
                        ))),
                    },
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Save failed: {error}"
                    ))),
                }''',
)

# Video native project recovery.
video = "loom-video/crates/loom-video-app/src/main.rs"
replace_once(
    video,
    '''const SAVE_FILENAME: &str = "project.loomvideo";
''',
    '''const SAVE_FILENAME: &str = "project.loomvideo";

loom_production::define_snapshot_recovery!(
    VIDEO_RECOVERY,
    "org.loom.video",
    "loom.video/1"
);
''',
)
replace_once(
    video,
    '''    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
    }
}''',
    '''    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
    }
    if let Ok(bytes) = save_video_project(project) {
        let _ = record_snapshot_recovery("video state", bytes);
    }
}''',
)
replace_once(
    video,
    '''    let state = Arc::new(AppState {
        session: Mutex::new(initial_session(&args)?),''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_session(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_video_project(bytes).ok())
            .map(VideoSession::new)
            .unwrap_or(initial_session(&args)?)
    };
    let state = Arc::new(AppState {
        session: Mutex::new(initial),''',
)
replace_once(
    video,
    '''                let result = save_video_project(&lock(&state.session).project).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                });''',
    '''                let result = save_video_project(&lock(&state.session).project).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                });''',
)

# Studio embedded-audio bundle recovery.
studio = "loom-studio/crates/loom-studio-app/src/main.rs"
replace_once(
    studio,
    '''const SAVE_FILENAME: &str = "song.loomstudio";
''',
    '''const SAVE_FILENAME: &str = "song.loomstudio";

loom_production::define_snapshot_recovery!(
    STUDIO_RECOVERY,
    "org.loom.studio",
    "loom.studio/1"
);
''',
)
replace_once(
    studio,
    '''    app.set_midi_status(state.midi_status.borrow().as_str().into());
}''',
    '''    app.set_midi_status(state.midi_status.borrow().as_str().into());
    if let Ok(bytes) = save_studio_bundle(project, &state.assets.borrow()) {
        let _ = record_snapshot_recovery("studio state", bytes);
    }
}''',
)
replace_once(
    studio,
    '''    let (session, assets) = initial_session(&args)?;
    let audio = AudioIo::open_default().ok();''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let (session, assets) = if args.open.is_some() {
        initial_session(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_studio_bundle(bytes).ok())
            .map(|(project, assets)| (StudioSession::new(project), assets))
            .unwrap_or(initial_session(&args)?)
    };
    let audio = AudioIo::open_default().ok();''',
)
replace_once(
    studio,
    '''                match save_studio_bundle(&session.project, &assets).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                }) {''',
    '''                match save_studio_bundle(&session.project, &assets).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                }) {''',
)

# Encode queue recovery, including background progress snapshots posted on the UI thread.
encode = "loom-encode/crates/loom-encode-app/src/main.rs"
replace_once(
    encode,
    '''const SAVE_FILENAME: &str = "batch.loomencode";
''',
    '''const SAVE_FILENAME: &str = "batch.loomencode";

loom_production::define_snapshot_recovery!(
    ENCODE_RECOVERY,
    "org.loom.encode",
    "loom.encode/1"
);
''',
)
replace_once(
    encode,
    '''    app.set_status_right(
        if backend.is_some() {
            "Local FFmpeg"
        } else {
            "Encoder unavailable"
        }
        .into(),
    );
}''',
    '''    app.set_status_right(
        if backend.is_some() {
            "Local FFmpeg"
        } else {
            "Encoder unavailable"
        }
        .into(),
    );
    if let Ok(bytes) = save_encode_queue(queue) {
        let _ = record_snapshot_recovery("encode queue state", bytes);
    }
}''',
)
replace_once(
    encode,
    '''    let backend = discover_ffmpeg(&[]).ok();
    let state = Arc::new(AppState {
        queue: Mutex::new(initial_queue(&args)?),''',
    '''    let backend = discover_ffmpeg(&[]).ok();
    let recovered = initialize_snapshot_recovery()?;
    let mut initial = if args.open.is_some() {
        initial_queue(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_encode_queue(bytes).ok())
            .unwrap_or(initial_queue(&args)?)
    };
    initial.recover_interrupted();
    let state = Arc::new(AppState {
        queue: Mutex::new(initial),''',
)
replace_once(
    encode,
    '''                let result = save_encode_queue(&snapshot(&state)).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                });''',
    '''                let result = save_encode_queue(&snapshot(&state)).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                });''',
)
