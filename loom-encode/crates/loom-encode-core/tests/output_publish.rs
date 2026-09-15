#[cfg(unix)]
use loom_encode_core::{
    execute_job, EncodeError, EncodeJob, EncodePreset, EncoderBackend, ExecutionPolicy,
};

#[cfg(unix)]
#[test]
fn no_overwrite_publish_preserves_destination_created_during_encode() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "loom-encode-publish-integration-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("source.mov");
    let output = root.join("result.mp4");
    std::fs::write(&input, b"source").unwrap();
    let script = root.join("encoder.sh");
    let output_literal = output.to_string_lossy();
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

    let backend = EncoderBackend {
        executable: script,
        version: "fixture".into(),
    };
    let mut job = EncodeJob::new(
        "publish-race",
        input.to_string_lossy(),
        output.to_string_lossy(),
        EncodePreset::h264_1080p(),
    );
    let plan = job.plan(&backend, ExecutionPolicy::default()).unwrap();
    let error = execute_job(&mut job, &plan, Some(1.0), |_| {}).unwrap_err();
    assert!(matches!(error, EncodeError::InvalidJob(message) if message.contains("appeared")));
    assert_eq!(std::fs::read(&output).unwrap(), b"IMPORTANT_OTHER_FILE");
    assert!(!std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .contains(".loom-encode-")));

    let _ = std::fs::remove_dir_all(root);
}
