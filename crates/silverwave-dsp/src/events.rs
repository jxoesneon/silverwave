//! Real-time POD event structures (`TriggerEvent`), waveform/filter/FX enums,
//! and branchless floating-point sanitization (`03-SPEC-AUDIO-DSP-AND-SAMPLES.md` Section 2.2).

/// Maximum additive partials per voice (`Section 2.2`).
pub const MAX_PARTIALS: usize = 64;

/// Maximum sidechain `duckorbit` targets per event (`Section 2.2`).
pub const MAX_DUCK_TARGETS: usize = 8;

/// Maximum FM operators in the 8x8 FM matrix (`Section 2.2`).
pub const MAX_FM_OPERATORS: usize = 8;

/// Default number of stereo orbit buses (`0..MAX_ORBITS`, `Section 3.1`).
pub const MAX_ORBITS: usize = 16;

/// Default pre-allocated voice pool capacity (`Section 3.4`).
pub const MAX_VOICES: usize = 256;

/// Pre-allocated lookahead event ring capacity (`Section 2.3`).
pub const LOOKAHEAD_CAPACITY: usize = 512;

/// Control-rate sub-block frame quantum (`64` frames = `1.33 ms` at `48 kHz`, `PP-01`).
pub const SUB_BLOCK_FRAMES: usize = 64;

/// Interned numeric handle identifying a decoded sample or wavetable buffer.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleId(pub u32);

/// Interned numeric handle identifying a sample bank (e.g. `RolandTR909`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BankId(pub u16);

/// Voice synthesis or sample playback generator category (`Section 2.2`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum VoiceKind {
    #[default]
    Synth = 0,
    Sample = 1,
    Wavetable = 2,
    Zzfx = 3,
    Noise = 4,
    SoundFont = 5,
    KarplusStrong = 6,
    Granular = 7,
    InlinePipe = 8,
}

/// Sample/wavetable loop behavior (`Section 2.2` & `Section 4.5`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LoopMode {
    #[default]
    Off = 0,
    On = 1,
    WavetableDefault = 2,
}

/// Band-limited oscillator waveform and noise types (`Section 4.1`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Waveform {
    #[default]
    Sine = 0,
    Sawtooth = 1,
    Square = 2,
    Triangle = 3,
    Supersaw = 4,
    WhiteNoise = 5,
    PinkNoise = 6,
    BrownNoise = 7,
    Crackle = 8,
}

/// Per-operator parameters in the 8-operator FM matrix (`Section 4.3`).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FmOpParams {
    pub ratio: f32,
    pub level: f32,
    pub feedback: f32,
    pub waveform: Waveform,
}

impl Default for FmOpParams {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            level: 0.0,
            feedback: 0.0,
            waveform: Waveform::Sine,
        }
    }
}

/// ZZFX oscillator waveshape (`Section 4.4`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ZzfxShape {
    #[default]
    Sine = 0,
    Triangle = 1,
    Sawtooth = 2,
    Tan = 3,
    Noise = 4,
    Square = 5,
}

/// 20-parameter Frank Force ZZFX synthesizer contract (`Section 4.4`).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ZzfxParams {
    pub enabled: bool,
    pub shape: ZzfxShape,
    pub gain: f32,
    pub zrand: f32,
    pub freq: f32,
    pub attack: f32,
    pub sustain_time: f32,
    pub release: f32,
    pub curve: f32,
    pub slide: f32,
    pub delta_slide: f32,
    pub pitch_jump: f32,
    pub pitch_jump_time: f32,
    pub lfo: f32,
    pub znoise: f32,
    pub zmod: f32,
    pub zcrush: f32,
    pub zdelay: f32,
    pub sustain_vol: f32,
    pub decay: f32,
    pub tremolo: f32,
}

impl Default for ZzfxParams {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: ZzfxShape::Sine,
            gain: 1.0,
            zrand: 0.0,
            freq: 220.0,
            attack: 0.01,
            sustain_time: 0.05,
            release: 0.1,
            curve: 1.0,
            slide: 0.0,
            delta_slide: 0.0,
            pitch_jump: 0.0,
            pitch_jump_time: 0.0,
            lfo: 0.0,
            znoise: 0.0,
            zmod: 0.0,
            zcrush: 0.0,
            zdelay: 0.0,
            sustain_vol: 0.8,
            decay: 0.02,
            tremolo: 0.0,
        }
    }
}

/// General MIDI SoundFont (`.sf2`) preset voice parameters (`Section 4.7`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundFontVoiceParams {
    pub sf2_bank_id: u16,
    pub preset_num: u8,
    pub bank_num: u8,
    pub key: u8,
    pub velocity: u8,
}

