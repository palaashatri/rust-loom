use loom_writer_core::{WriterDocument, RichBlock, TextSelection};
use loom_photo_core::{PhotoDocument, PhotoCanvas, PhotoSession, RgbaImage};
use loom_present_core::{PresentationDocument, PresentationSession, TransitionKind};
fn main() {
    let mut doc = WriterDocument::new("audit", "Audit document");
    for i in 1..=60 { doc.push(RichBlock::new(i, "paragraph", &format!("paragraph {i:02} unique content"))); }
    let pdf = loom_writer_core::export_pdf(&doc);
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/writer-60-paragraphs.pdf"), &pdf).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    println!("Writer PDF: input blocks={}, exported paragraph occurrences={}, contains final paragraph={}", doc.len(), text.matches("unique content").count(), text.contains("paragraph 60"));
    println!("Writer layout pages={}", doc.paginate(&doc.page.page_style()).unwrap().len());

    let mut commentdoc = WriterDocument::new("c", "Comment doc");
    commentdoc.push(RichBlock::new(1, "paragraph", "Hello world"));
    commentdoc.add_comment_thread(1, 6, 11, "Comment on world").unwrap();
    commentdoc.set_selection(TextSelection::caret(0));
    commentdoc.replace_selection_text("New ").unwrap();
    let thread = commentdoc.live_comment_threads()[0];
    let block = commentdoc.get(thread.block_id).unwrap();
    println!("Writer comment: text={:?}, anchor={}..{}, selected={:?}", block.text.as_str(),thread.start,thread.end,&block.text.as_str()[thread.start..thread.end]);

    let canvas = PhotoCanvas::new(PhotoDocument::new("p", "Photo", 2, 2)).unwrap();
    let mut session = PhotoSession::new(canvas);
    for _ in 0..2 { let n = session.canvas.document.layers.len()+1;session.add_pixel_layer(format!("layer-{n}"),format!("Pixel Layer {n}"));session.canvas.set_layer_image(&format!("layer-{n}"),RgbaImage::transparent(2,2).unwrap()).unwrap(); }
    session.remove_layer(1);
    let n = session.canvas.document.layers.len()+1;
    session.add_pixel_layer(format!("layer-{n}"),format!("Pixel Layer {n}"));
    session.canvas.set_layer_image(&format!("layer-{n}"),RgbaImage::transparent(2,2).unwrap()).unwrap();
    println!("Photo add/delete/add: ids={:?}; save result={:?}",session.canvas.document.layers.iter().map(|l| &l.id).collect::<Vec<_>>(),loom_photo_core::save_photo_canvas(&session.canvas).map(|b|b.len()));

    let mut deck = PresentationDocument::new("d", "Deck");
    deck.add_slide("Second", "content"); deck.add_slide("Third", "content");
    deck.remove_slide(1); deck.add_slide("Fourth", "content");
    println!("Present add/delete/add ids={:?}",deck.slides.iter().map(|s| &s.id).collect::<Vec<_>>());
    let mut presentation = PresentationSession::new(deck);
    let id = presentation.document.slides[0].id.clone();
    presentation.set_transition(&id,TransitionKind::Dissolve);
    let before = presentation.transition_for(&id);
    let undo = presentation.undo();
    println!("Present transition undo: before={:?} undo={} after={:?}",before,undo,presentation.transition_for(&id));
}
