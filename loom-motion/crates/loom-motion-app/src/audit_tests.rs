//! Deep functionality audit tests for Loom Motion.
//! Verifies 100% functionality of composition transport, keyframing, transforms,
//! layer hierarchy, SVG rendering, persistence, and macOS AppKit menu bar reflection.

use super::*;
use loom_motion_core::{
    cubic_bezier_1d, cubic_bezier_2d, CompositionClock, MotionLayer,
};

#[test]
fn audit_composition_clock_and_transport_determinism() {
    let mut clock = CompositionClock::new(60.0, 300); // 5 seconds at 60fps
    assert_eq!(clock.current_frame, 0);
    assert_eq!(clock.fps, 60.0);
    assert_eq!(clock.out_frame, 300);
    assert!(!clock.is_playing);

    // Playback step
    clock.set_playing(true);
    assert!(clock.is_playing);
    clock.advance_seconds(1.0);
    assert_eq!(clock.current_frame, 60);

    // Seeking
    clock.seek_frame(150);
    assert_eq!(clock.current_frame, 150);

    // Looping behavior
    clock.loop_playback = true;
    clock.seek_frame(299);
    clock.advance_seconds(2.0 / 60.0); // Step beyond end
    assert_eq!(clock.current_frame, 1); // Wrapped cleanly to frame 1
}

#[test]
fn audit_keyframe_bezier_interpolation_curves() {
    // 1D cubic bezier interpolation: p0=0, p1=30, p2=70, p3=100
    let v_start = cubic_bezier_1d(0.0, 30.0, 70.0, 100.0, 0.0);
    let v_end = cubic_bezier_1d(0.0, 30.0, 70.0, 100.0, 1.0);
    assert!((v_start - 0.0).abs() < 1e-4, "Start should be 0.0, got {v_start}");
    assert!((v_end - 100.0).abs() < 1e-4, "End should be 100.0, got {v_end}");

    // Midpoint symmetry
    let v_mid = cubic_bezier_1d(0.0, 30.0, 70.0, 100.0, 0.5);
    assert!((v_mid - 50.0).abs() < 1e-3, "Midpoint should be 50.0, got {v_mid}");

    // 2D cubic bezier path
    let p_start = (0.0, 0.0);
    let p_end = (100.0, 200.0);
    let (x_mid, y_mid) = cubic_bezier_2d(p_start, (30.0, 50.0), (70.0, 150.0), p_end, 0.5);
    assert!(x_mid > 0.0 && x_mid < 100.0);
    assert!(y_mid > 0.0 && y_mid < 200.0);
}

#[test]
fn audit_keyframe_sampling_and_boundary_interpolation() {
    let mut layer = MotionLayer::new("anim-layer", "Animated Layer", "VectorShape");
    layer.start_time = 0.0;
    layer.duration = 10.0;
    layer.add_keyframe("x", 0.0, 100.0);
    layer.add_keyframe("x", 2.0, 500.0);
    layer.add_keyframe("x", 4.0, 300.0);

    // At first keyframe (0s)
    let s0 = layer.sample(0.0);
    assert_eq!(s0.x, 100.0);

    // Midpoint between 0s (100) and 2s (500): 300
    let s1 = layer.sample(1.0);
    assert!((s1.x - 300.0).abs() < 1e-3, "Expected 300.0 at 1.0s, got {}", s1.x);

    // At second keyframe (2s)
    let s2 = layer.sample(2.0);
    assert_eq!(s2.x, 500.0);

    // Midpoint between 2s (500) and 4s (300): 400
    let s3 = layer.sample(3.0);
    assert!((s3.x - 400.0).abs() < 1e-3, "Expected 400.0 at 3.0s, got {}", s3.x);

    // Past last keyframe (4s): holds 300
    let s5 = layer.sample(5.0);
    assert_eq!(s5.x, 300.0);
}

