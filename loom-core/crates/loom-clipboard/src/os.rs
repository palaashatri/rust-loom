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

#[cfg(not(windows))]
mod unix {
    use super::*;
    use copypasta::ClipboardProvider;

    pub fn write(data: &ClipboardData) -> Result<(), ClipboardOsError> {
        let mut ctx = copypasta::ClipboardContext::new()
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        let text = data
            .plain_text
            .as_deref()
            .or(data.tsv.as_deref())
            .unwrap_or("");
        ctx.set_contents(text.to_string())
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        Ok(())
    }

    pub fn read() -> Result<ClipboardData, ClipboardOsError> {
        let mut ctx = copypasta::ClipboardContext::new()
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        let text = ctx
            .get_contents()
            .map_err(|e| ClipboardOsError::IoError(e.to_string()))?;
        let mut data = ClipboardData::default();
        if !text.is_empty() {
            data.plain_text = Some(text.clone());
            if text.contains('\t') {
                data.tsv = Some(text);
            }
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
}
