//! Stable Writer template descriptors, chooser data, and document builders.

use slint::{ComponentHandle, Global, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::sync::Arc;

use crate::{
    WriterApp, WriterTemplateCatalog, WriterTemplateDescriptor, WriterTemplatePreviewBlock,
};
use loom_writer_core::{PageAppearance, RichBlock, WriterDocument};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateId {
    Blank,
    BlankBlack,
    Report,
    Letter,
    Cv,
    Newsletter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    Paragraph,
    Heading1,
    Heading2,
}

impl BlockKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading1 => "heading1",
            Self::Heading2 => "heading2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemplateBlock {
    pub(crate) kind: BlockKind,
    pub(crate) text: &'static str,
}

const REPORT_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Report Title",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Executive summary",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Overview",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Start writing your report here.",
    },
];
const LETTER_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Your Name",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Date",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Dear Recipient,",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Write your letter here.",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Sincerely,",
    },
];
const CV_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Your Name",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Professional title · email@example.com",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Experience",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Role — Company · 2020–Present",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Education",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Degree — Institution",
    },
];
const NEWSLETTER_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Newsletter title",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "A short introduction for your readers.",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Story one",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Write the first newsletter story here.",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Story two",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Add another useful update for your readers.",
    },
];

/// The catalog order is stable; Slint mirrors its rows for right-to-left layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemplateDescriptor {
    pub(crate) id: TemplateId,
    pub(crate) stable_id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) document_title: &'static str,
    pub(crate) category: i32,
    pub(crate) preview_kind: &'static str,
    pub(crate) appearance: PageAppearance,
    pub(crate) blocks: &'static [TemplateBlock],
}

pub(crate) static TEMPLATE_CATALOG: [TemplateDescriptor; 6] = [
    TemplateDescriptor {
        id: TemplateId::Blank,
        stable_id: "blank",
        display_name: "Blank",
        document_title: "Untitled Document",
        category: 1,
        preview_kind: "blank",
        appearance: PageAppearance::White,
        blocks: &[],
    },
    TemplateDescriptor {
        id: TemplateId::BlankBlack,
        stable_id: "blank-black",
        display_name: "Blank Black",
        document_title: "Untitled Document",
        category: 1,
        preview_kind: "blank-black",
        appearance: PageAppearance::Black,
        blocks: &[],
    },
    TemplateDescriptor {
        id: TemplateId::Report,
        stable_id: "report",
        display_name: "Executive Report",
        document_title: "Untitled Report",
        category: 2,
        preview_kind: "report",
        appearance: PageAppearance::White,
        blocks: REPORT_BLOCKS,
    },
    TemplateDescriptor {
        id: TemplateId::Letter,
        stable_id: "letter",
        display_name: "Business Letter",
        document_title: "Untitled Letter",
        category: 3,
        preview_kind: "letter",
        appearance: PageAppearance::White,
        blocks: LETTER_BLOCKS,
    },
    TemplateDescriptor {
        id: TemplateId::Cv,
        stable_id: "cv",
        display_name: "Curriculum Vitae",
        document_title: "Untitled CV",
        category: 4,
        preview_kind: "cv",
        appearance: PageAppearance::White,
        blocks: CV_BLOCKS,
    },
    TemplateDescriptor {
        id: TemplateId::Newsletter,
        stable_id: "newsletter",
        display_name: "Newsletter",
        document_title: "Untitled Newsletter",
        category: 5,
        preview_kind: "newsletter",
        appearance: PageAppearance::White,
        blocks: NEWSLETTER_BLOCKS,
    },
];

impl TemplateId {
    #[cfg(test)]
    pub(crate) const fn stable_id(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::BlankBlack => "blank-black",
            Self::Report => "report",
            Self::Letter => "letter",
            Self::Cv => "cv",
            Self::Newsletter => "newsletter",
        }
    }

    pub(crate) fn descriptor(self) -> &'static TemplateDescriptor {
        TEMPLATE_CATALOG
            .iter()
            .find(|d| d.id == self)
            .expect("catalog covers every template ID")
    }
}

pub(crate) fn parse_template_id(value: &str) -> Option<TemplateId> {
    TEMPLATE_CATALOG
        .iter()
        .find(|d| d.stable_id == value)
        .map(|d| d.id)
}

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

fn catalog_model() -> ModelRc<WriterTemplateDescriptor> {
    ModelRc::new(VecModel::from(
        TEMPLATE_CATALOG
            .iter()
            .map(|descriptor| WriterTemplateDescriptor {
                stable_id: SharedString::from(descriptor.stable_id),
                display_name: SharedString::from(descriptor.display_name),
                category: descriptor.category,
                preview_kind: SharedString::from(descriptor.preview_kind),
                appearance: SharedString::from(descriptor.appearance.as_str()),
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
            })
            .collect::<Vec<_>>(),
    ))
}

pub(crate) fn template_document(template: TemplateId) -> WriterDocument {
    let descriptor = template.descriptor();
    let mut document = WriterDocument::new(
        format!("template-{}", descriptor.stable_id),
        descriptor.document_title,
    );
    document.page.appearance = descriptor.appearance;
    for block in descriptor.blocks {
        document.push(RichBlock::new(
            document.next_id(),
            block.kind.as_str(),
            block.text,
        ));
    }
    document
}

#[cfg(test)]
pub(crate) fn blank_document() -> WriterDocument {
    template_document(TemplateId::Blank)
}

#[cfg(test)]
mod tests {
    use super::{parse_template_id, template_document, TemplateId, TEMPLATE_CATALOG};
    use loom_writer_core::PageAppearance;

    #[test]
    fn catalog_ids_select_their_own_names_previews_and_generated_documents() {
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
            assert_eq!(
                document
                    .blocks
                    .iter()
                    .map(|b| (b.kind.as_str(), b.text.as_str()))
                    .collect::<Vec<_>>(),
                descriptor
                    .blocks
                    .iter()
                    .map(|b| (b.kind.as_str(), b.text))
                    .collect::<Vec<_>>(),
                "preview and generated document share the same ordered block descriptors"
            );
        }
    }

    #[test]
    fn blank_black_is_a_stable_template_with_a_black_page() {
        let id = parse_template_id("blank-black").expect("the audited template ID remains offered");
        let descriptor = id.descriptor();
        let document = template_document(id);

        assert_eq!(id, TemplateId::BlankBlack);
        assert_eq!(descriptor.display_name, "Blank Black");
        assert_eq!(descriptor.appearance, PageAppearance::Black);
        assert!(descriptor.blocks.is_empty());
        assert_eq!(document.id, "template-blank-black");
        assert_eq!(document.page.appearance, PageAppearance::Black);
        assert!(document.blocks.is_empty());
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
