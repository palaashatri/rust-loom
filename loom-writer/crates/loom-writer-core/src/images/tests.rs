use super::*;
use crate::{PageStyle, PageViewport};

/// A solid PNG of the given size.
fn png(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([shade, 80, 160, 255]));
    pixels
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

fn picture(width: u32, height: u32, shade: u8) -> InlineImage {
    InlineImage::from_bytes("fixture.png", png(width, height, shade)).unwrap()
}

fn document_with_text() -> WriterDocument {
    let mut doc = WriterDocument::new("doc", "Doc");
    doc.blocks.push(RichBlock::new(1, "paragraph", "before"));
    doc.blocks.push(RichBlock::new(2, "paragraph", "after"));
    doc
}

#[test]
fn the_header_gives_size_and_garbage_is_refused() {
    let image = picture(40, 20, 9);
    assert_eq!((image.width_px, image.height_px), (40, 20));
    assert_eq!((image.width_pt, image.height_pt), (30.0, 15.0));
    assert_eq!(image.format, ImageFormat::Png);
    assert!(InlineImage::from_bytes("x", b"not an image".to_vec()).is_err());
    assert!(InlineImage::from_bytes("x", Vec::new()).is_err());
}

#[test]
fn base64_round_trips_every_length() {
    for len in 0..40usize {
        let bytes: Vec<u8> = (0..len).map(|n| (n * 37 + 11) as u8).collect();
        assert_eq!(
            base64_decode(&base64_encode(&bytes)).unwrap(),
            bytes,
            "{len}"
        );
    }
    assert!(base64_decode("not base64!").is_none());
}

#[test]
fn a_picture_is_one_block_that_survives_save_and_reopen() {
    let mut doc = document_with_text();
    let original = picture(64, 32, 7);
    let id = doc.insert_image_block(6, original.clone()).unwrap();
    let block = doc.blocks.iter().find(|b| b.id == id).unwrap();
    assert!(block.is_image());
    let reopened = WriterDocument::from_content_json(&doc.to_content_json()).unwrap();
    let back = reopened.blocks.iter().find(|b| b.id == id).unwrap();
    assert!(back.is_image());
    assert_eq!(back.image, Some(original), "bytes and size are saved");
    assert_eq!(reopened.editor_text(), doc.editor_text());
}

#[test]
fn a_picture_inserted_last_leaves_a_paragraph_to_type_in() {
    let mut doc = WriterDocument::new("doc", "Doc");
    doc.blocks.push(RichBlock::new(1, "paragraph", "only"));
    let end = doc.editor_text().len();
    doc.insert_image_block(end, picture(10, 10, 1)).unwrap();
    assert!(doc.blocks.last().unwrap().text.as_str().is_empty());
    assert_eq!(doc.blocks.len(), 3);
}

#[test]
fn the_picture_line_is_as_tall_as_the_picture_and_never_wider_than_the_text() {
    let style = PageStyle::default();
    let usable = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let mut doc = document_with_text();
    doc.insert_image_block(6, picture(4000, 1000, 3)).unwrap();
    let viewport = PageViewport {
        width: 900.0,
        height: 700.0,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let flow = doc.flow(&style, viewport).unwrap();
    let line = flow
        .lines()
        .iter()
        .find(|line| doc.blocks[line.block_index].is_image())
        .expect("the picture has a line");
    assert!(
        (line.bounds.width - usable).abs() < 0.01,
        "{}",
        line.bounds.width
    );
    assert!(
        (line.bounds.height - usable / 4.0).abs() < 0.01,
        "{}",
        line.bounds.height
    );
    // A text line is a fraction of that.
    let text_line = flow.lines().iter().find(|l| l.block_index == 0).unwrap();
    assert!(text_line.bounds.height < line.bounds.height / 4.0);
}

#[test]
fn a_tall_picture_moves_to_the_next_page_instead_of_overflowing() {
    let style = PageStyle::default();
    let usable_h = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
    let mut doc = WriterDocument::new("doc", "Doc");
    for id in 1..=3u64 {
        doc.blocks
            .push(RichBlock::new(id, "paragraph", &format!("paragraph {id}")));
    }
    // 60 percent of a page tall: two cannot share a page.
    let h = (usable_h * 0.6 / 0.75) as u32;
    doc.insert_image_block(0, picture(100, h, 1)).unwrap();
    let end = doc.editor_text().len();
    doc.insert_image_block(end, picture(100, h, 2)).unwrap();
    let pages = doc.paginate(&style).unwrap();
    let image_pages: Vec<usize> = pages
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.fragments.iter().any(|f| {
                doc.blocks
                    .iter()
                    .any(|b| b.id == f.block_id && b.is_image())
            })
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(image_pages, vec![0, 1], "one picture to a page");
}

#[test]
fn deleting_the_first_of_two_pictures_keeps_the_second_ones_bytes() {
    let mut doc = document_with_text();
    let first = picture(20, 20, 10);
    let second = picture(20, 20, 200);
    let a = doc.insert_image_block(6, first).unwrap();
    let b = doc
        .insert_image_block(doc.editor_text().len(), second.clone())
        .unwrap();
    assert_ne!(a, b);
    // Remove the first picture's two characters and its separator.
    let text = doc.editor_text();
    let start = text.find('\u{FFFC}').unwrap();
    let end = start + '\u{FFFC}'.len_utf8() + 3 + 1;
    let mut edited = text.clone();
    edited.replace_range(start..end, "");
    doc.replace_editor_text_at(&edited, Some(start)).unwrap();
    let images: Vec<_> = doc.blocks.iter().filter(|b| b.is_image()).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].image.as_ref(), Some(&second));
}

