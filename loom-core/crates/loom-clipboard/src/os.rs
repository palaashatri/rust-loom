//! Operating system clipboard integration for Windows and Unix.

use crate::model::ClipboardData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardOsError {
    Unavailable,
    LockFailed,
    IoError(String),
    Unsupported(String),
}

impl std::fmt::Display for ClipboardOsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "Clipboard is unavailable"),
            Self::LockFailed => write!(f, "Failed to acquire clipboard lock"),
            Self::IoError(msg) => write!(f, "Clipboard I/O error: {msg}"),
            Self::Unsupported(msg) => write!(f, "Unsupported clipboard format: {msg}"),
        }
    }
}

impl std::error::Error for ClipboardOsError {}

#[cfg(windows)]
mod windows {
    use super::*;

    pub fn write(data: &ClipboardData) -> Result<(), ClipboardOsError> {
        let _clip = clipboard_win::Clipboard::new()
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        clipboard_win::empty().map_err(|e| ClipboardOsError::IoError(e.to_string()))?;

        // 1. Plain text / TSV via CF_UNICODETEXT without clearing
        if let Some(text) = data.plain_text.as_deref().or(data.tsv.as_deref()) {
            clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
                .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        }

        // 2. HTML via registered format "HTML Format" (CF_HTML) without clearing
        if let Some(html) = &data.html {
            if let Some(html_format) = clipboard_win::formats::Html::new() {
                clipboard_win::raw::set_html(html_format.code(), html.as_str())
                    .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
            }
        }

        // 3. Private Loom format via registered format without clearing
        if let Some(json) = &data.loom_objects_json {
            if let Some(format_id) = clipboard_win::register_format(crate::model::MIME_LOOM_OBJECTS)
            {
                let mut bytes = json.as_bytes().to_vec();
                bytes.push(0);
                clipboard_win::raw::set_without_clear(format_id.get(), &bytes)
                    .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub fn read() -> Result<ClipboardData, ClipboardOsError> {
        let _clip = clipboard_win::Clipboard::new()
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;

        let mut data = ClipboardData::default();

        // 1. Check private format
        if let Some(format_id) = clipboard_win::register_format(crate::model::MIME_LOOM_OBJECTS) {
            if let Ok(bytes) =
                clipboard_win::get::<Vec<u8>, _>(clipboard_win::formats::RawData(format_id.get()))
            {
                if !bytes.is_empty() {
                    if let Ok(text) = String::from_utf8(bytes) {
                        let trimmed = text.trim_matches('\0').trim().to_string();
                        if !trimmed.is_empty() {
                            data.loom_objects_json = Some(trimmed);
                        }
                    }
                }
            }
        }

        // 2. Check HTML Format
        if let Some(html_format) = clipboard_win::formats::Html::new() {
            if let Ok(html_str) = clipboard_win::get::<String, _>(html_format) {
                let trimmed = html_str.trim_matches('\0').trim().to_string();
                if !trimmed.is_empty() {
                    data.html = Some(trimmed);
                }
            }
        }

        // 3. Check plain text
        if let Ok(text) = clipboard_win::get::<String, _>(clipboard_win::formats::Unicode) {
            let trimmed = text.trim_matches('\0').to_string();
            if !trimmed.is_empty() {
                data.plain_text = Some(trimmed.clone());
                if trimmed.contains('\t') {
                    data.tsv = Some(trimmed);
                }
            }
        }

        Ok(data)
    }
}

/// Marker that carries the Loom-private JSON inside the HTML flavour, for
/// systems whose clipboard API only serves standard formats. Other programs
/// ignore an HTML comment; a Loom application reads it back, so a copied table
/// or chart keeps everything Loom knows about it.
const LOOM_COMMENT_OPEN: &str = "<!--LOOM-OBJECTS:";

/// Escapes `-` and `%` so the JSON can never contain the `-->` that would end
/// the comment early. Only the non-Windows clipboard calls it (Windows has a
/// registered private format), so it is unused in a Windows build.
#[cfg_attr(windows, allow(dead_code))]
fn embed_loom_json(html: &str, json: &str) -> String {
    let escaped = json.replace('%', "%25").replace('-', "%2D");
    format!("{LOOM_COMMENT_OPEN}{escaped}-->{html}")
}

/// Pulls the private JSON back out of HTML written by [`embed_loom_json`].
#[cfg_attr(windows, allow(dead_code))]
fn extract_loom_json(html: &str) -> Option<String> {
    let start = html.find(LOOM_COMMENT_OPEN)? + LOOM_COMMENT_OPEN.len();
    let end = start + html[start..].find("-->")?;
    let json = html[start..end].replace("%2D", "-").replace("%25", "%");
    (!json.is_empty()).then_some(json)
}

#[cfg(not(windows))]
mod unix {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        // On X11 the program that last copied owns the selection and answers
        // other programs' requests, so the owner must outlive the copy call.
        static OWNER: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
    }

    fn with_clipboard<R>(
        f: impl FnOnce(&mut arboard::Clipboard) -> Result<R, arboard::Error>,
    ) -> Result<R, ClipboardOsError> {
        OWNER.with(|owner| {
            let mut owner = owner.borrow_mut();
            if owner.is_none() {
                *owner = Some(
                    arboard::Clipboard::new()
                        .map_err(|e| ClipboardOsError::IoError(e.to_string()))?,
                );
            }
            let clipboard = owner.as_mut().ok_or(ClipboardOsError::Unavailable)?;
            f(clipboard).map_err(|e| ClipboardOsError::IoError(e.to_string()))
        })
    }

    pub fn write(data: &ClipboardData) -> Result<(), ClipboardOsError> {
        let text = data
            .plain_text
            .as_deref()
            .or(data.tsv.as_deref())
            .unwrap_or("")
            .to_string();
        let html = data
            .html
            .as_deref()
            .map(|html| match &data.loom_objects_json {
                Some(json) => embed_loom_json(html, json),
                None => html.to_string(),
            });
        with_clipboard(|clipboard| match html {
            Some(html) => clipboard.set().html(html, Some(text)),
            None => clipboard.set_text(text),
        })
    }

    pub fn read() -> Result<ClipboardData, ClipboardOsError> {
        let mut data = ClipboardData::default();
        let html = with_clipboard(|clipboard| Ok(clipboard.get().html().ok()))?;
        if let Some(html) = html.filter(|html| !html.trim().is_empty()) {
            data.loom_objects_json = extract_loom_json(&html);
            data.html = Some(html);
        }
        let text = with_clipboard(|clipboard| Ok(clipboard.get_text().ok()))?;
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            if text.contains('\t') {
                data.tsv = Some(text.clone());
            }
            data.plain_text = Some(text);
        }
        Ok(data)
    }
}

// Test / mock storage
thread_local! {
    static MOCK_CLIPBOARD: std::cell::RefCell<Option<ClipboardData>> = const { std::cell::RefCell::new(None) };
    static MOCK_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static MOCK_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test hook: enable in-memory mock clipboard.
pub fn set_mock_clipboard_enabled(enabled: bool) {
    MOCK_ENABLED.with(|f| f.set(enabled));
}

/// Test hook: make mock clipboard operations fail.
pub fn set_mock_clipboard_fails(fails: bool) {
    MOCK_FAILS.with(|f| f.set(fails));
}

/// Words for a status line when this system can only carry plain text, so a
/// copied table or chart will paste as text. `None` where rich formats work
/// (and while the in-memory test clipboard is active).
pub fn rich_format_note() -> Option<&'static str> {
    let has_display = cfg!(windows)
        || std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if MOCK_ENABLED.with(std::cell::Cell::get) || has_display {
        None
    } else {
        Some("copied as plain text; there is no display to share rich table formats with")
    }
}

/// Writes multi-format data to the clipboard.
pub fn write_clipboard(data: &ClipboardData) -> Result<(), ClipboardOsError> {
    if MOCK_ENABLED.with(std::cell::Cell::get) {
        if MOCK_FAILS.with(std::cell::Cell::get) {
            return Err(ClipboardOsError::Unavailable);
        }
        MOCK_CLIPBOARD.with(|c| *c.borrow_mut() = Some(data.clone()));
        return Ok(());
    }

    #[cfg(windows)]
    {
        windows::write(data)
    }
    #[cfg(not(windows))]
    {
        unix::write(data)
    }
}

/// Reads multi-format data from the clipboard.
pub fn read_clipboard() -> Result<ClipboardData, ClipboardOsError> {
    if MOCK_ENABLED.with(std::cell::Cell::get) {
        if MOCK_FAILS.with(std::cell::Cell::get) {
            return Err(ClipboardOsError::Unavailable);
        }
        return Ok(MOCK_CLIPBOARD.with(|c| c.borrow().clone().unwrap_or_default()));
    }

    #[cfg(windows)]
    {
        windows::read()
    }
    #[cfg(not(windows))]
    {
        unix::read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_json_survives_the_html_comment_round_trip() {
        // Text that would end the comment early, a percent sign that must not be
        // read as an escape, and negative numbers outside any string.
        let json = r#"{"title":"a --> b -- 100%2D","values":[-1.5,-2,3],"n":"%25"}"#;
        let html = embed_loom_json("<table><tr><td>x</td></tr></table>", json);
        assert!(html.ends_with("<table><tr><td>x</td></tr></table>"));
        let inside = &html[LOOM_COMMENT_OPEN.len()..html.find("-->").unwrap()];
        assert!(
            !inside.contains('-'),
            "a dash would let the text end the comment"
        );
        assert_eq!(extract_loom_json(&html).as_deref(), Some(json));
        assert_eq!(extract_loom_json("<table></table>"), None);
    }

    #[test]
    fn test_mock_clipboard_operations() {
        set_mock_clipboard_enabled(true);
        let data = ClipboardData {
            plain_text: Some("hello\tworld".into()),
            html: Some("<table><tr><td>hello</td><td>world</td></tr></table>".into()),
            ..Default::default()
        };

        write_clipboard(&data).expect("write to mock clipboard");
        let read = read_clipboard().expect("read from mock clipboard");
        assert_eq!(read.plain_text.as_deref(), Some("hello\tworld"));
        assert_eq!(read.html.as_deref(), data.html.as_deref());

        set_mock_clipboard_fails(true);
        assert!(write_clipboard(&data).is_err());
        assert!(read_clipboard().is_err());

        set_mock_clipboard_fails(false);
        set_mock_clipboard_enabled(false);
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_real_clipboard_round_trip() {
        let data = ClipboardData {
            plain_text: Some("Alpha\tBeta".into()),
            html: Some("<table><tr><td>Alpha</td><td>Beta</td></tr></table>".into()),
            loom_objects_json: Some("{\"type\":\"test\"}".into()),
            ..Default::default()
        };

        write_clipboard(&data).expect("real Windows write");
        let read = read_clipboard().expect("real Windows read");

        assert_eq!(read.plain_text.as_deref(), Some("Alpha\tBeta"));
        assert!(read.html.is_some(), "HTML must be read back");
        assert!(read.html.unwrap().contains("Alpha"));
        assert_eq!(
            read.loom_objects_json.as_deref(),
            Some("{\"type\":\"test\"}")
        );
    }

    /// Needs a running X server (WSLg works): `cargo test -p loom-clipboard --
    /// --ignored real_x11`. Another program can read the HTML flavour while this
    /// process owns the selection: `xclip -selection clipboard -o -t text/html`.
    #[cfg(not(windows))]
    #[test]
    #[ignore = "needs a display server"]
    fn real_x11_clipboard_round_trip() {
        let data = ClipboardData {
            plain_text: Some("Alpha	Beta".into()),
            html: Some("<table><tr><td>Alpha</td><td>Beta</td></tr></table>".into()),
            loom_objects_json: Some("{\"type\":\"test\",\"v\":-1}".into()),
            ..Default::default()
        };
        write_clipboard(&data).expect("real X11 write");
        if let Ok(path) = std::env::var("LOOM_X11_HOLD_FILE") {
            // Lets a script read the clipboard from another process, then release us.
            while !std::path::Path::new(&path).exists() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        let read = read_clipboard().expect("real X11 read");
        assert_eq!(read.plain_text.as_deref(), Some("Alpha	Beta"));
        assert!(read.html.as_deref().is_some_and(|h| h.contains("Alpha")));
        assert_eq!(
            read.loom_objects_json.as_deref(),
            Some("{\"type\":\"test\",\"v\":-1}")
        );
    }
}

#[cfg(test)]
mod note_tests {
    use super::*;

    #[test]
    fn the_plain_text_note_is_absent_for_the_test_clipboard_and_windows() {
        set_mock_clipboard_enabled(true);
        assert!(rich_format_note().is_none());
        set_mock_clipboard_enabled(false);
        assert_eq!(rich_format_note().is_some(), !cfg!(windows));
    }
}
