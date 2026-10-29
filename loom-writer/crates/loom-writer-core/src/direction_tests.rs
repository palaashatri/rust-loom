use loom_text::{Alignment, ParagraphDirection};

use crate::{load_document, save_document, RichBlock, WriterDocument, WRITER_CONTENT_FORMAT};

#[test]
fn current_format_round_trips_paragraph_direction_and_logical_alignment() {
    let mut document = WriterDocument::new("direction", "Direction");
    let mut rtl = RichBlock::new(1, "paragraph", "שלום world");
    rtl.style.alignment = Alignment::End;
    rtl.style.direction = ParagraphDirection::RightToLeft;
    document.blocks.push(rtl);
    let mut ltr = RichBlock::new(2, "paragraph", "Latin");
    ltr.style.alignment = Alignment::Left;
    ltr.style.direction = ParagraphDirection::LeftToRight;
    document.blocks.push(ltr);
    let mut auto = RichBlock::new(3, "paragraph", "Automatic");
    auto.style.alignment = Alignment::Start;
    auto.style.direction = ParagraphDirection::Auto;
    document.blocks.push(auto);
    let mut physical_right = RichBlock::new(4, "paragraph", "Physical right");
    physical_right.style.alignment = Alignment::Right;
    physical_right.style.direction = ParagraphDirection::LeftToRight;
    document.blocks.push(physical_right);

    let content = document.to_content_json();
    assert!(content.contains(&format!("\"format\":{WRITER_CONTENT_FORMAT}")));
    assert!(content.contains("\"alignment\":\"end\",\"direction\":\"rtl\""));
    assert!(content.contains("\"alignment\":\"left\",\"direction\":\"ltr\""));
    assert!(content.contains("\"alignment\":\"start\",\"direction\":\"auto\""));
    assert!(content.contains("\"alignment\":\"right\",\"direction\":\"ltr\""));
    let from_content = WriterDocument::from_content_json(&content).expect("parse current format");
    assert_eq!(from_content, document);

    let saved = save_document(&document).expect("save .loomdoc");
    let reopened = load_document(&saved).expect("reopen .loomdoc");
    assert_eq!(reopened, document);
}

#[test]
fn formats_one_and_two_default_missing_direction_to_auto() {
    let format_one = r#"{"id":"v1","title":"Legacy","blocks":[{"id":1,"kind":"paragraph","text":"Old","align":"right"}]}"#;
    let old = WriterDocument::from_content_json(format_one).expect("parse format 1");
    assert_eq!(old.blocks[0].style.alignment, Alignment::Right);
    assert_eq!(old.blocks[0].style.direction, ParagraphDirection::Auto);

    let format_two = r#"{"format":2,"id":"v2","title":"Legacy","blocks":[{"id":1,"kind":"paragraph","text":"Old","align":"right"}]}"#;
    let old = WriterDocument::from_content_json(format_two).expect("parse format 2");
    assert_eq!(old.blocks[0].style.alignment, Alignment::Right);
    assert_eq!(old.blocks[0].style.direction, ParagraphDirection::Auto);
}

#[test]
fn future_content_format_is_rejected() {
    let content = format!("{{\"format\":{},\"blocks\":[]}}", WRITER_CONTENT_FORMAT + 1);
    let error =
        WriterDocument::from_content_json(&content).expect_err("future content format is refused");
    assert!(error.to_string().contains("newer Writer"));
}

#[test]
fn unknown_paragraph_direction_is_rejected_without_guessing() {
    let content = r#"{"format":3,"id":"bad","title":"Bad","blocks":[{"id":1,"kind":"paragraph","text":"text","style":{"direction":"sideways"}}]}"#;
    let error = WriterDocument::from_content_json(content)
        .expect_err("unknown direction must not be silently treated as Auto");
    assert!(error.to_string().contains("unknown paragraph direction"));
}

#[test]
fn split_copies_direction_and_merge_keeps_the_first_paragraph_direction() {
    let mut document = WriterDocument::new("split-merge", "Split and merge");
    let mut block = RichBlock::new(1, "paragraph", "firstsecond");
    block.style.alignment = Alignment::Start;
    block.style.direction = ParagraphDirection::RightToLeft;
    document.blocks.push(block);

    let second_id = document.split_block(1, 5).expect("split paragraph");
    assert_eq!(
        document.blocks[0].style.direction,
        ParagraphDirection::RightToLeft
    );
    assert_eq!(
        document.blocks[1].style.direction,
        ParagraphDirection::RightToLeft
    );
    document.blocks[1].style.direction = ParagraphDirection::LeftToRight;
    document.blocks[1].style.alignment = Alignment::Left;
    document
        .merge_blocks(1, second_id)
        .expect("merge adjacent paragraphs");
    assert_eq!(document.blocks[0].style.alignment, Alignment::Start);
    assert_eq!(
        document.blocks[0].style.direction,
        ParagraphDirection::RightToLeft
    );
}

#[test]
fn cloned_document_snapshot_retains_direction() {
    let mut document = WriterDocument::new("snapshot", "Snapshot");
    let mut block = RichBlock::new(1, "paragraph", "body");
    block.style.direction = ParagraphDirection::RightToLeft;
    document.blocks.push(block);

    let snapshot = document.clone();
    document.blocks[0].style.direction = ParagraphDirection::LeftToRight;
    assert_eq!(
        snapshot.blocks[0].style.direction,
        ParagraphDirection::RightToLeft
    );
    assert_eq!(
        WriterDocument::from_content_json(&snapshot.to_content_json()).expect("restore snapshot"),
        snapshot
    );
}
