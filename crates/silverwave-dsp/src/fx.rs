//! Per-Voice Serial Effects Chain (`stretch`, `coarse`, `crush`, `shape`, `distort`,
//! `ring`, `tremolo`, `compressor`, `pan`, `phaser`, `postgain` — `03-SPEC` Section 5.1, `DSP-07`)
//! and `OrderedVoiceFxExecutor` (`PP-09`).

use crate::events::{
    CompressorParams, DistortionMode, FilterTopology, InsertFxStage, LfoShape, OrderedVoiceFxChain,
    RingModParams, sanitize_f32_finite, soft_limit_feedback,
};
use crate::filters::{FilterPassMode, MultiModeFilter, VowelFormantFilter};
use core::f32::consts::{FRAC_PI_2, PI, TAU};

/// Zero-order hold `coarse` sample-rate reducer and $2^{\text{crush}}$ `crush` bit-crusher (`Section 5.1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct BitCrusherAndCoarse {
    hold_counter: u32,
    held_sample: f32,
}

impl BitCrusherAndCoarse {
    /// Processes `x` through `coarse` (sample-rate decimation) followed by `crush` (bit quantization).
    #[inline]
    pub fn process(&mut self, x: f32, coarse: u32, crush: f32) -> f32 {
        let mut y = if coarse > 1 {
            if self.hold_counter == 0 {
                self.held_sample = x;
            }
            self.hold_counter = (self.hold_counter + 1) % coarse;
            self.held_sample
        } else {
            x
        };
        if crush > 0.0 {
            let steps = 2.0f32.powf(crush.clamp(1.0, 16.0));
            y = (y * steps).round() / steps;
        }
        y
    }
}

/// `shape` waveshaper (`y = ((1 + k) * x) / (1 + k * |x|)`) and `distort` overdrive (`Section 5.1`).
#[inline]
pub fn process_shape_and_distort(x: f32, shape: f32, distort: f32, mode: DistortionMode) -> f32 {
    let mut y = x;
    if shape > 0.0 {
        let s = shape.clamp(0.0, 0.999);
        let k = (2.0 * s) / (1.0 - s);
        y = ((1.0 + k) * y) / (1.0 + k * y.abs());
    }
    if distort > 0.0 {
        let drive = 1.0 + distort * 4.0;
        let driven = y * drive;
        y = match mode {
            DistortionMode::SoftClipTanh => driven.tanh(),
            DistortionMode::HardClip => driven.clamp(-1.0, 1.0),
            DistortionMode::WavefoldSaturate => (driven * PI * 0.5).sin(),
            DistortionMode::AsymmetricTube => {
                if driven >= 0.0 {
                    driven.tanh()
                } else {
                    (driven * 0.7).tanh()
                }
            }
        };
    }
    sanitize_f32_finite(y)
}

/// Per-voice Ring Modulator (`ring`, `ringf`, `ringdf` — `Section 4.7`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RingModulator {
    pub phase: f32,
    pub elapsed_s: f32,
}

impl RingModulator {
    /// Processes `x` through the ring modulator.
    #[inline]
    pub fn process(&mut self, x: f32, params: &RingModParams, sample_rate: f32) -> f32 {
        if params.ring_wet <= 0.0 {
            return x;
        }
        let sr = sample_rate.max(1.0);
        let inst_freq = params.ring_freq_hz + params.ring_slide_hz * self.elapsed_s;
        self.elapsed_s += 1.0 / sr;
        self.phase = (self.phase + (inst_freq.abs() / sr)).fract();
        let carrier = (self.phase * TAU).sin();
        let wet = params.ring_wet.clamp(0.0, 1.0);
        x * (1.0 - wet) + (x * carrier) * wet
    }
}

/// Amplitude modulation LFO (`tremolo_rate`, `tremolo_depth`, `tremolo_shape`, `tremolo_phase` — `Section 5.1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TremoloModulator {
    pub phase: f32,
    pub initialized: bool,
}

