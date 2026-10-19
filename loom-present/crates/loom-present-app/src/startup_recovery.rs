//! The deck the window opens with when recovery holds an unsaved draft.
//!
//! A draft is never replaced silently. It is restored as unsaved work under its
//! own file name, or, when a different file is opened, kept in a file of its own
//! before recovery is reset. Only an explicit Discard removes a draft.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use loom_present_core::PresentationSession;

use crate::{deck_guard, empty_session, file_title, load_session, recovery_draft};

/// The recovery store's application id. Kept drafts sit beside its folder.
const APPLICATION_ID: &str = "org.loom.present";
/// The name a recovered entry that cannot be read is kept under.
const UNREADABLE_NAME: &str = "Unreadable recovered draft";

/// What the window shows at startup, and what it compares "saved" with.
pub(crate) struct StartupPlan {
    /// The deck the window opens with.
    pub(crate) session: PresentationSession,
    /// The file as it is on disk, or the blank deck when there is no file.
    pub(crate) baseline: PresentationSession,
    /// Where Save writes; `None` for a deck that was never saved to a file.
    pub(crate) save_path: Option<PathBuf>,
    /// The display name of a recovered draft restored as unsaved work.
    pub(crate) restored: Option<String>,
    /// A draft that must leave recovery before recovery is reset.
    pub(crate) kept: Option<KeptDraft>,
    /// A lossy PowerPoint candidate awaiting an explicit choice.
    pub(crate) pending_import: Option<crate::pptx_import::Candidate>,
}

/// A recovered draft that is written to a file of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeptDraft {
    /// The name the draft is kept under.
    pub(crate) name: String,
    /// The deck package when the draft loads, otherwise the bytes as recovered.
    pub(crate) bytes: Vec<u8>,
    /// Whether `bytes` is a deck package that opens as a `.loomdeck` file.
    pub(crate) readable: bool,
}

impl StartupPlan {
    /// A deck that matches its file, or the blank deck: nothing is unsaved.
    fn clean(session: PresentationSession, save_path: Option<PathBuf>) -> Self {
        Self {
            baseline: session.clone(),
            session,
            save_path,
            restored: None,
            kept: None,
            pending_import: None,
        }
    }

    /// A recovered draft shown as unsaved work. `saved` is the file as it is on
    /// disk, when there is one, and is what "saved" compares with.
    fn unsaved(
        session: PresentationSession,
        saved: Option<PresentationSession>,
        save_path: Option<PathBuf>,
        name: String,
    ) -> Self {
        Self {
            session,
            baseline: saved.unwrap_or_else(empty_session),
            save_path,
            restored: Some(name),
            kept: None,
            pending_import: None,
        }
    }
}

/// Decides what to show at startup from the recovered entry (if any) and the
/// file named on the command line (if any). A failure to read the requested file
/// returns an error before recovery is touched.
pub(crate) fn startup_sessions(
    recovered: Option<&[u8]>,
    open: Option<&Path>,
) -> Result<StartupPlan, String> {
    if let Some(path) = open.filter(|path| crate::pptx_import::is_pptx_path(path)) {
        // A draft of the requested PowerPoint file remains the startup source;
        // importing the file again would replace recovered work before a choice.
        if recovery_source_matches(recovered, path) {
            return startup_sessions_with_opened(recovered, open, None);
        }

        let candidate = crate::pptx_import::prepare(path)?;
        if candidate.warnings.is_empty() {
            return startup_sessions_with_opened(recovered, open, Some(candidate.session));
        }

        // Keep the recovered session live and defer both draft quarantine and
        // recovery reset until Continue. Cancel then leaves recovery untouched.
        let mut requested =
            startup_sessions_with_opened(recovered, open, Some(candidate.session.clone()))?;
        let kept = requested.kept.take();
        let mut current = startup_sessions_with_opened(recovered, None, None)?;
        current.kept = kept;
        current.pending_import = Some(candidate);
        return Ok(current);
    }

    startup_sessions_with_opened(recovered, open, None)
}

fn startup_sessions_with_opened(
    recovered: Option<&[u8]>,
    open: Option<&Path>,
    opened: Option<PresentationSession>,
) -> Result<StartupPlan, String> {
    let Some(bytes) = recovered else {
        return opened_or_blank(open, opened);
    };
    let Ok((draft, deck)) = recovery_draft::read(bytes) else {
        // Recovery data this version cannot read is kept as it was found.
        let mut plan = opened_or_blank(open, opened)?;
        plan.kept = Some(KeptDraft {
            name: UNREADABLE_NAME.into(),
            bytes: bytes.to_vec(),
            readable: false,
        });
        return Ok(plan);
    };
    let name = file_title::display_title(draft.source.as_deref(), &deck.document.title);
    let source = draft.source;
    match (open, source) {
        (Some(path), Some(ref source)) if same_file(source, path) => {
            // The draft was edited from the file being opened.
            match opened.clone().or_else(|| load_session(path).ok()) {
                Some(saved) if same_content(&deck, &saved) => {
                    Ok(StartupPlan::clean(saved, Some(path.to_path_buf())))
                }
                saved => Ok(StartupPlan::unsaved(
                    deck,
                    saved,
                    Some(path.to_path_buf()),
                    name,
                )),
            }
        }
        (Some(path), ref source) => {
            // Another file is opened: its draft is kept beside it, never replaced,
            // unless the previous session was clean/unedited.
            let is_clean = match source {
                Some(ref source) => match load_session(source).ok() {
                    Some(saved) => same_content(&deck, &saved),
                    None => false,
                },
                None => same_content(&deck, &empty_session()),
            };
            let session = match opened.clone() {
                Some(session) => session,
                None => load_session(path)?,
            };
            let mut plan = StartupPlan::clean(session, Some(path.to_path_buf()));
            if !is_clean {
                plan.kept = Some(KeptDraft {
                    name,
                    bytes: draft.package,
                    readable: true,
                });
            }
            Ok(plan)
        }
        (None, Some(source)) => match load_session(&source).ok() {
            Some(saved) if same_content(&deck, &saved) => {
                Ok(StartupPlan::clean(saved, Some(source)))
            }
            saved => Ok(StartupPlan::unsaved(deck, saved, Some(source), name)),
        },
        (None, None) => Ok(StartupPlan::unsaved(deck, None, None, name)),
    }
}

