//! ADSR Envelopes, Zero-Delay-Feedback SVF (`12db` & `24db`), 4-Pole Nonlinear Moog Ladder Filter,
//! and 15-Vowel Formant Filter Bank (`03-SPEC-AUDIO-DSP-AND-SAMPLES.md` Section 5.1, `DSP-06`).

use crate::events::{soft_limit_feedback, FilterTopology, VowelFormant};
use core::f32::consts::PI;

/// Sample-accurate ADSR envelope generator (`Section 2.2` & `Section 5.1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrEnvelope {
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain_level: f32,
    pub release_s: f32,
    pub current_frame: u64,
    pub note_off_frame: u64,
    pub release_start_level: f32,
}

impl Default for AdsrEnvelope {
    fn default() -> Self {
        Self::new(0.002, 0.05, 0.8, 0.02, 4800)
    }
}

impl AdsrEnvelope {
    /// Creates a new `AdsrEnvelope` with `note_off_frame` duration.
    pub const fn new(
        attack_s: f32,
        decay_s: f32,
        sustain_level: f32,
        release_s: f32,
        note_off_frame: u64,
    ) -> Self {
        Self {
            attack_s,
            decay_s,
            sustain_level,
            release_s,
            current_frame: 0,
            note_off_frame,
            release_start_level: sustain_level,
        }
    }

    /// Evaluates the envelope at the current sample and advances `current_frame`.
    #[inline]
    pub fn next_value(&mut self, sample_rate: f32) -> (f32, bool) {
        let sr = sample_rate.max(1.0);
        let f = self.current_frame;
        self.current_frame += 1;

        let att_frames = ((self.attack_s.max(0.0001) * sr) as u64).max(1);
        let dec_frames = ((self.decay_s.max(0.0001) * sr) as u64).max(1);
        let rel_frames = ((self.release_s.max(0.0001) * sr) as u64).max(1);
        let sus = self.sustain_level.clamp(0.0, 1.0);

        if f < self.note_off_frame {
            let val = if f < att_frames {
                (f as f32) / (att_frames as f32)
            } else if f < att_frames + dec_frames {
                let d_prog = ((f - att_frames) as f32) / (dec_frames as f32);
                1.0 - (1.0 - sus) * d_prog
            } else {
                sus
            };
            self.release_start_level = val;
            (val, false)
        } else {
            let r_elapsed = f - self.note_off_frame;
            if r_elapsed >= rel_frames {
                (0.0, true)
            } else {
                let val = self.release_start_level * (1.0 - (r_elapsed as f32) / (rel_frames as f32));
                (val.max(0.0), false)
            }
        }
    }
}

/// Filter pass mode for `ZdfSvfFilter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterPassMode {
    LowPass,
    HighPass,
    BandPass,
    Notch,
}

/// Cytomic Zero-Delay-Feedback State Variable Filter (`12db` 2-pole or `24db` cascaded 4-pole)
/// and 4-pole nonlinear Huovilainen Moog Ladder filter (`Section 5.1`, `DSP-06`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MultiModeFilter {
    ic1eq_a: f32,
    ic2eq_a: f32,
    ic1eq_b: f32,
    ic2eq_b: f32,
    ladder_stage: [f32; 4],
}

impl MultiModeFilter {
    /// Creates a zeroed `MultiModeFilter`.
    pub const fn new() -> Self {
        Self {
            ic1eq_a: 0.0,
            ic2eq_a: 0.0,
            ic1eq_b: 0.0,
            ic2eq_b: 0.0,
            ladder_stage: [0.0; 4],
        }
    }

    #[inline]
    fn process_svf_2pole(
        x: f32,
        g: f32,
        k: f32,
        mode: FilterPassMode,
        ic1eq: &mut f32,
        ic2eq: &mut f32,
    ) -> f32 {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - *ic2eq;
        let v1 = a1 * (*ic1eq) + a2 * v3;
        let v2 = *ic2eq + a2 * (*ic1eq) + a3 * v3;
        *ic1eq = soft_limit_feedback(2.0 * v1 - *ic1eq);
        *ic2eq = soft_limit_feedback(2.0 * v2 - *ic2eq);
        match mode {
            FilterPassMode::LowPass => v2,
            FilterPassMode::BandPass => v1,
            FilterPassMode::HighPass => x - k * v1 - v2,
            FilterPassMode::Notch => x - k * v1,
        }
    }

