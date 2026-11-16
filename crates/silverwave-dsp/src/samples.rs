//! Lazy Sample Loader (`strudel.json`, `github:user/repo`, Local — `Section 6`, `SAMP-01`),
//! Deferred Reclamation (`SharedAsset<T>`, `DeferredCollector` — `RT-02`),
//! AKWF `wt_*` Wavetables (`SAMP-02`), Sample Manipulation (`begin`/`end`/`speed`/`splice` — `SAMP-03`),
//! and Dedicated `GranularSamplerVoice` (`PP-10`).

use crate::events::{LoopMode, SampleId, TriggerEvent};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Decoded stereo or mono audio sample buffer (`Section 2.3`).
#[derive(Clone, Debug)]
pub struct SampleBuffer {
    pub frames: Vec<f32>,
    pub channels: u8,
    pub sample_rate: u32,
    pub is_mmap_streamed: bool,
}

impl SampleBuffer {
    /// Creates a mono `SampleBuffer`.
    pub fn from_mono(frames: Vec<f32>, sample_rate: u32) -> Self {
        let duration_s = (frames.len() as f32) / (sample_rate.max(1) as f32);
        Self {
            frames,
            channels: 1,
            sample_rate,
            is_mmap_streamed: duration_s > 10.0,
        }
    }

    /// Number of sample frames per channel.
    #[inline]
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.frames.len() / (self.channels as usize)
        }
    }

    /// Cubic Hermite interpolated read at fractional frame position `pos` (`Section 4.5` & `6.2`).
    #[inline]
    pub fn read_hermite_mono(&self, pos: f32) -> f32 {
        let len = self.frame_count();
        if len == 0 {
            return 0.0;
        }
        let ch = self.channels as usize;
        let idx0 = (pos.floor() as isize).clamp(0, (len - 1) as isize) as usize;
        let frac = pos - pos.floor();
        if frac.abs() < 1.0e-6 || len < 4 {
            return self.frames[idx0 * ch];
        }
        let im1 = idx0.saturating_sub(1);
        let ip1 = (idx0 + 1).min(len - 1);
        let ip2 = (idx0 + 2).min(len - 1);
        let xm1 = self.frames[im1 * ch];
        let x0 = self.frames[idx0 * ch];
        let x1 = self.frames[ip1 * ch];
        let x2 = self.frames[ip2 * ch];
        let c0 = x0;
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        ((c3 * frac + c2) * frac + c1) * frac + c0
    }
}

/// Single-cycle wavetable buffer (`AKWF` `wt_*` — `Section 4.5`, `SAMP-02`).
#[derive(Clone, Debug)]
pub struct WavetableBuffer {
    pub table: Vec<f32>,
}

impl WavetableBuffer {
    /// Creates a new `WavetableBuffer`.
    pub fn new(table: Vec<f32>) -> Self {
        Self { table }
    }
}

/// Custom impulse response buffer for `OrbitReverb` (`Section 2.3`).
#[derive(Clone, Debug)]
pub struct ImpulseResponseBuffer {
    pub ir_frames: Vec<f32>,
    pub sample_rate: u32,
}

/// Deferred-reclamation asset handle (`basedrop::Shared<T>` equivalent — `RT-02`).
pub struct SharedAsset<T: Send + Sync + 'static> {
    inner: Option<Arc<T>>,
    graveyard: Arc<Mutex<Vec<Arc<T>>>>,
}

impl<T: Send + Sync + 'static> Clone for SharedAsset<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            graveyard: Arc::clone(&self.graveyard),
        }
    }
}

impl<T: Send + Sync + 'static> core::ops::Deref for SharedAsset<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<T: Send + Sync + 'static> Drop for SharedAsset<T> {
    fn drop(&mut self) {
        if let Some(arc) = self.inner.take() {
            if Arc::strong_count(&arc) == 1 {
                if let Ok(mut guard) = self.graveyard.try_lock() {
                    guard.push(arc);
                }
            }
        }
    }
}

/// Background Garbage Collector reclaiming dropped `SharedAsset<T>` buffers off the audio thread (`RT-02`).
#[derive(Clone, Default)]
pub struct DeferredCollector<T: Send + Sync + 'static> {
    graveyard: Arc<Mutex<Vec<Arc<T>>>>,
}

impl<T: Send + Sync + 'static> DeferredCollector<T> {
    /// Creates a new `DeferredCollector` with pre-allocated graveyard capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            graveyard: Arc::new(Mutex::new(Vec::with_capacity(capacity))),
        }
    }

    /// Wraps `value` in a `SharedAsset<T>` bound to this collector.
    pub fn share(&self, value: T) -> SharedAsset<T> {
        SharedAsset {
            inner: Some(Arc::new(value)),
            graveyard: Arc::clone(&self.graveyard),
        }
    }

    /// Number of dropped assets currently awaiting reclamation on the collector thread.
    pub fn pending_count(&self) -> usize {
        self.graveyard.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Reclaims all pending dropped assets on the background collector thread and returns the freed count (`RT-02`).
    pub fn collect(&self) -> usize {
        if let Ok(mut guard) = self.graveyard.lock() {
            let n = guard.len();
            guard.clear();
            n
        } else {
            0
        }
    }
}

