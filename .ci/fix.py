from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:140]!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


# Shared per-application recovery slot macro.
replace_once(
    "loom-core/crates/loom-production/src/snapshot.rs",
    '''#[cfg(test)]
mod tests {''',
    '''/// Define one thread-local recovery slot for a desktop application.
///
/// The generated helpers are intentionally private to the invoking binary:
/// `initialize_snapshot_recovery`, `record_snapshot_recovery`, and
/// `checkpoint_snapshot_recovery`. Headless render paths that never initialize
/// the slot safely treat recording as a no-op.
#[macro_export]
macro_rules! define_snapshot_recovery {
    ($slot:ident, $application_id:literal, $schema:literal) => {
        std::thread_local! {
            static $slot: std::cell::RefCell<Option<$crate::snapshot::SnapshotRecovery>> =
                std::cell::RefCell::new(None);
        }

        fn initialize_snapshot_recovery() -> Result<Option<Vec<u8>>, String> {
            let mut recovery = $crate::snapshot::SnapshotRecovery::open($application_id)
                .map_err(|error| error.to_string())?;
            let restored = recovery.take_restored_payload();
            $slot.with(|slot| {
                *slot.borrow_mut() = Some(recovery);
            });
            Ok(restored)
        }

        fn record_snapshot_recovery(label: &str, payload: Vec<u8>) -> Result<(), String> {
            $slot.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some(recovery) => recovery
                        .record(label, payload)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    None => Ok(()),
                }
            })
        }

        fn checkpoint_snapshot_recovery(payload: Vec<u8>) -> Result<(), String> {
            $slot.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some(recovery) => recovery
                        .checkpoint($schema, payload)
                        .map_err(|error| error.to_string()),
                    None => Ok(()),
                }
            })
        }
    };
}

#[cfg(test)]
mod tests {''',
)

# App dependencies.
for path, anchor in [
    ("loom-writer/crates/loom-writer-app/Cargo.toml", 'loom-writer-core = { path = "../loom-writer-core" }\n'),
    ("loom-sheets/crates/loom-sheets-app/Cargo.toml", 'loom-sheets-core = { path = "../loom-sheets-core" }\n'),
    ("loom-present/crates/loom-present-app/Cargo.toml", 'loom-present-core = { path = "../loom-present-core" }\n'),
    ("loom-photo/crates/loom-photo-app/Cargo.toml", 'loom-photo-core = { path = "../loom-photo-core" }\n'),
]:
    replace_once(
        path,
        anchor,
        anchor + 'loom-production = { path = "../../../loom-core/crates/loom-production" }\n',
    )

# Writer: native package snapshots on every model refresh and checkpoints on save.
writer = "loom-writer/crates/loom-writer-app/src/main.rs"
replace_once(
    writer,
    '''const TYPING_COALESCE_WINDOW_MS: u64 = 750;
''',
    '''const TYPING_COALESCE_WINDOW_MS: u64 = 750;

loom_production::define_snapshot_recovery!(
    WRITER_RECOVERY,
    "org.loom.writer",
    "loom.writer/1"
);
''',
)
replace_once(
    writer,
    '''fn apply_state(app: &WriterApp, state: &GuiState) {
    // TextEdit owns a native text buffer. Rebinding it after a model/history
    // operation must not be observed as another user edit transaction.
    state.syncing_editor.set(true);
    apply_document(app, &state.current.borrow());
    state.syncing_editor.set(false);
}''',
    '''fn apply_state(app: &WriterApp, state: &GuiState) {
    // TextEdit owns a native text buffer. Rebinding it after a model/history
    // operation must not be observed as another user edit transaction.
    state.syncing_editor.set(true);
    let current = state.current.borrow();
    apply_document(app, &current);
    if let Ok(bytes) = loom_writer_core::save_document(&current) {
        let _ = record_snapshot_recovery("writer state", bytes);
    }
    state.syncing_editor.set(false);
}''',
)
replace_once(
    writer,
    '''    let state = Rc::new(GuiState {
        current: RefCell::new(match &args.open {
            Some(p) => load_file(p)?,
            None => sample_document(),
        }),''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial_document = match &args.open {
        Some(path) => load_file(path)?,
        None => recovered
            .as_deref()
            .and_then(|bytes| loom_writer_core::load_document(bytes).ok())
            .unwrap_or_else(sample_document),
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(initial_document),''',
)
replace_once(
    writer,
    '''                match save_file(&p, &state.current.borrow()) {
                    Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }''',
    '''                match save_file(&p, &state.current.borrow()) {
                    Ok(()) => {
                        let checkpoint = loom_writer_core::save_document(&state.current.borrow())
                            .map_err(|error| error.to_string())
                            .and_then(checkpoint_snapshot_recovery);
                        match checkpoint {
                            Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "saved {p}, but recovery checkpoint failed: {error}"
                            ))),
                        }
                    }
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }''',
)

