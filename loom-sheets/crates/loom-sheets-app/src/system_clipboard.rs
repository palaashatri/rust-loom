//! Multi-format clipboard support for Sheets via `loom_clipboard`.
//!
//! Sheets exchanges multi-format data (`application/x-loom-objects+json`,
//! `text/html`, and tab-separated plain text) with other programs. Reads and
//! writes fail soft: with no clipboard available callers fall back to the private
//! in-app buffer. Tests use an in-memory stand-in so they never touch the
//! developer's real clipboard.

use loom_clipboard::ClipboardData;

#[cfg(not(test))]
pub(crate) fn get_clipboard_data() -> Option<ClipboardData> {
    loom_clipboard::read_clipboard().ok()
}

#[cfg(not(test))]
pub(crate) fn set_clipboard_data(data: &ClipboardData) -> bool {
    loom_clipboard::write_clipboard(data).is_ok()
}

#[cfg(test)]
thread_local! {
    static FAKE: std::cell::RefCell<Option<ClipboardData>> = const { std::cell::RefCell::new(None) };
    static WRITE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn get_clipboard_data() -> Option<ClipboardData> {
    FAKE.with(|fake| fake.borrow().clone())
}

#[cfg(test)]
pub(crate) fn set_clipboard_data(data: &ClipboardData) -> bool {
    if WRITE_FAILS.with(std::cell::Cell::get) {
        return false;
    }
    FAKE.with(|fake| *fake.borrow_mut() = Some(data.clone()));
    true
}

#[cfg(test)]
pub(crate) fn get_text() -> Option<String> {
    get_clipboard_data().and_then(|c| c.tsv.or(c.plain_text))
}

/// Test hook: make writes fail the way an unavailable clipboard does.
#[cfg(test)]
pub(crate) fn set_write_fails_for_test(fails: bool) {
    WRITE_FAILS.with(|flag| flag.set(fails));
}

/// Test hook: act as another program changing the clipboard (or clearing it).
#[cfg(test)]
pub(crate) fn set_external_text_for_test(text: Option<&str>) {
    FAKE.with(|fake| {
        *fake.borrow_mut() = text.map(|t| ClipboardData {
            plain_text: Some(t.to_string()),
            tsv: if t.contains('\t') {
                Some(t.to_string())
            } else {
                None
            },
            ..Default::default()
        });
    });
}

/// Test hook: set full multi-format clipboard data for testing.
#[cfg(test)]
pub(crate) fn set_external_clipboard_for_test(data: Option<ClipboardData>) {
    FAKE.with(|fake| *fake.borrow_mut() = data);
}
