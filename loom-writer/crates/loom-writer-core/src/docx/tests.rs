//! Structural tests: every part is parsed by a real XML parser and every
//! cross-reference (content types, relationships, style ids, numbering ids)
//! is resolved, so a broken package fails here instead of in Word.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use loom_package::zip::PackageArchive;
use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};
use roxmltree::{Document, Node};

use super::{export_docx, DocxExport};
use crate::{
    CommentThread, PageAppearance, PageMarginsPreset, PageOrientation, PageSetup, PaperSize,
    RevisionKind, RichBlock, WriterDocument, TABLE_BLOCK_KIND,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn run(start: usize, end: usize, style: CharacterStyle) -> StyleRun {
    StyleRun { start, end, style }
}

fn styled(f: impl FnOnce(&mut CharacterStyle)) -> CharacterStyle {
    let mut style = CharacterStyle::default();
    f(&mut style);
    style
}

/// Reads a committed DOCX fixture authored outside Writer's exporter.
fn committed_fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn rewrite_document_xml(export: &DocxExport, rewrite: impl FnOnce(&str) -> String) -> Vec<u8> {
    let archive = PackageArchive::from_bytes(&export.bytes).expect("read DOCX package");
    let document_xml = std::str::from_utf8(
        archive
            .get("word/document.xml")
            .expect("document part exists"),
    )
    .expect("document XML is UTF-8");
    let rewritten = rewrite(document_xml);
    let mut fixture = PackageArchive::new();
    for path in archive.paths() {
        let data = if path == "word/document.xml" {
            rewritten.as_bytes().to_vec()
        } else {
            archive.get(path).expect("part exists").to_vec()
        };
        fixture.add(path, data).expect("copy package part");
    }
    fixture.to_bytes().expect("write fixture package")
}

fn rewrite_package_part(
    export: &DocxExport,
    target: &str,
    rewrite: impl FnOnce(&str) -> String,
) -> Vec<u8> {
    let archive = PackageArchive::from_bytes(&export.bytes).expect("read DOCX package");
    let target_xml = std::str::from_utf8(archive.get(target).expect("target part exists"))
        .expect("XML part is UTF-8");
    let rewritten = rewrite(target_xml).into_bytes();
    let mut fixture = PackageArchive::new();
    for path in archive.paths() {
        let data = if path == target {
            rewritten.clone()
        } else {
            archive.get(path).expect("part exists").to_vec()
        };
        fixture.add(path, data).expect("copy package part");
    }
    fixture.to_bytes().expect("write fixture package")
}

/// A document touching every feature the export claims to carry.
pub(super) fn rich_document() -> WriterDocument {
    let mut doc = WriterDocument::new("rich", "Quarterly <Report> & Notes");
    doc.page = PageSetup {
        paper: PaperSize::Letter,
        orientation: PageOrientation::Landscape,
        margins: PageMarginsPreset::Moderate,
        ..PageSetup::default()
    };
    doc.blocks.push(RichBlock::new(1, "heading1", "Overview"));
    let mut p = RichBlock::new(
        2,
        "paragraph",
        "Plain bold italic under strike mix & <angle> \"q\"",
    );
    p.runs = vec![
        run(6, 10, styled(|s| s.weight = FontWeight::Bold)),
        run(11, 17, styled(|s| s.italic = true)),
        run(18, 23, styled(|s| s.underline = true)),
        run(24, 30, styled(|s| s.strikethrough = true)),
    ];
    doc.blocks.push(p);
    doc.blocks.push(RichBlock::new(3, "heading2", "Details"));
    let mut centered = RichBlock::new(4, "paragraph", "Centered line");
    centered.style.alignment = Alignment::Center;
    doc.blocks.push(centered);
    doc.blocks.push(RichBlock::new(5, "heading6", "Deepest"));
    doc.blocks
        .push(RichBlock::new(6, "list-bulleted", "Apples"));
    doc.blocks.push(RichBlock::new(7, "list-bulleted", "Pears"));
    doc.blocks.push(RichBlock::new(8, "list-numbered", "First"));
    doc.blocks
        .push(RichBlock::new(9, "list-numbered", "Second"));
    doc.blocks
        .push(RichBlock::new(10, "paragraph", "Interrupt"));
    doc.blocks
        .push(RichBlock::new(11, "list-numbered", "Restarted"));
    doc.blocks.push(crate::tables::markdown_table_block(
        12,
        "| Name | Qty |\n| --- | --- |\n| Tea | 2 |\n| A \\| B | 3 |",
    ));
    doc.blocks
        .push(RichBlock::new(13, "paragraph", "tab\there\nnew line"));
    doc.blocks.push(RichBlock::new(
        14,
        "paragraph",
        "caf\u{e9} \u{65e5}\u{672c}\u{8a9e} \u{1F600}",
    ));
    doc.comments.push(CommentThread {
        id: "c1".into(),
        author: "Ada Lovelace".into(),
        block_id: 2,
        start: 6,
        end: 10,
        body: "Check this\nsecond line".into(),
        resolved: true,
        orphaned: false,
    });
    doc.comments.push(CommentThread {
        id: "c2".into(),
        author: "Bob".into(),
        block_id: 4,
        start: 0,
        end: 8,
        body: "Open".into(),
        resolved: false,
        orphaned: false,
    });
    doc.comments.push(CommentThread {
        id: "c3".into(),
        author: "Bob".into(),
        block_id: 4,
        start: 0,
        end: 1,
        body: "Gone".into(),
        resolved: false,
        orphaned: true,
    });
    doc
}

fn parts(export: &DocxExport) -> HashMap<String, String> {
    let archive = PackageArchive::from_bytes(&export.bytes).expect("zip reads back");
    archive
        .paths()
        .into_iter()
        .map(|name| {
            let bytes = archive.get(name).unwrap();
            (
                name.to_string(),
                String::from_utf8(bytes.to_vec()).expect("utf-8"),
            )
        })
        .collect()
}

fn parse(part: &str) -> Document<'_> {
    Document::parse(part).expect("part is well-formed XML")
}

fn w_attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute((W, name))
}

fn is_w<'a>(node: &Node<'a, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(W)
}

fn all<'a, 'i>(doc: &'a Document<'i>, name: &str) -> Vec<Node<'a, 'i>> {
    doc.descendants().filter(|n| is_w(n, name)).collect()
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|c| is_w(c, name))
}

fn paragraph_text(p: Node) -> String {
    let mut out = String::new();
    for n in p.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "t" => out.push_str(n.text().unwrap_or("")),
            "tab" => out.push('\t'),
            "br" => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn body_paragraph<'a, 'i>(doc: &'a Document<'i>, text: &str) -> Node<'a, 'i> {
    all(doc, "p")
        .into_iter()
        .find(|p| paragraph_text(*p) == text)
        .unwrap_or_else(|| panic!("no paragraph {text:?}"))
}

#[test]
fn every_part_is_well_formed_and_declared() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    for (name, xml) in &parts {
        Document::parse(xml).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(xml.starts_with("<?xml"), "{name} lacks an XML declaration");
    }
    let expected: BTreeSet<&str> = [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/_rels/document.xml.rels",
        "word/styles.xml",
        "word/numbering.xml",
        "word/settings.xml",
        "word/comments.xml",
        "word/commentsExtended.xml",
        "docProps/core.xml",
        "docProps/app.xml",
        "docProps/custom.xml",
    ]
    .into();
    assert_eq!(
        parts.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        expected
    );

    let types = parse(&parts["[Content_Types].xml"]);
    let overrides: BTreeSet<String> = types
        .descendants()
        .filter(|n| n.tag_name().name() == "Override")
        .map(|n| {
            n.attribute("PartName")
                .unwrap()
                .trim_start_matches('/')
                .to_string()
        })
        .collect();
    for name in expected
        .iter()
        .filter(|n| **n != "[Content_Types].xml" && !n.ends_with(".rels"))
    {
        assert!(overrides.contains(*name), "no content type for {name}");
    }
    for name in &overrides {
        assert!(parts.contains_key(name), "override for missing part {name}");
    }
}

