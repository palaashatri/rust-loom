use super::{AudioBuffer, StudioProject};
use serde::{Deserialize, Serialize};

pub(super) const STUDIO_BUNDLE_FORMAT_VERSION: u32 = 2;

pub(super) fn validate_format_version(version: u32) -> Result<(), String> {
    if (1..=STUDIO_BUNDLE_FORMAT_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(format!("unsupported Studio format version {version}"))
    }
}

pub(super) fn to_wav_float32(audio: &AudioBuffer) -> Result<Vec<u8>, String> {
    audio.validate()?;
    let data_len = audio
        .samples
        .len()
        .checked_mul(4)
        .ok_or_else(|| "WAV data length overflow".to_string())?;
    if data_len > u32::MAX as usize - 36 {
        return Err("WAV output exceeds the 4 GiB RIFF limit".into());
    }
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(data_len + 44));
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).map_err(|error| error.to_string())?;
        for &sample in &audio.samples {
            writer
                .write_sample(sample)
                .map_err(|error| error.to_string())?;
        }
        writer.finalize().map_err(|error| error.to_string())?;
    }
    Ok(cursor.into_inner())
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StudioBundleMetadata {
    #[serde(default = "legacy_studio_bundle_format_version")]
    pub(super) format_version: u32,
    pub(super) project: StudioProject,
    pub(super) assets: Vec<StudioBundleAsset>,
}

fn legacy_studio_bundle_format_version() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StudioBundleAsset {
    pub(super) name: String,
    pub(super) path: String,
}
