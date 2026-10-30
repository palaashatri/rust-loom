//! The fixed parts of a `.docx` package: content types, relationships,
//! styles, numbering, settings and document properties.

use super::header_footer::HeaderFooterParts;
use super::xml::{self, W_NS, XML_DECLARATION};
use crate::PageStyle;
use loom_package::zip::PackageArchive;
use roxmltree::Node;

const CT_MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";

/// Style ids the body refers to; every one is defined by [`styles`].
pub(super) const STYLE_LIST: &str = "ListParagraph";
pub(super) const STYLE_QUOTE: &str = "Quote";
pub(super) const STYLE_TABLE: &str = "TableGrid";

/// Spacing the paragraph styles inherit, in twentieths of a point. A
/// paragraph only carries direct spacing when it differs from these.
pub(super) const DEFAULT_SPACE_AFTER_TWIPS: i32 = 160;
pub(super) const DEFAULT_LINE_TWIPS: i32 = 276;
/// The face Writer's "Sans" text is written with, and the face of text that
/// names none. Arial is a sans face every Word install has; the PDF export
/// embeds Inter instead, so DOCX and PDF line breaks can differ slightly.
pub(super) const SANS_FAMILY: &str = "Arial";

/// `[Content_Types].xml`.
pub(super) fn content_types(
    has_comments: bool,
    has_footnotes: bool,
    chart_count: usize,
    images: &[super::body::BodyImage],
    header_footer: &HeaderFooterParts,
) -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    );
    let mut seen: Vec<&str> = Vec::new();
    for image in images {
        if !seen.contains(&image.extension) {
            seen.push(image.extension);
            out.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                image.extension, image.mime
            ));
        }
    }
    let mut part = |name: &str, content_type: &str| {
        out.push_str(&format!(
            "<Override PartName=\"{name}\" ContentType=\"{content_type}\"/>"
        ));
    };
    part("/word/document.xml", CT_MAIN);
    part("/word/styles.xml", &format!("{WML}.styles+xml"));
    part("/word/numbering.xml", &format!("{WML}.numbering+xml"));
    part("/word/settings.xml", &format!("{WML}.settings+xml"));
    if has_footnotes {
        part(
            "/word/footnotes.xml",
            super::footnotes::FOOTNOTES_CONTENT_TYPE,
        );
    }
    if has_comments {
        part("/word/comments.xml", &format!("{WML}.comments+xml"));
        part(
            "/word/commentsExtended.xml",
            &format!("{WML}.commentsExtended+xml"),
        );
    }
    for c in 1..=chart_count {
        part(
            &format!("/word/charts/chart{c}.xml"),
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        );
    }
    part(
        "/docProps/core.xml",
        "application/vnd.openxmlformats-package.core-properties+xml",
    );
    part(
        "/docProps/app.xml",
        "application/vnd.openxmlformats-officedocument.extended-properties+xml",
    );
    part(
        "/docProps/custom.xml",
        "application/vnd.openxmlformats-officedocument.custom-properties+xml",
    );
    out.push_str(&header_footer.content_type_overrides());
    out.push_str("</Types>");
    out
}

pub(super) fn package_rels() -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    out.push_str(&format!(
        "<Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"word/document.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
         <Relationship Id=\"rId3\" Type=\"{REL}/extended-properties\" Target=\"docProps/app.xml\"/>\
         <Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties\" Target=\"docProps/custom.xml\"/>"
    ));
    out.push_str("</Relationships>");
    out
}

/// `word/_rels/document.xml.rels`.
pub(super) fn document_rels(
    has_comments: bool,
    has_footnotes: bool,
    chart_count: usize,
    images: &[super::body::BodyImage],
    header_footer: &HeaderFooterParts,
) -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    out.push_str(&format!(
        "<Relationship Id=\"rId1\" Type=\"{REL}/styles\" Target=\"styles.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"{REL}/numbering\" Target=\"numbering.xml\"/>\
         <Relationship Id=\"rId3\" Type=\"{REL}/settings\" Target=\"settings.xml\"/>"
    ));
    if has_comments {
        out.push_str(&format!(
            "<Relationship Id=\"rId4\" Type=\"{REL}/comments\" Target=\"comments.xml\"/>\
             <Relationship Id=\"rId5\" Type=\"{REL_COMMENTS_EXTENDED}\" Target=\"commentsExtended.xml\"/>"
        ));
    }
    if has_footnotes {
        out.push_str(&format!(
            "<Relationship Id=\"rIdFootnotes\" Type=\"{}\" Target=\"footnotes.xml\"/>",
            super::footnotes::FOOTNOTES_REL
        ));
    }
    for c in 1..=chart_count {
        out.push_str(&format!(
            "<Relationship Id=\"rIdChart{c}\" Type=\"{REL}/chart\" Target=\"charts/chart{c}.xml\"/>"
        ));
    }
    for image in images {
        out.push_str(&format!(
            "<Relationship Id=\"rIdImg{i}\" Type=\"{REL}/image\" Target=\"media/image{i}.{ext}\"/>",
            i = image.index,
            ext = image.extension
        ));
    }
    out.push_str(&header_footer.relationships_xml());
    out.push_str("</Relationships>");
    out
}

