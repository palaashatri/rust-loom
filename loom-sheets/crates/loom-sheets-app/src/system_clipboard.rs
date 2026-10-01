//! Plain text on the operating system clipboard.
//!
//! Sheets exchanges tab-separated text with other programs, the way every
//! spreadsheet does. Reads and writes fail soft: with no clipboard available
//! (a headless session, a locked desktop) callers fall back to the private
//! in-app buffer. Tests use an in-memory stand-in so they never touch the
//! developer's real clipboard.

#[cfg(not(test))]
pub(crate) fn get_text() -> Option<String> {
    use copypasta::{ClipboardContext, ClipboardProvider};
    ClipboardContext::new().ok()?.get_contents().ok()
}

#[cfg(not(test))]
pub(crate) fn set_text(text: &str) -> bool {
    use copypasta::{ClipboardContext, ClipboardProvider};
    ClipboardContext::new()
        .and_then(|mut context| context.set_contents(text.to_string()))
        .is_ok()
}

#[cfg(test)]
thread_local! {
    static FAKE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn get_text() -> Option<String> {
    FAKE.with(|fake| fake.borrow().clone())
}

#[cfg(test)]
pub(crate) fn set_text(text: &str) -> bool {
    FAKE.with(|fake| *fake.borrow_mut() = Some(text.to_string()));
    true
}

/// Test hook: act as another program changing the clipboard (or clearing it).
#[cfg(test)]
pub(crate) fn set_external_text_for_test(text: Option<&str>) {
    FAKE.with(|fake| *fake.borrow_mut() = text.map(str::to_string));
}
