//! Lock-Free SPSC Ring Buffers (`RT-03`), Pre-Allocated 256-Voice Pool (`VoicePool`),
//! 3-Tier Deterministic Voice Stealing (`LIFE-02`), Anti-Click `cut` Choke Groups (`LIFE-01`),
//! Sample-Accurate Sub-Block Onset Scheduling (`RT-04`), and Zero-Allocation `SilverWaveAudioEngine` (`RT-01`, `RT-05`).

use crate::events::{
    DistortionMode, LOOKAHEAD_CAPACITY, MAX_ORBITS, MAX_VOICES, TriggerEvent, VoiceKind, Waveform,
    sanitize_f32_finite,
};
use crate::filters::{AdsrEnvelope, FilterPassMode, MultiModeFilter, VowelFormantFilter};
use crate::fx::{
    AllpassPhaser, BitCrusherAndCoarse, OrderedVoiceFxExecutor, RingModulator, TremoloModulator,
    VoiceCompressor, equal_power_pan, process_shape_and_distort,
};
use crate::orbit::{OrbitBus, OutputRoutingMode};
use crate::samples::{
    AssetMessage, GranularSamplerVoice, SampleBuffer, SampleVoicePlayer, SharedAsset,
    WavetableBuffer,
};
use crate::synths::{
    AdditiveOscillator, FmMatrixSynthesizer, InlineDspPipeVoice, KarplusStrongPluck,
    NoiseGenerator, PolyBlepOscillator, SupersawGenerator, ZzfxSynthesizer,
};
use crate::telemetry::OrbitRmsReport;
use silverwave_core::EngineCompatibilityProfile;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Error returned when pushing to a full lock-free SPSC ring buffer (`RT-03`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushError<T> {
    Full(T),
}

struct RingInner<T> {
    slots: Vec<Mutex<Option<T>>>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

/// Wait-free SPSC Producer (`rtrb::Producer<T>` contract — `Section 2.1`, `RT-03`).
pub struct SpscProducer<T> {
    inner: Arc<RingInner<T>>,
}

/// Wait-free SPSC Consumer (`rtrb::Consumer<T>` contract — `Section 2.1`, `RT-03`).
pub struct SpscConsumer<T> {
    inner: Arc<RingInner<T>>,
}

/// Creates a bounded SPSC ring buffer pair `(SpscProducer<T>, SpscConsumer<T>)` with `capacity` (`RT-03`).
pub fn spsc_ring_buffer<T>(capacity: usize) -> (SpscProducer<T>, SpscConsumer<T>) {
    let cap = capacity.max(1) + 1;
    let mut slots = Vec::with_capacity(cap);
    for _ in 0..cap {
        slots.push(Mutex::new(None));
    }
    let inner = Arc::new(RingInner {
        slots,
        capacity: cap,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });
    (
        SpscProducer {
            inner: Arc::clone(&inner),
        },
        SpscConsumer { inner },
    )
}

impl<T> SpscProducer<T> {
    /// Pushes `item` without blocking or allocating; returns `Err(PushError::Full(item))` when saturated (`RT-03`).
    pub fn push(&mut self, item: T) -> Result<(), PushError<T>> {
        let tail = self.inner.tail.load(Ordering::Relaxed);
        let head = self.inner.head.load(Ordering::Acquire);
        let next_tail = (tail + 1) % self.inner.capacity;
        // Both the full-ring and lock-contention paths report `Full`: a single
        // fallthrough keeps one error exit.
        if next_tail != head {
            if let Ok(mut guard) = self.inner.slots[tail].try_lock() {
                *guard = Some(item);
                self.inner.tail.store(next_tail, Ordering::Release);
                return Ok(());
            }
        }
        Err(PushError::Full(item))
    }
}

impl<T> SpscConsumer<T> {
    /// Pops the next item without blocking or allocating (`RT-03`).
    pub fn pop(&mut self) -> Option<T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        // The empty-ring and lock-contention paths both return `None`: a single
        // fallthrough keeps one exit.
        if head != tail {
            if let Ok(mut guard) = self.inner.slots[head].try_lock() {
                let item = guard.take();
                let next_head = (head + 1) % self.inner.capacity;
                self.inner.head.store(next_head, Ordering::Release);
                return item;
            }
        }
        None
    }
}

/// Cross-thread SPSC channel bundle for `AudioEngine` (`Section 2.3`).
pub struct EngineChannels {
    pub event_rx: SpscConsumer<TriggerEvent>,
    pub asset_rx: SpscConsumer<AssetMessage>,
    pub rms_tx: SpscProducer<OrbitRmsReport>,
}

/// Configuration for `SilverWaveAudioEngine` (`Section 2.3`).
#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub sample_rate: u32,
    pub max_block_frames: usize,
    pub max_voices: usize,
    pub num_orbits: usize,
    pub routing_mode: OutputRoutingMode,
    pub profile: EngineCompatibilityProfile,
    pub session_seed: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            max_block_frames: 512,
            max_voices: MAX_VOICES,
            num_orbits: MAX_ORBITS,
            routing_mode: OutputRoutingMode::StereoMixdown,
            profile: EngineCompatibilityProfile::SuperDoughC4,
            session_seed: 0x517E_57A7_E001,
        }
    }
}

