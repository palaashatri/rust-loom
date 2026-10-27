//! User-visible direction and inline-formatting behavior for the M3 UI lane.

use super::super::*;
use crate::actions_tests::{test_state, text_document};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{Key, WindowEvent};
use std::rc::Rc;

fn session(document: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    (app, state)
}

fn render_format_inspector(app: &WriterApp) {
    app.set_inspector_tab(0);
    app.set_show_inspector(true);
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render Format inspector");
}

fn has_accessible_role(app: &WriterApp, label: &str, role: AccessibleRole) -> bool {
    ElementHandle::find_by_accessible_label(app, label)
        .any(|element| element.accessible_role() == Some(role))
}

fn accessible_button(app: &WriterApp, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no accessible button named {label:?}"))
}

fn assert_checkable_button(app: &WriterApp, label: &str, checked: bool) {
    let button = accessible_button(app, label);
    assert_eq!(
        button.accessible_checkable(),
        Some(true),
        "{label} is a toggle choice"
    );
    assert_eq!(
        button.accessible_checked(),
        Some(checked),
        "{label} selected state"
    );
}

fn invoke_button(app: &WriterApp, label: &str) {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no accessible button named {label:?}"))
        .invoke_accessible_default_action();
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render after button");
}

fn key(app: &WriterApp, text: SharedString) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn replace_accessible_text_input(app: &WriterApp, label: &str, value: &str) {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput))
        .unwrap_or_else(|| panic!("no accessible text input named {label:?}"))
        .invoke_accessible_default_action();

    let control = SharedString::from(Key::Control);
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: control.clone(),
    });
    key(app, "a".into());
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: control });
    for character in value.chars() {
        key(app, character.to_string().into());
    }
}

fn style_at(document: &WriterDocument, offset: usize) -> loom_text::CharacterStyle {
    document.blocks[0]
        .runs
        .iter()
        .find(|run| run.start <= offset && offset < run.end)
        .map(|run| run.style.clone())
        .unwrap_or_default()
}

fn recovery_write_count() -> usize {
    recovery::WRITES.with(|writes| writes.get())
}

fn save_reload(document: &WriterDocument) -> WriterDocument {
    let bytes = loom_writer_core::save_document(document).expect("save formatted document");
    loom_writer_core::load_document(&bytes).expect("reload formatted document")
}

#[test]
fn format_inspector_exposes_direction_and_logical_alignment_choices() {
    let (app, _) = session(text_document("A paragraph"));
    render_format_inspector(&app);

    assert!(
        ElementHandle::find_by_accessible_label(&app, "Paragraph Direction")
            .next()
            .is_some(),
        "the Format inspector must expose paragraph direction"
    );
    for choice in ["Automatic", "Left to Right", "Right to Left"] {
        assert!(
            has_accessible_role(&app, choice, AccessibleRole::Button),
            "paragraph direction choice {choice:?} must be reachable"
        );
    }
    for choice in ["Start", "Left", "Center", "Right", "End"] {
        assert!(
            has_accessible_role(&app, choice, AccessibleRole::Button),
            "alignment choice {choice:?} must remain distinct and reachable"
        );
    }
}

#[test]
fn rtl_direction_command_preserves_physical_alignment_and_is_undoable() {
    let mut document = text_document("Latin words");
    document.blocks[0].style.alignment = loom_text::Alignment::Left;
    document.set_selection(TextSelection::range(0, 5));
    let (app, state) = session(document);
    let writes_before = recovery_write_count();

    assert!(
        toolbar_commands::dispatch(&app, "writer.direction.rtl"),
        "the direction command must be routed from toolbar and menu surfaces"
    );
    {
        let current = state.current.borrow();
        assert_eq!(
            current.blocks[0].style.direction,
            loom_text::ParagraphDirection::RightToLeft
        );
        assert_eq!(
            current.blocks[0].style.alignment,
            loom_text::Alignment::Left,
            "physical Left keeps its meaning when direction changes"
        );
        assert_eq!(current.editor_text(), "Latin words");
    }
    assert_eq!(state.history.borrow().undo_len(), 1);
    assert!(
        recovery_write_count() > writes_before,
        "paragraph direction edits must use the normal recovery path"
    );
    app.invoke_undo();
    assert_eq!(
        state.current.borrow().blocks[0].style.direction,
        loom_text::ParagraphDirection::Auto
    );
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        loom_text::Alignment::Left
    );
    app.invoke_redo();
    let reloaded = save_reload(&state.current.borrow());
    assert_eq!(
        reloaded.blocks[0].style.direction,
        loom_text::ParagraphDirection::RightToLeft
    );
    assert_eq!(
        reloaded.blocks[0].style.alignment,
        loom_text::Alignment::Left
    );
}

