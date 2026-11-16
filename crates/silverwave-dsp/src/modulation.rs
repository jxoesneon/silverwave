//! 64-Sample Sub-Block Continuous `k-rate` & `a-rate` Modulation Buses (`PP-01`, `PP-02`)
//! and Octatrack Scene A/B Crossfader Macros (`CK-03`).

use crate::events::SUB_BLOCK_FRAMES;
use silverwave_core::{Pattern, Rational64, State, TimeSpan};

/// Rate at which a continuous control signal is sampled and ramped on active voices (`PP-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulationRate {
    /// Sampled only at `hap.has_onset()` (legacy discrete behavior).
    OnsetOnly,
    /// Evaluated every 64-sample sub-block (`~750 Hz` at 48 kHz) with linear per-sample SIMD ramp.
    ControlRateSubBlock,
    /// Evaluated at full audio sample rate (`48 kHz` / `96 kHz`) for FM/AM/ring-mod/waveshaping.
    AudioRateSampleExact,
}

/// Identifies either a voice-local modulation lane or a globally shared named bus (`^bus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlBusId {
    VoiceLocal {
        stream_id: u16,
        param: ModulatableParam,
    },
    NamedSharedBus {
        bus_index: u16,
    },
}

/// Complete enumeration of all 39 modulatable DSP parameters (`PP-02`: zero non-modulatable parameters).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModulatableParam {
    Gain = 0,
    Pan = 1,
    NoteSemitones = 2,
    SpeedRatio = 3,
    PitchShiftSemitones = 4,
    DetuneCents = 5,
    LpfCutoffHz = 6,
    LpfResonanceQ = 7,
    HpfCutoffHz = 8,
    HpfResonanceQ = 9,
    BpfCutoffHz = 10,
    BpfResonanceQ = 11,
    VowelFormantMorph = 12,
    CrushBits = 13,
    CoarseDownsample = 14,
    ShapeDrive = 15,
    DistortDrive = 16,
    RingModFreqHz = 17,
    RingModDepth = 18,
    FmIndex = 19,
    FmRatio = 20,
    WavetablePosition = 21,
    UnisonSpread = 22,
    DelaySend = 23,
    DelayTimeSec = 24,
    DelayFeedback = 25,
    ReverbRoomSend = 26,
    ReverbSize = 27,
    ReverbDamping = 28,
    PhaserRateHz = 29,
    PhaserDepth = 30,
    LeslieSpeed = 31,
    GrainDensityHz = 32,
    GrainDurationSec = 33,
    GrainPositionNormalized = 34,
    OscillatorMetaZ1 = 35,
    OscillatorMetaZ2 = 36,
    OscillatorMetaZ3 = 37,
    OscillatorMetaZ4 = 38,
}

impl ModulatableParam {
    /// Total number of modulatable DSP parameters (`39`).
    pub const COUNT: usize = 39;

    /// Returns the 0..39 index of this parameter.
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Maps an index in `0..39` back to `Some(ModulatableParam)`.
    pub const fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::Gain),
            1 => Some(Self::Pan),
            2 => Some(Self::NoteSemitones),
            3 => Some(Self::SpeedRatio),
            4 => Some(Self::PitchShiftSemitones),
            5 => Some(Self::DetuneCents),
            6 => Some(Self::LpfCutoffHz),
            7 => Some(Self::LpfResonanceQ),
            8 => Some(Self::HpfCutoffHz),
            9 => Some(Self::HpfResonanceQ),
            10 => Some(Self::BpfCutoffHz),
            11 => Some(Self::BpfResonanceQ),
            12 => Some(Self::VowelFormantMorph),
            13 => Some(Self::CrushBits),
            14 => Some(Self::CoarseDownsample),
            15 => Some(Self::ShapeDrive),
            16 => Some(Self::DistortDrive),
            17 => Some(Self::RingModFreqHz),
            18 => Some(Self::RingModDepth),
            19 => Some(Self::FmIndex),
            20 => Some(Self::FmRatio),
            21 => Some(Self::WavetablePosition),
            22 => Some(Self::UnisonSpread),
            23 => Some(Self::DelaySend),
            24 => Some(Self::DelayTimeSec),
            25 => Some(Self::DelayFeedback),
            26 => Some(Self::ReverbRoomSend),
            27 => Some(Self::ReverbSize),
            28 => Some(Self::ReverbDamping),
            29 => Some(Self::PhaserRateHz),
            30 => Some(Self::PhaserDepth),
            31 => Some(Self::LeslieSpeed),
            32 => Some(Self::GrainDensityHz),
            33 => Some(Self::GrainDurationSec),
            34 => Some(Self::GrainPositionNormalized),
            35 => Some(Self::OscillatorMetaZ1),
            36 => Some(Self::OscillatorMetaZ2),
            37 => Some(Self::OscillatorMetaZ3),
            38 => Some(Self::OscillatorMetaZ4),
            _ => None,
        }
    }
}