#[test]
fn typing_in_another_paragraph_keeps_every_picture() {
    let mut doc = document_with_text();
    let image = picture(30, 30, 99);
    doc.insert_image_block(6, image.clone()).unwrap();
    let mut text = doc.editor_text();
    text.insert_str(2, "XYZ");
    doc.replace_editor_text_at(&text, Some(5)).unwrap();
    let kept = doc.blocks.iter().find(|b| b.is_image()).unwrap();
    assert_eq!(kept.image.as_ref(), Some(&image));
}

#[test]
fn a_picture_exports_as_a_word_drawing_with_its_bytes_in_the_package() {
    let mut doc = document_with_text();
    let image = picture(120, 60, 42);
    doc.insert_image_block(6, image.clone()).unwrap();
    let export = crate::export_docx(&doc).unwrap();
    let archive = loom_package::zip::PackageArchive::from_bytes(&export.bytes).unwrap();
    assert_eq!(
        archive.get("word/media/image1.png").map(<[u8]>::to_vec),
        Some(image.bytes.to_vec()),
        "the original bytes are the media part"
    );
    let document = String::from_utf8(archive.get("word/document.xml").unwrap().to_vec()).unwrap();
    let parsed = roxmltree::Document::parse(&document).expect("well-formed document part");
    assert!(document.contains("r:embed=\"rIdImg1\""));
    assert!(document.contains("descr=\"fixture.png\""));
    // 120 x 60 px is 90 x 45 pt, 1,143,000 x 571,500 EMU.
    assert!(
        document.contains("cx=\"1143000\" cy=\"571500\""),
        "{document}"
    );
    assert!(parsed.descendants().any(|n| n.tag_name().name() == "blip"));
    let rels = String::from_utf8(
        archive
            .get("word/_rels/document.xml.rels")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(rels.contains("Id=\"rIdImg1\"") && rels.contains("media/image1.png"));
    let types = String::from_utf8(archive.get("[Content_Types].xml").unwrap().to_vec()).unwrap();
    assert!(types.contains("Extension=\"png\" ContentType=\"image/png\""));
    // The paragraphs around the picture are still there, in order.
    let before = document.find("before").unwrap();
    let drawing = document.find("<w:drawing>").unwrap();
    let after = document.find("after").unwrap();
    assert!(before < drawing && drawing < after);
}

#[test]
fn a_picture_exports_to_pdf_as_an_image_between_its_paragraphs() {
    let mut doc = document_with_text();
    doc.insert_image_block(6, picture(100, 50, 77)).unwrap();
    let with_image = crate::export_pdf(&doc);
    let text = String::from_utf8_lossy(&with_image);
    assert!(
        text.contains("/Subtype /Image"),
        "an image object is written"
    );
    assert!(text.contains("/Width 100 /Height 50"));
    let without = crate::export_pdf(&document_with_text());
    assert!(!String::from_utf8_lossy(&without).contains("/Subtype /Image"));
    assert!(with_image.len() > without.len());
}

#[test]
fn a_picture_is_not_a_word_and_reads_as_its_name_in_text_exports() {
    let mut doc = document_with_text();
    doc.insert_image_block(6, picture(10, 10, 5)).unwrap();
    let markdown = doc.to_markdown();
    assert!(markdown.contains("[Image: fixture.png]"), "{markdown}");
    assert!(
        !markdown.contains('\u{FFFC}')
            && !markdown
                .chars()
                .any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c))
    );
    assert!(doc.to_html_string().contains("<p>[Image: fixture.png]</p>"));
    assert_eq!(
        doc.estimate_pagination().words,
        2,
        "only `before` and `after` count"
    );
}