#[test]
fn logical_alignment_commands_remain_distinct_from_physical_left_and_right() {
    let mut document = text_document("paragraph start");
    document.blocks[0].style.direction = loom_text::ParagraphDirection::RightToLeft;
    document.blocks[0].style.alignment = loom_text::Alignment::Left;
    document.set_selection(TextSelection::range(0, 9));
    let (app, state) = session(document);

    assert!(toolbar_commands::dispatch(&app, "writer.align.start"));
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        loom_text::Alignment::Start
    );
    assert!(toolbar_commands::dispatch(&app, "writer.align.end"));
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        loom_text::Alignment::End
    );
    app.invoke_undo();
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        loom_text::Alignment::Start
    );
    app.invoke_redo();
    let reloaded = save_reload(&state.current.borrow());
    assert_eq!(
        reloaded.blocks[0].style.alignment,
        loom_text::Alignment::End
    );
    assert_eq!(
        reloaded.blocks[0].style.direction,
        loom_text::ParagraphDirection::RightToLeft
    );
}

#[test]
fn selected_superscript_and_subscript_are_mutually_exclusive_undoable_edits() {
    let mut document = text_document("first second");
    let mut existing = loom_text::CharacterStyle::default();
    existing.subscript = true;
    document.blocks[0].runs.push(loom_text::StyleRun {
        start: 0,
        end: 5,
        style: existing,
    });
    let end = document.blocks[0].text.as_str().len();
    document.set_selection(TextSelection::range(0, end));
    let (app, state) = session(document);
    let writes_before = recovery_write_count();

    assert!(toolbar_commands::dispatch(&app, "writer.style.superscript"));
    {
        let current = state.current.borrow();
        assert!(style_at(&current, 2).superscript);
        assert!(!style_at(&current, 2).subscript);
        assert!(
            style_at(&current, 8).superscript,
            "mixed selections become uniform"
        );
        assert_eq!(current.editor_text(), "first second");
    }
    assert!(recovery_write_count() > writes_before);
    assert!(toolbar_commands::dispatch(&app, "writer.style.subscript"));
    {
        let current = state.current.borrow();
        assert!(!style_at(&current, 2).superscript);
        assert!(style_at(&current, 2).subscript);
    }

    app.invoke_undo();
    assert!(style_at(&state.current.borrow(), 2).superscript);
    assert!(!style_at(&state.current.borrow(), 2).subscript);
    app.invoke_undo();
    assert!(!style_at(&state.current.borrow(), 2).superscript);
    assert!(style_at(&state.current.borrow(), 2).subscript);
    app.invoke_redo();
    assert!(style_at(&state.current.borrow(), 2).superscript);
    assert!(style_at(&save_reload(&state.current.borrow()), 8).superscript);
}

#[test]
fn direction_script_and_color_controls_follow_uniform_mixed_and_caret_selection() {
    let mut document = text_document("first\nsecond");
    document.blocks[0].style.direction = loom_text::ParagraphDirection::RightToLeft;
    let mut first_style = loom_text::CharacterStyle::default();
    first_style.superscript = true;
    first_style.color = Some("#CC3300FF".into());
    document.blocks[0].runs.push(loom_text::StyleRun {
        start: 0,
        end: 5,
        style: first_style,
    });
    document.set_selection(TextSelection::range(0, 5));
    let (app, _) = session(document);
    render_format_inspector(&app);

    assert_checkable_button(&app, "Right to Left", true);
    assert_checkable_button(&app, "Automatic", false);
    assert_checkable_button(&app, "Superscript", true);
    assert_checkable_button(&app, "Subscript", false);
    assert!(accessible_button(&app, "Text Color")
        .accessible_description()
        .is_some_and(|description| description.contains("#CC3300FF")));

    app.invoke_selection_changed(0, "first\nsecond".len() as i32);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render mixed selection state");
    assert_checkable_button(&app, "Right to Left", false);
    assert_checkable_button(&app, "Automatic", false);
    assert_checkable_button(&app, "Superscript", false);
    assert!(accessible_button(&app, "Superscript")
        .accessible_description()
        .is_some_and(|description| description.to_lowercase().contains("mixed")));
    assert_checkable_button(&app, "Subscript", false);
    assert_eq!(
        accessible_button(&app, "Subscript").accessible_description(),
        Some("Toggle subscript".into()),
        "a uniformly off script action should not be announced as mixed"
    );
    assert!(accessible_button(&app, "Text Color")
        .accessible_description()
        .is_some_and(|description| description.to_lowercase().contains("mixed")));

    app.invoke_selection_changed(2, 2);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render caret state");
    assert_checkable_button(&app, "Right to Left", true);
    assert_checkable_button(&app, "Superscript", true);
    assert_checkable_button(&app, "Subscript", false);
}

