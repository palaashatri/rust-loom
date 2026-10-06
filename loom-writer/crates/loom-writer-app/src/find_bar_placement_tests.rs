//! Where the find bar sits: in the document area, under the toolbar, and never
//! over the inspector or the outline, which own the sides of the window.
use super::scale_surfaces_tests::{editor, open_surface};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;

/// x, y, width, height in window pixels.
type Box4 = (f32, f32, f32, f32);

fn rect_of(element: &ElementHandle) -> Box4 {
    let (p, s) = (element.absolute_position(), element.size());
    (p.x, p.y, s.width, s.height)
}

fn intersects(a: Box4, b: Box4) -> bool {
    (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0) > 0.5
        && (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1) > 0.5
}

fn by_id(app: &WriterApp, id: &str) -> Box4 {
    let element = ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("no element {id}"));
    rect_of(&element)
}

fn find_bar(app: &WriterApp) -> Box4 {
    ElementHandle::find_by_accessible_label(app, "Find and replace")
        .next()
        .map(|element| rect_of(&element))
        .expect("the find bar is open")
}

/// Everything the find bar must stay clear of, with a name for the failure text.
fn neighbours(app: &WriterApp) -> Vec<(&'static str, Box4)> {
    let mut out = vec![("toolbar", by_id(app, "WriterApp::action-toolbar"))];
    if app.get_show_inspector() {
        let id = if app.get_compact_inspector_layout() {
            "WriterApp::compact-inspector"
        } else {
            "WriterApp::docked-inspector"
        };
        out.push(("inspector", by_id(app, id)));
    }
    if app.get_show_navigator() {
        let outline = ElementHandle::find_by_element_type_name(app, "WriterOutlinePane")
            .next()
            .expect("the outline is open");
        out.push(("outline", rect_of(&outline)));
    }
    out
}

fn open_find(app: &WriterApp, replace: bool) {
    open_surface(app, if replace { "find-replace" } else { "find" });
}

#[test]
fn the_find_bar_never_covers_the_toolbar_inspector_or_outline() {
    let mut failures = Vec::new();
    for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0), (1920.0, 1200.0)] {
        for scale in [1.0f32, 1.5, 2.0] {
            for rtl in [false, true] {
                for (inspector, outline, replace) in [
                    (false, false, false),
                    (true, false, false),
                    (true, true, true),
                    (false, true, true),
                ] {
                    let app = editor(width, height, scale);
                    configure_direction(&app, rtl);
                    apply_layout_breakpoints(&app, width as u32);
                    app.set_show_inspector(inspector);
                    app.set_show_navigator(outline);
                    open_find(&app, replace);
                    let _ = snapshot_component(&app, width, height, 1.0).expect("render");

                    let what = format!(
                        "{width}x{height} x{scale} rtl={rtl} inspector={inspector} outline={outline} replace={replace}"
                    );
                    let bar = find_bar(&app);
                    if bar.0 < -0.5
                        || bar.1 < -0.5
                        || bar.0 + bar.2 > width + 0.5
                        || bar.1 + bar.3 > height + 0.5
                    {
                        failures.push(format!("{what}: bar {bar:?} leaves the window"));
                    }
                    for (name, rect) in neighbours(&app) {
                        if intersects(bar, rect) {
                            failures
                                .push(format!("{what}: bar {bar:?} covers the {name} {rect:?}"));
                        }
                    }
                    if let Ok(dir) = std::env::var("LOOM_FIND_DUMP") {
                        let side = if rtl { "rtl" } else { "ltr" };
                        let panels = format!(
                            "{}{}{}",
                            if inspector { "i" } else { "" },
                            if outline { "o" } else { "" },
                            if replace { "r" } else { "" }
                        );
                        let image = snapshot_component(&app, width, height, 1.0).expect("render");
                        let _ =
                            image.save(format!("{dir}/find-{width}-{scale}-{side}-{panels}.png"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_find_bar_sits_at_the_reading_end_of_the_document_area() {
    for (width, height) in [(1280.0f32, 800.0f32), (1920.0, 1200.0)] {
        let ltr = editor(width, height, 1.0);
        ltr.set_show_inspector(true);
        open_find(&ltr, false);
        let _ = snapshot_component(&ltr, width, height, 1.0).expect("render");
        let (x, y, w, _) = find_bar(&ltr);
        let inspector = by_id(&ltr, "WriterApp::docked-inspector");
        let toolbar = by_id(&ltr, "WriterApp::action-toolbar");
        assert!(
            (x + w - (inspector.0 - 16.0)).abs() < 1.5,
            "LTR: the bar ends 16 px before the inspector ({x} + {w} vs {})",
            inspector.0
        );
        assert!(
            (y - (toolbar.1 + toolbar.3 + 8.0)).abs() < 1.5,
            "the bar sits 8 px under the toolbar"
        );

        let rtl = editor(width, height, 1.0);
        configure_direction(&rtl, true);
        apply_layout_breakpoints(&rtl, width as u32);
        rtl.set_show_inspector(true);
        open_find(&rtl, false);
        let _ = snapshot_component(&rtl, width, height, 1.0).expect("render");
        let (x, _, _, _) = find_bar(&rtl);
        let inspector = by_id(&rtl, "WriterApp::docked-inspector");
        assert!(
            (x - (inspector.0 + inspector.2 + 16.0)).abs() < 1.5,
            "RTL mirrors: the bar starts 16 px after the inspector ({x} vs {})",
            inspector.0 + inspector.2
        );
    }
}
