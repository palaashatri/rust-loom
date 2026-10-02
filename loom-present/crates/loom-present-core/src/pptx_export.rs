//! Full-deck PowerPoint (`.pptx`) export.
//!
//! The deck's authoring plane (`SLIDE_WIDTH` x `SLIDE_HEIGHT`, 1000 x 562.5) is exactly a
//! 16:9 PowerPoint slide of 12,192,000 x 6,858,000 EMU, so one authoring unit is 12,192 EMU
//! and every element keeps its position, size and rotation. Text sizes follow the editor's
//! type scale (a 840 px wide slide drawn at 14 px body, 20 px heading, 24 px display text is
//! 960 pt wide in PowerPoint, so 1 px = 8/7 pt).
//!
//! Element mapping:
//! * the first `Title` element of a slide becomes the slide's title placeholder; further
//!   `Title` elements, `Subtitle` and `BodyText` become text boxes;
//! * `ShapeRectangle`, `ShapeCircle` (ellipse) and `StatCard` become real `p:sp` shapes
//!   carrying their text;
//! * speaker notes become notes slides, transitions map to PowerPoint transitions
//!   (`Dissolve` -> fade, `Push` -> push; `Morph` has no equivalent and plays as a
//!   dissolve in Loom, so it is written as fade).
//!
//! Element click actions and per-element styling that the model does not have are not
//! written.

use crate::{
    normalize_angle_degrees, ElementType, PresentationSession, Slide, SlideElement, TransitionKind,
    SLIDE_HEIGHT, SLIDE_WIDTH,
};
use loom_package::zip::PackageArchive;

const EMU_PER_UNIT: f64 = 12_192.0;
/// 1 editor px (on an 840 px wide slide) in hundredths of a point.
const HUNDREDTH_PT_PER_PX: f64 = 800.0 / 7.0;

const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.presentationml";
const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

const INK: &str = "1D1D1F";
const MUTED: &str = "6E6E73";
const LIGHT_INK: &str = "F5F5F7";
const RECT_FILL: &str = "E6F0FC";
const CIRCLE_FILL: &str = "E3F6EF";
const STAT_FILL: &str = "0071E3";

/// Converts authoring units to EMU.
pub(crate) fn emu(units: f32) -> i64 {
    if units.is_finite() {
        (f64::from(units) * EMU_PER_UNIT).round() as i64
    } else {
        0
    }
}

fn size_hundredths(px: f64) -> i64 {
    (px * HUNDREDTH_PT_PER_PX).round() as i64
}

/// Escapes text for XML content and attribute values and drops characters XML 1.0 forbids.
pub(crate) fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// `RRGGBB` for a `#rrggbb` (or `rrggbb`) colour, or `fallback`.
fn hex_color(value: &str, fallback: &str) -> String {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() == 6 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        trimmed.to_ascii_uppercase()
    } else {
        fallback.to_string()
    }
}

fn is_dark(rrggbb: &str) -> bool {
    let channel = |range: std::ops::Range<usize>| {
        f64::from(u8::from_str_radix(&rrggbb[range], 16).unwrap_or(255))
    };
    0.299 * channel(0..2) + 0.587 * channel(2..4) + 0.114 * channel(4..6) < 128.0
}

