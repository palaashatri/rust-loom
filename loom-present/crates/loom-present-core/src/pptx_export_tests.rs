use super::*;
use crate::{extract_pptx_titles, PresentationDocument, PresentationSession, Slide, SlideElement};
use std::collections::{BTreeSet, HashSet};

fn element(
    id: &str,
    kind: ElementType,
    content: &str,
    rect: (f32, f32, f32, f32),
    rotation: f32,
) -> SlideElement {
    SlideElement {
        id: id.to_string(),
        element_type: kind,
        content: content.to_string(),
        x: rect.0,
        y: rect.1,
        width: rect.2,
        height: rect.3,
        rotation_deg: rotation,
        action: None,
    }
}

/// A deck that exercises every element kind, notes, every transition and tricky text.
pub(crate) fn rich_session() -> PresentationSession {
    let mut document = PresentationDocument::new("deck", "Quarterly <Review> & Plan");
    document.author = "Ada \"Lovelace\"".to_string();
    document.slides.clear();

    let mut one = Slide::new("s1", "Welcome", "cover");
    one.add_element(element(
        "t1",
        ElementType::Title,
        "Q3 & Q4 <Plan>",
        (100.0, 200.0, 800.0, 100.0),
        0.0,
    ));
    one.add_element(element(
        "sub1",
        ElementType::Subtitle,
        "Prepared by \"Finance\"",
        (100.0, 320.0, 800.0, 50.0),
        0.0,
    ));
    one.speaker_notes = "Open warmly.\nMention the <budget> & the 'timeline'.".to_string();

    let mut two = Slide::new("s2", "Numbers", "content");
    two.add_element(element(
        "t2",
        ElementType::Title,
        "Results",
        (50.0, 30.0, 900.0, 80.0),
        0.0,
    ));
    two.add_element(element(
        "body2",
        ElementType::BodyText,
        "First line\nSecond line\n\nAfter a blank line",
        (60.0, 140.0, 500.0, 300.0),
        0.0,
    ));
    two.add_element(element(
        "rect2",
        ElementType::ShapeRectangle,
        "Box",
        (600.0, 140.0, 300.0, 120.0),
        15.0,
    ));
    two.add_element(element(
        "circle2",
        ElementType::ShapeCircle,
        "",
        (620.0, 300.0, 140.0, 140.0),
        -30.0,
    ));
    two.add_element(element(
        "stat2",
        ElementType::StatCard,
        "42%",
        (780.0, 300.0, 160.0, 120.0),
        0.0,
    ));
    two.bg_color = "#f4efe6".to_string();

    let mut three = Slide::new("s3", "Dark closer", "content");
    three.bg_color = "#16181d".to_string();
    three.add_element(element(
        "t3",
        ElementType::Title,
        "Thank you",
        (100.0, 220.0, 800.0, 100.0),
        0.0,
    ));
    // A second Title element must not become a second title placeholder.
    three.add_element(element(
        "t3b",
        ElementType::Title,
        "Questions?",
        (100.0, 340.0, 800.0, 60.0),
        0.0,
    ));

    // No Title element: the slide title is still carried, as in the PDF export.
    let four = Slide::new("s4", "Appendix", "blank");

    document.slides = vec![one, two, three, four];
    let mut session = PresentationSession::new(document);
    session.theme.accent = "#C9834B".to_string();
    assert!(session.set_transition("s1", TransitionKind::Dissolve));
    assert!(session.set_transition("s2", TransitionKind::Push));
    assert!(session.set_transition("s3", TransitionKind::Morph));
    session
}

struct Pptx {
    archive: PackageArchive,
}

impl Pptx {
    fn new(session: &PresentationSession) -> Self {
        let bytes = export_pptx(session).expect("export");
        Self {
            archive: PackageArchive::from_bytes(&bytes).expect("zip readable"),
        }
    }
    fn text(&self, path: &str) -> String {
        String::from_utf8(
            self.archive
                .get(path)
                .unwrap_or_else(|| panic!("missing part {path}"))
                .to_vec(),
        )
        .unwrap()
    }
    fn doc(&self, path: &str) -> roxmltree::Document<'static> {
        let text: &'static str = Box::leak(self.text(path).into_boxed_str());
        roxmltree::Document::parse(text).unwrap_or_else(|e| panic!("{path} is not XML: {e}"))
    }
}

fn names<'a>(
    doc: &'a roxmltree::Document<'static>,
    local: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'static>> + 'a {
    doc.descendants()
        .filter(move |n| n.is_element() && n.tag_name().name() == local)
}