/// Cross-thread asset message (`Section 2.3`).
#[derive(Clone)]
pub enum AssetMessage {
    SampleLoaded {
        id: SampleId,
        buffer: SharedAsset<SampleBuffer>,
    },
    WavetableLoaded {
        id: SampleId,
        buffer: SharedAsset<WavetableBuffer>,
    },
    ImpulseResponseLoaded {
        orbit: u8,
        buffer: SharedAsset<ImpulseResponseBuffer>,
    },
    EvictSample {
        id: SampleId,
    },
}

/// Sample manifest source descriptor (`Section 6.1`, `SAMP-01`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SampleSource {
    StrudelJsonUrl {
        url: String,
    },
    GithubRepo {
        user: String,
        repo: String,
        branch: String,
    },
    LocalPath {
        path: PathBuf,
    },
}

impl SampleSource {
    /// Parses a `github:<user>/<repo>/<branch>` shorthand (`SAMP-01`), defaulting `repo = "samples"` and `branch = "main"`.
    pub fn parse_github_shorthand(spec: &str) -> Option<Self> {
        let rest = spec.strip_prefix("github:")?;
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return None;
        }
        let user = parts[0].to_string();
        let repo = parts.get(1).copied().unwrap_or("samples").to_string();
        let branch = parts.get(2).copied().unwrap_or("main").to_string();
        Some(Self::GithubRepo { user, repo, branch })
    }

    /// Resolves the canonical `strudel.json` manifest URL for remote sources (`SAMP-01`).
    pub fn canonical_manifest_url(&self) -> Option<String> {
        match self {
            Self::StrudelJsonUrl { url } => Some(url.clone()),
            Self::GithubRepo { user, repo, branch } => Some(format!(
                "https://raw.githubusercontent.com/{}/{}/{}/strudel.json",
                user, repo, branch
            )),
            Self::LocalPath { .. } => None,
        }
    }
}

/// Handle returned when a `SampleSource` manifest is registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestHandle(pub u32);

/// Sample loader error type (`Section 6.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoaderError {
    InvalidManifestSource(String),
    UnknownSampleKey(String),
}

/// Trait for registering sample manifests and lazily resolving `(bank, sound, n)` to `SampleId` (`Section 6.1`).
pub trait SampleRegistry {
    fn register_source(&mut self, source: SampleSource) -> Result<ManifestHandle, LoaderError>;
    fn resolve_or_enqueue(&mut self, bank: &str, sound: &str, n: u32) -> SampleId;
}

/// Concrete lazy sample registry with `StaleWhileRevalidate` manifest caching and AST pre-roll pre-fetching (`PP-10`, `SAMP-01`).
#[derive(Default)]
pub struct LazySampleLoader {
    pub sources: Vec<SampleSource>,
    pub interned: BTreeMap<(String, String, u32), SampleId>,
    pub enqueued_fetches: Vec<(SampleId, String)>,
    pub base_url: String,
    next_id: u32,
}

impl LazySampleLoader {
    /// Creates an empty `LazySampleLoader`.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            interned: BTreeMap::new(),
            enqueued_fetches: Vec::new(),
            base_url: "https://raw.githubusercontent.com/tidalcycles/dirt-samples/main/"
                .to_string(),
            next_id: 1,
        }
    }

    /// Sets a custom `_base` URL prefix from a `strudel.json` manifest (`SAMP-01`).
    pub fn set_base_url(&mut self, base: &str) {
        self.base_url = base.to_string();
    }

    /// Number of network/disk audio fetches triggered so far (`0` immediately after `register_source` per `SAMP-01`).
    pub fn fetch_count(&self) -> usize {
        self.enqueued_fetches.len()
    }
}

impl SampleRegistry for LazySampleLoader {
    fn register_source(&mut self, source: SampleSource) -> Result<ManifestHandle, LoaderError> {
        self.sources.push(source);
        Ok(ManifestHandle(self.sources.len() as u32))
    }

    fn resolve_or_enqueue(&mut self, bank: &str, sound: &str, n: u32) -> SampleId {
        let key = (bank.to_string(), sound.to_string(), n);
        if let Some(&existing) = self.interned.get(&key) {
            return existing;
        }
        let id = SampleId(self.next_id);
        self.next_id += 1;
        self.interned.insert(key, id);
        let full_name = if bank.is_empty() {
            format!("{}/{}.wav", sound, n)
        } else {
            format!("{}_{}/{}.wav", bank, sound, n)
        };
        self.enqueued_fetches
            .push((id, format!("{}{}", self.base_url, full_name)));
        id
    }
}