fn relationships(entries: &[(&str, &str, String)]) -> String {
    let mut xml = format!("{XML_DECL}<Relationships xmlns=\"{REL_NS}\">");
    for (id, kind, target) in entries {
        xml.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{REL_TYPE}/{kind}\" Target=\"{target}\"/>"
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

const GROUP_PROPS: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

/// Text properties of one paragraph run set.
struct TextSpec {
    size: i64,
    bold: bool,
    color: String,
    center: bool,
    heading_font: bool,
}

fn text_body(content: &str, spec: &TextSpec) -> String {
    let algn = if spec.center { "ctr" } else { "l" };
    let font = if spec.heading_font {
        "<a:latin typeface=\"+mj-lt\"/>"
    } else {
        ""
    };
    let rpr = format!(
        "sz=\"{}\" b=\"{}\" dirty=\"0\"",
        spec.size,
        u8::from(spec.bold)
    );
    let mut xml = String::from(
        "<p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\" anchor=\"ctr\"><a:noAutofit/></a:bodyPr><a:lstStyle/>",
    );
    let lines: Vec<&str> = if content.is_empty() {
        vec![""]
    } else {
        content.lines().collect()
    };
    for line in lines {
        xml.push_str(&format!("<a:p><a:pPr algn=\"{algn}\"/>"));
        if line.is_empty() {
            xml.push_str(&format!("<a:endParaRPr lang=\"en-US\" {rpr}/>"));
        } else {
            xml.push_str(&format!(
                "<a:r><a:rPr lang=\"en-US\" {rpr}><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>{font}</a:rPr><a:t>{}</a:t></a:r>",
                spec.color,
                esc(line)
            ));
        }
        xml.push_str("</a:p>");
    }
    xml.push_str("</p:txBody>");
    xml
}

fn xfrm(x: f32, y: f32, width: f32, height: f32, rotation_deg: f32) -> String {
    let rot =
        (f64::from(normalize_angle_degrees(rotation_deg)) * 60_000.0).round() as i64 % 21_600_000;
    let rot_attr = if rot == 0 {
        String::new()
    } else {
        format!(" rot=\"{rot}\"")
    };
    format!(
        "<a:xfrm{rot_attr}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        emu(x),
        emu(y),
        emu(width.max(0.0)),
        emu(height.max(0.0))
    )
}

/// Foreground colour for text drawn straight on the slide background.
fn slide_ink(background: &str, normal: &str) -> String {
    if is_dark(background) {
        LIGHT_INK.to_string()
    } else {
        normal.to_string()
    }
}

fn element_shape(id: u32, element: &SlideElement, background: &str, title_ph: bool) -> String {
    let name = esc(&element.id);
    let xf = xfrm(
        element.x,
        element.y,
        element.width,
        element.height,
        element.rotation_deg,
    );
    let heading = size_hundredths(24.0);
    let (geometry, fill, text) = match element.element_type {
        ElementType::Title => (
            "rect",
            None,
            TextSpec {
                size: heading,
                bold: true,
                color: slide_ink(background, INK),
                center: true,
                heading_font: true,
            },
        ),
        ElementType::Subtitle => (
            "rect",
            None,
            TextSpec {
                size: size_hundredths(14.0),
                bold: false,
                color: slide_ink(background, MUTED),
                center: false,
                heading_font: false,
            },
        ),
        ElementType::BodyText => (
            "rect",
            None,
            TextSpec {
                size: size_hundredths(14.0),
                bold: false,
                color: slide_ink(background, INK),
                center: false,
                heading_font: false,
            },
        ),
        ElementType::ShapeRectangle => (
            "rect",
            Some(RECT_FILL),
            TextSpec {
                size: size_hundredths(14.0),
                bold: false,
                color: INK.to_string(),
                center: false,
                heading_font: false,
            },
        ),
        ElementType::ShapeCircle => (
            "ellipse",
            Some(CIRCLE_FILL),
            TextSpec {
                size: size_hundredths(14.0),
                bold: false,
                color: INK.to_string(),
                center: false,
                heading_font: false,
            },
        ),
        ElementType::StatCard => (
            "rect",
            Some(STAT_FILL),
            TextSpec {
                size: size_hundredths(20.0),
                bold: true,
                color: "FFFFFF".to_string(),
                center: true,
                heading_font: false,
            },
        ),
    };
    let fill_xml = match fill {
        Some(color) => format!("<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>"),
        None => "<a:noFill/>".to_string(),
    };
    let (c_nv_sp_pr, nv_pr) = if title_ph {
        (
            "<p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>",
            "<p:nvPr><p:ph type=\"title\"/></p:nvPr>",
        )
    } else if fill.is_some() {
        ("<p:cNvSpPr/>", "<p:nvPr/>")
    } else {
        ("<p:cNvSpPr txBox=\"1\"/>", "<p:nvPr/>")
    };
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/>{c_nv_sp_pr}{nv_pr}</p:nvSpPr><p:spPr>{xf}<a:prstGeom prst=\"{geometry}\"><a:avLst/></a:prstGeom>{fill_xml}<a:ln><a:noFill/></a:ln></p:spPr>{}</p:sp>",
        text_body(&element.content, &text)
    )
}

/// Title placeholder for slides that carry a title but no `Title` element, matching the
/// PDF exporter's top-left fallback.
fn fallback_title_shape(id: u32, slide: &Slide, background: &str) -> String {
    let spec = TextSpec {
        size: size_hundredths(20.0),
        bold: true,
        color: slide_ink(background, INK),
        center: false,
        heading_font: true,
    };
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Slide title\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>{}</p:sp>",
        xfrm(40.0, 20.0, 920.0, 60.0, 0.0),
        text_body(&slide.title, &spec)
    )
}

/// PowerPoint transition element for a Loom transition, if one exists.
fn transition_xml(kind: &TransitionKind) -> &'static str {
    match kind {
        TransitionKind::None => "",
        // Morph has no PowerPoint counterpart and plays as a dissolve in Loom.
        TransitionKind::Dissolve | TransitionKind::Morph => {
            "<p:transition spd=\"fast\"><p:fade/></p:transition>"
        }
        // Loom's push brings the next slide in from the right edge.
        TransitionKind::Push => "<p:transition spd=\"fast\"><p:push dir=\"l\"/></p:transition>",
    }
}

