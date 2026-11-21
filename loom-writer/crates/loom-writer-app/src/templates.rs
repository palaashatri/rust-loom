//! Stable Writer template descriptors, chooser data, and document builders.

#[path = "template_catalog/mod.rs"]
mod template_catalog;

use slint::{ComponentHandle, Global, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::sync::Arc;

use crate::{
    WriterApp, WriterTemplateCatalog, WriterTemplateDescriptor, WriterTemplatePreviewBlock,
};
use loom_writer_core::PageOrientation;
#[cfg(test)]
use loom_writer_core::WriterDocument;

pub(crate) use template_catalog::{
    parse_template_id, template_document, TemplateDescriptor, TemplateId, TEMPLATE_CATALOG,
};

pub(crate) fn install_catalog(app: &WriterApp) {
    WriterTemplateCatalog::get(app).set_templates(catalog_model());
}

pub(crate) fn wire_create_template_callback(
    app: &WriterApp,
    state: &Rc<super::GuiState>,
    menu_service: Option<Arc<super::NativeMenuBar>>,
) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_create_template(move |stable_id| {
        let Some(app) = app_ref.upgrade() else { return };
        let Some(template) = parse_template_id(stable_id.as_str()) else {
            app.set_status_left(SharedString::from(format!(
                "Unknown template ID: {}",
                stable_id.as_str()
            )));
            return;
        };
        let guard = state
            .registry
            .lock()
            .unwrap()
            .invoke(&super::CommandInvocation::new(
                "file.new",
                super::InvocationSource::Toolbar,
            ));
        if guard.is_err()
            || super::request_document_replacement(
                &app,
                &state,
                super::PendingReplacement::CreateTemplate(template),
            )
        {
            return;
        }
        apply_template_document(&app, &state, template, menu_service.as_deref());
    });
}

pub(crate) fn apply_template_document(
    app: &WriterApp,
    state: &super::GuiState,
    template: TemplateId,
    menu_service: Option<&super::NativeMenuBar>,
) {
    let next = template_document(template);
    *state.last_saved.borrow_mut() = next.clone();
    *state.current.borrow_mut() = next;
    *state.save_path.borrow_mut() = None;
    state.compatibility_save.set(false);
    *state.history.borrow_mut() = super::EditorHistory::new();
    app.set_template_chooser_open(false);
    super::apply_state(app, state);
    if let Some(menu_service) = menu_service {
        super::sync_menu_state(menu_service, app, state);
    } else {
        super::refresh_writer_registry(app, state);
    }
    app.set_status_left(SharedString::from(format!(
        "Created {}",
        template.descriptor().display_name
    )));
    super::recovery::end_replaced_session(app);
}

fn orientation_label(orientation: PageOrientation) -> &'static str {
    match orientation {
        PageOrientation::Portrait => "portrait",
        PageOrientation::Landscape => "landscape",
    }
}

fn to_slint_descriptor(descriptor: &TemplateDescriptor) -> WriterTemplateDescriptor {
    WriterTemplateDescriptor {
        stable_id: SharedString::from(descriptor.stable_id),
        display_name: SharedString::from(descriptor.display_name),
        category: descriptor.category,
        preview_kind: SharedString::from(descriptor.preview_kind),
        appearance: SharedString::from(descriptor.appearance.as_str()),
        orientation: SharedString::from(orientation_label(descriptor.orientation)),
        has_table: descriptor.has_table,
        preview_blocks: ModelRc::new(VecModel::from(
            descriptor
                .blocks
                .iter()
                .map(|block| WriterTemplatePreviewBlock {
                    kind: SharedString::from(block.kind.as_str()),
                    text: SharedString::from(block.text),
                })
                .collect::<Vec<_>>(),
        )),
    }
}

fn catalog_model() -> ModelRc<WriterTemplateDescriptor> {
    ModelRc::new(VecModel::from(
        TEMPLATE_CATALOG
            .iter()
            .map(to_slint_descriptor)
            .collect::<Vec<_>>(),
    ))
}

#[cfg(test)]
pub(crate) fn blank_document() -> WriterDocument {
    template_document(TemplateId::Blank)
}

#[cfg(test)]
mod tests {
    use super::{parse_template_id, template_document, TemplateId, TEMPLATE_CATALOG};
    use loom_writer_core::{PageAppearance, PageOrientation, TABLE_BLOCK_KIND};

    #[test]
    fn catalog_ids_select_their_own_names_previews_and_generated_documents() {
        assert_eq!(TEMPLATE_CATALOG.len(), 18);
        assert_eq!(TEMPLATE_CATALOG[0].stable_id, "blank");
        assert_eq!(TEMPLATE_CATALOG[1].stable_id, "blank-black");
        assert_eq!(TEMPLATE_CATALOG[2].stable_id, "report");

        let mut reordered = TEMPLATE_CATALOG;
        reordered.reverse();

        for descriptor in reordered {
            let selected = parse_template_id(descriptor.stable_id)
                .expect("every offered template ID must resolve");
            assert_eq!(selected, descriptor.id);
            assert_eq!(selected.stable_id(), descriptor.stable_id);
            assert_eq!(descriptor.preview_kind, descriptor.stable_id);

            let document = template_document(selected);
            assert_eq!(document.id, format!("template-{}", descriptor.stable_id));
            assert_eq!(document.title, descriptor.document_title);
            assert_eq!(document.page.appearance, descriptor.appearance);
            assert_eq!(document.page.orientation, descriptor.orientation);
            let non_table_blocks: Vec<_> = document
                .blocks
                .iter()
                .filter(|block| block.kind != TABLE_BLOCK_KIND)
                .map(|b| (b.kind.as_str(), b.text.as_str()))
                .collect();
            assert_eq!(
                non_table_blocks,
                descriptor
                    .blocks
                    .iter()
                    .map(|b| (b.kind.as_str(), b.text))
                    .collect::<Vec<_>>(),
                "preview and generated document share the same ordered block descriptors"
            );
            let has_typed_table = document
                .blocks
                .iter()
                .any(|block| block.kind == TABLE_BLOCK_KIND && block.typed_table().is_some());
            assert_eq!(has_typed_table, descriptor.has_table);
        }
        assert_eq!(parse_template_id("memo"), None);
    }

    #[test]
    fn blank_black_and_landscape_preserve_distinct_page_setups() {
        let black = parse_template_id("blank-black").expect("blank-black offered");
        let black_doc = template_document(black);
        assert_eq!(black, TemplateId::BlankBlack);
        assert_eq!(black_doc.page.appearance, PageAppearance::Black);
        assert_eq!(black_doc.page.orientation, PageOrientation::Portrait);
        assert!(black_doc.blocks.is_empty());

        let landscape = parse_template_id("blank-landscape").expect("blank-landscape offered");
        let landscape_doc = template_document(landscape);
        assert_eq!(landscape, TemplateId::BlankLandscape);
        assert_eq!(landscape_doc.page.appearance, PageAppearance::White);
        assert_eq!(landscape_doc.page.orientation, PageOrientation::Landscape);
        assert!(landscape_doc.blocks.is_empty());
    }

    #[test]
    fn newsletter_is_an_ordered_single_column_story_flow() {
        let document = template_document(TemplateId::Newsletter);
        assert_eq!(document.blocks.len(), 6);
        assert_eq!(document.page, loom_writer_core::PageSetup::default());
        assert_eq!(
            document
                .blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            [
                "Newsletter title",
                "A short introduction for your readers.",
                "Story one",
                "Write the first newsletter story here.",
                "Story two",
                "Add another useful update for your readers.",
            ]
        );
    }
}
