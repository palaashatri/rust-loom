//! Opt-in performance measurements on representative long documents: the real
//! edit, scroll, find, open/save and export paths plus a software-rendered
//! window frame. `LOOM_FRAME_BENCH=1` prints numbers; `LOOM_FRAME_BENCH=enforce`
//! also fails when a budget is exceeded.

use super::actions_tests::{test_state, text_document};
use super::*;
use std::time::Instant;

const KEYSTROKE_BUDGET_100_MS: f64 = 16.7;
const KEYSTROKE_BUDGET_400_MS: f64 = 33.0;
const RENDER_BUDGET_MS: f64 = 45.0;
const OPEN_BUDGET_400_MS: f64 = 2000.0;
/// The software renderer panics once content passes 32,767 px, and the editor's
/// text input holds the whole document, so past about this many pages a
/// software-rendered frame cannot be captured. That is a limit of the capture
/// harness (the GPU renderer is not affected); windowing the editor text is a
/// separate redesign. Long-document render timing is therefore skipped, not
/// counted as a failure.
const SOFTWARE_RENDER_MAX_PAGES: usize = 30;

fn pct(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1e3
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(f64::total_cmp);
    (pct(&v, 0.5), pct(&v, 0.95), v[v.len() - 1])
}

fn peak_working_set_mb() -> f64 {
    let pid = std::process::id();
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {pid}).PeakWorkingSet64"),
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map_or(f64::NAN, |b| b / 1048576.0)
}

fn paragraph(n: usize) -> String {
    format!(
        "Paragraph {n} lorem ipsum dolor sit amet, consectetur dolor adipiscing elit sed do \
         eiusmod tempor incididunt ut labore et dolore magna aliqua."
    )
}

/// A document of roughly `pages` pages: paragraphs, a heading every 25th, a
/// bold run in every 7th and a comment on every 200th paragraph.
pub(super) fn build_document(pages: usize) -> WriterDocument {
    let paragraphs = pages * 20;
    let text = (0..paragraphs)
        .map(paragraph)
        .collect::<Vec<_>>()
        .join("\n");
    let mut doc = text_document(&text);
    let mut offset = 0usize;
    let mut starts = Vec::with_capacity(paragraphs);
    for n in 0..paragraphs {
        starts.push(offset);
        offset += paragraph(n).len() + 1;
    }
    for (n, &start) in starts.iter().enumerate() {
        if n % 25 == 0 {
            loom_writer_core::set_selection_heading(&mut doc, TextSelection::caret(start), 1);
        }
        if n % 7 == 0 {
            loom_writer_core::set_selection_bold(
                &mut doc,
                TextSelection::range(start + 10, start + 25),
                true,
            );
        }
    }
    for n in (0..paragraphs).step_by(200) {
        let id = doc.blocks[n].id;
        doc.add_comment_thread(id, 0, 9, "bench comment")
            .expect("comment");
    }
    doc
}

pub(super) fn setup(doc: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_state(&app, &state);
    (app, state)
}

/// Render one frame; `None` when Slint's software renderer panics (its glyph
/// coordinates are 16-bit, so text laid out beyond 32,767 px overflows).
fn try_render(app: &WriterApp) -> Option<f64> {
    if !RENDER_SUPPORTED.with(std::cell::Cell::get) {
        return None;
    }
    let t = Instant::now();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot_component(app, 1280.0, 800.0, 1.0).expect("frame");
    }))
    .ok()
    .map(|()| ms(t))
}

