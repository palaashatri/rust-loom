from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def save(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


# Motion: truthful undo/redo and real SVG-frame export.
motion_main_path = "loom-motion/crates/loom-motion-app/src/main.rs"
motion = load(motion_main_path)
motion = replace_once(
    motion,
    'const SAVE_FILENAME: &str = "comp.loommotion";\n',
    'const SAVE_FILENAME: &str = "comp.loommotion";\nconst EXPORT_FILENAME: &str = "composition-frame.svg";\nconst HISTORY_LIMIT: usize = 128;\n',
    "Motion constants",
)

motion = replace_once(
    motion,
    '''fn apply_motion(app: &MotionApp, doc: &CompositionDocument) {
''',
    '''fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn export_svg_frame(doc: &CompositionDocument, time_secs: f32) -> String {
    let mut svg = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
  <rect width="1920" height="1080" fill="#101217"/>
"#,
    );
    for (index, layer) in doc.layers.iter().enumerate() {
        let sample = layer.sample(time_secs);
        let opacity = sample.opacity.clamp(0.0, 1.0);
        let scale = sample.scale.max(0.001);
        let name = xml_escape(&layer.name);
        let transform = format!(
            "translate({:.3} {:.3}) rotate({:.3}) scale({:.5})",
            sample.x, sample.y, sample.rotation, scale
        );
        match layer.layer_type.as_str() {
            "Text" => svg.push_str(&format!(
                "  <text transform=\"{transform}\" opacity=\"{opacity:.5}\" text-anchor=\"middle\" fill=\"#f5f2eb\" font-family=\"sans-serif\" font-size=\"72\">{name}</text>\n"
            )),
            "VectorShape" => svg.push_str(&format!(
                "  <rect transform=\"{transform}\" opacity=\"{opacity:.5}\" x=\"-180\" y=\"-100\" width=\"360\" height=\"200\" rx=\"24\" fill=\"#b86f4b\"/>\n"
            )),
            _ => svg.push_str(&format!(
                "  <g transform=\"{transform}\" opacity=\"{opacity:.5}\"><rect x=\"-160\" y=\"-90\" width=\"320\" height=\"180\" rx=\"16\" fill=\"#303744\" stroke=\"#b86f4b\"/><text y=\"8\" text-anchor=\"middle\" fill=\"#f5f2eb\" font-family=\"sans-serif\" font-size=\"28\">{name} {}</text></g>\n",
                index + 1
            )),
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn write_svg_frame(doc: &CompositionDocument, path: impl AsRef<Path>) -> Result<(), String> {
    std::fs::write(path, export_svg_frame(doc, 0.0)).map_err(|error| error.to_string())
}

fn apply_motion(app: &MotionApp, doc: &CompositionDocument) {
''',
    "Motion SVG export helpers",
)

motion = replace_once(
    motion,
    '''struct GuiState {
    current: RefCell<CompositionDocument>,
}
''',
    '''#[derive(Default)]
struct MotionHistory {
    undo: Vec<CompositionDocument>,
    redo: Vec<CompositionDocument>,
    coalescing_key: Option<String>,
}

impl MotionHistory {
    fn checkpoint(&mut self, current: &CompositionDocument, key: impl Into<String>) {
        let key = key.into();
        if self.coalescing_key.as_deref() != Some(key.as_str()) {
            self.undo.push(current.clone());
            if self.undo.len() > HISTORY_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.coalescing_key = Some(key);
    }

    fn break_coalescing(&mut self) {
        self.coalescing_key = None;
    }

    fn undo(&mut self, current: &mut CompositionDocument) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(current.clone());
        *current = previous;
        self.break_coalescing();
        true
    }

    fn redo(&mut self, current: &mut CompositionDocument) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(current.clone());
        *current = next;
        self.break_coalescing();
        true
    }
}

struct GuiState {
    current: RefCell<CompositionDocument>,
    history: RefCell<MotionHistory>,
}

impl GuiState {
    fn checkpoint(&self, key: impl Into<String>) {
        let current = self.current.borrow();
        self.history.borrow_mut().checkpoint(&current, key);
    }

    fn replace(&self, document: CompositionDocument) {
        *self.current.borrow_mut() = document;
        *self.history.borrow_mut() = MotionHistory::default();
    }
}

fn refresh_motion(app: &MotionApp, state: &GuiState) {
    apply_motion(app, &state.current.borrow());
    let history = state.history.borrow();
    app.set_can_undo(!history.undo.is_empty());
    app.set_can_redo(!history.redo.is_empty());
}
''',
    "Motion history state",
)

motion = replace_once(
    motion,
    '''    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
    });
''',
    '''    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
        history: RefCell::new(MotionHistory::default()),
    });
''',
    "Motion state initialization",
)

motion = replace_once(
    motion,
    '''                *state.current.borrow_mut() = sample_motion();
                apply_motion(&app, &state.current.borrow());
''',
    '''                state.replace(sample_motion());
                refresh_motion(&app, &state);
''',
    "Motion new composition",
)

motion = replace_once(
    motion,
    '''                        *state.current.borrow_mut() = doc;
                        apply_motion(&app, &state.current.borrow());
''',
    '''                        state.replace(doc);
                        refresh_motion(&app, &state);
''',
    "Motion open composition",
)

motion = replace_once(
    motion,
    '''            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.len() + 1;
''',
    '''            if let Some(app) = app_ref.upgrade() {
                state.checkpoint("add-layer");
                let mut current = state.current.borrow_mut();
                let count = current.len() + 1;
''',
    "Motion layer checkpoint",
)

motion = replace_once(
    motion,
    '''                apply_motion(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer''',
    '''                drop(current);
                refresh_motion(&app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer''',
    "Motion add layer refresh",
)

motion = replace_once(
    motion,
    '''                if current.select_layer(index as usize) {
                    apply_motion(&app, &current);
                }
''',
    '''                if current.select_layer(index as usize) {
                    state.history.borrow_mut().break_coalescing();
                    drop(current);
                    refresh_motion(&app, &state);
                }
''',
    "Motion selection refresh",
)

motion = replace_once(
    motion,
    '''                let mut current = state.current.borrow_mut();
                let active = current.active_layer_index;
                let property = match prop.as_str() {
''',
    '''                let active = state.current.borrow().active_layer_index;
                let property = match prop.as_str() {
''',
    "Motion transform borrow order",
)

motion = replace_once(
    motion,
    '''                let stored_value = match property {
                    "scale" | "opacity" => val / 100.0,
                    _ => val,
                };
                if let Some(layer) = current.layers.get_mut(active) {
''',
    '''                let stored_value = match property {
                    "scale" | "opacity" => val / 100.0,
                    _ => val,
                };
                state.checkpoint(format!("transform:{active}:{property}"));
                let mut current = state.current.borrow_mut();
                if let Some(layer) = current.layers.get_mut(active) {
''',
    "Motion transform checkpoint",
)

motion = replace_once(
    motion,
    '''                    apply_motion(&app, &current);
                    app.set_status_left(SharedString::from(format!(
''',
    '''                    drop(current);
                    refresh_motion(&app, &state);
                    app.set_status_left(SharedString::from(format!(
''',
    "Motion transform refresh",
)

motion = replace_once(
    motion,
    '''    {
        let app_ref = app.as_weak();
        app.on_toggle_curve_drawer''',
    '''    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                let changed = {
                    let mut current = state.current.borrow_mut();
                    state.history.borrow_mut().undo(&mut current)
                };
                if changed {
                    refresh_motion(&app, &state);
                    app.set_status_left("Undid composition edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                let changed = {
                    let mut current = state.current.borrow_mut();
                    state.history.borrow_mut().redo(&mut current)
                };
                if changed {
                    refresh_motion(&app, &state);
                    app.set_status_left("Redid composition edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_frame(move || {
            if let Some(app) = app_ref.upgrade() {
                match write_svg_frame(&state.current.borrow(), EXPORT_FILENAME) {
                    Ok(()) => app.set_status_left(
                        format!("Exported SVG frame to {EXPORT_FILENAME}").into(),
                    ),
                    Err(error) => app
                        .set_status_left(format!("SVG frame export failed: {error}").into()),
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_curve_drawer''',
    "Motion undo redo export callbacks",
)

motion = replace_once(
    motion,
    '''    apply_motion(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
''',
    '''    refresh_motion(&app, &state);
    app.show().map_err(|e| e.to_string())?;
''',
    "Motion initial refresh",
)

motion += '''

#[cfg(test)]
mod product_tests {
    use super::*;

    #[test]
    fn svg_frame_export_contains_sampled_layers() {
        let svg = export_svg_frame(&sample_motion(), 0.0);
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Subtitle Motion"));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn motion_history_undoes_and_redoes_edits() {
        let mut current = sample_motion();
        let original = current.clone();
        let mut history = MotionHistory::default();
        history.checkpoint(&current, "add-layer");
        current.add_layer(MotionLayer::new("extra", "Extra", "VectorShape"));
        assert!(history.undo(&mut current));
        assert_eq!(current.layers.len(), original.layers.len());
        assert!(history.redo(&mut current));
        assert_eq!(current.layers.len(), original.layers.len() + 1);
    }
}
'''
save(motion_main_path, motion)

motion_ui_path = "loom-motion/crates/loom-motion-app/ui/app.slint"
motion_ui = load(motion_ui_path)
motion_ui = replace_once(
    motion_ui,
    '''    in property <string> status-left: "Ready";
    in property <string> status-right: "Offline";

    callback new-comp;
''',
    '''    in property <string> status-left: "Ready";
    in property <string> status-right: "Offline";
    in property <bool> can-undo: false;
    in property <bool> can-redo: false;

    callback new-comp;
''',
    "Motion UI history properties",
)
motion_ui = replace_once(
    motion_ui,
    '''    callback save-comp;
    callback add-layer;
''',
    '''    callback save-comp;
    callback undo;
    callback redo;
    callback export-frame;
    callback add-layer;
''',
    "Motion UI callbacks",
)
motion_ui = replace_once(
    motion_ui,
    '''            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-comp(); } }
            Rectangle { width: 1px; height: 20px; background: Theme.palette().border; }
            ToolButton { icon: "plus"; text: "Layer"; clicked => { root.add-layer(); } }
''',
    '''            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-comp(); } }
            IconButton { icon: "undo"; label: "Undo"; enabled: root.can-undo; clicked => { root.undo(); } }
            IconButton { icon: "redo"; label: "Redo"; enabled: root.can-redo; clicked => { root.redo(); } }
            Rectangle { width: 1px; height: 20px; background: Theme.palette().border; }
            ToolButton { icon: "plus"; text: "Layer"; clicked => { root.add-layer(); } }
            ToolButton { icon: "export"; text: "Export SVG Frame"; clicked => { root.export-frame(); } }
''',
    "Motion toolbar actions",
)
save(motion_ui_path, motion_ui)

# Encode: real queue-edit undo/redo, disabled while the encoder owns the queue.
encode_main_path = "loom-encode/crates/loom-encode-app/src/main.rs"
encode = load(encode_main_path)
encode = replace_once(
    encode,
    'const SAVE_FILENAME: &str = "batch.loomencode";\n',
    'const SAVE_FILENAME: &str = "batch.loomencode";\nconst HISTORY_LIMIT: usize = 128;\n',
    "Encode history constant",
)
encode = replace_once(
    encode,
    '''struct AppState {
    queue: Mutex<EncodeQueue>,
    backend: Option<EncoderBackend>,
''',
    '''#[derive(Default)]
struct QueueHistory {
    undo: Vec<EncodeQueue>,
    redo: Vec<EncodeQueue>,
}

impl QueueHistory {
    fn checkpoint(&mut self, queue: &EncodeQueue) {
        self.undo.push(queue.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self, queue: &mut EncodeQueue) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(queue.clone());
        *queue = previous;
        true
    }

    fn redo(&mut self, queue: &mut EncodeQueue) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(queue.clone());
        *queue = next;
        true
    }
}

struct AppState {
    queue: Mutex<EncodeQueue>,
    history: Mutex<QueueHistory>,
    backend: Option<EncoderBackend>,
''',
    "Encode history state",
)
encode = replace_once(
    encode,
    '''fn post_refresh(weak: &slint::Weak<EncodeApp>, state: &Arc<AppState>, message: String) {
''',
    '''fn update_history_controls(app: &EncodeApp, state: &AppState) {
    let history = state
        .history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let editable = !state.running.load(Ordering::Relaxed);
    app.set_can_undo(editable && !history.undo.is_empty());
    app.set_can_redo(editable && !history.redo.is_empty());
}

fn checkpoint_queue(state: &AppState, queue: &EncodeQueue) {
    state
        .history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .checkpoint(queue);
}

fn post_refresh(weak: &slint::Weak<EncodeApp>, state: &Arc<AppState>, message: String) {
''',
    "Encode history helpers",
)
encode = replace_once(
    encode,
    '''        refresh(&app, &queue, backend.as_ref(), running);
        app.set_status_left(message.into());
''',
    '''        refresh(&app, &queue, backend.as_ref(), running);
        update_history_controls(&app, &state);
        app.set_status_left(message.into());
''',
    "Encode post refresh history state",
)
encode = replace_once(
    encode,
    '''    let state = Arc::new(AppState {
        queue: Mutex::new(initial),
        backend,
''',
    '''    let state = Arc::new(AppState {
        queue: Mutex::new(initial),
        history: Mutex::new(QueueHistory::default()),
        backend,
''',
    "Encode state initialization",
)
encode = replace_once(
    encode,
    '''                    ($operation)(&mut queue);
                    refresh(
''',
    '''                    checkpoint_queue(&state, &queue);
                    ($operation)(&mut queue);
                    refresh(
''',
    "Encode queue callback checkpoint",
)
encode = replace_once(
    encode,
    '''                        state.running.load(Ordering::Relaxed),
                    );
                }
            });
''',
    '''                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            });
''',
    "Encode queue callback history controls",
)
encode = replace_once(
    encode,
    '''                        *state
                            .queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = queue;
                        let queue = snapshot(&state);
''',
    '''                        *state
                            .queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = queue;
                        *state
                            .history
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                            QueueHistory::default();
                        let queue = snapshot(&state);
''',
    "Encode open resets history",
)
encode = replace_once(
    encode,
    '''                        refresh(&app, &queue, state.backend.as_ref(), false);
                        app.set_status_left(format!("Opened {SAVE_FILENAME}").into());
''',
    '''                        refresh(&app, &queue, state.backend.as_ref(), false);
                        update_history_controls(&app, &state);
                        app.set_status_left(format!("Opened {SAVE_FILENAME}").into());
''',
    "Encode open controls",
)

for label, needle in (
    ("source", '''                    let index = queue.active_job_index;
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.source_file = value.as_str().to_string();'''),
    ("output", '''                    let index = queue.active_job_index;
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.output_file = value.as_str().to_string();'''),
    ("preset", '''                    let index = queue.active_job_index;
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.preset = match value.as_str() {'''),
):
    replacement = needle.replace(
        "                    let index = queue.active_job_index;\n",
        "                    let index = queue.active_job_index;\n                    checkpoint_queue(&state, &queue);\n",
        1,
    )
    encode = replace_once(encode, needle, replacement, f"Encode {label} checkpoint")

# Each field callback has the same refresh tail; add history-control synchronization to all three.
field_tail = '''                        state.running.load(Ordering::Relaxed),
                    );
                }
            }),
'''
field_tail_new = '''                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            }),
'''
if encode.count(field_tail) != 3:
    raise RuntimeError(f"Encode field refresh tails: expected 3, found {encode.count(field_tail)}")
encode = encode.replace(field_tail, field_tail_new)

encode = replace_once(
    encode,
    '''    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_start_queue''',
    '''    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    return;
                }
                let changed = {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state
                        .history
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .undo(&mut queue)
                };
                if changed {
                    let queue = snapshot(&state);
                    refresh(&app, &queue, state.backend.as_ref(), false);
                    update_history_controls(&app, &state);
                    app.set_status_left("Undid queue edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    return;
                }
                let changed = {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state
                        .history
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .redo(&mut queue)
                };
                if changed {
                    let queue = snapshot(&state);
                    refresh(&app, &queue, state.backend.as_ref(), false);
                    update_history_controls(&app, &state);
                    app.set_status_left("Redid queue edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_start_queue''',
    "Encode undo redo callbacks",
)
encode = replace_once(
    encode,
    '''    refresh(&app, &queue, state.backend.as_ref(), false);
    app.set_status_left("Edit a source/output path, then start the local queue".into());
''',
    '''    refresh(&app, &queue, state.backend.as_ref(), false);
    update_history_controls(&app, &state);
    app.set_status_left("Edit a source/output path, then start the local queue".into());
''',
    "Encode initial history controls",
)
encode += '''

#[cfg(test)]
mod product_tests {
    use super::*;

    #[test]
    fn queue_history_undoes_and_redoes_edits() {
        let mut queue = sample_queue();
        let original_len = queue.jobs.len();
        let mut history = QueueHistory::default();
        history.checkpoint(&queue);
        queue.add_job(EncodeJob::new(
            "extra",
            "extra.mov",
            "extra.mp4",
            EncodePreset::h264_1080p(),
        ));
        assert!(history.undo(&mut queue));
        assert_eq!(queue.jobs.len(), original_len);
        assert!(history.redo(&mut queue));
        assert_eq!(queue.jobs.len(), original_len + 1);
    }
}
'''
save(encode_main_path, encode)

encode_ui_path = "loom-encode/crates/loom-encode-app/ui/app.slint"
encode_ui = load(encode_ui_path)
encode_ui = replace_once(
    encode_ui,
    '''    in property <bool> running: false;
    in property <bool> can-start: false;
''',
    '''    in property <bool> running: false;
    in property <bool> can-start: false;
    in property <bool> can-undo: false;
    in property <bool> can-redo: false;
''',
    "Encode UI history properties",
)
encode_ui = replace_once(
    encode_ui,
    '''    callback save-queue;
    callback add-job;
''',
    '''    callback save-queue;
    callback undo;
    callback redo;
    callback add-job;
''',
    "Encode UI callbacks",
)
encode_ui = replace_once(
    encode_ui,
    '''            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-queue(); } }
            Rectangle { width: 1px; height: 20px; background: Theme.palette().border; }
            ToolButton { icon: "plus"; text: "Add Job"; clicked => { root.add-job(); } }
''',
    '''            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-queue(); } }
            IconButton { icon: "undo"; label: "Undo queue edit"; enabled: root.can-undo && !root.running; clicked => { root.undo(); } }
            IconButton { icon: "redo"; label: "Redo queue edit"; enabled: root.can-redo && !root.running; clicked => { root.redo(); } }
            Rectangle { width: 1px; height: 20px; background: Theme.palette().border; }
            ToolButton { icon: "plus"; text: "Add Job"; enabled: !root.running; clicked => { root.add-job(); } }
''',
    "Encode toolbar history controls",
)
save(encode_ui_path, encode_ui)