fn resolve(base_part: &str, target: &str) -> String {
    let mut segments: Vec<&str> = base_part.split('/').collect();
    segments.pop();
    for piece in target.split('/') {
        match piece {
            ".." => {
                segments.pop();
            }
            "." | "" => {}
            other => segments.push(other),
        }
    }
    segments.join("/")
}

#[test]
fn every_part_is_well_formed_xml() {
    let pptx = Pptx::new(&rich_session());
    let paths: Vec<String> = pptx.archive.paths().iter().map(|p| p.to_string()).collect();
    assert!(paths.len() > 20, "{paths:?}");
    for path in &paths {
        let doc = pptx.doc(path);
        assert!(doc.root_element().is_element(), "{path}");
    }
}

#[test]
fn content_types_cover_every_part_exactly_once() {
    let pptx = Pptx::new(&rich_session());
    let types = pptx.doc("[Content_Types].xml");
    let mut overridden = BTreeSet::new();
    for node in names(&types, "Override") {
        let part = node.attribute("PartName").unwrap().trim_start_matches('/');
        assert!(overridden.insert(part.to_string()), "duplicate {part}");
        assert!(
            pptx.archive.get(part).is_some(),
            "override for missing {part}"
        );
    }
    for path in pptx.archive.paths() {
        if path == "[Content_Types].xml" || path.ends_with(".rels") {
            continue;
        }
        assert!(overridden.contains(path), "no content type for {path}");
    }
    assert!(pptx.text("[Content_Types].xml").contains("slide+xml"));
}

#[test]
fn relationships_resolve_and_ids_are_unique() {
    let pptx = Pptx::new(&rich_session());
    let mut checked = 0;
    for path in pptx.archive.paths() {
        if !path.ends_with(".rels") {
            continue;
        }
        let owner = if path == "_rels/.rels" {
            String::new()
        } else {
            let (dir, file) = path.rsplit_once("/_rels/").unwrap();
            format!("{dir}/{}", file.trim_end_matches(".rels"))
        };
        let doc = pptx.doc(path);
        let mut ids = HashSet::new();
        for rel in names(&doc, "Relationship") {
            assert!(
                ids.insert(rel.attribute("Id").unwrap().to_string()),
                "{path}"
            );
            let target = resolve(&owner, rel.attribute("Target").unwrap());
            assert!(pptx.archive.get(&target).is_some(), "{path} -> {target}");
            checked += 1;
        }
    }
    assert_eq!(checked, 24);
    // r:id references in presentation.xml all exist in its relationships.
    let rels = pptx.doc("ppt/_rels/presentation.xml.rels");
    let known: HashSet<String> = names(&rels, "Relationship")
        .map(|n| n.attribute("Id").unwrap().to_string())
        .collect();
    let presentation = pptx.doc("ppt/presentation.xml");
    for node in presentation.descendants().filter(|n| n.is_element()) {
        for attr in node
            .attributes()
            .filter(|a| a.name() == "id" && a.namespace().is_some())
        {
            assert!(known.contains(attr.value()), "{}", attr.value());
        }
    }
}

#[test]
fn slide_size_matches_the_authoring_plane() {
    assert_eq!(emu(SLIDE_WIDTH), 12_192_000);
    assert_eq!(emu(SLIDE_HEIGHT), 6_858_000);
    assert_eq!(emu(100.0), 1_219_200);
    assert_eq!(emu(f32::NAN), 0);
    let pptx = Pptx::new(&rich_session());
    let presentation = pptx.doc("ppt/presentation.xml");
    let size = names(&presentation, "sldSz").next().unwrap();
    assert_eq!(size.attribute("cx"), Some("12192000"));
    assert_eq!(size.attribute("cy"), Some("6858000"));
    assert_eq!(names(&presentation, "sldId").count(), 4);
}

#[test]
fn elements_keep_exact_position_size_and_rotation() {
    let pptx = Pptx::new(&rich_session());
    let slide = pptx.doc("ppt/slides/slide1.xml");
    let title = names(&slide, "sp")
        .find(|n| names_in(n, "cNvPr").any(|c| c.attribute("name") == Some("t1")))
        .unwrap();
    let off = names_in(&title, "off").next().unwrap();
    let ext = names_in(&title, "ext").next().unwrap();
    assert_eq!(off.attribute("x"), Some("1219200"));
    assert_eq!(off.attribute("y"), Some("2438400"));
    assert_eq!(ext.attribute("cx"), Some("9753600"));
    assert_eq!(ext.attribute("cy"), Some("1219200"));

    let slide = pptx.doc("ppt/slides/slide2.xml");
    let rect = shape_named(&slide, "rect2");
    assert_eq!(
        names_in(&rect, "xfrm").next().unwrap().attribute("rot"),
        Some("900000")
    );
    let circle = shape_named(&slide, "circle2");
    assert_eq!(
        names_in(&circle, "xfrm").next().unwrap().attribute("rot"),
        Some("19800000"),
        "-30 degrees normalizes to 330"
    );
    assert_eq!(
        names_in(&circle, "prstGeom")
            .next()
            .unwrap()
            .attribute("prst"),
        Some("ellipse")
    );
    assert_eq!(
        names_in(&rect, "prstGeom")
            .next()
            .unwrap()
            .attribute("prst"),
        Some("rect")
    );
    // Unrotated shapes carry no rot attribute.
    let body = shape_named(&slide, "body2");
    assert_eq!(
        names_in(&body, "xfrm").next().unwrap().attribute("rot"),
        None
    );
}

