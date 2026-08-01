//! Core nondestructive photo and image editing engine for Loom Photo.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayerKind {
    Pixel,
    Adjustment,
    Text,
    Vector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub adjustment_type: Option<String>,
    pub adjustment_value: f32,
}

impl Layer {
    pub fn new_pixel(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Pixel,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustment_type: None,
            adjustment_value: 0.0,
        }
    }

    pub fn new_adjustment(
        id: impl Into<String>,
        name: impl Into<String>,
        adj_type: impl Into<String>,
        val: f32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Adjustment,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustment_type: Some(adj_type.into()),
            adjustment_value: val,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoDocument {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub color_space: String,
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
}

impl PhotoDocument {
    pub fn new(id: impl Into<String>, name: impl Into<String>, width: u32, height: u32) -> Self {
        let mut doc = Self {
            id: id.into(),
            name: name.into(),
            width,
            height,
            dpi: 300,
            color_space: "sRGB".to_string(),
            layers: Vec::new(),
            active_layer_index: 0,
        };
        doc.layers.push(Layer::new_pixel("layer-bg", "Background"));
        doc
    }

    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
        self.active_layer_index = self.layers.len() - 1;
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn select_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.active_layer_index = index;
        true
    }
}

pub fn save_photo(doc: &PhotoDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/photo.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Photo,
        id: doc.id.clone(),
        title: doc.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/photo.json".into(),
            mime: MimeType::parse("application/vnd.loom.photo-content").unwrap(),
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_photo(bytes: &[u8]) -> Result<PhotoDocument, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Photo {
        return Err("not a Photo project".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/photo.json")
        .ok_or_else(|| "missing photo.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_photo_doc_creation() {
        let doc = PhotoDocument::new("photo-1", "Portrait Retouch", 3840, 2160);
        assert_eq!(doc.width, 3840);
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.layers[0].name, "Background");
    }

    #[test]
    fn test_add_layers() {
        let mut doc = PhotoDocument::new("photo-1", "Portrait Retouch", 1920, 1080);
        doc.add_layer(Layer::new_adjustment(
            "adj-1",
            "Exposure Adjustment",
            "Exposure",
            0.5,
        ));
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.layers[1].kind, LayerKind::Adjustment);
    }

    #[test]
    fn test_select_layer_rejects_invalid_index() {
        let mut doc = PhotoDocument::new("photo-1", "Portrait Retouch", 1920, 1080);
        doc.add_layer(Layer::new_pixel("layer-2", "Foreground"));
        assert!(doc.select_layer(0));
        assert!(!doc.select_layer(2));
        assert_eq!(doc.active_layer_index, 0);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = PhotoDocument::new("photo-test", "Landscape Composite", 4000, 3000);
        doc.add_layer(Layer::new_pixel("layer-sky", "Sky Mask"));
        let bytes = save_photo(&doc).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Photo);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_photo(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Landscape Composite");
        assert_eq!(loaded.len(), 2);
    }
}