#[test]
fn relationship_targets_exist() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    for (rels, base) in [
        ("_rels/.rels", ""),
        ("word/_rels/document.xml.rels", "word/"),
    ] {
        let doc = parse(&parts[rels]);
        let mut ids = BTreeSet::new();
        for rel in doc
            .descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
        {
            assert!(
                ids.insert(rel.attribute("Id").unwrap().to_string()),
                "duplicate rel id"
            );
            let target = format!("{base}{}", rel.attribute("Target").unwrap());
            assert!(
                parts.contains_key(&target),
                "{rels}: missing target {target}"
            );
        }
    }
    assert!(parts["_rels/.rels"].contains("/officeDocument\""));
}

#[test]
fn styles_and_numbering_used_by_the_body_are_defined() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let styles_doc = parse(&parts["word/styles.xml"]);
    let defined: BTreeSet<&str> = all(&styles_doc, "style")
        .into_iter()
        .filter_map(|s| w_attr(s, "styleId"))
        .collect();
    let body = parse(&parts["word/document.xml"]);
    let comments = parse(&parts["word/comments.xml"]);
    for doc in [&body, &comments] {
        for tag in ["pStyle", "rStyle", "tblStyle"] {
            for node in all(doc, tag) {
                let id = w_attr(node, "val").unwrap();
                assert!(defined.contains(id), "undefined style {id}");
            }
        }
    }
    for level in 1..=6 {
        assert!(defined.contains(format!("Heading{level}").as_str()));
    }
    for tag in ["basedOn", "next"] {
        for node in all(&styles_doc, tag) {
            assert!(defined.contains(w_attr(node, "val").unwrap()));
        }
    }

    let numbering = parse(&parts["word/numbering.xml"]);
    let abstracts: BTreeSet<&str> = all(&numbering, "abstractNum")
        .into_iter()
        .filter_map(|n| w_attr(n, "abstractNumId"))
        .collect();
    let nums: BTreeSet<&str> = all(&numbering, "num")
        .into_iter()
        .filter_map(|n| w_attr(n, "numId"))
        .collect();
    for num in all(&numbering, "num") {
        let target = w_attr(child(num, "abstractNumId").unwrap(), "val").unwrap();
        assert!(abstracts.contains(target));
    }
    for num_id in all(&body, "numId") {
        assert!(nums.contains(w_attr(num_id, "val").unwrap()));
    }
}

#[test]
fn headings_use_word_heading_styles_with_outline_levels() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let mut seen = Vec::new();
    for p in all(&body, "p") {
        let style = child(p, "pPr").and_then(|ppr| child(ppr, "pStyle"));
        if let Some(id) = style.and_then(|s| w_attr(s, "val")) {
            if id.starts_with("Heading") {
                seen.push((id.to_string(), paragraph_text(p)));
            }
        }
    }
    assert_eq!(
        seen,
        vec![
            ("Heading1".to_string(), "Overview".to_string()),
            ("Heading2".to_string(), "Details".to_string()),
            ("Heading6".to_string(), "Deepest".to_string()),
        ]
    );
    let styles = parse(&parts["word/styles.xml"]);
    let h3 = all(&styles, "style")
        .into_iter()
        .find(|s| w_attr(*s, "styleId") == Some("Heading3"))
        .unwrap();
    let outline = all(&styles, "outlineLvl")
        .into_iter()
        .find(|n| n.ancestors().any(|a| a == h3))
        .unwrap();
    assert_eq!(w_attr(outline, "val"), Some("2"));
}

#[test]
fn inline_runs_carry_bold_italic_underline_and_strike() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let mut by_flag: HashMap<&str, String> = HashMap::new();
    for r in all(&body, "r") {
        if r.ancestors().any(|a| is_w(&a, "tbl")) {
            continue;
        }
        let text: String = r
            .children()
            .filter(|c| is_w(c, "t"))
            .filter_map(|c| c.text())
            .collect();
        for (tag, key) in [
            ("b", "bold"),
            ("i", "italic"),
            ("u", "underline"),
            ("strike", "strike"),
        ] {
            let on = child(r, "rPr").is_some_and(|pr| child(pr, tag).is_some());
            if on && !text.is_empty() {
                by_flag.entry(key).or_default().push_str(&text);
            }
        }
    }
    assert_eq!(by_flag["bold"], "bold");
    assert_eq!(by_flag["italic"], "italic");
    assert_eq!(by_flag["underline"], "under");
    assert_eq!(by_flag["strike"], "strike");
}

#[test]
fn runs_state_their_size_and_only_an_authored_font_family() {
    let mut doc = WriterDocument::new("fonts", "Fonts");
    doc.push(RichBlock::new(
        doc.next_id(),
        "paragraph",
        "Plain body text",
    ));
    doc.push(RichBlock::new(doc.next_id(), "heading1", "Heading text"));
    let mut serif = RichBlock::new(doc.next_id(), "paragraph", "Serif text");
    serif.runs = vec![run(
        0,
        11,
        styled(|style| {
            style.font_family = "Serif".into();
            style.font_size = 18.0;
        }),
    )];
    doc.push(serif);
    let export = export_docx(&doc).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);

    // Word substitutes its own size for any run that does not name one, so
    // every run with text states its size. Inherited text names no family and
    // relies on the Arial document default; an authored family is stated so a
    // reopened document can tell it apart from inherited text.
    for r in all(&body, "r") {
        let text: String = r
            .children()
            .filter(|c| is_w(c, "t"))
            .filter_map(|c| c.text())
            .collect();
        if text.is_empty() {
            continue;
        }
        let props = child(r, "rPr").unwrap_or_else(|| panic!("run {text:?} has no run properties"));
        assert!(
            child(props, "sz").and_then(|s| w_attr(s, "val")).is_some(),
            "run {text:?} names no font size"
        );
    }

    let run_font = |text: &str| {
        let paragraph = body_paragraph(&body, text);
        let r = paragraph
            .descendants()
            .find(|n| is_w(n, "r"))
            .expect("paragraph has a run");
        let props = child(r, "rPr").expect("run properties");
        let family = child(props, "rFonts").and_then(|f| w_attr(f, "ascii"));
        let size = child(props, "sz").and_then(|s| w_attr(s, "val"));
        (family.map(str::to_string), size.map(str::to_string))
    };
    let font = |family: Option<&str>, half_points: &str| {
        (family.map(str::to_string), Some(half_points.to_string()))
    };
    // The editor's default is 12 pt Sans; text that names no family inherits
    // the Arial document default below (the PDF export embeds Inter).
    assert_eq!(run_font("Plain body text"), font(None, "24"));
    // A heading keeps the 24 pt its heading style gives.
    assert_eq!(run_font("Heading text"), font(None, "48"));
    assert_eq!(run_font("Serif text"), font(Some("Times New Roman"), "36"));

    // Unstyled text, the Normal style and new text typed in Word agree.
    let styles = parse(&parts["word/styles.xml"]);
    let defaults = child(
        all(&styles, "rPrDefault")
            .first()
            .copied()
            .expect("document defaults"),
        "rPr",
    )
    .expect("default run properties");
    assert_eq!(
        child(defaults, "rFonts").and_then(|f| w_attr(f, "ascii")),
        Some("Arial")
    );
    assert_eq!(
        child(defaults, "sz").and_then(|s| w_attr(s, "val")),
        Some("24")
    );
}

#[test]
fn alignment_lists_and_numbering_restart() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let centered = body_paragraph(&body, "Centered line");
    let jc = child(child(centered, "pPr").unwrap(), "jc").unwrap();
    assert_eq!(w_attr(jc, "val"), Some("center"));

    let num_of = |text: &str| -> Option<String> {
        let p = body_paragraph(&body, text);
        let n = p.descendants().find(|n| is_w(n, "numId"))?;
        w_attr(n, "val").map(str::to_string)
    };
    assert!(num_of("Apples").is_some());
    assert_eq!(num_of("Apples"), num_of("Pears"));
    assert_eq!(num_of("First"), num_of("Second"));
    assert_ne!(num_of("Apples"), num_of("First"));
    assert_ne!(
        num_of("First"),
        num_of("Restarted"),
        "an interrupted list restarts"
    );
    assert_eq!(num_of("Interrupt"), None);

    assert!(!parts["word/document.xml"].contains('\u{2022}'));
    let numbering = parse(&parts["word/numbering.xml"]);
    assert_eq!(all(&numbering, "startOverride").len(), 2);
    assert!(all(&numbering, "numFmt")
        .iter()
        .any(|n| w_attr(*n, "val") == Some("bullet")));
    assert!(all(&numbering, "numFmt")
        .iter()
        .any(|n| w_attr(*n, "val") == Some("decimal")));
}

