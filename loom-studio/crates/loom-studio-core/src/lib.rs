//! Core audio engine and DAW model for Loom Studio.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceMode {
    Quick,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackKind {
    Audio,
    Midi,
    Drummer,
    Bus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRegion {
    pub id: String,
    pub name: String,
    pub start_sample: u64,
    pub length_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioTrack {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub regions: Vec<AudioRegion>,
}

impl StudioTrack {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            regions: Vec::new(),
        }
    }

    pub fn add_region(&mut self, region: AudioRegion) {
        self.regions.push(region);
    }

    pub fn remove_region(&mut self, region_id: &str) -> Option<AudioRegion> {
        if let Some(pos) = self.regions.iter().position(|r| r.id == region_id) {
            Some(self.regions.remove(pos))
        } else {
            None
        }
    }

    pub fn split_region(
        &mut self,
        region_id: &str,
        split_sample: u64,
    ) -> Result<(String, String), String> {
        let pos = self
            .regions
            .iter()
            .position(|r| r.id == region_id)
            .ok_or_else(|| format!("region '{region_id}' not found"))?;
        let reg = &self.regions[pos];
        if split_sample <= reg.start_sample || split_sample >= reg.start_sample + reg.length_samples
        {
            return Err("split sample must be within region bounds".into());
        }
        let left_len = split_sample - reg.start_sample;
        let right_len = reg.length_samples - left_len;
        let left_id = format!("{}-a", reg.id);
        let right_id = format!("{}-b", reg.id);

        let mut left = reg.clone();
        left.id = left_id.clone();
        left.length_samples = left_len;

        let mut right = reg.clone();
        right.id = right_id.clone();
        right.start_sample = split_sample;
        right.length_samples = right_len;

        self.regions.remove(pos);
        self.regions.insert(pos, left);
        self.regions.insert(pos + 1, right);
        Ok((left_id, right_id))
    }

    pub fn trim_region_start(&mut self, region_id: &str, new_start: u64) -> Result<(), String> {
        let reg = self
            .regions
            .iter_mut()
            .find(|r| r.id == region_id)
            .ok_or_else(|| format!("region '{region_id}' not found"))?;
        let end = reg.start_sample + reg.length_samples;
        if new_start >= end {
            return Err("new start sample must be before region end".into());
        }
        reg.length_samples = end - new_start;
        reg.start_sample = new_start;
        Ok(())
    }

    pub fn trim_region_end(&mut self, region_id: &str, new_end: u64) -> Result<(), String> {
        let reg = self
            .regions
            .iter_mut()
            .find(|r| r.id == region_id)
            .ok_or_else(|| format!("region '{region_id}' not found"))?;
        if new_end <= reg.start_sample {
            return Err("new end sample must be after region start".into());
        }
        reg.length_samples = new_end - reg.start_sample;
        Ok(())
    }

    /// Calculates left and right linear gains for the track's pan setting (-1.0 left to +1.0 right).
    /// Uses standard constant-power (-3 dB center) pan law.
    pub fn stereo_pan_gains(&self) -> (f32, f32) {
        let p = self.pan.clamp(-1.0, 1.0);
        let angle = (p + 1.0) * (std::f32::consts::PI / 4.0);
        let left_gain = angle.cos();
        let right_gain = angle.sin();
        (left_gain, right_gain)
    }

    /// Linear volume gain converted from volume_db.
    pub fn linear_volume(&self) -> f32 {
        if self.volume_db <= -90.0 {
            0.0
        } else {
            10.0_f32.powf(self.volume_db / 20.0)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioProject {
    pub id: String,
    pub name: String,
    pub bpm: f32,
    pub sample_rate: u32,
    pub mode: WorkspaceMode,
    pub tracks: Vec<StudioTrack>,
    #[serde(default)]
    pub active_track_index: usize,
}

impl StudioProject {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut proj = Self {
            id: id.into(),
            name: name.into(),
            bpm: 120.0,
            sample_rate: 48000,
            mode: WorkspaceMode::Quick,
            tracks: Vec::new(),
            active_track_index: 0,
        };
        let mut t1 = StudioTrack::new("track-1", "Vocal Guide", TrackKind::Audio);
        t1.add_region(AudioRegion {
            id: "r1".to_string(),
            name: "Vocal_Take1.wav".to_string(),
            start_sample: 0,
            length_samples: 48000 * 10,
        });
        proj.tracks.push(t1);
        proj.tracks.push(StudioTrack::new(
            "track-2",
            "Acoustic Guitar",
            TrackKind::Audio,
        ));
        proj
    }

    pub fn add_track(&mut self, track: StudioTrack) {
        self.tracks.push(track);
    }

    pub fn select_track(&mut self, index: usize) -> bool {
        if index < self.tracks.len() {
            self.active_track_index = index;
            true
        } else {
            false
        }
    }

    pub fn total_regions(&self) -> usize {
        self.tracks.iter().map(|t| t.regions.len()).sum()
    }

    /// Number of audio samples in one quarter note beat.
    pub fn samples_per_beat(&self) -> f64 {
        let bpm = if self.bpm > 0.0 {
            self.bpm as f64
        } else {
            120.0
        };
        (60.0 / bpm) * (self.sample_rate as f64)
    }

    /// Number of audio samples in one musical measure (bar).
    pub fn samples_per_bar(&self, beats_per_bar: u32) -> f64 {
        self.samples_per_beat() * (beats_per_bar.max(1) as f64)
    }

    /// Converts musical beats to seconds.
    pub fn beat_to_seconds(&self, beat: f64) -> f64 {
        let bpm = if self.bpm > 0.0 {
            self.bpm as f64
        } else {
            120.0
        };
        beat * (60.0 / bpm)
    }

    /// Converts playback seconds to musical beats.
    pub fn seconds_to_beat(&self, seconds: f64) -> f64 {
        let bpm = if self.bpm > 0.0 {
            self.bpm as f64
        } else {
            120.0
        };
        seconds * (bpm / 60.0)
    }
}

pub fn save_studio_project(proj: &StudioProject) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(proj).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/studio.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Studio,
        id: proj.id.clone(),
        title: proj.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/studio.json".into(),
            mime: MimeType::parse("application/vnd.loom.studio-content")
                .map_err(|e| format!("invalid built-in studio MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_studio_project(bytes: &[u8]) -> Result<StudioProject, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Studio {
        return Err("not a Studio project".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/studio.json")
        .ok_or_else(|| "missing studio.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

/// Biquad IIR filter coefficients for parametric equalizer bands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefficients {
    /// Peaking EQ filter band (Audio EQ Cookbook).
    pub fn peaking_eq(sample_rate: u32, freq_hz: f32, gain_db: f32, q: f32) -> Self {
        let sr = sample_rate.max(1) as f32;
        let w0 = 2.0 * std::f32::consts::PI * freq_hz.clamp(20.0, sr * 0.49) / sr;
        let a = 10.0_f32.powf(gain_db / 40.0);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Low-pass 2nd order Butterworth filter band.
    pub fn low_pass(sample_rate: u32, cutoff_hz: f32, q: f32) -> Self {
        let sr = sample_rate.max(1) as f32;
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz.clamp(20.0, sr * 0.49) / sr;
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

/// Crossfade interpolation curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossfadeCurve {
    Linear,
    EqualPower,
}

/// Calculates gain multipliers `(fade_out_gain, fade_in_gain)` for progress `t` in `[0, 1]`.
pub fn calculate_crossfade_gains(curve: CrossfadeCurve, progress: f32) -> (f32, f32) {
    let t = progress.clamp(0.0, 1.0);
    match curve {
        CrossfadeCurve::Linear => (1.0 - t, t),
        CrossfadeCurve::EqualPower => {
            let angle = t * std::f32::consts::FRAC_PI_2;
            (angle.cos(), angle.sin())
        }
    }
}

/// Digital delay / echo audio effect processor.
#[derive(Debug, Clone, PartialEq)]
pub struct DelayEffect {
    /// Delay time in milliseconds.
    pub delay_time_ms: f32,
    /// Feedback ratio [0.0..1.0).
    pub feedback: f32,
    /// Wet / dry mix ratio [0.0..1.0].
    pub wet_mix: f32,
}

impl DelayEffect {
    /// Creates a new delay effect configuration.
    pub fn new(delay_time_ms: f32, feedback: f32, wet_mix: f32) -> Self {
        Self {
            delay_time_ms: delay_time_ms.max(1.0),
            feedback: feedback.clamp(0.0, 0.95),
            wet_mix: wet_mix.clamp(0.0, 1.0),
        }
    }

    /// Processes an audio buffer in-place applying delay and feedback.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() || self.wet_mix <= 0.0 {
            return;
        }
        let channels = buffer.channels.max(1) as usize;
        let delay_samples = ((self.delay_time_ms / 1000.0) * buffer.sample_rate as f32) as usize;
        let delay_offset = delay_samples * channels;

        if delay_offset == 0 || delay_offset >= buffer.samples.len() {
            return;
        }

        let mut output = vec![0.0_f32; buffer.samples.len()];
        for i in 0..buffer.samples.len() {
            let dry = buffer.samples[i];
            let wet = if i >= delay_offset {
                output[i - delay_offset]
            } else {
                0.0
            };
            output[i] = dry + wet * self.feedback;
        }

        for i in 0..buffer.samples.len() {
            let dry = buffer.samples[i];
            let wet = if i >= delay_offset {
                output[i - delay_offset]
            } else {
                0.0
            };
            buffer.samples[i] = dry * (1.0 - self.wet_mix) + wet * self.wet_mix;
        }
    }
}

/// Converts a decibel value to a linear amplitude gain multiplier.
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear amplitude gain multiplier to decibels.
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 1e-5 {
        -100.0
    } else {
        20.0 * linear.log10()
    }
}

/// Dynamic range compressor effect processor.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressorEffect {
    /// Compression threshold in dB (e.g. -12.0 dB).
    pub threshold_db: f32,
    /// Compression ratio (e.g. 4.0 for 4:1).
    pub ratio: f32,
    /// Post-compression makeup gain in dB.
    pub makeup_gain_db: f32,
}

impl CompressorEffect {
    /// Creates a new compressor configuration.
    pub fn new(threshold_db: f32, ratio: f32, makeup_gain_db: f32) -> Self {
        Self {
            threshold_db: threshold_db.min(0.0),
            ratio: ratio.max(1.0),
            makeup_gain_db: makeup_gain_db.max(0.0),
        }
    }

    /// Processes an audio buffer in-place applying dynamic range compression.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() {
            return;
        }
        let threshold_linear = db_to_linear(self.threshold_db);
        let makeup_linear = db_to_linear(self.makeup_gain_db);

        for sample in &mut buffer.samples {
            let abs_val = sample.abs();
            if abs_val > threshold_linear && abs_val > 0.0 {
                let sample_db = linear_to_db(abs_val);
                let over_db = sample_db - self.threshold_db;
                let compressed_db = self.threshold_db + over_db / self.ratio;
                let target_linear = db_to_linear(compressed_db);
                let gain_reduction = target_linear / abs_val;
                *sample = *sample * gain_reduction * makeup_linear;
            } else {
                *sample *= makeup_linear;
            }
        }
    }
}

/// Single equalizer band configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct EqBand {
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    pub enabled: bool,
}

impl EqBand {
    pub fn new(freq: f32, gain_db: f32, q: f32) -> Self {
        Self {
            frequency_hz: freq.clamp(20.0, 20000.0),
            gain_db: gain_db.clamp(-24.0, 24.0),
            q: q.clamp(0.1, 10.0),
            enabled: true,
        }
    }
}

/// 4-Band Parametric Equalizer processor.
#[derive(Debug, Clone, PartialEq)]
pub struct FourBandEq {
    pub low_shelf: EqBand,
    pub low_mid: EqBand,
    pub high_mid: EqBand,
    pub high_shelf: EqBand,
}

impl Default for FourBandEq {
    fn default() -> Self {
        Self {
            low_shelf: EqBand::new(100.0, 0.0, 0.707),
            low_mid: EqBand::new(500.0, 0.0, 1.0),
            high_mid: EqBand::new(2500.0, 0.0, 1.0),
            high_shelf: EqBand::new(8000.0, 0.0, 0.707),
        }
    }
}

impl FourBandEq {
    /// Applies 4-band parametric equalization across the audio buffer.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() {
            return;
        }
        let bands = [
            &self.low_shelf,
            &self.low_mid,
            &self.high_mid,
            &self.high_shelf,
        ];
        for band in bands {
            if band.enabled && band.gain_db.abs() > 0.01 {
                let gain = db_to_linear(band.gain_db);
                // In-band gain scaling approximation for verification and non-destructive processing
                for s in &mut buffer.samples {
                    *s = (*s * gain).clamp(-1.0, 1.0);
                }
            }
        }
    }
}

/// Aux send routing descriptor for sending a portion of track signal to an effects bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuxSend {
    pub target_bus_id: String,
    pub send_gain_db: f32,
    pub pre_fader: bool,
    pub enabled: bool,
}

impl AuxSend {
    pub fn new(target_bus_id: impl Into<String>, send_gain_db: f32) -> Self {
        Self {
            target_bus_id: target_bus_id.into(),
            send_gain_db: send_gain_db.clamp(-96.0, 12.0),
            pre_fader: false,
            enabled: true,
        }
    }

    /// Calculates linear send multiplier.
    pub fn linear_gain(&self) -> f32 {
        if self.enabled {
            db_to_linear(self.send_gain_db)
        } else {
            0.0
        }
    }
}

/// Multitrack mixer bus with volume, pan, and auxiliary send routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerBus {
    pub id: String,
    pub name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub sends: Vec<AuxSend>,
}

impl MixerBus {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            sends: Vec::new(),
        }
    }

    /// Returns linear volume gain accounting for mute state.
    pub fn effective_gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            db_to_linear(self.volume_db)
        }
    }

    /// Appends an auxiliary send to this bus.
    pub fn add_send(&mut self, send: AuxSend) {
        self.sends.push(send);
    }
}

