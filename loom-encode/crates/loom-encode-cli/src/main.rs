//! CLI batch encoder tool for Loom Encode.

use loom_encode_core::{
    discover_ffmpeg, execute_job, load_encode_queue, save_encode_queue, EncodeJob, EncodePreset,
    EncodeQueue, ExecutionPolicy, JobStatus,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Encode CLI");
        println!("Usage:");
        println!("  loom-encode-cli create <output.loomencode> <name>");
        println!("  loom-encode-cli prepare-recovery-demo <input.loomencode>");
        println!("  loom-encode-cli inspect <input.loomencode>");
        println!("  loom-encode-cli plan <input.loomencode> [job-index] [ffmpeg]");
        println!("  loom-encode-cli run <input.loomencode> [job-index] [ffmpeg] [duration-secs]");
        println!("  loom-encode-cli recover <input.loomencode>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args
                .get(3)
                .map(|s| s.as_str())
                .unwrap_or("Untitled Batch Queue");
            let mut queue = EncodeQueue::new("cli-queue", name);
            queue.add_job(EncodeJob::new(
                "j-prores",
                "Feature_Master.mov",
                "Feature_Master_ProRes.mov",
                EncodePreset::prores_master(),
            ));
            let bytes = save_encode_queue(&queue)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!(
                "Created encode queue: {out_path} ({} jobs queued)",
                queue.jobs.len()
            );
        }
        "prepare-recovery-demo" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let mut queue = load_encode_queue(&bytes)?;

            if !queue.jobs.iter().any(|job| job.id == "j-review") {
                queue.add_job(EncodeJob::new(
                    "j-review",
                    "Feature_Master.mov",
                    "Feature_Master_Review.mp4",
                    EncodePreset::h264_1080p(),
                ));
            }
            let first = queue.jobs.first_mut().ok_or("queue has no jobs")?;
            first.status = JobStatus::Encoding { progress: 0.42 };
            queue.active_job_index = 0;

            std::fs::write(in_path, save_encode_queue(&queue)?)
                .map_err(|e| format!("write error: {e}"))?;
            println!(
                "Prepared recovery demo: {} jobs, aggregate progress {:.3}",
                queue.jobs.len(),
                queue.progress()
            );
        }
        "plan" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let index = args
                .get(3)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0usize);
            let candidates = args
                .get(4)
                .map(|path| vec![std::path::PathBuf::from(path)])
                .unwrap_or_default();
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let queue = load_encode_queue(&bytes)?;
            let job = queue.jobs.get(index).ok_or("job index is out of bounds")?;
            let backend = discover_ffmpeg(&candidates).map_err(|e| e.to_string())?;
            let plan = job
                .plan(&backend, ExecutionPolicy::default())
                .map_err(|e| e.to_string())?;
            println!("backend: {}", backend.version);
            println!(
                "command: {} {}",
                plan.executable.display(),
                shell_join(&plan.arguments)
            );
        }
        "run" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let index = args
                .get(3)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0usize);
            let candidates = args
                .get(4)
                .map(|path| vec![std::path::PathBuf::from(path)])
                .unwrap_or_default();
            let duration = args
                .get(5)
                .map(|value| value.parse::<f64>())
                .transpose()
                .map_err(|_| "duration must be a number".to_string())?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let mut queue = load_encode_queue(&bytes)?;
            let backend = discover_ffmpeg(&candidates).map_err(|e| e.to_string())?;
            let job = queue
                .jobs
                .get_mut(index)
                .ok_or("job index is out of bounds")?;
            let plan = job
                .plan(&backend, ExecutionPolicy::default())
                .map_err(|e| e.to_string())?;
            execute_job(job, &plan, duration, |progress| {
                eprintln!("progress: {:.1}%", progress * 100.0);
            })
            .map_err(|e| e.to_string())?;
            let bytes = save_encode_queue(&queue)?;
            std::fs::write(in_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("completed job {} and updated {in_path}", index + 1);
        }
        "recover" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let mut queue = load_encode_queue(&bytes)?;
            let count = queue.recover_interrupted();
            std::fs::write(in_path, save_encode_queue(&queue)?)
                .map_err(|e| format!("write error: {e}"))?;
            println!("requeued {count} interrupted job(s)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let queue = load_encode_queue(&bytes)?;
            println!("Encode Batch Queue: {}", queue.name);
            println!("Pending Jobs: {}", queue.pending_count());
            println!("Aggregate Progress: {:.3}", queue.progress());
            println!("Jobs ({} total):", queue.jobs.len());
            for job in &queue.jobs {
                println!(
                    "  Job {}: {} -> {} [{}] status: {:?}",
                    job.id, job.source_file, job.output_file, job.preset.name, job.status
                );
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}

fn shell_join(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            if argument
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-._/:".contains(character))
            {
                argument.clone()
            } else {
                format!("\"{}\"", argument.replace('\"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