#[test]
fn justified_paragraph_alignment_round_trips_through_loomdoc_and_docx() {
    let mut doc = WriterDocument::new("justify-roundtrip", "Justify round trip");
    let mut paragraph = RichBlock::new(1, "paragraph", "one two three four");
    paragraph.style.alignment = Alignment::Justify;
    doc.blocks.push(paragraph);

    let loomdoc = crate::save_document(&doc).expect("save .loomdoc");
    let loaded = crate::load_document(&loomdoc).expect("load .loomdoc");
    assert_eq!(loaded.blocks[0].style.alignment, Alignment::Justify);

    let exported = export_docx(&doc).expect("export .docx");
    let parts = parts(&exported);
    let body = parse(&parts["word/document.xml"]);
    let paragraph = body_paragraph(&body, "one two three four");
    let justification = child(child(paragraph, "pPr").expect("paragraph properties"), "jc")
        .expect("Word paragraph justification");
    assert_eq!(w_attr(justification, "val"), Some("both"));

    let imported = super::import_docx(&exported.bytes).expect("import exported .docx");
    assert_eq!(imported.blocks[0].style.alignment, Alignment::Justify);
}

#[test]
fn paragraph_spacing_line_height_and_indents_round_trip_through_docx() {
    let mut document = WriterDocument::new("paragraph-layout", "Paragraph layout");
    let mut paragraph = RichBlock::new(1, "paragraph", "Layout values");
    paragraph.style.space_before = 3.2;
    paragraph.style.space_after = 10.4;
    paragraph.style.line_spacing = 1.5;
    paragraph.style.left_indent = 18.0;
    paragraph.style.right_indent = 4.5;
    paragraph.style.first_line_indent = 9.0;
    let expected = paragraph.style.clone();
    document.blocks.push(paragraph);

    let exported = export_docx(&document).expect("export paragraph layout");
    let parts = parts(&exported);
    let body = parse(&parts["word/document.xml"]);
    let p = body_paragraph(&body, "Layout values");
    let p_pr = child(p, "pPr").expect("paragraph properties");
    let spacing = child(p_pr, "spacing").expect("spacing");
    assert_eq!(w_attr(spacing, "before"), Some("64"));
    assert_eq!(w_attr(spacing, "after"), Some("208"));
    assert_eq!(w_attr(spacing, "line"), Some("360"));
    assert_eq!(w_attr(spacing, "lineRule"), Some("auto"));
    let indent = child(p_pr, "ind").expect("indent");
    assert_eq!(w_attr(indent, "left"), Some("360"));
    assert_eq!(w_attr(indent, "right"), Some("90"));
    assert_eq!(w_attr(indent, "firstLine"), Some("180"));

    let imported = super::import_docx(&exported.bytes).expect("import paragraph layout");
    assert_eq!(imported.blocks[0].style, expected);
    let reopened = super::import_docx(
        &export_docx(&imported)
            .expect("re-export paragraph layout")
            .bytes,
    )
    .expect("reopen paragraph layout");
    assert_eq!(reopened.blocks[0].style, expected);
}

#[test]
fn docx_list_marker_indents_do_not_become_user_paragraph_indents() {
    let mut document = WriterDocument::new("nested-list", "Nested list");
    let mut item = RichBlock::new(1, "list-bulleted", "nested item");
    item.style.list_level = 1;
    document.blocks.push(item);

    let imported = super::import_docx(&export_docx(&document).expect("export nested list").bytes)
        .expect("import nested list");
    assert_eq!(imported.blocks[0].style.list_level, 1);
    assert_eq!(imported.blocks[0].style.left_indent, 0.0);
    assert_eq!(imported.blocks[0].style.first_line_indent, 0.0);

    let reexport = export_docx(&imported).expect("re-export nested list");
    let parts = parts(&reexport);
    let body = parse(&parts["word/document.xml"]);
    let p = body_paragraph(&body, "nested item");
    let indent = child(child(p, "pPr").unwrap(), "ind").unwrap();
    assert_eq!(w_attr(indent, "left"), Some("1080"));
    assert_eq!(w_attr(indent, "hanging"), Some("360"));
}

#[test]
fn malformed_or_unrepresented_docx_paragraph_layout_is_rejected() {
    let cases = [
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:spacing w:before="invalid"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#,
            "integer number of twips",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:spacing w:line="360" w:lineRule="exact"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#,
            "exact or minimum line spacing",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:ind w:firstLine="180" w:hanging="120"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#,
            "both firstLine and hanging",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:pPr><w:spacing w:before="20"/></w:pPr><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            "spacing and indents in DOCX table cells",
        ),
    ];
    for (xml, expected_error) in cases {
        let mut archive = PackageArchive::new();
        archive
            .add("word/document.xml", xml.as_bytes().to_vec())
            .unwrap();
        let error = super::import_docx(&archive.to_bytes().unwrap())
            .expect_err("unrepresentable paragraph layout must fail");
        assert!(
            error.contains(expected_error),
            "expected {expected_error:?} in error {error:?}"
        );
    }
}

#[test]
fn docx_bidi_and_logical_start_alignment_survive_import_export() {
    let mut doc = WriterDocument::new("bidi-roundtrip", "BiDi round trip");
    let mut paragraph = RichBlock::new(1, "paragraph", "שלום world");
    paragraph.style.alignment = Alignment::Center;
    doc.blocks.push(paragraph);

    let exported = export_docx(&doc).expect("export seed .docx");
    let fixture = rewrite_document_xml(&exported, |xml| {
        xml.replace(
            "<w:jc w:val=\"center\"/>",
            "<!-- </w:pPr> --><w:bidi/><w:jc w:val=\"start\"/>",
        )
    });
    let imported = super::import_docx(&fixture).expect("import DOCX bidi paragraph");
    let roundtrip = export_docx(&imported).expect("re-export DOCX bidi paragraph");
    let output = parts(&roundtrip);
    let body = parse(&output["word/document.xml"]);
    let paragraph = body_paragraph(&body, "שלום world");
    let properties = child(paragraph, "pPr").expect("paragraph properties");
    assert!(
        child(properties, "bidi").is_some(),
        "direct w:bidi direction must survive DOCX import/export"
    );
    assert_eq!(
        w_attr(child(properties, "jc").expect("logical alignment"), "val"),
        Some("start"),
        "logical start must not be flattened to a physical left/right edge"
    );
    assert_eq!(imported.blocks[0].style.alignment, Alignment::Start);
    assert_eq!(
        imported.blocks[0].style.direction,
        loom_text::ParagraphDirection::RightToLeft
    );
}

#[test]
fn docx_start_end_left_and_right_alignments_round_trip_exactly() {
    for (value, expected) in [
        ("start", Alignment::Start),
        ("end", Alignment::End),
        ("left", Alignment::Left),
        ("right", Alignment::Right),
    ] {
        let mut doc = WriterDocument::new("alignment", "Alignment");
        doc.blocks
            .push(RichBlock::new(1, "paragraph", "Alignment sample"));
        doc.blocks[0].style.alignment = Alignment::Center;
        let exported = export_docx(&doc).expect("export seed");
        let fixture = rewrite_document_xml(&exported, |xml| {
            xml.replace(
                "<w:jc w:val=\"center\"/>",
                &format!("<w:jc w:val=\"{value}\"/>"),
            )
        });
        let imported = super::import_docx(&fixture).expect("import alignment fixture");
        assert_eq!(imported.blocks[0].style.alignment, expected, "{value}");
        let roundtrip = export_docx(&imported).expect("re-export alignment");
        let output = parts(&roundtrip);
        let body = parse(&output["word/document.xml"]);
        let paragraph = body_paragraph(&body, "Alignment sample");
        let jc = child(child(paragraph, "pPr").expect("paragraph properties"), "jc")
            .expect("alignment value");
        assert_eq!(w_attr(jc, "val"), Some(value), "{value}");
    }
}