fn bench_size(pages: usize, mode: &str) {
    let built = Instant::now();
    let doc = build_document(pages);
    let build_ms = ms(built);
    let real_pages = doc
        .paginate(&doc.page.page_style())
        .map(|p| p.len())
        .unwrap_or(0);
    let render_supported = real_pages <= SOFTWARE_RENDER_MAX_PAGES;
    RENDER_SUPPORTED.with(|flag| flag.set(render_supported));

    // Layout/paginate and first render.
    let t = Instant::now();
    let (app, state) = setup(doc);
    let apply_ms = ms(t);
    let first_render_ms = try_render(&app).unwrap_or(f64::NAN);
    let t = Instant::now();
    {
        let current = state.current.borrow();
        let _ = current.paginate(&current.page.page_style());
    }
    let paginate_ms = ms(t);

    // Keystrokes in the middle of the document through the real edit callback.
    let base = state.current.borrow().editor_text();
    let mid = {
        let mut i = base.len() / 2;
        while !base.is_char_boundary(i) || base.as_bytes()[i] == b'\n' {
            i += 1;
        }
        i
    };
    let mut text = base.clone();
    let mut keys = Vec::new();
    for k in 0..200usize {
        let at = mid + k;
        text.insert(at, 'x');
        let caret = (at + 1) as i32;
        let t = Instant::now();
        app.invoke_document_edited(text.as_str().into(), caret, caret);
        keys.push(ms(t));
    }
    let (k50, k95, kmax) = stats(keys);
    assert_eq!(state.current.borrow().editor_text(), text, "edits applied");

    // Scroll callback + software-rendered frame.
    let mut scroll = Vec::new();
    let mut render = Vec::new();
    let mut render_panics = 0;
    for step in 0..60u32 {
        let y = 120.0 + step as f32 * 900.0;
        let t = Instant::now();
        app.invoke_page_scroll_changed(0.0, y);
        scroll.push(ms(t));
        match try_render(&app) {
            Some(t) => render.push(t),
            None if render_supported => render_panics += 1,
            None => {}
        }
    }
    let (s50, s95, smax) = stats(scroll);
    let (r50, r95, rmax) = if render.is_empty() {
        (f64::NAN, f64::NAN, f64::NAN)
    } else {
        stats(render)
    };

    // Open and save.
    let dir = std::env::temp_dir().join(format!("loom-writer-bench-{pages}"));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("bench.loomdoc");
    let current = state.current.borrow().clone();
    let t = Instant::now();
    save_file(&path, &current).expect("save");
    let save_ms = ms(t);
    let t = Instant::now();
    let loaded = load_file(&path).expect("open");
    let open_ms = ms(t);
    let size_kb = std::fs::metadata(&path)
        .map(|m| m.len() / 1024)
        .unwrap_or(0);
    assert_eq!(loaded.editor_text(), current.editor_text());
    let t = Instant::now();
    let _opened = setup(loaded);
    let open_apply_ms = ms(t);

    // PDF export.
    let t = Instant::now();
    let pdf = loom_writer_core::export_pdf(&current);
    let pdf_ms = ms(t);
    let _ = std::fs::remove_dir_all(&dir);

    // Find all: thousands of matches.
    let bar = app.global::<FindBar>();
    bar.invoke_open_requested(false);
    let t = Instant::now();
    bar.invoke_query_changed("dolor".into());
    let find_ms = ms(t);
    let match_count = find_bar::editor_matches(&state.current.borrow(), "dolor", false).len();
    let rss = peak_working_set_mb();

    eprintln!(
        "WRITER_BENCH pages={pages} real_pages={real_pages} build_ms={build_ms:.0} \
         apply_state_ms={apply_ms:.1} first_render_ms={first_render_ms:.1} paginate_ms={paginate_ms:.1} \
         keystroke_ms p50={k50:.2} p95={k95:.2} max={kmax:.2}; scroll_cb_ms p50={s50:.2} p95={s95:.2} max={smax:.2}; \
         render_ms p50={r50:.2} p95={r95:.2} max={rmax:.2}; save_ms={save_ms:.1} open_ms={open_ms:.1} \
         open_apply_ms={open_apply_ms:.1} file_kb={size_kb}; pdf_ms={pdf_ms:.1} pdf_kb={}; \
         find_matches={match_count} find_update_ms={find_ms:.1}; peak_rss_mb={rss:.0}; render_panics={render_panics}/60 render_skipped_over_software_limit={}",
        pdf.len() / 1024,
        !render_supported
    );
    if mode == "enforce" {
        let key_budget = if pages <= 100 {
            KEYSTROKE_BUDGET_100_MS
        } else {
            KEYSTROKE_BUDGET_400_MS
        };
        assert!(
            k95 < key_budget,
            "{pages}p keystroke p95 {k95:.2} ms > {key_budget} ms"
        );
        if render_supported {
            assert_eq!(render_panics, 0, "{pages}p software render panicked");
            assert!(
                r95 < RENDER_BUDGET_MS,
                "{pages}p render p95 {r95:.2} ms > {RENDER_BUDGET_MS} ms"
            );
        }
        assert!(
            s95 < KEYSTROKE_BUDGET_100_MS,
            "{pages}p scroll callback p95 {s95:.2} ms"
        );
        if pages >= 400 {
            let open = open_ms + open_apply_ms;
            assert!(open < OPEN_BUDGET_400_MS, "{pages}p open {open:.0} ms");
        }
    }
}