/// `word/settings.xml`. Compatibility mode 15 keeps Word from opening the
/// file in "Compatibility Mode".
pub(super) fn settings(recording: bool) -> String {
    let track_revisions = if recording { "<w:trackRevisions/>" } else { "" };
    format!(
        "{XML_DECLARATION}<w:settings xmlns:w=\"{W_NS}\"><w:zoom w:percent=\"100\"/>         {track_revisions}<w:defaultTabStop w:val=\"720\"/><w:characterSpacingControl w:val=\"doNotCompress\"/>         <w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/>         <w:compatSetting w:name=\"overrideTableStyleFontSizeAndJustification\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"1\"/>         </w:compat></w:settings>"
    )
}

/// `docProps/core.xml`. Only the title is known to Writer; the file carries
/// no author and no timestamps so the export stays byte-for-byte repeatable.
pub(super) fn core_properties(title: &str) -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" \
         xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
    );
    if !title.trim().is_empty() {
        out.push_str(&format!("<dc:title>{}</dc:title>", xml::text(title)));
    }
    out.push_str("</cp:coreProperties>");
    out
}

/// `docProps/app.xml`.
pub(super) fn app_properties() -> String {
    format!(
        "{XML_DECLARATION}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" \
         xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
         <Application>Loom Writer</Application><DocSecurity>0</DocSecurity><ScaleCrop>false</ScaleCrop></Properties>"
    )
}

/// `docProps/custom.xml` stores the future tracked-changes author.
pub(super) fn custom_properties(tracked_changes_author: &str) -> String {
    format!(
        "{XML_DECLARATION}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\" \\
         xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\\
         <property fmtid=\"{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}\" pid=\"2\" name=\"LoomTrackedChangesAuthor\"><vt:lpwstr>{}</vt:lpwstr></property></Properties>",
        xml::text(tracked_changes_author)
    )
}

/// Imports and validates Loom's future tracked-changes author custom property.
pub(super) fn import_tracked_changes_author(
    archive: &PackageArchive,
) -> Result<Option<String>, String> {
    const CUSTOM_PROPERTIES_NS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
    const CUSTOM_VALUES_NS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
    const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
    const CONTENT_TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
    const CUSTOM_REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
    const CUSTOM_CONTENT_TYPE: &str =
        "application/vnd.openxmlformats-officedocument.custom-properties+xml";

    let Some(bytes) = archive.get("docProps/custom.xml") else {
        return Ok(None);
    };
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| "DOCX custom properties are not valid UTF-8".to_string())?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX custom properties: {error}"))?;
    if !document
        .root_element()
        .has_tag_name((CUSTOM_PROPERTIES_NS, "Properties"))
    {
        return Err("DOCX custom properties root is invalid".into());
    }
    let children: Vec<_> = document
        .root_element()
        .children()
        .filter(Node::is_element)
        .collect();
    if children
        .iter()
        .any(|node| !node.has_tag_name((CUSTOM_PROPERTIES_NS, "property")))
    {
        return Err("DOCX custom properties contain an unsupported element".into());
    }
    let mut property_ids = std::collections::HashSet::new();
    let mut author_property = None;
    for property in children {
        let raw_id = property
            .attribute("pid")
            .ok_or_else(|| "DOCX custom property is missing pid".to_string())?;
        let id = raw_id
            .parse::<u32>()
            .map_err(|_| "DOCX custom property pid is invalid".to_string())?;
        if id < 2 || !property_ids.insert(id) {
            return Err("DOCX custom property pids must be unique and at least 2".into());
        }
        if property.attribute("name") == Some("LoomTrackedChangesAuthor") {
            if author_property.replace(property).is_some() {
                return Err("DOCX has duplicate LoomTrackedChangesAuthor properties".into());
            }
        }
    }
    let Some(property) = author_property else {
        validate_custom_property_package(
            archive,
            CUSTOM_REL,
            CUSTOM_CONTENT_TYPE,
            PACKAGE_REL_NS,
            CONTENT_TYPES_NS,
        )?;
        return Ok(None);
    };
    if property.attribute("fmtid") != Some("{D5CDD505-2E9C-101B-9397-08002B2CF9AE}") {
        return Err("DOCX LoomTrackedChangesAuthor property has an invalid fmtid".into());
    }
    if property.attributes().any(|attribute| {
        attribute.namespace().is_some() || !["fmtid", "pid", "name"].contains(&attribute.name())
    }) {
        return Err("DOCX LoomTrackedChangesAuthor property has unsupported attributes".into());
    }
    let mut values = property.children().filter(Node::is_element);
    let Some(value) = values.next() else {
        return Err("DOCX LoomTrackedChangesAuthor property has no value".into());
    };
    if values.next().is_some()
        || !value.has_tag_name((CUSTOM_VALUES_NS, "lpwstr"))
        || value.attributes().next().is_some()
        || value.children().any(|node| node.is_element())
    {
        return Err("DOCX LoomTrackedChangesAuthor must be a string custom property".into());
    }
    validate_custom_property_package(
        archive,
        CUSTOM_REL,
        CUSTOM_CONTENT_TYPE,
        PACKAGE_REL_NS,
        CONTENT_TYPES_NS,
    )?;
    Ok(Some(value.text().unwrap_or_default().to_owned()))
}