#[test]
fn docx_bidi_on_off_tokens_round_trip_to_canonical_direction() {
    for (attribute, expected) in [
        ("", loom_text::ParagraphDirection::RightToLeft),
        (
            " w:val=\"true\"",
            loom_text::ParagraphDirection::RightToLeft,
        ),
        (" w:val=\"1\"", loom_text::ParagraphDirection::RightToLeft),
        (" w:val=\"on\"", loom_text::ParagraphDirection::RightToLeft),
        (
            " w:val=\"false\"",
            loom_text::ParagraphDirection::LeftToRight,
        ),
        (" w:val=\"0\"", loom_text::ParagraphDirection::LeftToRight),
        (" w:val=\"off\"", loom_text::ParagraphDirection::LeftToRight),
    ] {
        let mut doc = WriterDocument::new("direction", "Direction");
        let mut paragraph = RichBlock::new(1, "paragraph", "Direction sample");
        paragraph.style.alignment = Alignment::Center;
        doc.blocks.push(paragraph);
        let exported = export_docx(&doc).expect("export seed");
        let fixture = rewrite_document_xml(&exported, |xml| {
            xml.replace(
                "<w:jc w:val=\"center\"/>",
                &format!("<w:bidi{attribute}/><w:jc w:val=\"center\"/>"),
            )
        });
        let imported = super::import_docx(&fixture).expect("import direction fixture");
        assert_eq!(imported.blocks[0].style.direction, expected, "{attribute}");
        let roundtrip = export_docx(&imported).expect("re-export direction");
        let output = parts(&roundtrip);
        let body = parse(&output["word/document.xml"]);
        let paragraph = body_paragraph(&body, "Direction sample");
        let properties = child(paragraph, "pPr").expect("paragraph properties");
        match expected {
            loom_text::ParagraphDirection::RightToLeft => {
                assert!(child(properties, "bidi").is_some(), "{attribute}");
            }
            loom_text::ParagraphDirection::LeftToRight => {
                let bidi = child(properties, "bidi").expect("explicit LTR property");
                assert_eq!(w_attr(bidi, "val"), Some("0"), "{attribute}");
            }
            loom_text::ParagraphDirection::Auto => unreachable!(),
        }
    }
}

#[test]
fn docx_paragraph_without_bidi_property_remains_auto() {
    let mut doc = WriterDocument::new("direction", "Direction");
    doc.blocks
        .push(RichBlock::new(1, "paragraph", "Automatic direction"));
    doc.blocks[0].style.alignment = Alignment::Center;
    let exported = export_docx(&doc).expect("export seed");
    let fixture = rewrite_document_xml(&exported, |xml| xml.to_string());
    let imported = super::import_docx(&fixture).expect("import automatic direction");
    assert_eq!(
        imported.blocks[0].style.direction,
        loom_text::ParagraphDirection::Auto
    );
    let roundtrip = export_docx(&imported).expect("re-export automatic direction");
    let output = parts(&roundtrip);
    let body = parse(&output["word/document.xml"]);
    let paragraph = body_paragraph(&body, "Automatic direction");
    assert!(
        child(
            child(paragraph, "pPr").expect("paragraph properties"),
            "bidi"
        )
        .is_none(),
        "automatic direction must be represented by omitting w:bidi"
    );
}

#[test]
fn markdown_table_becomes_a_real_table_with_header_row() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let tbl = all(&body, "tbl").into_iter().next().expect("a w:tbl");
    let rows: Vec<Vec<String>> = tbl
        .children()
        .filter(|c| is_w(c, "tr"))
        .map(|tr| {
            tr.children()
                .filter(|c| is_w(c, "tc"))
                .map(|tc| {
                    tc.descendants()
                        .filter(|n| is_w(n, "p"))
                        .map(paragraph_text)
                        .collect::<String>()
                })
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            vec!["Name".to_string(), "Qty".to_string()],
            vec!["Tea".to_string(), "2".to_string()],
            vec!["A | B".to_string(), "3".to_string()],
        ]
    );
    let first_row = child(tbl, "tr").unwrap();
    assert!(first_row.descendants().any(|n| is_w(&n, "tblHeader")));
    let grid = child(tbl, "tblGrid").unwrap();
    assert_eq!(grid.children().filter(|c| is_w(c, "gridCol")).count(), 2);
    assert!(!parts["word/document.xml"].contains("| Name"));
}

#[test]
fn page_setup_becomes_section_properties() {
    let export = export_docx(&rich_document()).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let size = all(&body, "pgSz")[0];
    // US Letter, landscape: 11in x 8.5in.
    assert_eq!(w_attr(size, "w"), Some("15840"));
    assert_eq!(w_attr(size, "h"), Some("12240"));
    assert_eq!(w_attr(size, "orient"), Some("landscape"));
    let margins = all(&body, "pgMar")[0];
    assert_eq!(w_attr(margins, "top"), Some("1440"));
    assert_eq!(w_attr(margins, "left"), Some("1080"));
    assert_eq!(w_attr(margins, "right"), Some("1080"));
    let sect = all(&body, "sectPr")[0];
    let body_node = sect.parent().unwrap();
    assert_eq!(body_node.tag_name().name(), "body");
    assert_eq!(body_node.last_element_child(), Some(sect));
}

#[test]
fn black_document_background_round_trips_and_unsupported_backgrounds_warn() {
    let mut document = WriterDocument::new("black-background", "Black background");
    document.page.appearance = PageAppearance::Black;
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Default ink"));

    let exported = export_docx(&document).expect("export black background");
    let package = parts(&exported);
    let body = parse(&package["word/document.xml"]);
    let background = child(body.root_element(), "background").expect("document background");
    assert_eq!(w_attr(background, "color"), Some("000000"));
    assert!(
        package["word/styles.xml"].contains("<w:color w:val=\"FFFFFF\"/>"),
        "default run ink is white on black"
    );
    let imported = super::import_docx(&exported.bytes).expect("import black background");
    assert_eq!(imported.page.appearance, PageAppearance::Black);

    let unsupported = rewrite_document_xml(&exported, |xml| {
        xml.replace(
            "<w:background w:color=\"000000\"/>",
            "<w:background w:color=\"FF0000\"/>",
        )
    });
    let warnings = super::inspect_docx_unsupported(&unsupported).expect("inspect background");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("document background")),
        "unsupported backgrounds use the existing import warning path: {warnings:?}"
    );
    let imported = super::import_docx(&unsupported).expect("import unsupported background");
    assert_eq!(imported.page.appearance, PageAppearance::White);
}

#[test]
fn custom_page_geometry_exports_exact_twips() {
    let mut document = rich_document();
    document
        .page
        .set_custom_geometry_points(612.25, 792.5, 24.0, 30.0, 36.0, 42.0)
        .expect("valid custom page geometry");
    let export = export_docx(&document).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let size = all(&body, "pgSz")[0];
    assert_eq!(w_attr(size, "w"), Some("12245"));
    assert_eq!(w_attr(size, "h"), Some("15850"));
    assert_eq!(w_attr(size, "orient"), None);
    let margins = all(&body, "pgMar")[0];
    assert_eq!(w_attr(margins, "top"), Some("480"));
    assert_eq!(w_attr(margins, "bottom"), Some("600"));
    assert_eq!(w_attr(margins, "left"), Some("720"));
    assert_eq!(w_attr(margins, "right"), Some("840"));
}

#[test]
fn custom_page_geometry_round_trips_through_docx_import() {
    let mut document = rich_document();
    document
        .page
        .set_custom_geometry_points(612.25, 792.5, 24.0, 30.0, 36.0, 42.0)
        .expect("valid custom page geometry");

    let bytes = export_docx(&document).unwrap().bytes;
    let imported = super::import_docx(&bytes).expect("custom geometry imports");

    assert_eq!(imported.page, document.page);
}

