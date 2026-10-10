//! Window chrome takes the interface face; slide content keeps the bundled face.
//!
//! The same window is captured twice: once with the bundled face and once with a
//! family no machine installs, which Slint replaces with its fallback. Toolbar
//! text must change between the two captures and slide text must not.
use super::keyboard_flow_tests::launched;
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::region::{region_changed, region_is_drawn, PixelRect};

/// A family no machine installs, so text drawn in it differs from the bundled face.
const OTHER_FACE: &str = "Loom Test Face That Is Not Installed";

fn rect_of(element: ElementHandle) -> PixelRect {
    let (p, s) = (element.absolute_position(), element.size());
    (
        p.x.floor() as u32,
        p.y.floor() as u32,
        s.width.ceil() as u32,
        s.height.ceil() as u32,
    )
}

fn by_id(app: &PresentApp, id: &str) -> PixelRect {
    rect_of(
        ElementHandle::find_by_element_id(app, id)
            .next()
            .unwrap_or_else(|| panic!("no element {id}")),
    )
}

fn canvas(app: &PresentApp) -> PixelRect {
    rect_of(
        ElementHandle::find_by_element_type_name(app, "PresentSlideCanvas")
            .next()
            .expect("the slide canvas is on screen"),
    )
}

#[test]
fn chrome_takes_the_interface_face_and_slides_keep_the_bundled_face() {
    let session = launched();
    let app = &session.app;
    let bundled = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render bundled face");
    app.global::<Theme>().set_ui_font_family(OTHER_FACE.into());
    let other = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render other face");

    let toolbar = by_id(app, "PresentApp::action-toolbar");
    let slide = canvas(app);
    assert!(
        region_is_drawn(&bundled, slide),
        "the slide canvas must show the sample deck"
    );
    assert!(
        region_changed(&bundled, &other, toolbar),
        "toolbar text must follow the window's interface face"
    );
    assert!(
        !region_changed(&bundled, &other, slide),
        "slide text must keep the bundled document face"
    );
}