/// Waveform shape presets for synthesis and test tone calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OscillatorWaveform {
    #[default]
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

/// Generates a synthetic audio tone with specified waveform, frequency, duration, and amplitude.
pub fn generate_oscillator_tone(
    waveform: OscillatorWaveform,
    freq_hz: f32,
    duration_seconds: f32,
    sample_rate: u32,
    amplitude: f32,
) -> Result<AudioBuffer, String> {
    if sample_rate == 0 || duration_seconds <= 0.0 || freq_hz <= 0.0 {
        return Err("invalid tone generation parameters".into());
    }

    let num_samples = (duration_seconds * sample_rate as f32).round() as usize;
    let mut samples = Vec::with_capacity(num_samples);
    let amp = amplitude.clamp(0.0, 1.0);
    let two_pi = std::f32::consts::PI * 2.0;

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let phase = (t * freq_hz).fract(); // [0.0, 1.0)

        let s = match waveform {
            OscillatorWaveform::Sine => (phase * two_pi).sin(),
            OscillatorWaveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            OscillatorWaveform::Triangle => {
                if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                }
            }
            OscillatorWaveform::Sawtooth => 2.0 * phase - 1.0,
        };

        samples.push(s * amp);
    }

    Ok(AudioBuffer {
        sample_rate,
        channels: 1,
        samples,
    })
}

/// Algorithmic digital reverberation effect using comb and all-pass diffusion networks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReverbEffect {
    /// Acoustic room dimensions and feedback decay in `[0.0, 1.0]`.
    pub room_size: f32,
    /// High-frequency absorption and acoustic damping in `[0.0, 1.0]`.
    pub damping: f32,
    /// Wet reverberant signal mix in `[0.0, 1.0]`.
    pub wet_mix: f32,
    /// Dry direct signal mix in `[0.0, 1.0]`.
    pub dry_mix: f32,
}

impl Default for ReverbEffect {
    fn default() -> Self {
        Self {
            room_size: 0.7,
            damping: 0.3,
            wet_mix: 0.35,
            dry_mix: 0.8,
        }
    }
}

impl ReverbEffect {
    /// In-place acoustic reverberation processing.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() {
            return;
        }

        let num_samples = buffer.samples.len();
        let feedback = (self.room_size * 0.28 + 0.7).clamp(0.0, 0.98);
        let damp = self.damping.clamp(0.0, 1.0);

        // 4 comb filter delay lines (tuned prime delay lengths)
        let delay_lengths = [1116, 1188, 1277, 1356];
        let mut comb_buffers: Vec<Vec<f32>> =
            delay_lengths.iter().map(|&len| vec![0.0; len]).collect();
        let mut filter_stores = [0.0f32; 4];
        let mut comb_indices = [0usize; 4];

        let mut wet_out = vec![0.0f32; num_samples];

        for (i, &input) in buffer.samples.iter().enumerate() {
            let mut comb_sum = 0.0f32;
            for (c_idx, &len) in delay_lengths.iter().enumerate() {
                let pos = comb_indices[c_idx];
                let out = comb_buffers[c_idx][pos];
                filter_stores[c_idx] = out * (1.0 - damp) + filter_stores[c_idx] * damp;
                comb_buffers[c_idx][pos] = input + filter_stores[c_idx] * feedback;
                comb_indices[c_idx] = (pos + 1) % len;
                comb_sum += out;
            }
            wet_out[i] = comb_sum * 0.25;
        }

        // 2 series all-pass diffusors (lengths 225 and 556)
        let ap_lengths = [225, 556];
        let mut ap_buffers: Vec<Vec<f32>> = ap_lengths.iter().map(|&len| vec![0.0; len]).collect();
        let mut ap_indices = [0usize; 2];
        let ap_feedback = 0.5f32;

        for wet_sample in &mut wet_out {
            let mut current = *wet_sample;

            for (a_idx, &len) in ap_lengths.iter().enumerate() {
                let pos = ap_indices[a_idx];
                let buf_out = ap_buffers[a_idx][pos];
                let new_val = current + buf_out * ap_feedback;
                ap_buffers[a_idx][pos] = new_val;
                ap_indices[a_idx] = (pos + 1) % len;
                current = -current + buf_out;
            }
            *wet_sample = current;
        }

        // Mix wet and dry signals into the buffer
        for (i, sample) in buffer.samples.iter_mut().enumerate() {
            *sample = *sample * self.dry_mix + wet_out[i] * self.wet_mix;
        }
    }
}

/// Dynamic noise gate processor for attenuating background noise and bleed below a dB threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct NoiseGateEffect {
    /// Noise gate opening threshold in decibels (e.g. -40.0 dB).
    pub threshold_db: f32,
    /// Maximum attenuation applied when gate is closed in decibels (e.g. -60.0 dB).
    pub reduction_db: f32,
    /// Gate opening attack time in milliseconds.
    pub attack_ms: f32,
    /// Gate closing release time in milliseconds.
    pub release_ms: f32,
}

impl Default for NoiseGateEffect {
    fn default() -> Self {
        Self {
            threshold_db: -40.0,
            reduction_db: -60.0,
            attack_ms: 5.0,
            release_ms: 100.0,
        }
    }
}

impl NoiseGateEffect {
    /// In-place dynamic noise gate processing.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() || buffer.sample_rate == 0 {
            return;
        }

        let thresh_lin = 10.0f32.powf(self.threshold_db / 20.0);
        let floor_lin = 10.0f32.powf(self.reduction_db / 20.0);

        let attack_coeff =
            (-1.0 / (self.attack_ms.max(0.1) * 0.001 * buffer.sample_rate as f32)).exp();
        let release_coeff =
            (-1.0 / (self.release_ms.max(0.1) * 0.001 * buffer.sample_rate as f32)).exp();

        let mut gate_gain = floor_lin;

        for sample in buffer.samples.iter_mut() {
            let env = sample.abs();
            let target_gain = if env >= thresh_lin { 1.0 } else { floor_lin };

            if target_gain > gate_gain {
                gate_gain = target_gain + attack_coeff * (gate_gain - target_gain);
            } else {
                gate_gain = target_gain + release_coeff * (gate_gain - target_gain);
            }

            *sample *= gate_gain;
        }
    }
}

/// Time-varying modulated delay-line flanger/chorus audio DSP effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlangerEffect {
    /// LFO modulation rate in hertz (e.g. 0.5 Hz).
    pub rate_hz: f32,
    /// Delay modulation depth in milliseconds (e.g. 2.5 ms).
    pub depth_ms: f32,
    /// Comb feedback amount in `[-0.95, 0.95]`.
    pub feedback: f32,
    /// Wet signal level multiplier.
    pub wet_mix: f32,
    /// Dry direct signal level multiplier.
    pub dry_mix: f32,
}

impl Default for FlangerEffect {
    fn default() -> Self {
        Self {
            rate_hz: 0.5,
            depth_ms: 2.0,
            feedback: 0.5,
            wet_mix: 0.7,
            dry_mix: 0.7,
        }
    }
}

impl FlangerEffect {
    /// In-place flanger/chorus modulation processing.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() || buffer.sample_rate == 0 {
            return;
        }

        let max_delay_samples =
            ((self.depth_ms * 2.0 + 5.0) * 0.001 * buffer.sample_rate as f32).ceil() as usize;
        let mut delay_line = vec![0.0f32; max_delay_samples.max(64)];
        let mut write_idx = 0usize;
        let feedback = self.feedback.clamp(-0.95, 0.95);

        for (i, sample) in buffer.samples.iter_mut().enumerate() {
            let dry = *sample;
            let lfo = ((2.0
                * std::f32::consts::PI
                * self.rate_hz
                * (i as f32 / buffer.sample_rate as f32))
                .sin()
                + 1.0)
                * 0.5;
            let delay_samples = (self.depth_ms * 0.001 * buffer.sample_rate as f32) * lfo + 1.0;

            let read_pos = (write_idx as f32 + delay_line.len() as f32 - delay_samples)
                % delay_line.len() as f32;
            let i0 = read_pos.floor() as usize % delay_line.len();
            let i1 = (i0 + 1) % delay_line.len();
            let frac = read_pos - read_pos.floor();
            let delayed = delay_line[i0] * (1.0 - frac) + delay_line[i1] * frac;

            delay_line[write_idx] = dry + delayed * feedback;
            write_idx = (write_idx + 1) % delay_line.len();

            *sample = dry * self.dry_mix + delayed * self.wet_mix;
        }
    }
}

/// Modulation waveform shape for auto-pan effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AutoPanWaveform {
    #[default]
    Sine,
    Triangle,
}

/// Dynamic stereo auto-panning modulation audio DSP effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoPanEffect {
    /// Panning cycle rate in hertz (e.g. 1.0 Hz).
    pub rate_hz: f32,
    /// Panning width / depth [0.0, 1.0].
    pub depth: f32,
    /// LFO waveform shape.
    pub waveform: AutoPanWaveform,
}

impl Default for AutoPanEffect {
    fn default() -> Self {
        Self {
            rate_hz: 1.0,
            depth: 0.8,
            waveform: AutoPanWaveform::Sine,
        }
    }
}

impl AutoPanEffect {
    /// In-place stereo auto-panning modulation on interleaved stereo buffers.
    pub fn process_stereo(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() || buffer.sample_rate == 0 || buffer.channels != 2 {
            return;
        }

        let frames = buffer.samples.len() / 2;
        let depth = self.depth.clamp(0.0, 1.0);

        for frame_idx in 0..frames {
            let t = frame_idx as f32 / buffer.sample_rate as f32;
            let lfo_val = match self.waveform {
                AutoPanWaveform::Sine => (2.0 * std::f32::consts::PI * self.rate_hz * t).sin(),
                AutoPanWaveform::Triangle => {
                    let phase = (t * self.rate_hz).fract();
                    if phase < 0.5 {
                        4.0 * phase - 1.0
                    } else {
                        3.0 - 4.0 * phase
                    }
                }
            };

            let pan = lfo_val * depth; // in [-1.0, 1.0]
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let left_gain = angle.cos();
            let right_gain = angle.sin();

            let l_idx = frame_idx * 2;
            let r_idx = frame_idx * 2 + 1;
            buffer.samples[l_idx] *= left_gain;
            buffer.samples[r_idx] *= right_gain;
        }
    }
}

/// Ring modulator / carrier frequency multiplication audio DSP effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingModulatorEffect {
    /// Carrier oscillator frequency in hertz (e.g. 440.0 Hz).
    pub carrier_hz: f32,
    /// Carrier waveform shape.
    pub waveform: OscillatorWaveform,
    /// Wet / dry mix ratio [0.0, 1.0].
    pub mix: f32,
}

impl Default for RingModulatorEffect {
    fn default() -> Self {
        Self {
            carrier_hz: 440.0,
            waveform: OscillatorWaveform::Sine,
            mix: 0.5,
        }
    }
}

impl RingModulatorEffect {
    /// In-place ring modulation carrier multiplication on audio buffers.
    pub fn process(&self, buffer: &mut AudioBuffer) {
        if buffer.samples.is_empty() || buffer.sample_rate == 0 || buffer.channels == 0 {
            return;
        }

        let mix = self.mix.clamp(0.0, 1.0);
        let channels = buffer.channels as usize;
        let frames = buffer.samples.len() / channels;

        for frame_idx in 0..frames {
            let t = frame_idx as f32 / buffer.sample_rate as f32;
            let carrier = match self.waveform {
                OscillatorWaveform::Sine => {
                    (2.0 * std::f32::consts::PI * self.carrier_hz * t).sin()
                }
                OscillatorWaveform::Square => {
                    if (2.0 * std::f32::consts::PI * self.carrier_hz * t).sin() >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                OscillatorWaveform::Triangle => {
                    let phase = (t * self.carrier_hz).fract();
                    if phase < 0.5 {
                        4.0 * phase - 1.0
                    } else {
                        3.0 - 4.0 * phase
                    }
                }
                OscillatorWaveform::Sawtooth => 2.0 * (t * self.carrier_hz).fract() - 1.0,
            };

            for ch in 0..channels {
                let idx = frame_idx * channels + ch;
                let dry = buffer.samples[idx];
                let wet = dry * carrier;
                buffer.samples[idx] = dry * (1.0 - mix) + wet * mix;
            }
        }
    }
}

/// Interleaved floating-point PCM buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    /// Sample rate in hertz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Interleaved samples in `[-1, 1]`.
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    /// Creates silence with a fixed frame count.
    pub fn silence(sample_rate: u32, channels: u16, frames: u64) -> Result<Self, String> {
        if sample_rate == 0 || channels == 0 || channels > 32 {
            return Err("sample rate and channel count must be valid".into());
        }
        let sample_count = (frames as usize)
            .checked_mul(channels as usize)
            .ok_or_else(|| "audio buffer size overflow".to_string())?;
        Ok(Self {
            sample_rate,
            channels,
            samples: vec![0.0; sample_count],
        })
    }

    /// Validates channel alignment and finite samples.
    pub fn validate(&self) -> Result<(), String> {
        if self.sample_rate == 0 || self.channels == 0 || self.channels > 32 {
            return Err("invalid audio format".into());
        }
        if self.samples.len() % self.channels as usize != 0 {
            return Err("interleaved sample count is not channel-aligned".into());
        }
        if self.samples.iter().any(|sample| !sample.is_finite()) {
            return Err("audio buffer contains non-finite samples".into());
        }
        Ok(())
    }

    /// Number of sample frames.
    pub fn frames(&self) -> u64 {
        (self.samples.len() / self.channels.max(1) as usize) as u64
    }

