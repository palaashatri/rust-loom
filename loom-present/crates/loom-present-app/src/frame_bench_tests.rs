//! Opt-in performance measurements on large decks: the real drag, slide-switch,
//! edit, open/save and export paths plus software-rendered frames.
//! `LOOM_FRAME_BENCH=1` prints numbers; `LOOM_FRAME_BENCH=enforce` also fails
//! when a budget is exceeded.

use super::*;
use loom_desktop::ScriptedFileDialogs;
use slint::Model;
use std::time::Instant;

const CALLBACK_BUDGET_MS: f64 = 16.7;
const RENDER_BUDGET_MS: f64 = 45.0;
const OPEN_BUDGET_300_MS: f64 = 2000.0;

fn pct(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1e3
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
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

fn png(seed: u8) -> Vec<u8> {
    let mut image = image::RgbaImage::new(200, 120);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([seed, (x % 251) as u8, (y % 251) as u8, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

/// `slides` slides of 20 elements: 15 text boxes and 5 pictures.
fn build_session(slides: usize) -> PresentationSession {
    let mut document = PresentationDocument::new("bench-deck", "Bench deck");
    let assets: Vec<_> = (0..5u8)
        .map(|seed| {
            loom_present_core::ImageAsset::from_bytes(&format!("pic{seed}.png"), png(seed * 40))
                .expect("asset")
        })
        .collect();
    for asset in &assets {
        document.assets.insert(asset.id.clone(), asset.clone());
    }
    for n in 0..slides {
        if n > 0 {
            document.add_slide(format!("Slide {n}"), "content");
        }
        let slide = document.slides.last_mut().expect("slide");
        slide.elements.clear();
        for e in 0..15 {
            slide.add_element(text_element(
                &format!("s{n}-t{e}"),
                if e == 0 {
                    ElementType::Title
                } else {
                    ElementType::BodyText
                },
                &format!("Slide {n} text element {e} with some representative words"),
                40.0 + (e % 5) as f32 * 180.0,
                40.0 + (e / 5) as f32 * 150.0,
                170.0,
                60.0,
            ));
        }
        for (p, asset) in assets.iter().enumerate() {
            slide.add_element(SlideElement {
                id: format!("s{n}-p{p}"),
                element_type: ElementType::Picture,
                content: asset.id.clone(),
                x: 40.0 + p as f32 * 180.0,
                y: 420.0,
                width: 160.0,
                height: 96.0,
                rotation_deg: 0.0,
                action: None,
            });
        }
    }
    document.active_index = 0;
    PresentationSession::new(document)
}

fn state_for(session: PresentationSession) -> Rc<GuiState> {
    Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        session: RefCell::new(session),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::new([], [])),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    })
}

fn setup(session: PresentationSession) -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = state_for(session);
    wire_app_callbacks(&app, &state);
    app.window().set_size(PhysicalSize::new(1280, 800));
    configure_responsive_layout(&app, (1280, 800));
    refresh_without_recovery(&app, &state);
    (app, state)
}

fn try_render(app: &PresentApp) -> Option<f64> {
    let t = Instant::now();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot_component(app, 1280.0, 800.0, 1.0).expect("frame");
    }))
    .ok()
    .map(|()| ms(t))
}