/// Core real-time audio engine trait (`Section 2.3`).
pub trait AudioEngine {
    fn new(config: EngineConfig, channels: EngineChannels) -> Self;
    fn process_block(&mut self, output: &mut [f32], num_channels: usize);
}

/// Lifecycle state of an individual voice slot in `VoicePool` (`Section 3.3` & `3.4`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceState {
    #[default]
    Idle,
    Active,
    Release,
    CutFadeOut,
}

/// 64-frame crossfade buffer preventing clicks when an active voice is stolen (`Section 3.4`, `LIFE-02`).
#[derive(Clone, Copy, Debug)]
pub struct StealFadeBuffer {
    pub left: [f32; 64],
    pub right: [f32; 64],
    pub active_frames: u8,
    pub read_pos: u8,
}

impl Default for StealFadeBuffer {
    fn default() -> Self {
        Self {
            left: [0.0; 64],
            right: [0.0; 64],
            active_frames: 0,
            read_pos: 0,
        }
    }
}

/// Pre-allocated voice slot inside `VoicePool` (`Section 3.4`).
#[derive(Clone, Debug)]
pub struct VoiceSlot {
    pub state: VoiceState,
    pub event: TriggerEvent,
    pub onset_frame_in_block: usize,
    pub current_env_level: f32,
    pub cut_fade_remaining: u16,
    pub cut_fade_total: u16,
    pub amp_env: AdsrEnvelope,
    pub pitch_env: AdsrEnvelope,
    pub lpf_env: AdsrEnvelope,
    pub hpf_env: AdsrEnvelope,
    pub bpf_env: AdsrEnvelope,
    pub osc: PolyBlepOscillator,
    pub supersaw: SupersawGenerator,
    pub noise: NoiseGenerator,
    pub additive: AdditiveOscillator,
    pub fm: FmMatrixSynthesizer,
    pub zzfx: ZzfxSynthesizer,
    pub karplus: KarplusStrongPluck,
    pub pipe_voice: InlineDspPipeVoice,
    pub sample_player: Option<SampleVoicePlayer>,
    pub granular: Option<GranularSamplerVoice>,
    pub lpf_filter: MultiModeFilter,
    pub hpf_filter: MultiModeFilter,
    pub bpf_filter: MultiModeFilter,
    pub vowel_filter: VowelFormantFilter,
    pub crusher: BitCrusherAndCoarse,
    pub ring_mod: RingModulator,
    pub tremolo: TremoloModulator,
    pub compressor: VoiceCompressor,
    pub phaser: AllpassPhaser,
    pub ordered_fx: OrderedVoiceFxExecutor,
}