fn slide_xml(slide: &Slide, transition: &TransitionKind) -> String {
    let background = hex_color(&slide.bg_color, "FFFFFF");
    let has_title = slide
        .elements
        .iter()
        .any(|element| element.element_type == ElementType::Title);
    let mut shapes = String::new();
    let mut next_id = 2u32;
    if !has_title && !slide.title.trim().is_empty() {
        shapes.push_str(&fallback_title_shape(next_id, slide, &background));
        next_id += 1;
    }
    let mut placeholder_used = false;
    for element in &slide.elements {
        let title_ph = element.element_type == ElementType::Title && !placeholder_used;
        placeholder_used |= title_ph;
        shapes.push_str(&element_shape(next_id, element, &background, title_ph));
        next_id += 1;
    }
    format!(
        "{XML_DECL}<p:sld {NS}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{background}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>{GROUP_PROPS}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{}</p:sld>",
        transition_xml(transition)
    )
}

fn notes_slide_xml(notes: &str) -> String {
    let spec = TextSpec {
        size: 1200,
        bold: false,
        color: INK.to_string(),
        center: false,
        heading_font: false,
    };
    let body = text_body(notes, &spec).replace(
        "<a:bodyPr wrap=\"square\" rtlCol=\"0\" anchor=\"ctr\"><a:noAutofit/></a:bodyPr>",
        "<a:bodyPr/>",
    );
    format!(
        "{XML_DECL}<p:notes {NS}><p:cSld><p:spTree>{GROUP_PROPS}<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image Placeholder 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes Placeholder 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/>{body}</p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"
    )
}

const CLR_MAP: &str = "<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>";

fn placeholder_sp(id: u32, name: &str, ph: &str, geometry: &str, text: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr>{geometry}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"
    )
}

fn master_xml() -> String {
    let title = placeholder_sp(
        2,
        "Title Placeholder 1",
        "type=\"title\"",
        &xfrm(40.0, 20.0, 920.0, 60.0, 0.0),
        "Click to edit Master title style",
    );
    let body = placeholder_sp(
        3,
        "Text Placeholder 2",
        "type=\"body\" idx=\"1\"",
        &xfrm(40.0, 110.0, 920.0, 400.0, 0.0),
        "Click to edit Master text styles",
    );
    let level = |tag: &str, size: i64, bold: bool, font: &str| {
        format!(
            "<a:{tag} algn=\"l\"><a:defRPr sz=\"{size}\" b=\"{}\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+{font}-lt\"/><a:ea typeface=\"+{font}-ea\"/><a:cs typeface=\"+{font}-cs\"/></a:defRPr></a:{tag}>",
            u8::from(bold)
        )
    };
    format!(
        "{XML_DECL}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}{title}{body}</p:spTree></p:cSld>{CLR_MAP}<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle>{}</p:titleStyle><p:bodyStyle>{}</p:bodyStyle><p:otherStyle>{}</p:otherStyle></p:txStyles></p:sldMaster>",
        level("lvl1pPr", size_hundredths(24.0), true, "mj"),
        level("lvl1pPr", size_hundredths(14.0), false, "mn"),
        level("lvl1pPr", size_hundredths(14.0), false, "mn"),
    )
}

fn layout_xml() -> String {
    let title = placeholder_sp(
        2,
        "Title 1",
        "type=\"title\"",
        &xfrm(40.0, 20.0, 920.0, 60.0, 0.0),
        "Click to edit Master title style",
    );
    format!(
        "{XML_DECL}<p:sldLayout {NS} type=\"titleOnly\" preserve=\"1\"><p:cSld name=\"Title Only\"><p:spTree>{GROUP_PROPS}{title}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
    )
}

fn notes_master_xml() -> String {
    format!(
        "{XML_DECL}<p:notesMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image Placeholder 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\" idx=\"2\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"685800\" y=\"1143000\"/><a:ext cx=\"5486400\" cy=\"3086100\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln w=\"12700\"><a:solidFill><a:prstClr val=\"black\"/></a:solidFill></a:ln></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes Placeholder 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" sz=\"quarter\" idx=\"3\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"685800\" y=\"4400550\"/><a:ext cx=\"5486400\" cy=\"3600450\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Click to edit Master text styles</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld>{CLR_MAP}</p:notesMaster>"
    )
}