#[test]
fn docx_import_rejects_malformed_or_section_specific_page_geometry() {
    let mut document = rich_document();
    document
        .page
        .set_custom_geometry_points(612.25, 792.5, 24.0, 30.0, 36.0, 42.0)
        .expect("valid custom page geometry");
    let export = export_docx(&document).unwrap();

    let malformed = rewrite_document_xml(&export, |xml| {
        xml.replace("w:w=\"12245\" w:h=\"15850\"", "w:w=\"0\" w:h=\"15850\"")
    });
    assert!(super::import_docx(&malformed)
        .unwrap_err()
        .contains("invalid DOCX page geometry"));

    let multiple_sections = rewrite_document_xml(&export, |xml| {
        xml.replacen(
            "</w:p>",
            "</w:p><w:p><w:pPr><w:sectPr><w:pgSz w:w=\"14400\" w:h=\"21600\"/><w:pgMar w:top=\"720\" w:bottom=\"720\" w:left=\"720\" w:right=\"720\"/></w:sectPr></w:pPr><w:r><w:t>Second section</w:t></w:r></w:p>",
            1,
        )
    });
    let error = super::import_docx(&multiple_sections).unwrap_err();
    assert!(error.contains("different page geometry"), "{error}");
}

#[test]
fn text_survives_escaping_unicode_tabs_and_breaks() {
    let doc = rich_document();
    let export = export_docx(&doc).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    let texts: Vec<String> = all(&body, "p").into_iter().map(paragraph_text).collect();
    for block in doc.blocks.iter().filter(|b| b.kind != TABLE_BLOCK_KIND) {
        assert!(
            texts.iter().any(|t| t == block.text.as_str()),
            "lost: {:?}",
            block.text.as_str()
        );
    }
    assert!(!parts["word/document.xml"].contains("<angle>"));
    let core = parse(&parts["docProps/core.xml"]);
    let title = core
        .descendants()
        .find(|n| n.tag_name().name() == "title")
        .unwrap();
    assert_eq!(title.text(), Some("Quarterly <Report> & Notes"));
}

#[test]
fn forbidden_xml_characters_are_dropped_not_written() {
    let mut doc = WriterDocument::new("ctl", "T\u{1}itle");
    doc.blocks
        .push(RichBlock::new(1, "paragraph", "a\u{0}b\u{8}c\u{FFFE}d"));
    let export = export_docx(&doc).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);
    assert_eq!(paragraph_text(all(&body, "p")[0]), "abcd");
    parse(&parts["docProps/core.xml"]);
}

#[test]
fn comments_are_anchored_and_resolved_state_is_kept() {
    let export = export_docx(&rich_document()).unwrap();
    assert_eq!((export.comments_exported, export.comments_skipped), (2, 1));
    let parts = parts(&export);
    let comments = parse(&parts["word/comments.xml"]);
    let list: Vec<(String, String, String)> = all(&comments, "comment")
        .into_iter()
        .map(|c| {
            (
                w_attr(c, "id").unwrap().to_string(),
                w_attr(c, "author").unwrap().to_string(),
                c.children()
                    .filter(|n| is_w(n, "p"))
                    .map(paragraph_text)
                    .collect::<Vec<_>>()
                    .join("|"),
            )
        })
        .collect();
    assert_eq!(
        list,
        vec![
            (
                "0".into(),
                "Ada Lovelace".into(),
                "Check this|second line".into()
            ),
            ("1".into(), "Bob".into(), "Open".into()),
        ]
    );
    let body = parse(&parts["word/document.xml"]);
    let p = all(&body, "p")
        .into_iter()
        .find(|p| paragraph_text(*p).starts_with("Plain bold"))
        .unwrap();
    let mut inside = String::new();
    let mut open = false;
    let mut referenced = false;
    for n in p.descendants() {
        match n.tag_name().name() {
            "commentRangeStart" => open = true,
            "commentRangeEnd" => open = false,
            "commentReference" => referenced = true,
            "t" if open => inside.push_str(n.text().unwrap_or("")),
            _ => {}
        }
    }
    assert_eq!(inside, "bold");
    assert!(referenced);
    let ext = parse(&parts["word/commentsExtended.xml"]);
    let done: Vec<&str> = ext
        .descendants()
        .filter(|n| n.tag_name().name() == "commentEx")
        .map(|n| n.attributes().find(|a| a.name() == "done").unwrap().value())
        .collect();
    assert_eq!(done, vec!["1", "0"]);
}

#[test]
fn docx_comments_round_trip_with_author_body_anchor_and_resolved_state() {
    let exported = export_docx(&rich_document()).expect("export comments");
    let imported = super::import_docx(&exported.bytes).expect("import comments");
    assert_eq!(
        imported.comments.len(),
        2,
        "orphaned comments stay unanchored"
    );
    let ada = imported
        .comments
        .iter()
        .find(|comment| comment.author == "Ada Lovelace")
        .expect("resolved Ada thread");
    assert_eq!(
        ada.id, "0",
        "the numeric OOXML comment identity is retained"
    );
    assert_eq!(ada.body, "Check this\nsecond line");
    assert!(ada.resolved);
    let block = imported
        .blocks
        .iter()
        .find(|block| block.id == ada.block_id)
        .expect("comment paragraph exists");
    assert_eq!(&block.text.as_str()[ada.start..ada.end], "bold");

    let bob = imported
        .comments
        .iter()
        .find(|comment| comment.author == "Bob")
        .expect("unresolved Bob thread");
    assert_eq!(bob.id, "1");
    assert_eq!(bob.body, "Open");
    assert!(!bob.resolved);
    let block = imported
        .blocks
        .iter()
        .find(|block| block.id == bob.block_id)
        .expect("comment paragraph exists");
    assert_eq!(&block.text.as_str()[bob.start..bob.end], "Centered");

    let reopened = super::import_docx(
        &export_docx(&imported)
            .expect("re-export imported comments")
            .bytes,
    )
    .expect("reopen comments");
    assert_eq!(reopened.comments, imported.comments);
}

#[test]
fn docx_dangling_comment_markers_and_records_are_rejected() {
    let exported = export_docx(&rich_document()).expect("export comment fixture");
    let missing_end = rewrite_document_xml(&exported, |xml| {
        xml.replacen("<w:commentRangeEnd w:id=\"0\"/>", "", 1)
    });
    let error = super::import_docx(&missing_end).expect_err("incomplete anchor is rejected");
    assert!(error.contains("incomplete range anchor"), "{error}");

    let missing_record = rewrite_package_part(&exported, "word/comments.xml", |xml| {
        xml.replacen("<w:comment w:id=\"0\"", "<w:comment w:id=\"99\"", 1)
    });
    let error =
        super::import_docx(&missing_record).expect_err("dangling comment record is rejected");
    assert!(error.contains("do not match"), "{error}");

    let missing_author = rewrite_package_part(&exported, "word/comments.xml", |xml| {
        xml.replacen(" w:author=\"Ada Lovelace\"", "", 1)
    });
    let error = super::import_docx(&missing_author).expect_err("malformed comment is rejected");
    assert!(error.contains("missing w:author"), "{error}");
}

#[test]
fn no_comments_means_no_comment_parts() {
    let mut doc = rich_document();
    doc.comments.clear();
    let export = export_docx(&doc).unwrap();
    let parts = parts(&export);
    assert!(!parts.contains_key("word/comments.xml"));
    assert!(!parts["[Content_Types].xml"].contains("comments"));
    assert!(!parts["word/_rels/document.xml.rels"].contains("comments"));
}