# Sheets: JSON model snapshots are deterministic and are checkpointed alongside the native package save.
sheets = "loom-sheets/crates/loom-sheets-app/src/main.rs"
replace_once(
    sheets,
    '''const EXPORT_FILENAME: &str = "loom-sheets-export.csv";
''',
    '''const EXPORT_FILENAME: &str = "loom-sheets-export.csv";

loom_production::define_snapshot_recovery!(
    SHEETS_RECOVERY,
    "org.loom.sheets",
    "loom.sheets/1"
);
''',
)
replace_once(
    sheets,
    '''    app.set_status_right("Offline".into());
}''',
    '''    app.set_status_right("Offline".into());
    let _ = record_snapshot_recovery("sheets state", sheet_to_json(sheet).into_bytes());
}''',
)
replace_once(
    sheets,
    '''    let state = Rc::new(GuiState {
        current: RefCell::new(match &args.open {
            Some(p) => load_sheet(p)?,
            None => sample_sheet(),
        }),''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial_sheet = match &args.open {
        Some(path) => load_sheet(path)?,
        None => recovered
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|json| sheet_from_json(json).ok())
            .unwrap_or_else(sample_sheet),
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(initial_sheet),''',
)
replace_once(
    sheets,
    '''                match save_sheet(&p, &state.current.borrow()) {
                    Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }''',
    '''                match save_sheet(&p, &state.current.borrow()) {
                    Ok(()) => {
                        let checkpoint = checkpoint_snapshot_recovery(
                            sheet_to_json(&state.current.borrow()).into_bytes(),
                        );
                        match checkpoint {
                            Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "saved {p}, but recovery checkpoint failed: {error}"
                            ))),
                        }
                    }
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }''',
)

# Present: package snapshots on refresh and checkpointed save.
present = "loom-present/crates/loom-present-app/src/main.rs"
replace_once(
    present,
    '''const EXPORT_FILENAME: &str = "presentation.pdf";
''',
    '''const EXPORT_FILENAME: &str = "presentation.pdf";

loom_production::define_snapshot_recovery!(
    PRESENT_RECOVERY,
    "org.loom.present",
    "loom.present/1"
);
''',
)
replace_once(
    present,
    '''    app.set_status_right("Local deck engine".into());
}''',
    '''    app.set_status_right("Local deck engine".into());
    if let Ok(bytes) = save_presentation_session(&session) {
        let _ = record_snapshot_recovery("presentation state", bytes);
    }
}''',
)
replace_once(
    present,
    '''    let state = Rc::new(GuiState {
        session: RefCell::new(initial_session(&args)?),
        selected_element: Cell::new(0),
    });''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_session(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_presentation_session(bytes).ok())
            .unwrap_or(initial_session(&args)?)
    };
    let state = Rc::new(GuiState {
        session: RefCell::new(initial),
        selected_element: Cell::new(0),
    });''',
)
replace_once(
    present,
    '''                match save_presentation_session(&state.session.borrow()).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                }) {
                    Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }''',
    '''                match save_presentation_session(&state.session.borrow()) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                        Err(error) => set_status(&app, format!("Save/checkpoint failed: {error}")),
                    },
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }''',
)

# Photo: canvas package snapshots on every compositor refresh.
photo = "loom-photo/crates/loom-photo-app/src/main.rs"
replace_once(
    photo,
    '''const DEFAULT_EXPORT_FILENAME: &str = "loom-photo-export.png";
''',
    '''const DEFAULT_EXPORT_FILENAME: &str = "loom-photo-export.png";

loom_production::define_snapshot_recovery!(
    PHOTO_RECOVERY,
    "org.loom.photo",
    "loom.photo/1"
);
''',
)
replace_once(
    photo,
    '''    app.set_status_right("Local CPU compositor".into());
    Ok(())''',
    '''    app.set_status_right("Local compositor".into());
    if let Ok(bytes) = save_photo_canvas(&session.canvas) {
        let _ = record_snapshot_recovery("photo state", bytes);
    }
    Ok(())''',
)
replace_once(
    photo,
    '''    let state = Rc::new(GuiState {
        session: RefCell::new(PhotoSession::new(initial_canvas(&args)?)),
    });''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_canvas(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_photo_canvas(bytes).ok())
            .unwrap_or(initial_canvas(&args)?)
    };
    let state = Rc::new(GuiState {
        session: RefCell::new(PhotoSession::new(initial)),
    });''',
)
replace_once(
    photo,
    '''                match save_photo_canvas(&state.session.borrow().canvas).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                }) {
                    Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }''',
    '''                match save_photo_canvas(&state.session.borrow().canvas) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                        Err(error) => set_status(&app, format!("Save/checkpoint failed: {error}")),
                    },
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }''',
)
