//! Command line tool for Loom Video.

use loom_video_core::{
    load_video_project, save_video_project, CaptionCue, Clip, TimelineMarker, VideoProject,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Video CLI");
        println!("Usage:");
        println!("  loom-video-cli create <output.loomvideo> <name>");
        println!("  loom-video-cli edit-demo <input.loomvideo>");
        println!("  loom-video-cli inspect <input.loomvideo>");
        println!("  loom-video-cli edl <input.loomvideo> [output.edl]");
        println!("  loom-video-cli plan <input.loomvideo>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args.get(3).map(|s| s.as_str()).unwrap_or("Untitled Video");
            let mut project = VideoProject::new("cli-video", name);

            let mut intro = Clip::new("c-intro", "Intro Shot", 4.5);
            intro.source_path = "Intro_Shot.mov".to_string();
            intro.start_time = 0.0;

            let mut interview = Clip::new("c-interview", "Interview A", 15.0);
            interview.source_path = "Interview_A.mov".to_string();
            interview.start_time = intro.end_time();

            project.tracks[0].add_clip(intro);
            project.tracks[0].add_clip(interview);
            project.tracks[0].sort_clips();

            let bytes = save_video_project(&project)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created video project: {out_path} (2 tracks, 2 clips)");
        }
        "edit-demo" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let mut project = load_video_project(&bytes)?;

            let video_track = project.tracks.get_mut(0).ok_or("missing video track")?;
            let intro = video_track
                .clips
                .iter_mut()
                .find(|clip| clip.id == "c-intro")
                .ok_or("missing c-intro")?;
            intro.trim_out(3.0).map_err(|error| error.to_string())?;

            project
                .split_clip(0, "c-interview", 10.5)
                .map_err(|error| error.to_string())?;
            project
                .move_clip(0, 0, "c-interview-a", 3.0, false)
                .map_err(|error| error.to_string())?;

            let video_track = project.tracks.get_mut(0).ok_or("missing video track")?;
            let right = video_track
                .clips
                .iter_mut()
                .find(|clip| clip.id == "c-interview-b")
                .ok_or("missing split clip")?;
            right
                .set_playback_rate(1.5)
                .map_err(|error| error.to_string())?;
            right.start_time = 9.0;
            video_track.sort_clips();

            project
                .add_marker(TimelineMarker {
                    id: "marker-interview".to_string(),
                    time: 3.0,
                    label: "Interview begins".to_string(),
                    color: "copper".to_string(),
                })
                .map_err(|error| error.to_string())?;
            project
                .add_caption(CaptionCue {
                    id: "caption-intro".to_string(),
                    start: 0.25,
                    end: 2.75,
                    text: "Loom native timeline journey".to_string(),
                    language: "en".to_string(),
                })
                .map_err(|error| error.to_string())?;

            let bytes = save_video_project(&project)?;
            std::fs::write(in_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!(
                "Edited video project: {} clips, {} marker, {} caption, duration {:.3}s",
                project.total_clips(),
                project.markers.len(),
                project.captions.len(),
                project.duration()
            );
        }
        "edl" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let project = load_video_project(&bytes)?;
            let edl = project.to_edl();
            if let Some(out_path) = args.get(3) {
                std::fs::write(out_path, edl).map_err(|e| format!("write error: {e}"))?;
                println!("Exported EDL: {out_path}");
            } else {
                print!("{edl}");
            }
        }
        "plan" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let project = load_video_project(&bytes)?;
            println!("duration: {:.3}s", project.duration());
            for segment in project.render_plan() {
                println!(
                    "{}:{} {:.3}..{:.3} source {:.3}..{:.3} rate {:.3} path={} proxy={}",
                    segment.track_id,
                    segment.clip_id,
                    segment.timeline_start,
                    segment.timeline_end,
                    segment.source_in,
                    segment.source_out,
                    segment.playback_rate,
                    segment.source_path,
                    segment.proxy_path.as_deref().unwrap_or("-")
                );
            }
            for track in &project.tracks {
                for (left, right) in track.overlaps() {
                    eprintln!(
                        "warning: track {} clips {left} and {right} overlap",
                        track.id
                    );
                }
            }
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let project = load_video_project(&bytes)?;
            println!("Video Project: {}", project.name);
            println!(
                "Format: {}x{} @ {} fps",
                project.width, project.height, project.frame_rate
            );
            println!("Duration: {:.3}s", project.duration());
            println!(
                "Timeline: {} clips, {} markers, {} captions",
                project.total_clips(),
                project.markers.len(),
                project.captions.len()
            );
            println!("Tracks ({} total):", project.tracks.len());
            for track in &project.tracks {
                println!(
                    "  Track '{}' [{:?}]: {} clips",
                    track.name,
                    track.track_type,
                    track.clips.len()
                );
                for clip in &track.clips {
                    println!(
                        "    - {} [{}]: {:.3}..{:.3}s, source {:.3}..{:.3}, rate {:.3}",
                        clip.name,
                        clip.id,
                        clip.start_time,
                        clip.end_time(),
                        clip.in_point,
                        clip.out_point,
                        clip.playback_rate
                    );
                }
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
