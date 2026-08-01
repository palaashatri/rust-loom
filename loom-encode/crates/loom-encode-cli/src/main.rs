//! CLI batch encoder tool for Loom Encode.

use loom_encode_core::{
    load_encode_queue, save_encode_queue, EncodeJob, EncodePreset, EncodeQueue,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Encode CLI");
        println!("Usage:");
        println!("  loom-encode-cli create <output.loomencode> <name>");
        println!("  loom-encode-cli inspect <input.loomencode>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args
                .get(3)
                .map(|s| s.as_str())
                .unwrap_or("Untitled Batch Queue");
            let mut q = EncodeQueue::new("cli-queue", name);
            q.add_job(EncodeJob::new(
                "j-prores",
                "Feature_Master.mov",
                "Feature_Master_ProRes.mov",
                EncodePreset::prores_master(),
            ));
            let bytes = save_encode_queue(&q)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created encode queue: {out_path} (2 jobs queued)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let q = load_encode_queue(&bytes)?;
            println!("Encode Batch Queue: {}", q.name);
            println!("Pending Jobs: {}", q.pending_count());
            println!("Jobs ({} total):", q.jobs.len());
            for j in &q.jobs {
                println!(
                    "  Job {}: {} -> {} [{}] status: {:?}",
                    j.id, j.source_file, j.output_file, j.preset.name, j.status
                );
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