fn theme_xml(name: &str, accent: &str, heading_font: &str, body_font: &str) -> String {
    let fills = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>".repeat(3);
    let lines: String = [6350, 12700, 19050]
        .iter()
        .map(|w| format!("<a:ln w=\"{w}\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>"))
        .collect();
    let effects = "<a:effectStyle><a:effectLst/></a:effectStyle>".repeat(3);
    let (name, heading, body) = (esc(name), esc(heading_font), esc(body_font));
    format!(
        "{XML_DECL}<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"{name}\"><a:themeElements><a:clrScheme name=\"{name}\"><a:dk1><a:srgbClr val=\"{INK}\"/></a:dk1><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk2><a:srgbClr val=\"{MUTED}\"/></a:dk2><a:lt2><a:srgbClr val=\"E5E5EA\"/></a:lt2><a:accent1><a:srgbClr val=\"{accent}\"/></a:accent1><a:accent2><a:srgbClr val=\"1A8F6E\"/></a:accent2><a:accent3><a:srgbClr val=\"{STAT_FILL}\"/></a:accent3><a:accent4><a:srgbClr val=\"E6F0FC\"/></a:accent4><a:accent5><a:srgbClr val=\"E3F6EF\"/></a:accent5><a:accent6><a:srgbClr val=\"6E6E73\"/></a:accent6><a:hlink><a:srgbClr val=\"{STAT_FILL}\"/></a:hlink><a:folHlink><a:srgbClr val=\"6E6E73\"/></a:folHlink></a:clrScheme><a:fontScheme name=\"{name}\"><a:majorFont><a:latin typeface=\"{heading}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"{body}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"{name}\"><a:fillStyleLst>{fills}</a:fillStyleLst><a:lnStyleLst>{lines}</a:lnStyleLst><a:effectStyleLst>{effects}</a:effectStyleLst><a:bgFillStyleLst>{fills}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"
    )
}