#[test]
fn imports_and_round_trips_unresolved_docx_text_and_format_revisions() {
    let mut archive = PackageArchive::new();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>left</w:t></w:r>
      <w:del w:id="0" w:author="Delete Reviewer"><w:r><w:delText>gone</w:delText></w:r></w:del>
      <w:ins w:id="7" w:author="Insert Reviewer"><w:r><w:t>new</w:t></w:r></w:ins>
      <w:r><w:rPr><w:b/><w:rPrChange w:id="8" w:author="Format Reviewer"><w:rPr/></w:rPrChange></w:rPr><w:t>bold</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let settings_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:trackRevisions/></w:settings>"#;
    archive
        .add("word/document.xml", document_xml.as_bytes().to_vec())
        .unwrap();
    archive
        .add("word/settings.xml", settings_xml.as_bytes().to_vec())
        .unwrap();

    let imported = super::import_docx(&archive.to_bytes().unwrap())
        .expect("import standard OOXML tracked changes");
    assert_eq!(imported.blocks[0].text.as_str(), "leftnewbold");
    assert!(imported.tracked_changes.recording());
    let revisions = imported.tracked_changes.revisions();
    assert_eq!(revisions.len(), 3);

    let deletion = revisions
        .iter()
        .find(|revision| revision.kind == RevisionKind::Deletion)
        .expect("unresolved deletion is retained");
    assert_eq!(deletion.id, 9, "reserved zero ID is normalized once");
    assert_eq!(deletion.author, "Delete Reviewer");
    assert_eq!(deletion.anchor.start, 4);
    assert_eq!(deletion.anchor.end, 4);
    assert_eq!(deletion.before.as_ref().unwrap().text, "gone");
    assert!(deletion.after.is_none());

    let insertion = revisions
        .iter()
        .find(|revision| revision.kind == RevisionKind::Insertion)
        .expect("unresolved insertion is retained");
    assert_eq!(insertion.id, 7);
    assert_eq!(insertion.author, "Insert Reviewer");
    assert_eq!(insertion.anchor.start, 4);
    assert_eq!(insertion.anchor.end, 7);
    assert!(insertion.before.is_none());
    assert_eq!(insertion.after.as_ref().unwrap().text, "new");

    let formatting = revisions
        .iter()
        .find(|revision| revision.kind == RevisionKind::Formatting)
        .expect("unresolved formatting change is retained");
    assert_eq!(formatting.id, 8);
    assert_eq!(formatting.author, "Format Reviewer");
    assert_eq!(formatting.before.as_ref().unwrap().text, "bold");
    assert!(formatting.before.as_ref().unwrap().runs.is_empty());
    assert_eq!(formatting.after.as_ref().unwrap().text, "bold");
    assert_eq!(formatting.after.as_ref().unwrap().runs.len(), 1);
    assert_eq!(
        formatting.after.as_ref().unwrap().runs[0].style.weight,
        FontWeight::Bold
    );

    let exported = export_docx(&imported).expect("export unresolved revisions");
    let parts = parts(&exported);
    assert!(parts["word/settings.xml"].contains("<w:trackRevisions/>"));
    let xml = parse(&parts["word/document.xml"]);
    let deleted = all(&xml, "del");
    assert_eq!(deleted.len(), 1);
    assert_eq!(w_attr(deleted[0], "id"), Some("9"));
    assert_eq!(w_attr(deleted[0], "author"), Some("Delete Reviewer"));
    assert_eq!(
        deleted[0]
            .descendants()
            .find(|node| is_w(node, "delText"))
            .and_then(|node| node.text()),
        Some("gone")
    );
    let inserted = all(&xml, "ins");
    assert_eq!(inserted.len(), 1);
    assert_eq!(w_attr(inserted[0], "id"), Some("7"));
    assert_eq!(w_attr(inserted[0], "author"), Some("Insert Reviewer"));
    let format_change = all(&xml, "rPrChange");
    assert_eq!(format_change.len(), 1);
    assert_eq!(w_attr(format_change[0], "id"), Some("8"));
    assert_eq!(w_attr(format_change[0], "author"), Some("Format Reviewer"));

    let reopened = super::import_docx(&exported.bytes).expect("reopen exported DOCX");
    assert_eq!(
        reopened.tracked_changes.revisions(),
        imported.tracked_changes.revisions(),
        "IDs, authors, anchors, and unresolved before/after payloads survive DOCX save/open"
    );
    assert_eq!(reopened.blocks[0].text.as_str(), "leftnewbold");
    assert!(reopened.tracked_changes.recording());
}

#[test]
fn block_level_revision_wrappers_are_rejected_instead_of_flattened() {
    let cases = [
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:ins w:id="1" w:author="Reviewer"><w:p><w:r><w:t>first</w:t></w:r></w:p><w:p><w:r><w:t>second</w:t></w:r></w:p></w:ins></w:body></w:document>"#,
            "block-level DOCX revision wrapper w:ins is not supported",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:del w:id="2" w:author="Reviewer"><w:p><w:r><w:delText>deleted</w:delText></w:r></w:p></w:del></w:body></w:document>"#,
            "block-level DOCX revision wrapper w:del is not supported",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:moveFrom w:id="3" w:author="Reviewer"><w:p><w:r><w:t>moved</w:t></w:r></w:p></w:moveFrom></w:body></w:document>"#,
            "DOCX move revision element w:moveFrom",
        ),
    ];
    for (xml, expected_error) in cases {
        let mut archive = PackageArchive::new();
        archive
            .add("word/document.xml", xml.as_bytes().to_vec())
            .unwrap();
        let error = super::import_docx(&archive.to_bytes().unwrap())
            .expect_err("unsupported block-level revision markup must not become plain text");
        assert!(
            error.contains(expected_error),
            "expected {expected_error:?} in error {error:?}"
        );
    }
}

#[test]
fn malformed_and_unrepresentable_docx_revisions_are_rejected() {
    let cases = [
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:ins w:id="1"><w:r><w:t>text</w:t></w:r></w:ins></w:p></w:body></w:document>"#,
            "missing w:author",
        ),
        (
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr><w:keepNext/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#,
            "tracked DOCX paragraph property w:keepNext is not supported",
        ),
    ];
    for (xml, expected_error) in cases {
        let mut archive = PackageArchive::new();
        archive
            .add("word/document.xml", xml.as_bytes().to_vec())
            .unwrap();
        let error = super::import_docx(&archive.to_bytes().unwrap())
            .expect_err("invalid or unrepresentable revision markup is not flattened");
        assert!(
            error.contains(expected_error),
            "expected {expected_error:?} in error {error:?}"
        );
    }
}

#[test]
fn export_is_deterministic_and_empty_document_is_valid() {
    let doc = rich_document();
    assert_eq!(
        export_docx(&doc).unwrap().bytes,
        export_docx(&doc).unwrap().bytes
    );
    let empty = WriterDocument::new("e", "");
    let export = export_docx(&empty).unwrap();
    let parts = parts(&export);
    for xml in parts.values() {
        parse(xml);
    }
    assert!(!all(&parse(&parts["word/document.xml"]), "sectPr").is_empty());
}

#[test]
fn documents_round_trip_through_the_importer() {
    let doc = rich_document();
    let blocks = crate::extract_docx_blocks(&export_docx(&doc).unwrap().bytes).unwrap();
    let kinds: Vec<&str> = blocks.iter().map(|b| b.kind.as_str()).collect();
    assert!(kinds.contains(&"heading1") && kinds.contains(&"heading6"));
    assert!(blocks.iter().any(|b| b.text.as_str() == "Overview"));
}

#[test]
fn a_chosen_family_is_written_for_every_script_slot_and_reads_back() {
    let mut doc = WriterDocument::new("families", "Families");
    let mut block = RichBlock::new(
        doc.next_id(),
        "paragraph",
        "Plain Georgia Courier Mono Sans",
    );
    let family = |name: &str| styled(|style| style.font_family = name.into());
    block.runs = vec![
        run(6, 13, family("Georgia")),
        run(14, 21, family("Courier Prime")),
        run(22, 26, family("Mono")),
        run(27, 31, family("Sans")),
    ];
    doc.push(block);
    let export = export_docx(&doc).unwrap();
    let parts = parts(&export);
    let body = parse(&parts["word/document.xml"]);

    // Read every authored text run back with the family it states.
    let mut read_back: Vec<(String, String)> = Vec::new();
    for r in all(&body, "r") {
        let text: String = r
            .children()
            .filter(|c| is_w(c, "t"))
            .filter_map(|c| c.text())
            .collect();
        if text.trim().is_empty() {
            continue;
        }
        let fonts = child(child(r, "rPr").expect("run properties"), "rFonts");
        if text.trim() == "Plain" {
            assert!(
                fonts.is_none(),
                "inherited text names no family and takes the document default"
            );
            continue;
        }
        let fonts = fonts.expect("an authored family is stated on its run");
        // All four slots name the same family, so Word uses it for any script.
        let ascii = w_attr(fonts, "ascii").expect("ascii");
        for slot in ["hAnsi", "eastAsia", "cs"] {
            assert_eq!(w_attr(fonts, slot), Some(ascii), "slot {slot} of {text:?}");
        }
        read_back.push((text.trim().to_owned(), ascii.to_owned()));
    }
    let pairs: Vec<(&str, &str)> = read_back
        .iter()
        .map(|(text, family)| (text.as_str(), family.as_str()))
        .collect();
    assert!(pairs.contains(&("Georgia", "Georgia")), "{pairs:?}");
    assert!(pairs.contains(&("Courier", "Courier Prime")), "{pairs:?}");
    // The generic names are written as the concrete Word faces.
    assert!(pairs.contains(&("Mono", "Courier New")), "{pairs:?}");
    assert!(pairs.contains(&("Sans", "Arial")), "{pairs:?}");
    assert!(!pairs.iter().any(|(text, _)| *text == "Plain"), "{pairs:?}");
    let styles = parse(&parts["word/styles.xml"]);
    let defaults = child(
        all(&styles, "rPrDefault")
            .first()
            .copied()
            .expect("document defaults"),
        "rPr",
    )
    .expect("default run properties");
    assert_eq!(
        child(defaults, "rFonts").and_then(|fonts| w_attr(fonts, "ascii")),
        Some("Arial"),
        "inherited text takes the Arial document default"
    );
}

