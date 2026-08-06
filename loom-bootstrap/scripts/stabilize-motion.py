from pathlib import Path
import re


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def regex_once(text: str, pattern: str, replacement: str, label: str) -> str:
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE | re.DOTALL)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return updated


core_path = Path("loom-motion/crates/loom-motion-core/src/lib.rs")
core = core_path.read_text()
for struct_name in ("Keyframe", "MotionLayer", "CompositionDocument"):
    core = replace_once(
        core,
        f"#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct {struct_name}",
        f"#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct {struct_name}",
        f"{struct_name} equality",
    )
core = regex_once(
    core,
    r"    pub fn new\(id: impl Into<String>, name: impl Into<String>\) -> Self \{.*?^    \}\n\n    pub fn add_layer",
    '''    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            width: 1920,
            height: 1080,
            frame_rate: 60.0,
            duration_secs: 10.0,
            layers: Vec::new(),
            active_layer_index: 0,
        }
    }

    pub fn add_layer''',
    "blank CompositionDocument constructor",
)
core = replace_once(core, "        assert_eq!(doc.len(), 1);\n", "        assert!(doc.is_empty());\n", "blank constructor test")
core = replace_once(
    core,
    '''        doc.add_layer(MotionLayer::new("l2", "Background", "VectorShape"));
        assert!(doc.select_layer(1));
        assert!(!doc.select_layer(2));
        assert_eq!(doc.active_layer_index, 1);
''',
    '''        doc.add_layer(MotionLayer::new("l2", "Background", "VectorShape"));
        assert!(doc.select_layer(0));
        assert!(!doc.select_layer(1));
        assert_eq!(doc.active_layer_index, 0);
''',
    "layer selection test",
)
core = replace_once(
    core,
    "        assert_eq!(loaded.len(), 2);\n",
    '''        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded, doc);
        let loaded_again = load_motion(&bytes).expect("second load failed");
        assert_eq!(loaded_again, loaded);
''',
    "exact idempotent core round trip",
)
core = replace_once(
    core,
    '''        let mut doc = CompositionDocument::new("comp-invalid", "Invalid");
        doc.frame_rate = 0.0;
        doc.layers[0].position_x_keys = vec![
''',
    '''        let mut doc = CompositionDocument::new("comp-invalid", "Invalid");
        doc.frame_rate = 0.0;
        doc.add_layer(MotionLayer::new("layer-invalid", "Invalid Layer", "Text"));
        doc.layers[0].position_x_keys = vec![
''',
    "validation test explicit layer",
)
core_path.write_text(core)

main_path = Path("loom-motion/crates/loom-motion-app/src/main.rs")
main = main_path.read_text()
main = replace_once(
    main,
    '    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");\n',
    '''    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");
    let mut title = MotionLayer::new("layer-title", "Animated Title", "Text");
    title.add_keyframe("opacity", 0.0, 0.0);
    title.add_keyframe("opacity", 1.0, 1.0);
    doc.add_layer(title);
''',
    "explicit Motion sample title",
)
constructor = '''    let app = MotionApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
'''
if main.count(constructor) != 3:
    raise SystemExit(f"Motion app constructors: expected 3 matches, found {main.count(constructor)}")
main = main.replace(
    constructor,
    '''    let app = MotionApp::new().map_err(|e| e.to_string())?;
    app.set_compact_layout(args.size.0 < 1180);
    apply_theme(&app, &args.theme);
''',
)
main = regex_once(
    main,
    r"    fn motion_path_round_trip_preserves_composition\(\) \{.*?^    \}\n",
    '''    fn motion_path_round_trip_preserves_composition() {
        let path = std::env::temp_dir().join(format!(
            "loom-motion-roundtrip-{}.loommotion",
            std::process::id()
        ));
        let mut document = empty_motion();
        document.width = 3840;
        document.height = 2160;
        document.frame_rate = 23.976;
        document.duration_secs = 37.5;
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.start_time = 1.25;
        layer.duration = 12.0;
        layer.add_keyframe("x", 0.0, 120.0);
        layer.add_keyframe("x", 2.0, 840.0);
        layer.add_keyframe("rotation", 1.0, 33.0);
        document.add_layer(layer);
        document.active_layer_index = 0;

        let bytes = save_motion(&document).expect("serialize");
        std::fs::write(&path, bytes).expect("write");
        let loaded = load_motion_path(&path).expect("load");
        let loaded_again = load_motion_path(&path).expect("repeated load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded, document);
        assert_eq!(loaded_again, loaded);
    }
''',
    "exact path round trip test",
)
main_path.write_text(main)

ui_path = Path("loom-motion/crates/loom-motion-app/ui/product_workspace_v2.slint")
ui = replace_once(
    ui_path.read_text(),
    "    property <bool> compact-layout: root.width < 1180px;\n",
    "    in property <bool> compact-layout: false;\n",
    "one-way compact layout state",
)
ui_path.write_text(ui)
