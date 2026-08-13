//! Native local audio and MIDI I/O for Loom Studio.
//!
//! The audio callback never allocates or takes a mutex. Playback swaps immutable
//! buffers atomically, and recording writes into a bounded lock-free queue.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossbeam_queue::ArrayQueue;
use loom_studio_core::AudioBuffer;
use midir::{Ignore, MidiInput, MidiInputConnection};

#[derive(Debug)]
struct PlaybackClip {
    sample_rate: u32,
    channels: usize,
    samples: Vec<f32>,
}

#[derive(Debug)]
struct PlaybackShared {
    clip: ArcSwapOption<PlaybackClip>,
    position_bits: AtomicU64,
    playing: AtomicBool,
    looping: AtomicBool,
}

impl Default for PlaybackShared {
    fn default() -> Self {
        Self {
            clip: ArcSwapOption::empty(),
            position_bits: AtomicU64::new(0.0_f64.to_bits()),
            playing: AtomicBool::new(false),
            looping: AtomicBool::new(false),
        }
    }
}

#[derive(Debug)]
struct CaptureShared {
    samples: ArrayQueue<f32>,
    recording: AtomicBool,
    overruns: AtomicU64,
}

impl CaptureShared {
    fn new(capacity: usize) -> Self {
        Self {
            samples: ArrayQueue::new(capacity),
            recording: AtomicBool::new(false),
            overruns: AtomicU64::new(0),
        }
    }
}

/// Compact MIDI event retained by the local input monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent {
    pub timestamp_micros: u64,
    pub bytes: [u8; 3],
    pub len: u8,
}

/// CPAL audio engine plus a local MIDI monitor.
pub struct AudioIo {
    output_stream: Stream,
    input_stream: Option<Stream>,
    playback: Arc<PlaybackShared>,
    capture: Arc<CaptureShared>,
    input_rate: u32,
    input_channels: u16,
    midi_connection: Option<MidiInputConnection<()>>,
    midi_events: Arc<ArrayQueue<MidiEvent>>,
    output_device_name: String,
    input_device_name: Option<String>,
}

impl AudioIo {
    /// Opens the default local output device and, when available, input device.
    pub fn open_default() -> Result<Self, String> {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .ok_or_else(|| "no default audio output device is available".to_string())?;
        let output_device_name = output_device.to_string();
        let output_supported = output_device
            .default_output_config()
            .map_err(|error| format!("read output configuration: {error}"))?;
        let output_format = output_supported.sample_format();
        let output_config = output_supported.config();
        let playback = Arc::new(PlaybackShared::default());
        let output_stream = build_output_stream(
            &output_device,
            output_config,
            output_format,
            playback.clone(),
        )?;
        output_stream
            .play()
            .map_err(|error| format!("start output stream: {error}"))?;

        let capture = Arc::new(CaptureShared::new(2_000_000));
        let mut input_stream = None;
        let mut input_rate = 0;
        let mut input_channels = 0;
        let mut input_device_name = None;
        if let Some(input_device) = host.default_input_device() {
            if let Ok(supported) = input_device.default_input_config() {
                let format = supported.sample_format();
                let config = supported.config();
                input_rate = config.sample_rate;
                input_channels = config.channels;
                input_device_name = Some(input_device.to_string());
                if let Ok(stream) =
                    build_input_stream(&input_device, config, format, capture.clone())
                {
                    if stream.play().is_ok() {
                        input_stream = Some(stream);
                    }
                }
            }
        }

        Ok(Self {
            output_stream,
            input_stream,
            playback,
            capture,
            input_rate,
            input_channels,
            midi_connection: None,
            midi_events: Arc::new(ArrayQueue::new(2048)),
            output_device_name,
            input_device_name,
        })
    }

    pub fn output_device_name(&self) -> &str {
        &self.output_device_name
    }

    pub fn input_device_name(&self) -> Option<&str> {
        self.input_device_name.as_deref()
    }

    pub fn has_input(&self) -> bool {
        self.input_stream.is_some()
    }