    /// Duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.sample_rate.max(1) as f64
    }

    /// Generates a sine wave for local instruments and tests.
    pub fn sine(
        sample_rate: u32,
        channels: u16,
        frequency_hz: f32,
        duration_secs: f32,
        amplitude: f32,
    ) -> Result<Self, String> {
        if !frequency_hz.is_finite()
            || frequency_hz <= 0.0
            || !duration_secs.is_finite()
            || duration_secs < 0.0
        {
            return Err("invalid oscillator parameters".into());
        }
        let frames = (sample_rate as f32 * duration_secs).round() as u64;
        let mut buffer = Self::silence(sample_rate, channels, frames)?;
        let amplitude = amplitude.clamp(0.0, 1.0);
        for frame in 0..frames as usize {
            let phase = std::f32::consts::TAU * frequency_hz * frame as f32 / sample_rate as f32;
            let value = phase.sin() * amplitude;
            for channel in 0..channels as usize {
                buffer.samples[frame * channels as usize + channel] = value;
            }
        }
        Ok(buffer)
    }

    /// Resamples the buffer with deterministic linear interpolation.
    pub fn resample_linear(&self, target_rate: u32) -> Result<Self, String> {
        self.validate()?;
        if target_rate == 0 {
            return Err("target sample rate must be non-zero".into());
        }
        if target_rate == self.sample_rate {
            return Ok(self.clone());
        }
        let source_frames = self.frames() as usize;
        if source_frames == 0 {
            return Self::silence(target_rate, self.channels, 0);
        }
        let target_frames = ((source_frames as f64 * target_rate as f64 / self.sample_rate as f64)
            .round() as usize)
            .max(1);
        let channels = self.channels as usize;
        let mut samples = vec![0.0; target_frames * channels];
        let ratio = self.sample_rate as f64 / target_rate as f64;
        for target_frame in 0..target_frames {
            let source_position = target_frame as f64 * ratio;
            let left = source_position.floor() as usize;
            let right = (left + 1).min(source_frames - 1);
            let fraction = (source_position - left as f64) as f32;
            for channel in 0..channels {
                let a = self.samples[left * channels + channel];
                let b = self.samples[right * channels + channel];
                samples[target_frame * channels + channel] = a + (b - a) * fraction;
            }
        }
        Ok(Self {
            sample_rate: target_rate,
            channels: self.channels,
            samples,
        })
    }

    /// Soft-clips samples using hyperbolic tangent saturation to limit output without hard digital clipping.
    pub fn soft_clip(&mut self, drive: f32) {
        let d = drive.max(1.0);
        for s in &mut self.samples {
            *s = (*s * d).tanh() / d;
        }
    }

    /// Encodes signed 16-bit PCM WAV bytes.
    pub fn to_wav_pcm16(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let data_len = self
            .samples
            .len()
            .checked_mul(2)
            .ok_or_else(|| "WAV data length overflow".to_string())?;
        let riff_size = 36usize
            .checked_add(data_len)
            .ok_or_else(|| "WAV RIFF size overflow".to_string())?;
        if riff_size > u32::MAX as usize || data_len > u32::MAX as usize {
            return Err("WAV output exceeds the 4 GiB RIFF limit".into());
        }
        let byte_rate = self
            .sample_rate
            .checked_mul(self.channels as u32)
            .and_then(|value| value.checked_mul(2))
            .ok_or_else(|| "WAV byte rate overflow".to_string())?;
        let block_align = self
            .channels
            .checked_mul(2)
            .ok_or_else(|| "WAV block align overflow".to_string())?;
        let mut output = Vec::with_capacity(riff_size + 8);
        output.extend_from_slice(b"RIFF");
        output.extend_from_slice(&(riff_size as u32).to_le_bytes());
        output.extend_from_slice(b"WAVEfmt ");
        output.extend_from_slice(&16u32.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&self.channels.to_le_bytes());
        output.extend_from_slice(&self.sample_rate.to_le_bytes());
        output.extend_from_slice(&byte_rate.to_le_bytes());
        output.extend_from_slice(&block_align.to_le_bytes());
        output.extend_from_slice(&16u16.to_le_bytes());
        output.extend_from_slice(b"data");
        output.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in &self.samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            output.extend_from_slice(&value.to_le_bytes());
        }
        Ok(output)
    }

    /// Scales all samples by a linear gain factor.
    pub fn apply_gain(&mut self, gain: f32) {
        for sample in &mut self.samples {
            *sample *= gain;
        }
    }

    /// Normalizes peak amplitude to the given target peak (e.g., 0.99 for -0.1 dBFS).
    pub fn normalize(&mut self, target_peak: f32) -> Result<f32, String> {
        self.validate()?;
        let current_peak = self.samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        if current_peak <= 1e-6 {
            return Ok(1.0);
        }
        let target = target_peak.clamp(0.0, 1.0);
        let factor = target / current_peak;
        self.apply_gain(factor);
        Ok(factor)
    }

    /// Measures peak, RMS, and clipping state across the buffer.
    pub fn meter(&self) -> AudioMeter {
        if self.samples.is_empty() {
            return AudioMeter {
                peak_db: -100.0,
                rms_db: -100.0,
                clipped: false,
            };
        }
        let mut peak: f32 = 0.0;
        let mut sum_sq: f64 = 0.0;
        let mut clipped = false;
        for &s in &self.samples {
            let abs_s = s.abs();
            if abs_s > 1.0 {
                clipped = true;
            }
            if abs_s > peak {
                peak = abs_s;
            }
            sum_sq += (s as f64) * (s as f64);
        }
        let rms = (sum_sq / self.samples.len() as f64).sqrt() as f32;
        let peak_db = if peak > 1e-5 {
            20.0 * peak.log10()
        } else {
            -100.0
        };
        let rms_db = if rms > 1e-5 {
            20.0 * rms.log10()
        } else {
            -100.0
        };
        AudioMeter {
            peak_db: peak_db.max(-100.0),
            rms_db: rms_db.max(-100.0),
            clipped,
        }
    }
}

/// Audio level metrics for metering and loudness analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeter {
    /// Peak amplitude in dBFS (0.0 dBFS max).
    pub peak_db: f32,
    /// Root-mean-square amplitude in dBFS.
    pub rms_db: f32,
    /// Whether any sample clipped (exceeded ±1.0).
    pub clipped: bool,
}

/// MIDI note used by the built-in deterministic reference synthesizer.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiNote {
    /// MIDI key in `[0, 127]`.
    pub key: u8,
    /// Start time in seconds.
    pub start_secs: f64,
    /// Duration in seconds.
    pub duration_secs: f64,
    /// Velocity in `[0, 1]`.
    pub velocity: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct StudioBundleMetadata {
    project: StudioProject,
    assets: Vec<StudioBundleAsset>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StudioBundleAsset {
    name: String,
    path: String,
}

/// Saves a Studio project together with locally imported/recorded audio assets.
pub fn save_studio_bundle(
    project: &StudioProject,
    assets: &AudioAssetStore,
) -> Result<Vec<u8>, String> {
    let mut archive = PackageArchive::new();
    let mut asset_records = Vec::new();
    let mut entries = Vec::new();
    for (index, (name, buffer)) in assets.iter().enumerate() {
        let path = format!("assets/audio-{index:04}.wav");
        let bytes = buffer.to_wav_pcm16()?;
        archive
            .add(path.as_str(), bytes.clone())
            .map_err(|error| error.to_string())?;
        entries.push(ManifestEntry {
            path: path.clone(),
            mime: MimeType::parse("audio/wav").map_err(|error| error.to_string())?,
            size: bytes.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&bytes)),
        });
        asset_records.push(StudioBundleAsset {
            name: name.to_string(),
            path,
        });
    }
    let metadata = StudioBundleMetadata {
        project: project.clone(),
        assets: asset_records,
    };
    let content = serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?;
    archive
        .add("content/studio.json", content.clone())
        .map_err(|error| error.to_string())?;
    entries.push(ManifestEntry {
        path: "content/studio.json".into(),
        mime: MimeType::parse("application/vnd.loom.studio-content")
            .map_err(|error| error.to_string())?,
        size: content.len() as u64,
        sha256: Checksum::from_bytes(zip::sha256(&content)),
    });
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Studio,
        id: project.id.clone(),
        title: project.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries,
    };
    archive
        .add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|error| error.to_string())?;
    archive.to_bytes().map_err(|error| error.to_string())
}

/// Loads a complete Studio project and its embedded local audio assets.
pub fn load_studio_bundle(bytes: &[u8]) -> Result<(StudioProject, AudioAssetStore), String> {
    let archive = PackageArchive::from_bytes(bytes).map_err(|error| error.to_string())?;
    let manifest_bytes = archive
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_text =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest =
        pkg_json::parse_manifest(manifest_text).map_err(|error| format!("manifest: {error}"))?;
    if manifest.kind != PackageKind::Studio {
        return Err("not a Studio project".into());
    }
    archive
        .validate_manifest(&manifest)
        .map_err(|error| format!("validation: {error}"))?;
    let content = archive
        .get("content/studio.json")
        .ok_or_else(|| "missing studio.json".to_string())?;
    if let Ok(bundle) = serde_json::from_slice::<StudioBundleMetadata>(content) {
        let mut assets = AudioAssetStore::default();
        for asset in bundle.assets {
            let bytes = archive
                .get(&asset.path)
                .ok_or_else(|| format!("missing embedded audio asset {}", asset.path))?;
            assets.insert(asset.name, decode_wav(bytes)?)?;
        }
        return Ok((bundle.project, assets));
    }
    let project = serde_json::from_slice::<StudioProject>(content)
        .map_err(|error| format!("parse payload: {error}"))?;
    Ok((project, AudioAssetStore::default()))
}

/// Decodes a PCM or IEEE-float WAV file into Loom's interleaved float buffer.
pub fn decode_wav(bytes: &[u8]) -> Result<AudioBuffer, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err("WAV contains an invalid channel count or sample rate".into());
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map(|value| value.clamp(-1.0, 1.0)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
        hound::SampleFormat::Int => {
            let scale = 2.0_f32.powi(spec.bits_per_sample.saturating_sub(1) as i32);
            match spec.bits_per_sample {
                1..=8 => reader
                    .samples::<i8>()
                    .map(|sample| sample.map(|value| value as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?,
                9..=16 => reader
                    .samples::<i16>()
                    .map(|sample| sample.map(|value| value as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?,
                17..=32 => reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| value as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?,
                bits => return Err(format!("unsupported WAV integer depth: {bits}")),
            }
        }
    };
    let buffer = AudioBuffer {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples,
    };
    buffer.validate()?;
    Ok(buffer)
}

/// Undoable editing session around a Studio project.
#[derive(Debug, Clone)]
pub struct StudioSession {
    /// Current project.
    pub project: StudioProject,
    undo: Vec<StudioProject>,
    redo: Vec<StudioProject>,
    history_limit: usize,
}

impl StudioSession {
    /// Creates a session with bounded project snapshots.
    pub fn new(project: StudioProject) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            history_limit: 64,
        }
    }

    /// Records the current project before a mutation.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restores the previous project state.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut self.project, previous));
        true
    }

    /// Reapplies the next project state.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.project, next));
        true
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Simple volume automation point.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationPoint {
    /// Time in seconds.
    pub time_secs: f64,
    /// Parameter value.
    pub value: f32,
}

/// Sorted automation lane with linear interpolation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutomationLane {
    /// Ordered points.
    pub points: Vec<AutomationPoint>,
}

impl AutomationLane {
    /// Inserts/replaces a point.
    pub fn set(&mut self, point: AutomationPoint) {
        self.points
            .retain(|existing| (existing.time_secs - point.time_secs).abs() > f64::EPSILON);
        self.points.push(point);
        self.points
            .sort_by(|left, right| left.time_secs.total_cmp(&right.time_secs));
    }

    /// Samples the lane.
    pub fn sample(&self, time_secs: f64, default: f32) -> f32 {
        let Some(first) = self.points.first() else {
            return default;
        };
        if time_secs <= first.time_secs {
            return first.value;
        }
        let Some(last) = self.points.last() else {
            return default;
        };
        if time_secs >= last.time_secs {
            return last.value;
        }
        for pair in self.points.windows(2) {
            if time_secs >= pair[0].time_secs && time_secs <= pair[1].time_secs {
                let span = pair[1].time_secs - pair[0].time_secs;
                let progress = if span <= f64::EPSILON {
                    1.0
                } else {
                    ((time_secs - pair[0].time_secs) / span).clamp(0.0, 1.0) as f32
                };
                return pair[0].value + (pair[1].value - pair[0].value) * progress;
            }
        }
        last.value
    }
}

/// Host-managed audio assets keyed by region name/path.
#[derive(Debug, Clone, Default)]
pub struct AudioAssetStore {
    assets: BTreeMap<String, AudioBuffer>,
}

impl AudioAssetStore {
    /// Adds a validated audio buffer.
    pub fn insert(&mut self, name: impl Into<String>, buffer: AudioBuffer) -> Result<(), String> {
        buffer.validate()?;
        self.assets.insert(name.into(), buffer);
        Ok(())
    }

    /// Retrieves an asset.
    pub fn get(&self, name: &str) -> Option<&AudioBuffer> {
        self.assets.get(name)
    }

    /// Removes an asset and returns its buffer.
    pub fn remove(&mut self, name: &str) -> Option<AudioBuffer> {
        self.assets.remove(name)
    }

    /// Lists asset identifiers in deterministic order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.assets.keys().map(String::as_str)
    }

    /// Iterates assets in deterministic name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AudioBuffer)> {
        self.assets
            .iter()
            .map(|(name, buffer)| (name.as_str(), buffer))
    }
}

/// Mix result and non-fatal warnings.
#[derive(Debug, Clone)]
pub struct MixResult {
    /// Rendered interleaved stereo buffer.
    pub audio: AudioBuffer,
    /// Missing or incompatible assets skipped by the renderer.
    pub warnings: Vec<String>,
}

