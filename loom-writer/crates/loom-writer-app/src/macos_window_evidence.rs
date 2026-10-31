//! macOS window-evidence adapter.
//!
//! The adapter attaches only in the real application. Test builds exercise its
//! pure geometry helpers without constructing the full evidence writer.
#![cfg_attr(test, allow(dead_code))]

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use i_slint_backend_winit::winit::event::WindowEvent;
use i_slint_backend_winit::WinitWindowAccessor;
use objc2::{rc::Retained, MainThreadMarker};
use objc2_app_kit::{NSButton, NSView, NSWindow, NSWindowButton};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Global, Timer};

use crate::WriterApp;

#[derive(Clone, Copy)]
pub(crate) struct RequestedWindow {
    pub physical_size: (u32, u32),
    pub physical_position: Option<(i32, i32)>,
}

#[derive(Clone)]
struct EvidenceWriter(Arc<Mutex<File>>);

impl EvidenceWriter {
    fn from_environment() -> Option<Self> {
        let path = std::env::var_os("LOOM_MACOS_CHROME_EVIDENCE")?;
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(PathBuf::from(path))
        {
            Ok(file) => Some(Self(Arc::new(Mutex::new(file)))),
            Err(error) => {
                eprintln!("loom-writer macOS window evidence could not be opened: {error}");
                None
            }
        }
    }

    fn append(&self, line: &str) {
        if let Err(error) = writeln!(self.0.lock().unwrap(), "{line}") {
            eprintln!("loom-writer macOS window evidence write failed: {error}");
        }
    }
}

pub(crate) fn install(app: &WriterApp, requested: RequestedWindow) -> Option<()> {
    let _main_thread = MainThreadMarker::new()?;
    app.window().with_winit_window(|winit_window| {
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let winit_view_ptr = handle.ns_view.as_ptr().cast::<NSView>();
        // SAFETY: The borrowed raw window handle guarantees this AppKit view is live here.
        let winit_view = unsafe { &*winit_view_ptr };
        let window = winit_view.window()?;
        let content_view = window.contentView()?;
        std::ptr::eq(winit_view, content_view.as_ref()).then_some(())
    })??;

    let evidence = EvidenceWriter::from_environment();
    let weak_app = app.as_weak();
    let event_evidence = evidence.clone();
    app.window().on_winit_window_event(move |_, event| {
        if matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::Focused(_)
                | WindowEvent::Moved(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            schedule_settled_sample(
                weak_app.clone(),
                requested,
                event_evidence.clone(),
                "window-geometry-settled",
            );
        }
        i_slint_backend_winit::EventResult::Propagate
    });

    let state_weak_app = app.as_weak();
    let state_evidence = evidence.clone();
    app.on_view_state_changed(move || {
        schedule_settled_sample(
            state_weak_app.clone(),
            requested,
            state_evidence.clone(),
            "view-state-changed",
        );
    });

    refresh_and_record(app, requested, evidence.as_ref(), "startup");
    schedule_settled_sample(app.as_weak(), requested, evidence, "startup-settled");
    Some(())
}

fn schedule_settled_sample(
    app: slint::Weak<WriterApp>,
    requested: RequestedWindow,
    evidence: Option<EvidenceWriter>,
    trigger: &'static str,
) {
    let Some(evidence) = evidence else {
        return;
    };
    Timer::single_shot(Duration::from_millis(350), move || {
        if let Some(app) = app.upgrade() {
            refresh_and_record(&app, requested, Some(&evidence), trigger);
        }
    });
}

fn refresh_and_record(
    app: &WriterApp,
    requested: RequestedWindow,
    evidence: Option<&EvidenceWriter>,
    trigger: &str,
) {
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };
    let Some(evidence) = evidence else {
        return;
    };
    if let Some(line) = capture_sample(app, requested, trigger) {
        evidence.append(&line);
    }
}

fn toolbar_rect_in_content(app: &WriterApp, content_view: &NSView) -> NSRect {
    let bounds = content_view.bounds();
    let height = f64::from(app.get_macos_toolbar_height()).max(0.0);
    let y = appkit_y_for_slint_top(
        bounds,
        content_view.isFlipped(),
        f64::from(app.get_macos_toolbar_y()),
        height,
    );
    NSRect::new(
        NSPoint::new(bounds.origin.x, y),
        NSSize::new(bounds.size.width, height),
    )
}

fn appkit_y_for_slint_top(bounds: NSRect, flipped: bool, slint_top: f64, height: f64) -> f64 {
    bounds.origin.y
        + if flipped {
            slint_top
        } else {
            bounds.size.height - slint_top - height
        }
}

