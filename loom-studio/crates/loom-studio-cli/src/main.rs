//! Command line interface for Loom Studio DAW.

use loom_studio_core::{
    load_studio_project, save_studio_project, StudioProject, StudioTrack, TrackKind,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Studio CLI");
        println!("Usage:");
        println!("  loom-studio-cli create <output.loomstudio> <name> [bpm]");
        println!("  loom-studio-cli inspect <input.loomstudio>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args.get(3).map(|s| s.as_str()).unwrap_or("Untitled Song");
            let bpm: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(120.0);
            let mut proj = StudioProject::new("cli-studio", name);
            proj.bpm = bpm;
            proj.add_track(StudioTrack::new(
                "t-drums",
                "Drummer Track",
                TrackKind::Drummer,
            ));
            let bytes = save_studio_project(&proj)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created DAW project: {out_path} (BPM: {bpm}, 3 tracks)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let proj = load_studio_project(&bytes)?;
            println!("Loom Studio Project: {}", proj.name);
            println!(
                "Tempo: {} BPM | Rate: {} Hz | Mode: {:?}",
                proj.bpm, proj.sample_rate, proj.mode
            );
            println!("Tracks ({} total):", proj.tracks.len());
            for t in &proj.tracks {
                println!(
                    "  Track '{}' [{:?}] Vol: {:.1}dB (regions: {})",
                    t.name,
                    t.kind,
                    t.volume_db,
                    t.regions.len()
                );
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