fn validate_custom_property_package(
    archive: &PackageArchive,
    custom_relationship_type: &str,
    custom_content_type: &str,
    relationship_namespace: &str,
    content_type_namespace: &str,
) -> Result<(), String> {
    let rels = archive
        .get("_rels/.rels")
        .ok_or_else(|| "DOCX custom properties are missing the package relationship".to_string())?;
    let rels = std::str::from_utf8(rels)
        .map_err(|_| "DOCX package relationships are not valid UTF-8".to_string())?;
    let rels = roxmltree::Document::parse(rels)
        .map_err(|error| format!("invalid DOCX package relationships: {error}"))?;
    let mut custom_rels = rels.root_element().children().filter(|node| {
        node.has_tag_name((relationship_namespace, "Relationship"))
            && node.attribute("Type") == Some(custom_relationship_type)
    });
    let Some(relationship) = custom_rels.next() else {
        return Err("DOCX custom properties are missing their package relationship".into());
    };
    if custom_rels.next().is_some()
        || relationship.attribute("Target") != Some("docProps/custom.xml")
        || relationship.attribute("TargetMode").is_some()
    {
        return Err("DOCX custom properties have an invalid package relationship".into());
    }

    let content_types = archive
        .get("[Content_Types].xml")
        .ok_or_else(|| "DOCX custom properties are missing a content type".to_string())?;
    let content_types = std::str::from_utf8(content_types)
        .map_err(|_| "DOCX content types are not valid UTF-8".to_string())?;
    let content_types = roxmltree::Document::parse(content_types)
        .map_err(|error| format!("invalid DOCX content types: {error}"))?;
    let mut overrides = content_types.root_element().children().filter(|node| {
        node.has_tag_name((content_type_namespace, "Override"))
            && node.attribute("PartName") == Some("/docProps/custom.xml")
    });
    let Some(override_node) = overrides.next() else {
        return Err("DOCX custom properties are missing their content type".into());
    };
    if overrides.next().is_some()
        || override_node.attribute("ContentType") != Some(custom_content_type)
    {
        return Err("DOCX custom properties have an invalid content type".into());
    }
    Ok(())
}

/// Half-points for a size in points, never below one point.
fn half_points(points: f32) -> u32 {
    (points * 2.0).round().max(2.0) as u32
}