impl TremoloModulator {
    /// Steps the tremolo LFO and modulates `x`.
    #[inline]
    pub fn process(
        &mut self,
        x: f32,
        rate_hz: f32,
        depth: f32,
        shape: LfoShape,
        initial_phase: f32,
        sample_rate: f32,
    ) -> f32 {
        if depth <= 0.0 || rate_hz <= 0.0 {
            return x;
        }
        if !self.initialized {
            self.phase = initial_phase.fract();
            self.initialized = true;
        }
        let lfo = match shape {
            LfoShape::Sine => 0.5 * (1.0 + (self.phase * TAU).sin()),
            LfoShape::Tri => 1.0 - 2.0 * (self.phase - 0.5).abs(),
            LfoShape::Saw => self.phase,
            LfoShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        };
        self.phase = (self.phase + rate_hz / sample_rate.max(1.0)).fract();
        let d = depth.clamp(0.0, 1.0);
        x * (1.0 - d * (1.0 - lfo))
    }
}

/// Per-voice dynamics compressor (`Section 5.1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceCompressor {
    pub env_lin: f32,
}

impl VoiceCompressor {
    /// Applies feed-forward dynamics compression to `x`.
    #[inline]
    pub fn process(&mut self, x: f32, params: &CompressorParams, sample_rate: f32) -> f32 {
        if !params.enabled {
            return x;
        }
        let sr = sample_rate.max(1.0);
        let abs_x = x.abs().max(1.0e-6);
        let att_coeff = (-1.0 / (params.attack_s.max(0.0005) * sr)).exp();
        let rel_coeff = (-1.0 / (params.release_s.max(0.005) * sr)).exp();
        if abs_x > self.env_lin {
            self.env_lin = att_coeff * self.env_lin + (1.0 - att_coeff) * abs_x;
        } else {
            self.env_lin = rel_coeff * self.env_lin + (1.0 - rel_coeff) * abs_x;
        }
        let env_db = 20.0 * self.env_lin.max(1.0e-6).log10();
        let over_db = env_db - params.threshold_db;
        if over_db > 0.0 {
            let ratio = params.ratio.max(1.0);
            let gain_red_db = over_db * (1.0 - 1.0 / ratio);
            let gain_lin = 10.0f32.powf(-gain_red_db / 20.0);
            x * gain_lin
        } else {
            x
        }
    }
}

/// Equal-power stereo panning for `pan` in `[0.0, 1.0]` (`L = cos(pi/2 * p)`, `R = sin(pi/2 * p)` — `Section 5.1`).
#[inline]
pub fn equal_power_pan(mono: f32, pan: f32) -> (f32, f32) {
    let angle = pan.clamp(0.0, 1.0) * FRAC_PI_2;
    (mono * angle.cos(), mono * angle.sin())
}

/// 4-Stage stereo all-pass phaser network swept by an internal LFO (`Section 5.1`, `DSP-07`).
#[derive(Clone, Copy, Debug, Default)]
pub struct AllpassPhaser {
    pub lfo_phase: f32,
    ap_x1_l: [f32; 4],
    ap_y1_l: [f32; 4],
    ap_x1_r: [f32; 4],
    ap_y1_r: [f32; 4],
}

impl AllpassPhaser {
    /// Processes a stereo pair `(left, right)` through the 4-stage all-pass phaser.
    #[allow(clippy::too_many_arguments)]
    pub fn process_stereo(
        &mut self,
        left: f32,
        right: f32,
        rate_hz: f32,
        depth: f32,
        sweep_hz: f32,
        center_hz: f32,
        sample_rate: f32,
    ) -> (f32, f32) {
        if depth <= 0.0 || rate_hz <= 0.0 {
            return (left, right);
        }
        let sr = sample_rate.max(1.0);
        let lfo_l = (self.lfo_phase * TAU).sin();
        let lfo_r = ((self.lfo_phase + 0.25) * TAU).sin();
        self.lfo_phase = (self.lfo_phase + rate_hz / sr).fract();

        let fc_l = (center_hz + lfo_l * sweep_hz * 0.5).clamp(50.0, 0.45 * sr);
        let fc_r = (center_hz + lfo_r * sweep_hz * 0.5).clamp(50.0, 0.45 * sr);
        let t_l = (PI * fc_l / sr).tan();
        let t_r = (PI * fc_r / sr).tan();
        let c_l = (t_l - 1.0) / (t_l + 1.0);
        let c_r = (t_r - 1.0) / (t_r + 1.0);

        let mut yl = left;
        let mut yr = right;
        for i in 0..4 {
            let nl = soft_limit_feedback(c_l * yl + self.ap_x1_l[i] - c_l * self.ap_y1_l[i]);
            self.ap_x1_l[i] = yl;
            self.ap_y1_l[i] = nl;
            yl = nl;

            let nr = soft_limit_feedback(c_r * yr + self.ap_x1_r[i] - c_r * self.ap_y1_r[i]);
            self.ap_x1_r[i] = yr;
            self.ap_y1_r[i] = nr;
            yr = nr;
        }
        let d = depth.clamp(0.0, 1.0) * 0.5;
        (left * (1.0 - d) + yl * d, right * (1.0 - d) + yr * d)
    }
}