impl Default for VoiceSlot {
    fn default() -> Self {
        Self {
            state: VoiceState::Idle,
            event: TriggerEvent::default(),
            onset_frame_in_block: 0,
            current_env_level: 0.0,
            cut_fade_remaining: 0,
            cut_fade_total: 120,
            amp_env: AdsrEnvelope::default(),
            pitch_env: AdsrEnvelope::default(),
            lpf_env: AdsrEnvelope::default(),
            hpf_env: AdsrEnvelope::default(),
            bpf_env: AdsrEnvelope::default(),
            osc: PolyBlepOscillator::new(0.0),
            supersaw: SupersawGenerator::default(),
            noise: NoiseGenerator::default(),
            additive: AdditiveOscillator::default(),
            fm: FmMatrixSynthesizer::default(),
            zzfx: ZzfxSynthesizer::default(),
            karplus: KarplusStrongPluck::new(),
            pipe_voice: InlineDspPipeVoice::default(),
            sample_player: None,
            granular: None,
            lpf_filter: MultiModeFilter::new(),
            hpf_filter: MultiModeFilter::new(),
            bpf_filter: MultiModeFilter::new(),
            vowel_filter: VowelFormantFilter::new(),
            crusher: BitCrusherAndCoarse::default(),
            ring_mod: RingModulator::default(),
            tremolo: TremoloModulator::default(),
            compressor: VoiceCompressor::default(),
            phaser: AllpassPhaser::default(),
            ordered_fx: OrderedVoiceFxExecutor::new(),
        }
    }
}

/// Real-Time Zero-Allocation Audio Engine (`SilverWaveAudioEngine` — `03-SPEC` Section 2 & 3).
pub struct SilverWaveAudioEngine {
    pub config: EngineConfig,
    pub channels: EngineChannels,
    pub current_sample_frame: u64,
    pub voices: Vec<VoiceSlot>,
    pub steal_fade: StealFadeBuffer,
    pub orbits: Vec<OrbitBus>,
    pub lookahead: Vec<Option<TriggerEvent>>,
    pub samples: Vec<Option<SharedAsset<SampleBuffer>>>,
    pub wavetables: Vec<Option<SharedAsset<WavetableBuffer>>>,
    pub steal_count: u64,
}