/// `word/styles.xml`: Normal, Heading 1 to 6 (so Word's navigation pane
/// works), List Paragraph, Quote, Table Grid and the comment styles. Sizes
/// follow the page style the editor lays out with.
pub(super) fn styles(page: &PageStyle, appearance: crate::PageAppearance) -> String {
    // Unnamed text is the editor's default run: 12 pt in the Sans family.
    let body = half_points(loom_text::CharacterStyle::default().font_size);
    let family = SANS_FAMILY;
    let default_color = if appearance.is_black() {
        "<w:color w:val=\"FFFFFF\"/>"
    } else {
        ""
    };
    let mut out = String::from(XML_DECLARATION);
    out.push_str(&format!("<w:styles xmlns:w=\"{W_NS}\">"));
    out.push_str(&format!(
        "<w:docDefaults><w:rPrDefault><w:rPr>\
         <w:rFonts w:ascii=\"{family}\" w:hAnsi=\"{family}\" w:eastAsia=\"{family}\" w:cs=\"{family}\"/>\
         <w:sz w:val=\"{body}\"/><w:szCs w:val=\"{body}\"/>{default_color}<w:lang w:val=\"en-US\" w:eastAsia=\"en-US\" w:bidi=\"ar-SA\"/>\
         </w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>\
         <w:spacing w:after=\"{DEFAULT_SPACE_AFTER_TWIPS}\" w:line=\"{DEFAULT_LINE_TWIPS}\" w:lineRule=\"auto\"/>\
         </w:pPr></w:pPrDefault></w:docDefaults>"
    ));
    out.push_str(
        "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         <w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"><w:name w:val=\"Default Paragraph Font\"/>\
         <w:uiPriority w:val=\"1\"/><w:semiHidden/><w:unhideWhenUsed/></w:style>\
         <w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/>\
         <w:uiPriority w:val=\"99\"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/>\
         <w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"108\" w:type=\"dxa\"/>\
         <w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>\
         <w:style w:type=\"numbering\" w:default=\"1\" w:styleId=\"NoList\"><w:name w:val=\"No List\"/>\
         <w:uiPriority w:val=\"99\"/><w:semiHidden/><w:unhideWhenUsed/></w:style>",
    );
    for level in 1..=6u32 {
        let size = half_points(page.font_size_for_kind(&format!("heading{level}")));
        out.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{level}\"><w:name w:val=\"heading {level}\"/>\
             <w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:uiPriority w:val=\"9\"/><w:qFormat/>\
             <w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before=\"240\" w:after=\"80\"/><w:outlineLvl w:val=\"{}\"/></w:pPr>\
             <w:rPr><w:b/><w:bCs/><w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr></w:style>",
            level - 1
        ));
    }
    out.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"{STYLE_LIST}\"><w:name w:val=\"List Paragraph\"/>\
         <w:basedOn w:val=\"Normal\"/><w:uiPriority w:val=\"34\"/><w:qFormat/>\
         <w:pPr><w:ind w:left=\"720\"/><w:contextualSpacing/></w:pPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"{STYLE_QUOTE}\"><w:name w:val=\"Quote\"/>\
         <w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:uiPriority w:val=\"29\"/><w:qFormat/>\
         <w:pPr><w:ind w:left=\"864\" w:right=\"864\"/></w:pPr><w:rPr><w:i/><w:iCs/></w:rPr></w:style>\
         <w:style w:type=\"table\" w:styleId=\"{STYLE_TABLE}\"><w:name w:val=\"Table Grid\"/>\
         <w:basedOn w:val=\"TableNormal\"/><w:uiPriority w:val=\"39\"/>\
         <w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr>\
         <w:tblPr><w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
         </w:tblBorders></w:tblPr></w:style>\
         <w:style w:type=\"character\" w:styleId=\"CommentReference\"><w:name w:val=\"annotation reference\"/>\
         <w:basedOn w:val=\"DefaultParagraphFont\"/><w:uiPriority w:val=\"99\"/><w:semiHidden/><w:unhideWhenUsed/>\
         <w:rPr><w:sz w:val=\"16\"/><w:szCs w:val=\"16\"/></w:rPr></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"CommentText\"><w:name w:val=\"annotation text\"/>\
         <w:basedOn w:val=\"Normal\"/><w:uiPriority w:val=\"99\"/><w:unhideWhenUsed/>\
         <w:pPr><w:spacing w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:rPr><w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/></w:rPr></w:style>"
    ));
    out.push_str("</w:styles>");
    out
}

/// `word/numbering.xml`: abstract definition 0 is the bullet list used by
/// every bulleted block; definition 1 is the numbered list. `num` 1 is the
/// shared bullet list and `num` 2 onward are numbered lists, each restarting
/// at 1, because Writer restarts numbering after any block that is not a
/// numbered item.
pub(super) fn numbering(numbered_lists: usize) -> String {
    let family = SANS_FAMILY;
    let mut out = String::from(XML_DECLARATION);
    out.push_str(&format!("<w:numbering xmlns:w=\"{W_NS}\">"));

    out.push_str(
        "<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>",
    );
    for level in 0..9u32 {
        let glyph = if level % 2 == 0 {
            "\u{2022}"
        } else {
            "\u{2013}"
        };
        out.push_str(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/>\
             <w:lvlText w:val=\"{glyph}\"/><w:lvlJc w:val=\"left\"/>\
             <w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr>\
             <w:rPr><w:rFonts w:ascii=\"{family}\" w:hAnsi=\"{family}\" w:cs=\"{family}\" w:hint=\"default\"/></w:rPr></w:lvl>",
            720 * (level + 1)
        ));
    }
    out.push_str("</w:abstractNum>");

    out.push_str(
        "<w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>",
    );
    for level in 0..9u32 {
        let format = ["decimal", "lowerLetter", "lowerRoman"][(level % 3) as usize];
        out.push_str(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/>\
             <w:lvlText w:val=\"%{}.\"/><w:lvlJc w:val=\"left\"/>\
             <w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            level + 1,
            720 * (level + 1)
        ));
    }
    out.push_str("</w:abstractNum>");

    out.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>");
    for list in 0..numbered_lists {
        out.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"1\"/>\
             <w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>",
            list + 2
        ));
    }
    out.push_str("</w:numbering>");
    out
}
