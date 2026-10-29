//! Export to Microsoft Word (`.docx`).
//!
//! A Writer document becomes a WordprocessingML package that Word opens
//! without a repair prompt: real heading styles (so the navigation pane
//! works), real numbering for lists, real tables, a page-setup section,
//! core properties and Word comments. The output is deterministic: no
//! timestamps or random ids are written.

mod body;
mod comments;
mod comments_import;
mod footnotes;
mod header_footer;
pub mod import;
mod inline_style;
mod package;
mod paragraph_style_import;
mod xml;

pub use import::{import_docx, inspect_docx_unsupported};

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
    doc.validate_docx_revision_export()
        .map_err(ArchiveError::InvalidArgument)?;
    doc.page.validate().map_err(ArchiveError::InvalidArgument)?;
    doc.header_footer
        .validate_for_page_style(&doc.page.page_style())
        .map_err(|error| ArchiveError::InvalidArgument(error.to_string()))?;
    validate_inline_colors(doc)?;
    let plan = CommentPlan::new(doc);
    let has_comments = plan.exported() > 0;
    let footnotes = footnotes::ExportPlan::new(doc).map_err(ArchiveError::InvalidArgument)?;
    let has_footnotes = footnotes.xml.is_some();
    let header_footer = header_footer::HeaderFooterParts::new(&doc.header_footer);
    let section_references = header_footer.section_references_xml();
    let section_settings = header_footer.section_settings_xml();
    let body = body::build(
        doc,
        &plan,
        &footnotes,
        &section_references,
        &section_settings,
    )
    .map_err(ArchiveError::InvalidArgument)?;
    let chart_count = body.charts.len();
    let style = doc.page.page_style();

    let mut archive = PackageArchive::new();
    let mut add = |name: &str, content: String| archive.add(name, content.into_bytes());
    add(
        "[Content_Types].xml",
        package::content_types(
            has_comments,
            has_footnotes,
            chart_count,
            &body.images,
            &header_footer,
        ),
    )?;
    add("_rels/.rels", package::package_rels())?;
    add("word/document.xml", body.xml)?;
    add(
        "word/_rels/document.xml.rels",
        package::document_rels(
            has_comments,
            has_footnotes,
            chart_count,
            &body.images,
            &header_footer,
        ),
    )?;
    add(
        "word/styles.xml",
        package::styles(&style, doc.page.appearance),
    )?;
    add(
        "word/numbering.xml",
        package::numbering(body.numbered_lists),
    )?;
    add(
        "word/settings.xml",
        package::settings(doc.tracked_changes.recording()),
    )?;
    if has_comments {
        add("word/comments.xml", plan.comments_xml())?;
        add("word/commentsExtended.xml", plan.comments_extended_xml())?;
    }
    if let Some(xml) = footnotes.xml {
        add("word/footnotes.xml", xml)?;
    }
    for part in &header_footer.parts {
        add(part.path, part.xml.clone())?;
    }
    for (chart_idx, chart_xml) in &body.charts {
        add(
            &format!("word/charts/chart{chart_idx}.xml"),
            chart_xml.clone(),
        )?;
    }
    add("docProps/core.xml", package::core_properties(&doc.title))?;
    add("docProps/app.xml", package::app_properties())?;
    for image in &body.images {
        archive.add(
            &format!("word/media/image{}.{}", image.index, image.extension),
            image.bytes.to_vec(),
        )?;
    }

    Ok(DocxExport {
        bytes: archive.to_bytes()?,
        comments_exported: plan.exported(),
        comments_skipped: plan.skipped(),
    })
}

fn validate_inline_colors(doc: &WriterDocument) -> Result<(), ArchiveError> {
    for run in doc.blocks.iter().flat_map(|block| block.runs.iter()) {
        if let Some(color) = run.style.color.as_deref() {
            validate_inline_color(color)?;
        }
    }
    for cell in doc
        .blocks
        .iter()
        .filter_map(|block| block.table.as_deref())
        .flat_map(|table| table.rows.iter())
        .flat_map(|row| row.cells.iter())
    {
        for run in &cell.content.runs {
            if let Some(color) = run.color.as_deref() {
                validate_inline_color(color)?;
            }
        }
    }
    Ok(())
}

fn validate_inline_color(color: &str) -> Result<(), ArchiveError> {
    if inline_style::docx_rgb(color).is_none() {
        return Err(ArchiveError::Corrupt(
            "invalid inline color; DOCX requires opaque #RRGGBB or #RRGGBBFF".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod footnotes_tests;
#[cfg(test)]
mod inline_styles_tests;
#[cfg(test)]
mod revision_tests;
#[cfg(test)]
mod table_run_tests;
#[cfg(test)]
mod tests;
