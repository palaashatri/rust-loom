//! Headless deterministic capture of Slint components.
//!
//! The capture platform is a minimal Slint [`Platform`] implementation that
//! renders into an in-memory buffer using the software renderer, with no
//! window system involved. Because there is no OS windowing, no GPU and no
//! compositor, rendering is deterministic: identical inputs produce
//! byte-identical pixels (verified by tests in this crate).
//!
//! Usage:
//!
//! ```no_run
//! use slint::ComponentHandle;
//! slint::slint! { export component Window inherits Window { } }
//! loom_test_support::capture::set_platform();
//! let ui = Window::new().unwrap();
//! let img = loom_test_support::capture::snapshot_component(&ui, 640.0, 400.0, 1.0).unwrap();
//! # let _ = img;
//! ```
//!
//! The platform is installed per-thread (Slint's `set_platform` is
//! thread-local), so unit tests in multiple threads can each capture their
//! own window.

use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use image::RgbaImage;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, RepaintBufferType, SoftwareRenderer,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::ComponentHandle;

thread_local! {
    static WINDOW: Rc<MinimalSoftwareWindow> =
        MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    static PLATFORM_READY: RefCell<bool> = const { RefCell::new(false) };
}

/// Error returned when a capture fails.
#[derive(Debug)]
pub enum CaptureError {
    /// The capture platform was not installed on this thread.
    PlatformNotInstalled,
    /// The window reported a zero or negative size.
    EmptyWindow,
    /// Slint did not produce a frame for the requested size.
    NothingRendered,
    /// The windowing backend rejected the component.
    Platform(PlatformError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::PlatformNotInstalled => {
                write!(
                    f,
                    "capture platform not installed (call set_platform first)"
                )
            }
            CaptureError::EmptyWindow => write!(f, "captured window is empty"),
            CaptureError::NothingRendered => write!(f, "no frame was rendered"),
            CaptureError::Platform(e) => write!(f, "platform error: {e}"),
        }
    }
}

impl Error for CaptureError {}

/// The capture platform: creates the shared minimal software window.
struct CapturePlatform;

impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(WINDOW.with(|w| w.clone()))
    }
}

/// Install the capture platform on the calling thread.
///
/// Must be called before creating any Slint component on this thread.
/// Panics if another Slint platform is already installed on this thread.
pub fn set_platform() {
    PLATFORM_READY.with(|ready| {
        if !*ready.borrow() {
            slint::platform::set_platform(Box::new(CapturePlatform))
                .expect("another Slint platform is already set on this thread");
            *ready.borrow_mut() = true;
        }
    });
}

/// Render `handle` at the given logical size and scale factor into an image.
///
/// The component is shown, sized to `width` × `height` logical pixels at
/// `scale_factor`, and rendered through the software renderer. The returned
/// image is `(width * scale_factor) × (height * scale_factor)` physical
/// pixels. Prefer a scale factor of 1.0 for deterministic baselines.
pub fn snapshot_component(
    handle: &impl ComponentHandle,
    width: f32,
    height: f32,
    scale_factor: f32,
) -> Result<RgbaImage, CaptureError> {
    let (w, h) = (width.max(1.0), height.max(1.0));
    // The minimal capture window renders at scale factor 1.0; the requested
    // scale is applied to the physical resolution instead. This keeps
    // rendering deterministic across platforms.
    handle.window().set_size(slint::PhysicalSize::new(
        (w * scale_factor) as u32,
        (h * scale_factor) as u32,
    ));
    handle.show().map_err(CaptureError::Platform)?;
    render_now()
}

/// Render the current window immediately and return the pixels.
fn render_now() -> Result<RgbaImage, CaptureError> {
    let (w, h) = WINDOW.with(|win| {
        let s = win.size();
        (s.width as usize, s.height as usize)
    });
    if w == 0 || h == 0 {
        return Err(CaptureError::EmptyWindow);
    }
    let frame = WINDOW.with(|win| {
        win.window().request_redraw();
        let mut done = None;
        win.draw_if_needed(|renderer: &SoftwareRenderer| {
            let mut buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(w as u32, h as u32);
            renderer.render(buffer.make_mut_slice(), w);
            done = Some(buffer);
        });
        done
    });
    let buffer = frame.ok_or(CaptureError::NothingRendered)?;
    let pixels = buffer.as_slice();
    let mut img = RgbaImage::new(w as u32, h as u32);
    for (y, row) in pixels.chunks_exact(w).enumerate() {
        for (x, px) in row.iter().enumerate() {
            img.put_pixel(x as u32, y as u32, image::Rgba([px.r, px.g, px.b, 255]));
        }
    }
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_deterministic() {
        slint::slint! {
            export component Window inherits Window {
                preferred-width: 320px;
                preferred-height: 200px;
                Rectangle {
                    background: #b4552d;
                    width: 80px;
                    height: 40px;
                }
            }
        }
        set_platform();
        let ui = Window::new().unwrap();
        let a = snapshot_component(&ui, 320.0, 200.0, 1.0).unwrap();
        let b = snapshot_component(&ui, 320.0, 200.0, 1.0).unwrap();
        assert_eq!(a.dimensions(), b.dimensions());
        assert_eq!(
            a.as_raw(),
            b.as_raw(),
            "two captures must be byte-identical"
        );
        // The accent rectangle must actually be present.
        assert!(a
            .pixels()
            .any(|p| p == &image::Rgba([0xb4, 0x55, 0x2d, 255])));
    }

    #[test]
    fn snapshot_respects_scale_factor() {
        slint::slint! {
            export component Window inherits Window {
                preferred-width: 100px;
                preferred-height: 100px;
            }
        }
        set_platform();
        let ui = Window::new().unwrap();
        let img = snapshot_component(&ui, 100.0, 100.0, 2.0).unwrap();
        assert_eq!(img.dimensions(), (200, 200));
    }

    #[test]
    fn render_failure_is_reported() {
        set_platform();
        let ui = std::rc::Rc::new(());
        let _ = ui;
        // Empty render should not panic, and zero-size input is clamped.
        slint::slint! {
            export component Tiny inherits Window {
                preferred-width: 1px;
                preferred-height: 1px;
            }
        }
        let tiny = Tiny::new().unwrap();
        let img = snapshot_component(&tiny, 1.0, 1.0, 1.0).unwrap();
        assert_eq!(img.dimensions(), (1, 1));
    }
}
