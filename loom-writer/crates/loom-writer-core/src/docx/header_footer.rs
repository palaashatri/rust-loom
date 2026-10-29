//! WordprocessingML header/footer parts for export and package wiring.

#[path = "header_footer_import.rs"]
mod import;

use super::xml::{self, R_NS, W_NS, XML_DECLARATION};
use crate::{HeaderFooterConfig, PageNumberFormat};

pub(super) const HEADER_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
pub(super) const FOOTER_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
const HEADER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const FOOTER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
pub(super) const MAX_PART_XML_BYTES: usize = 32 * 1_024;

#[derive(Debug, Clone)]
pub(super) struct DocxPart {
    pub(super) path: &'static str,
    pub(super) relationship_id: &'static str,
    pub(super) relationship_type: &'static str,
    pub(super) content_type: &'static str,
    pub(super) xml: String,
}

/// Header/footer parts and the section references that use them.
#[derive(Debug, Clone)]
pub(super) struct HeaderFooterParts {
    config: HeaderFooterConfig,
    pub(super) parts: Vec<DocxPart>,
}

impl HeaderFooterParts {
    pub(super) fn new(config: &HeaderFooterConfig) -> Self {
        let mut parts = Vec::with_capacity(4);
        if !config.header_text.is_empty() {
            parts.push(part(
                "word/header1.xml",
                "rIdHeaderDefault",
                HEADER_REL,
                HEADER_CONTENT_TYPE,
                part_xml("hdr", &config.header_text, config),
            ));
        }
        if !config.footer_text.is_empty() {
            parts.push(part(
                "word/footer1.xml",
                "rIdFooterDefault",
                FOOTER_REL,
                FOOTER_CONTENT_TYPE,
                part_xml("ftr", &config.footer_text, config),
            ));
        }
        if config.different_first_page && !config.header_text.is_empty() {
            parts.push(part(
                "word/header2.xml",
                "rIdHeaderFirst",
                HEADER_REL,
                HEADER_CONTENT_TYPE,
                empty_part_xml("hdr"),
            ));
        }
        if config.different_first_page && !config.footer_text.is_empty() {
            parts.push(part(
                "word/footer2.xml",
                "rIdFooterFirst",
                FOOTER_REL,
                FOOTER_CONTENT_TYPE,
                empty_part_xml("ftr"),
            ));
        }
        Self {
            config: config.clone(),
            parts,
        }
    }

    pub(super) fn content_type_overrides(&self) -> String {
        self.parts
            .iter()
            .map(|part| {
                format!(
                    "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                    part.path, part.content_type
                )
            })
            .collect()
    }

    pub(super) fn relationships_xml(&self) -> String {
        self.parts
            .iter()
            .map(|part| {
                let target = part.path.strip_prefix("word/").unwrap_or(part.path);
                format!(
                    "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
                    part.relationship_id, part.relationship_type, target
                )
            })
            .collect()
    }

    /// Section references appear before the section page-size/page-margin properties.
    pub(super) fn section_references_xml(&self) -> String {
        let mut out = String::new();
        for (path, id, kind) in [
            ("word/header1.xml", "rIdHeaderDefault", "default"),
            ("word/header2.xml", "rIdHeaderFirst", "first"),
            ("word/footer1.xml", "rIdFooterDefault", "default"),
            ("word/footer2.xml", "rIdFooterFirst", "first"),
        ] {
            if self.parts.iter().any(|part| part.path == path) {
                let tag = if path.contains("header") {
                    "header"
                } else {
                    "footer"
                };
                out.push_str(&format!(
                    "<w:{tag}Reference w:type=\"{kind}\" r:id=\"{id}\"/>"
                ));
            }
        }
        out
    }

    /// Page-number formatting and first-page suppression follow page margins.
    pub(super) fn section_settings_xml(&self) -> String {
        let mut out = format!(
            "<w:pgNumType w:fmt=\"{}\"/>",
            page_number_format_name(self.config.page_number_format)
        );
        if self.config.different_first_page {
            out.push_str("<w:titlePg/>");
        }
        out
    }
}

fn part(
    path: &'static str,
    relationship_id: &'static str,
    relationship_type: &'static str,
    content_type: &'static str,
    xml: String,
) -> DocxPart {
    DocxPart {
        path,
        relationship_id,
        relationship_type,
        content_type,
        xml,
    }
}

fn empty_part_xml(root: &str) -> String {
    format!("{XML_DECLARATION}<w:{root} xmlns:w=\"{W_NS}\"><w:p/></w:{root}>")
}

fn part_xml(root: &str, text: &str, config: &HeaderFooterConfig) -> String {
    let mut out = format!("{XML_DECLARATION}<w:{root} xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\"><w:p>");
    if config.effective_alignment() != "left" {
        out.push_str(&format!(
            "<w:pPr><w:jc w:val=\"{}\"/></w:pPr>",
            xml::attr(config.effective_alignment())
        ));
    }
    push_template(&mut out, text, config.page_number_format);
    out.push_str(&format!("</w:p></w:{root}>"));
    out
}

fn push_template(out: &mut String, template: &str, format: PageNumberFormat) {
    let mut rest = template;
    loop {
        let page = rest.find("{page}").map(|at| (at, "{page}"));
        let total = rest.find("{total}").map(|at| (at, "{total}"));
        let next = match (page, total) {
            (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
        let Some((at, token)) = next else {
            push_text_run(out, rest);
            break;
        };
        push_text_run(out, &rest[..at]);
        push_field(
            out,
            if token == "{page}" {
                "PAGE"
            } else {
                "NUMPAGES"
            },
            format,
        );
        rest = &rest[at + token.len()..];
    }
}

fn push_text_run(out: &mut String, text: &str) {
    if !text.is_empty() {
        out.push_str("<w:r>");
        let _ = xml::push_run_children(out, text);
        out.push_str("</w:r>");
    }
}

fn push_field(out: &mut String, name: &str, format: PageNumberFormat) {
    let instruction = format!(" {name} \\* {} ", field_switch(format));
    out.push_str("<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>");
    out.push_str(&format!(
        "<w:r><w:instrText xml:space=\"preserve\">{}</w:instrText></w:r>",
        xml::text(&instruction)
    ));
    out.push_str("<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>");
    out.push_str(&format!(
        "<w:r><w:t>{}</w:t></w:r>",
        xml::text(&format.format(1))
    ));
    out.push_str("<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>");
}

fn field_switch(format: PageNumberFormat) -> &'static str {
    match format {
        PageNumberFormat::Arabic => "ARABIC",
        PageNumberFormat::RomanUpper => "ROMAN",
        PageNumberFormat::RomanLower => "roman",
        PageNumberFormat::Alphabetical => "ALPHABETIC",
    }
}

fn page_number_format_name(format: PageNumberFormat) -> &'static str {
    match format {
        PageNumberFormat::Arabic => "decimal",
        PageNumberFormat::RomanUpper => "upperRoman",
        PageNumberFormat::RomanLower => "lowerRoman",
        PageNumberFormat::Alphabetical => "upperLetter",
    }
}

pub(super) fn import_config(
    document_xml: &str,
    archive: &loom_package::zip::PackageArchive,
) -> Result<crate::HeaderFooterConfig, String> {
    import::import_config(document_xml, archive)
}

#[cfg(test)]
#[path = "header_footer_tests.rs"]
mod tests;