fn names_in<'a>(
    node: &roxmltree::Node<'a, 'static>,
    local: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'static>> + 'a {
    node.descendants()
        .filter(move |n| n.is_element() && n.tag_name().name() == local)
}

fn shape_named<'a>(
    doc: &'a roxmltree::Document<'static>,
    name: &str,
) -> roxmltree::Node<'a, 'static> {
    names(doc, "sp")
        .find(|n| names_in(n, "cNvPr").any(|c| c.attribute("name") == Some(name)))
        .unwrap_or_else(|| panic!("no shape {name}"))
}

#[test]
fn shape_ids_are_unique_per_slide() {
    let pptx = Pptx::new(&rich_session());
    for n in 1..=4 {
        let slide = pptx.doc(&format!("ppt/slides/slide{n}.xml"));
        let ids: Vec<&str> = names(&slide, "cNvPr")
            .map(|c| c.attribute("id").unwrap())
            .collect();
        let unique: HashSet<&&str> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "slide {n}: {ids:?}");
    }
}

#[test]
fn text_is_real_paragraphs_with_escaping_and_styles() {
    let pptx = Pptx::new(&rich_session());
    let slide = pptx.doc("ppt/slides/slide1.xml");
    let title = shape_named(&slide, "t1");
    let run_text: String = names_in(&title, "t")
        .map(|t| t.text().unwrap_or(""))
        .collect();
    assert_eq!(run_text, "Q3 & Q4 <Plan>");
    assert_eq!(
        names_in(&title, "ph").next().unwrap().attribute("type"),
        Some("title")
    );
    let rpr = names_in(&title, "rPr").next().unwrap();
    assert_eq!(rpr.attribute("b"), Some("1"));
    assert_eq!(rpr.attribute("sz"), Some("2743"));
    assert_eq!(
        names_in(&title, "pPr").next().unwrap().attribute("algn"),
        Some("ctr")
    );

    let slide2 = pptx.doc("ppt/slides/slide2.xml");
    let body = shape_named(&slide2, "body2");
    assert_eq!(names_in(&body, "p").count(), 4, "one paragraph per line");
    let sizes: HashSet<&str> = names_in(&body, "rPr")
        .filter_map(|r| r.attribute("sz"))
        .collect();
    assert_eq!(sizes, HashSet::from(["1600"]));
    assert_eq!(
        names_in(&body, "cNvSpPr")
            .next()
            .unwrap()
            .attribute("txBox"),
        Some("1")
    );
    assert!(
        names_in(&body, "ph").next().is_none(),
        "body is not a placeholder"
    );

    // Shapes carry fill and text.
    let rect = shape_named(&slide2, "rect2");
    let fill = names_in(&rect, "srgbClr").next().unwrap();
    assert_eq!(fill.attribute("val"), Some("E6F0FC"));
    let stat = shape_named(&slide2, "stat2");
    assert_eq!(names_in(&stat, "t").next().unwrap().text(), Some("42%"));
    assert_eq!(
        names_in(&stat, "srgbClr").next().unwrap().attribute("val"),
        Some("0071E3")
    );
}