/// Karplus-Strong plucked-string physical model parameters (`Section 4.7`).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct KarplusStrongParams {
    pub freq_hz: f32,
    pub damping: f32,
    pub decay: f32,
    pub pluck_pos: f32,
}

impl Default for KarplusStrongParams {
    fn default() -> Self {
        Self {
            freq_hz: 220.0,
            damping: 0.5,
            decay: 0.992,
            pluck_pos: 0.5,
        }
    }
}

/// Per-voice Ring Modulator parameters (`ring`, `ringf`, `ringdf` — `Section 4.7`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct RingModParams {
    pub ring_wet: f32,
    pub ring_freq_hz: f32,
    pub ring_slide_hz: f32,
}

/// Filter topology selection (`12db`, `24db`, `ladder` — `Section 5.1`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum FilterTopology {
    #[default]
    Svf12dB = 0,
    Svf24dB = 1,
    MoogLadder = 2,
}

/// All 15 Strudel/SuperDirt vowel formant qualities + `None` (`Section 5.1`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum VowelFormant {
    #[default]
    None = 0,
    A = 1,
    E = 2,
    I = 3,
    O = 4,
    U = 5,
    Ae = 6,
    Aa = 7,
    Oe = 8,
    Ue = 9,
    Y = 10,
    Uh = 11,
    Un = 12,
    En = 13,
    An = 14,
    On = 15,
}

impl VowelFormant {
    /// Parses a vowel string including Unicode/ASCII aliases (`å`/`ɑ` -> `Aa`, `ø`/`ö` -> `Oe`, `ı` -> `Y`, `æ` -> `Ae`).
    pub fn from_str_alias(s: &str) -> Self {
        match s.trim() {
            "a" | "A" => Self::A,
            "e" | "E" => Self::E,
            "i" | "I" => Self::I,
            "o" | "O" => Self::O,
            "u" | "U" => Self::U,
            "ae" | "AE" | "æ" | "Æ" => Self::Ae,
            "aa" | "AA" | "å" | "Å" | "ɑ" => Self::Aa,
            "oe" | "OE" | "ø" | "Ø" | "ö" | "Ö" => Self::Oe,
            "ue" | "UE" | "ü" | "Ü" => Self::Ue,
            "y" | "Y" | "ı" => Self::Y,
            "uh" | "UH" => Self::Uh,
            "un" | "UN" => Self::Un,
            "en" | "EN" => Self::En,
            "an" | "AN" => Self::An,
            "on" | "ON" => Self::On,
            _ => Self::None,
        }
    }
}

/// LFO waveform shape (`Section 2.2`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LfoShape {
    #[default]
    Sine = 0,
    Tri = 1,
    Saw = 2,
    Square = 3,
}

/// Per-voice dynamics compressor parameters (`Section 5.1`).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CompressorParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub knee_db: f32,
    pub ratio: f32,
    pub attack_s: f32,
    pub release_s: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            knee_db: 6.0,
            ratio: 4.0,
            attack_s: 0.005,
            release_s: 0.080,
        }
    }
}

/// Nonlinear distortion mode for `InsertFxStage::Distort` (`08-SPEC` Section 6.3).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum DistortionMode {
    #[default]
    SoftClipTanh = 0,
    HardClip = 1,
    WavefoldSaturate = 2,
    AsymmetricTube = 3,
}

/// Single stage in an `OrderedVoiceFxChain` (`PP-09`, `08-SPEC` Section 6.3).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum InsertFxStage {
    LowPass { cutoff_hz: f32, q: f32 },
    HighPass { cutoff_hz: f32, q: f32 },
    BandPass { cutoff_hz: f32, q: f32 },
    Notch { cutoff_hz: f32, q: f32 },
    VowelFormant { morph: f32 },
    Distort { drive: f32, mode: DistortionMode },
    Bitcrush { bits: f32, downsample: f32 },
    RingMod { freq_hz: f32, depth: f32 },
    Compressor {
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
    },
}

/// Ordered per-voice insert FX chain preserving repeated filter/distortion stages in exact call order (`PP-09`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct OrderedVoiceFxChain {
    pub stages: [Option<InsertFxStage>; 8],
    pub stage_count: u8,
}

