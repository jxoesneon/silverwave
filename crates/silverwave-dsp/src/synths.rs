//! Band-Limited PolyBLEP Oscillators (`DSP-01`), Unison Supersaw (`DSP-02`),
//! Spectral Noise Generators (`DSP-03`), Additive Partials, 8-Operator FM Matrix (`DSP-04`),
//! 20-Parameter ZZFX (`DSP-05`), Karplus-Strong, and Inline `>>` Modular DSP Pipe (`CK-05`).

use crate::events::{
    sanitize_f32_finite, soft_limit_feedback, FmOpParams, KarplusStrongParams, Waveform,
    ZzfxParams, ZzfxShape, MAX_FM_OPERATORS, MAX_PARTIALS,
};
use core::f32::consts::TAU;

/// 2-point Polynomial Band-Limited Step (PolyBLEP) residual suppressing alias foldback by >= 50 dB (`DSP-01`).
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Band-limited PolyBLEP oscillator (`Sine`, `Sawtooth`, `Square`, `Triangle` — `Section 4.1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct PolyBlepOscillator {
    pub phase: f32,
    pub tri_integrator: f32,
}

impl PolyBlepOscillator {
    /// Creates a new `PolyBlepOscillator` with `initial_phase` in `[0.0, 1.0)`.
    #[inline]
    pub const fn new(initial_phase: f32) -> Self {
        Self {
            phase: initial_phase,
            tri_integrator: 0.0,
        }
    }

    /// Advances the oscillator by one sample at `freq_hz` and `sample_rate`.
    #[inline]
    pub fn next_sample(&mut self, waveform: Waveform, freq_hz: f32, sample_rate: f32) -> f32 {
        let dt = (freq_hz.abs() / sample_rate.max(1.0)).clamp(0.0, 0.499);
        let t = self.phase;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        match waveform {
            Waveform::Sine => (t * TAU).sin(),
            Waveform::Sawtooth => {
                let naive = 2.0 * t - 1.0;
                let blep = naive - poly_blep(t, dt);
                if dt >= 0.06 {
                    let max_h = (0.45 / dt).floor() as usize;
                    let mut sum = 0.0f32;
                    for k in 1..=max_h.min(12) {
                        let sign = if k % 2 == 0 { -1.0 } else { 1.0 };
                        sum += sign * ((k as f32) * t * TAU).sin() / (k as f32);
                    }
                    sum * (2.0 / core::f32::consts::PI)
                } else {
                    blep
                }
            }
            Waveform::Square => {
                let naive = if t < 0.5 { 1.0 } else { -1.0 };
                let blep = naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt);
                if dt >= 0.06 {
                    let max_h = (0.45 / dt).floor() as usize;
                    let mut sum = 0.0f32;
                    let mut k = 1usize;
                    while k <= max_h.min(11) {
                        sum += ((k as f32) * t * TAU).sin() / (k as f32);
                        k += 2;
                    }
                    sum * (4.0 / core::f32::consts::PI)
                } else {
                    blep
                }
            }
            Waveform::Triangle => {
                let sq = if t < 0.5 { 1.0 } else { -1.0 }
                    + poly_blep(t, dt)
                    - poly_blep((t + 0.5).fract(), dt);
                self.tri_integrator = 0.999 * self.tri_integrator + 4.0 * dt * sq;
                self.tri_integrator.clamp(-1.0, 1.0)
            }
            _ => (t * TAU).sin(),
        }
    }
}

/// Unison detuned stereo Supersaw generator (`unison: 1..=16`, `detune`, `spread`, `1/sqrt(unison)` gain — `DSP-02`).
#[derive(Clone, Copy, Debug)]
pub struct SupersawGenerator {
    pub phases: [f32; 16],
}

impl Default for SupersawGenerator {
    fn default() -> Self {
        Self::new(0x517C_C1B7_2722_0A95)
    }
}

impl SupersawGenerator {
    /// Initializes 16 deterministic phase offsets from `seed`.
    pub fn new(seed: u64) -> Self {
        let mut phases = [0.0f32; 16];
        let mut s = seed;
        for (i, p) in phases.iter_mut().enumerate() {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            *p = ((s >> 40) as f32) / ((1u64 << 24) as f32) + (i as f32) * 0.0625;
            *p = p.fract();
        }
        Self { phases }
    }

    /// Generates one stereo frame `(left, right)` with `1.0 / sqrt(unison)` RMS normalization (`DSP-02`).
    pub fn next_stereo(
        &mut self,
        freq_hz: f32,
        unison: u8,
        detune: f32,
        spread: f32,
        sample_rate: f32,
    ) -> (f32, f32) {
        let n = unison.clamp(1, 16) as usize;
        let norm = 1.0 / (n as f32).sqrt();
        let spread_clamped = spread.clamp(0.0, 1.0);
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        for i in 0..n {
            let offset_norm = if n == 1 {
                0.0
            } else {
                2.0 * (i as f32) / ((n - 1) as f32) - 1.0
            };
            let voice_freq = freq_hz * (1.0 + offset_norm * detune * 0.03);
            let dt = (voice_freq.abs() / sample_rate.max(1.0)).clamp(0.0, 0.49);
            let t = self.phases[i];
            self.phases[i] = (t + dt).fract();
            let sample = (2.0 * t - 1.0) - poly_blep(t, dt);

            let pan = if n == 1 {
                0.5
            } else {
                let dir = if i % 2 == 0 { -1.0 } else { 1.0 };
                0.5 + 0.5 * dir * spread_clamped * offset_norm.abs().max(0.5)
            };
            let angle = pan.clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
            left += sample * angle.cos();
            right += sample * angle.sin();
        }
        (left * norm * 1.4142135, right * norm * 1.4142135)
    }
}

/// White, Pink (`-3 dB/oct` Paul Kellet), Brown (`-6 dB/oct` integrator), and Crackle noise generator (`DSP-03`).
#[derive(Clone, Copy, Debug)]
pub struct NoiseGenerator {
    pub rng_state: u64,
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
    b6: f32,
    brown_state: f32,
    crackle_y1: f32,
    crackle_y2: f32,
}

impl Default for NoiseGenerator {
    fn default() -> Self {
        Self::new(0x1234_5678_9ABC_DEF0)
    }
}

impl NoiseGenerator {
    /// Creates a new `NoiseGenerator` with deterministic `seed`.
    pub const fn new(seed: u64) -> Self {
        Self {
            rng_state: if seed == 0 { 0xCAFE_BABE } else { seed },
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            b3: 0.0,
            b4: 0.0,
            b5: 0.0,
            b6: 0.0,
            brown_state: 0.0,
            crackle_y1: 0.27,
            crackle_y2: 0.0,
        }
    }

    #[inline]
    fn next_white(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let u = ((z >> 40) as f32) / ((1u64 << 24) as f32);
        2.0 * u - 1.0
    }

    /// Generates the next noise sample for `waveform` (`WhiteNoise`, `PinkNoise`, `BrownNoise`, `Crackle`).
    pub fn next_sample(&mut self, waveform: Waveform) -> f32 {
        match waveform {
            Waveform::WhiteNoise => self.next_white(),
            Waveform::PinkNoise => {
                let white = self.next_white();
                self.b0 = 0.99886 * self.b0 + white * 0.0555179;
                self.b1 = 0.99332 * self.b1 + white * 0.0750759;
                self.b2 = 0.96900 * self.b2 + white * 0.1538520;
                self.b3 = 0.86650 * self.b3 + white * 0.3104856;
                self.b4 = 0.55000 * self.b4 + white * 0.5329522;
                self.b5 = -0.7616 * self.b5 - white * 0.0168980;
                let pink = self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + self.b6 + white * 0.5362;
                self.b6 = white * 0.115926;
                (pink * 0.11).clamp(-1.0, 1.0)
            }
            Waveform::BrownNoise => {
                let white = self.next_white();
                self.brown_state = (self.brown_state + 0.02 * white) * 0.998;
                (self.brown_state * 3.5).clamp(-1.0, 1.0)
            }
            Waveform::Crackle => {
                let chaos = 1.95f32;
                let y0 = (self.crackle_y1 * chaos - self.crackle_y2 - 0.05).abs();
                self.crackle_y2 = self.crackle_y1;
                self.crackle_y1 = if y0.is_finite() && y0 < 2.0 { y0 } else { 0.27 };
                (self.crackle_y1 - 0.5).clamp(-1.0, 1.0)
            }
            _ => self.next_white(),
        }
    }
}

/// Additive harmonic synthesizer with automatic Nyquist zeroing (`Section 4.2`).
#[derive(Clone, Copy, Debug, Default)]
pub struct AdditiveOscillator {
    pub phase: f32,
}

impl AdditiveOscillator {
    /// Synthesizes one sample from `partials[0..partial_count]` and `phases[0..partial_count]`,
    /// zeroing any harmonic $k \cdot f_0 \ge f_s / 2$.
    pub fn next_sample(
        &mut self,
        freq_hz: f32,
        sample_rate: f32,
        partial_count: u8,
        partials: &[f32; MAX_PARTIALS],
        phases: &[f32; MAX_PARTIALS],
    ) -> f32 {
        let nyquist = 0.5 * sample_rate;
        let dt = (freq_hz.abs() / sample_rate.max(1.0)).clamp(0.0, 0.5);
        let phi = self.phase;
        self.phase = (phi + dt).fract();

        let count = (partial_count as usize).min(MAX_PARTIALS);
        let mut sum = 0.0f32;
        for k in 1..=count {
            let harm_freq = (k as f32) * freq_hz.abs();
            if harm_freq >= nyquist {
                continue;
            }
            let amp = partials[k - 1];
            let ph = phases[k - 1];
            sum += amp * (TAU * (k as f32) * phi + ph).sin();
        }
        sum
    }
}

/// 8-Operator FM Matrix Synthesizer (`fm1..fm8`, `fm13`, and 2-op `fm`/`fmh` compatibility — `DSP-04`).
#[derive(Clone, Copy, Debug)]
pub struct FmMatrixSynthesizer {
    pub phases: [f32; MAX_FM_OPERATORS],
    pub prev_outputs: [f32; MAX_FM_OPERATORS],
}

impl Default for FmMatrixSynthesizer {
    fn default() -> Self {
        Self {
            phases: [0.0; MAX_FM_OPERATORS],
            prev_outputs: [0.0; MAX_FM_OPERATORS],
        }
    }
}

impl FmMatrixSynthesizer {
    /// Synthesizes one sample from either the 8x8 FM matrix or the 2-op `fm`/`fmh` carrier-modulator pair.
    pub fn next_sample(
        &mut self,
        carrier_freq_hz: f32,
        sample_rate: f32,
        fm_index: f32,
        fmh: f32,
        fm_env_val: f32,
        ops: &[FmOpParams; MAX_FM_OPERATORS],
        matrix: &[[f32; MAX_FM_OPERATORS]; MAX_FM_OPERATORS],
    ) -> f32 {
        let sr = sample_rate.max(1.0);
        let matrix_active = ops.iter().any(|o| o.level > 0.0)
            || matrix.iter().any(|row| row.iter().any(|&v| v != 0.0));

        if !matrix_active {
            let mod_freq = carrier_freq_hz * fmh.max(0.01);
            let mod_dt = (mod_freq / sr).fract();
            let car_dt = (carrier_freq_hz / sr).fract();
            let mod_sig = (self.phases[1] * TAU).sin();
            self.phases[1] = (self.phases[1] + mod_dt).fract();
            let beta = fm_index * fm_env_val;
            let out = (self.phases[0] * TAU + beta * mod_sig).sin();
            self.phases[0] = (self.phases[0] + car_dt).fract();
            return out;
        }

        let mut current_out = [0.0f32; MAX_FM_OPERATORS];
        let mut bus_sum = 0.0f32;
        for dst in 0..MAX_FM_OPERATORS {
            let mut phase_mod = 0.0f32;
            for src in 0..MAX_FM_OPERATORS {
                let idx_val = matrix[src][dst];
                if idx_val != 0.0 {
                    let mod_out = if src < dst {
                        current_out[src]
                    } else {
                        self.prev_outputs[src]
                    };
                    phase_mod += idx_val * mod_out;
                }
            }
            if ops[dst].feedback != 0.0 {
                phase_mod += ops[dst].feedback * self.prev_outputs[dst];
            }
            let op_freq = carrier_freq_hz * ops[dst].ratio.max(0.0);
            let dt = (op_freq / sr).fract();
            let sig = (self.phases[dst] * TAU + phase_mod).sin();
            self.phases[dst] = (self.phases[dst] + dt).fract();
            current_out[dst] = soft_limit_feedback(sig);
            bus_sum += current_out[dst] * ops[dst].level;
        }
        self.prev_outputs = current_out;
        bus_sum
    }
}

/// 20-parameter deterministic Frank Force ZZFX Synthesizer (`Section 4.4`, `DSP-05`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ZzfxSynthesizer {
    pub sample_index: u64,
    pub phase: f32,
    pub freq: f32,
    pub slide: f32,
    pub initialized: bool,
}

impl ZzfxSynthesizer {
    /// Generates one deterministic ZZFX sample (`DSP-05`).
    pub fn next_sample(&mut self, params: &ZzfxParams, sample_rate: f32) -> f32 {
        let sr = sample_rate.max(1.0);
        if !self.initialized {
            self.freq = params.freq;
            self.slide = params.slide;
            self.initialized = true;
        }
        let t = (self.sample_index as f32) / sr;
        self.sample_index += 1;

        let attack = params.attack.max(0.0001);
        let decay = params.decay.max(0.0001);
        let sustain_t = params.sustain_time.max(0.0);
        let release = params.release.max(0.0001);

        let env = if t < attack {
            t / attack
        } else if t < attack + decay {
            1.0 - (1.0 - params.sustain_vol) * ((t - attack) / decay)
        } else if t < attack + decay + sustain_t {
            params.sustain_vol
        } else if t < attack + decay + sustain_t + release {
            params.sustain_vol * (1.0 - (t - attack - decay - sustain_t) / release)
        } else {
            0.0
        };

        self.slide += params.delta_slide / sr;
        let mut f = self.freq + self.slide;
        if params.pitch_jump != 0.0 && t >= params.pitch_jump_time && params.pitch_jump_time > 0.0 {
            f += params.pitch_jump;
        }
        self.freq = f;
        self.phase = (self.phase + (f.abs() / sr)).fract();

        let raw = match params.shape {
            ZzfxShape::Sine => (self.phase * TAU).sin(),
            ZzfxShape::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
            ZzfxShape::Sawtooth => 2.0 * self.phase - 1.0,
            ZzfxShape::Tan => (self.phase * core::f32::consts::PI).tan().clamp(-1.0, 1.0),
            ZzfxShape::Noise => ((self.phase * 12_345.678).sin() * 43_758.547).fract() * 2.0 - 1.0,
            ZzfxShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        let trem = if params.tremolo > 0.0 && params.lfo > 0.0 {
            1.0 - params.tremolo * 0.5 * (1.0 + (TAU * t / params.lfo).sin())
        } else {
            1.0
        };
        sanitize_f32_finite(raw * env * params.gain * trem)
    }
}

/// Karplus-Strong plucked-string waveguide physical model (`Section 4.7`).
#[derive(Clone, Debug)]
pub struct KarplusStrongPluck {
    delay_line: [f32; 2048],
    write_pos: usize,
    length: usize,
    lp_state: f32,
    excited: bool,
}

impl Default for KarplusStrongPluck {
    fn default() -> Self {
        Self::new()
    }
}

impl KarplusStrongPluck {
    /// Creates a pre-allocated `KarplusStrongPluck` waveguide.
    pub const fn new() -> Self {
        Self {
            delay_line: [0.0; 2048],
            write_pos: 0,
            length: 100,
            lp_state: 0.0,
            excited: false,
        }
    }

    /// Excites and steps the plucked string by one sample.
    pub fn next_sample(&mut self, params: &KarplusStrongParams, sample_rate: f32) -> f32 {
        if !self.excited {
            let len = ((sample_rate / params.freq_hz.max(25.0)) as usize).clamp(4, 2047);
            self.length = len;
            let mut rng = 0xA5A5_5A5Au64;
            for i in 0..len {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let u = ((rng >> 40) as f32) / ((1u64 << 24) as f32) * 2.0 - 1.0;
                self.delay_line[i] = u * (core::f32::consts::PI * (i as f32) / (len as f32)).sin();
            }
            self.excited = true;
        }
        let read_pos = (self.write_pos + self.length - 1) % self.length;
        let delayed = self.delay_line[read_pos];
        let damp = params.damping.clamp(0.01, 0.99);
        self.lp_state = (1.0 - damp) * delayed + damp * self.lp_state;
        let next_val = soft_limit_feedback(self.lp_state * params.decay.clamp(0.9, 0.9999));
        self.delay_line[self.write_pos] = next_val;
        self.write_pos = (self.write_pos + 1) % self.length;
        delayed
    }
}

/// Zero-allocation Inline Modular `>>` DSP Pipe Graph (`CK-05`: `saw(freq) >> lpf(1200, 0.8) >> tanh(2.0)`)
/// with PolyBLEP anti-aliasing and `5 ms` (`240` frames at `48 kHz`) equal-power hot-swap crossfade.
#[derive(Clone, Copy, Debug)]
pub struct InlineDspPipeVoice {
    pub active_osc: PolyBlepOscillator,
    pub active_waveform: Waveform,
    pub active_lpf_cutoff: f32,
    pub active_lpf_q: f32,
    pub active_drive: f32,
    svf_ic1eq: f32,
    svf_ic2eq: f32,

    prev_waveform: Waveform,
    prev_lpf_cutoff: f32,
    prev_drive: f32,
    hotswap_frames_remaining: u16,
    hotswap_total_frames: u16,
}

impl Default for InlineDspPipeVoice {
    fn default() -> Self {
        Self::new(Waveform::Sawtooth, 1200.0, 0.8, 2.0)
    }
}

impl InlineDspPipeVoice {
    /// Creates a new `InlineDspPipeVoice`.
    pub const fn new(waveform: Waveform, lpf_cutoff: f32, lpf_q: f32, drive: f32) -> Self {
        Self {
            active_osc: PolyBlepOscillator::new(0.0),
            active_waveform: waveform,
            active_lpf_cutoff: lpf_cutoff,
            active_lpf_q: lpf_q,
            active_drive: drive,
            svf_ic1eq: 0.0,
            svf_ic2eq: 0.0,
            prev_waveform: waveform,
            prev_lpf_cutoff: lpf_cutoff,
            prev_drive: drive,
            hotswap_frames_remaining: 0,
            hotswap_total_frames: 240,
        }
    }

    /// Hot-swaps the inline pipe topology with a `5 ms` equal-power crossfade (`CK-05`).
    pub fn hot_swap(
        &mut self,
        new_waveform: Waveform,
        new_lpf_cutoff: f32,
        new_lpf_q: f32,
        new_drive: f32,
        sample_rate: f32,
    ) {
        self.prev_waveform = self.active_waveform;
        self.prev_lpf_cutoff = self.active_lpf_cutoff;
        self.prev_drive = self.active_drive;
        self.active_waveform = new_waveform;
        self.active_lpf_cutoff = new_lpf_cutoff;
        self.active_lpf_q = new_lpf_q;
        self.active_drive = new_drive;
        let frames = ((0.005 * sample_rate).round() as u16).max(16);
        self.hotswap_total_frames = frames;
        self.hotswap_frames_remaining = frames;
    }

    /// Steps the inline `>>` pipe by one sample.
    pub fn next_sample(&mut self, freq_hz: f32, sample_rate: f32) -> f32 {
        let raw_new = self
            .active_osc
            .next_sample(self.active_waveform, freq_hz, sample_rate);
        let g = (core::f32::consts::PI
            * (self.active_lpf_cutoff.clamp(20.0, 0.48 * sample_rate) / sample_rate))
            .tan();
        let k = 1.0 / self.active_lpf_q.max(0.5);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = raw_new - self.svf_ic2eq;
        let v1 = a1 * self.svf_ic1eq + a2 * v3;
        let v2 = self.svf_ic2eq + a2 * self.svf_ic1eq + a3 * v3;
        self.svf_ic1eq = soft_limit_feedback(2.0 * v1 - self.svf_ic1eq);
        self.svf_ic2eq = soft_limit_feedback(2.0 * v2 - self.svf_ic2eq);
        let shaped_new = (v2 * self.active_drive.max(0.1)).tanh();

        if self.hotswap_frames_remaining > 0 {
            let progress = 1.0
                - (self.hotswap_frames_remaining as f32) / (self.hotswap_total_frames as f32);
            self.hotswap_frames_remaining -= 1;
            let angle = progress * core::f32::consts::FRAC_PI_2;
            let old_est = (raw_new * self.prev_drive.max(0.1)).tanh();
            old_est * angle.cos() + shaped_new * angle.sin()
        } else {
            shaped_new
        }
    }
}