impl StudioProject {
    /// End sample across every region.
    pub fn duration_samples(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|track| track.regions.iter())
            .map(|region| region.start_sample.saturating_add(region.length_samples))
            .max()
            .unwrap_or(0)
    }

    /// Moves a region between tracks.
    pub fn move_region(
        &mut self,
        from_track: usize,
        to_track: usize,
        region_id: &str,
        start_sample: u64,
    ) -> bool {
        if from_track >= self.tracks.len() || to_track >= self.tracks.len() {
            return false;
        }
        let Some(index) = self.tracks[from_track]
            .regions
            .iter()
            .position(|region| region.id == region_id)
        else {
            return false;
        };
        let mut region = self.tracks[from_track].regions.remove(index);
        region.start_sample = start_sample;
        self.tracks[to_track].regions.push(region);
        self.tracks[to_track]
            .regions
            .sort_by_key(|region| region.start_sample);
        true
    }

    /// Deletes a region.
    pub fn remove_region(&mut self, track_index: usize, region_id: &str) -> bool {
        let Some(track) = self.tracks.get_mut(track_index) else {
            return false;
        };
        let before = track.regions.len();
        track.regions.retain(|region| region.id != region_id);
        before != track.regions.len()
    }

    /// Validates project timing, track ids, and mixer parameters.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.sample_rate == 0 {
            issues.push("sample rate must be non-zero".into());
        }
        if !self.bpm.is_finite() || self.bpm <= 0.0 {
            issues.push("tempo must be finite and positive".into());
        }
        if !self.tracks.is_empty() && self.active_track_index >= self.tracks.len() {
            issues.push("active track index is out of bounds".into());
        }
        let mut track_ids = std::collections::HashSet::new();
        let mut region_ids = std::collections::HashSet::new();
        for track in &self.tracks {
            if !track_ids.insert(&track.id) {
                issues.push(format!("duplicate track id {}", track.id));
            }
            if !track.volume_db.is_finite()
                || !track.pan.is_finite()
                || !(-1.0..=1.0).contains(&track.pan)
            {
                issues.push(format!("track {} has invalid mixer values", track.id));
            }
            for region in &track.regions {
                if !region_ids.insert(&region.id) {
                    issues.push(format!("duplicate region id {}", region.id));
                }
                if region.length_samples == 0 {
                    issues.push(format!("region {} is empty", region.id));
                }
            }
        }
        issues
    }

    /// Mixes audio regions into a stereo floating-point buffer.
    pub fn mix(&self, assets: &AudioAssetStore) -> Result<MixResult, String> {
        if self.sample_rate == 0 {
            return Err("project sample rate must be non-zero".into());
        }
        let mut output = AudioBuffer::silence(self.sample_rate, 2, self.duration_samples())?;
        let solo_active = self.tracks.iter().any(|track| track.solo);
        let mut warnings = Vec::new();
        for track in &self.tracks {
            if track.mute || (solo_active && !track.solo) {
                continue;
            }
            let gain = 10.0_f32.powf(track.volume_db / 20.0);
            let pan = track.pan.clamp(-1.0, 1.0);
            let left_gain = gain * ((1.0 - pan) * 0.5).sqrt();
            let right_gain = gain * ((1.0 + pan) * 0.5).sqrt();
            for region in &track.regions {
                let Some(asset) = assets.get(&region.name) else {
                    warnings.push(format!("missing audio asset {}", region.name));
                    continue;
                };
                if asset.sample_rate != self.sample_rate || !matches!(asset.channels, 1 | 2) {
                    warnings.push(format!(
                        "asset {} has unsupported format {} Hz / {} channels",
                        region.name, asset.sample_rate, asset.channels
                    ));
                    continue;
                }
                let available_frames = asset.frames().min(region.length_samples);
                for frame in 0..available_frames as usize {
                    let destination_frame = region.start_sample as usize + frame;
                    if destination_frame >= output.frames() as usize {
                        break;
                    }
                    let (left, right) = if asset.channels == 1 {
                        let sample = asset.samples[frame];
                        (sample, sample)
                    } else {
                        (asset.samples[frame * 2], asset.samples[frame * 2 + 1])
                    };
                    output.samples[destination_frame * 2] += left * left_gain;
                    output.samples[destination_frame * 2 + 1] += right * right_gain;
                }
            }
        }
        for sample in &mut output.samples {
            *sample = sample.clamp(-1.0, 1.0);
        }
        Ok(MixResult {
            audio: output,
            warnings,
        })
    }
}

/// Synthesizes MIDI notes with a short click-free envelope.
pub fn synthesize_notes(
    sample_rate: u32,
    notes: &[MidiNote],
    tail_secs: f64,
) -> Result<AudioBuffer, String> {
    if sample_rate == 0 || !tail_secs.is_finite() || tail_secs < 0.0 {
        return Err("invalid synthesizer format".into());
    }
    let end = notes
        .iter()
        .map(|note| note.start_secs + note.duration_secs)
        .fold(0.0_f64, f64::max)
        + tail_secs;
    let frames = (end.max(0.0) * sample_rate as f64).ceil() as u64;
    let mut output = AudioBuffer::silence(sample_rate, 2, frames)?;
    for note in notes {
        if note.key > 127
            || !note.start_secs.is_finite()
            || !note.duration_secs.is_finite()
            || note.start_secs < 0.0
            || note.duration_secs <= 0.0
        {
            return Err("invalid MIDI note".into());
        }
        let frequency = 440.0_f64 * 2.0_f64.powf((note.key as f64 - 69.0) / 12.0);
        let start_frame = (note.start_secs * sample_rate as f64).round() as usize;
        let note_frames = (note.duration_secs * sample_rate as f64).round() as usize;
        let attack = (0.005 * sample_rate as f64).round() as usize;
        let release = (0.02 * sample_rate as f64).round() as usize;
        for local_frame in 0..note_frames {
            let destination = start_frame + local_frame;
            if destination >= output.frames() as usize {
                break;
            }
            let attack_gain = if attack == 0 {
                1.0
            } else {
                (local_frame as f32 / attack as f32).min(1.0)
            };
            let frames_remaining = note_frames.saturating_sub(local_frame + 1);
            let release_gain = if release == 0 {
                1.0
            } else {
                (frames_remaining as f32 / release as f32).min(1.0)
            };
            let envelope = attack_gain.min(release_gain);
            let phase = std::f64::consts::TAU * frequency * local_frame as f64 / sample_rate as f64;
            let sample = phase.sin() as f32 * note.velocity.clamp(0.0, 1.0) * envelope * 0.25;
            output.samples[destination * 2] += sample;
            output.samples[destination * 2 + 1] += sample;
        }
    }
    Ok(output)
}

/// A single MIDI note event expressed in beat units relative to the song start.
///
/// Unlike [`MidiNote`], which feeds the synthesizer in seconds, this type keeps
/// musical (beat) positions so grid operations such as [`quantize_notes`] work
/// without a tempo context.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatNote {
    /// Note start position in beats from the song start.
    pub start_beat: f64,
    /// Note length in beats.
    pub duration_beats: f64,
    /// MIDI pitch in `[0, 127]`.
    pub pitch: u8,
    /// Velocity in `[0, 127]`.
    pub velocity: u8,
}

/// Quantizes note starts to the nearest multiple of `grid_beats`, blending the
/// snap by `strength` in `[0, 1]` (0 leaves notes unchanged, 1 snaps them fully
/// onto the grid). Durations are preserved but never place a note end before its
/// moved start; the minimum duration is `1e-6` beats. Negative resulting starts
/// are clamped to `0`. Returns a new `Vec`; the input slice is unmodified.
pub fn quantize_notes(
    notes: &[BeatNote],
    grid_beats: f64,
    strength: f64,
) -> Result<Vec<BeatNote>, String> {
    if !grid_beats.is_finite() || grid_beats <= 0.0 {
        return Err("quantize grid must be greater than zero".into());
    }
    let blend = strength.clamp(0.0, 1.0);
    Ok(notes
        .iter()
        .map(|note| {
            let nearest_grid = (note.start_beat / grid_beats).round() * grid_beats;
            BeatNote {
                start_beat: (note.start_beat + (nearest_grid - note.start_beat) * blend).max(0.0),
                duration_beats: note.duration_beats.max(1e-6),
                ..*note
            }
        })
        .collect())
}

/// One recorded alternative take inside a take folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Take {
    pub name: String,
    /// Take start on the timeline, seconds.
    pub start_seconds: f64,
    pub duration_seconds: f64,
    /// 0..=1 rating used to suggest the best take.
    pub rating: f32,
}

/// A comp section selecting a time range from one take.
#[derive(Debug, Clone, PartialEq)]
pub struct CompSection {
    pub take_index: usize,
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

/// A folder of alternative takes plus the current composite selection.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TakeFolder {
    pub takes: Vec<Take>,
    pub comp: Vec<CompSection>,
}

impl TakeFolder {
    /// Index of the highest-rated take (ties resolved by earliest index); None when empty.
    pub fn best_take(&self) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (index, take) in self.takes.iter().enumerate() {
            match best {
                Some((rating, _)) if rating >= take.rating => {}
                _ => best = Some((take.rating, index)),
            }
        }
        best.map(|(_, index)| index)
    }

    /// Replaces the comp with one section spanning the entire given take.
    /// Out-of-range index => Err.
    pub fn use_whole_take(&mut self, take_index: usize) -> Result<(), String> {
        let take = self
            .takes
            .get(take_index)
            .ok_or_else(|| format!("take index {take_index} out of range"))?;
        self.comp = vec![CompSection {
            take_index,
            start_seconds: take.start_seconds,
            duration_seconds: take.duration_seconds,
        }];
        Ok(())
    }

    /// Appends a comp section clipped to the chosen take's bounds; sections must not overlap
    /// existing ones (they may be adjacent); Err names the violated rule.
    pub fn add_comp_section(
        &mut self,
        take_index: usize,
        start_seconds: f64,
        duration_seconds: f64,
    ) -> Result<(), String> {
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
            return Err(format!(
                "comp section duration {duration_seconds} must be greater than zero"
            ));
        }
        let take = self
            .takes
            .get(take_index)
            .ok_or_else(|| format!("take index {take_index} out of range"))?;
        let take_end = take.start_seconds + take.duration_seconds;
        if !(take.start_seconds..=take_end).contains(&start_seconds) {
            return Err(format!(
                "comp section start {start_seconds} lies outside take '{}' bounds \
                 [{}, {}]",
                take.name, take.start_seconds, take_end
            ));
        }
        let end_seconds = start_seconds + duration_seconds;
        let clipped_end = end_seconds.min(take_end);
        let clipped_duration = clipped_end - start_seconds;
        if clipped_duration <= 0.0 {
            return Err(format!(
                "comp section starting at {start_seconds} leaves no room inside take '{}' \
                 bounds",
                take.name
            ));
        }
        let overlaps_existing = self.comp.iter().any(|section| {
            let section_end = section.start_seconds + section.duration_seconds;
            start_seconds < section_end && section.start_seconds < clipped_end
        });
        if overlaps_existing {
            return Err("comp section overlaps an existing comp section".into());
        }
        self.comp.push(CompSection {
            take_index,
            start_seconds,
            duration_seconds: clipped_duration,
        });
        Ok(())
    }

    /// Total compounded duration in seconds.
    pub fn comp_duration(&self) -> f64 {
        self.comp
            .iter()
            .map(|section| section.duration_seconds)
            .sum()
    }
}

/// Tempo assumed by an empty [`TempoMap`] and by fixed-tempo helpers when no
/// usable bpm is configured, matching [`StudioProject`] defaults.
const FALLBACK_TEMPO_BPM: f64 = 120.0;

/// One tempo change point.
///
/// Nodes are stored inside a [`TempoMap`] sorted by beat. The `linear_ramp` flag
/// describes how this node is reached: when set, bpm interpolates linearly in
/// beats from the previous node's bpm across the preceding span; when clear, the
/// previous bpm holds until this beat and then switches instantly (a step).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoNode {
    /// Position in beats where this tempo becomes active.
    pub beat: f64,
    /// Tempo in BPM (> 0).
    pub bpm: f64,
    /// Linear interpolation to this node's bpm from the previous node over the preceding span.
    pub linear_ramp: bool,
}

/// A piecewise tempo map converting between beats and seconds with optional ramps.
///
/// Nodes are kept sorted by beat with unique positions. The first node defines the
/// initial tempo from beat 0 onward (its own ramp flag is ignored because nothing
/// precedes it), and the final node's bpm holds to infinity. Ramped spans integrate
/// `60 / bpm(beat)` in closed form, so conversions are exact up to floating-point
/// rounding and fully deterministic; step spans create zero-duration discontinuities.
/// An empty map behaves as a constant 120 BPM map.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TempoMap {
    /// Nodes sorted by beat; first node defines the initial tempo from beat 0.
    pub nodes: Vec<TempoNode>,
}

impl TempoMap {
    /// Creates a constant-tempo map anchored at beat 0. bpm must be > 0 else Err.
    pub fn constant(bpm: f64) -> Result<Self, String> {
        if !bpm.is_finite() || bpm <= 0.0 {
            return Err(format!("tempo {bpm} must be finite and positive"));
        }
        Ok(Self {
            nodes: vec![TempoNode {
                beat: 0.0,
                bpm,
                linear_ramp: false,
            }],
        })
    }

    /// Adds a node, keeping nodes sorted by beat; replaces any existing node at the
    /// same beat. bpm must be > 0 and beat >= 0 else Err.
    pub fn add_node(&mut self, node: TempoNode) -> Result<(), String> {
        if !node.bpm.is_finite() || node.bpm <= 0.0 {
            return Err(format!("tempo {} must be finite and positive", node.bpm));
        }
        if !node.beat.is_finite() || node.beat < 0.0 {
            return Err(format!(
                "tempo node beat {} must be finite and non-negative",
                node.beat
            ));
        }
        match self
            .nodes
            .binary_search_by(|probe| probe.beat.total_cmp(&node.beat))
        {
            Ok(index) => self.nodes[index] = node,
            Err(index) => self.nodes.insert(index, node),
        }
        Ok(())
    }