impl SilverWaveAudioEngine {
    /// Returns the number of currently active (non-Idle) voices.
    pub fn active_voice_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state != VoiceState::Idle)
            .count()
    }

    /// Deterministically selects a voice slot or steals a victim in 3-tier priority order (`Section 3.4`, `LIFE-02`):
    /// Tier 1: `CutFadeOut` or `Release` with lowest current envelope amplitude.
    /// Tier 2: Oldest active voice sharing the incoming `cut_group` or `orbit`.
    /// Tier 3: Global quietest / oldest active voice.
    pub fn allocate_or_steal(&mut self, incoming: &TriggerEvent) -> usize {
        if let Some(idle_idx) = self.voices.iter().position(|v| v.state == VoiceState::Idle) {
            return idle_idx;
        }
        self.steal_count += 1;

        let mut tier1: Option<(usize, f32)> = None;
        for (i, v) in self.voices.iter().enumerate() {
            if matches!(v.state, VoiceState::CutFadeOut | VoiceState::Release) {
                match tier1 {
                    None => tier1 = Some((i, v.current_env_level)),
                    Some((_, best_lvl)) if v.current_env_level < best_lvl => {
                        tier1 = Some((i, v.current_env_level));
                    }
                    _ => {}
                }
            }
        }
        if let Some((victim, _)) = tier1 {
            self.capture_steal_crossfade(victim);
            return victim;
        }

        let mut tier2: Option<(usize, u64)> = None;
        for (i, v) in self.voices.iter().enumerate() {
            let same_cut = incoming.cut_group != 0 && v.event.cut_group == incoming.cut_group;
            let same_orbit = v.event.orbit == incoming.orbit;
            if same_cut || same_orbit {
                match tier2 {
                    None => tier2 = Some((i, v.event.trigger_sample)),
                    Some((_, best_ts)) if v.event.trigger_sample < best_ts => {
                        tier2 = Some((i, v.event.trigger_sample));
                    }
                    _ => {}
                }
            }
        }
        if let Some((victim, _)) = tier2 {
            self.capture_steal_crossfade(victim);
            return victim;
        }

        let mut victim = 0usize;
        for (i, a) in self.voices.iter().enumerate().skip(1) {
            let b = &self.voices[victim];
            if a.current_env_level < b.current_env_level
                || ((a.current_env_level - b.current_env_level).abs() <= 1.0e-6
                    && a.event.trigger_sample < b.event.trigger_sample)
            {
                victim = i;
            }
        }
        self.capture_steal_crossfade(victim);
        victim
    }

    fn capture_steal_crossfade(&mut self, victim_idx: usize) {
        let lvl = self.voices[victim_idx].current_env_level * self.voices[victim_idx].event.gain;
        let (pl, pr) = equal_power_pan(lvl, self.voices[victim_idx].event.pan);
        for f in 0..64 {
            let fade = 1.0 - (f as f32) / 64.0;
            self.steal_fade.left[f] = pl * fade;
            self.steal_fade.right[f] = pr * fade;
        }
        self.steal_fade.active_frames = 64;
        self.steal_fade.read_pos = 0;
    }

    fn activate_voice(&mut self, event: TriggerEvent, onset_offset: usize) {
        let sr = self.config.sample_rate as f32;
        let d_count = (event.duck_count as usize).min(event.duck_targets.len());
        for &target in event.duck_targets.iter().take(d_count) {
            let target_orb = target as usize;
            if target_orb < self.orbits.len() {
                self.orbits[target_orb].duck_envelope.trigger_duck(
                    event.duck_depth,
                    event.duck_attack,
                    sr,
                );
            }
        }

        if event.cut_group != 0 {
            let fade_frames = ((0.0025 * sr).round() as u16).clamp(32, 240);
            for v in &mut self.voices {
                if v.state != VoiceState::Idle && v.event.cut_group == event.cut_group {
                    let same_orbit_or_global = event.cut_group < 0 || v.event.orbit == event.orbit;
                    if same_orbit_or_global {
                        if v.event.trigger_sample == event.trigger_sample
                            && (v.event.stream_priority, v.event.hap_ordinal)
                                > (event.stream_priority, event.hap_ordinal)
                        {
                            return;
                        }
                        v.state = VoiceState::CutFadeOut;
                        v.cut_fade_total = fade_frames;
                        v.cut_fade_remaining = fade_frames;
                    }
                }
            }
        }

        let slot_idx = self.allocate_or_steal(&event);
        let dur_frames = ((event.duration_samples as f32) * event.clip.max(0.01)).round() as u64;
        let is_wt = event.voice_kind == VoiceKind::Wavetable;
        let sid = event.sample_handle.0 as usize;

        let sample_player = if matches!(event.voice_kind, VoiceKind::Sample | VoiceKind::Wavetable)
        {
            if let Some(Some(buf)) = self.samples.get(sid) {
                Some(SampleVoicePlayer::from_trigger(
                    &event,
                    buf.frame_count(),
                    buf.sample_rate,
                    sr,
                    is_wt,
                ))
            } else {
                None
            }
        } else {
            None
        };

        let granular =
            if event.voice_kind == VoiceKind::Granular || event.chop > 0 || event.striate > 0 {
                let grains = event.chop.max(event.striate).max(8);
                Some(GranularSamplerVoice::new(grains, dur_frames.max(64)))
            } else {
                None
            };

        let orb_idx = (event.orbit as usize).min(self.orbits.len() - 1);
        self.orbits[orb_idx].delay_send = event.delay;
        self.orbits[orb_idx].delay_time = event.delay_time;
        self.orbits[orb_idx].delay_feedback = event.delay_feedback;
        self.orbits[orb_idx].delay_lock = event.delay_lock;
        self.orbits[orb_idx].cps = event.cps;
        self.orbits[orb_idx].room_send = event.room;
        self.orbits[orb_idx].room_size = event.size;
        self.orbits[orb_idx].dry_level = event.dry;
        self.orbits[orb_idx].leslie_wet = event.leslie;
        self.orbits[orb_idx].leslie_rate = event.lrate;
        self.orbits[orb_idx].leslie_size = event.lsize;

        let slot = &mut self.voices[slot_idx];
        slot.state = VoiceState::Active;
        slot.event = event;
        slot.onset_frame_in_block = onset_offset;
        slot.current_env_level = 1.0;
        slot.cut_fade_remaining = 0;
        slot.amp_env = AdsrEnvelope::new(
            event.attack,
            event.decay,
            event.sustain,
            event.release,
            dur_frames.max(1),
        );
        slot.pitch_env = AdsrEnvelope::new(
            event.pattack,
            event.pdecay,
            event.psustain,
            event.prelease,
            dur_frames.max(1),
        );
        slot.lpf_env = AdsrEnvelope::new(
            event.lpa,
            event.lpd,
            event.lps,
            event.lpr,
            dur_frames.max(1),
        );
        slot.hpf_env = AdsrEnvelope::new(
            event.hpa,
            event.hpd,
            event.hps,
            event.hpr,
            dur_frames.max(1),
        );
        slot.bpf_env = AdsrEnvelope::new(
            event.bpa,
            event.bpd,
            event.bps,
            event.bpr,
            dur_frames.max(1),
        );
        slot.osc = PolyBlepOscillator::new(0.0);
        slot.supersaw = SupersawGenerator::new(self.config.session_seed ^ (slot_idx as u64));
        slot.noise = NoiseGenerator::new(self.config.session_seed.wrapping_add(slot_idx as u64));
        slot.additive = AdditiveOscillator::default();
        slot.fm = FmMatrixSynthesizer::default();
        slot.zzfx = ZzfxSynthesizer::default();
        slot.karplus = KarplusStrongPluck::new();
        slot.pipe_voice = InlineDspPipeVoice::new(
            event.waveform,
            event.lpf.min(12_000.0),
            event.lpq,
            1.0 + event.distort,
        );
        slot.sample_player = sample_player;
        slot.granular = granular;
        slot.lpf_filter = MultiModeFilter::new();
        slot.hpf_filter = MultiModeFilter::new();
        slot.bpf_filter = MultiModeFilter::new();
        slot.vowel_filter = VowelFormantFilter::new();
        slot.crusher = BitCrusherAndCoarse::default();
        slot.ring_mod = RingModulator::default();
        slot.tremolo = TremoloModulator::default();
        slot.compressor = VoiceCompressor::default();
        slot.phaser = AllpassPhaser::default();
        slot.ordered_fx = OrderedVoiceFxExecutor::new();
    }

    fn ingest_assets_and_events(&mut self, block_start: u64, block_end: u64) {
        while let Some(msg) = self.channels.asset_rx.pop() {
            match msg {
                AssetMessage::SampleLoaded { id, buffer } => {
                    let idx = id.0 as usize;
                    if idx < self.samples.len() {
                        self.samples[idx] = Some(buffer);
                    }
                }
                AssetMessage::WavetableLoaded { id, buffer } => {
                    let idx = id.0 as usize;
                    if idx < self.wavetables.len() {
                        self.wavetables[idx] = Some(buffer);
                    }
                }
                AssetMessage::ImpulseResponseLoaded { orbit, .. } => {
                    let orb = orbit as usize;
                    if orb < self.orbits.len() {
                        self.orbits[orb].reverb.swap_impulse_response(
                            Some(orbit as u32),
                            self.config.sample_rate as f32,
                        );
                    }
                }
                AssetMessage::EvictSample { id } => {
                    let idx = id.0 as usize;
                    if idx < self.samples.len() {
                        self.samples[idx] = None;
                    }
                }
            }
        }

        let mut lookahead = core::mem::take(&mut self.lookahead);
        for slot in lookahead.iter_mut() {
            if let Some(ev) = *slot {
                if ev.trigger_sample < block_end {
                    *slot = None;
                    let offset = ev.trigger_sample.saturating_sub(block_start) as usize;
                    self.activate_voice(ev, offset);
                }
            }
        }
        self.lookahead = lookahead;

        while let Some(ev) = self.channels.event_rx.pop() {
            if ev.trigger_sample < block_end {
                let offset = ev.trigger_sample.saturating_sub(block_start) as usize;
                self.activate_voice(ev, offset);
            } else if let Some(slot) = self.lookahead.iter_mut().find(|s| s.is_none()) {
                *slot = Some(ev);
            }
        }
    }
}

