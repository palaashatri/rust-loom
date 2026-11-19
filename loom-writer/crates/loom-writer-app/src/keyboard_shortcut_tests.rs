//! Integration tests for Writer's unified keyboard shortcuts, modal shortcut guards,
//! menu/toolbar/palette dispatch parity, shortcut search, and scaled/RTL modal bounds.

use std::cell::Cell;
use std::rc::Rc;

use loom_desktop::ScriptedFileDialogs;
use loom_text::Alignment;
use loom_writer_core::{RichBlock, TextSelection, WriterDocument};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Global, ModelRc, SharedString, VecModel};

use crate::actions_tests::{test_state, text_document};
use crate::command_dispatch::{dispatch_command, dispatch_shortcut, shortcut_for_id};
use crate::scale_surfaces_tests::rects;
use crate::{
    apply_state, build_writer_registry, local_menu, master_palette, palette_wiring, ranked_matches,
    snapshot_component, wire_palette, wire_writer_inspector_toggle, wire_writer_shared_callbacks,
    FindBar, PrintPreviewState, StartHubRecentItem, StartHubState, Theme, WelcomeGuideState,
    WordCountState, WriterApp, WriterFootnoteState, WriterFormatState, WriterHeaderFooterState,
    WriterReviewState,
};

fn tap_raw(app: &WriterApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn send_chord(app: &WriterApp, modifiers: &[Key], key: impl Into<SharedString>) {
    for modifier in modifiers {
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: SharedString::from(*modifier),
        });
    }
    tap_raw(app, key);
    for modifier in modifiers.iter().rev() {
        app.window().dispatch_event(WindowEvent::KeyReleased {
            text: SharedString::from(*modifier),
        });
    }
}

fn primary_key() -> Key {
    if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    }
}

fn setup_app(doc: WriterDocument) -> (WriterApp, Rc<crate::GuiState>) {
    let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(doc, dialogs);
    app.set_shortcut_modifier(crate::command_dispatch::PRIMARY_MOD.into());
    wire_writer_shared_callbacks(&app, &state, None);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_palette(&app);
    palette_wiring::wire(&app, &state);
    apply_state(&app, &state);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("initial render");
    app.invoke_focus_page();
    (app, state)
}

#[test]
fn welcome_guide_start_hub_print_preview_word_count_and_palette_shortcuts() {
    let (app, _state) = setup_app(text_document("Alpha beta gamma delta."));
    let m = primary_key();

    // F1 (bare) opens Welcome Guide
    let f1: SharedString = Key::F1.into();
    tap_raw(&app, f1);
    assert!(
        app.global::<WelcomeGuideState>().get_open(),
        "F1 opens Welcome Guide"
    );
    app.global::<WelcomeGuideState>().invoke_close_guide();
    assert!(!app.global::<WelcomeGuideState>().get_open());

    // Primary+Shift+/ opens Welcome Guide
    send_chord(&app, &[m, Key::Shift], "/");
    assert!(
        app.global::<WelcomeGuideState>().get_open(),
        "Cmd/Ctrl+Shift+/ opens Welcome Guide"
    );
    app.global::<WelcomeGuideState>().invoke_close_guide();

    // Primary+Shift+O opens Start Hub
    send_chord(&app, &[m, Key::Shift], "O");
    assert!(
        app.global::<StartHubState>().get_open(),
        "Start Hub shortcut opens modal"
    );
    app.global::<StartHubState>().invoke_close_hub();

    // Primary+P opens Print Preview
    send_chord(&app, &[m], "p");
    assert!(
        app.global::<PrintPreviewState>().get_open(),
        "Print Preview shortcut opens modal"
    );
    app.global::<PrintPreviewState>().invoke_close_preview();

    // Primary+Shift+C opens Word Count with live document counts
    send_chord(&app, &[m, Key::Shift], "C");
    assert!(
        app.global::<WordCountState>().get_open(),
        "Word Count shortcut opens modal"
    );
    assert_eq!(app.global::<WordCountState>().get_words(), 4);
    app.global::<WordCountState>().invoke_close_dialog();

    // Primary+Shift+P opens Command Palette
    send_chord(&app, &[m, Key::Shift], "P");
    assert!(app.get_palette_open(), "Cmd/Ctrl+Shift+P opens Palette");
    app.invoke_palette_close();

    // Primary+Shift+T opens Review drawer
    send_chord(&app, &[m, Key::Shift], "T");
    assert!(app.global::<WriterReviewState>().get_panel_open());
    app.global::<WriterReviewState>().set_panel_open(false);
}

