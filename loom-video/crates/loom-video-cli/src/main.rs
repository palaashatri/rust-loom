//! Command line tool for Loom Video.

use loom_video_core::{load_video_project, save_video_project, Clip, VideoProject};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Video CLI");
        println!("Usage:");
        println!("  loom-video-cli create <output.loomvideo> <name>");
        println!("  loom-video-cli inspect <input.loomvideo>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args.get(3).map(|s| s.as_str()).unwrap_or("Untitled Video");
            let mut proj = VideoProject::new("cli-video", name);
            proj.tracks[0].add_clip(Clip::new("c-intro", "Intro_Shot.mov", 4.5));
            proj.tracks[0].add_clip(Clip::new("c-interview", "Interview_A.mov", 15.0));
            let bytes = save_video_project(&proj)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created video project: {out_path} (2 tracks, 2 clips)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let proj = load_video_project(&bytes)?;
            println!("Video Project: {}", proj.name);
            println!(
                "Format: {}x{} @ {} fps",
                proj.width, proj.height, proj.frame_rate
            );
            println!("Tracks ({} total):", proj.tracks.len());
            for t in &proj.tracks {
                println!(
                    "  Track '{}' [{:?}]: {} clips",
                    t.name,
                    t.track_type,
                    t.clips.len()
                );
                for c in &t.clips {
                    println!("    - Clip: {} ({:.1}s)", c.name, c.duration);
                }
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
