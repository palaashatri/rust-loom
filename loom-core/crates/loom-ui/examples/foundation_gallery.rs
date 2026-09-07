include!(concat!(env!("OUT_DIR"), "/gallery.rs"));

use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = FoundationGallery::new()?;
    if let Ok(theme) = std::env::var("LOOM_THEME") {
        Theme::get(&window).set_active_theme(theme.into());
    }
    if let Ok(cat) = std::env::var("LOOM_GALLERY_CATEGORY") {
        if let Ok(c) = cat.parse::<i32>() {
            window.set_selected_category(c);
        }
    }
    window.show()?;
    slint::run_event_loop()?;
    Ok(())
}
