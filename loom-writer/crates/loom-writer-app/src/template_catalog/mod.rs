//! Shared 18-template catalog definitions and document builders.

mod academic;
mod business;
mod personal;

use loom_writer_core::{FootnoteEntry, PageAppearance, PageOrientation, RichBlock, WriterDocument};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateId {
    Blank,
    BlankBlack,
    Report,
    BlankLandscape,
    ProjectProposal,
    MeetingNotes,
    InvoiceDoc,
    Letter,
    CoverLetter,
    CompanyMemo,
    PressRelease,
    Cv,
    AcademicEssay,
    ResearchPaper,
    Newsletter,
    BookChapter,
    StudyNotes,
    TravelJournal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    BulletList,
    NumberedList,
}

impl BlockKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading1 => "heading1",
            Self::Heading2 => "heading2",
            Self::Heading3 => "heading3",
            Self::BulletList => "list-bulleted",
            Self::NumberedList => "list-numbered",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemplateBlock {
    pub(crate) kind: BlockKind,
    pub(crate) text: &'static str,
}

/// The catalog order is stable; Slint mirrors its rows for right-to-left layout.
/// Indices 0, 1, 2 remain `blank`, `blank-black`, and `report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemplateDescriptor {
    pub(crate) id: TemplateId,
    pub(crate) stable_id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) document_title: &'static str,
    pub(crate) category: i32,
    pub(crate) preview_kind: &'static str,
    pub(crate) appearance: PageAppearance,
    pub(crate) orientation: PageOrientation,
    pub(crate) has_table: bool,
    pub(crate) blocks: &'static [TemplateBlock],
}

pub(crate) static TEMPLATE_CATALOG: [TemplateDescriptor; 18] = [
    personal::BLANK,
    personal::BLANK_BLACK,
    business::REPORT,
    personal::BLANK_LANDSCAPE,
    business::PROJECT_PROPOSAL,
    business::MEETING_NOTES,
    business::INVOICE_DOC,
    business::LETTER,
    business::COVER_LETTER,
    business::COMPANY_MEMO,
    business::PRESS_RELEASE,
    academic::CV,
    academic::ACADEMIC_ESSAY,
    academic::RESEARCH_PAPER,
    academic::NEWSLETTER,
    academic::BOOK_CHAPTER,
    personal::STUDY_NOTES,
    personal::TRAVEL_JOURNAL,
];

impl TemplateId {
    #[cfg(test)]
    pub(crate) const fn stable_id(self) -> &'static str {
        self.descriptor().stable_id
    }

    pub(crate) const fn descriptor(self) -> &'static TemplateDescriptor {
        &TEMPLATE_CATALOG[self as usize]
    }
}

pub(crate) fn parse_template_id(value: &str) -> Option<TemplateId> {
    TEMPLATE_CATALOG
        .iter()
        .find(|d| d.stable_id == value)
        .map(|d| d.id)
}

pub(crate) fn template_document(template: TemplateId) -> WriterDocument {
    let descriptor = template.descriptor();
    let mut document = WriterDocument::new(
        format!("template-{}", descriptor.stable_id),
        descriptor.document_title,
    );
    document.page.appearance = descriptor.appearance;
    document.page.orientation = descriptor.orientation;
    for block in descriptor.blocks {
        document.push(RichBlock::new(
            document.next_id(),
            block.kind.as_str(),
            block.text,
        ));
    }
    business::enrich(template, &mut document);
    academic::enrich(template, &mut document);
    document
}

pub(super) fn push_table_block(document: &mut WriterDocument, id: &str, rows: &[&[&str]]) {
    let row_count = rows.len().max(1);
    let col_count = rows.first().map_or(1, |row| row.len()).max(1);
    let mut table = loom_objects::Table::new(row_count, col_count);
    table.id = id.to_string();
    for (row_index, row) in rows.iter().enumerate() {
        for (col_index, cell) in row.iter().enumerate() {
            table.set_cell_text(row_index, col_index, *cell);
        }
    }
    document.push(RichBlock::table_block(document.next_id(), table));
}

pub(super) fn add_footnote_on_block(document: &mut WriterDocument, block_index: usize, body: &str) {
    let Some(block) = document.blocks.get(block_index) else {
        return;
    };
    let next_id = document
        .footnotes
        .iter()
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0)
        + 1;
    document.footnotes.push(FootnoteEntry {
        id: next_id,
        block_id: block.id,
        offset: block.text.len_bytes(),
        body: body.to_string(),
    });
}
