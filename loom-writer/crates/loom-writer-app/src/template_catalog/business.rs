//! Business, report, and correspondence templates for Loom Writer.

use loom_writer_core::{PageAppearance, PageOrientation, WriterDocument};

use super::{push_table_block, BlockKind, TemplateBlock, TemplateDescriptor, TemplateId};

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

const PROPOSAL_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Project Proposal",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Stakeholders • Q4",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Scope & Goals",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Core platform milestones",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Quarterly budget target",
    },
];

const MEETING_MINUTES_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Meeting Minutes",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Monday • Team Lead",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Agenda",
    },
    TemplateBlock {
        kind: BlockKind::NumberedList,
        text: "Review sprint deliverables",
    },
    TemplateBlock {
        kind: BlockKind::NumberedList,
        text: "Confirm release owners",
    },
];

const INVOICE_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Service Invoice",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Invoice #1042 • Net 30",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Billing Summary",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Client: Northwind Partners",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Payment due in 30 days.",
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

const COVER_LETTER_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Your Full Name",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Role Application • Contact",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Dear Hiring Committee,",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "I am eager to apply here.",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "Warm regards,",
    },
];

const COMPANY_MEMO_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Memorandum",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "To: All Teams • From: Ops",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Key Updates",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Schedule begins Monday",
    },
    TemplateBlock {
        kind: BlockKind::BulletList,
        text: "Send feedback by Friday",
    },
];

const PRESS_RELEASE_BLOCKS: &[TemplateBlock] = &[
    TemplateBlock {
        kind: BlockKind::Heading1,
        text: "Press Release",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "FOR IMMEDIATE RELEASE",
    },
    TemplateBlock {
        kind: BlockKind::Heading2,
        text: "Summary",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "City — Milestone overview.",
    },
    TemplateBlock {
        kind: BlockKind::Heading3,
        text: "Media Contact",
    },
    TemplateBlock {
        kind: BlockKind::Paragraph,
        text: "press@example.com",
    },
];

pub(super) const REPORT: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::Report,
    stable_id: "report",
    display_name: "Executive Report",
    document_title: "Untitled Report",
    category: 2,
    preview_kind: "report",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: REPORT_BLOCKS,
};

pub(super) const PROJECT_PROPOSAL: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::ProjectProposal,
    stable_id: "project-proposal",
    display_name: "Project Proposal",
    document_title: "Untitled Project Proposal",
    category: 2,
    preview_kind: "project-proposal",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: true,
    blocks: PROPOSAL_BLOCKS,
};

pub(super) const MEETING_NOTES: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::MeetingNotes,
    stable_id: "meeting-notes",
    display_name: "Meeting Minutes",
    document_title: "Untitled Meeting Minutes",
    category: 2,
    preview_kind: "meeting-notes",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: true,
    blocks: MEETING_MINUTES_BLOCKS,
};

pub(super) const INVOICE_DOC: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::InvoiceDoc,
    stable_id: "invoice-doc",
    display_name: "Service Invoice",
    document_title: "Untitled Service Invoice",
    category: 2,
    preview_kind: "invoice-doc",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: true,
    blocks: INVOICE_BLOCKS,
};

pub(super) const LETTER: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::Letter,
    stable_id: "letter",
    display_name: "Business Letter",
    document_title: "Untitled Letter",
    category: 3,
    preview_kind: "letter",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: LETTER_BLOCKS,
};

pub(super) const COVER_LETTER: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::CoverLetter,
    stable_id: "cover-letter",
    display_name: "Cover Letter",
    document_title: "Untitled Cover Letter",
    category: 3,
    preview_kind: "cover-letter",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: COVER_LETTER_BLOCKS,
};

pub(super) const COMPANY_MEMO: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::CompanyMemo,
    stable_id: "company-memo",
    display_name: "Internal Memorandum",
    document_title: "Untitled Memorandum",
    category: 3,
    preview_kind: "company-memo",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: COMPANY_MEMO_BLOCKS,
};

pub(super) const PRESS_RELEASE: TemplateDescriptor = TemplateDescriptor {
    id: TemplateId::PressRelease,
    stable_id: "press-release",
    display_name: "Press Release",
    document_title: "Untitled Press Release",
    category: 3,
    preview_kind: "press-release",
    appearance: PageAppearance::White,
    orientation: PageOrientation::Portrait,
    has_table: false,
    blocks: PRESS_RELEASE_BLOCKS,
};

/// Enrich business templates with tables and headers/footers after base blocks.
pub(super) fn enrich(template: TemplateId, document: &mut WriterDocument) {
    match template {
        TemplateId::ProjectProposal => {
            document.header_footer.footer_text = "Project Proposal • Page {page} of {total}".into();
            push_table_block(
                document,
                "proposal-milestones",
                &[
                    &["Phase", "Deliverable", "Target"],
                    &["Phase 1", "Discovery", "Week 2"],
                    &["Phase 2", "Delivery", "Week 6"],
                ],
            );
        }
        TemplateId::MeetingNotes => {
            push_table_block(
                document,
                "meeting-actions",
                &[
                    &["Action Item", "Owner", "Due"],
                    &["Finalize specs", "Engineering", "Friday"],
                    &["Publish summary", "Operations", "Monday"],
                ],
            );
        }
        TemplateId::InvoiceDoc => {
            document.header_footer.footer_text = "Thank you for your business • Page {page}".into();
            push_table_block(
                document,
                "invoice-items",
                &[
                    &["Service", "Hours", "Rate", "Amount"],
                    &["Design Review", "8", "$150", "$1,200"],
                    &["Architecture", "12", "$175", "$2,100"],
                    &["Total Due", "", "", "$3,300"],
                ],
            );
        }
        TemplateId::CompanyMemo => {
            document.header_footer.header_text = "INTERNAL MEMORANDUM".into();
        }
        TemplateId::PressRelease => {
            document.header_footer.footer_text = "### • Page {page} of {total}".into();
            document.header_footer.alignment = "center".into();
        }
        _ => {}
    }
}