/// Pre-evaluated 64-sample control frame pushed from the scheduler into the lock-free voice bus table (`08-SPEC` Section 2.3).
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlSubBlockFrame {
    pub bus_id: ControlBusId,
    pub start_sample_frame: u64,
    pub sub_block_len: u16,
    pub start_value: f32,
    pub end_value: f32,
    pub rate: ModulationRate,
}

impl Default for ControlSubBlockFrame {
    fn default() -> Self {
        Self {
            bus_id: ControlBusId::NamedSharedBus { bus_index: 0 },
            start_sample_frame: 0,
            sub_block_len: SUB_BLOCK_FRAMES as u16,
            start_value: 0.0,
            end_value: 0.0,
            rate: ModulationRate::ControlRateSubBlock,
        }
    }
}

impl ControlSubBlockFrame {
    /// Linearly interpolates the control value at sample `offset` inside `0..sub_block_len`.
    #[inline]
    pub fn sample_at(&self, offset: usize) -> f32 {
        if self.sub_block_len <= 1 {
            return self.end_value;
        }
        let t = (offset.min(self.sub_block_len as usize) as f32) / (self.sub_block_len as f32);
        self.start_value + (self.end_value - self.start_value) * t
    }
}

/// Trait for evaluating continuous modulation signals into 64-sample sub-block frames (`08-SPEC` Section 2.3).
pub trait ContinuousModulationEngine: Send + Sync + 'static {
    /// Evaluates all active continuous modulation lanes over `[block_start_cycle, block_end_cycle)`
    /// into pre-allocated `ControlSubBlockFrame` slices without triggering new voice onsets.
    fn evaluate_modulation_sub_blocks(
        &self,
        block_start_cycle: Rational64,
        block_end_cycle: Rational64,
        sample_frame_start: u64,
        frames_in_block: usize,
        out_frames: &mut [ControlSubBlockFrame],
    ) -> usize;
}

/// Pre-allocated per-stream 39-parameter modulation table with a `u64` dirty-bit bitmask (`Section 8.1`).
#[derive(Clone, Debug)]
pub struct ModulationBusTable {
    pub dirty_mask: u64,
    pub start_values: [f32; ModulatableParam::COUNT],
    pub end_values: [f32; ModulatableParam::COUNT],
}

impl Default for ModulationBusTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ModulationBusTable {
    /// Creates a clean `ModulationBusTable` with `dirty_mask == 0`.
    pub const fn new() -> Self {
        Self {
            dirty_mask: 0,
            start_values: [0.0; ModulatableParam::COUNT],
            end_values: [0.0; ModulatableParam::COUNT],
        }
    }

    /// Updates a parameter ramp `[start_val, end_val]`, setting its dirty bit ONLY if the value changed.
    #[inline]
    pub fn apply_sub_block_ramp(
        &mut self,
        param: ModulatableParam,
        start_val: f32,
        end_val: f32,
    ) -> bool {
        let idx = param.index();
        let bit = 1u64 << idx;
        if (self.start_values[idx] - start_val).abs() > 1.0e-6
            || (self.end_values[idx] - end_val).abs() > 1.0e-6
        {
            self.start_values[idx] = start_val;
            self.end_values[idx] = end_val;
            self.dirty_mask |= bit;
            true
        } else {
            self.dirty_mask &= !bit;
            false
        }
    }

    /// Checks whether `param` has an active modulation update in the current sub-block.
    #[inline]
    pub const fn is_dirty(&self, param: ModulatableParam) -> bool {
        (self.dirty_mask & (1u64 << (param as usize))) != 0
    }

    /// Interpolates the modulated value of `param` at frame `offset` within a 64-sample sub-block.
    #[inline]
    pub fn value_at(&self, param: ModulatableParam, offset: usize, sub_block_len: usize) -> f32 {
        let idx = param.index();
        let s = self.start_values[idx];
        let e = self.end_values[idx];
        if sub_block_len <= 1 {
            return e;
        }
        let t = (offset.min(sub_block_len) as f32) / (sub_block_len as f32);
        s + (e - s) * t
    }
}

/// Registered continuous pattern lane driving a `ControlBusId`.
pub struct RegisteredModulationLane {
    pub bus_id: ControlBusId,
    pub rate: ModulationRate,
    pub signal: Pattern<f64>,
}