std::thread_local! {
    /// Whether the document being measured is short enough to render in software.
    static RENDER_SUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

#[test]
fn writer_frame_bench_20_pages() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(20, &mode);
    }
}

#[test]
fn writer_frame_bench_100_pages() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(100, &mode);
    }
}

#[test]
fn writer_frame_bench_400_pages() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(400, &mode);
    }
}

/// Times the pieces of one keystroke on the UI thread so a failed budget can be
/// pinned to a stage. Printed with `LOOM_FRAME_BENCH=1`; asserts nothing.
#[test]
fn writer_keystroke_stage_profile() {
    if std::env::var("LOOM_FRAME_BENCH").is_err() {
        return;
    }
    for pages in [20usize, 100, 400] {
        let (app, state) = setup(build_document(pages));
        let base = state.current.borrow().clone();
        let text = base.editor_text();
        let mid = text.len() / 2;
        let mut edited = text.clone();
        edited.insert(mid, 'x');
        let stage = |name: &str, f: &mut dyn FnMut()| {
            let t = Instant::now();
            for _ in 0..5 {
                f();
            }
            eprintln!("STAGE pages={pages} {name}: {:.2} ms", ms(t) / 5.0);
        };
        stage("document clone", &mut || {
            let _ = base.clone();
        });
        stage("replace_editor_text_at", &mut || {
            let mut d = base.clone();
            let _ = d.replace_editor_text_at(&edited, Some(mid + 1));
        });
        stage("document equality", &mut || {
            let _ = base == base.clone();
        });
        stage("editor_text()", &mut || {
            let _ = base.editor_text();
        });
        stage("apply_document_with_viewport", &mut || {
            apply_document_with_viewport(&app, &base, *state.viewport.borrow());
        });
        stage("recovery::record_document", &mut || {
            let _ = recovery::record_document(&base);
        });
        stage("document_is_dirty", &mut || {
            let _ = document_is_dirty(&state);
        });
        stage("refresh_writer_registry", &mut || {
            refresh_writer_registry(&app, &state);
        });
        stage("reveal_caret", &mut || {
            reveal_caret(&app, &state);
        });
        stage("layout (full)", &mut || {
            let style = base.page.page_style();
            let _ = base.layout(
                &style,
                PageViewport {
                    width: style.width_pt,
                    height: style.height_pt,
                    zoom: 1.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                },
            );
        });
        stage("apply_state (whole)", &mut || {
            apply_state(&app, &state);
        });
        stage("projection::project (window)", &mut || {
            let _ = projection::project(&base, *state.viewport.borrow(), Some(800.0));
        });
        stage("text_counts", &mut || {
            let _ = base.text_counts();
        });
        stage("selection_announcement", &mut || {
            let _ = selection_announcement(&base, &base.selection());
        });
        stage("formatting_state_for_selection", &mut || {
            let selection = base.selection();
            let _ = formatting_state_for_selection(
                &base,
                DocumentSelection::range(selection.anchor, selection.focus),
            );
        });
        stage("document_bytes", &mut || {
            let _ = document_bytes(&base);
        });
        stage("apply_state_with deferred", &mut || {
            apply_state_with(&app, &state, RecoveryWrite::Deferred);
        });
        stage("apply_typing (whole keystroke)", &mut || {
            let mut next = state.current.borrow().clone();
            let at = next.selection().focus;
            let mut text = next.editor_text();
            text.insert(at, 'y');
            let _ = next.replace_editor_text_at(&text, Some(at + 1));
            apply_typing(&app, &state, next);
        });
    }
}