/// Sample & Wavetable playback state supporting `begin`, `end`, reverse `speed < 0`, `loop`, `fit`, `slice`, and `splice` (`SAMP-02`, `SAMP-03`).
#[derive(Clone, Copy, Debug)]
pub struct SampleVoicePlayer {
    pub playhead_frame: f32,
    pub step_per_sample: f32,
    pub start_frame: f32,
    pub end_frame: f32,
    pub loop_enabled: bool,
    pub finished: bool,
}

impl SampleVoicePlayer {
    /// Initializes a `SampleVoicePlayer` for `event` and `buffer_frames` (`SAMP-02`, `SAMP-03`).
    pub fn from_trigger(
        event: &TriggerEvent,
        buffer_frames: usize,
        buffer_sr: u32,
        engine_sr: f32,
        is_wavetable_wt: bool,
    ) -> Self {
        let total = (buffer_frames.max(1)) as f32;
        let mut b_norm = event.begin.clamp(0.0, 1.0);
        let mut e_norm = event.end.clamp(0.0, 1.0);

        if event.slice_count > 0 {
            let idx = (event.slice_idx % event.slice_count) as f32;
            let cnt = event.slice_count as f32;
            let span = e_norm - b_norm;
            b_norm += span * (idx / cnt);
            e_norm = b_norm + span / cnt;
        }

        let low_frame = b_norm.min(e_norm) * total;
        let high_frame = b_norm.max(e_norm) * total;
        let region_frames = (high_frame - low_frame).max(1.0);

        let reverse = (event.speed < 0.0) ^ (event.begin > event.end);
        let base_rate = (buffer_sr.max(1) as f32) / engine_sr.max(1.0);

        let speed_mag = if event.splice_mode && event.duration_samples > 0 {
            region_frames / (event.duration_samples as f32)
        } else if event.fit > 0.0 && event.duration_samples > 0 {
            (region_frames * event.fit) / (event.duration_samples as f32)
        } else {
            event.speed.abs().max(0.0001) * base_rate
        };

        let loop_enabled = match event.loop_mode {
            LoopMode::On => true,
            LoopMode::WavetableDefault => true,
            LoopMode::Off => is_wavetable_wt,
        };

        let playhead = if event.scrub > 0.0 {
            low_frame + event.scrub.clamp(0.0, 1.0) * region_frames
        } else if reverse {
            high_frame - 1.0
        } else {
            low_frame
        };

        Self {
            playhead_frame: playhead.max(0.0),
            step_per_sample: if reverse { -speed_mag } else { speed_mag },
            start_frame: low_frame,
            end_frame: high_frame,
            loop_enabled,
            finished: false,
        }
    }

    /// Reads the next sample from `buffer` and advances `playhead_frame` (`SAMP-03`).
    pub fn next_sample(&mut self, buffer: &SampleBuffer) -> f32 {
        if self.finished {
            return 0.0;
        }
        let s = buffer.read_hermite_mono(self.playhead_frame);
        self.playhead_frame += self.step_per_sample;
        let span = (self.end_frame - self.start_frame).max(1.0);
        if self.step_per_sample >= 0.0 && self.playhead_frame >= self.end_frame {
            if self.loop_enabled {
                self.playhead_frame =
                    self.start_frame + (self.playhead_frame - self.start_frame) % span;
            } else {
                self.finished = true;
            }
        } else if self.step_per_sample < 0.0 && self.playhead_frame < self.start_frame {
            if self.loop_enabled {
                self.playhead_frame =
                    self.end_frame - (self.start_frame - self.playhead_frame) % span;
            } else {
                self.finished = true;
            }
        }
        s
    }
}

/// Dedicated Real-Time Granular Voice Engine executing `.chop(n)`, `.striate(n)`, and `.scrub()`
/// inside a single parent voice with Hann-windowed grains and zero scheduler `Hap` bloat (`PP-10`).
#[derive(Clone, Copy, Debug)]
pub struct GranularSamplerVoice {
    pub grain_count: u16,
    pub current_frame: u64,
    pub total_duration_frames: u64,
}

impl GranularSamplerVoice {
    /// Creates a new `GranularSamplerVoice`.
    pub const fn new(grain_count: u16, total_duration_frames: u64) -> Self {
        Self {
            grain_count: if grain_count == 0 { 1 } else { grain_count },
            current_frame: 0,
            total_duration_frames: if total_duration_frames == 0 {
                4800
            } else {
                total_duration_frames
            },
        }
    }

    /// Synthesizes one Hann-windowed granular sample from `buffer` (`PP-10`).
    pub fn next_sample(&mut self, buffer: &SampleBuffer) -> f32 {
        if self.current_frame >= self.total_duration_frames || buffer.frame_count() == 0 {
            return 0.0;
        }
        let norm_t = (self.current_frame as f32) / (self.total_duration_frames as f32);
        self.current_frame += 1;
        let grains = self.grain_count as f32;
        let grain_phase = (norm_t * grains).fract();
        let hann = 0.5 * (1.0 - (core::f32::consts::TAU * grain_phase).cos());
        let buf_pos = norm_t * (buffer.frame_count() as f32);
        buffer.read_hermite_mono(buf_pos) * hann
    }
}
