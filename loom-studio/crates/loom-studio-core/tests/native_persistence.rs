use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use loom_studio_core::{
    load_studio_bundle, save_studio_bundle, AudioAssetStore, AudioBuffer, StudioProject,
};

fn project() -> StudioProject {
    StudioProject::new("native-persistence", "Native persistence")
}

fn asset_store(samples: Vec<f32>) -> AudioAssetStore {
    let mut assets = AudioAssetStore::default();
    assets
        .insert(
            "precision.wav",
            AudioBuffer {
                sample_rate: 48_000,
                channels: 1,
                samples,
            },
        )
        .unwrap();
    assets
}

fn legacy_bundle(project: &StudioProject, audio: &AudioBuffer) -> Vec<u8> {
    let audio_path = "assets/audio-0000.wav";
    let audio_bytes = audio.to_wav_pcm16().unwrap();
    let content = serde_json::json!({
        "project": project,
        "assets": [{"name": "precision.wav", "path": audio_path}],
    });
    let content_bytes = serde_json::to_vec_pretty(&content).unwrap();
    let entries = vec![
        ManifestEntry {
            path: audio_path.into(),
            mime: MimeType::parse("audio/wav").unwrap(),
            size: audio_bytes.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&audio_bytes)),
        },
        ManifestEntry {
            path: "content/studio.json".into(),
            mime: MimeType::parse("application/vnd.loom.studio-content").unwrap(),
            size: content_bytes.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&content_bytes)),
        },
    ];
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Studio,
        id: project.id.clone(),
        title: project.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        entries,
    };
    let mut archive = PackageArchive::new();
    archive.add(audio_path, audio_bytes).unwrap();
    archive.add("content/studio.json", content_bytes).unwrap();
    archive
        .add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .unwrap();
    archive.to_bytes().unwrap()
}

#[test]
fn native_bundle_round_trip_preserves_float_sample_bits_across_saves() {
    let source_samples = vec![0.000001, 0.75, -0.25, -1.0, 1.0];
    let source = asset_store(source_samples.clone());
    let project = project();
    let mut current = source;

    for _ in 0..3 {
        let bytes = save_studio_bundle(&project, &current).unwrap();
        let archive = PackageArchive::from_bytes(&bytes).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(archive.get("content/studio.json").unwrap()).unwrap();
        assert_eq!(metadata["format_version"], 2);

        let (_, loaded) = load_studio_bundle(&bytes).unwrap();
        let audio = loaded.get("precision.wav").unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.channels, 1);
        assert_eq!(audio.frames(), source_samples.len() as u64);
        assert_eq!(
            audio
                .samples
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>(),
            source_samples
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        );
        current = loaded;
    }
}

#[test]
fn legacy_bundle_without_format_version_still_loads_pcm16_assets() {
    let source = AudioBuffer {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.75, -1.0],
    };
    let bytes = legacy_bundle(&project(), &source);
    let (_, assets) = load_studio_bundle(&bytes).unwrap();
    let loaded = assets.get("precision.wav").unwrap();
    assert_eq!(loaded.sample_rate, source.sample_rate);
    assert_eq!(loaded.channels, source.channels);
    assert_eq!(loaded.frames(), source.frames());
    assert_ne!(loaded.samples, source.samples);
}
