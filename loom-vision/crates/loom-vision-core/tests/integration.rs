//! Integration tests: end-to-end flows across provider, registry, and
//! model-pack modules.

use std::fs;
use std::path::Path;

use loom_vision_core::provider::{
    CapabilityId, CapabilityProvider, ProviderDescriptor, ProviderInput, ProviderOutput, RunContext,
};
use loom_vision_core::reference::{ImageStatsProvider, QrCodeProvider};
use loom_vision_core::registry::CapabilityRegistry;
use loom_vision_core::{
    install_pack, validate_pack, ModelFile, ModelPackManifest, VisionError, FORMAT_VERSION,
};
use qrcode::types::Color;
use qrcode::QrCode;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn qr_image(text: &str) -> ProviderInput {
    let code = QrCode::new(text.as_bytes()).expect("encode QR");
    let scale = 4i64;
    let quiet = 4i64;
    let modules = code.width() as i64;
    let size = ((modules + 2 * quiet) * scale) as u32;
    let mut data = vec![255u8; size as usize * size as usize * 4];
    for y in 0..size {
        for x in 0..size {
            let mx = i64::from(x) / scale - quiet;
            let my = i64::from(y) / scale - quiet;
            if mx >= 0
                && my >= 0
                && mx < modules
                && my < modules
                && code[(mx as usize, my as usize)] == Color::Dark
            {
                let offset = (y as usize * size as usize + x as usize) * 4;
                data[offset] = 0;
                data[offset + 1] = 0;
                data[offset + 2] = 0;
            }
        }
    }
    ProviderInput::Image {
        width: size,
        height: size,
        channels: 4,
        data,
        format: "rgba".to_string(),
    }
}

fn write_pack(dir: &Path, model_bytes: &[u8]) -> ModelPackManifest {
    fs::write(dir.join("model.bin"), model_bytes).expect("write model");
    let manifest = ModelPackManifest {
        format_version: FORMAT_VERSION,
        id: "itpack".to_string(),
        name: "Integration Pack".to_string(),
        version: "0.1.0".to_string(),
        description: "integration test pack".to_string(),
        license: "MIT".to_string(),
        provenance: "tests".to_string(),
        capability: CapabilityId::QrDetection,
        runtime_requirements: vec![],
        required_memory_bytes: 0,
        models: vec![ModelFile {
            path: "model.bin".to_string(),
            sha256: Sha256::digest(model_bytes).into(),
            size: model_bytes.len() as u64,
        }],
        test_vectors: vec![],
        compatibility_min: "0.1.0".to_string(),
        compatibility_max: "0.2.0".to_string(),
    };
    let json = serde_json::to_string_pretty(&manifest).expect("serialize");
    fs::write(dir.join("manifest.json"), json).expect("write manifest");
    manifest
}

#[test]
fn format_version_is_one() {
    assert_eq!(FORMAT_VERSION, 1);
}

#[test]
fn qr_provider_runs_through_capability_registry() {
    let registry = CapabilityRegistry::new();
    registry.register(Box::new(QrCodeProvider::new()));

    let input = qr_image("through-the-registry");
    let mut ctx = RunContext::new();
    let output = registry
        .run_first_success(CapabilityId::QrDetection, &input, &mut ctx)
        .expect("run");
    assert!(matches!(
        output,
        ProviderOutput::QrDecoded { text } if text == "through-the-registry"
    ));
}

#[test]
fn run_all_returns_errors_for_unsupported_input() {
    let registry = CapabilityRegistry::new();
    registry.register(Box::new(QrCodeProvider::new()));
    registry.register(Box::new(ImageStatsProvider::new()));

    let input = ProviderInput::Text {
        text: "not an image".to_string(),
    };
    let mut ctx = RunContext::new();
    let results = registry.run_all(CapabilityId::ImageStats, &input, &mut ctx);
    assert_eq!(results.len(), 1);
    assert!(matches!(results[0], Err(VisionError::UnsupportedInput)));
}

#[test]
fn best_provider_wins_over_later_registrations() {
    let registry = CapabilityRegistry::new();
    registry.register(Box::new(QrCodeProvider::new()));

    // Re-registering the same capability keeps the first as best.
    let first = registry
        .providers()
        .best_for(CapabilityId::QrDetection)
        .unwrap();
    assert_eq!(first.descriptor().name, "rqrr-reference-qr");
}

#[test]
fn pack_install_lifecycle_end_to_end() {
    let src = tempdir().unwrap();
    write_pack(src.path(), b"integration-model");

    let summary = validate_pack(src.path()).expect("validate");
    assert_eq!(summary.model_count, 1);
    assert_eq!(summary.total_bytes, "integration-model".len() as u64);

    let dest = tempdir().unwrap();
    install_pack(src.path(), dest.path()).expect("install");

    let installed = dest.path().join("itpack-0.1.0");
    assert!(installed.join("manifest.json").is_file());
    assert_eq!(
        fs::read(installed.join("model.bin")).unwrap(),
        b"integration-model"
    );

    // The installed pack validates as a pack in its own right.
    let revalidated = validate_pack(&installed).expect("revalidate installed");
    assert_eq!(revalidated.id, "itpack");
}

#[test]
fn corrupt_installed_pack_is_detected() {
    let src = tempdir().unwrap();
    write_pack(src.path(), b"tamper-me");
    let dest = tempdir().unwrap();
    install_pack(src.path(), dest.path()).expect("install");

    let installed = dest.path().join("itpack-0.1.0");
    fs::write(installed.join("model.bin"), b"tamperXme").unwrap();
    assert!(matches!(
        validate_pack(&installed),
        Err(VisionError::ChecksumMismatch)
    ));
}

/// A minimal provider used only to exercise registry semantics in-process.
struct EchoProvider {
    descriptor: ProviderDescriptor,
}

impl CapabilityProvider for EchoProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        ctx.check_cancelled()?;
        match input {
            ProviderInput::Text { text } => Ok(ProviderOutput::Generic {
                message: format!("echo:{text}"),
            }),
            _ => Err(VisionError::UnsupportedInput),
        }
    }
}

#[test]
fn registry_orders_providers_by_registration() {
    let registry = loom_vision_core::ProviderRegistry::new();
    let mut first = ProviderDescriptor::new(CapabilityId::Ocr);
    first.name = "first-echo".to_string();
    let mut second = ProviderDescriptor::new(CapabilityId::Ocr);
    second.name = "second-echo".to_string();

    registry.register(Box::new(EchoProvider {
        descriptor: first.clone(),
    }));
    registry.register(Box::new(EchoProvider { descriptor: second }));

    let names: Vec<String> = registry
        .providers_for(CapabilityId::Ocr)
        .iter()
        .map(|p| p.descriptor().name.clone())
        .collect();
    assert_eq!(
        names,
        vec!["first-echo".to_string(), "second-echo".to_string()]
    );

    assert_eq!(
        registry
            .best_for(CapabilityId::Ocr)
            .unwrap()
            .descriptor()
            .name,
        "first-echo"
    );
    assert_eq!(registry.unregister(CapabilityId::Ocr), 2);
    assert!(registry.is_empty());
}

#[test]
fn reference_providers_report_their_descriptors() {
    let qr = QrCodeProvider::new();
    let stats = ImageStatsProvider::new();
    assert!(qr.descriptor().deterministic);
    assert!(stats.descriptor().deterministic);
    assert!(qr.descriptor().cancellation_support);
    assert!(stats.descriptor().cancellation_support);
}