fn opened_or_blank(
    open: Option<&Path>,
    opened: Option<PresentationSession>,
) -> Result<StartupPlan, String> {
    match open {
        Some(path) => {
            let session = match opened {
                Some(session) => session,
                None => load_session(path)?,
            };
            Ok(StartupPlan::clean(session, Some(path.to_path_buf())))
        }
        None => Ok(StartupPlan::clean(empty_session(), None)),
    }
}

fn recovery_source_matches(recovered: Option<&[u8]>, path: &Path) -> bool {
    recovered
        .and_then(|bytes| recovery_draft::read(bytes).ok())
        .and_then(|(draft, _)| draft.source)
        .is_some_and(|source| same_file(&source, path))
}

/// Two paths name one file, comparing canonical paths when both exist.
fn same_file(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// The same deck content, as the window's dirty check defines it.
fn same_content(left: &PresentationSession, right: &PresentationSession) -> bool {
    deck_guard::content_matches(&left.document, &right.document)
        && left.transitions == right.transitions
}

/// The folder kept drafts are written to. It sits beside the recovery store, not
/// inside it, because clearing the store removes everything inside that folder.
pub(crate) fn kept_drafts_directory() -> Result<PathBuf, String> {
    let store = loom_production::snapshot::application_state_directory(APPLICATION_ID)
        .map_err(|error| error.to_string())?;
    let parent = store
        .parent()
        .ok_or_else(|| "the recovery folder has no parent".to_string())?;
    Ok(parent.join(format!("{APPLICATION_ID}-unsaved")))
}

/// Writes a kept draft to a new file in `directory` and returns its path. An
/// existing file is never overwritten.
pub(crate) fn write_kept_draft(
    directory: &Path,
    kept: &KeptDraft,
    stamp: u64,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("could not create '{}': {error}", directory.display()))?;
    let extension = if kept.readable {
        "loomdeck"
    } else {
        "recovered"
    };
    let name = file_safe(&kept.name);
    let mut attempt = 0u32;
    let path = loop {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let candidate = directory.join(format!("{name} (unsaved {stamp}{suffix}).{extension}"));
        if !candidate.exists() {
            break candidate;
        }
        attempt += 1;
    };
    loom_storage::atomic_write(&path, &kept.bytes)
        .map_err(|error| format!("could not write '{}': {error}", path.display()))?;
    Ok(path)
}

/// Makes sure a recovered draft has been durably written before its recovery
/// entry can be replaced or cleared. The output path is retained for retries.
pub(crate) fn ensure_kept_draft_is_preserved(
    directory: &Path,
    kept: &KeptDraft,
    preserved_path: &mut Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(path) = preserved_path.as_ref() {
        verify_kept_draft_file(path, kept)?;
        return Ok(path.clone());
    }
    let path = write_kept_draft(directory, kept, unix_seconds())?;
    verify_kept_draft_file(&path, kept)?;
    *preserved_path = Some(path.clone());
    Ok(path)
}

fn verify_kept_draft_file(path: &Path, kept: &KeptDraft) -> Result<(), String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("could not verify '{}': {error}", path.display()))?;
    let length = file
        .metadata()
        .map_err(|error| format!("could not inspect '{}': {error}", path.display()))?
        .len();
    if length != kept.bytes.len() as u64 {
        return Err(format!(
            "could not verify '{}': preserved bytes have the wrong length",
            path.display()
        ));
    }

    let mut offset = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("could not verify '{}': {error}", path.display()))?;
        if count == 0 {
            break;
        }
        if kept.bytes.get(offset..offset + count) != Some(&buffer[..count]) {
            return Err(format!(
                "could not verify '{}': preserved bytes do not match the recovered draft",
                path.display()
            ));
        }
        offset += count;
    }
    Ok(())
}

/// A name that is safe as a file name on every platform.
fn file_safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_control() || "/\\:*?\"<>|".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Untitled Presentation".into()
    } else {
        trimmed.into()
    }
}

/// Whole seconds since the Unix epoch, for names of kept drafts.
pub(crate) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
