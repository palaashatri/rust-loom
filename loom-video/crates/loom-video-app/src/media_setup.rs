use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn version(path: &Path, tool: &str) -> Result<String, String> {
    let mut child = Command::new(path)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Cannot run {}: {error}", path.display()))?;
    let stdout = child.stdout.take().ok_or("Cannot read tool version")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(65536).read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(format!("{} rejected the version check", path.display()));
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not answer within 3 seconds",
                    path.display()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
    }
    let bytes = rx
        .recv_timeout(Duration::from_millis(100))
        .map_err(|_| "Version output timed out")?
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let first = text.lines().next().unwrap_or("");
    if !first.starts_with(&format!("{tool} version ")) {
        return Err(format!(
            "Choose the {tool} executable; this file did not identify itself as {tool}"
        ));
    }
    Ok(first.chars().take(200).collect())
}

fn setting(app: &str) -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Cannot locate your local settings folder")?;
    Ok(base.join(app).join("ffmpeg-path"))
}
pub fn saved(app: &str) -> Option<PathBuf> {
    std::fs::read_to_string(setting(app).ok()?)
        .ok()
        .map(PathBuf::from)
}
pub fn save(app: &str, path: &Path) -> Result<(), String> {
    let file = setting(app)?;
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(file, path.to_string_lossy().as_bytes())
        .map_err(|e| format!("Could not save tool setting: {e}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn script(body: &str) -> PathBuf {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "loom-tool-check-{}-{}",
            std::process::id(),
            ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    #[test]
    fn accepts_correct_tool_and_rejects_wrong_tool() {
        let path = script("echo 'ffmpeg version test'");
        assert_eq!(version(&path, "ffmpeg").unwrap(), "ffmpeg version test");
        assert!(version(&path, "ffprobe")
            .unwrap_err()
            .contains("did not identify"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn missing_and_hanging_tools_are_actionable_and_bounded() {
        assert!(version(Path::new("/nonexistent/loom-ffmpeg"), "ffmpeg")
            .unwrap_err()
            .contains("Cannot run"));
        let path = script("exec sleep 30");
        let start = Instant::now();
        assert!(version(&path, "ffmpeg")
            .unwrap_err()
            .contains("within 3 seconds"));
        assert!(start.elapsed() < Duration::from_secs(5));
        std::fs::remove_file(path).unwrap();
    }
}
