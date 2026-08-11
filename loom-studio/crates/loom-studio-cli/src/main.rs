//! Command line interface for Loom Studio DAW.

use loom_studio_core::{
    load_studio_project, save_studio_project, synthesize_notes, AudioBuffer, MidiNote,
    StudioProject, StudioTrack, TrackKind,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Studio CLI");
        println!("Usage:");
        println!("  loom-studio-cli create <output.loomstudio> <name> [bpm]");
        println!("  loom-studio-cli inspect <input.loomstudio>");
        println!("  loom-studio-cli sine <output.wav> <frequency-hz> <seconds>");
        println!("  loom-studio-cli synth <output.wav> <midi-key> [seconds]");
        println!("  loom-studio-cli validate <input.loomstudio>");
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
        "sine" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let frequency = args
                .get(3)
                .ok_or("missing frequency")?
                .parse::<f32>()
                .map_err(|_| "frequency must be a number".to_string())?;
            let seconds = args
                .get(4)
                .ok_or("missing duration")?
                .parse::<f32>()
                .map_err(|_| "duration must be a number".to_string())?;
            let audio = AudioBuffer::sine(48_000, 2, frequency, seconds, 0.25)?;
            std::fs::write(out_path, audio.to_wav_pcm16()?)
                .map_err(|e| format!("write error: {e}"))?;
            println!(
                "Rendered oscillator: {out_path} ({:.3}s)",
                audio.duration_secs()
            );
        }
        "synth" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let key = args
                .get(3)
                .ok_or("missing MIDI key")?
                .parse::<u8>()
                .map_err(|_| "MIDI key must be an integer".to_string())?;
            let seconds = args
                .get(4)
                .map(|value| value.parse::<f64>())
                .transpose()
                .map_err(|_| "duration must be a number".to_string())?
                .unwrap_or(1.0);
            let audio = synthesize_notes(
                48_000,
                &[MidiNote {
                    key,
                    start_secs: 0.0,
                    duration_secs: seconds,
                    velocity: 0.8,
                }],
                0.05,
            )?;
            std::fs::write(out_path, audio.to_wav_pcm16()?)
                .map_err(|e| format!("write error: {e}"))?;
            println!("Rendered MIDI note {key}: {out_path}");
        }
        "validate" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let project = load_studio_project(&bytes)?;
            let issues = project.validate();
            if issues.is_empty() {
                println!("valid project: {} samples", project.duration_samples());
            } else {
                for issue in &issues {
                    println!("{issue}");
                }
                return Err(format!("{} validation issue(s)", issues.len()));
            }
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