#[test]
fn superscript_at_a_collapsed_caret_styles_only_new_text_and_survives_undo_redo() {
    let mut document = text_document("plain");
    document.set_selection(TextSelection::caret(5));
    let (app, state) = session(document);

    assert!(toolbar_commands::dispatch(&app, "writer.style.superscript"));
    assert_eq!(state.current.borrow().editor_text(), "plain");
    app.invoke_document_edited("plainX".into(), 6, 6);
    assert!(style_at(&state.current.borrow(), 5).superscript);
    assert!(!style_at(&state.current.borrow(), 4).superscript);

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), "plain");
    app.invoke_redo();
    assert_eq!(state.current.borrow().editor_text(), "plainX");
    assert!(style_at(&state.current.borrow(), 5).superscript);
    assert!(style_at(&save_reload(&state.current.borrow()), 5).superscript);
}

#[test]
fn foreground_color_input_applies_canonical_color_and_refuses_invalid_text() {
    let mut document = text_document("color me");
    document.set_selection(TextSelection::range(0, 5));
    let (app, state) = session(document);
    render_format_inspector(&app);

    invoke_button(&app, "Text Color");
    replace_accessible_text_input(&app, "Foreground color", "#a1b2c3");
    invoke_button(&app, "Apply Color");
    assert_eq!(
        style_at(&state.current.borrow(), 2).color.as_deref(),
        Some("#A1B2C3FF"),
        "six-digit input becomes canonical opaque RGBA"
    );
    assert_eq!(state.current.borrow().editor_text(), "color me");
    assert_eq!(state.history.borrow().undo_len(), 1);
    assert_eq!(
        style_at(&save_reload(&state.current.borrow()), 2)
            .color
            .as_deref(),
        Some("#A1B2C3FF")
    );
    app.invoke_undo();
    assert_eq!(state.current.borrow().blocks[0].runs.len(), 0);
    app.invoke_redo();
    assert_eq!(
        style_at(&state.current.borrow(), 2).color.as_deref(),
        Some("#A1B2C3FF")
    );

    let before_invalid = state.current.borrow().clone();
    let undo_count = state.history.borrow().undo_len();
    let writes_before_invalid = recovery_write_count();
    invoke_button(&app, "Text Color");
    replace_accessible_text_input(&app, "Foreground color", "#GG00FF");
    invoke_button(&app, "Apply Color");
    assert_eq!(
        *state.current.borrow(),
        before_invalid,
        "invalid color input must leave document content and styles untouched"
    );
    assert_eq!(state.history.borrow().undo_len(), undo_count);
    assert_eq!(
        recovery_write_count(),
        writes_before_invalid,
        "invalid color text must not create a recovery snapshot"
    );
}

#[test]
fn foreground_color_at_a_collapsed_caret_styles_only_new_text() {
    typing_style::clear();
    let mut document = text_document("plain");
    document.set_selection(TextSelection::caret(5));
    let (app, state) = session(document);
    render_format_inspector(&app);

    invoke_button(&app, "Text Color");
    replace_accessible_text_input(&app, "Foreground color", "#336699");
    invoke_button(&app, "Apply Color");
    assert_eq!(state.current.borrow().editor_text(), "plain");
    assert_eq!(state.history.borrow().undo_len(), 0);

    app.invoke_document_edited("plainX".into(), 6, 6);
    assert_eq!(
        style_at(&state.current.borrow(), 5).color.as_deref(),
        Some("#336699FF")
    );
    assert_eq!(
        style_at(&state.current.borrow(), 4).color.as_deref(),
        Some("#000000FF"),
        "the pending color starts at the caret instead of restyling preceding text"
    );
    let reloaded = save_reload(&state.current.borrow());
    assert_eq!(style_at(&reloaded, 5).color.as_deref(), Some("#336699FF"));
    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), "plain");
    assert_eq!(
        style_at(&state.current.borrow(), 4).color.as_deref(),
        Some("#000000FF")
    );
    typing_style::clear();
}

#[test]
fn format_inspector_exposes_real_foreground_and_script_actions() {
    let (app, _) = session(text_document("Selected text"));
    render_format_inspector(&app);

    for control in ["Text Color", "Superscript", "Subscript"] {
        assert!(
            has_accessible_role(&app, control, AccessibleRole::Button),
            "the Format inspector must expose the {control:?} action"
        );
    }
}

struct ColorTestDirectory(std::path::PathBuf);

impl ColorTestDirectory {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-writer-native-color-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create isolated color-test directory");
        Self(path)
    }
}