    /// Atomically loads a rendered project mix for playback.
    pub fn load(&self, buffer: &AudioBuffer) -> Result<(), String> {
        buffer.validate()?;
        self.playback.clip.store(Some(Arc::new(PlaybackClip {
            sample_rate: buffer.sample_rate,
            channels: buffer.channels as usize,
            samples: buffer.samples.clone(),
        })));
        self.playback
            .position_bits
            .store(0.0_f64.to_bits(), Ordering::Release);
        Ok(())
    }

    pub fn play(&self) {
        self.playback.playing.store(true, Ordering::Release);
    }

    pub fn pause(&self) {
        self.playback.playing.store(false, Ordering::Release);
    }

    pub fn stop(&self) {
        self.pause();
        self.playback
            .position_bits
            .store(0.0_f64.to_bits(), Ordering::Release);
    }

    pub fn set_looping(&self, looping: bool) {
        self.playback.looping.store(looping, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        self.playback.playing.load(Ordering::Acquire)
    }

    pub fn position_seconds(&self) -> f64 {
        let source_frame = f64::from_bits(self.playback.position_bits.load(Ordering::Acquire));
        self.playback
            .clip
            .load_full()
            .map(|clip| source_frame / clip.sample_rate.max(1) as f64)
            .unwrap_or(0.0)
    }

    pub fn seek_seconds(&self, seconds: f64) {
        let frame = self
            .playback
            .clip
            .load_full()
            .map(|clip| {
                (seconds.max(0.0) * clip.sample_rate as f64)
                    .min(clip.samples.len() as f64 / clip.channels.max(1) as f64)
            })
            .unwrap_or(0.0);
        self.playback
            .position_bits
            .store(frame.to_bits(), Ordering::Release);
    }

    /// Begins bounded local input capture.
    pub fn start_recording(&self) -> Result<(), String> {
        if self.input_stream.is_none() {
            return Err("no audio input stream is available".into());
        }
        while self.capture.samples.pop().is_some() {}
        self.capture.overruns.store(0, Ordering::Release);
        self.capture.recording.store(true, Ordering::Release);
        Ok(())
    }

    /// Stops capture and returns the collected interleaved audio.
    pub fn stop_recording(&self) -> Result<(AudioBuffer, u64), String> {
        self.capture.recording.store(false, Ordering::Release);
        if self.input_rate == 0 || self.input_channels == 0 {
            return Err("audio input is unavailable".into());
        }
        let mut samples = Vec::with_capacity(self.capture.samples.len());
        while let Some(sample) = self.capture.samples.pop() {
            samples.push(sample);
        }
        let aligned = samples.len() - samples.len() % self.input_channels as usize;
        samples.truncate(aligned);
        let buffer = AudioBuffer {
            sample_rate: self.input_rate,
            channels: self.input_channels,
            samples,
        };
        buffer.validate()?;
        Ok((buffer, self.capture.overruns.load(Ordering::Acquire)))
    }

    pub fn is_recording(&self) -> bool {
        self.capture.recording.load(Ordering::Acquire)
    }

    /// Lists local MIDI input ports.
    pub fn midi_ports() -> Result<Vec<String>, String> {
        let midi =
            MidiInput::new("Loom Studio MIDI discovery").map_err(|error| error.to_string())?;
        Ok(midi
            .ports()
            .iter()
            .enumerate()
            .map(|(index, port)| {
                midi.port_name(port)
                    .unwrap_or_else(|_| format!("MIDI Input {}", index + 1))
            })
            .collect())
    }

    /// Connects one MIDI input port and retains bounded recent messages.
    pub fn connect_midi(&mut self, index: usize) -> Result<String, String> {
        self.midi_connection = None;
        let mut midi =
            MidiInput::new("Loom Studio MIDI input").map_err(|error| error.to_string())?;
        midi.ignore(Ignore::None);
        let ports = midi.ports();
        let port = ports
            .get(index)
            .ok_or_else(|| "MIDI input index is out of bounds".to_string())?;
        let name = midi
            .port_name(port)
            .unwrap_or_else(|_| format!("MIDI Input {}", index + 1));
        let events = self.midi_events.clone();
        let connection = midi
            .connect(
                port,
                "loom-studio-input",
                move |timestamp, message, _| {
                    let mut bytes = [0_u8; 3];
                    let len = message.len().min(3);
                    bytes[..len].copy_from_slice(&message[..len]);
                    let event = MidiEvent {
                        timestamp_micros: timestamp,
                        bytes,
                        len: len as u8,
                    };
                    if events.push(event).is_err() {
                        let _ = events.pop();
                        let _ = events.push(event);
                    }
                },
                (),
            )
            .map_err(|error| error.to_string())?;
        self.midi_connection = Some(connection);
        Ok(name)
    }

    pub fn drain_midi_events(&self) -> Vec<MidiEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.midi_events.pop() {
            events.push(event);
        }
        events
    }
}

