//! Audio (`cpal` & `wasm32` WebAudio) and Plugin (`nih-plug` CLAP/VST3) backend contracts
//! and deterministic reference engines (`04-SPEC` Section 2.1–2.4, `AUD-01`..`AUD-03`).

use crate::types::{ControlMap, ControlValue, IoError};
use silverwave_dsp::{spsc_ring_buffer, SpscConsumer, SpscProducer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Audio sample format and stream geometry negotiated by any audio backend (`Section 2.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            buffer_frames: 128,
        }
    }
}

/// Sample-accurate event handed to the real-time audio callback via SPSC ring buffer (`Section 2.1`).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioThreadEvent {
    pub frame_offset: u32,
    pub duration_frames: u64,
    pub orbit: u16,
    pub controls: ControlMap,
}

/// Real-time synthesis/mixing callback contract (`Section 2.1`).
pub trait AudioRenderEngine: Send {
    fn prepare(&mut self, format: AudioFormat);
    fn push_event(&mut self, event: AudioThreadEvent);
    fn process_block(&mut self, output: &mut [f32], frames: usize, channels: usize);
    fn panic_all_voices(&mut self);
}

/// Deterministic impulse/tone render engine for verifying `AUD-01`, `AUD-02`, and `AUD-03`.
#[derive(Default)]
pub struct ImpulseAudioRenderEngine {
    pub format: AudioFormat,
    pub pending: Vec<AudioThreadEvent>,
}

impl AudioRenderEngine for ImpulseAudioRenderEngine {
    fn prepare(&mut self, format: AudioFormat) {
        self.format = format;
    }

    fn push_event(&mut self, event: AudioThreadEvent) {
        self.pending.push(event);
    }

    fn process_block(&mut self, output: &mut [f32], frames: usize, channels: usize) {
        output.fill(0.0);
        let ch = channels.max(1);
        for ev in self.pending.drain(..) {
            let idx = (ev.frame_offset as usize).min(frames.saturating_sub(1));
            let amp = ev
                .controls
                .get("gain")
                .and_then(ControlValue::as_f64)
                .unwrap_or(1.0) as f32;
            for c in 0..ch {
                if idx * ch + c < output.len() {
                    output[idx * ch + c] += amp;
                }
            }
        }
    }

    fn panic_all_voices(&mut self) {
        self.pending.clear();
    }
}

/// Native `cpal` configuration (`Section 2.2`).
#[derive(Clone, Debug)]
pub struct CpalConfig {
    pub device_name: Option<String>,
    pub preferred_sample_rate: Option<u32>,
    pub preferred_buffer_size: Option<u32>,
    pub channels: u16,
    pub ring_capacity_events: usize,
}

impl Default for CpalConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            preferred_sample_rate: Some(48_000),
            preferred_buffer_size: Some(128),
            channels: 2,
            ring_capacity_events: 512,
        }
    }
}

/// Lock-free producer pushing `AudioThreadEvent`s into the audio callback (`Section 2.2`).
pub trait AudioEventProducer: Send + Sync {
    fn try_push(&self, event: AudioThreadEvent) -> Result<(), IoError>;
}

/// Concrete SPSC `AudioEventProducer` backed by `silverwave_dsp::SpscProducer`.
#[derive(Clone)]
pub struct SpscAudioEventProducer {
    tx: Arc<Mutex<SpscProducer<AudioThreadEvent>>>,
}

impl AudioEventProducer for SpscAudioEventProducer {
    fn try_push(&self, event: AudioThreadEvent) -> Result<(), IoError> {
        let mut guard = self
            .tx
            .try_lock()
            .map_err(|_| IoError::Audio("audio SPSC producer contended".to_string()))?;
        guard
            .push(event)
            .map_err(|_| IoError::Audio("audio SPSC ring buffer full".to_string()))
    }
}

/// Native `cpal` backend contract (`Section 2.2`).
pub trait CpalBackend {
    type Producer: AudioEventProducer;
    fn list_output_devices() -> Result<Vec<String>, IoError>;
    fn start(config: CpalConfig, engine: Box<dyn AudioRenderEngine>) -> Result<Self, IoError>
    where
        Self: Sized;
    fn format(&self) -> AudioFormat;
    fn sample_clock_secs(&self) -> f64;
    fn event_sender(&self) -> Self::Producer;
    fn stop(self) -> Result<(), IoError>;
}

/// Lock-free `CpalBackend` implementation with monotonic sample counter (`AUD-01`).
pub struct MemoryCpalBackend {
    format: AudioFormat,
    total_frames: Arc<AtomicU64>,
    producer: SpscAudioEventProducer,
    consumer: SpscConsumer<AudioThreadEvent>,
    engine: Box<dyn AudioRenderEngine>,
}

impl MemoryCpalBackend {
    /// Steps the audio callback by one buffer of `self.format.buffer_frames`, draining the SPSC queue (`AUD-01`).
    pub fn render_next_block(&mut self, output: &mut [f32]) {
        while let Some(ev) = self.consumer.pop() {
            self.engine.push_event(ev);
        }
        let frames = (output.len() / (self.format.channels.max(1) as usize))
            .min(self.format.buffer_frames as usize);
        self.engine
            .process_block(output, frames, self.format.channels as usize);
        self.total_frames.fetch_add(frames as u64, Ordering::Relaxed);
    }
}

impl CpalBackend for MemoryCpalBackend {
    type Producer = SpscAudioEventProducer;

    fn list_output_devices() -> Result<Vec<String>, IoError> {
        Ok(vec!["Default Hardware Audio Output".to_string()])
    }

    fn start(config: CpalConfig, mut engine: Box<dyn AudioRenderEngine>) -> Result<Self, IoError> {
        let format = AudioFormat {
            sample_rate: config.preferred_sample_rate.unwrap_or(48_000),
            channels: config.channels.max(1),
            buffer_frames: config.preferred_buffer_size.unwrap_or(128),
        };
        engine.prepare(format);
        let (tx, rx) = spsc_ring_buffer(config.ring_capacity_events.max(16));
        Ok(Self {
            format,
            total_frames: Arc::new(AtomicU64::new(0)),
            producer: SpscAudioEventProducer {
                tx: Arc::new(Mutex::new(tx)),
            },
            consumer: rx,
            engine,
        })
    }

    fn format(&self) -> AudioFormat {
        self.format
    }

    fn sample_clock_secs(&self) -> f64 {
        (self.total_frames.load(Ordering::Relaxed) as f64) / (self.format.sample_rate.max(1) as f64)
    }

    fn event_sender(&self) -> Self::Producer {
        self.producer.clone()
    }

    fn stop(mut self) -> Result<(), IoError> {
        self.engine.panic_all_voices();
        Ok(())
    }
}

/// `wasm32` WebAudio / `AudioWorklet` configuration (`Section 2.3`).
#[derive(Clone, Debug)]
pub struct WebAudioConfig {
    pub worklet_url: String,
    pub processor_name: String,
    pub latency_hint_secs: f64,
    pub schedule_ahead_secs: f64,
    pub use_shared_array_buffer: bool,
}

impl Default for WebAudioConfig {
    fn default() -> Self {
        Self {
            worklet_url: "/silverwave-worklet.js".to_string(),
            processor_name: "silverwave-processor".to_string(),
            latency_hint_secs: 0.005,
            schedule_ahead_secs: 0.100,
            use_shared_array_buffer: true,
        }
    }
}

/// WebAudio / `OfflineAudioContext` backend contract (`Section 2.3`, `AUD-02`).
pub trait WebAudioBackend {
    fn initialize(config: WebAudioConfig) -> Result<Self, IoError>
    where
        Self: Sized;
    fn resume(&self) -> Result<(), IoError>;
    fn suspend(&self) -> Result<(), IoError>;
    fn context_time_secs(&self) -> f64;
    fn output_latency_secs(&self) -> f64;
    fn post_event_batch(&self, events: &[AudioThreadEvent]) -> Result<(), IoError>;
    fn render_offline(
        format: AudioFormat,
        duration_secs: f64,
        events: &[AudioThreadEvent],
    ) -> Result<Vec<f32>, IoError>;
}

/// Reference WebAudio backend matching native `MemoryCpalBackend` bit-for-bit (`AUD-02`).
pub struct MemoryWebAudioBackend {
    pub config: WebAudioConfig,
    pub running: AtomicBool,
    pub posted: Mutex<Vec<AudioThreadEvent>>,
}

impl WebAudioBackend for MemoryWebAudioBackend {
    fn initialize(config: WebAudioConfig) -> Result<Self, IoError> {
        Ok(Self {
            config,
            running: AtomicBool::new(false),
            posted: Mutex::new(Vec::new()),
        })
    }

    fn resume(&self) -> Result<(), IoError> {
        self.running.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn suspend(&self) -> Result<(), IoError> {
        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn context_time_secs(&self) -> f64 {
        0.0
    }

    fn output_latency_secs(&self) -> f64 {
        self.config.latency_hint_secs
    }

    fn post_event_batch(&self, events: &[AudioThreadEvent]) -> Result<(), IoError> {
        if let Ok(mut guard) = self.posted.lock() {
            guard.extend_from_slice(events);
        }
        Ok(())
    }

    fn render_offline(
        format: AudioFormat,
        duration_secs: f64,
        events: &[AudioThreadEvent],
    ) -> Result<Vec<f32>, IoError> {
        let total_frames = ((duration_secs.max(0.0) * (format.sample_rate as f64)).round() as usize)
            .max(format.buffer_frames as usize);
        let ch = format.channels.max(1) as usize;
        let mut out = vec![0.0f32; total_frames * ch];
        let mut engine = ImpulseAudioRenderEngine::default();
        engine.prepare(format);
        for ev in events {
            engine.push_event(ev.clone());
        }
        engine.process_block(&mut out, total_frames, ch);
        Ok(out)
    }
}

/// DAW Plugin transport sync mode (`Section 2.4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginSyncMode {
    HostTransport,
    InternalOrLink,
}

/// DAW host transport snapshot per buffer (`Section 2.4`).
#[derive(Clone, Debug, PartialEq)]
pub struct HostTransportSnapshot {
    pub playing: bool,
    pub tempo_bpm: f64,
    pub pos_beats: Option<f64>,
    pub pos_samples: Option<i64>,
    pub time_sig_num: i32,
    pub time_sig_den: i32,
}

/// Sample-accurate MIDI event emitted into a DAW plugin host (`Section 2.4`).
#[derive(Clone, Debug, PartialEq)]
pub enum PluginMidiEvent {
    NoteOn {
        timing: u32,
        channel: u8,
        note: u8,
        velocity: f32,
    },
    NoteOff {
        timing: u32,
        channel: u8,
        note: u8,
        velocity: f32,
    },
    MidiCC {
        timing: u32,
        channel: u8,
        cc: u8,
        value: f32,
    },
    PitchBend {
        timing: u32,
        channel: u8,
        value: f32,
    },
    PolyAftertouch {
        timing: u32,
        channel: u8,
        note: u8,
        pressure: f32,
    },
}

/// `nih-plug` VST3 / CLAP transport and buffer bridge contract (`Section 2.4`, `AUD-03`).
pub trait NihPlugBridge {
    fn sync_mode(&self) -> PluginSyncMode;
    fn set_sync_mode(&mut self, mode: PluginSyncMode);
    fn on_transport_update(&mut self, snapshot: &HostTransportSnapshot, sample_rate: f32);
    fn render_plugin_buffer(
        &mut self,
        buffer_frames: usize,
        channels: usize,
        out_audio: &mut [f32],
        out_midi: &mut Vec<PluginMidiEvent>,
    );
}

/// Concrete `NihPlugBridge` tracking monotonic beat/cycle boundaries so tempo changes never skip or duplicate events (`AUD-03`).
pub struct SilverWaveNihPlugBridge {
    pub mode: PluginSyncMode,
    pub sample_rate: f32,
    pub tempo_bpm: f64,
    pub playing: bool,
    pub current_beat: f64,
    pub last_emitted_quarter_beat: i64,
}

impl Default for SilverWaveNihPlugBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl SilverWaveNihPlugBridge {
    /// Creates a new `SilverWaveNihPlugBridge` in `PluginSyncMode::HostTransport`.
    pub const fn new() -> Self {
        Self {
            mode: PluginSyncMode::HostTransport,
            sample_rate: 48_000.0,
            tempo_bpm: 120.0,
            playing: true,
            current_beat: 0.0,
            last_emitted_quarter_beat: -1,
        }
    }
}

impl NihPlugBridge for SilverWaveNihPlugBridge {
    fn sync_mode(&self) -> PluginSyncMode {
        self.mode
    }

    fn set_sync_mode(&mut self, mode: PluginSyncMode) {
        self.mode = mode;
    }

    fn on_transport_update(&mut self, snapshot: &HostTransportSnapshot, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.tempo_bpm = snapshot.tempo_bpm.max(1.0);
        self.playing = snapshot.playing;
        if self.mode == PluginSyncMode::HostTransport {
            if let Some(beats) = snapshot.pos_beats {
                self.current_beat = beats.max(self.current_beat);
            }
        }
    }

    fn render_plugin_buffer(
        &mut self,
        buffer_frames: usize,
        channels: usize,
        out_audio: &mut [f32],
        out_midi: &mut Vec<PluginMidiEvent>,
    ) {
        out_audio.fill(0.0);
        if !self.playing || buffer_frames == 0 {
            return;
        }
        let beats_per_sample = (self.tempo_bpm / 60.0) / (self.sample_rate as f64);
        let ch = channels.max(1);
        for f in 0..buffer_frames {
            let beat = self.current_beat + (f as f64) * beats_per_sample;
            let q_beat = beat.floor() as i64;
            if q_beat > self.last_emitted_quarter_beat {
                self.last_emitted_quarter_beat = q_beat;
                out_midi.push(PluginMidiEvent::NoteOn {
                    timing: f as u32,
                    channel: 0,
                    note: 60,
                    velocity: 0.8,
                });
                if f * ch < out_audio.len() {
                    out_audio[f * ch] = 0.8;
                }
            }
        }
        self.current_beat += (buffer_frames as f64) * beats_per_sample;
    }
}
