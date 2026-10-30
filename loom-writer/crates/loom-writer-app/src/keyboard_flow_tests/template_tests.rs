use super::*;

#[test]
fn template_chooser_is_keyboard_complete_and_traps_tab() {
    let s = launched("Hello");
    let page = focus_weak(&s.app);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_create_template(move |template_id| {
        seen.borrow_mut().push(format!("create {template_id}"));
        weak.upgrade().unwrap().set_template_chooser_open(false);
    });
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_cancel_template(move || {
        seen.borrow_mut().push("cancel".into());
        weak.upgrade().unwrap().set_template_chooser_open(false);
    });

    ctrl(&s.app, "n");
    assert!(
        s.app.get_template_chooser_open(),
        "Ctrl+N opens the chooser"
    );
    assert_eq!(s.app.get_template_selected().as_str(), "blank");

    // Blank Black is now a distinct stable choice between Blank and Report.
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_template_selected().as_str(), "blank-black");
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_template_selected().as_str(), "report");
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_template_selected().as_str(), "blank-black");
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_template_selected().as_str(), "blank");
    press(&s.app, Key::LeftArrow);
    assert_eq!(
        s.app.get_template_selected().as_str(),
        "blank",
        "Left stops at the first template"
    );
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 1);
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 2);
    assert_eq!(
        s.app.get_template_selected().as_str(),
        "report",
        "Reports selects the report template"
    );
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::UpArrow);
    assert_eq!(s.app.get_template_category(), 0);

    // Tab cycles chooser -> Cancel -> Create Document -> chooser, both ways.
    let chooser = focus_weak(&s.app);
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create Document");
    tab(&s.app);
    assert!(
        same_focus(&focus_weak(&s.app), &chooser),
        "Tab wrapped inside the dialog"
    );
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create Document");
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel"], "Enter on Cancel cancels");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the page"
    );

    // Return on the chooser itself creates the selected template.
    ctrl(&s.app, "n");
    press(&s.app, Key::RightArrow);
    press(&s.app, Key::RightArrow);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "create report"]);
    assert!(same_focus(&focus_weak(&s.app), &page));

    // Escape cancels.
    ctrl(&s.app, "n");
    press(&s.app, Key::Escape);
    assert_eq!(log.borrow().last().map(String::as_str), Some("cancel"));
    assert!(same_focus(&focus_weak(&s.app), &page));
}

