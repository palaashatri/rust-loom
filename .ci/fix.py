from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


sheets_main = ROOT / "loom-sheets/crates/loom-sheets-app/src/main.rs"
text = sheets_main.read_text()
text = replace_once(
    text,
    '''    app.set_selection_formula(formula);
    app.invoke_reset_formula_edit_buffer();
    app.set_selected_cell(selected.to_a1().into());
    app.set_selection_value(SharedString::from(cell_value(''',
    '''    app.set_selection_formula(formula);
    app.invoke_reset_formula_edit_buffer();
    app.set_selected_cell(selected.to_a1().into());
    app.set_selected_row(selected.row as i32);
    app.set_selected_col(selected.col as i32);
    app.set_selection_value(SharedString::from(cell_value(''',
    "Sheets selected row and column",
)
sheets_main.write_text(text)

writer_ui = ROOT / "loom-writer/crates/loom-writer-app/ui/app.slint"
text = writer_ui.read_text()
text = replace_once(
    text,
    '''    in property <string> status-left: "";
    in property <string> status-right: "";

    in-out property <bool> is-bold: false;''',
    '''    in property <string> status-left: "";
    in property <string> status-right: "";
    in property <bool> can-undo: false;
    in property <bool> can-redo: false;

    in-out property <bool> is-bold: false;''',
    "Writer history properties",
)
text = replace_once(
    text,
    '''            IconButton { icon: "undo"; label: "Undo"; clicked => { root.undo(); } }
            IconButton { icon: "redo"; label: "Redo"; clicked => { root.redo(); } }''',
    '''            IconButton { icon: "undo"; label: "Undo"; enabled: root.can-undo; clicked => { root.undo(); } }
            IconButton { icon: "redo"; label: "Redo"; enabled: root.can-redo; clicked => { root.redo(); } }''',
    "Writer history control enablement",
)
writer_ui.write_text(text)

writer_main = ROOT / "loom-writer/crates/loom-writer-app/src/main.rs"
text = writer_main.read_text()
text = replace_once(
    text,
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
    apply_document(app, &state.current.borrow());
    let history = state.history.borrow();
    app.set_can_undo(!history.undo.is_empty());
    app.set_can_redo(!history.redo.is_empty());
    drop(history);
    state.syncing_editor.set(false);
}''',
    "Writer history binding",
)
writer_main.write_text(text)