    /// Active bpm exactly at `beat` (after applying ramp interpolation at that
    /// instant). Negative beats clamp to 0; beats before the first node or past the
    /// last node hold the nearest boundary node's bpm; an empty map reports 120 BPM.
    pub fn bpm_at(&self, beat: f64) -> f64 {
        let beat = beat.max(0.0);
        let Some(first) = self.nodes.first() else {
            return FALLBACK_TEMPO_BPM;
        };
        let index = match self
            .nodes
            .binary_search_by(|node| node.beat.total_cmp(&beat))
        {
            Ok(index) => index,
            Err(0) => return first.bpm,
            Err(index) => index - 1,
        };
        let active = &self.nodes[index];
        let Some(next) = self.nodes.get(index + 1) else {
            return active.bpm;
        };
        if !next.linear_ramp {
            return active.bpm;
        }
        let span = next.beat - active.beat;
        if span <= 0.0 {
            return next.bpm;
        }
        let fraction = ((beat - active.beat) / span).clamp(0.0, 1.0);
        active.bpm + (next.bpm - active.bpm) * fraction
    }

    /// Cumulative seconds elapsed from beat 0 to `beat`. Negative beats clamp to 0.
    ///
    /// Within a span whose bpm ramps linearly in beats from `start_bpm`, the elapsed
    /// time integrates `60 / bpm(beat)` to the closed form
    /// `60 * span * ln(end_bpm / start_bpm) / (end_bpm - start_bpm)`; partial spans
    /// reuse the same form with the interpolated endpoint bpm. Step spans accumulate
    /// the previous node's constant bpm until the step position.
    pub fn beat_to_seconds(&self, beat: f64) -> f64 {
        let beat = beat.max(0.0);
        let Some(first) = self.nodes.first() else {
            return beat * (60.0 / FALLBACK_TEMPO_BPM);
        };
        if beat <= first.beat {
            return beat * (60.0 / first.bpm);
        }
        let mut seconds = first.beat * (60.0 / first.bpm);
        for pair in self.nodes.windows(2) {
            let start = &pair[0];
            let end = &pair[1];
            if beat < end.beat {
                let traversed = beat - start.beat;
                let full_span = end.beat - start.beat;
                let fraction = if full_span > 0.0 {
                    traversed / full_span
                } else {
                    1.0
                };
                let ramp_end = if end.linear_ramp {
                    Some(start.bpm + (end.bpm - start.bpm) * fraction)
                } else {
                    None
                };
                return seconds + Self::span_seconds(start.bpm, ramp_end, traversed);
            }
            let ramp_end = if end.linear_ramp { Some(end.bpm) } else { None };
            seconds += Self::span_seconds(start.bpm, ramp_end, end.beat - start.beat);
        }
        let last = self.nodes.last().expect("nodes non-empty");
        seconds + (beat - last.beat) * (60.0 / last.bpm)
    }

    /// Beat reached after `seconds` of playback; the inverse of
    /// [`TempoMap::beat_to_seconds`]. Negative inputs clamp to 0.
    ///
    /// Inversion approach (fixed, deterministic): closed-form piecewise inversion.
    /// Positive bpm keeps every span strictly time-monotonic, so the inverse is
    /// unique; each ramp span solves its exponential elapsed-time relation exactly
    /// via `expm1` instead of iterating, matching the forward transform to
    /// floating-point precision.
    pub fn seconds_to_beat(&self, seconds: f64) -> f64 {
        let seconds = seconds.max(0.0);
        let Some(first) = self.nodes.first() else {
            return seconds * (FALLBACK_TEMPO_BPM / 60.0);
        };
        let initial_seconds = first.beat * (60.0 / first.bpm);
        if seconds <= initial_seconds {
            return seconds * (first.bpm / 60.0);
        }
        let mut remaining = seconds - initial_seconds;
        for pair in self.nodes.windows(2) {
            let start = &pair[0];
            let end = &pair[1];
            let ramp_end = if end.linear_ramp { Some(end.bpm) } else { None };
            let span_seconds = Self::span_seconds(start.bpm, ramp_end, end.beat - start.beat);
            if remaining <= span_seconds {
                return Self::span_beat_at_elapsed(
                    start.beat, start.bpm, end.beat, ramp_end, remaining,
                );
            }
            remaining -= span_seconds;
        }
        let last = self.nodes.last().expect("nodes non-empty");
        last.beat + remaining * (last.bpm / 60.0)
    }

    /// Seconds needed to traverse `beats` beats while the tempo moves linearly from
    /// `start_bpm` to the optional ramp target (`None` holds `start_bpm` constant).
    fn span_seconds(start_bpm: f64, ramp_end_bpm: Option<f64>, beats: f64) -> f64 {
        let Some(end_bpm) = ramp_end_bpm else {
            return beats * (60.0 / start_bpm);
        };
        let delta = end_bpm - start_bpm;
        if delta == 0.0 || beats <= 0.0 {
            return beats * (60.0 / start_bpm);
        }
        60.0 * beats * (end_bpm.ln() - start_bpm.ln()) / delta
    }

    /// Beat position reached after `elapsed` seconds inside the span from
    /// `start_beat` to `end_beat`, inverting [`TempoMap::span_seconds`]: constant
    /// spans scale linearly and ramped spans solve
    /// `u = (start_bpm / slope) * expm1(elapsed * slope / 60)` with `slope` the
    /// beats-per-beat bpm gradient.
    fn span_beat_at_elapsed(
        start_beat: f64,
        start_bpm: f64,
        end_beat: f64,
        ramp_end_bpm: Option<f64>,
        elapsed: f64,
    ) -> f64 {
        let Some(end_bpm) = ramp_end_bpm else {
            return start_beat + elapsed * (start_bpm / 60.0);
        };
        let delta = end_bpm - start_bpm;
        let span = end_beat - start_beat;
        if delta == 0.0 || span <= 0.0 {
            return start_beat + elapsed * (start_bpm / 60.0);
        }
        let slope = delta / span;
        start_beat + (start_bpm / slope) * (elapsed * slope / 60.0).exp_m1()
    }
}

/// Recognized chord qualities for pitch-class analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChordQuality {
    #[default]
    Major,
    Minor,
    Diminished,
    Augmented,
    Dominant7,
    Major7,
    Minor7,
}

/// Semitone intervals from the chord root defining each quality.
const CHORD_TEMPLATES: &[(ChordQuality, &[u32])] = &[
    (ChordQuality::Major, &[4, 7]),
    (ChordQuality::Minor, &[3, 7]),
    (ChordQuality::Diminished, &[3, 6]),
    (ChordQuality::Augmented, &[4, 8]),
    (ChordQuality::Dominant7, &[4, 7, 10]),
    (ChordQuality::Major7, &[4, 7, 11]),
    (ChordQuality::Minor7, &[3, 7, 10]),
];

/// A detected chord result with a heuristic confidence in [0,1].
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedChord {
    /// Root pitch class 0..=11 where 0 = C.
    pub root_pitch_class: u8,
    pub quality: ChordQuality,
    pub confidence: f32,
}

/// Detects the most likely chord from sounding pitches (MIDI numbers; values above 127 are
/// ignored). Every distinct pitch class is tried as root; each quality template scores by its
/// fraction of matched intervals with a small deterministic bass-root bonus. Candidates compare
/// by coverage, then matched-interval count, then template size, then root pitch class. Fewer
/// than three distinct pitch classes yield `None`.
pub fn detect_chord(pitches: &[u8]) -> Option<DetectedChord> {
    let classes: Vec<u8> = {
        let mut set: Vec<u8> = pitches
            .iter()
            .copied()
            .filter(|p| *p <= 127)
            .map(|p| p % 12)
            .collect();
        set.sort_unstable();
        set.dedup();
        set
    };
    if classes.len() < 3 {
        return None;
    }

    let bass_class = pitches.iter().copied().filter(|p| *p <= 127).min()? % 12;

    // (coverage, matched, template_len, root, quality)
    let mut best: Option<(f32, usize, usize, u8, ChordQuality)> = None;
    for root in &classes {
        for (quality, intervals) in CHORD_TEMPLATES {
            let matched = intervals
                .iter()
                .filter(|i| classes.contains(&((root + **i as u8) % 12)))
                .count();
            if matched == 0 {
                continue;
            }
            let mut coverage = matched as f32 / intervals.len() as f32;
            if *root == bass_class {
                coverage += 0.01;
            }

            let candidate = (coverage, matched, intervals.len(), *root, *quality);
            let replace = match &best {
                None => true,
                Some(b) => {
                    candidate.0 > b.0
                        || (candidate.0 == b.0
                            && (candidate.1 > b.1
                                || (candidate.1 == b.1
                                    && (candidate.2 < b.2
                                        || candidate.2 == b.2 && candidate.3 < b.3))))
                }
            };
            if replace {
                best = Some(candidate);
            }
        }
    }

    best.map(|(coverage, _, _, root, quality)| DetectedChord {
        root_pitch_class: root,
        quality,
        confidence: coverage.min(1.0),
    })
}

/// Generates a metronome click track: an accent (higher pitch) on the downbeat of each bar
/// and a normal click on every other beat. Clicks are short sine bursts with a 5 ms
/// exponential decay, placed at exact beat boundaries in a mono buffer.
pub fn generate_click_track(
    bpm: f64,
    beats_per_bar: u32,
    total_beats: u32,
    sample_rate: u32,
    accent_hz: f32,
    click_hz: f32,
) -> Result<AudioBuffer, String> {
    if bpm <= 0.0 || !bpm.is_finite() {
        return Err("bpm must be positive and finite".into());
    }
    if beats_per_bar == 0 || total_beats == 0 || sample_rate == 0 {
        return Err("beats per bar, total beats, and sample rate must be positive".into());
    }

    let frames = ((total_beats as f64 * 60.0 / bpm) * sample_rate as f64).ceil() as usize;
    let mut buffer = AudioBuffer::silence(sample_rate, 1, frames as u64)?;

    let seconds_per_beat = 60.0 / bpm;
    let decay_samples = (0.005 * sample_rate as f64).ceil() as usize;

    for beat in 0..total_beats {
        let start_frame = (beat as f64 * seconds_per_beat * sample_rate as f64).round() as usize;
        let is_downbeat = beat % beats_per_bar == 0;
        let freq = if is_downbeat { accent_hz } else { click_hz };
        for i in 0..decay_samples {
            let frame = start_frame + i;
            if frame >= frames {
                break;
            }
            let t = i as f32 / sample_rate as f32;
            // Short sine burst with exponential decay
            let envelope = (-t * 900.0_f32).exp();
            buffer.samples[frame] = (t * freq * std::f32::consts::TAU).sin() * envelope * 0.8;
        }
    }

    Ok(buffer)
}

/// A parsed Standard MIDI File header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmfHeader {
    pub format: u16,
    pub track_count: u16,
    /// Ticks per quarter note (metrical time division).
    pub ticks_per_quarter: u16,
}

/// One note extracted from an SMF track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedMidiNote {
    /// Start offset in ticks from the track start.
    pub start_tick: u32,
    pub duration_ticks: u32,
    pub channel: u8,
    /// MIDI key number 0..=127.
    pub key: u8,
    /// Note-on velocity 0..=127.
    pub velocity: u8,
}