impl OrderedVoiceFxChain {
    /// Creates an empty `OrderedVoiceFxChain`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            stages: [None; 8],
            stage_count: 0,
        }
    }

    /// Appends an `InsertFxStage` in exact user call order (up to 8 stages).
    #[inline]
    pub fn push_stage(&mut self, stage: InsertFxStage) -> bool {
        let idx = self.stage_count as usize;
        if idx < 8 {
            self.stages[idx] = Some(stage);
            self.stage_count += 1;
            true
        } else {
            false
        }
    }
}

/// Plain Old Data (`#[repr(C)]`, `Copy + Clone`, zero heap pointers) event descriptor
/// crossing the lock-free `rtrb` SPSC ring buffer into the audio thread (`Section 2.2`).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TriggerEvent {
    pub trigger_sample: u64,
    pub duration_samples: u64,
    pub orbit: u8,
    pub cut_group: i32,
    pub stream_priority: u8,
    pub hap_ordinal: u16,
    pub voice_kind: VoiceKind,
    pub waveform: Waveform,

    pub sample_handle: SampleId,
    pub bank_handle: BankId,
    pub n: u32,
    pub begin: f32,
    pub end: f32,
    pub speed: f32,
    pub loop_mode: LoopMode,
    pub loop_begin: f32,
    pub loop_end: f32,
    pub fit: f32,
    pub chop: u16,
    pub striate: u16,
    pub slice_idx: u16,
    pub slice_count: u16,
    pub splice_mode: bool,
    pub scrub: f32,

    pub freq_hz: f32,
    pub penv: f32,
    pub pattack: f32,
    pub pdecay: f32,
    pub psustain: f32,
    pub prelease: f32,
    pub pcurve: f32,
    pub panchor: f32,
    pub vib: f32,
    pub vibmod: f32,

    pub gain: f32,
    pub postgain: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub clip: f32,

    pub unison: u8,
    pub detune: f32,
    pub spread: f32,
    pub partial_count: u8,
    pub partials: [f32; MAX_PARTIALS],
    pub phases: [f32; MAX_PARTIALS],

    pub fm_index: f32,
    pub fmh: f32,
    pub fmattack: f32,
    pub fmdecay: f32,
    pub fmsustain: f32,
    pub fmenv: f32,
    pub fm_ops: [FmOpParams; MAX_FM_OPERATORS],
    pub fm_matrix: [[f32; MAX_FM_OPERATORS]; MAX_FM_OPERATORS],

    pub zzfx: ZzfxParams,
    pub soundfont: SoundFontVoiceParams,
    pub karplus: KarplusStrongParams,
    pub ring_mod: RingModParams,
    pub ordered_fx: OrderedVoiceFxChain,

    pub ftype: FilterTopology,
    pub lpf: f32,
    pub lpq: f32,
    pub lpenv: f32,
    pub lpa: f32,
    pub lpd: f32,
    pub lps: f32,
    pub lpr: f32,
    pub hpf: f32,
    pub hpq: f32,
    pub hpenv: f32,
    pub hpa: f32,
    pub hpd: f32,
    pub hps: f32,
    pub hpr: f32,
    pub bpf: f32,
    pub bpq: f32,
    pub bpenv: f32,
    pub bpa: f32,
    pub bpd: f32,
    pub bps: f32,
    pub bpr: f32,
    pub vowel: VowelFormant,

    pub stretch: f32,
    pub coarse: u32,
    pub crush: f32,
    pub shape: f32,
    pub distort: f32,

    pub tremolo_rate: f32,
    pub tremolo_depth: f32,
    pub tremolo_shape: LfoShape,
    pub tremolo_phase: f32,
    pub compressor: CompressorParams,
    pub pan: f32,
    pub phaser: f32,
    pub phaser_depth: f32,
    pub phaser_sweep: f32,
    pub phaser_center: f32,

    pub delay: f32,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_lock: bool,
    pub cps: f32,
    pub room: f32,
    pub size: f32,
    pub dry: f32,
    pub leslie: f32,
    pub lrate: f32,
    pub lsize: f32,
    pub duck_targets: [u8; MAX_DUCK_TARGETS],
    pub duck_count: u8,
    pub duck_attack: f32,
    pub duck_depth: f32,
    pub orbit_send_count: u8,
}

impl Default for TriggerEvent {
    fn default() -> Self {
        Self::default_synth(Waveform::Sawtooth, 440.0)
    }
}

