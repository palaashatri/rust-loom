use loom_encode_core::{
    execute_job, EncodeError, EncodeJob, EncodePreset, EncoderBackend, ExecutionPolicy, JobStatus,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn test_directory() -> PathBuf {
    let id = NEXT_TEST_DIRECTORY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "loom-encode-publish-integration-{}-{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[cfg(unix)]
fn quote_shell_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(windows)]
fn quote_powershell_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(unix)]
fn encoder_fixture(root: &Path, output: &Path) -> (EncoderBackend, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;

    let script = root.join("encoder.sh");
    let output_literal = quote_shell_path(output);
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor arg do last=$arg; done\nprintf NEWENCODE > \"$last\"\nprintf IMPORTANT_OTHER_FILE > {output_literal}\nprintf 'progress=end\\n'\n"
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();

    (
        EncoderBackend {
            executable: script,
            version: "fixture".into(),
        },
        Vec::new(),
    )
}

#[cfg(windows)]
fn encoder_fixture(root: &Path, output: &Path) -> (EncoderBackend, Vec<String>) {
    let script = root.join("encoder.ps1");
    let output_literal = quote_powershell_path(output);
    std::fs::write(
        &script,
        format!(
            "param([string]$temporary)\n[System.IO.File]::WriteAllBytes($temporary, [System.Text.Encoding]::ASCII.GetBytes('NEWENCODE'))\n[System.IO.File]::WriteAllBytes({output_literal}, [System.Text.Encoding]::ASCII.GetBytes('IMPORTANT_OTHER_FILE'))\nWrite-Output 'progress=end'\n"
        ),
    )
    .unwrap();

    (
        EncoderBackend {
            executable: PathBuf::from("powershell.exe"),
            version: "fixture".into(),
        },
        vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            script.to_string_lossy().into_owned(),
        ],
    )
}

#[test]
fn no_overwrite_publish_preserves_destination_created_during_encode() {
    let root = test_directory();
    let input = root.join("source.mov");
    let output = root.join("result.mp4");
    std::fs::write(&input, b"source").unwrap();
    let (backend, arguments) = encoder_fixture(&root, &output);

    let mut job = EncodeJob::new(
        "publish-race",
        input.to_string_lossy(),
        output.to_string_lossy(),
        EncodePreset::h264_1080p(),
    );
    let mut plan = job.plan(&backend, ExecutionPolicy::default()).unwrap();
    plan.arguments = arguments;
    let error = execute_job(&mut job, &plan, Some(1.0), |_| {}).unwrap_err();

    assert!(matches!(
        error,
        EncodeError::InvalidJob(message) if message.contains("appeared during encoding")
    ));
    assert!(matches!(
        job.status,
        JobStatus::Failed(ref message) if message.contains("appeared during encoding")
    ));
    assert_eq!(std::fs::read(&output).unwrap(), b"IMPORTANT_OTHER_FILE");
    assert!(!std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .contains(".loom-encode-")));

    std::fs::remove_dir_all(root).unwrap();
}