/// Opt-in: writes the rich sample to `$LOOM_INTEROP_OUT/loom-writer-rich.docx`
/// so it can be opened in real Word.
#[test]
fn writes_interop_sample_when_requested() {
    let Ok(dir) = std::env::var("LOOM_INTEROP_OUT") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let export = export_docx(&rich_document()).unwrap();
    std::fs::write(dir.join("loom-writer-rich.docx"), export.bytes).unwrap();
}

#[test]
fn test_import_docx_rich_document_fidelity() {
    let source_doc = rich_document();
    let export = export_docx(&source_doc).expect("export succeeds");
    let imported = super::import_docx(&export.bytes).expect("import succeeds");

    assert_eq!(imported.title, "Quarterly <Report> & Notes");

    // Check headings
    let headings: Vec<(&str, &str)> = imported
        .blocks
        .iter()
        .filter(|b| b.kind.starts_with("heading"))
        .map(|b| (b.kind.as_str(), b.text.as_str()))
        .collect();
    assert!(headings.contains(&("heading1", "Overview")));
    assert!(headings.contains(&("heading2", "Details")));
    assert!(headings.contains(&("heading6", "Deepest")));

    // Check rich formatting runs in the styled paragraph
    let styled_block = imported
        .blocks
        .iter()
        .find(|b| b.text.as_str().starts_with("Plain bold italic"))
        .expect("styled paragraph present");
    assert!(!styled_block.runs.is_empty(), "must carry style runs");
    assert!(
        styled_block
            .runs
            .iter()
            .any(|r| r.style.weight == FontWeight::Bold),
        "must carry bold run"
    );
    assert!(
        styled_block.runs.iter().any(|r| r.style.italic),
        "must carry italic run"
    );
    assert!(
        styled_block.runs.iter().any(|r| r.style.underline),
        "must carry underline run"
    );
    assert!(
        styled_block.runs.iter().any(|r| r.style.strikethrough),
        "must carry strikethrough run"
    );

    // Check list blocks
    let bulleted: Vec<&str> = imported
        .blocks
        .iter()
        .filter(|b| b.kind == "list-bulleted")
        .map(|b| b.text.as_str())
        .collect();
    assert!(bulleted.contains(&"Apples"));
    assert!(bulleted.contains(&"Pears"));

    let numbered: Vec<&str> = imported
        .blocks
        .iter()
        .filter(|b| b.kind == "list-numbered")
        .map(|b| b.text.as_str())
        .collect();
    assert!(numbered.contains(&"First"));
    assert!(numbered.contains(&"Second"));

    // Check table import
    let table_block = imported
        .blocks
        .iter()
        .find(|b| b.kind == TABLE_BLOCK_KIND)
        .expect("table block must be imported");
    assert!(
        table_block.text.as_str().contains("Name"),
        "table must contain header column Name"
    );
    assert!(
        table_block.text.as_str().contains("Tea"),
        "table must contain cell Tea"
    );
}