impl TriggerEvent {
    /// Creates a default synth `TriggerEvent` with `waveform` and `freq_hz`.
    pub fn default_synth(waveform: Waveform, freq_hz: f32) -> Self {
        Self {
            trigger_sample: 0,
            duration_samples: 4800,
            orbit: 0,
            cut_group: 0,
            stream_priority: 0,
            hap_ordinal: 0,
            voice_kind: VoiceKind::Synth,
            waveform,
            sample_handle: SampleId(0),
            bank_handle: BankId(0),
            n: 0,
            begin: 0.0,
            end: 1.0,
            speed: 1.0,
            loop_mode: LoopMode::Off,
            loop_begin: 0.0,
            loop_end: 1.0,
            fit: 0.0,
            chop: 0,
            striate: 0,
            slice_idx: 0,
            slice_count: 0,
            splice_mode: false,
            scrub: 0.0,
            freq_hz,
            penv: 0.0,
            pattack: 0.001,
            pdecay: 0.05,
            psustain: 0.0,
            prelease: 0.05,
            pcurve: 1.0,
            panchor: 0.0,
            vib: 0.0,
            vibmod: 0.0,
            gain: 0.8,
            postgain: 1.0,
            attack: 0.002,
            decay: 0.05,
            sustain: 0.8,
            release: 0.02,
            clip: 1.0,
            unison: 1,
            detune: 0.0,
            spread: 0.5,
            partial_count: 0,
            partials: [0.0; MAX_PARTIALS],
            phases: [0.0; MAX_PARTIALS],
            fm_index: 0.0,
            fmh: 1.0,
            fmattack: 0.002,
            fmdecay: 0.05,
            fmsustain: 0.5,
            fmenv: 1.0,
            fm_ops: [FmOpParams::default(); MAX_FM_OPERATORS],
            fm_matrix: [[0.0; MAX_FM_OPERATORS]; MAX_FM_OPERATORS],
            zzfx: ZzfxParams::default(),
            soundfont: SoundFontVoiceParams::default(),
            karplus: KarplusStrongParams::default(),
            ring_mod: RingModParams::default(),
            ordered_fx: OrderedVoiceFxChain::new(),
            ftype: FilterTopology::Svf12dB,
            lpf: 20_000.0,
            lpq: 0.707,
            lpenv: 0.0,
            lpa: 0.002,
            lpd: 0.05,
            lps: 0.0,
            lpr: 0.05,
            hpf: 0.0,
            hpq: 0.707,
            hpenv: 0.0,
            hpa: 0.002,
            hpd: 0.05,
            hps: 0.0,
            hpr: 0.05,
            bpf: 0.0,
            bpq: 1.0,
            bpenv: 0.0,
            bpa: 0.002,
            bpd: 0.05,
            bps: 0.0,
            bpr: 0.05,
            vowel: VowelFormant::None,
            stretch: 0.0,
            coarse: 1,
            crush: 0.0,
            shape: 0.0,
            distort: 0.0,
            tremolo_rate: 0.0,
            tremolo_depth: 0.0,
            tremolo_shape: LfoShape::Sine,
            tremolo_phase: 0.0,
            compressor: CompressorParams::default(),
            pan: 0.5,
            phaser: 0.0,
            phaser_depth: 0.0,
            phaser_sweep: 800.0,
            phaser_center: 1000.0,
            delay: 0.0,
            delay_time: 0.25,
            delay_feedback: 0.35,
            delay_lock: false,
            cps: 0.5,
            room: 0.0,
            size: 0.5,
            dry: 1.0,
            leslie: 0.0,
            lrate: 6.7,
            lsize: 0.3,
            duck_targets: [0; MAX_DUCK_TARGETS],
            duck_count: 0,
            duck_attack: 0.2,
            duck_depth: 0.0,
            orbit_send_count: 1,
        }
    }

    /// Creates a default sample `TriggerEvent` for `sample_handle`.
    pub fn default_sample(sample_handle: SampleId) -> Self {
        let mut ev = Self::default_synth(Waveform::Sine, 440.0);
        ev.voice_kind = VoiceKind::Sample;
        ev.sample_handle = sample_handle;
        ev
    }
}

/// Branchless NaN / Inf / subnormal sanitizer (`Section 1.1` & `Section 8.8`).
#[inline]
pub fn sanitize_f32_finite(x: f32) -> f32 {
    if x.is_finite() {
        if x.abs() < 1.0e-30 {
            0.0
        } else {
            x
        }
    } else {
        0.0
    }
}

/// Sanitizes non-finite feedback state and applies a smooth $\pm 4.0$ soft limit (`Section 8.8`).
#[inline]
pub fn soft_limit_feedback(x: f32) -> f32 {
    let clean = sanitize_f32_finite(x);
    clean.clamp(-4.0, 4.0)
}
