//! Plain text on the operating system clipboard.
//!
//! Sheets exchanges tab-separated text with other programs, the way every
//! spreadsheet does. Reads and writes fail soft: with no clipboard available
//! (a headless session, a locked desktop) callers fall back to the private
//! in-app buffer. Tests use an in-memory stand-in so they never touch the
//! developer's real clipboard.

#[cfg(not(test))]
thread_local! {
    /// This process's clipboard connection. On X11 the copied text is served by
    /// the process that owns the selection, and dropping the connection gives
    /// the text up. So the connection is opened once and kept for the process.
    static OWNER: std::cell::RefCell<Option<copypasta::ClipboardContext>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(not(test))]
fn with_clipboard<T>(
    action: impl FnOnce(&mut copypasta::ClipboardContext) -> Option<T>,
) -> Option<T> {
    OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_none() {
            *owner = copypasta::ClipboardContext::new().ok();
        }
        action(owner.as_mut()?)
    })
}

#[cfg(not(test))]
pub(crate) fn get_text() -> Option<String> {
    use copypasta::ClipboardProvider;
    with_clipboard(|context| context.get_contents().ok())
}

#[cfg(not(test))]
pub(crate) fn set_text(text: &str) -> bool {
    use copypasta::ClipboardProvider;
    let written = with_clipboard(|context| context.set_contents(text.to_string()).ok());
    if written.is_none() {
        // A failed write may mean a broken connection: the next write opens a new one.
        OWNER.with(|owner| *owner.borrow_mut() = None);
    }
    written.is_some()
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
thread_local! {
    static WRITE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn set_text(text: &str) -> bool {
    if WRITE_FAILS.with(std::cell::Cell::get) {
        return false;
    }
    FAKE.with(|fake| *fake.borrow_mut() = Some(text.to_string()));
    true
}

/// Test hook: make writes fail the way an unavailable clipboard does.
#[cfg(test)]
pub(crate) fn set_write_fails_for_test(fails: bool) {
    WRITE_FAILS.with(|flag| flag.set(fails));
}

/// Test hook: act as another program changing the clipboard (or clearing it).
#[cfg(test)]
pub(crate) fn set_external_text_for_test(text: Option<&str>) {
    FAKE.with(|fake| *fake.borrow_mut() = text.map(str::to_string));
}
