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
    CommentThread, PageMarginsPreset, PageOrientation, PageSetup, PaperSize, RichBlock,
    WriterDocument, TABLE_BLOCK_KIND,
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

/// A document touching every feature the export claims to carry.
pub(super) fn rich_document() -> WriterDocument {
    let mut doc = WriterDocument::new("rich", "Quarterly <Report> & Notes");
    doc.page = PageSetup {
        paper: PaperSize::Letter,
        orientation: PageOrientation::Landscape,
        margins: PageMarginsPreset::Moderate,
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
    doc.blocks.push(RichBlock::new(
        12,
        TABLE_BLOCK_KIND,
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
