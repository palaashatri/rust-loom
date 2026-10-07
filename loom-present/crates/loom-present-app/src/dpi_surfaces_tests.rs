//! Every surface the scale tests open, rendered at real device scale factors
//! (1.25, 1.5, 2.0). A scale factor changes pixel density only: the logical
//! layout must match the 1.0 layout and the image must be `logical * factor`
//! pixels with at least as much drawn detail.
use super::scale_surfaces_tests::{open_surface, rects, ROLES};
use super::*;
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::dpi::{layout_difference, pixel_problem};

const SURFACES: [&str; 10] = [
    "none",
    "save-changes",
    "palette",
    "themes",
    "notes",
    "inspector",
    "inspector-selection",
    "view-menu",
    "zoom-menu",
    "overflow",
];

#[test]
fn every_surface_keeps_its_logical_layout_at_real_scale_factors() {
    for surface in SURFACES {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            set_platform();
            let app = PresentApp::new().expect("create app");
            apply_theme(&app, "light");
            configure_responsive_layout(&app, (width as u32, height as u32));
            if surface == "overflow" {
                app.set_overflow_toolbar(true);
            }
            open_surface(&app, surface);
            let base_image = snapshot_component(&app, width, height, 1.0).expect("render 1.0");
            if let Ok(dir) = std::env::var("LOOM_DPI_DUMP") {
                let _ = base_image.save(format!("{dir}/present-{surface}-{width}-1.0.png"));
            }
            let base = rects(&app, &ROLES);
            for factor in [1.25f32, 1.5, 2.0] {
                let image = snapshot_component(&app, width, height, factor).expect("render");
                if let Ok(dir) = std::env::var("LOOM_DPI_DUMP") {
                    let _ = image.save(format!("{dir}/present-{surface}-{width}-{factor}.png"));
                }
                let scaled = rects(&app, &ROLES);
                if let Some(problem) = layout_difference(&base, &scaled) {
                    panic!("{surface} {width}x{height} at {factor}: layout moved: {problem}");
                }
                if let Some(problem) = pixel_problem(&base_image, &image, (width, height), factor) {
                    panic!("{surface} {width}x{height} at {factor}: {problem}");
                }
            }
        }
    }
}

#[test]
fn the_presenter_window_keeps_its_layout_at_real_scale_factors() {
    set_platform();
    let window = PresenterWindow::new().expect("presenter window");
    window.set_slide_title("A fairly long slide title that has to wrap or elide".into());
    window.set_notes("Remember the pricing change and the open questions.".into());
    window.set_next_title("Next slide".into());
    window.set_has_next(true);
    window.set_has_previous(true);
    for (w, h) in [(960.0f32, 600.0f32), (640.0, 420.0)] {
        let base_image = snapshot_component(&window, w, h, 1.0).expect("render");
        let base = rects(&window, &ROLES);
        for factor in [1.25f32, 1.5, 2.0] {
            let image = snapshot_component(&window, w, h, factor).expect("render");
            if let Some(problem) = layout_difference(&base, &rects(&window, &ROLES)) {
                panic!("presenter {w}x{h} at {factor}: layout moved: {problem}");
            }
            if let Some(problem) = pixel_problem(&base_image, &image, (w, h), factor) {
                panic!("presenter {w}x{h} at {factor}: {problem}");
            }
        }
    }
}

#[test]
fn scale_factor_is_independent_of_text_scale() {
    set_platform();
    let args = |extra: &[&str]| {
        let mut parsed = Args {
            screenshot: None,
            smoke: false,
            palette: false,
            journey: None,
            size: DEFAULT_SIZE,
            theme: "light".into(),
            theme_explicit: false,
            rtl: false,
            text_scale: 1.0,
            scale_factor: 1.0,
            open: None,
            theme_chooser: false,
        };
        let mut it = extra.iter();
        while let Some(flag) = it.next() {
            match *flag {
                "--scale-factor" => parsed.scale_factor = it.next().unwrap().parse().unwrap(),
                "--text-scale" => parsed.text_scale = it.next().unwrap().parse().unwrap(),
                _ => {}
            }
        }
        parsed
    };
    let dir = std::env::temp_dir().join(format!("loom-present-dpi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let size_of = |name: &str, flags: &[&str]| {
        let path = dir.join(name);
        render_headless(&args(flags), &path.to_string_lossy()).expect("render");
        image::open(&path).expect("png").to_rgba8().dimensions()
    };
    assert_eq!(size_of("a.png", &[]), (1280, 800));
    assert_eq!(size_of("b.png", &["--scale-factor", "2"]), (2560, 1600));
    assert_eq!(
        size_of("c.png", &["--text-scale", "1.5"]),
        (1280, 800),
        "text scale never changes pixel density"
    );
    assert_eq!(
        size_of("d.png", &["--scale-factor", "1.5", "--text-scale", "1.5"]),
        (1920, 1200)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn picture_state() -> Rc<GuiState> {
    let mut session = empty_session();
    let mut image = image::RgbaImage::new(3000, 2000);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let on = (x / 6 + y / 6) % 2 == 0;
        *pixel = image::Rgba([if on { 20 } else { 235 }, 90, 160, 255]);
    }
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode");
    let asset = loom_present_core::ImageAsset::from_bytes("fine.png", bytes).expect("asset");
    let id = asset.id.clone();
    session.document.assets.insert(id.clone(), asset);
    session
        .document
        .active_slide_mut()
        .expect("slide")
        .add_element(SlideElement {
            id: "pic".into(),
            element_type: ElementType::Picture,
            content: id,
            x: 100.0,
            y: 100.0,
            width: 600.0,
            height: 400.0,
            rotation_deg: 0.0,
            action: None,
        });
    Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(session),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        pending_replacement: Cell::new(None),
        dialogs: Rc::new(NativeFileDialogs),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    })
}

/// A picture is decoded for the density it is drawn at: on a 2x display it has
/// twice the pixels, so it is not a 1x bitmap stretched and blurred.
#[test]
fn pictures_are_decoded_for_the_display_density() {
    use slint::Model;
    let width_at = |factor: f32| {
        set_platform();
        let app = PresentApp::new().expect("app");
        let state = picture_state();
        let _ = snapshot_component(&app, 1280.0, 800.0, factor).expect("render");
        refresh(&app, &state);
        let image = app.get_element_images().iter().find(|i| i.size().width > 0);
        image.expect("a decoded picture").size().width
    };
    let (one, two) = (width_at(1.0), width_at(2.0));
    assert_eq!(one, 1280, "1x keeps the 1280 px draw size");
    assert!(
        two >= 2 * one,
        "a 2x display needs at least twice the pixels: {one} -> {two}"
    );
}
