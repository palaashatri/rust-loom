from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
core_path = ROOT / "loom-motion/crates/loom-motion-core/src/lib.rs"
app_path = ROOT / "loom-motion/crates/loom-motion-app/src/main.rs"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


core = core_path.read_text()
core = replace_once(
    core,
    "    pub scale_keys: Vec<Keyframe>,\n}",
    "    pub scale_keys: Vec<Keyframe>,\n    #[serde(default)]\n    pub rotation_keys: Vec<Keyframe>,\n}",
    "rotation field",
)
core = replace_once(
    core,
    "            scale_keys: Vec::new(),\n        }",
    "            scale_keys: Vec::new(),\n            rotation_keys: Vec::new(),\n        }",
    "rotation initialization",
)
keyframe_match = '            "scale" => Some(&mut self.scale_keys),\n            _ => None,'
keyframe_replacement = '            "scale" => Some(&mut self.scale_keys),\n            "rotation" => Some(&mut self.rotation_keys),\n            _ => None,'
if core.count(keyframe_match) != 2:
    raise RuntimeError(
        f"rotation keyframe match sites: expected two, found {core.count(keyframe_match)}"
    )
core = core.replace(keyframe_match, keyframe_replacement, 1)
core = replace_once(
    core,
    "    /// Uniform scale, where `1` is original size.\n    pub scale: f32,\n    /// Whether the layer is active at this time.",
    "    /// Uniform scale, where `1` is original size.\n    pub scale: f32,\n    /// Rotation in degrees.\n    pub rotation: f32,\n    /// Whether the layer is active at this time.",
    "sample rotation field",
)
core = core.replace(keyframe_match, keyframe_replacement, 1)
core = replace_once(
    core,
    "            scale: sample_keys(&self.scale_keys, local_time, 1.0).max(0.0),\n            visible,",
    "            scale: sample_keys(&self.scale_keys, local_time, 1.0).max(0.0),\n            rotation: sample_keys(&self.rotation_keys, local_time, 0.0),\n            visible,",
    "sample rotation",
)
core = replace_once(
    core,
    '                ("scale", &layer.scale_keys),\n            ] {',
    '                ("scale", &layer.scale_keys),\n                ("rotation", &layer.rotation_keys),\n            ] {',
    "validate rotation",
)
core_path.write_text(core)

app = app_path.read_text()
app = replace_once(
    app,
    '''fn sample_motion() -> CompositionDocument {
    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");
    doc.add_layer(MotionLayer::new("l-sub", "Subtitle Motion", "Text"));
    doc
}''',
    '''fn sample_motion() -> CompositionDocument {
    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");
    doc.add_layer(MotionLayer::new("l-sub", "Subtitle Motion", "Text"));
    for layer in &mut doc.layers {
        layer.add_keyframe("x", 0.0, 960.0);
        layer.add_keyframe("y", 0.0, 540.0);
        layer.add_keyframe("scale", 0.0, 1.0);
        layer.add_keyframe("rotation", 0.0, 0.0);
        if layer.opacity_keys.is_empty() {
            layer.add_keyframe("opacity", 0.0, 1.0);
        }
    }
    doc
}''',
    "sample transforms",
)
app = replace_once(
    app,
    '''    app.set_active_layer_index(doc.active_layer_index as i32);
    app.set_status_left(SharedString::from(format!(''',
    '''    app.set_active_layer_index(doc.active_layer_index as i32);
    if let Some(layer) = doc.layers.get(doc.active_layer_index) {
        let sample = layer.sample(0.0);
        app.set_pos_x(sample.x);
        app.set_pos_y(sample.y);
        app.set_scale_val(sample.scale * 100.0);
        app.set_rotation_val(sample.rotation);
        app.set_opacity_val(sample.opacity * 100.0);
    }
    app.set_status_left(SharedString::from(format!(''',
    "apply sampled transforms",
)
app = replace_once(
    app,
    '''                current.add_layer(MotionLayer::new(
                    format!("layer-{count}"),
                    format!("Motion Layer {count}"),
                    "VectorShape",
                ));
                apply_motion(&app, &current);''',
    '''                let mut layer = MotionLayer::new(
                    format!("layer-{count}"),
                    format!("Motion Layer {count}"),
                    "VectorShape",
                );
                layer.add_keyframe("x", 0.0, 960.0);
                layer.add_keyframe("y", 0.0, 540.0);
                layer.add_keyframe("scale", 0.0, 1.0);
                layer.add_keyframe("rotation", 0.0, 0.0);
                layer.add_keyframe("opacity", 0.0, 1.0);
                current.add_layer(layer);
                current.active_layer_index = current.layers.len().saturating_sub(1);
                apply_motion(&app, &current);''',
    "new layer defaults",
)
app = replace_once(
    app,
    '''    {
        let app_ref = app.as_weak();
        app.on_transform_changed(move |prop, val| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Transform {prop}: {val:.1}")));
            }
        });
    }''',
    '''    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_transform_changed(move |prop, val| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let active = current.active_layer_index;
                let property = match prop.as_str() {
                    "pos-x" => "x",
                    "pos-y" => "y",
                    "scale" => "scale",
                    "rotation" => "rotation",
                    "opacity" => "opacity",
                    _ => {
                        app.set_status_left(
                            SharedString::from(format!("Unsupported transform property: {prop}")),
                        );
                        return;
                    }
                };
                let stored_value = match property {
                    "scale" | "opacity" => val / 100.0,
                    _ => val,
                };
                if let Some(layer) = current.layers.get_mut(active) {
                    layer.add_keyframe(property, 0.0, stored_value);
                    let layer_name = layer.name.clone();
                    apply_motion(&app, &current);
                    app.set_status_left(SharedString::from(format!(
                        "Updated {layer_name} {property} to {val:.1}"
                    )));
                }
            }
        });
    }''',
    "transform callback",
)
app_path.write_text(app)
