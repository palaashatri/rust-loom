//! Recovery drafts remember the file they were edited from.
//!
//! A draft is the deck package wrapped with the path of its file, recorded as one
//! recovery entry. The path and the content therefore reach recovery together: a
//! relaunch can restore the draft under its own file name, and a draft of a
//! different file can be kept instead of replacing the file just opened. Drafts
//! written before this header existed are plain deck packages and read as
//! untitled work.

use std::path::{Path, PathBuf};

use loom_present_core::{
    load_presentation_session, save_presentation_session, PresentationSession,
};

const HEADER: &[u8] = b"loom-present draft 1\n";

/// A draft entry split back into the file it came from and its deck package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Draft {
    /// The file the draft was edited from; `None` for untitled work.
    pub(crate) source: Option<PathBuf>,
    /// The deck package, exactly as `save_presentation_session` wrote it.
    pub(crate) package: Vec<u8>,
}

/// The recovery entry for `session`, edited from `source` (`None` when untitled).
pub(crate) fn payload(
    session: &PresentationSession,
    source: Option<&Path>,
) -> Result<Vec<u8>, String> {
    Ok(wrap(source, &save_presentation_session(session)?))
}

/// The header, the file path and the package, in that order.
pub(crate) fn wrap(source: Option<&Path>, package: &[u8]) -> Vec<u8> {
    // A path that is not valid text cannot be written down, so such a draft reads
    // as untitled work rather than as a guess at its file.
    let source = source.and_then(Path::to_str).unwrap_or("");
    let mut bytes = Vec::with_capacity(HEADER.len() + 24 + source.len() + package.len());
    bytes.extend_from_slice(HEADER);
    bytes.extend_from_slice(format!("{}\n", source.len()).as_bytes());
    bytes.extend_from_slice(source.as_bytes());
    bytes.extend_from_slice(package);
    bytes
}

/// Splits a recovery entry. Anything without the header is an older plain package.
pub(crate) fn split_entry(bytes: &[u8]) -> Result<Draft, String> {
    let Some(rest) = bytes.strip_prefix(HEADER) else {
        return Ok(Draft {
            source: None,
            package: bytes.to_vec(),
        });
    };
    let newline = rest
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("the draft header is cut short")?;
    let length: usize = std::str::from_utf8(&rest[..newline])
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or("the draft header has no path length")?;
    let body = &rest[newline + 1..];
    if body.len() < length {
        return Err("the draft's file path is cut short".into());
    }
    let (path, package) = body.split_at(length);
    let path =
        std::str::from_utf8(path).map_err(|_| "the draft's file path is not text".to_string())?;
    Ok(Draft {
        source: (!path.is_empty()).then(|| PathBuf::from(path)),
        package: package.to_vec(),
    })
}

/// A recovery entry whose package also loads, with the deck it holds.
pub(crate) fn read(bytes: &[u8]) -> Result<(Draft, PresentationSession), String> {
    let draft = split_entry(bytes)?;
    let session = load_presentation_session(&draft.package)?;
    Ok((draft, session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_session;

    #[test]
    fn a_draft_keeps_its_file_path_and_its_package() {
        let session = sample_session();
        let path = Path::new("/decks/Quarterly plan.loomdeck");
        let bytes = payload(&session, Some(path)).expect("payload");
        let (draft, deck) = read(&bytes).expect("read");
        assert_eq!(draft.source.as_deref(), Some(path));
        assert_eq!(deck.document.slides.len(), session.document.slides.len());
    }

    #[test]
    fn an_untitled_draft_has_no_source_and_older_packages_read_as_untitled() {
        let session = sample_session();
        let (untitled, _) = read(&payload(&session, None).expect("payload")).expect("read");
        assert_eq!(untitled.source, None);

        let plain = save_presentation_session(&session).expect("package");
        let legacy = split_entry(&plain).expect("legacy");
        assert_eq!(legacy.source, None);
        assert_eq!(legacy.package, plain);
    }

    #[test]
    fn a_cut_short_header_is_an_error_and_never_a_guessed_path() {
        let full = wrap(Some(Path::new("/decks/a.loomdeck")), b"PK");
        // Cut inside the length line, right after it, and inside the path.
        assert!(split_entry(&full[..HEADER.len() + 1]).is_err());
        assert!(split_entry(&full[..HEADER.len() + 3]).is_err());
        assert!(split_entry(&full[..HEADER.len() + 9]).is_err());
        // A whole path followed by an empty package is a valid entry.
        let empty = wrap(Some(Path::new("/decks/a.loomdeck")), b"");
        assert_eq!(
            split_entry(&empty).expect("header").package,
            Vec::<u8>::new()
        );
    }
}