#[test]
fn formatting_headings_alignment_lists_zoom_and_dialogs_respond_to_shortcuts() {
    let (app, state) = setup_app(text_document(
        "First paragraph with enough words for justify test.",
    ));
    app.invoke_selection_changed(0, 5);
    let m = primary_key();

    // Primary+Shift+X toggles strikethrough
    send_chord(&app, &[m, Key::Shift], "X");
    assert!(
        app.get_is_strikethrough(),
        "Cmd/Ctrl+Shift+X toggles strikethrough"
    );

    // Superscript & Subscript shortcuts
    send_chord(&app, &[m, Key::Shift], ">");
    assert!(app.global::<WriterFormatState>().get_superscript_selected());
    send_chord(&app, &[m, Key::Shift], "<");
    assert!(app.global::<WriterFormatState>().get_subscript_selected());

    // Headings Primary+Alt+1 / 2 / 3 / 0
    for (digit, expected_kind) in [
        ("1", "heading1"),
        ("2", "heading2"),
        ("3", "heading3"),
        ("0", "paragraph"),
    ] {
        send_chord(&app, &[m, Key::Alt], digit);
        assert_eq!(state.current.borrow().blocks[0].kind, expected_kind);
    }

    // Alignment Primary+Shift+E / R / L
    send_chord(&app, &[m, Key::Shift], "E");
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        Alignment::Center
    );
    send_chord(&app, &[m, Key::Shift], "R");
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        Alignment::Right
    );
    send_chord(&app, &[m, Key::Shift], "L");
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        Alignment::Left
    );

    // Lists Primary+Shift+7 / 8
    send_chord(&app, &[m, Key::Shift], "7");
    assert_eq!(app.get_list_style_index(), 2);
    send_chord(&app, &[m, Key::Shift], "8");
    assert_eq!(app.get_list_style_index(), 1);

    // Zoom Primary+= / - / 0
    let before_zoom = app.get_page_zoom();
    send_chord(&app, &[m], "=");
    assert!(app.get_page_zoom() > before_zoom);
    send_chord(&app, &[m], "-");
    assert!((app.get_page_zoom() - before_zoom).abs() < 0.01);
    app.invoke_page_zoom_changed(1.5);
    send_chord(&app, &[m], "0");
    assert!((app.get_page_zoom() - 1.0).abs() < 0.01);

    // Inspector & Navigator Primary+Alt+I / O
    let nav_before = app.get_show_navigator();
    send_chord(&app, &[m, Key::Alt], "o");
    assert_ne!(app.get_show_navigator(), nav_before);
    let insp_before = app.get_show_inspector();
    send_chord(&app, &[m, Key::Alt], "i");
    assert_ne!(app.get_show_inspector(), insp_before);

    // Header/Footer & Footnotes Primary+Alt+H / F
    send_chord(&app, &[m, Key::Alt], "h");
    assert!(app.global::<WriterHeaderFooterState>().get_dialog_open());
    app.global::<WriterHeaderFooterState>().invoke_cancel();
    send_chord(&app, &[m, Key::Alt], "f");
    assert!(app.global::<WriterFootnoteState>().get_dialog_open());
    app.global::<WriterFootnoteState>().invoke_cancel();

    // Undo / Ctrl+Y redo
    let edited_blocks = state.current.borrow().blocks[0].text.clone();
    app.invoke_undo();
    app.invoke_shortcut_pressed("y".into(), false, false);
    assert_eq!(state.current.borrow().blocks[0].text, edited_blocks);

    // Find & Find+Replace
    send_chord(&app, &[m], "f");
    assert!(app.global::<FindBar>().get_open());
    assert!(!app.global::<FindBar>().get_show_replace());
    send_chord(&app, &[m], "h");
    assert!(app.global::<FindBar>().get_show_replace());

    // DOCX export shortcut triggers export handler
    let exports = Rc::new(Cell::new(0));
    let exports_clone = exports.clone();
    app.on_export_docx(move || exports_clone.set(exports_clone.get() + 1));
    assert!(dispatch_shortcut(&app, "D", true, false));
    assert_eq!(exports.get(), 1);
}

