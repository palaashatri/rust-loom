//! Academic, CV, and publishing templates for Loom Writer.

use loom_writer_core::{PageAppearance, PageOrientation, WriterDocument};

use super::{add_footnote_on_block, BlockKind, TemplateBlock, TemplateDescriptor, TemplateId};

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

const ACADEMIC_ESSAY_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Essay Title",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Author • Course • Term",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Introduction",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "State central thesis here.",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Conclusion",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Synthesize your findings.",
    },
];

const RESEARCH_PAPER_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Research Paper",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Abstract: Methods & results.",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Methodology",
    },
    TemplateBlock {
        kind: BlockKind::NumberedList,
        text: "Define baseline metrics",
    },
    TemplateBlock {
        kind: BlockKind::NumberedList,
        text: "Evaluate test outcomes",
    },
    TemplateBlock {
        kind: BlockKind::Heading3,
        text: "References",
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

const BOOK_CHAPTER_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Chapter One",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Opening Scene",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Begin the narrative here.",
    },
    TemplateBlock {
        kind: BlockKind::Heading3,
        text: "Scene Break",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Continue with steady pace.",
    },
];

pub(super) const CV: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::Cv,
    stable_id: "cv",
    display_name: "Curriculum Vitae",
    document_title: "Untitled CV",
    category: 4,
    preview_kind: "cv",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: CV_BLOCKS,
};

pub(super) const ACADEMIC_ESSAY: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::AcademicEssay,
    stable_id: "academic-essay",
    display_name: "Academic Essay",
    document_title: "Untitled Academic Essay",
    category: 4,
    preview_kind: "academic-essay",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: ACADEMIC_ESSAY_BLOCKS,
};

pub(super) const RESEARCH_PAPER: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::ResearchPaper,
    stable_id: "research-paper",
    display_name: "Research Paper",
    document_title: "Untitled Research Paper",
    category: 4,
    preview_kind: "research-paper",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: RESEARCH_PAPER_BLOCKS,
};

pub(super) const NEWSLETTER: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::Newsletter,
    stable_id: "newsletter",
    display_name: "Newsletter",
    document_title: "Untitled Newsletter",
    category: 5,
    preview_kind: "newsletter",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: NEWSLETTER_BLOCKS,
};

pub(super) const BOOK_CHAPTER: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::BookChapter,
    stable_id: "book-chapter",
    display_name: "Book Chapter",
    document_title: "Untitled Book Chapter",
    category: 5,
    preview_kind: "book-chapter",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: BOOK_CHAPTER_BLOCKS,
};

/// Enrich academic and publishing templates with headers, footers, and footnotes.
pub(super) fn enrich(template: TemplateId, document: &mut WriterDocument) {
    match template {
        TemplateId::AcademicEssay => {
            document.header_footer.header_text = "Essay Title • Page {page}".into();
            document.header_footer.alignment = "right".into();
            document.header_footer.different_first_page = true;
            add_footnote_on_block(document, 3, "Cite primary source and page number here.");
        }
        TemplateId::ResearchPaper => {
            document.header_footer.footer_text = "Page {page} of {total}".into();
            document.header_footer.alignment = "center".into();
            add_footnote_on_block(
                document,
                1,
                "Corresponding author and dataset provenance note.",
            );
        }
        TemplateId::BookChapter => {
            document.header_footer.header_text = "Chapter One".into();
            document.header_footer.footer_text = "{page}".into();
            document.header_footer.alignment = "center".into();
            document.header_footer.different_first_page = true;
        }
        _ => {}
    }
}