fn capture_sample(app: &WriterApp, requested: RequestedWindow, trigger: &str) -> Option<String> {
    let _main_thread = MainThreadMarker::new()?;
    let mut sample = None;
    let _ = app.window().with_winit_window(|winit_window| {
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let winit_view_ptr = handle.ns_view.as_ptr().cast::<NSView>();
        // SAFETY: The borrowed raw window handle keeps this NSView alive in the callback.
        let winit_view = unsafe { &*winit_view_ptr };
        let window = winit_view.window()?;
        let content_view = window.contentView()?;
        if !std::ptr::eq(winit_view, content_view.as_ref()) {
            return None;
        }

        let appkit_frame = window.frame();
        let content_rect = window.contentRectForFrameRect(appkit_frame);
        let content_layout_rect = window.contentLayoutRect();
        let content_frame = content_view.frame();
        let content_bounds = content_view.bounds();
        let backing_bounds = content_view.convertRectToBacking(content_bounds);
        let toolbar_rect = toolbar_rect_in_content(app, &content_view);
        let toolbar_backing = content_view.convertRectToBacking(toolbar_rect);
        let close = traffic_light_rect(&window, &content_view, NSWindowButton::CloseButton);
        let minimize = traffic_light_rect(&window, &content_view, NSWindowButton::MiniaturizeButton);
        let zoom = traffic_light_rect(&window, &content_view, NSWindowButton::ZoomButton);

        let monitor = winit_window
            .current_monitor()
            .map(|monitor| {
                format!(
                    "{{\"name\":{},\"physical_size\":[{},{}],\"physical_position\":[{},{}],\"scale_factor\":{}}}",
                    monitor.name().as_deref().map(json_string).unwrap_or_else(|| "null".into()),
                    monitor.size().width,
                    monitor.size().height,
                    monitor.position().x,
                    monitor.position().y,
                    number_json(monitor.scale_factor()),
                )
            })
            .unwrap_or_else(|| "null".into());
        let requested_position = requested
            .physical_position
            .map(|(x, y)| format!("[{x},{y}]"))
            .unwrap_or_else(|| "null".into());
        let actual_position = winit_window
            .outer_position()
            .ok()
            .map(|position| format!("[{},{}]", position.x, position.y))
            .unwrap_or_else(|| "null".into());
        let slint_window = app.window();
        let window_size = slint_window.size();
        let scale_factor = slint_window.scale_factor();
        let root_logical_size = window_size.to_logical(scale_factor);
        let theme = crate::Theme::get(app).get_active_theme().to_string();
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        sample = Some(format!(
            "{{\"record\":\"macos_window_sample\",\"timestamp_unix_ms\":{millis},\"trigger\":{},\"requested_physical_size\":[{},{}],\"requested_outer_position_physical\":{},\"winit\":{{\"inner_size_physical\":[{},{}],\"outer_size_physical\":[{},{}],\"outer_position_physical\":{},\"scale_factor\":{},\"fullscreen\":{},\"monitor\":{}}},\"slint\":{{\"root_logical_size\":[{},{}],\"window_size_physical\":[{},{}],\"scale_factor\":{},\"toolbar_bounds_logical\":[0,{}, {},{}],\"active_theme\":{}}},\"appkit\":{{\"window_frame_points\":{},\"content_rect_points\":{},\"content_layout_rect_points\":{},\"content_view_frame_points\":{},\"content_view_bounds_points\":{},\"content_view_bounds_backing\":{},\"toolbar_bounds_content_points\":{},\"toolbar_bounds_backing\":{},\"backing_scale\":{},\"content_view_is_flipped\":{},\"traffic_lights_content_points\":[{},{},{}],\"window_is_key\":{},\"content_view_is_original_winit_view\":true}}}}",
            json_string(trigger),
            requested.physical_size.0,
            requested.physical_size.1,
            requested_position,
            winit_window.inner_size().width,
            winit_window.inner_size().height,
            winit_window.outer_size().width,
            winit_window.outer_size().height,
            actual_position,
            number_json(winit_window.scale_factor()),
            winit_window.fullscreen().is_some(),
            monitor,
            number_json(f64::from(root_logical_size.width)),
            number_json(f64::from(root_logical_size.height)),
            window_size.width,
            window_size.height,
            number_json(f64::from(scale_factor)),
            number_json(f64::from(app.get_macos_toolbar_y())),
            number_json(f64::from(root_logical_size.width)),
            number_json(f64::from(app.get_macos_toolbar_height())),
            json_string(&theme),
            rect_json(appkit_frame),
            rect_json(content_rect),
            rect_json(content_layout_rect),
            rect_json(content_frame),
            rect_json(content_bounds),
            rect_json(backing_bounds),
            rect_json(toolbar_rect),
            rect_json(toolbar_backing),
            number_json(window.backingScaleFactor()),
            content_view.isFlipped(),
            optional_rect_json(close),
            optional_rect_json(minimize),
            optional_rect_json(zoom),
            window.isKeyWindow(),
        ));
        Some(())
    });
    sample
}

fn traffic_light_rect(
    window: &NSWindow,
    content_view: &NSView,
    button: NSWindowButton,
) -> Option<NSRect> {
    let button: Retained<NSButton> = window.standardWindowButton(button)?;
    Some(button.convertRect_toView(button.bounds(), Some(content_view)))
}

fn rect_json(rect: NSRect) -> String {
    format!(
        "[{}, {}, {}, {}]",
        number_json(rect.origin.x),
        number_json(rect.origin.y),
        number_json(rect.size.width),
        number_json(rect.size.height),
    )
}

fn optional_rect_json(rect: Option<NSRect>) -> String {
    rect.map(rect_json).unwrap_or_else(|| "null".into())
}

fn number_json(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.3}")
    } else {
        "null".into()
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::appkit_y_for_slint_top;
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    #[test]
    fn toolbar_y_conversion_handles_flipped_and_unflipped_appkit_views() {
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 800.0));
        assert_eq!(appkit_y_for_slint_top(bounds, true, 40.0, 52.0), 40.0);
        assert_eq!(appkit_y_for_slint_top(bounds, false, 40.0, 52.0), 708.0);
    }
}