/// Exports the whole deck, with its theme, notes and transitions, to a `.pptx` archive.
pub fn export_pptx(session: &PresentationSession) -> Result<Vec<u8>, String> {
    let document = &session.document;
    let theme = &session.theme;
    let accent = hex_color(&theme.accent, "0071E3");
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut add = |path: &str, content_type: Option<String>, xml: String| {
        if let Some(ct) = content_type {
            overrides.push((format!("/{path}"), ct));
        }
        parts.push((path.to_string(), xml));
    };

    let mut pres_rels: Vec<(&str, &str, String)> = vec![
        (
            "rId1",
            "slideMaster",
            "slideMasters/slideMaster1.xml".into(),
        ),
        (
            "rId2",
            "notesMaster",
            "notesMasters/notesMaster1.xml".into(),
        ),
        ("rId3", "theme", "theme/theme1.xml".into()),
        ("rId4", "presProps", "presProps.xml".into()),
        ("rId5", "viewProps", "viewProps.xml".into()),
        ("rId6", "tableStyles", "tableStyles.xml".into()),
    ];
    let slide_ids: Vec<String> = (0..document.slides.len())
        .map(|i| format!("rId{}", i + 7))
        .collect();
    let mut slide_list = String::new();
    for (index, rid) in slide_ids.iter().enumerate() {
        slide_list.push_str(&format!("<p:sldId id=\"{}\" r:id=\"{rid}\"/>", 256 + index));
    }
    for (index, rid) in slide_ids.iter().enumerate() {
        pres_rels.push((rid, "slide", format!("slides/slide{}.xml", index + 1)));
    }
    let slide_list = if slide_list.is_empty() {
        String::new()
    } else {
        format!("<p:sldIdLst>{slide_list}</p:sldIdLst>")
    };
    let presentation = format!(
        "{XML_DECL}<p:presentation {NS} saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:notesMasterIdLst><p:notesMasterId r:id=\"rId2\"/></p:notesMasterIdLst>{slide_list}<p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
        emu(SLIDE_WIDTH),
        emu(SLIDE_HEIGHT)
    );
    add(
        "ppt/presentation.xml",
        Some(format!("{CT}.presentation.main+xml")),
        presentation,
    );
    add(
        "ppt/_rels/presentation.xml.rels",
        None,
        relationships(&pres_rels),
    );
    add(
        "ppt/slideMasters/slideMaster1.xml",
        Some(format!("{CT}.slideMaster+xml")),
        master_xml(),
    );
    add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        None,
        relationships(&[
            (
                "rId1",
                "slideLayout",
                "../slideLayouts/slideLayout1.xml".into(),
            ),
            ("rId2", "theme", "../theme/theme1.xml".into()),
        ]),
    );
    add(
        "ppt/slideLayouts/slideLayout1.xml",
        Some(format!("{CT}.slideLayout+xml")),
        layout_xml(),
    );
    add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        None,
        relationships(&[(
            "rId1",
            "slideMaster",
            "../slideMasters/slideMaster1.xml".into(),
        )]),
    );
    let theme_ct = "application/vnd.openxmlformats-officedocument.theme+xml".to_string();
    add(
        "ppt/theme/theme1.xml",
        Some(theme_ct.clone()),
        theme_xml(&theme.name, &accent, &theme.heading_font, &theme.body_font),
    );
    add(
        "ppt/theme/theme2.xml",
        Some(theme_ct),
        theme_xml(&theme.name, &accent, &theme.heading_font, &theme.body_font),
    );
    add(
        "ppt/notesMasters/notesMaster1.xml",
        Some(format!("{CT}.notesMaster+xml")),
        notes_master_xml(),
    );
    add(
        "ppt/notesMasters/_rels/notesMaster1.xml.rels",
        None,
        relationships(&[("rId1", "theme", "../theme/theme2.xml".into())]),
    );
    add(
        "ppt/presProps.xml",
        Some(format!("{CT}.presProps+xml")),
        format!("{XML_DECL}<p:presentationPr {NS}/>"),
    );
    add(
        "ppt/viewProps.xml",
        Some(format!("{CT}.viewProps+xml")),
        format!("{XML_DECL}<p:viewPr {NS}/>"),
    );
    add(
        "ppt/tableStyles.xml",
        Some(format!("{CT}.tableStyles+xml")),
        format!("{XML_DECL}<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>"),
    );

    for (index, slide) in document.slides.iter().enumerate() {
        let number = index + 1;
        let transition = session
            .transitions
            .get(&slide.id)
            .cloned()
            .unwrap_or_default();
        add(
            &format!("ppt/slides/slide{number}.xml"),
            Some(format!("{CT}.slide+xml")),
            slide_xml(slide, &transition),
        );
        let mut rels = vec![(
            "rId1",
            "slideLayout",
            "../slideLayouts/slideLayout1.xml".to_string(),
        )];
        if !slide.speaker_notes.trim().is_empty() {
            rels.push((
                "rId2",
                "notesSlide",
                format!("../notesSlides/notesSlide{number}.xml"),
            ));
            add(
                &format!("ppt/notesSlides/notesSlide{number}.xml"),
                Some(format!("{CT}.notesSlide+xml")),
                notes_slide_xml(&slide.speaker_notes),
            );
            add(
                &format!("ppt/notesSlides/_rels/notesSlide{number}.xml.rels"),
                None,
                relationships(&[
                    (
                        "rId1",
                        "notesMaster",
                        "../notesMasters/notesMaster1.xml".into(),
                    ),
                    ("rId2", "slide", format!("../slides/slide{number}.xml")),
                ]),
            );
        }
        add(
            &format!("ppt/slides/_rels/slide{number}.xml.rels"),
            None,
            relationships(&rels),
        );
    }

    add(
        "docProps/core.xml",
        Some("application/vnd.openxmlformats-package.core-properties+xml".into()),
        format!(
            "{XML_DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
            esc(&document.title),
            esc(&document.author)
        ),
    );
    add(
        "docProps/app.xml",
        Some("application/vnd.openxmlformats-officedocument.extended-properties+xml".into()),
        format!(
            "{XML_DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Loom Present</Application><Slides>{}</Slides><PresentationFormat>Widescreen</PresentationFormat></Properties>",
            document.slides.len()
        ),
    );

    let mut content_types = format!(
        "{XML_DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>"
    );
    for (part, ct) in &overrides {
        content_types.push_str(&format!(
            "<Override PartName=\"{part}\" ContentType=\"{ct}\"/>"
        ));
    }
    content_types.push_str("</Types>");

    let root_rels = relationships(&[
        ("rId1", "officeDocument", "ppt/presentation.xml".into()),
        ("rId2", "extended-properties", "docProps/app.xml".into()),
    ])
    .replace(
        "</Relationships>",
        "<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>",
    );

    let mut archive = PackageArchive::new();
    let mut put = |path: &str, xml: &str| {
        archive
            .add(path, xml.as_bytes().to_vec())
            .map_err(|e| format!("pptx export failed: {e}"))
    };
    put("[Content_Types].xml", &content_types)?;
    put("_rels/.rels", &root_rels)?;
    for (path, xml) in &parts {
        put(path, xml)?;
    }
    archive
        .to_bytes()
        .map_err(|e| format!("pptx export failed: {e}"))
}

#[cfg(test)]
#[path = "pptx_export_tests.rs"]
mod tests;