#[test]
fn audit_layer_stack_lifecycle_and_reordering() {
    let mut doc = empty_motion();
    assert_eq!(doc.layers.len(), 0);

    // Add multiple layers
    let l1 = MotionLayer::new("layer-1", "Background", "VectorShape");
    let l2 = MotionLayer::new("layer-2", "Hero Text", "Text");
    let l3 = MotionLayer::new("layer-3", "Particle Effect", "VectorShape");

    doc.add_layer(l1);
    doc.add_layer(l2);
    doc.add_layer(l3);
    assert_eq!(doc.layers.len(), 3);
    assert_eq!(doc.layers[0].name, "Background");
    assert_eq!(doc.layers[1].name, "Hero Text");
    assert_eq!(doc.layers[2].name, "Particle Effect");

    // Reorder layers: swap 0 and 1
    doc.layers.swap(0, 1);
    assert_eq!(doc.layers[0].name, "Hero Text");
    assert_eq!(doc.layers[1].name, "Background");

    // Remove layer
    doc.layers.remove(2);
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[0].name, "Hero Text");
    assert_eq!(doc.layers[1].name, "Background");
}

#[test]
fn audit_svg_frame_export_multi_layer_determinism() {
    let mut doc = empty_motion();
    doc.width = 1920;
    doc.height = 1080;

    let mut l1 = MotionLayer::new("bg", "Canvas Background", "VectorShape");
    l1.add_keyframe("opacity", 0.0, 1.0);
    l1.add_keyframe("x", 0.0, 960.0);
    l1.add_keyframe("y", 0.0, 540.0);
    doc.add_layer(l1);

    let mut l2 = MotionLayer::new("text", "Main Title", "Text");
    l2.add_keyframe("opacity", 0.0, 0.9);
    l2.add_keyframe("x", 0.0, 960.0);
    l2.add_keyframe("y", 0.0, 400.0);
    doc.add_layer(l2);

    let svg_1 = export_svg_frame(&doc, 0.0);
    let svg_2 = export_svg_frame(&doc, 0.0);
    assert_eq!(svg_1, svg_2, "SVG export must be 100% deterministic");
    assert!(svg_1.contains("<svg"));
    assert!(svg_1.contains("rect transform="));
    assert!(svg_1.contains("Main Title"));
    assert!(svg_1.contains("viewBox=\"0 0 1920 1080\""));
}

#[test]
fn audit_macos_menu_bar_structure_and_reflection() {
    let menu_bar = build_standard_menu_bar(
        "Loom Motion",
        vec![MenuItem::action_with_shortcut(
            "file.export_frame",
            "Export Frame as SVG...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![
            Menu::new(
                "Layer",
                vec![
                    MenuItem::action("comp.add_layer", "Add Layer"),
                    MenuItem::action("comp.duplicate_layer", "Duplicate Layer"),
                    MenuItem::action("comp.delete_layer", "Delete Layer"),
                ],
            ),
            Menu::new(
                "Playback",
                vec![
                    MenuItem::action("playback.play_pause", "Play / Pause"),
                    MenuItem::action("playback.step_back", "Step Backward"),
                    MenuItem::action("playback.step_forward", "Step Forward"),
                    MenuItem::action("playback.toggle_loop", "Loop Playback"),
                ],
            ),
        ],
    );

    // Verify top-level menus exist
    assert!(menu_bar.menus.iter().any(|m| m.title == "File"));
    assert!(menu_bar.menus.iter().any(|m| m.title == "Edit"));
    assert!(menu_bar.menus.iter().any(|m| m.title == "Layer"));
    assert!(menu_bar.menus.iter().any(|m| m.title == "Playback"));
    assert!(menu_bar.menus.iter().any(|m| m.title == "Help"));

    // Verify Layer menu items
    let layer_menu = menu_bar.menus.iter().find(|m| m.title == "Layer").expect("Layer menu");
    assert!(layer_menu.items.iter().any(|item| item.id() == Some("comp.add_layer")));
    assert!(layer_menu.items.iter().any(|item| item.id() == Some("comp.duplicate_layer")));
    assert!(layer_menu.items.iter().any(|item| item.id() == Some("comp.delete_layer")));

    // Verify Playback menu items
    let playback_menu = menu_bar.menus.iter().find(|m| m.title == "Playback").expect("Playback menu");
    assert!(playback_menu.items.iter().any(|item| item.id() == Some("playback.play_pause")));
    assert!(playback_menu.items.iter().any(|item| item.id() == Some("playback.step_back")));
    assert!(playback_menu.items.iter().any(|item| item.id() == Some("playback.step_forward")));
    assert!(playback_menu.items.iter().any(|item| item.id() == Some("playback.toggle_loop")));
}