#[test]
fn template_catalog_drives_accessible_cards_preview_blocks_and_document_flow() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after UNIX epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "loom-writer-template-catalog-{}-{nonce}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create catalog integration test directory");

    let mut open_paths = Vec::new();
    let mut save_paths = Vec::new();
    let mut native_paths = Vec::new();
    let mut pdf_paths = Vec::new();
    let mut docx_paths = Vec::new();
    for descriptor in templates::TEMPLATE_CATALOG {
        let native_path = dir.join(format!("{}.loomdoc", descriptor.stable_id));
        let pdf_path = dir.join(format!("{}.pdf", descriptor.stable_id));
        let docx_path = dir.join(format!("{}.docx", descriptor.stable_id));
        open_paths.push(Some(native_path.clone()));
        save_paths.push(Some(native_path.clone()));
        if !descriptor.blocks.is_empty() {
            save_paths.push(Some(pdf_path.clone()));
            save_paths.push(Some(docx_path.clone()));
        }
        native_paths.push(native_path);
        pdf_paths.push(pdf_path);
        docx_paths.push(docx_path);
    }
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        open_paths, save_paths,
    ));
    let s = launched_with(text_document("Hello"), dialogs);
    s.app.set_template_chooser_open(true);

    for (template_index, descriptor) in templates::TEMPLATE_CATALOG.into_iter().enumerate() {
        s.app.set_template_category(descriptor.category);
        s.app.set_template_selected(descriptor.stable_id.into());
        render(&s.app);
        let card = ElementHandle::find_by_accessible_label(&s.app, descriptor.display_name)
            .find(|element| {
                element.accessible_role() == Some(AccessibleRole::Button)
                    && element.size().width > 0.0
                    && element.size().height > 0.0
            })
            .unwrap_or_else(|| panic!("visible catalog card {}", descriptor.stable_id));
        let expected_card_description = format!("Preview: {}", descriptor.preview_kind);
        assert_eq!(
            card.accessible_description()
                .map(|value| value.to_string())
                .as_deref(),
            Some(expected_card_description.as_str()),
            "preview selection must come from the same descriptor as its name"
        );

        let preview_name = format!("{} preview", descriptor.display_name);
        let preview = ElementHandle::find_by_accessible_label(&s.app, &preview_name)
            .find(|element| element.accessible_role() == Some(AccessibleRole::Groupbox))
            .unwrap_or_else(|| panic!("accessible preview for {}", descriptor.stable_id));
        assert_eq!(
            preview
                .accessible_description()
                .map(|value| value.to_string())
                .as_deref(),
            Some(descriptor.preview_kind),
            "the rendered preview surface must expose the catalog's preview kind"
        );
        let preview_origin = preview.absolute_position();
        let preview_size = preview.size();
        let mut previous_y = f32::NEG_INFINITY;
        let mut preview_x: Option<f32> = None;
        for block in descriptor.blocks {
            let text = ElementHandle::find_by_accessible_label(&s.app, block.text)
                .find(|element| {
                    if element.accessible_role() != Some(AccessibleRole::Text) {
                        return false;
                    }
                    let position = element.absolute_position();
                    let size = element.size();
                    size.width > 0.0
                        && size.height > 0.0
                        && position.x >= preview_origin.x
                        && position.x + size.width <= preview_origin.x + preview_size.width + 1.0
                        && position.y >= preview_origin.y
                        && position.y + size.height <= preview_origin.y + preview_size.height + 1.0
                })
                .unwrap_or_else(|| {
                    panic!(
                        "preview for {} contains {:?}",
                        descriptor.stable_id, block.text
                    )
                });
            let position = text.absolute_position();
            if let Some(x) = preview_x {
                assert!((position.x - x).abs() < 1.0, "preview stays in one column");
            } else {
                preview_x = Some(position.x);
            }
            assert!(
                position.y > previous_y,
                "preview follows document block order"
            );
            previous_y = position.y;
        }

        let generated = templates::template_document(descriptor.id);
        assert_eq!(generated.id, format!("template-{}", descriptor.stable_id));
        assert_eq!(generated.title, descriptor.document_title);
        let mut expected_page = loom_writer_core::PageSetup::default();
        expected_page.appearance = descriptor.appearance;
        assert_eq!(generated.page, expected_page);
        assert_eq!(
            generated
                .blocks
                .iter()
                .map(|block| (block.kind.as_str(), block.text.as_str()))
                .collect::<Vec<_>>(),
            descriptor
                .blocks
                .iter()
                .map(|block| (block.kind.as_str(), block.text))
                .collect::<Vec<_>>(),
            "generated content must match the catalog data rendered in the preview"
        );

        let flow = generated
            .flow(&generated.page.page_style(), PageViewport::default())
            .expect("every offered template has valid page geometry");
        let mut previous_block_y = f32::NEG_INFINITY;
        let mut document_x: Option<f32> = None;
        for block_index in 0..generated.blocks.len() {
            let line = flow
                .lines()
                .iter()
                .find(|line| line.block_index == block_index)
                .unwrap_or_else(|| panic!("generated block {block_index} has layout geometry"));
            if let Some(x) = document_x {
                assert!(
                    (line.bounds.x - x).abs() < 1.0,
                    "document uses one text column"
                );
            } else {
                document_x = Some(line.bounds.x);
            }
            assert!(
                line.bounds.y > previous_block_y,
                "document blocks remain sequential"
            );
            previous_block_y = line.bounds.y;
        }

        // Dispatch every descriptor through the production callback and compare
        // the actual page state with the preview's shared catalog data.
        s.app
            .invoke_create_template(SharedString::from(descriptor.stable_id));
        assert!(
            document_content_equal(&s.state.current.borrow(), &generated),
            "Create Document uses the descriptor for {}",
            descriptor.stable_id
        );
        assert_eq!(
            s.app.get_page_appearance().as_str(),
            descriptor.appearance.as_str(),
            "the actual page fill matches the {} preview",
            descriptor.stable_id
        );
        assert_eq!(
            s.app.get_doc_content().as_str(),
            generated.editor_text(),
            "the actual editor page has the catalog's generated content"
        );
        assert_eq!(
            s.app.get_page_width_pt(),
            generated.page.page_style().width_pt,
            "the actual page width matches the catalog document"
        );
        assert_eq!(
            s.app.get_page_height_pt(),
            generated.page.page_style().height_pt,
            "the actual page height matches the catalog document"
        );

        let expected_blocks: Vec<_> = descriptor
            .blocks
            .iter()
            .map(|block| (block.kind.as_str(), block.text))
            .collect();
        let native_path = &native_paths[template_index];
        s.app.invoke_save_doc();
        let package_bytes = std::fs::read(native_path).expect("native template package was saved");
        let archive = loom_package::zip::PackageArchive::from_bytes(&package_bytes)
            .expect("saved native package is a valid archive");
        assert!(
            archive.get("manifest.json").is_some(),
            "{} package has a manifest",
            descriptor.stable_id
        );
        assert!(
            archive.get("content/document.json").is_some(),
            "{} package has document content",
            descriptor.stable_id
        );
        let package_document = loom_writer_core::load_document(&package_bytes)
            .expect("saved native package validates and decodes");
        assert!(
            document_content_equal(&package_document, &generated),
            "{} package preserves template content, styles, page setup, and appearance",
            descriptor.stable_id
        );
        assert_eq!(
            package_document
                .blocks
                .iter()
                .map(|block| (block.kind.as_str(), block.text.as_str()))
                .collect::<Vec<_>>(),
            expected_blocks,
            "{} package preserves the preview's substantive blocks and heading structure",
            descriptor.stable_id
        );

        s.app.invoke_open_doc();
        assert!(
            document_content_equal(&s.state.current.borrow(), &generated),
            "{} reopens with the template content and styles",
            descriptor.stable_id
        );
        assert_eq!(
            s.app.get_page_appearance().as_str(),
            descriptor.appearance.as_str(),
            "{} retains its preview page appearance after reopen",
            descriptor.stable_id
        );

        if descriptor.blocks.is_empty() {
            // Blank templates are intentionally export-disabled by the app's
            // command registry until the user adds substantive content.
            s.app.invoke_export_pdf();
            assert_eq!(
                s.app.get_status_right().as_str(),
                "Add content before exporting PDF"
            );
            assert!(
                !pdf_paths[template_index].exists(),
                "{} has no PDF because it has no content",
                descriptor.stable_id
            );
            s.app.invoke_export_docx();
            assert_eq!(
                s.app.get_status_right().as_str(),
                "Add content before exporting a Word document"
            );
            assert!(
                !docx_paths[template_index].exists(),
                "{} has no DOCX because it has no content",
                descriptor.stable_id
            );
        } else {
            let review = s.app.global::<WriterReviewState>();
            s.app.invoke_export_pdf();
            assert!(
                review.get_pdf_export_open(),
                "{} opens the PDF output options",
                descriptor.stable_id
            );
            review.invoke_export_pdf_view(0);
            let pdf_bytes =
                std::fs::read(&pdf_paths[template_index]).expect("PDF export was written");
            let pages = loom_pdf::inspect::page_text(&pdf_bytes)
                .expect("independent PDF extractor reads exported content");
            assert_eq!(
                pages.len(),
                1,
                "{} fits on one PDF page",
                descriptor.stable_id
            );
            let readable_pdf =
                loom_pdf::inspect::readable_content(&pdf_bytes).expect("inspect PDF text style");
            for block in descriptor.blocks {
                let size = generated
                    .page
                    .page_style()
                    .font_size_for_kind(block.kind.as_str());
                let font_size_operator = format!(" {size:.2} Tf");
                assert!(
                    readable_pdf.contains(&font_size_operator),
                    "{} PDF preserves the {} style at {size:.2} pt",
                    descriptor.stable_id,
                    block.kind.as_str()
                );
            }
            let pdf_text = pages.join("\n");
            let mut text_cursor = 0;
            for block in descriptor.blocks {
                let relative = pdf_text[text_cursor..].find(block.text).unwrap_or_else(|| {
                    panic!(
                        "{} PDF is missing or reorders content {:?}: {pdf_text}",
                        descriptor.stable_id, block.text
                    )
                });
                text_cursor += relative + block.text.len();
            }

            s.app.invoke_export_docx();
            let docx_bytes =
                std::fs::read(&docx_paths[template_index]).expect("Word export was written");
            let docx_blocks = loom_writer_core::extract_docx_blocks(&docx_bytes)
                .expect("independent DOCX importer reads exported content");
            assert_eq!(
                docx_blocks
                    .iter()
                    .map(|block| (block.kind.as_str(), block.text.as_str()))
                    .collect::<Vec<_>>(),
                expected_blocks,
                "{} DOCX preserves ordered text and paragraph heading styles",
                descriptor.stable_id
            );
        }

        assert!(
            document_content_equal(&s.state.current.borrow(), &generated),
            "exports leave {} unchanged",
            descriptor.stable_id
        );
        render(&s.app);
        s.app.set_template_chooser_open(true);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn template_cards_and_create_button_dispatch_stable_ids_by_pointer() {
    let s = launched("Hello");
    let created: Rc<RefCell<Vec<String>>> = Rc::default();
    let (weak, seen) = (s.app.as_weak(), created.clone());
    s.app.on_create_template(move |template_id| {
        seen.borrow_mut().push(template_id.to_string());
        weak.upgrade().unwrap().set_template_chooser_open(false);
    });

    ctrl(&s.app, "n");
    for (name, id) in [
        ("Blank", "blank"),
        ("Executive Report", "report"),
        ("Business Letter", "letter"),
        ("Curriculum Vitae", "cv"),
        ("Newsletter", "newsletter"),
    ] {
        let card = ElementHandle::find_by_accessible_label(&s.app, name)
            .find(|element| element.size().width > 0.0 && element.size().height > 0.0)
            .unwrap_or_else(|| panic!("visible {name} template card exists"));
        let origin = card.absolute_position();
        let size = card.size();
        pointer_click(
            &s.app,
            origin.x + size.width / 2.0,
            origin.y + size.height / 2.0,
        );
        assert_eq!(s.app.get_template_selected().as_str(), id);
    }

    let create = ElementHandle::find_by_accessible_label(&s.app, "Create Document")
        .find(|element| element.size().width > 0.0 && element.size().height > 0.0)
        .expect("Create Document button is visible");
    let origin = create.absolute_position();
    let size = create.size();
    pointer_click(
        &s.app,
        origin.x + size.width / 2.0,
        origin.y + size.height / 2.0,
    );
    assert_eq!(*created.borrow(), ["newsletter"]);
}

#[test]
fn compact_double_scale_template_navigation_keeps_selected_cards_reachable() {
    for rtl in [false, true] {
        let s = launched("Hello");
        s.app.window().set_size(PhysicalSize::new(1024, 720));
        configure_direction(&s.app, rtl);
        Theme::get(&s.app).set_text_scale(2.0);
        apply_layout_breakpoints(&s.app, 1024);
        ctrl(&s.app, "n");
        let create = ElementHandle::find_by_accessible_label(&s.app, "Create Document")
            .find(|element| element.size().width > 0.0 && element.size().height > 0.0)
            .expect("Create Document remains visible");
        let footer_top = create.absolute_position().y;
        for (label, id) in [
            ("Blank Black", "blank-black"),
            ("Executive Report", "report"),
            ("Business Letter", "letter"),
            ("Curriculum Vitae", "cv"),
            ("Newsletter", "newsletter"),
        ] {
            press(&s.app, if rtl { Key::LeftArrow } else { Key::RightArrow });
            assert_eq!(s.app.get_template_selected().as_str(), id);
            let _ = snapshot_component(&s.app, 1024.0, 720.0, 1.0).expect("render compact chooser");
            let card = ElementHandle::find_by_accessible_label(&s.app, label)
                .find(|element| {
                    element.accessible_role() == Some(AccessibleRole::Button)
                        && element.size().width > 0.0
                        && element.size().height > 0.0
                })
                .unwrap_or_else(|| panic!("selected {label} card remains visible"));
            let origin = card.absolute_position();
            let size = card.size();
            assert!(
                origin.x >= 0.0
                    && origin.y >= 0.0
                    && origin.x + size.width <= 1024.0
                    && origin.y + size.height <= footer_top,
                "selected {label} stays inside the chooser viewport in rtl={rtl}: ({}, {}, {}, {})",
                origin.x,
                origin.y,
                size.width,
                size.height
            );
        }

        let backward = if rtl { Key::RightArrow } else { Key::LeftArrow };
        for _ in 0..4 {
            press(&s.app, backward);
        }
        assert_eq!(s.app.get_template_selected().as_str(), "blank-black");
        press(&s.app, Key::Return);
        assert_eq!(s.app.get_page_appearance().as_str(), "black");
    }
}

#[test]
fn production_template_callback_rejects_unknown_ids_and_guards_dirty_documents() {
    let s = launched("Original document");
    let original = s.state.current.borrow().clone();

    s.app
        .invoke_create_template(SharedString::from("not-a-template"));
    assert!(document_content_equal(&s.state.current.borrow(), &original));
    assert_eq!(
        s.app.get_status_left(),
        "Unknown template ID: not-a-template"
    );
    assert_eq!(s.state.pending_replacement.get(), None);

    let dirty = text_document("Unsaved document edit");
    *s.state.current.borrow_mut() = dirty.clone();
    apply_state(&s.app, &s.state);
    s.app.set_template_chooser_open(true);
    s.app
        .invoke_create_template(SharedString::from("newsletter"));

    assert!(document_content_equal(&s.state.current.borrow(), &dirty));
    assert!(s.app.get_document_dirty());
    assert!(s.app.get_save_changes_open());
    assert_eq!(
        s.state.pending_replacement.get(),
        Some(PendingReplacement::CreateTemplate(TemplateId::Newsletter))
    );

    // Exercise the same continuation used by the production Save/Discard
    // handlers; the selected stable ID must survive the dirty-document guard.
    s.app.set_save_changes_open(false);
    let menu = s.state.menu_service.borrow().clone();
    assert!(menu.is_some(), "launched app has its menu service");
    continue_pending_replacement(&s.app, &s.state, &menu);
    let newsletter = templates::template_document(TemplateId::Newsletter);
    assert!(document_content_equal(
        &s.state.current.borrow(),
        &newsletter
    ));
    assert!(!s.app.get_document_dirty());
    assert!(!s.app.get_template_chooser_open());
    assert_eq!(s.state.pending_replacement.get(), None);
}

#[test]
fn newsletter_template_saves_reopens_and_exports_seeded_pdf_text() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after UNIX epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "loom-writer-template-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create template test directory");
    let document_path = dir.join("newsletter.loomdoc");
    let pdf_path = dir.join("newsletter.pdf");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(document_path.clone())],
        [Some(document_path.clone()), Some(pdf_path.clone())],
    ));
    let s = launched_with(text_document("Original document"), dialogs);

    ctrl(&s.app, "n");
    let newsletter_card = ElementHandle::find_by_accessible_label(&s.app, "Newsletter")
        .find(|element| element.size().width > 0.0 && element.size().height > 0.0)
        .expect("Newsletter card is visible");
    let origin = newsletter_card.absolute_position();
    let size = newsletter_card.size();
    pointer_click(
        &s.app,
        origin.x + size.width / 2.0,
        origin.y + size.height / 2.0,
    );
    assert_eq!(s.app.get_template_selected().as_str(), "newsletter");
    let create = ElementHandle::find_by_accessible_label(&s.app, "Create Document")
        .find(|element| element.size().width > 0.0 && element.size().height > 0.0)
        .expect("Create Document button is visible");
    let origin = create.absolute_position();
    let size = create.size();
    pointer_click(
        &s.app,
        origin.x + size.width / 2.0,
        origin.y + size.height / 2.0,
    );

    let expected = templates::template_document(TemplateId::Newsletter);
    assert!(document_content_equal(&s.state.current.borrow(), &expected));

    s.app.invoke_save_doc();
    let package_bytes = std::fs::read(&document_path).expect("saved Writer package exists");
    let archive = loom_package::zip::PackageArchive::from_bytes(&package_bytes)
        .expect("saved Writer package can be independently inspected");
    assert!(archive.get("manifest.json").is_some());
    let content = std::str::from_utf8(
        archive
            .get("content/document.json")
            .expect("Writer package contains document content"),
    )
    .expect("document content is UTF-8");
    for block in expected.blocks.iter() {
        assert!(
            content.contains(block.text.as_str()),
            "saved package retains seeded text {:?}: {content}",
            block.text.as_str()
        );
    }

    s.app.invoke_open_doc();
    assert!(document_content_equal(&s.state.current.borrow(), &expected));

    s.app.invoke_export_pdf();
    s.app
        .global::<WriterReviewState>()
        .invoke_export_pdf_view(0);
    let pdf_bytes = std::fs::read(&pdf_path).expect("PDF export exists");
    let readable_pdf = loom_pdf::inspect::page_text(&pdf_bytes)
        .expect("independent PDF text extractor reads the export")
        .join("\n");
    for block in expected.blocks.iter() {
        assert!(
            readable_pdf.contains(block.text.as_str()),
            "PDF retains seeded text {:?}: {readable_pdf}",
            block.text.as_str()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