#[test]
fn docx_warnings_name_only_what_is_lost() {
    let mut archive = PackageArchive::new();
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
    archive
        .add("[Content_Types].xml", content_types.as_bytes().to_vec())
        .unwrap();

    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>Header Text</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
      <w:r><w:t>List Item</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>Footnote ref</w:t><w:footnoteReference w:id="2"/><w:endnoteReference w:id="1"/></w:r>
    </w:p>
    <w:p>
      <w:r><w:drawing><w:pict/></w:drawing></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    archive
        .add("word/document.xml", doc_xml.as_bytes().to_vec())
        .unwrap();
    archive
        .add(
            "word/media/image1.png",
            vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        )
        .unwrap();
    archive
        .add("word/footnotes.xml", b"<w:footnotes/>".to_vec())
        .unwrap();
    archive
        .add("word/endnotes.xml", b"<w:endnotes/>".to_vec())
        .unwrap();

    let docx_bytes = archive.to_bytes().expect("pack test archive");
    let warnings = super::inspect_docx_unsupported(&docx_bytes).expect("inspect succeeds");

    let all = warnings.join(" | ").to_lowercase();
    // The PNG in the package is read now; the old-format `w:pict` still is not.
    assert!(
        all.contains("pictures in the older"),
        "old-format pictures are dropped: {warnings:?}"
    );
    assert!(
        all.contains("endnote"),
        "endnotes are still reported as unsupported: {warnings:?}"
    );
    assert!(
        !all.contains("footnote"),
        "supported footnotes are not reported as lost: {warnings:?}"
    );
    // Headings and flat lists import correctly, so warning about them is noise.
    assert!(!all.contains("heading"), "no heading warning: {warnings:?}");
    assert!(!all.contains("list"), "no flat-list warning: {warnings:?}");

    let nested = doc_xml.replace(r#"<w:ilvl w:val="0"/>"#, r#"<w:ilvl w:val="1"/>"#);
    let mut archive = PackageArchive::new();
    archive
        .add("[Content_Types].xml", content_types.as_bytes().to_vec())
        .unwrap();
    archive
        .add("word/document.xml", nested.into_bytes())
        .unwrap();
    let bytes = archive.to_bytes().expect("pack nested");
    // Nested levels import at their level now, so a nested list is not a loss.
    let nested_warnings = super::inspect_docx_unsupported(&bytes).expect("inspect");
    assert!(
        !nested_warnings.iter().any(|w| w.contains("list")),
        "{nested_warnings:?}"
    );
}

#[test]
fn docx_import_resolves_bullet_and_decimal_lists_from_numbering_xml() {
    let mut archive = PackageArchive::new();
    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
      <w:r><w:t>Outline Heading</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr>
      <w:r><w:t>Bullet Item</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="3"/></w:numPr></w:pPr>
      <w:r><w:t>Decimal Item</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let num_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="none"/></w:lvl></w:abstractNum>
  <w:abstractNum w:abstractNumId="2"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
  <w:abstractNum w:abstractNumId="3"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="2"/></w:num>
  <w:num w:numId="3"><w:abstractNumId w:val="3"/></w:num>
</w:numbering>"#;
    archive
        .add("word/document.xml", doc_xml.as_bytes().to_vec())
        .unwrap();
    archive
        .add("word/numbering.xml", num_xml.as_bytes().to_vec())
        .unwrap();
    let imported = super::import_docx(&archive.to_bytes().unwrap()).expect("import succeeds");
    assert_eq!(imported.blocks.len(), 3);
    assert_eq!(imported.blocks[0].kind, "heading1");
    assert_eq!(imported.blocks[1].kind, "list-bulleted");
    assert_eq!(imported.blocks[2].kind, "list-numbered");
}

#[test]
fn a_complex_script_bold_tag_alone_does_not_make_latin_text_bold() {
    // Word writes <w:bCs/> for bold in complex scripts. Without <w:b/> the Latin
    // text of the run is not bold, and <w:u/> is still read as underline.
    let mut archive = PackageArchive::new();
    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:bCs/></w:rPr><w:t xml:space="preserve">Complex only </w:t></w:r>
      <w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>under</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    archive
        .add("word/document.xml", doc_xml.as_bytes().to_vec())
        .unwrap();
    let imported = super::import_docx(&archive.to_bytes().unwrap()).expect("import succeeds");
    let runs = &imported.blocks[0].runs;
    assert!(
        runs.iter().all(|run| run.style.weight.numeric() < 700),
        "<w:bCs/> alone is complex-script bold, not Latin bold: {runs:?}"
    );
    assert!(
        runs.iter().any(|run| run.style.underline),
        "<w:u w:val=\"single\"/> is read as underline: {runs:?}"
    );
}

#[test]
fn a_soft_break_inside_a_table_cell_is_a_line_break_in_the_docx() {
    let mut document = WriterDocument::new("cell-break", "Cell break");
    let id = document.next_id();
    let mut table = loom_objects::Table::new(1, 1);
    table.set_cell_text(0, 0, "ab\u{b}cd");
    document.push(RichBlock::table_block(id, table));
    let export = export_docx(&document).unwrap();
    let xml = &parts(&export)["word/document.xml"];
    assert!(xml.contains("<w:br/>"), "the cell's line break is a w:br");
    assert!(
        !xml.contains('\u{b}'),
        "the vertical tab itself is not written"
    );
    assert!(
        xml.contains(">ab</w:t>") && xml.contains(">cd</w:t>"),
        "both halves of the cell are written: {xml}"
    );
}

#[test]
fn externally_authored_arial_survives_import_save_and_pdf_export() {
    // The fixture's inherited document font is Georgia; only the first run
    // declares Arial. The fixture Arial has double advances, so wrapping in
    // Arial is visibly narrower than wrapping in Writer's inherited font.
    let _fonts = crate::test_fonts::install(&[("Arial", 200), ("Georgia", 100)]);
    let bytes = committed_fixture("writer_external_arial.docx");
    let imported = super::import_docx(&bytes).expect("import the external Arial fixture");

    let arial_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("Arial stays Arial"))
        .expect("the Arial paragraph imports");
    assert_eq!(
        arial_block.runs.len(),
        1,
        "the Arial paragraph carries one explicit run: {:?}",
        arial_block.runs
    );
    assert_eq!(
        arial_block.runs[0].style.font_family, "Arial",
        "an explicit Arial declaration is authored text, not an inherited default"
    );
    let inherited_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("This paragraph inherits"))
        .expect("the inherited paragraph imports");
    assert!(
        inherited_block.runs.is_empty(),
        "text with no explicit run font keeps the inherited default: {:?}",
        inherited_block.runs
    );

    // Saving and reopening keeps the explicit family.
    let saved = export_docx(&imported).expect("export the imported Arial fixture");
    let saved_xml = parts(&saved)["word/document.xml"].clone();
    assert!(
        saved_xml.contains("w:ascii=\"Arial\""),
        "the export states the explicit Arial family: {saved_xml}"
    );
    let reopened = super::import_docx(&saved.bytes).expect("reopen the exported Arial document");
    let reopened_block = reopened
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("Arial stays Arial"))
        .expect("the Arial paragraph reopens");
    assert_eq!(
        reopened_block.runs[0].style.font_family, "Arial",
        "save and reopen keeps explicit Arial"
    );
    assert_eq!(
        reopened.blocks, imported.blocks,
        "the save/reopen round trip keeps the imported run styles exactly"
    );

    // Editor wrapping uses Arial, and reopening reproduces it exactly.
    let style = crate::PageStyle::default();
    let viewport = || crate::PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        ..crate::PageViewport::default()
    };
    let line_ranges = |doc: &WriterDocument, block_id: u64| {
        let block_index = doc
            .blocks
            .iter()
            .position(|block| block.id == block_id)
            .expect("the laid-out block exists");
        doc.flow(&style, viewport())
            .expect("layout the Arial document")
            .lines()
            .iter()
            .filter(|line| line.block_index == block_index)
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>()
    };
    let arial_lines = line_ranges(&imported, arial_block.id);
    assert!(
        arial_lines.len() > 1,
        "the fixture sentence wraps so the comparison is meaningful: {arial_lines:?}"
    );
    assert_eq!(
        line_ranges(&reopened, reopened_block.id),
        arial_lines,
        "save and reopen keeps Arial's line wrapping"
    );
    let mut control = WriterDocument::new("arial-control", "Arial control");
    control
        .blocks
        .push(RichBlock::new(1, "paragraph", arial_block.text.as_str()));
    assert_ne!(
        line_ranges(&control, control.blocks[0].id),
        arial_lines,
        "the Arial run wraps in Arial, not in Writer's inherited font"
    );

    // PDF export draws the run with the Arial face rather than the default.
    let pdf = crate::export_pdf(&reopened).expect("export the reopened Arial document");
    let faces = loom_pdf::inspect::embedded_fonts(&pdf)
        .expect("inspect the PDF fonts")
        .into_iter()
        .map(|font| font.base_font)
        .collect::<Vec<_>>();
    assert!(
        faces.iter().any(|face| face.ends_with("+Arial-Regular")),
        "the PDF embeds the authored Arial face: {faces:?}"
    );
}

#[test]
fn complex_script_only_formatting_is_disclosed_and_does_not_restyle_latin_text() {
    let cases: [(&str, &str, &[&str]); 2] = [
        (
            "writer_complex_script_arabic.docx",
            "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}",
            &["w:bCs", "w:szCs", "w:cs"],
        ),
        (
            "writer_complex_script_hebrew.docx",
            "\u{5e2}\u{5d1}\u{5e8}\u{5d9}\u{5ea}",
            &["w:iCs"],
        ),
    ];
    for (fixture, script_text, properties) in cases {
        let bytes = committed_fixture(fixture);
        let warnings = super::inspect_docx_unsupported(&bytes).expect("inspect the fixture");
        let joined = warnings.join(" | ").to_lowercase();
        assert!(
            joined.contains("complex-script"),
            "{fixture} discloses that complex-script formatting cannot be imported: {warnings:?}"
        );
        for property in properties {
            assert!(
                joined.contains(&property.to_lowercase()),
                "{fixture} names the dropped {property}: {warnings:?}"
            );
        }

        let imported = super::import_docx(&bytes).expect("import the complex-script fixture");
        let all_text = imported
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<String>();
        assert!(
            all_text.contains(script_text),
            "{fixture} imports the affected script text instead of dropping it"
        );
        let latin = imported
            .blocks
            .iter()
            .find(|block| block.text.as_str().starts_with("Latin neighbour"))
            .expect("the Latin neighbour imports");
        assert!(
            latin.runs.iter().all(
                |run| run.style.weight.numeric() < FontWeight::Bold.numeric() && !run.style.italic
            ),
            "{fixture} does not apply complex-script-only styling to its Latin run: {:?}",
            latin.runs
        );

        // The affected script text itself is imported plainly rather than
        // being given the Latin run's style or silently dropped.
        let script_blocks = imported
            .blocks
            .iter()
            .filter(|block| block.text.as_str().contains(script_text))
            .collect::<Vec<_>>();
        assert!(
            !script_blocks.is_empty(),
            "{fixture} keeps the script paragraphs"
        );
        for block in script_blocks {
            for run in &block.runs {
                assert!(
                    run.style.weight.numeric() < FontWeight::Bold.numeric(),
                    "{fixture} never invents complex-script bold: {:?}",
                    run.style
                );
            }
        }
    }

    // An explicit Latin italic with complex-script italic switched off keeps
    // the run's single italic style, which also covers the script text. The
    // import is not silent about that difference: the fixture's warning names
    // `w:iCs`, so the limitation is disclosed rather than accepted.
    let bytes = committed_fixture("writer_complex_script_hebrew.docx");
    let imported = super::import_docx(&bytes).expect("import the Hebrew fixture");
    let latin_italic = imported
        .blocks
        .iter()
        .find(|block| {
            block.text.as_str().contains('\u{5e2}') && block.runs.iter().any(|run| run.style.italic)
        })
        .expect("the w:i run imports its italic style");
    assert!(
        latin_italic
            .runs
            .iter()
            .any(|run| run.style.italic && run.style.font_family.is_empty()),
        "the run's w:i declaration still imports: {:?}",
        latin_italic.runs
    );
}
