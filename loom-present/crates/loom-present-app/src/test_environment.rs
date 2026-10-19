use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static STATE_DIRECTORY_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
const STATE_HOME_ENV: &str = "LOCALAPPDATA";
#[cfg(target_os = "macos")]
const STATE_HOME_ENV: &str = "HOME";
#[cfg(not(any(windows, target_os = "macos")))]
const STATE_HOME_ENV: &str = "XDG_STATE_HOME";

pub(crate) fn lock_state_directory() -> MutexGuard<'static, ()> {
    STATE_DIRECTORY_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Serializes tests that read or override the platform state-home variable.
pub(crate) struct StateDirectoryScratch {
    _guard: MutexGuard<'static, ()>,
    path: PathBuf,
    previous: Option<OsString>,
}

impl StateDirectoryScratch {
    pub(crate) fn new() -> Self {
        let guard = lock_state_directory();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "loom-present-state-home-{}-{stamp}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        let previous = std::env::var_os(STATE_HOME_ENV);
        std::env::set_var(STATE_HOME_ENV, &path);
        Self {
            _guard: guard,
            path,
            previous,
        }
    }
}

impl Drop for StateDirectoryScratch {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(previous) => std::env::set_var(STATE_HOME_ENV, previous),
            None => std::env::remove_var(STATE_HOME_ENV),
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
