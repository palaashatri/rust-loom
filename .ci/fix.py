from pathlib import Path

path = Path(__file__).resolve().parents[1] / "loom-encode/crates/loom-encode-app/src/main.rs"
text = path.read_text(encoding="utf-8")
old = '''    let backend = state.backend.clone();
    let running = state.running.load(Ordering::Relaxed);
    let _ = weak.upgrade_in_event_loop(move |app| {
        refresh(&app, &queue, backend.as_ref(), running);
        update_history_controls(&app, &state);
'''
new = '''    let backend = state.backend.clone();
    let running = state.running.load(Ordering::Relaxed);
    let state = state.clone();
    let _ = weak.upgrade_in_event_loop(move |app| {
        refresh(&app, &queue, backend.as_ref(), running);
        update_history_controls(&app, &state);
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one Encode event-loop ownership block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")
