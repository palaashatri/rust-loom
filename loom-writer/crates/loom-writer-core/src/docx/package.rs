//! The fixed parts of a `.docx` package: content types, relationships,
//! styles, numbering, settings and document properties.

use super::xml::{self, W_NS, XML_DECLARATION};
use crate::PageStyle;

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

/// `[Content_Types].xml`.
pub(super) fn content_types(has_comments: bool) -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    );
    let mut part = |name: &str, content_type: &str| {
        out.push_str(&format!(
            "<Override PartName=\"{name}\" ContentType=\"{content_type}\"/>"
        ));
    };
    part("/word/document.xml", CT_MAIN);
    part("/word/styles.xml", &format!("{WML}.styles+xml"));
    part("/word/numbering.xml", &format!("{WML}.numbering+xml"));
    part("/word/settings.xml", &format!("{WML}.settings+xml"));
    if has_comments {
        part("/word/comments.xml", &format!("{WML}.comments+xml"));
        part(
            "/word/commentsExtended.xml",
            &format!("{WML}.commentsExtended+xml"),
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
    out.push_str("</Types>");
    out
}

/// `_rels/.rels`.
pub(super) fn package_rels() -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    out.push_str(&format!(
        "<Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"word/document.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
         <Relationship Id=\"rId3\" Type=\"{REL}/extended-properties\" Target=\"docProps/app.xml\"/>"
    ));
    out.push_str("</Relationships>");
    out
}

/// `word/_rels/document.xml.rels`.
pub(super) fn document_rels(has_comments: bool) -> String {
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
    out.push_str("</Relationships>");
    out
}

/// `word/settings.xml`. Compatibility mode 15 keeps Word from opening the
/// file in "Compatibility Mode".
pub(super) fn settings() -> String {
    format!(
        "{XML_DECLARATION}<w:settings xmlns:w=\"{W_NS}\"><w:zoom w:percent=\"100\"/>\
         <w:defaultTabStop w:val=\"720\"/><w:characterSpacingControl w:val=\"doNotCompress\"/>\
         <w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/>\
         <w:compatSetting w:name=\"overrideTableStyleFontSizeAndJustification\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"1\"/>\
         </w:compat></w:settings>"
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

/// Half-points for a size in points, never below one point.
fn half_points(points: f32) -> u32 {
    (points * 2.0).round().max(2.0) as u32
}

/// `word/styles.xml`: Normal, Heading 1 to 6 (so Word's navigation pane
/// works), List Paragraph, Quote, Table Grid and the comment styles. Sizes
/// follow the page style the editor lays out with.
pub(super) fn styles(page: &PageStyle) -> String {
    let body = half_points(page.body_font_size_pt);
    let mut out = String::from(XML_DECLARATION);
    out.push_str(&format!("<w:styles xmlns:w=\"{W_NS}\">"));
    out.push_str(&format!(
        "<w:docDefaults><w:rPrDefault><w:rPr>\
         <w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Calibri\" w:cs=\"Calibri\"/>\
         <w:sz w:val=\"{body}\"/><w:szCs w:val=\"{body}\"/><w:lang w:val=\"en-US\" w:eastAsia=\"en-US\" w:bidi=\"ar-SA\"/>\
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
             <w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\" w:hint=\"default\"/></w:rPr></w:lvl>",
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