fn bench_size(slides: usize, mode: &str) {
    let t = Instant::now();
    let session = build_session(slides);
    let build_ms = ms(t);
    let t = Instant::now();
    let (app, state) = setup(session);
    let first_refresh_ms = ms(t);
    let first_render_ms = try_render(&app).unwrap_or(f64::NAN);

    // Drag: press an element, then 60 move steps through the real callbacks.
    app.invoke_element_pressed(1, false);
    let mut drag = Vec::new();
    for step in 0..60 {
        let t = Instant::now();
        app.invoke_element_moved(1, 2.0 * (step + 1) as f32, 1.5 * (step + 1) as f32);
        drag.push(ms(t));
    }
    app.invoke_element_released(1);
    let (d50, d95, dmax) = stats(drag);

    // One text edit: only the edited slide's thumbnail may be rewritten.
    let keys_before: Vec<String> = app
        .get_slide_thumbs()
        .iter()
        .map(|row| row.key.to_string())
        .collect();
    app.invoke_select_element(0);
    let t = Instant::now();
    app.invoke_update_element_content("Edited title text".into());
    let edit_ms = ms(t);
    let keys_after: Vec<String> = app
        .get_slide_thumbs()
        .iter()
        .map(|row| row.key.to_string())
        .collect();
    let thumbs_changed = keys_before
        .iter()
        .zip(&keys_after)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(keys_before.len(), slides);
    assert_eq!(
        thumbs_changed, 1,
        "an edit must rewrite exactly the edited slide's thumbnail"
    );

    // Slide switching.
    let mut switch = Vec::new();
    for step in 0..60usize {
        let target = ((step + 1) * 7) % slides;
        let t = Instant::now();
        app.invoke_select_slide(target as i32);
        switch.push(ms(t));
    }
    let (s50, s95, smax) = stats(switch);

    // Editor frames and a slideshow transition frame.
    let mut render = Vec::new();
    let mut render_panics = 0;
    for _ in 0..15 {
        match try_render(&app) {
            Some(t) => render.push(t),
            None => render_panics += 1,
        }
    }
    let (r50, r95, rmax) = stats(render);
    app.set_is_preview_mode(true);
    app.set_transition_index(1);
    app.set_reveal(0.5);
    let mut transition = Vec::new();
    for _ in 0..15 {
        match try_render(&app) {
            Some(t) => transition.push(t),
            None => render_panics += 1,
        }
    }
    let (t50, t95, tmax) = stats(transition);
    app.set_is_preview_mode(false);
    app.set_reveal(1.0);

    // Save, open and export.
    let session = state.session.borrow();
    let t = Instant::now();
    let bytes = save_presentation_session(&session).expect("save");
    let save_ms = ms(t);
    let t = Instant::now();
    let loaded = load_presentation_session(&bytes).expect("load");
    let load_ms = ms(t);
    let t = Instant::now();
    let pdf = loom_present_core::export_pdf(&session.document);
    let pdf_ms = ms(t);
    let t = Instant::now();
    let pptx = export_pptx(&session).expect("pptx");
    let pptx_ms = ms(t);
    drop(session);
    let t = Instant::now();
    let _opened = setup(loaded);
    let open_apply_ms = ms(t);
    let rss = peak_working_set_mb();

    eprintln!(
        "PRESENT_BENCH slides={slides} build_ms={build_ms:.0} first_refresh_ms={first_refresh_ms:.1} \
         first_render_ms={first_render_ms:.1}; drag_step_ms p50={d50:.2} p95={d95:.2} max={dmax:.2}; \
         edit_ms={edit_ms:.2} thumbs_rewritten={thumbs_changed}; slide_switch_ms p50={s50:.2} p95={s95:.2} max={smax:.2}; \
         editor_render_ms p50={r50:.2} p95={r95:.2} max={rmax:.2}; transition_render_ms p50={t50:.2} p95={t95:.2} max={tmax:.2}; \
         save_ms={save_ms:.1} file_kb={} load_ms={load_ms:.1} open_apply_ms={open_apply_ms:.1}; \
         pdf_ms={pdf_ms:.1} pdf_kb={}; pptx_ms={pptx_ms:.1} pptx_kb={}; peak_rss_mb={rss:.0}; render_panics={render_panics}/30",
        bytes.len() / 1024,
        pdf.len() / 1024,
        pptx.len() / 1024
    );
    if mode == "enforce" {
        assert_eq!(render_panics, 0, "{slides}: software render panicked");
        assert!(d95 < CALLBACK_BUDGET_MS, "{slides}: drag p95 {d95:.2} ms");
        assert!(s95 < CALLBACK_BUDGET_MS, "{slides}: switch p95 {s95:.2} ms");
        assert!(r95 < RENDER_BUDGET_MS, "{slides}: render p95 {r95:.2} ms");
        assert!(
            t95 < RENDER_BUDGET_MS,
            "{slides}: transition p95 {t95:.2} ms"
        );
        if slides >= 300 {
            let open = load_ms + open_apply_ms;
            assert!(open < OPEN_BUDGET_300_MS, "{slides}: open {open:.0} ms");
        }
    }
}

#[test]
fn present_frame_bench_20_slides() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(20, &mode);
    }
}

#[test]
fn present_frame_bench_100_slides() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(100, &mode);
    }
}

#[test]
fn present_frame_bench_300_slides() {
    if let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") {
        bench_size(300, &mode);
    }
}