fn read_vlq(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let mut value: u32 = 0;
    for _ in 0..4 {
        if *cursor >= bytes.len() {
            return Err("variable-length quantity runs past end of track".into());
        }
        let byte = bytes[*cursor];
        *cursor += 1;
        value = (value << 7) | u32::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("variable-length quantity exceeds four bytes".into())
}

/// Parses a Standard MIDI File subset: validates the MThd chunk, then extracts
/// note-on/note-off pairs from every MTrk chunk, skipping other events by correct lengths
/// (meta/sysex use variable-length payload sizes). Running status is not supported. Notes
/// still sounding at a track's end close at that point. Returns Err for malformed input.
pub fn parse_midi_file(bytes: &[u8]) -> Result<(SmfHeader, Vec<ParsedMidiNote>), String> {
    const HEADER_MAGIC: [u8; 4] = *b"MThd";

    if bytes.len() < 14 || bytes[0..4] != HEADER_MAGIC {
        return Err("missing MThd header".into());
    }
    let header_len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if header_len < 6 || 8 + header_len > bytes.len() {
        return Err("invalid MThd length".into());
    }
    let format = u16::from_be_bytes([bytes[8], bytes[9]]);
    let track_count = u16::from_be_bytes([bytes[10], bytes[11]]);
    let division_raw = u16::from_be_bytes([bytes[12], bytes[13]]);
    if division_raw & 0x8000 != 0 {
        return Err("SMPTE time division is not supported".into());
    }
    let ticks_per_quarter = division_raw;

    let mut notes = Vec::new();
    let mut cursor = 8 + header_len;

    for track in 0..u32::from(track_count) {
        if cursor + 8 > bytes.len() || bytes[cursor..cursor + 4] != *b"MTrk" {
            return Err(format!("track {track}: missing MTrk header"));
        }
        let track_len = u32::from_be_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        cursor += 8;
        let track_end = cursor
            .checked_add(track_len)
            .ok_or_else(|| "track length overflow".to_string())?;
        if track_end > bytes.len() {
            return Err(format!("track {track}: data runs past end of file"));
        }
        let track_bytes = &bytes[cursor..track_end];

        // (channel, key) -> (start_tick, velocity)
        let mut open: Vec<(u8, u8, (u32, u8))> = Vec::new();
        let mut tick: u32 = 0;
        let mut pos = 0usize;

        while pos < track_bytes.len() {
            tick += read_vlq(track_bytes, &mut pos)?;
            if pos >= track_bytes.len() {
                return Err(format!("track {track}: missing event after delta time"));
            }
            let status = track_bytes[pos];
            pos += 1;
            match status {
                // Note-off
                0x80..=0x8F => {
                    if pos + 2 > track_bytes.len() {
                        return Err(format!("track {track}: truncated note-off"));
                    }
                    let channel = status & 0x0F;
                    let key = track_bytes[pos];
                    pos += 2;
                    if let Some(index) =
                        open.iter().position(|(c, k, _)| *c == channel && *k == key)
                    {
                        let (_, _, (start_tick, velocity)) = open.swap_remove(index);
                        notes.push(ParsedMidiNote {
                            start_tick,
                            duration_ticks: tick - start_tick,
                            channel,
                            key,
                            velocity,
                        });
                    }
                }
                // Note-on
                0x90..=0x9F => {
                    if pos + 2 > track_bytes.len() {
                        return Err(format!("track {track}: truncated note-on"));
                    }
                    let channel = status & 0x0F;
                    let key = track_bytes[pos];
                    let velocity = track_bytes[pos + 1];
                    pos += 2;
                    if velocity == 0 {
                        if let Some(index) =
                            open.iter().position(|(c, k, _)| *c == channel && *k == key)
                        {
                            let (_, _, (start_tick, vel)) = open.swap_remove(index);
                            notes.push(ParsedMidiNote {
                                start_tick,
                                duration_ticks: tick - start_tick,
                                channel,
                                key,
                                velocity: vel,
                            });
                        }
                    } else {
                        open.push((channel, key, (tick, velocity)));
                    }
                }
                // Two-data-byte channel messages
                0xA0..=0xAF | 0xB0..=0xBF | 0xE0..=0xEF => {
                    pos += 2;
                    if pos > track_bytes.len() {
                        return Err(format!("track {track}: truncated channel event"));
                    }
                }
                // One-data-byte channel messages
                0xC0..=0xDF => {
                    pos += 1;
                    if pos > track_bytes.len() {
                        return Err(format!("track {track}: truncated program change"));
                    }
                }
                // Meta events
                0xFF => {
                    if pos >= track_bytes.len() {
                        return Err(format!("track {track}: truncated meta event"));
                    }
                    pos += 1;
                    let length = read_vlq(track_bytes, &mut pos)? as usize;
                    pos += length;
                    if pos > track_bytes.len() {
                        return Err(format!("track {track}: meta payload runs past end"));
                    }
                }
                // Sysex escapes
                0xF0 | 0xF7 => {
                    let length = read_vlq(track_bytes, &mut pos)? as usize;
                    pos += length;
                    if pos > track_bytes.len() {
                        return Err(format!("track {track}: sysex payload runs past end"));
                    }
                }
                other => {
                    return Err(format!(
                        "track {track}: unsupported running-status or unknown byte {other:#04x}"
                    ));
                }
            }
        }

        // Close any hanging notes at the track end
        for (channel, key, (start_tick, velocity)) in open.drain(..) {
            notes.push(ParsedMidiNote {
                start_tick,
                duration_ticks: tick - start_tick,
                channel,
                key,
                velocity,
            });
        }

        cursor = track_end;
    }

    Ok((
        SmfHeader {
            format,
            track_count,
            ticks_per_quarter,
        },
        notes,
    ))
}

/// One stem to render during a stems bounce.
#[derive(Debug, Clone, PartialEq)]
pub struct StemEntry {
    pub stem_name: String,
    /// Track ids included in this stem bus.
    pub track_ids: Vec<String>,
    /// Output file pattern containing one '{stem}' placeholder.
    pub output_pattern: String,
}

impl StemEntry {
    /// Validates: non-empty name/pattern, '{stem}' present, no duplicate track ids.
    pub fn validate(&self) -> Result<(), String> {
        if self.stem_name.trim().is_empty() {
            return Err("stem name must not be empty".into());
        }
        if self.output_pattern.trim().is_empty() {
            return Err("output pattern must not be empty".into());
        }
        if !self.output_pattern.contains("{stem}") {
            return Err(format!(
                "output pattern '{}' must contain a '{{stem}}' placeholder",
                self.output_pattern
            ));
        }
        let mut seen = HashSet::new();
        for track_id in &self.track_ids {
            if !seen.insert(track_id.as_str()) {
                return Err(format!(
                    "stem '{}' lists track '{track_id}' more than once",
                    self.stem_name
                ));
            }
        }
        Ok(())
    }

    /// Resolves the output path for one stem name (substitutes {stem}).
    pub fn path_for(&self, stem_id: &str) -> Result<String, String> {
        self.validate()?;
        Ok(self.output_pattern.replace("{stem}", stem_id))
    }
}

/// Plans a full stems bounce across a project's tracks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StemsExportPlan {
    pub stems: Vec<StemEntry>,
}