/// Stateful per-voice executor for `OrderedVoiceFxChain` (`PP-09`: preserves `.lpf(200).distort(3.0).lpf(1200)`).
#[derive(Clone, Copy, Debug, Default)]
pub struct OrderedVoiceFxExecutor {
    filters: [MultiModeFilter; 8],
    vowels: [VowelFormantFilter; 8],
    crushers: [BitCrusherAndCoarse; 8],
    rings: [RingModulator; 8],
    comps: [VoiceCompressor; 8],
}

impl OrderedVoiceFxExecutor {
    /// Creates a new `OrderedVoiceFxExecutor`.
    pub const fn new() -> Self {
        Self {
            filters: [MultiModeFilter::new(); 8],
            vowels: [VowelFormantFilter::new(); 8],
            crushers: [BitCrusherAndCoarse {
                hold_counter: 0,
                held_sample: 0.0,
            }; 8],
            rings: [RingModulator {
                phase: 0.0,
                elapsed_s: 0.0,
            }; 8],
            comps: [VoiceCompressor { env_lin: 0.0 }; 8],
        }
    }

    /// Executes all configured stages in `chain` sequentially on `sample`.
    pub fn process(
        &mut self,
        mut sample: f32,
        chain: &OrderedVoiceFxChain,
        sample_rate: f32,
    ) -> f32 {
        let count = (chain.stage_count as usize).min(8);
        for idx in 0..count {
            let Some(stage) = chain.stages[idx] else {
                continue;
            };
            sample = match stage {
                InsertFxStage::LowPass { cutoff_hz, q } => self.filters[idx].process_sample(
                    sample,
                    cutoff_hz,
                    q,
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::LowPass,
                ),
                InsertFxStage::HighPass { cutoff_hz, q } => self.filters[idx].process_sample(
                    sample,
                    cutoff_hz,
                    q,
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::HighPass,
                ),
                InsertFxStage::BandPass { cutoff_hz, q } => self.filters[idx].process_sample(
                    sample,
                    cutoff_hz,
                    q,
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::BandPass,
                ),
                InsertFxStage::Notch { cutoff_hz, q } => self.filters[idx].process_sample(
                    sample,
                    cutoff_hz,
                    q,
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::Notch,
                ),
                InsertFxStage::VowelFormant { morph } => {
                    self.vowels[idx].process_morph(sample, morph, sample_rate)
                }
                InsertFxStage::Distort { drive, mode } => {
                    process_shape_and_distort(sample, 0.0, drive, mode)
                }
                InsertFxStage::Bitcrush { bits, downsample } => {
                    self.crushers[idx].process(sample, downsample.max(1.0) as u32, bits)
                }
                InsertFxStage::RingMod { freq_hz, depth } => self.rings[idx].process(
                    sample,
                    &RingModParams {
                        ring_wet: depth,
                        ring_freq_hz: freq_hz,
                        ring_slide_hz: 0.0,
                    },
                    sample_rate,
                ),
                InsertFxStage::Compressor {
                    threshold_db,
                    ratio,
                    attack_ms,
                    release_ms,
                } => self.comps[idx].process(
                    sample,
                    &CompressorParams {
                        enabled: true,
                        threshold_db,
                        knee_db: 3.0,
                        ratio,
                        attack_s: attack_ms * 0.001,
                        release_s: release_ms * 0.001,
                    },
                    sample_rate,
                ),
            };
        }
        sanitize_f32_finite(sample)
    }
}