impl Drop for AudioIo {
    fn drop(&mut self) {
        self.playback.playing.store(false, Ordering::Release);
        self.capture.recording.store(false, Ordering::Release);
        let _ = self.output_stream.pause();
        if let Some(stream) = &self.input_stream {
            let _ = stream.pause();
        }
    }
}

fn build_output_stream(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    playback: Arc<PlaybackShared>,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let device_rate = config.sample_rate;
    let error = |error| eprintln!("Loom Studio output stream error: {error}");
    match format {
        SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    fill_output(data, channels, device_rate, &playback, |value| value)
                },
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |data: &mut [i16], _| {
                    fill_output(data, channels, device_rate, &playback, |value| {
                        (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                    })
                },
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_output_stream(
                config,
                move |data: &mut [u16], _| {
                    fill_output(data, channels, device_rate, &playback, |value| {
                        ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
                    })
                },
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        other => Err(format!("unsupported output sample format: {other}")),
    }
}

fn fill_output<T: Copy>(
    data: &mut [T],
    device_channels: usize,
    device_rate: u32,
    shared: &PlaybackShared,
    convert: impl Fn(f32) -> T + Copy,
) {
    let silence = convert(0.0);
    if device_channels == 0 || !shared.playing.load(Ordering::Acquire) {
        data.fill(silence);
        return;
    }
    let Some(clip) = shared.clip.load_full() else {
        data.fill(silence);
        return;
    };
    let source_frames = clip.samples.len() / clip.channels.max(1);
    if source_frames == 0 {
        data.fill(silence);
        return;
    }
    let ratio = clip.sample_rate as f64 / device_rate.max(1) as f64;
    let mut position = f64::from_bits(shared.position_bits.load(Ordering::Acquire));
    for frame in data.chunks_mut(device_channels) {
        if position >= source_frames as f64 {
            if shared.looping.load(Ordering::Acquire) {
                position %= source_frames as f64;
            } else {
                shared.playing.store(false, Ordering::Release);
                frame.fill(silence);
                continue;
            }
        }
        let source_frame = position.floor() as usize;
        for (channel, output) in frame.iter_mut().enumerate() {
            let source_channel = channel.min(clip.channels.saturating_sub(1));
            let index = source_frame * clip.channels + source_channel;
            *output = convert(clip.samples.get(index).copied().unwrap_or(0.0));
        }
        position += ratio;
    }
    shared
        .position_bits
        .store(position.to_bits(), Ordering::Release);
}

fn build_input_stream(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    capture: Arc<CaptureShared>,
) -> Result<Stream, String> {
    let error = |error| eprintln!("Loom Studio input stream error: {error}");
    match format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| capture_input(data.iter().copied(), &capture),
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| {
                    capture_input(
                        data.iter().map(|value| *value as f32 / i16::MAX as f32),
                        &capture,
                    )
                },
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| {
                    capture_input(
                        data.iter()
                            .map(|value| *value as f32 / u16::MAX as f32 * 2.0 - 1.0),
                        &capture,
                    )
                },
                error,
                None,
            )
            .map_err(|error| error.to_string()),
        other => Err(format!("unsupported input sample format: {other}")),
    }
}

fn capture_input(samples: impl Iterator<Item = f32>, capture: &CaptureShared) {
    if !capture.recording.load(Ordering::Acquire) {
        return;
    }
    for sample in samples {
        if capture.samples.push(sample.clamp(-1.0, 1.0)).is_err() {
            capture.overruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}