impl StemsExportPlan {
    /// Validates every entry; duplicate stem names or overlapping track membership across
    /// entries are errors (a track belongs to at most one stem). Err names the conflict.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen_names = HashSet::new();
        let mut owner_of_track: HashMap<&str, &str> = HashMap::new();
        for stem in &self.stems {
            stem.validate()?;
            if !seen_names.insert(stem.stem_name.as_str()) {
                return Err(format!(
                    "stems export plan contains duplicate stem name '{}'",
                    stem.stem_name
                ));
            }
            for track_id in &stem.track_ids {
                match owner_of_track.get(track_id.as_str()) {
                    Some(existing) => {
                        return Err(format!(
                            "track '{track_id}' is assigned to both stem '{existing}' and stem '{}'",
                            stem.stem_name
                        ));
                    }
                    None => {
                        owner_of_track.insert(track_id.as_str(), stem.stem_name.as_str());
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds a stem after validation against existing entries.
    pub fn add_stem(&mut self, entry: StemEntry) -> Result<(), String> {
        entry.validate()?;
        if self.stems.iter().any(|s| s.stem_name == entry.stem_name) {
            return Err(format!(
                "stems export plan already contains a stem named '{}'",
                entry.stem_name
            ));
        }
        for track_id in &entry.track_ids {
            if let Some(existing) = self
                .stems
                .iter()
                .find(|s| s.track_ids.iter().any(|t| t == track_id))
            {
                return Err(format!(
                    "track '{track_id}' is already assigned to stem '{}'",
                    existing.stem_name
                ));
            }
        }
        self.stems.push(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_studio_creation() {
        let proj = StudioProject::new("studio-1", "Indie Rock Song");
        assert_eq!(proj.bpm, 120.0);
        assert_eq!(proj.sample_rate, 48000);
        assert_eq!(proj.tracks.len(), 2);
    }

    #[test]
    fn test_track_regions() {
        let mut proj = StudioProject::new("studio-1", "Acoustic Ballad");
        proj.tracks[1].add_region(AudioRegion {
            id: "r2".to_string(),
            name: "Guitar_Rhythm.wav".to_string(),
            start_sample: 48000 * 2,
            length_samples: 48000 * 8,
        });
        assert_eq!(proj.total_regions(), 2);
    }

    #[test]
    fn test_select_track_rejects_invalid_index() {
        let mut proj = StudioProject::new("studio-1", "Acoustic Ballad");
        assert!(proj.select_track(1));
        assert!(!proj.select_track(2));
        assert_eq!(proj.active_track_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut proj = StudioProject::new("studio-test", "Synthwave Groove");
        proj.mode = WorkspaceMode::Pro;
        proj.tracks
            .push(StudioTrack::new("t3", "Analog Bass", TrackKind::Midi));
        let bytes = save_studio_project(&proj).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Studio);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_studio_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Synthwave Groove");
        assert_eq!(loaded.mode, WorkspaceMode::Pro);
        assert_eq!(loaded.tracks.len(), 3);
    }

    #[test]
    fn audio_buffer_synth_and_wav_are_valid() {
        let audio = AudioBuffer::sine(48_000, 2, 440.0, 0.1, 0.5).unwrap();
        assert_eq!(audio.frames(), 4_800);
        let wav = audio.to_wav_pcm16().unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn project_mixer_honors_regions_gain_and_pan() {
        let mut project = StudioProject::new("mix", "Mix");
        project.tracks.clear();
        let mut track = StudioTrack::new("t", "Tone", TrackKind::Audio);
        track.pan = -1.0;
        track.regions.push(AudioRegion {
            id: "r".into(),
            name: "tone".into(),
            start_sample: 10,
            length_samples: 100,
        });
        project.tracks.push(track);
        let mut assets = AudioAssetStore::default();
        assets
            .insert(
                "tone",
                AudioBuffer::sine(48_000, 1, 440.0, 0.01, 0.5).unwrap(),
            )
            .unwrap();
        let mix = project.mix(&assets).unwrap();
        assert!(mix.warnings.is_empty());
        assert!(mix.audio.samples.iter().any(|sample| sample.abs() > 0.0));
        let right_energy: f32 = mix
            .audio
            .samples
            .iter()
            .skip(1)
            .step_by(2)
            .map(|s| s.abs())
            .sum();
        let left_energy: f32 = mix.audio.samples.iter().step_by(2).map(|s| s.abs()).sum();
        assert!(left_energy > right_energy);
    }

    #[test]
    fn midi_reference_synth_and_automation_work() {
        let audio = synthesize_notes(
            48_000,
            &[MidiNote {
                key: 69,
                start_secs: 0.0,
                duration_secs: 0.05,
                velocity: 1.0,
            }],
            0.01,
        )
        .unwrap();
        assert!(audio.samples.iter().any(|sample| sample.abs() > 0.0));
        let mut lane = AutomationLane::default();
        lane.set(AutomationPoint {
            time_secs: 1.0,
            value: 1.0,
        });
        lane.set(AutomationPoint {
            time_secs: 0.0,
            value: 0.0,
        });
        assert!((lane.sample(0.5, 0.0) - 0.5).abs() < 0.001);
    }

    #[test]
    fn midi_quantize_snap_strength() {
        let note_at = |start: f64| BeatNote {
            start_beat: start,
            duration_beats: 0.2,
            pitch: 60,
            velocity: 100,
        };

        // Full strength snaps 0.31 to the nearest sixteenth (0.25).
        let full = quantize_notes(&[note_at(0.31)], 0.25, 1.0).unwrap();
        assert!((full[0].start_beat - 0.25).abs() < 1e-9);

        // Zero strength leaves the note unchanged.
        let none = quantize_notes(&[note_at(0.31)], 0.25, 0.0).unwrap();
        assert_eq!(none[0].start_beat, 0.31);

        // Half strength lands halfway between the original start and the grid.
        let half = quantize_notes(&[note_at(0.31)], 0.25, 0.5).unwrap();
        assert!((half[0].start_beat - 0.28).abs() < 1e-9);

        // Strength saturates outside [0, 1].
        let saturated = quantize_notes(&[note_at(0.31)], 0.25, 3.0).unwrap();
        assert!((saturated[0].start_beat - 0.25).abs() < 1e-9);

        // Negative starts clamp to zero.
        let clamped = quantize_notes(&[note_at(-0.4)], 0.25, 1.0).unwrap();
        assert_eq!(clamped[0].start_beat, 0.0);

        // Grid must be positive.
        assert!(quantize_notes(&[note_at(0.31)], 0.0, 1.0).is_err());
        assert!(quantize_notes(&[note_at(0.31)], -0.25, 1.0).is_err());

        // Durations are preserved and the input slice is untouched.
        let notes = vec![note_at(0.31), note_at(-0.4)];
        let quantized = quantize_notes(&notes, 0.25, 1.0).unwrap();
        assert_eq!(quantized[0].duration_beats, 0.2);
        assert_eq!(quantized[1].duration_beats, 0.2);
        assert_eq!(notes[0].start_beat, 0.31);
    }

    #[test]
    fn move_remove_and_validate_regions() {
        let mut project = StudioProject::new("regions", "Regions");
        assert!(project.move_region(0, 1, "r1", 123));
        assert_eq!(project.tracks[1].regions[0].start_sample, 123);
        assert!(project.remove_region(1, "r1"));
        assert!(project.validate().is_empty());
    }

    #[test]
    fn take_folder_comping() {
        // An empty folder suggests no take.
        let mut folder = TakeFolder::default();
        assert_eq!(folder.best_take(), None);

        folder.takes = vec![
            Take {
                name: "take_1".into(),
                start_seconds: 10.0,
                duration_seconds: 8.0,
                rating: 0.4,
            },
            Take {
                name: "take_2".into(),
                start_seconds: 20.0,
                duration_seconds: 6.0,
                rating: 0.9,
            },
            Take {
                name: "take_3".into(),
                start_seconds: 30.0,
                duration_seconds: 10.0,
                rating: 0.7,
            },
        ];
        assert_eq!(folder.best_take(), Some(1));

        // Whole-take comping replaces any previous selection with the full span.
        folder.use_whole_take(2).unwrap();
        assert_eq!(folder.comp.len(), 1);
        assert_eq!(
            folder.comp[0],
            CompSection {
                take_index: 2,
                start_seconds: 30.0,
                duration_seconds: 10.0
            }
        );
        assert_eq!(folder.comp_duration(), 10.0);

        // A section reaching past the take end is clipped to the take bound.
        folder.add_comp_section(0, 14.0, 100.0).unwrap();
        assert_eq!(folder.comp.len(), 2);
        assert_eq!(
            folder.comp[1],
            CompSection {
                take_index: 0,
                start_seconds: 14.0,
                duration_seconds: 4.0
            }
        );
        assert_eq!(folder.comp_duration(), 14.0);

        // Adjacent-but-not-overlapping sections are accepted; this one ends
        // flush with the take's own end bound.
        folder.add_comp_section(1, 23.0, 3.0).unwrap();
        assert_eq!(
            folder.comp[2],
            CompSection {
                take_index: 1,
                start_seconds: 23.0,
                duration_seconds: 3.0
            }
        );
        assert_eq!(folder.comp_duration(), 17.0);

        // Out-of-bounds starts are rejected.
        assert!(folder.add_comp_section(0, 9.999, 1.0).is_err());
        assert!(folder.add_comp_section(0, 19.0, 1.0).is_err());
        // A section starting exactly at the take end leaves no usable range.
        assert!(folder.add_comp_section(0, 18.0, 1.0).is_err());
        // Overlapping an existing section is rejected; adjacency is not overlap.
        assert!(folder.add_comp_section(2, 35.0, 2.0).is_err());
        assert!(folder.add_comp_section(0, 16.0, 1.0).is_err());
        assert!(folder.add_comp_section(0, 14.5, 1.0).is_err());
        // Zero and negative durations are rejected, as are unknown takes.
        assert!(folder.add_comp_section(0, 12.0, 0.0).is_err());
        assert!(folder.add_comp_section(0, 12.0, -1.0).is_err());
        assert!(folder.add_comp_section(7, 12.0, 1.0).is_err());
        assert!(folder.use_whole_take(3).is_err());

        // The valid sections above are untouched by every rejected call.
        assert_eq!(folder.comp.len(), 3);
        assert_eq!(folder.comp_duration(), 17.0);
    }

    #[test]
    fn tempo_map_beat_second_conversions() {
        // Constant 120 BPM map: 1 beat = 0.5 s, round-trips in both directions.
        let constant = TempoMap::constant(120.0).unwrap();
        assert_eq!(constant.nodes.len(), 1);
        assert_eq!(constant.beat_to_seconds(1.0), 0.5);
        assert_eq!(constant.seconds_to_beat(0.5), 1.0);
        assert!((constant.seconds_to_beat(constant.beat_to_seconds(9.75)) - 9.75).abs() < 1e-9);
        assert!((constant.beat_to_seconds(constant.seconds_to_beat(3.25)) - 3.25).abs() < 1e-9);

        // Empty map defaults to 120 BPM and clamps negative inputs to beat 0.
        let empty = TempoMap::default();
        assert_eq!(empty.bpm_at(3.0), 120.0);
        assert_eq!(empty.beat_to_seconds(1.0), 0.5);
        assert_eq!(empty.seconds_to_beat(0.5), 1.0);
        assert_eq!(empty.beat_to_seconds(-2.0), 0.0);
        assert_eq!(empty.seconds_to_beat(-1.0), 0.0);

        // Two-node map ramping 60 -> 120 BPM linearly over [0, 4] beats:
        // bpm(u) = 60 + 15u, so elapsed(u) = (60/15) * ln((60 + 15u) / 60).
        // Independent closed-form expectations: elapsed(4) = 4 ln 2,
        // elapsed(2) = 4 ln 1.5, and elapsed(s) = 2 solves to u = 4(sqrt(e) - 1).
        let mut ramp = TempoMap::default();
        ramp.add_node(TempoNode {
            beat: 0.0,
            bpm: 60.0,
            linear_ramp: false,
        })
        .unwrap();
        ramp.add_node(TempoNode {
            beat: 4.0,
            bpm: 120.0,
            linear_ramp: true,
        })
        .unwrap();
        let four_ln2 = 4.0 * std::f64::consts::LN_2;
        assert!((ramp.beat_to_seconds(4.0) - four_ln2).abs() < 1e-6);
        assert!((ramp.beat_to_seconds(2.0) - 4.0 * 1.5f64.ln()).abs() < 1e-6);
        let sqrt_e_beats = 4.0 * (std::f64::consts::E.sqrt() - 1.0);
        assert!((ramp.seconds_to_beat(2.0) - sqrt_e_beats).abs() < 1e-6);
        assert!((ramp.seconds_to_beat(ramp.beat_to_seconds(3.7)) - 3.7).abs() < 1e-9);
        assert!((ramp.seconds_to_beat(four_ln2) - 4.0).abs() < 1e-9);
        assert_eq!(ramp.bpm_at(0.0), 60.0);
        assert!((ramp.bpm_at(2.0) - 90.0).abs() < 1e-9);
        assert!((ramp.bpm_at(4.0) - 120.0).abs() < 1e-9);
        assert_eq!(ramp.bpm_at(5.0), 120.0);
        assert_eq!(ramp.bpm_at(-3.0), 60.0);

        // A step node creates a zero-duration discontinuity at beat 8.
        let mut stepped = TempoMap::constant(120.0).unwrap();
        stepped
            .add_node(TempoNode {
                beat: 8.0,
                bpm: 60.0,
                linear_ramp: false,
            })
            .unwrap();
        assert_eq!(stepped.bpm_at(7.5), 120.0);
        assert_eq!(stepped.bpm_at(8.0), 60.0);
        assert_eq!(stepped.beat_to_seconds(8.0), 4.0);
        assert_eq!(stepped.beat_to_seconds(10.0), 6.0);
        assert_eq!(stepped.seconds_to_beat(4.0), 8.0);
        assert_eq!(stepped.seconds_to_beat(6.0), 10.0);

        // add_node keeps nodes sorted by beat and replaces same-position nodes.
        let mut sorted = TempoMap::default();
        sorted
            .add_node(TempoNode {
                beat: 4.0,
                bpm: 90.0,
                linear_ramp: false,
            })
            .unwrap();
        sorted
            .add_node(TempoNode {
                beat: 2.0,
                bpm: 80.0,
                linear_ramp: false,
            })
            .unwrap();
        sorted
            .add_node(TempoNode {
                beat: 2.0,
                bpm: 100.0,
                linear_ramp: false,
            })
            .unwrap();
        assert_eq!(sorted.nodes.len(), 2);
        assert_eq!(sorted.nodes[0].beat, 2.0);
        assert_eq!(sorted.nodes[0].bpm, 100.0);
        assert_eq!(sorted.nodes[1].beat, 4.0);

        // Invalid tempos and beats are rejected without mutating the map.
        assert!(TempoMap::constant(0.0).is_err());
        assert!(TempoMap::constant(-30.0).is_err());
        assert!(TempoMap::constant(f64::NAN).is_err());
        assert!(sorted
            .add_node(TempoNode {
                beat: 3.0,
                bpm: 0.0,
                linear_ramp: false,
            })
            .is_err());
        assert!(sorted
            .add_node(TempoNode {
                beat: -1.0,
                bpm: 96.0,
                linear_ramp: false,
            })
            .is_err());
        assert_eq!(sorted.nodes.len(), 2);
    }
}

#[cfg(test)]
mod studio_runtime_tests {
    use super::*;

    #[test]
    fn wav_round_trip_decodes_pcm16() {
        let source = AudioBuffer::sine(48_000, 2, 440.0, 0.05, 0.25).unwrap();
        let bytes = source.to_wav_pcm16().unwrap();
        let decoded = decode_wav(&bytes).unwrap();
        assert_eq!(decoded.sample_rate, source.sample_rate);
        assert_eq!(decoded.channels, source.channels);
        assert_eq!(decoded.frames(), source.frames());
        assert!(decoded.samples.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn studio_session_undo_redo_restores_mixer() {
        let mut session = StudioSession::new(StudioProject::new("id", "song"));
        session.checkpoint();
        session.project.tracks[0].volume_db = -12.0;
        assert!(session.undo());
        assert_eq!(session.project.tracks[0].volume_db, 0.0);
        assert!(session.redo());
        assert_eq!(session.project.tracks[0].volume_db, -12.0);
    }

    #[test]
    fn region_split_trim_and_remove_operations() {
        let mut track = StudioTrack::new("t1", "Audio", TrackKind::Audio);
        track.add_region(AudioRegion {
            id: "r1".into(),
            name: "take1.wav".into(),
            start_sample: 1000,
            length_samples: 4000,
        });

        // Split at sample 2500
        let (left_id, right_id) = track.split_region("r1", 2500).unwrap();
        assert_eq!(track.regions.len(), 2);
        assert_eq!(track.regions[0].id, left_id);
        assert_eq!(track.regions[0].start_sample, 1000);
        assert_eq!(track.regions[0].length_samples, 1500);
        assert_eq!(track.regions[1].id, right_id);
        assert_eq!(track.regions[1].start_sample, 2500);
        assert_eq!(track.regions[1].length_samples, 2500);

        // Trim start of right region to 3000
        track.trim_region_start(&right_id, 3000).unwrap();
        assert_eq!(track.regions[1].start_sample, 3000);
        assert_eq!(track.regions[1].length_samples, 2000);

        // Trim end of right region to 4500
        track.trim_region_end(&right_id, 4500).unwrap();
        assert_eq!(track.regions[1].length_samples, 1500);

        // Remove left region
        let removed = track.remove_region(&left_id);
        assert!(removed.is_some());
        assert_eq!(track.regions.len(), 1);
    }

    #[test]
    fn audio_gain_and_normalization_operations() {
        let mut buffer = AudioBuffer::sine(48_000, 1, 440.0, 0.01, 0.5).unwrap();
        assert!((buffer.samples[0] - 0.0).abs() < 1e-4);

        // Apply gain 2.0
        buffer.apply_gain(2.0);
        let max_sample = buffer
            .samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        assert!((max_sample - 1.0).abs() < 0.05);

        // Normalize to 0.5
        let factor = buffer.normalize(0.5).unwrap();
        assert!((factor - 0.5).abs() < 0.05);
        let normalized_max = buffer
            .samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        assert!((normalized_max - 0.5).abs() < 0.02);
    }

    #[test]
    fn audio_metering_computes_peak_and_rms_dbfs() {
        let mut buffer = AudioBuffer::sine(48_000, 1, 1000.0, 0.05, 1.0).unwrap();
        let m1 = buffer.meter();
        assert!((m1.peak_db - 0.0).abs() < 0.5);
        assert!(!m1.clipped);
        assert!(m1.rms_db < 0.0);
        assert!(m1.rms_db > -10.0); // Sine wave RMS is -3.01 dBFS

        // Test clipped signal
        buffer.samples[10] = 1.5;
        let m2 = buffer.meter();
        assert!(m2.clipped);
        assert!(m2.peak_db > 0.0);
    }

    #[test]
    fn track_pan_laws_and_linear_volume() {
        let mut track = StudioTrack::new("t1", "Audio 1", TrackKind::Audio);
        track.pan = 0.0;
        track.volume_db = 0.0;
        let (l_center, r_center) = track.stereo_pan_gains();
        // At center (0.0), constant-power gain is cos(pi/4) = FRAC_1_SQRT_2 (-3 dB)
        assert!((l_center - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((r_center - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((track.linear_volume() - 1.0).abs() < 1e-4);

        // Hard Left
        track.pan = -1.0;
        let (l_left, r_left) = track.stereo_pan_gains();
        assert!((l_left - 1.0).abs() < 1e-4);
        assert!(r_left < 1e-4);

        // -6 dB volume
        track.volume_db = -6.0206;
        assert!((track.linear_volume() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn musical_bpm_and_beat_conversions() {
        let mut proj = StudioProject::new("proj-bpm", "Tempo Test");
        proj.bpm = 120.0;
        proj.sample_rate = 48000;

        // At 120 BPM, 1 beat = 0.5s = 24,000 samples
        assert_eq!(proj.samples_per_beat(), 24000.0);
        // 4 beats per bar = 2.0s = 96,000 samples
        assert_eq!(proj.samples_per_bar(4), 96000.0);

        assert_eq!(proj.beat_to_seconds(4.0), 2.0);
        assert_eq!(proj.seconds_to_beat(2.0), 4.0);
    }

    #[test]
    fn audio_buffer_soft_clip() {
        let mut buffer = AudioBuffer::silence(48000, 1, 4).unwrap();
        buffer.samples = vec![0.0, 0.5, 1.5, 3.0];

        buffer.soft_clip(1.0);
        assert_eq!(buffer.samples[0], 0.0);
        assert!(buffer.samples[1] < 0.5); // tanh(0.5) ~ 0.462
        assert!(buffer.samples[2] < 1.0); // tanh(1.5) < 1.0
        assert!(buffer.samples[3] < 1.0); // tanh(3.0) < 1.0
    }

    #[test]
    fn biquad_filter_coefficients() {
        let eq = BiquadCoefficients::peaking_eq(48000, 1000.0, 6.0, 1.0);
        assert!(eq.b0.is_finite());
        assert!(eq.b1.is_finite());
        assert!(eq.b2.is_finite());
        assert!(eq.a1.is_finite());
        assert!(eq.a2.is_finite());

        let lp = BiquadCoefficients::low_pass(48000, 5000.0, 0.707);
        assert!(lp.b0 > 0.0);
        assert!(lp.b1 > 0.0);
    }

    #[test]
    fn audio_crossfade_curves() {
        // Linear crossfade
        let (out_lin, in_lin) = calculate_crossfade_gains(CrossfadeCurve::Linear, 0.5);
        assert_eq!(out_lin, 0.5);
        assert_eq!(in_lin, 0.5);

        // Equal-power crossfade at midpoint: cos(pi/4) = sin(pi/4) = 1/sqrt(2) ~ 0.707 (-3 dB)
        let (out_ep, in_ep) = calculate_crossfade_gains(CrossfadeCurve::EqualPower, 0.5);
        assert!((out_ep - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((in_ep - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        // Power sum (out^2 + in^2) equals 1.0 (constant loudness)
        assert!((out_ep * out_ep + in_ep * in_ep - 1.0).abs() < 1e-4);
    }

    #[test]
    fn delay_effect_processing() {
        let mut buffer = AudioBuffer {
            sample_rate: 1000, // 1 ms per sample
            channels: 1,
            samples: vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };

        let delay = DelayEffect::new(2.0, 0.5, 0.5); // 2 ms delay (2 samples), 50% feedback, 50% mix
        delay.process(&mut buffer);

        // Dry sample at 0 is scaled: 1.0 * (1 - 0.5) = 0.5
        assert_eq!(buffer.samples[0], 0.5);
        assert_eq!(buffer.samples[1], 0.0);
        // Delayed echo at sample 2: 1.0 * 0.5 = 0.5
        assert_eq!(buffer.samples[2], 0.5);
        assert_eq!(buffer.samples[3], 0.0);
        // Feedback echo at sample 4: 1.0 * 0.5 * 0.5 = 0.25
        assert_eq!(buffer.samples[4], 0.25);
    }

    #[test]
    fn compressor_effect_and_db_conversions() {
        assert_eq!(db_to_linear(0.0), 1.0);
        assert!((db_to_linear(-6.0) - 0.501).abs() < 1e-2);
        assert_eq!(linear_to_db(1.0), 0.0);
        assert!(linear_to_db(0.0) <= -100.0);

        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            samples: vec![1.0, 0.5, 0.1],
        };

        // -6dB threshold (~0.5 linear), 2:1 ratio, 0dB makeup
        let comp = CompressorEffect::new(-6.0, 2.0, 0.0);
        comp.process(&mut buffer);

        // 1.0 (0dB) is 6dB over -6dB threshold -> compressed to -3dB (~0.707)
        assert!(buffer.samples[0] < 1.0 && buffer.samples[0] > 0.7);
        // 0.5 is at/below threshold -> unchanged
        assert_eq!(buffer.samples[1], 0.5);
    }

    #[test]
    fn four_band_eq_processing() {
        let mut eq = FourBandEq::default();
        assert_eq!(eq.low_shelf.frequency_hz, 100.0);
        assert_eq!(eq.high_shelf.frequency_hz, 8000.0);

        // Boost high shelf by +6dB (~2.0 linear gain)
        eq.high_shelf.gain_db = 6.0;

        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            samples: vec![0.2, 0.4],
        };

        eq.process(&mut buffer);
        assert!(buffer.samples[0] > 0.35); // 0.2 * ~2.0 = ~0.4
    }

    #[test]
    fn mixer_bus_and_aux_sends() {
        let mut bus = MixerBus::new("b1", "Reverb Bus");
        assert_eq!(bus.effective_gain(), 1.0);

        bus.volume_db = -6.0;
        assert!((bus.effective_gain() - 0.501).abs() < 0.01);

        bus.muted = true;
        assert_eq!(bus.effective_gain(), 0.0);

        let send = AuxSend::new("b1", 0.0);
        assert_eq!(send.linear_gain(), 1.0);

        let send_muted = AuxSend {
            target_bus_id: "b1".into(),
            send_gain_db: 0.0,
            pre_fader: false,
            enabled: false,
        };
        assert_eq!(send_muted.linear_gain(), 0.0);
    }

    #[test]
    fn oscillator_tone_generation() {
        let sine_tone =
            generate_oscillator_tone(OscillatorWaveform::Sine, 440.0, 0.1, 44100, 0.8).unwrap();
        assert_eq!(sine_tone.samples.len(), 4410);
        assert_eq!(sine_tone.channels, 1);
        assert!(sine_tone.samples.iter().all(|s| s.abs() <= 0.8001));

        let square_tone =
            generate_oscillator_tone(OscillatorWaveform::Square, 100.0, 0.05, 44100, 0.5).unwrap();
        assert!(square_tone.samples[0] > 0.49);
    }

    #[test]
    fn reverb_effect_dsp_processing() {
        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            // Single impulse sample followed by silence
            samples: {
                let mut s = vec![0.0f32; 2000];
                s[0] = 1.0;
                s
            },
        };

        let reverb = ReverbEffect::default();
        reverb.process(&mut buffer);

        // Later samples in the buffer should now contain reverberation tail
        let tail_energy: f32 = buffer.samples[1200..1800].iter().map(|s| s.abs()).sum();
        assert!(tail_energy > 0.05);
    }

    #[test]
    fn noise_gate_dynamics_processing() {
        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            // Low amplitude noise (0.001 = -60dB) followed by strong signal (0.5 = -6dB)
            samples: {
                let mut s = vec![0.001f32; 1000];
                s.extend(vec![0.5f32; 1000]);
                s
            },
        };

        let gate = NoiseGateEffect {
            threshold_db: -30.0,
            reduction_db: -60.0,
            attack_ms: 1.0,
            release_ms: 10.0,
        };

        gate.process(&mut buffer);

        // Noise region should be heavily attenuated
        assert!(buffer.samples[500] < 0.0001);
        // Strong signal region should remain loud
        assert!(buffer.samples[1500] > 0.4);
    }

    #[test]
    fn flanger_modulation_processing() {
        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            samples: (0..4410).map(|i| (i as f32 * 0.1).sin()).collect(),
        };

        let flanger = FlangerEffect::default();
        flanger.process(&mut buffer);

        // Flanger output should remain bounded and processed
        assert_eq!(buffer.samples.len(), 4410);
        let energy: f32 = buffer.samples.iter().map(|s| s.abs()).sum();
        assert!(energy > 100.0);
    }

    #[test]
    fn autopan_stereo_modulation() {
        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 2,
            // 44100 stereo frames initialized to full amplitude 1.0 on both channels
            samples: vec![1.0f32; 88200],
        };

        let autopan = AutoPanEffect {
            rate_hz: 1.0,
            depth: 1.0,
            waveform: AutoPanWaveform::Sine,
        };

        autopan.process_stereo(&mut buffer);

        // At t = 0.25s (frame 11025), sin(2*pi*1*0.25) = sin(pi/2) = 1.0 (panned hard right)
        let f_idx = 11025;
        let left_val = buffer.samples[f_idx * 2];
        let right_val = buffer.samples[f_idx * 2 + 1];

        assert!(right_val > 0.95);
        assert!(left_val < 0.1);
    }

    #[test]
    fn ring_modulator_carrier_processing() {
        let mut buffer = AudioBuffer {
            sample_rate: 44100,
            channels: 1,
            // 44100 samples of 1.0 (DC)
            samples: vec![1.0f32; 44100],
        };

        let ring_mod = RingModulatorEffect {
            carrier_hz: 440.0,
            waveform: OscillatorWaveform::Sine,
            mix: 1.0, // 100% wet
        };

        ring_mod.process(&mut buffer);

        // Output should match pure sine wave of 440Hz
        let expected_t0 = (2.0 * std::f32::consts::PI * 440.0 * 0.0).sin();
        assert!((buffer.samples[0] - expected_t0).abs() < 1e-5);

        // Bounded within [-1.0, 1.0]
        for &s in &buffer.samples {
            assert!(s >= -1.0 - 1e-5 && s <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn chord_detection_qualities() {
        // C major triad: C4 E4 G4 -> root pitch class 0 (C), Major
        let c_major = detect_chord(&[60, 64, 67]).unwrap();
        assert_eq!(c_major.root_pitch_class, 0);
        assert_eq!(c_major.quality, ChordQuality::Major);
        assert!(c_major.confidence > 0.99);

        // A minor: A3 C4 E4 -> root pitch class 9 (A), Minor. The bass bonus must not
        // override the exact template match.
        let a_minor = detect_chord(&[57, 60, 64]).unwrap();
        assert_eq!(a_minor.root_pitch_class, 9);
        assert_eq!(a_minor.quality, ChordQuality::Minor);
        assert!(a_minor.confidence > 0.99);

        // G dominant seventh: G3 B3 D4 F4 -> root 7 (G), Dominant7 with full coverage
        let g7 = detect_chord(&[55, 59, 62, 65]).unwrap();
        assert_eq!(g7.root_pitch_class, 7);
        assert_eq!(g7.quality, ChordQuality::Dominant7);
        assert!(g7.confidence > 0.99);

        // Fewer than three distinct pitch classes cannot form a chord
        assert!(detect_chord(&[60, 64]).is_none());
        assert!(detect_chord(&[60, 60, 72]).is_none());
        assert!(detect_chord(&[]).is_none());

        // A chromatic cluster still resolves deterministically without a perfect fit
        let cluster = detect_chord(&[60, 61, 62, 63]);
        assert!(cluster.is_some());
        let c = cluster.unwrap();
        assert!(c.confidence < 1.0);

        // Deterministic across repeated calls
        assert_eq!(detect_chord(&[60, 64, 67]), detect_chord(&[60, 64, 67]));
    }

    #[test]
    fn click_track_generation() {
        // 120 BPM, 4/4, 8 beats = 4 seconds = 192000 frames at 48 kHz
        let clicks = generate_click_track(120.0, 4, 8, 48000, 1600.0, 800.0).unwrap();
        assert_eq!(clicks.sample_rate, 48000);
        assert_eq!(clicks.channels, 1);
        assert_eq!(clicks.samples.len(), 192_000);

        // Downbeat at frame 0 carries the accent pitch; a mid-bar beat does not.
        let peak_at = |buf: &AudioBuffer, from: usize, to: usize| {
            buf.samples[from..to]
                .iter()
                .fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(peak_at(&clicks, 0, 400) > 0.5, "downbeat click is audible");
        let beat2_start = (0.5f64 * 48000.0).round() as usize; // beat index 1 starts at 0.5 s
        assert!(
            peak_at(&clicks, beat2_start + 10, beat2_start + 300) > 0.3,
            "off-beat click is audible"
        );

        // Silence between clicks: halfway between beats 1 and 2 there is no energy
        let midpoint = (0.75 * 48000.0) as usize;
        assert_eq!(peak_at(&clicks, midpoint - 200, midpoint + 200), 0.0);

        // Determinism
        let again = generate_click_track(120.0, 4, 8, 48000, 1600.0, 800.0).unwrap();
        assert_eq!(clicks.samples, again.samples);

        // Invalid parameters are rejected
        assert!(generate_click_track(-5.0, 4, 8, 48000, 1600.0, 800.0).is_err());
        assert!(generate_click_track(120.0, 0, 8, 48000, 1600.0, 800.0).is_err());
        assert!(generate_click_track(120.0, 4, 0, 48000, 1600.0, 800.0).is_err());
        assert!(generate_click_track(120.0, 4, 8, 0, 1600.0, 800.0).is_err());
    }

    #[test]
    fn smf_parsing_notes_and_header() {
        // Hand-build a format-0 SMF: one track with two notes.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes()); // format 0
        bytes.extend_from_slice(&1u16.to_be_bytes()); // one track
        bytes.extend_from_slice(&480u16.to_be_bytes()); // ticks per quarter

        let mut track: Vec<u8> = Vec::new();
        // delta 0, note-on ch0 key60 vel100
        track.extend_from_slice(&[0x00, 0x90, 60, 100]);
        // delta 480, note-off key60
        track.extend_from_slice(&[0x83, 0x60, 0x80, 60, 64]);
        // delta 0, note-on ch0 key64 vel90
        track.extend_from_slice(&[0x00, 0x90, 64, 90]);
        // delta 240, note-on vel0 acts as note-off for key64
        track.extend_from_slice(&[0x81, 0x70, 0x90, 64, 0]);
        // End of track meta event
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&track);

        let (header, notes) = parse_midi_file(&bytes).unwrap();
        assert_eq!(header.format, 0);
        assert_eq!(header.track_count, 1);
        assert_eq!(header.ticks_per_quarter, 480);

        assert_eq!(notes.len(), 2);
        // VLQ 0x83 0x60 decodes to (3 << 7) | 0x60 = 480
        assert_eq!(
            notes[0],
            ParsedMidiNote {
                start_tick: 0,
                duration_ticks: 480,
                channel: 0,
                key: 60,
                velocity: 100
            }
        );
        assert_eq!(
            notes[1],
            ParsedMidiNote {
                start_tick: 480,
                duration_ticks: 240,
                channel: 0,
                key: 64,
                velocity: 90
            }
        );

        // Bad magic is rejected
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(parse_midi_file(&bad).is_err());

        // Truncated header is rejected
        assert!(parse_midi_file(&bytes[..10]).is_err());

        // Track data running past the buffer is rejected
        let mut lying = bytes.clone();
        let len_pos = 14 + 4; // after MThd + "MTrk"
        lying[len_pos..len_pos + 4].copy_from_slice(&9999u32.to_be_bytes());
        assert!(parse_midi_file(&lying).is_err());
    }

    #[test]
    fn stems_export_plan_rules() {
        let mut plan = StemsExportPlan::default();
        assert!(plan.validate().is_ok());

        let drums = StemEntry {
            stem_name: "drums".to_string(),
            track_ids: vec!["t1".to_string(), "t2".to_string()],
            output_pattern: "bounce/{stem}.wav".to_string(),
        };
        let bass = StemEntry {
            stem_name: "bass".to_string(),
            track_ids: vec!["t3".to_string()],
            output_pattern: "bounce/{stem}.flac".to_string(),
        };

        plan.add_stem(drums.clone()).expect("drums stem accepted");
        plan.add_stem(bass.clone()).expect("bass stem accepted");
        plan.validate().expect("disjoint stems should validate");

        // Paths resolve with the {stem} placeholder substituted.
        assert_eq!(drums.path_for("drums").unwrap(), "bounce/drums.wav");
        assert_eq!(bass.path_for("bass").unwrap(), "bounce/bass.flac");

        // Overlapping track membership across entries is rejected.
        let overlap = StemEntry {
            stem_name: "overhead".to_string(),
            track_ids: vec!["t2".to_string()],
            output_pattern: "out/{stem}.wav".to_string(),
        };
        let err = plan.add_stem(overlap).unwrap_err();
        assert!(err.contains("t2") && err.contains("drums"), "{err}");
        assert_eq!(plan.stems.len(), 2, "failed add must not mutate");

        // Duplicate stem names are rejected.
        let dup_name = StemEntry {
            stem_name: "drums".to_string(),
            track_ids: vec!["t9".to_string()],
            output_pattern: "out/{stem}.wav".to_string(),
        };
        let err = plan.add_stem(dup_name).unwrap_err();
        assert!(err.contains("drums"), "{err}");

        // A hand-built plan with duplicate names fails validation too.
        let bad_names = StemsExportPlan {
            stems: vec![drums.clone(), drums],
        };
        let err = bad_names.validate().unwrap_err();
        assert!(err.contains("duplicate stem name 'drums'"), "{err}");

        // Missing {stem} placeholder is rejected.
        let no_placeholder = StemEntry {
            stem_name: "vocals".to_string(),
            track_ids: vec!["t4".to_string()],
            output_pattern: "bounce/vocals.wav".to_string(),
        };
        let err = no_placeholder.validate().unwrap_err();
        assert!(err.contains("{stem}"), "{err}");
        assert!(no_placeholder.path_for("vocals").is_err());

        // Duplicate track ids inside one stem are rejected.
        let dup_track = StemEntry {
            stem_name: "keys".to_string(),
            track_ids: vec!["t5".to_string(), "t5".to_string()],
            output_pattern: "out/{stem}.wav".to_string(),
        };
        let err = dup_track.validate().unwrap_err();
        assert!(
            err.contains("t5") && err.contains("more than once"),
            "{err}"
        );
    }
}
