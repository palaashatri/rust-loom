//! The highlight Writer draws for a selection ends where the glyphs the page
//! draws end. Slint shapes the text with kerning; the selection rectangle comes
//! from Writer's own measurement, so the two agree only if that measurement
//! shapes the same way. A measurement that adds up unkerned glyph advances
//! leaves a highlight wider than the text by the kerning of the whole line.

use super::actions_tests::{test_state, text_document};
use super::*;

/// One rendered frame: RGBA bytes and size.
struct Frame {
    raw: Vec<u8>,
    width: u32,
    height: u32,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.raw[i],
            self.raw[i + 1],
            self.raw[i + 2],
            self.raw[i + 3],
        ]
    }

    fn luma(&self, x: u32, y: u32) -> u32 {
        let p = self.at(x, y);
        (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000
    }
}

const ZOOM: f32 = 2.0;

/// Render the page with the document's `selection` and the window at 1280x900.
fn render(document: &WriterDocument, selection: (usize, usize)) -> Frame {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::range(selection.0, selection.1));
    apply_layout_breakpoints(&app, 1280);
    app.invoke_page_zoom_changed(ZOOM);
    apply_state(&app, &state);
    let image = snapshot_component(&app, 1280.0, 900.0, 1.0).expect("render");
    let (width, height) = (image.width(), image.height());
    Frame {
        raw: image.into_raw(),
        width,
        height,
    }
}

/// Where the highlight ends and where the ink ends, in pixels, for a document
/// of one short line selected end to end.
fn highlight_and_ink_right_edges(document: &WriterDocument) -> (u32, u32) {
    let length = document.editor_text().len();
    let selected = render(document, (0, length));
    let plain = render(document, (0, 0));
    assert_eq!(
        (selected.width, selected.height),
        (plain.width, plain.height)
    );

    // Only the sheet of paper counts: the toolbar, inspector and status bar
    // change with the selection too. The sheet is the white span across the
    // middle row of the window.
    let row = plain.height / 2;
    let white = |x: u32| plain.at(x, row) == [255, 255, 255, 255];
    let left = (0..plain.width).find(|x| white(*x)).expect("paper");
    let right = (left..plain.width)
        .take_while(|x| white(*x))
        .last()
        .expect("paper");
    assert!(right - left > 400, "the sheet spans {left}..{right}");

    // The highlight is whatever changes between the two frames by more than
    // anti-aliasing does; the ink is what is dark in the unselected frame.
    let (mut highlight, mut ink) = (0u32, 0u32);
    // Below the toolbar and above the status bar, which spans the window.
    for y in 60..plain.height.saturating_sub(60) {
        for x in left..=right {
            let (a, b) = (selected.at(x, y), plain.at(x, y));
            let changed: i32 = (0..3)
                .map(|c| (i32::from(a[c]) - i32::from(b[c])).abs())
                .sum();
            if changed > 24 {
                highlight = highlight.max(x);
            }
            if plain.luma(x, y) < 110 {
                ink = ink.max(x);
            }
        }
    }
    assert!(
        highlight > 0 && ink > 0,
        "the line and its highlight are drawn"
    );
    (highlight, ink)
}

/// How much narrower than the sum of its glyphs the line is when kerned, in
/// pixels at the page zoom: what a measurement that ignores kerning would get
/// wrong.
fn kerning_pixels(text: &str, size_pt: f32) -> f32 {
    // A glyph shaped on its own has no neighbour to kern against.
    let natural: f32 = text
        .chars()
        .map(|ch| loom_writer_core::text_advance(&ch.to_string(), 0, &[], size_pt))
        .sum();
    (natural - loom_writer_core::text_advance(text, 0, &[], size_pt)) * ZOOM
}

#[test]
fn the_highlight_ends_where_a_kerned_line_of_text_ends() {
    let text = "AVATAR TAWA Toyota Wave";
    let document = text_document(text);
    let size = document.page.page_style().body_font_size_pt;
    let kerning = kerning_pixels(text, size);

    let (highlight, ink) = highlight_and_ink_right_edges(&document);
    let gap = highlight as f32 - ink as f32;
    eprintln!("kerned line: highlight ends {gap} px past the ink; kerning is {kerning} px");
    // The last glyph has a side bearing and edges are anti-aliased; an
    // unkerned highlight would overshoot by the whole kerning (several
    // pixels for this sample, checked below).
    assert!(
        (-2.0..=3.0).contains(&gap),
        "the highlight ends {gap} px past the ink (kerning is {kerning} px)"
    );
    assert!(
        kerning > 8.0,
        "the sample must kern by several pixels, got {kerning}"
    );
}

#[test]
fn the_highlight_follows_a_bold_run_inside_a_kerned_line() {
    let text = "AVATAR TAWA Toyota Wave";
    let mut document = text_document(text);
    // "TAWA Toyota" bold: kerned within the bold face, not across its edges.
    loom_writer_core::set_selection_bold(&mut document, TextSelection::range(7, 18), true);
    let (highlight, ink) = highlight_and_ink_right_edges(&document);
    let gap = highlight as f32 - ink as f32;
    eprintln!("kerned line with a bold run: highlight ends {gap} px past the ink");
    assert!(
        (-2.0..=3.0).contains(&gap),
        "the highlight ends {gap} px past the ink"
    );
}
