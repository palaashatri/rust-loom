use loom_encode_core::{EncodeJob, EncodePreset, EncoderBackend, ExecutionPolicy, execute_job};
use std::{fs, path::PathBuf, os::unix::fs::PermissionsExt};
fn main() {
    let root = PathBuf::from("/tmp/loom-audit-media/evidence-encode");
    fs::create_dir_all(&root).unwrap();
    let input = root.join("source.mp4"); let output = root.join("output.mp4");
    fs::write(&input, b"source").unwrap(); let _=fs::remove_file(&output);
    let script=root.join("fake-encoder");
    fs::write(&script, "#!/bin/sh\nfor arg do last=$arg; done\nprintf NEWENCODE > \"$last\"\nprintf IMPORTANT_OTHER_FILE > /tmp/loom-audit-media/evidence-encode/output.mp4\nprintf 'progress=end\\n'\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let backend=EncoderBackend { executable:script, version:"fake 1".into() };
    let mut job=EncodeJob::new("test", input.to_string_lossy(), output.to_string_lossy(), EncodePreset::h264_1080p());
    let plan=job.plan(&backend,ExecutionPolicy {overwrite:false,create_parent_directories:true}).unwrap();
    let result=execute_job(&mut job,&plan,Some(1.0), |_| {});
    println!("No overwrite (-n) result={result:?}, final content={:?}",fs::read_to_string(&output).unwrap());
    let alias=root.join("./source.mp4");
    let aliasjob=EncodeJob::new("alias",input.to_string_lossy(),alias.to_string_lossy(),EncodePreset::h264_1080p());
    println!("Input/output resolve to same existing file; validate={:?}", aliasjob.validate());
}
