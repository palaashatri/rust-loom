//! Core motion graphics and compositing engine for Loom Motion.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub time_secs: f32,
    pub value: f32,
    pub easing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionLayer {
    pub id: String,
    pub name: String,
    pub layer_type: String,
    pub start_time: f32,
    pub duration: f32,
    pub position_x_keys: Vec<Keyframe>,
    pub position_y_keys: Vec<Keyframe>,
    pub opacity_keys: Vec<Keyframe>,
    pub scale_keys: Vec<Keyframe>,
}

impl MotionLayer {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        layer_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            layer_type: layer_type.into(),
            start_time: 0.0,
            duration: 10.0,
            position_x_keys: Vec::new(),
            position_y_keys: Vec::new(),
            opacity_keys: Vec::new(),
            scale_keys: Vec::new(),
        }
    }

    pub fn add_keyframe(&mut self, property: &str, time: f32, val: f32) {
        let kf = Keyframe {
            time_secs: time,
            value: val,
            easing: "ease-in-out".to_string(),
        };
        match property {
            "x" => self.position_x_keys.push(kf),
            "y" => self.position_y_keys.push(kf),
            "opacity" => self.opacity_keys.push(kf),
            "scale" => self.scale_keys.push(kf),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionDocument {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub duration_secs: f32,
    pub layers: Vec<MotionLayer>,
}

impl CompositionDocument {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut doc = Self {
            id: id.into(),
            name: name.into(),
            width: 1920,
            height: 1080,
            frame_rate: 60.0,
            duration_secs: 10.0,
            layers: Vec::new(),
        };
        let mut title_layer = MotionLayer::new("layer-title", "Animated Title", "Text");
        title_layer.add_keyframe("opacity", 0.0, 0.0);
        title_layer.add_keyframe("opacity", 1.0, 1.0);
        doc.layers.push(title_layer);
        doc
    }

    pub fn add_layer(&mut self, layer: MotionLayer) {
        self.layers.push(layer);
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

pub fn save_motion(doc: &CompositionDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/motion.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Motion,
        id: doc.id.clone(),
        title: doc.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/motion.json".into(),
            mime: MimeType::parse("application/vnd.loom.motion-content").unwrap(),
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_motion(bytes: &[u8]) -> Result<CompositionDocument, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Motion {
        return Err("not a Motion composition".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/motion.json")
        .ok_or_else(|| "missing motion.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_creation() {
        let doc = CompositionDocument::new("comp-1", "Logo Intro Animation");
        assert_eq!(doc.frame_rate, 60.0);
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn test_motion_keyframes() {
        let mut layer = MotionLayer::new("l1", "Shape", "VectorShape");
        layer.add_keyframe("x", 0.0, 100.0);
        layer.add_keyframe("x", 2.0, 500.0);
        assert_eq!(layer.position_x_keys.len(), 2);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = CompositionDocument::new("comp-test", "Title Lower Third");
        doc.add_layer(MotionLayer::new("l2", "Background Ribbon", "VectorShape"));
        let bytes = save_motion(&doc).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Motion);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_motion(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Title Lower Third");
        assert_eq!(loaded.len(), 2);
    }
}