#[test]
fn open_modals_block_document_shortcuts_behind_them() {
    let (app, _state) = setup_app(text_document("Modal guard"));
    let m = primary_key();

    type ModalCase = (&'static str, fn(&WriterApp), fn(&WriterApp));
    let modals: &[ModalCase] = &[
        (
            "welcome",
            |a| a.global::<WelcomeGuideState>().set_open(true),
            |a| a.global::<WelcomeGuideState>().set_open(false),
        ),
        (
            "start-hub",
            |a| a.global::<StartHubState>().set_open(true),
            |a| a.global::<StartHubState>().set_open(false),
        ),
        (
            "print-preview",
            |a| a.global::<PrintPreviewState>().set_open(true),
            |a| a.global::<PrintPreviewState>().set_open(false),
        ),
        (
            "word-count",
            |a| a.global::<WordCountState>().set_open(true),
            |a| a.global::<WordCountState>().set_open(false),
        ),
        (
            "template-chooser",
            |a| a.set_template_chooser_open(true),
            |a| a.set_template_chooser_open(false),
        ),
    ];

    for (name, open_fn, close_fn) in modals {
        open_fn(&app);
        let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render with modal");
        app.set_palette_open(false);
        send_chord(&app, &[m], "k");
        assert!(
            !app.get_palette_open(),
            "{name} modal must block Cmd/Ctrl+K behind it"
        );
        send_chord(&app, &[m], "f");
        assert!(
            !app.global::<FindBar>().get_open(),
            "{name} modal must block Cmd/Ctrl+F behind it"
        );
        send_chord(&app, &[Key::Alt], "f");
        assert_eq!(
            app.get_local_menu_open_index(),
            -1,
            "{name} modal must block Alt+F menu opening"
        );
        close_fn(&app);
    }
}

#[test]
fn palette_shortcut_search_and_supported_command_dispatch_parity() {
    let (app, _state) = setup_app(text_document("Search target"));
    let mut registry = build_writer_registry();
    for spec in &mut registry.commands().cloned().collect::<Vec<_>>() {
        let _ = registry.set_enabled(&spec.id, true);
    }

    for (query, expected_id) in [
        ("ctrl+p", "file.print_preview"),
        ("cmd+p", "file.print_preview"),
        ("ctrl+shift+c", "review.word_count"),
        ("ctrl+shift+o", "file.start_hub"),
        ("templates", "file.templates"),
        ("ctrl+shift+/", "help.welcome"),
        ("f1", "help.welcome"),
    ] {
        let ids: Vec<String> = ranked_matches(&registry, query)
            .into_iter()
            .map(|s| s.id.as_str().to_string())
            .collect();
        assert!(
            ids.contains(&expected_id.to_string()),
            "query {query:?} should match {expected_id}, got {ids:?}"
        );
    }

    let palette = master_palette(&app);
    assert_eq!(
        palette
            .iter()
            .find(|c| c.id == "writer.export-docx")
            .expect("writer.export-docx in palette")
            .shortcut,
        shortcut_for_id("writer.export-docx")
    );
    assert_eq!(
        palette
            .iter()
            .find(|c| c.id == "file.print_preview")
            .expect("print_preview in palette")
            .shortcut,
        shortcut_for_id("file.print_preview")
    );

    let quits = Rc::new(Cell::new(0));
    let dialogs: Rc<dyn crate::FileDialogService> =
        Rc::new(ScriptedFileDialogs::new([None], [None, None, None, None]));
    let (dispatch_app, dispatch_state) = test_state(text_document("Dispatch test"), dialogs);
    wire_writer_shared_callbacks(&dispatch_app, &dispatch_state, None);
    let q = quits.clone();
    dispatch_app.on_application_quit_requested(move || q.set(q.get() + 1));

    for cmd in local_menu::SUPPORTED_COMMANDS {
        if *cmd == "app.quit" || *cmd == "loom writer.quit" {
            continue;
        }
        assert!(
            registry.get(&loom_command::CommandId::new(*cmd)).is_some(),
            "menu command {cmd} missing from Writer CommandRegistry catalog"
        );
        assert!(
            dispatch_command(&dispatch_app, cmd),
            "menu command {cmd} must dispatch cleanly"
        );
        dispatch_app.set_template_chooser_open(false);
        dispatch_app.global::<StartHubState>().set_open(false);
        dispatch_app.global::<WelcomeGuideState>().set_open(false);
        dispatch_app.global::<PrintPreviewState>().set_open(false);
        dispatch_app.global::<WordCountState>().set_open(false);
        dispatch_app
            .global::<WriterHeaderFooterState>()
            .set_dialog_open(false);
        dispatch_app
            .global::<WriterFootnoteState>()
            .set_dialog_open(false);
    }
    assert_eq!(quits.get(), 0);
}

#[test]
fn new_modals_stay_in_bounds_and_readable_across_scales_and_rtl() {
    let mut doc = WriterDocument::new("modal-layout", "Quarterly Summary");
    for i in 0..12 {
        let id = doc.next_id();
        doc.push(RichBlock::new(
            id,
            "paragraph",
            &format!("Section {i}: Structured document content for responsive modal verification."),
        ));
    }
    doc.set_selection(TextSelection::caret(0));

    let recent_items = vec![
        StartHubRecentItem {
            name: "Annual Report".into(),
            path: "/tmp/annual-report.docx".into(),
            subtitle: "Modified 5m ago".into(),
            kind: "DOCX".into(),
        },
        StartHubRecentItem {
            name: "Design Spec".into(),
            path: "/tmp/design-spec.loomdoc".into(),
            subtitle: "Modified 2h ago".into(),
            kind: "LOOM".into(),
        },
    ];

    for (w, h) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
        for scale in [1.0f32, 1.5, 2.0] {
            for rtl in [false, true] {
                for modal in ["welcome", "start-hub", "print-preview", "word-count"] {
                    let (app, state) = setup_app(doc.clone());
                    app.window()
                        .set_size(slint::PhysicalSize::new(w as u32, h as u32));
                    Theme::get(&app).set_text_scale(scale);
                    app.set_rtl(rtl);
                    match modal {
                        "welcome" => crate::welcome_guide::open(&app),
                        "start-hub" => {
                            crate::start_hub::open_hub(&app);
                            app.global::<StartHubState>()
                                .set_recents(ModelRc::new(VecModel::from(recent_items.clone())));
                        }
                        "print-preview" => crate::print_preview::open(&app, &state),
                        "word-count" => crate::word_count_dialog::open(&app, &state),
                        _ => unreachable!(),
                    }
                    let _ = snapshot_component(&app, w, h, 1.0).expect("render modal");
                    let items = rects(&app);
                    assert!(
                        items.len() > 2,
                        "{modal} at {w}x{h} scale={scale} rtl={rtl} produced no controls"
                    );
                    for (label, x, y, cw, ch) in &items {
                        assert!(
                            *x >= -1.0 && *y >= -1.0 && x + cw <= w + 1.0 && y + ch <= h + 1.0,
                            "{modal} control {label:?} ({x},{y},{cw}x{ch}) clipped outside {w}x{h} scale={scale} rtl={rtl}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cmd_and_ctrl_select_all_selects_without_typing_and_bolds() {
    for modifier in [Key::Control, Key::Meta] {
        let (app, state) = setup_app(text_document("Hello Select All"));
        app.invoke_focus_page();
        app.invoke_selection_changed(16, 16);

        send_chord(&app, &[modifier], "a");
        assert_eq!(
            state.current.borrow().editor_text(),
            "Hello Select All",
            "Cmd/Ctrl+A must not insert 'a' into the editor"
        );
        assert_eq!(app.get_selection_anchor(), 0);
        assert_eq!(app.get_selection_focus(), 16);

        send_chord(&app, &[modifier], "b");
        let doc = state.current.borrow();
        assert!(
            app.get_is_bold()
                && !doc.blocks[0].runs.is_empty()
                && doc.blocks[0]
                    .runs
                    .iter()
                    .all(|r| r.style.weight == loom_text::FontWeight::Bold),
            "Cmd/Ctrl+B after Cmd/Ctrl+A must bold the whole selection"
        );
    }
}

#[test]
fn alt_space_is_swallowed_on_ctrl_platforms_without_inserting_space_and_templates_opens() {
    let s = crate::keyboard_flow_tests::launched("Hello");
    s.app.set_shortcut_modifier("Ctrl".into());
    s.app.set_selection_anchor(5);
    s.app.set_selection_focus(5);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");

    tap_raw(&s.app, "!");
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render after !");
    assert_eq!(
        s.state.current.borrow().editor_text(),
        "Hello!",
        "Baseline typing must reach the page"
    );

    send_chord(&s.app, &[Key::Alt], " ");
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render after alt+space");
    assert_eq!(
        s.app.get_doc_content().as_str(),
        "Hello!",
        "Alt+Space must not insert a literal space into TextInput"
    );
    assert_eq!(
        s.state.current.borrow().editor_text(),
        "Hello!",
        "Alt+Space must not insert a literal space into the document"
    );

    tap_raw(&s.app, " ");
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render after plain space");
    assert_eq!(
        s.state.current.borrow().editor_text(),
        "Hello! ",
        "Plain Space must still insert normally after Alt+Space"
    );

    assert!(dispatch_command(&s.app, "file.templates"));
    assert!(
        s.app.get_save_changes_open(),
        "dirty document guards before switching to templates"
    );

    let clean = crate::keyboard_flow_tests::launched("Clean");
    assert!(dispatch_command(&clean.app, "file.templates"));
    assert!(
        clean.app.get_template_chooser_open(),
        "clean document opens template chooser directly"
    );
}