impl Drop for ColorTestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn document_with_invalid_native_color() -> WriterDocument {
    let mut document = text_document("original");
    document.blocks[0].runs.push(loom_text::StyleRun {
        start: 0,
        end: 8,
        style: loom_text::CharacterStyle {
            color: Some("not-a-color".into()),
            ..Default::default()
        },
    });
    document
}

fn unchecked_native_package(document: &WriterDocument) -> Vec<u8> {
    use loom_package::manifest::{
        json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
    };
    use loom_package::zip::{self, PackageArchive};

    let content = document.to_content_json();
    let mut archive = PackageArchive::new();
    archive
        .add("content/document.json", content.as_bytes().to_vec())
        .unwrap();
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Writer,
        id: document.id.clone(),
        title: document.title.clone(),
        app_version: "test".into(),
        entries: vec![ManifestEntry {
            path: "content/document.json".into(),
            mime: MimeType::parse("application/vnd.loom.document-content").unwrap(),
            size: content.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(content.as_bytes())),
        }],
    };
    archive
        .add("manifest.json", json::write(&manifest).into_bytes())
        .unwrap();
    archive.to_bytes().unwrap()
}

#[test]
fn native_color_open_refuses_bad_file_without_replacing_the_document() {
    let directory = ColorTestDirectory::new("open");
    let path = directory.0.join("invalid.loomdoc");
    std::fs::write(
        &path,
        unchecked_native_package(&document_with_invalid_native_color()),
    )
    .unwrap();
    let original = text_document("Keep the current document");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([Some(path)], []));
    let (app, state) = test_state(original.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);

    app.invoke_open_doc();

    assert_eq!(*state.current.borrow(), original);
    assert_eq!(state.history.borrow().undo_len(), 0);
    assert!(app.get_status_left().as_str().contains("Open failed"));
    assert!(app
        .get_status_left()
        .as_str()
        .to_ascii_lowercase()
        .contains("color"));
}

#[test]
fn native_color_save_refuses_bad_content_without_overwriting_the_file() {
    let directory = ColorTestDirectory::new("save");
    let path = directory.0.join("existing.loomdoc");
    let previous = b"existing file contents";
    std::fs::write(&path, previous).unwrap();
    let (app, state) = session(document_with_invalid_native_color());
    *state.save_path.borrow_mut() = Some(path.clone());
    let before = state.current.borrow().clone();

    app.invoke_save_doc();

    assert_eq!(std::fs::read(&path).unwrap(), previous);
    assert_eq!(*state.current.borrow(), before);
    assert!(app.get_status_left().as_str().contains("Save failed"));
    assert!(app
        .get_status_left()
        .as_str()
        .to_ascii_lowercase()
        .contains("color"));
}

struct NativeColorClipboardGuard;

impl Drop for NativeColorClipboardGuard {
    fn drop(&mut self) {
        loom_clipboard::set_mock_clipboard_enabled(false);
    }
}

fn colored_clipboard_table(color: &str) -> loom_objects::Table {
    let mut table = loom_objects::Table::new(1, 1);
    let mut run = loom_objects::table::TextRun::plain("pasted");
    run.color = Some(color.into());
    table.set_cell_rich_text(0, 0, vec![run]);
    table
}

#[test]
fn native_color_paste_refuses_bad_private_table_without_replacing_selection() {
    loom_clipboard::set_mock_clipboard_enabled(true);
    let _clipboard = NativeColorClipboardGuard;
    let mut document = text_document("Keep selected text");
    document.set_selection(TextSelection::range(0, 4));
    let (app, state) = session(document.clone());
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData::from_table(
        &colored_clipboard_table("not-a-color"),
    ))
    .unwrap();

    app.invoke_paste();

    assert_eq!(*state.current.borrow(), document);
    assert_eq!(state.history.borrow().undo_len(), 0);
    assert!(app.get_status_left().as_str().contains("Paste failed"));
    assert!(app
        .get_status_left()
        .as_str()
        .to_ascii_lowercase()
        .contains("color"));
}

#[test]
fn native_color_paste_preserves_valid_rgb_and_rgba_runs() {
    loom_clipboard::set_mock_clipboard_enabled(true);
    let _clipboard = NativeColorClipboardGuard;
    for color in ["#123456", "#aBcDef80"] {
        let (app, state) = session(text_document("body"));
        loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData::from_table(
            &colored_clipboard_table(color),
        ))
        .unwrap();

        app.invoke_paste();

        let saved = save_reload(&state.current.borrow());
        let table = saved
            .blocks
            .iter()
            .find_map(|block| block.typed_table())
            .expect("pasted table");
        assert_eq!(
            table.cell(0, 0).unwrap().content.runs[0].color.as_deref(),
            Some(color)
        );
    }
}
