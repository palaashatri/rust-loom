//! CLI tool for Loom Motion.

use loom_motion_core::{load_motion, save_motion, CompositionDocument, MotionLayer};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Motion CLI");
        println!("Usage:");
        println!("  loom-motion-cli create <output.loommotion> <name>");
        println!("  loom-motion-cli inspect <input.loommotion>");
        println!("  loom-motion-cli frame <input.loommotion> <frame-index>");
        println!("  loom-motion-cli validate <input.loommotion>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args
                .get(3)
                .map(|s| s.as_str())
                .unwrap_or("Untitled Composition");
            let mut doc = CompositionDocument::new("cli-comp", name);
            doc.add_layer(MotionLayer::new("l-bg", "Solid Background", "VectorShape"));
            let bytes = save_motion(&doc)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created motion composition: {out_path} (2 layers, 60fps)");
        }
        "frame" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let frame_index = args
                .get(3)
                .ok_or("missing frame index")?
                .parse::<u64>()
                .map_err(|_| "frame index must be an integer".to_string())?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_motion(&bytes)?;
            let frame = doc.frame(frame_index);
            println!("frame {} at {:.3}s", frame.frame_index, frame.time_secs);
            for layer in frame.layers {
                println!(
                    "{} visible={} x={:.2} y={:.2} opacity={:.3} scale={:.3}",
                    layer.name, layer.visible, layer.x, layer.y, layer.opacity, layer.scale
                );
            }
        }
        "validate" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_motion(&bytes)?;
            let issues = doc.validate();
            if issues.is_empty() {
                println!("valid composition: {} frames", doc.duration_frames());
            } else {
                for issue in &issues {
                    println!("layer {:?}: {}", issue.layer_id, issue.message);
                }
                return Err(format!("{} validation issue(s)", issues.len()));
            }
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_motion(&bytes)?;
            println!("Composition: {}", doc.name);
            println!(
                "Resolution: {}x{} @ {} fps",
                doc.width, doc.height, doc.frame_rate
            );
            println!("Duration: {}s", doc.duration_secs);
            println!("Layers:");
            for (idx, layer) in doc.layers.iter().enumerate() {
                println!(
                    "  Layer {}: {} [{}] (keys: x={}, y={}, opacity={})",
                    idx + 1,
                    layer.name,
                    layer.layer_type,
                    layer.position_x_keys.len(),
                    layer.position_y_keys.len(),
                    layer.opacity_keys.len()
                );
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