    /// Processes a single sample through `topology` (`12db`, `24db`, or `MoogLadder`) and `mode`.
    pub fn process_sample(
        &mut self,
        x: f32,
        cutoff_hz: f32,
        q: f32,
        sample_rate: f32,
        topology: FilterTopology,
        mode: FilterPassMode,
    ) -> f32 {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.clamp(20.0, 0.48 * sr);

        if topology == FilterTopology::MoogLadder && mode == FilterPassMode::LowPass {
            let f = (2.0 * (PI * fc / sr).sin()).clamp(0.0, 0.95);
            let res = ((q - 0.5) / 10.0).clamp(0.0, 0.95) * 3.8;
            let input = (x - res * self.ladder_stage[3]).tanh();
            self.ladder_stage[0] =
                soft_limit_feedback(self.ladder_stage[0] + f * (input - self.ladder_stage[0]));
            for i in 1..4 {
                self.ladder_stage[i] = soft_limit_feedback(
                    self.ladder_stage[i]
                        + f * (self.ladder_stage[i - 1] - self.ladder_stage[i]),
                );
            }
            return self.ladder_stage[3];
        }

        let g = (PI * fc / sr).tan();
        let k = (1.0 / q.max(0.5)).clamp(0.05, 2.0);
        let stage1 =
            Self::process_svf_2pole(x, g, k, mode, &mut self.ic1eq_a, &mut self.ic2eq_a);
        if topology == FilterTopology::Svf24dB {
            Self::process_svf_2pole(stage1, g, k, mode, &mut self.ic1eq_b, &mut self.ic2eq_b)
        } else {
            stage1
        }
    }
}

/// 3-Formant `[F1_Hz, F2_Hz, F3_Hz]` table for all 15 Strudel/SuperDirt vowels (`Section 5.1`, `DSP-06`).
pub fn vowel_formant_frequencies(vowel: VowelFormant) -> [f32; 3] {
    match vowel {
        VowelFormant::None => [500.0, 1500.0, 2500.0],
        VowelFormant::A => [800.0, 1150.0, 2900.0],
        VowelFormant::E => [350.0, 2000.0, 2800.0],
        VowelFormant::I => [270.0, 2140.0, 2950.0],
        VowelFormant::O => [450.0, 800.0, 2830.0],
        VowelFormant::U => [325.0, 700.0, 2700.0],
        VowelFormant::Ae => [660.0, 1720.0, 2410.0],
        VowelFormant::Aa => [730.0, 1090.0, 2440.0],
        VowelFormant::Oe => [500.0, 1400.0, 2300.0],
        VowelFormant::Ue => [300.0, 1650.0, 2250.0],
        VowelFormant::Y => [280.0, 1750.0, 2200.0],
        VowelFormant::Uh => [520.0, 1190.0, 2390.0],
        VowelFormant::Un => [480.0, 1100.0, 2450.0],
        VowelFormant::En => [550.0, 1600.0, 2550.0],
        VowelFormant::An => [700.0, 1150.0, 2500.0],
        VowelFormant::On => [460.0, 850.0, 2600.0],
    }
}

/// Parallel 3-band formant filter bank supporting discrete `VowelFormant` and continuous morphing (`Section 5.1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct VowelFormantFilter {
    bands: [MultiModeFilter; 3],
}

impl VowelFormantFilter {
    /// Creates a new `VowelFormantFilter`.
    pub const fn new() -> Self {
        Self {
            bands: [MultiModeFilter::new(); 3],
        }
    }

    /// Processes a sample through the 3 parallel formant resonators for `vowel`.
    pub fn process_vowel(&mut self, x: f32, vowel: VowelFormant, sample_rate: f32) -> f32 {
        if vowel == VowelFormant::None {
            return x;
        }
        let freqs = vowel_formant_frequencies(vowel);
        let gains = [1.0f32, 0.65, 0.35];
        let qs = [10.0f32, 12.0, 14.0];
        let mut out = 0.0f32;
        for i in 0..3 {
            out += gains[i]
                * self.bands[i].process_sample(
                    x,
                    freqs[i],
                    qs[i],
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::BandPass,
                );
        }
        out
    }

    /// Processes a sample using a continuous `morph` parameter in `[0.0, 4.0]` (`A -> E -> I -> O -> U`).
    pub fn process_morph(&mut self, x: f32, morph: f32, sample_rate: f32) -> f32 {
        let anchors = [
            VowelFormant::A,
            VowelFormant::E,
            VowelFormant::I,
            VowelFormant::O,
            VowelFormant::U,
        ];
        let clamped = morph.clamp(0.0, 4.0);
        let idx = (clamped.floor() as usize).min(3);
        let frac = clamped - (idx as f32);
        let f_a = vowel_formant_frequencies(anchors[idx]);
        let f_b = vowel_formant_frequencies(anchors[idx + 1]);
        let gains = [1.0f32, 0.65, 0.35];
        let qs = [10.0f32, 12.0, 14.0];
        let mut out = 0.0f32;
        for i in 0..3 {
            let fc = f_a[i] + (f_b[i] - f_a[i]) * frac;
            out += gains[i]
                * self.bands[i].process_sample(
                    x,
                    fc,
                    qs[i],
                    sample_rate,
                    FilterTopology::Svf12dB,
                    FilterPassMode::BandPass,
                );
        }
        out
    }
}
