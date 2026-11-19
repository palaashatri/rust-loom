//! Basic and personal templates for Loom Writer.

use loom_writer_core::{PageAppearance, PageOrientation};

use super::{BlockKind, TemplateBlock, TemplateDescriptor, TemplateId};

const STUDY_NOTES_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Course Notes",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Topic • Date",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Core Concepts",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Key concept summary",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Worked example review",
    },
    TemplateBlock {
        kind: BlockKind::Heading3,
        text: "Questions",
    },
];

const TRAVEL_JOURNAL_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Journey Notes",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Destination • Dates",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Highlights",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Morning old quarter walk",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Favorite local cafe stop",
    },
];

pub(super) const BLANK: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::Blank,
    stable_id: "blank",
    display_name: "Blank",
    document_title: "Untitled Document",
    category: 1,
    preview_kind: "blank",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: &[],
};

pub(super) const BLANK_BLACK: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::BlankBlack,
    stable_id: "blank-black",
    display_name: "Blank Black",
    document_title: "Untitled Document",
    category: 1,
    preview_kind: "blank-black",
    appearance: PageAppearance::Black,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: &[],
};

pub(super) const BLANK_LANDSCAPE: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::BlankLandscape,
    stable_id: "blank-landscape",
    display_name: "Blank Landscape",
    document_title: "Untitled Landscape Document",
    category: 1,
    preview_kind: "blank-landscape",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Landscape,
    has_table: false,
    blocks: &[],
};

pub(super) const STUDY_NOTES: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::StudyNotes,
    stable_id: "study-notes",
    display_name: "Lecture & Study Notes",
    document_title: "Untitled Study Notes",
    category: 6,
    preview_kind: "study-notes",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: STUDY_NOTES_BLOCKS,
};

pub(super) const TRAVEL_JOURNAL: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::TravelJournal,
    stable_id: "travel-journal",
    display_name: "Travel Journal",
    document_title: "Untitled Travel Journal",
    category: 6,
    preview_kind: "travel-journal",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: TRAVEL_JOURNAL_BLOCKS,
};
