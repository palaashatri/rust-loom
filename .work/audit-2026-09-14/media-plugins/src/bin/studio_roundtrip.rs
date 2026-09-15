use loom_studio_core::{AudioAssetStore, AudioBuffer, StudioProject, save_studio_bundle, load_studio_bundle};

fn main() {
    let project = StudioProject::new("audit", "Audit audio");
    let mut assets = AudioAssetStore::default();
    assets.insert("test.wav", AudioBuffer {
        sample_rate: 48000,
        channels: 1,
        samples: vec![0.000001, 0.75, -1.0],
    }).unwrap();
    println!("Original samples: {:?}", assets.get("test.wav").unwrap().samples);
    for iteration in 1..=3 {
        let bytes = save_studio_bundle(&project, &assets).unwrap();
        (_, assets) = load_studio_bundle(&bytes).unwrap();
        println!("After native save/reopen {iteration}: {:?}", assets.get("test.wav").unwrap().samples);
    }
}