#[test]
fn only_the_first_title_element_is_a_placeholder_and_titles_round_trip() {
    let session = rich_session();
    let pptx = Pptx::new(&session);
    let slide3 = pptx.doc("ppt/slides/slide3.xml");
    assert_eq!(names(&slide3, "ph").count(), 1);
    // Light text on the dark background, dark text on light ones.
    let colors: HashSet<&str> = names(&slide3, "rPr")
        .flat_map(|r| {
            names_in(&r, "srgbClr")
                .filter_map(|c| c.attribute("val"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(colors, HashSet::from(["F5F5F7"]));
    let slide3_bg = names(&slide3, "bgPr").next().unwrap();
    assert_eq!(
        names_in(&slide3_bg, "srgbClr")
            .next()
            .unwrap()
            .attribute("val"),
        Some("16181D")
    );

    let bytes = export_pptx(&session).unwrap();
    assert_eq!(
        extract_pptx_titles(&bytes).unwrap(),
        vec!["Q3 & Q4 <Plan>", "Results", "Thank you", "Appendix"]
    );
}

#[test]
fn notes_become_notes_slides_linked_both_ways() {
    let pptx = Pptx::new(&rich_session());
    let notes = pptx.doc("ppt/notesSlides/notesSlide1.xml");
    let body = names(&notes, "sp")
        .find(|n| names_in(n, "ph").any(|p| p.attribute("type") == Some("body")))
        .unwrap();
    let paragraphs: Vec<String> = names_in(&body, "p")
        .map(|p| names_in(&p, "t").map(|t| t.text().unwrap_or("")).collect())
        .collect();
    assert_eq!(
        paragraphs,
        vec!["Open warmly.", "Mention the <budget> & the 'timeline'."]
    );
    assert!(pptx
        .text("ppt/slides/_rels/slide1.xml.rels")
        .contains("notesSlide1.xml"));
    assert!(pptx
        .text("ppt/notesSlides/_rels/notesSlide1.xml.rels")
        .contains("../slides/slide1.xml"));
    // Slides without notes get no notes part.
    assert!(pptx
        .archive
        .get("ppt/notesSlides/notesSlide2.xml")
        .is_none());
}

#[test]
fn transitions_map_to_powerpoint_transitions() {
    let pptx = Pptx::new(&rich_session());
    let kind = |n: u32| {
        let slide = pptx.doc(&format!("ppt/slides/slide{n}.xml"));
        let found = names(&slide, "transition").next().map(|t| {
            t.children()
                .find(|c| c.is_element())
                .unwrap()
                .tag_name()
                .name()
                .to_string()
        });
        found
    };
    assert_eq!(kind(1).as_deref(), Some("fade"));
    assert_eq!(kind(2).as_deref(), Some("push"));
    assert_eq!(
        kind(3).as_deref(),
        Some("fade"),
        "Morph plays as a dissolve"
    );
    assert_eq!(kind(4), None);
    // The transition follows clrMapOvr, as the schema requires.
    let slide = pptx.doc("ppt/slides/slide1.xml");
    let order: Vec<&str> = slide
        .root_element()
        .children()
        .filter(|c| c.is_element())
        .map(|c| c.tag_name().name())
        .collect();
    assert_eq!(order, vec!["cSld", "clrMapOvr", "transition"]);
}

#[test]
fn metadata_and_theme_come_from_the_deck() {
    let pptx = Pptx::new(&rich_session());
    let core = pptx.doc("docProps/core.xml");
    let title = names(&core, "title").next().unwrap();
    assert_eq!(title.text(), Some("Quarterly <Review> & Plan"));
    assert_eq!(
        names(&core, "creator").next().unwrap().text(),
        Some("Ada \"Lovelace\"")
    );
    let app = pptx.doc("docProps/app.xml");
    assert_eq!(names(&app, "Slides").next().unwrap().text(), Some("4"));
    let theme = pptx.doc("ppt/theme/theme1.xml");
    let accent = names(&theme, "accent1").next().unwrap();
    assert_eq!(
        names_in(&accent, "srgbClr")
            .next()
            .unwrap()
            .attribute("val"),
        Some("C9834B")
    );
}

#[test]
fn control_characters_are_dropped_and_an_empty_deck_still_exports() {
    assert_eq!(esc("a\u{0}b\u{1}c\td"), "abc\td");
    let mut session = rich_session();
    session.document.slides[0].elements[0].content = "bell\u{7}ing".to_string();
    let pptx = Pptx::new(&session);
    let slide = pptx.doc("ppt/slides/slide1.xml");
    assert_eq!(names(&slide, "t").next().unwrap().text(), Some("belling"));

    session.document.slides.clear();
    let empty = Pptx::new(&session);
    let presentation = empty.doc("ppt/presentation.xml");
    assert_eq!(names(&presentation, "sldId").count(), 0);
}

/// Writes a sample deck for opening in real PowerPoint:
/// `LOOM_INTEROP_OUT=<dir> cargo test -p loom-present-core interop -- --nocapture`.
#[test]
fn interop_writes_a_sample_deck_when_asked() {
    let Ok(dir) = std::env::var("LOOM_INTEROP_OUT") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = export_pptx(&rich_session()).unwrap();
    let path = std::path::Path::new(&dir).join("loom-present-sample.pptx");
    std::fs::write(&path, bytes).unwrap();
    println!("wrote {}", path.display());
}