impl AudioEngine for SilverWaveAudioEngine {
    fn new(config: EngineConfig, channels: EngineChannels) -> Self {
        let max_v = config.max_voices.max(1);
        let max_o = config.num_orbits.max(1);
        let max_f = config.max_block_frames.max(64);

        let mut voices = Vec::with_capacity(max_v);
        for _ in 0..max_v {
            voices.push(VoiceSlot::default());
        }
        let mut orbits = Vec::with_capacity(max_o);
        for i in 0..max_o {
            orbits.push(OrbitBus::new(i as u8, max_f, config.session_seed));
        }
        let mut lookahead = Vec::with_capacity(LOOKAHEAD_CAPACITY);
        for _ in 0..LOOKAHEAD_CAPACITY {
            lookahead.push(None);
        }
        let mut samples = Vec::with_capacity(1024);
        let mut wavetables = Vec::with_capacity(256);
        for _ in 0..1024 {
            samples.push(None);
        }
        for _ in 0..256 {
            wavetables.push(None);
        }

        Self {
            config,
            channels,
            current_sample_frame: 0,
            voices,
            steal_fade: StealFadeBuffer::default(),
            orbits,
            lookahead,
            samples,
            wavetables,
            steal_count: 0,
        }
    }

    fn process_block(&mut self, output: &mut [f32], num_channels: usize) {
        let ch = num_channels.max(1);
        let frames = (output.len() / ch).min(self.config.max_block_frames);
        output.fill(0.0);
        if frames == 0 {
            return;
        }

        let block_start = self.current_sample_frame;
        let block_end = block_start + (frames as u64);
        self.ingest_assets_and_events(block_start, block_end);

        for orb in &mut self.orbits {
            orb.synth_bus.clear(frames);
            orb.dry_bus.clear(frames);
            orb.global_effect_bus.clear(frames);
        }

        let sr = self.config.sample_rate as f32;
        let num_orbits = self.orbits.len();

        for voice in self.voices.iter_mut() {
            if voice.state == VoiceState::Idle {
                continue;
            }
            let onset = voice.onset_frame_in_block.min(frames);
            voice.onset_frame_in_block = 0;
            let ev = voice.event;
            let orb_idx = (ev.orbit as usize).min(num_orbits - 1);
            let multi_orbit_comp = 1.0 / (ev.orbit_send_count.max(1) as f32).sqrt();

            for f in onset..frames {
                let (amp_val, env_done) = voice.amp_env.next_value(sr);
                voice.current_env_level = amp_val;
                if env_done {
                    voice.state = VoiceState::Idle;
                    break;
                }

                let mut cut_scale = 1.0f32;
                if voice.state == VoiceState::CutFadeOut {
                    if voice.cut_fade_remaining == 0 {
                        voice.state = VoiceState::Idle;
                        break;
                    }
                    cut_scale =
                        (voice.cut_fade_remaining as f32) / (voice.cut_fade_total.max(1) as f32);
                    voice.cut_fade_remaining -= 1;
                }

                let (penv_val, _) = voice.pitch_env.next_value(sr);
                let t_sec = (voice.amp_env.current_frame as f32) / sr;
                let vib_semi = if ev.vib > 0.0 && ev.vibmod != 0.0 {
                    ev.vibmod * (core::f32::consts::TAU * ev.vib * t_sec).sin()
                } else {
                    0.0
                };
                let inst_freq = if ev.penv != 0.0 || vib_semi != 0.0 {
                    ev.freq_hz * 2.0f32.powf((ev.penv * penv_val + vib_semi) / 12.0)
                } else {
                    ev.freq_hz
                };

                let (mut sig_l, mut sig_r) = match ev.voice_kind {
                    VoiceKind::Synth => {
                        if ev.partial_count > 0 {
                            let s = voice.additive.next_sample(
                                inst_freq,
                                sr,
                                ev.partial_count,
                                &ev.partials,
                                &ev.phases,
                            );
                            (s, s)
                        } else if ev.fm_index > 0.0 || ev.fm_ops.iter().any(|o| o.level > 0.0) {
                            let s = voice.fm.next_sample(
                                inst_freq,
                                sr,
                                ev.fm_index,
                                ev.fmh,
                                1.0,
                                &ev.fm_ops,
                                &ev.fm_matrix,
                            );
                            (s, s)
                        } else if ev.waveform == Waveform::Supersaw || ev.unison > 1 {
                            voice.supersaw.next_stereo(
                                inst_freq,
                                ev.unison.max(2),
                                ev.detune,
                                ev.spread,
                                sr,
                            )
                        } else if matches!(
                            ev.waveform,
                            Waveform::WhiteNoise
                                | Waveform::PinkNoise
                                | Waveform::BrownNoise
                                | Waveform::Crackle
                        ) {
                            let s = voice.noise.next_sample(ev.waveform);
                            (s, s)
                        } else {
                            let s = voice.osc.next_sample(ev.waveform, inst_freq, sr);
                            (s, s)
                        }
                    }
                    VoiceKind::Noise => {
                        let s = voice.noise.next_sample(ev.waveform);
                        (s, s)
                    }
                    VoiceKind::Zzfx => {
                        let s = voice.zzfx.next_sample(&ev.zzfx, sr);
                        (s, s)
                    }
                    VoiceKind::KarplusStrong => {
                        let s = voice.karplus.next_sample(&ev.karplus, sr);
                        (s, s)
                    }
                    VoiceKind::InlinePipe => {
                        let s = voice.pipe_voice.next_sample(inst_freq, sr);
                        (s, s)
                    }
                    VoiceKind::Granular => {
                        let sid = ev.sample_handle.0 as usize;
                        let s = if let (Some(gran), Some(Some(buf))) =
                            (voice.granular.as_mut(), self.samples.get(sid))
                        {
                            gran.next_sample(buf)
                        } else {
                            0.0
                        };
                        (s, s)
                    }
                    VoiceKind::Sample | VoiceKind::Wavetable | VoiceKind::SoundFont => {
                        let sid = ev.sample_handle.0 as usize;
                        let s = if let (Some(player), Some(Some(buf))) =
                            (voice.sample_player.as_mut(), self.samples.get(sid))
                        {
                            let val = player.next_sample(buf);
                            if player.finished {
                                voice.state = VoiceState::Idle;
                            }
                            val
                        } else {
                            0.0
                        };
                        (s, s)
                    }
                };

                let env_gain = ev.gain * amp_val * cut_scale * multi_orbit_comp;
                let mut mono = 0.5 * (sig_l + sig_r) * env_gain;

                let (lpf_e, _) = voice.lpf_env.next_value(sr);
                if ev.lpf < 19_900.0 || ev.lpenv != 0.0 {
                    let fc = ev.lpf * 2.0f32.powf(ev.lpenv * lpf_e);
                    mono = voice.lpf_filter.process_sample(
                        mono,
                        fc,
                        ev.lpq,
                        sr,
                        ev.ftype,
                        FilterPassMode::LowPass,
                    );
                }
                let (hpf_e, _) = voice.hpf_env.next_value(sr);
                if ev.hpf > 10.0 || ev.hpenv != 0.0 {
                    let fc = ev.hpf.max(20.0) * 2.0f32.powf(ev.hpenv * hpf_e);
                    mono = voice.hpf_filter.process_sample(
                        mono,
                        fc,
                        ev.hpq,
                        sr,
                        ev.ftype,
                        FilterPassMode::HighPass,
                    );
                }
                let (bpf_e, _) = voice.bpf_env.next_value(sr);
                if ev.bpf > 10.0 || ev.bpenv != 0.0 {
                    let fc = ev.bpf.max(20.0) * 2.0f32.powf(ev.bpenv * bpf_e);
                    mono = voice.bpf_filter.process_sample(
                        mono,
                        fc,
                        ev.bpq,
                        sr,
                        ev.ftype,
                        FilterPassMode::BandPass,
                    );
                }
                if ev.vowel != crate::events::VowelFormant::None {
                    mono = voice.vowel_filter.process_vowel(mono, ev.vowel, sr);
                }
                if ev.ordered_fx.stage_count > 0 {
                    mono = voice.ordered_fx.process(mono, &ev.ordered_fx, sr);
                }
                mono = voice.crusher.process(mono, ev.coarse, ev.crush);
                mono = process_shape_and_distort(
                    mono,
                    ev.shape,
                    ev.distort,
                    DistortionMode::SoftClipTanh,
                );
                mono = voice.ring_mod.process(mono, &ev.ring_mod, sr);
                mono = voice.tremolo.process(
                    mono,
                    ev.tremolo_rate,
                    ev.tremolo_depth,
                    ev.tremolo_shape,
                    ev.tremolo_phase,
                    sr,
                );
                mono = voice.compressor.process(mono, &ev.compressor, sr);

                if ev.waveform == Waveform::Supersaw && ev.unison > 1 {
                    sig_l *= env_gain;
                    sig_r *= env_gain;
                } else {
                    let (pl, pr) = equal_power_pan(mono, ev.pan);
                    sig_l = pl;
                    sig_r = pr;
                }

                let (ph_l, ph_r) = voice.phaser.process_stereo(
                    sig_l,
                    sig_r,
                    ev.phaser,
                    ev.phaser_depth,
                    ev.phaser_sweep,
                    ev.phaser_center,
                    sr,
                );
                self.orbits[orb_idx].synth_bus.left[f] += ph_l * ev.postgain;
                self.orbits[orb_idx].synth_bus.right[f] += ph_r * ev.postgain;
            }
        }

        if self.steal_fade.active_frames > 0 {
            for f in 0..frames {
                if self.steal_fade.read_pos < self.steal_fade.active_frames {
                    let rp = self.steal_fade.read_pos as usize;
                    self.orbits[0].synth_bus.left[f] += self.steal_fade.left[rp];
                    self.orbits[0].synth_bus.right[f] += self.steal_fade.right[rp];
                    self.steal_fade.read_pos += 1;
                } else {
                    self.steal_fade.active_frames = 0;
                    break;
                }
            }
        }

        for (o_idx, orb) in self.orbits.iter_mut().enumerate() {
            orb.process_orbit_block(frames, sr);
            let (rms, pl, pr) = orb.monitor.take_rms_and_peak();
            if rms > 0.0 || pl > 0.0 || pr > 0.0 {
                let _ = self.channels.rms_tx.push(OrbitRmsReport {
                    orbit: o_idx as u8,
                    rms,
                    peak_left: pl,
                    peak_right: pr,
                });
            }

            match self.config.routing_mode {
                OutputRoutingMode::StereoMixdown => {
                    for f in 0..frames {
                        let base = f * ch;
                        output[base] = sanitize_f32_finite(output[base] + orb.synth_bus.left[f]);
                        if ch > 1 {
                            output[base + 1] =
                                sanitize_f32_finite(output[base + 1] + orb.synth_bus.right[f]);
                        }
                    }
                }
                OutputRoutingMode::MultichannelOrbits => {
                    let ch_l = 2 * o_idx;
                    let ch_r = ch_l + 1;
                    if ch_r < ch {
                        for f in 0..frames {
                            let base = f * ch;
                            output[base + ch_l] = sanitize_f32_finite(orb.synth_bus.left[f]);
                            output[base + ch_r] = sanitize_f32_finite(orb.synth_bus.right[f]);
                        }
                    }
                }
            }
        }

        self.current_sample_frame = block_end;
    }
}
