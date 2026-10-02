//! Export to Microsoft Word (`.docx`).
//!
//! A Writer document becomes a WordprocessingML package that Word opens
//! without a repair prompt: real heading styles (so the navigation pane
//! works), real numbering for lists, real tables, a page-setup section,
//! core properties and Word comments. The output is deterministic: no
//! timestamps or random ids are written.

mod body;
mod comments;
mod package;
mod xml;

use loom_package::zip::{ArchiveError, PackageArchive};

use crate::WriterDocument;
use comments::CommentPlan;

/// The finished file plus what the export could not carry over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxExport {
    /// The `.docx` package bytes.
    pub bytes: Vec<u8>,
    /// Comment threads written as Word comments.
    pub comments_exported: usize,
    /// Comment threads left out because their text was deleted or they sit
    /// on a table, which Word anchors to cells rather than Markdown source.
    pub comments_skipped: usize,
}

/// Exports `doc` as a Word document.
pub fn export_docx(doc: &WriterDocument) -> Result<DocxExport, ArchiveError> {
    let plan = CommentPlan::new(doc);
    let has_comments = plan.exported() > 0;
    let body = body::build(doc, &plan);
    let style = doc.page.page_style();

    let mut archive = PackageArchive::new();
    let mut add = |name: &str, content: String| archive.add(name, content.into_bytes());
    add("[Content_Types].xml", package::content_types(has_comments))?;
    add("_rels/.rels", package::package_rels())?;
    add("word/document.xml", body.xml)?;
    add(
        "word/_rels/document.xml.rels",
        package::document_rels(has_comments),
    )?;
    add("word/styles.xml", package::styles(&style))?;
    add(
        "word/numbering.xml",
        package::numbering(body.numbered_lists),
    )?;
    add("word/settings.xml", package::settings())?;
    if has_comments {
        add("word/comments.xml", plan.comments_xml())?;
        add("word/commentsExtended.xml", plan.comments_extended_xml())?;
    }
    add("docProps/core.xml", package::core_properties(&doc.title))?;
    add("docProps/app.xml", package::app_properties())?;

    Ok(DocxExport {
        bytes: archive.to_bytes()?,
        comments_exported: plan.exported(),
        comments_skipped: plan.skipped(),
    })
}

#[cfg(test)]
mod tests;