/// Concrete `ContinuousModulationEngine` evaluating registered `Pattern<f64>` signals every 64-sample sub-block.
#[derive(Default)]
pub struct PatternModulationEvaluator {
    lanes: Vec<RegisteredModulationLane>,
}

impl PatternModulationEvaluator {
    /// Creates an empty `PatternModulationEvaluator`.
    pub fn new() -> Self {
        Self { lanes: Vec::new() }
    }

    /// Registers a continuous `Pattern<f64>` lane for `bus_id`.
    pub fn register_lane(
        &mut self,
        bus_id: ControlBusId,
        rate: ModulationRate,
        signal: Pattern<f64>,
    ) {
        self.lanes.push(RegisteredModulationLane {
            bus_id,
            rate,
            signal,
        });
    }
}

impl ContinuousModulationEngine for PatternModulationEvaluator {
    fn evaluate_modulation_sub_blocks(
        &self,
        block_start_cycle: Rational64,
        block_end_cycle: Rational64,
        sample_frame_start: u64,
        frames_in_block: usize,
        out_frames: &mut [ControlSubBlockFrame],
    ) -> usize {
        if frames_in_block == 0 || self.lanes.is_empty() || out_frames.is_empty() {
            return 0;
        }
        let num_sub = frames_in_block.div_ceil(SUB_BLOCK_FRAMES);
        let total_cycle_span = block_end_cycle - block_start_cycle;
        let sub_cycle_dt = total_cycle_span / Rational64::from_integer(num_sub as i64);
        let eps = Rational64::new_const(1, 10_000);

        let mut written = 0usize;
        for sub_idx in 0..num_sub {
            let sub_frame_offset = sub_idx * SUB_BLOCK_FRAMES;
            let sub_len = SUB_BLOCK_FRAMES.min(frames_in_block - sub_frame_offset) as u16;
            let c0 = block_start_cycle + sub_cycle_dt * Rational64::from_integer(sub_idx as i64);
            let c1 = c0 + sub_cycle_dt;

            for lane in &self.lanes {
                if written >= out_frames.len() {
                    return written;
                }
                let s_val = lane
                    .signal
                    .query(&State::new(TimeSpan::new_clamped(c0, c0 + eps)))
                    .first()
                    .map(|h| h.value as f32)
                    .unwrap_or(0.0);
                let e_val = lane
                    .signal
                    .query(&State::new(TimeSpan::new_clamped(c1, c1 + eps)))
                    .first()
                    .map(|h| h.value as f32)
                    .unwrap_or(s_val);
                out_frames[written] = ControlSubBlockFrame {
                    bus_id: lane.bus_id,
                    start_sample_frame: sample_frame_start + (sub_frame_offset as u64),
                    sub_block_len: sub_len,
                    start_value: s_val,
                    end_value: e_val,
                    rate: lane.rate,
                };
                written += 1;
            }
        }
        written
    }
}

/// Octatrack-style Crossfader Scene Lock and Live Loop Transition Freeze (`CK-03`, `09-SPEC` Section 4).
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderSceneBank {
    pub scene_a_id: u8,
    pub scene_b_id: u8,
    /// Continuous crossfader position `0.0` (Scene A) .. `1.0` (Scene B), slewed at 64-sample k-rate.
    pub xfader_position: f32,
    pub looper_freeze_active: bool,
    pub looper_freeze_cycles: Rational64,
    pub scenes: [[Option<f32>; ModulatableParam::COUNT]; 16],
}

impl Default for CrossfaderSceneBank {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossfaderSceneBank {
    /// Creates a new 16-scene `CrossfaderSceneBank`.
    pub const fn new() -> Self {
        Self {
            scene_a_id: 0,
            scene_b_id: 1,
            xfader_position: 0.0,
            looper_freeze_active: false,
            looper_freeze_cycles: Rational64::zero(),
            scenes: [[None; ModulatableParam::COUNT]; 16],
        }
    }

    /// Locks `param` to `value` inside `scene_id` (`0..16`).
    pub fn set_scene_param(&mut self, scene_id: u8, param: ModulatableParam, value: f32) {
        let s = (scene_id as usize).min(15);
        self.scenes[s][param.index()] = Some(value);
    }

    /// Interpolates `param` between `scene_a_id` and `scene_b_id` at `xfader_position` (`0.0..=1.0`).
    pub fn interpolate_param(&self, param: ModulatableParam, default_val: f32) -> f32 {
        let idx = param.index();
        let a = self.scenes[(self.scene_a_id as usize).min(15)][idx].unwrap_or(default_val);
        let b = self.scenes[(self.scene_b_id as usize).min(15)][idx].unwrap_or(default_val);
        let pos = self.xfader_position.clamp(0.0, 1.0);
        a + (b - a) * pos
    }
}
