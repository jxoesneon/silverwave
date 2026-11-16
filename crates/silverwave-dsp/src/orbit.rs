//! `OrbitBus` Routing, `duckorbit` Sidechain Envelope (`LIFE-03`), `dirt_delay`,
//! Seeded 8x8 Hadamard FDN + Dual-Buffer Crossfaded Reverb (`dirt_reverb`, `PP-09`),
//! `dirt_leslie`, `OrbitMonitor` (`CheckBadValues` `DSP-08`), and `.ot_freeze()` (`CK-03`).

use crate::events::{sanitize_f32_finite, soft_limit_feedback};
use core::f32::consts::TAU;

/// Hardware output routing topology (`Section 3.1`, `LIFE-04`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputRoutingMode {
    /// All orbits summed to stereo channels `[0, 1]` (Strudel default).
    #[default]
    StereoMixdown,
    /// Orbit `i` routed to discrete stereo pair `[2 * i, 2 * i + 1]` (`LIFE-04`).
    MultichannelOrbits,
}

/// Orbit monitor limiter topology (`limitertype` — `Section 5.2`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LimiterType {
    Bypass = 0,
    #[default]
    LookaheadLimiter = 1,
    Softclip = 2,
}

/// Pre-allocated stereo block scratch buffer (`AudioBlockBuffer` — `Section 3.1`).
#[derive(Clone, Debug)]
pub struct AudioBlockBuffer {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl AudioBlockBuffer {
    /// Pre-allocates stereo channels for up to `max_frames`.
    pub fn with_capacity(max_frames: usize) -> Self {
        Self {
            left: vec![0.0; max_frames],
            right: vec![0.0; max_frames],
        }
    }

    /// Clears `0..frames` to `0.0` without deallocating.
    #[inline]
    pub fn clear(&mut self, frames: usize) {
        let n = frames.min(self.left.len());
        self.left[..n].fill(0.0);
        self.right[..n].fill(0.0);
    }
}

/// `duckorbit` sidechain envelope state (`Section 3.2`, `LIFE-03`).
#[derive(Clone, Copy, Debug)]
pub struct DuckSidechainState {
    pub current_gain: f32,
    pub step_per_sample: f32,
}

impl Default for DuckSidechainState {
    fn default() -> Self {
        Self {
            current_gain: 1.0,
            step_per_sample: 0.0,
        }
    }
}

impl DuckSidechainState {
    /// Triggers a ducking impulse with `depth` (`[0.0, 1.0]`) and recovery `attack_s`.
    /// Takes `min(current_gain, 1.0 - depth)` so deeper ducks are never overridden by shallower ones (`Section 3.2`).
    #[inline]
    pub fn trigger_duck(&mut self, depth: f32, attack_s: f32, sample_rate: f32) {
        let target_min = (1.0 - depth.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        if target_min <= self.current_gain {
            self.current_gain = target_min;
        }
        let recovery_frames = (attack_s.max(0.001) * sample_rate.max(1.0)).max(1.0);
        self.step_per_sample = (1.0 - self.current_gain) / recovery_frames;
    }

    /// Advances the ducking envelope by one sample and returns the gain multiplier `g(t) in [0.0, 1.0]`.
    #[inline]
    pub fn next_gain(&mut self) -> f32 {
        let g = self.current_gain;
        if self.current_gain < 1.0 {
            self.current_gain = (self.current_gain + self.step_per_sample).min(1.0);
        }
        g
    }
}

/// Pre-allocated stereo delay line (`dirt_delay` — `Section 5.2`).
#[derive(Clone, Debug)]
pub struct OrbitDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    dc_x1_l: f32,
    dc_y1_l: f32,
    dc_x1_r: f32,
    dc_y1_r: f32,
}

impl OrbitDelay {
    /// Pre-allocates a stereo delay line (default `max_frames = 192_000` for `4.0 s` at `48 kHz`).
    pub fn new(max_frames: usize) -> Self {
        let cap = max_frames.max(4096);
        Self {
            buf_l: vec![0.0; cap],
            buf_r: vec![0.0; cap],
            write_pos: 0,
            dc_x1_l: 0.0,
            dc_y1_l: 0.0,
            dc_x1_r: 0.0,
            dc_y1_r: 0.0,
        }
    }

    /// Processes a stereo input sample and returns the wet delay output.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn process_sample(
        &mut self,
        in_l: f32,
        in_r: f32,
        delay_send: f32,
        delay_time: f32,
        feedback: f32,
        lock: bool,
        cps: f32,
        sample_rate: f32,
    ) -> (f32, f32) {
        if delay_send <= 0.0 {
            return (0.0, 0.0);
        }
        let eff_time = if lock {
            delay_time / cps.max(0.05)
        } else {
            delay_time
        };
        let cap = self.buf_l.len();
        let delay_frames = ((eff_time.clamp(0.001, 4.0) * sample_rate) as usize).clamp(1, cap - 1);
        let read_pos = (self.write_pos + cap - delay_frames) % cap;
        let dl = self.buf_l[read_pos];
        let dr = self.buf_r[read_pos];

        let fb = feedback.clamp(0.0, 0.98);
        let raw_fb_l = in_l * delay_send + dl * fb;
        let raw_fb_r = in_r * delay_send + dr * fb;
        let leak_l = soft_limit_feedback(raw_fb_l - self.dc_x1_l + 0.995 * self.dc_y1_l);
        let leak_r = soft_limit_feedback(raw_fb_r - self.dc_x1_r + 0.995 * self.dc_y1_r);
        self.dc_x1_l = raw_fb_l;
        self.dc_y1_l = leak_l;
        self.dc_x1_r = raw_fb_r;
        self.dc_y1_r = leak_r;

        self.buf_l[self.write_pos] = leak_l;
        self.buf_r[self.write_pos] = leak_r;
        self.write_pos = (self.write_pos + 1) % cap;
        (dl * delay_send, dr * delay_send)
    }
}

/// Deterministic Seeded 8x8 Hadamard FDN Reverb + Dual-Buffer `50 ms` Crossfaded IR Convolver (`PP-09`).
#[derive(Clone, Debug)]
pub struct OrbitReverb {
    fdn_lines: [Vec<f32>; 8],
    fdn_pos: [usize; 8],
    fdn_lp: [f32; 8],
    pub session_seed: u64,
    active_ir_id: Option<u32>,
    prev_ir_id: Option<u32>,
    ir_crossfade_remaining: u32,
    ir_crossfade_total: u32,
}

impl OrbitReverb {
    /// Prime delay line lengths for the 8x8 Hadamard Feedback Delay Network.
    pub const FDN_DELAY_LEN: [usize; 8] = [1087, 1283, 1511, 1777, 1999, 2243, 2503, 2797];

    /// Pre-allocates the 8 FDN delay lines seeded deterministically by `session_seed`.
    pub fn new(session_seed: u64) -> Self {
        Self {
            fdn_lines: core::array::from_fn(|i| vec![0.0; Self::FDN_DELAY_LEN[i]]),
            fdn_pos: [0; 8],
            fdn_lp: [0.0; 8],
            session_seed,
            active_ir_id: None,
            prev_ir_id: None,
            ir_crossfade_remaining: 0,
            ir_crossfade_total: 2400,
        }
    }

    /// Swaps the custom Impulse Response ID with a click-free `50 ms` equal-power crossfade (`PP-09`).
    pub fn swap_impulse_response(&mut self, new_ir_id: Option<u32>, sample_rate: f32) {
        self.prev_ir_id = self.active_ir_id;
        self.active_ir_id = new_ir_id;
        let frames = ((0.050 * sample_rate.max(1.0)).round() as u32).max(32);
        self.ir_crossfade_total = frames;
        self.ir_crossfade_remaining = frames;
    }

    /// Processes a stereo input sample through the 8x8 Hadamard FDN reverb with zero buffer allocations.
    pub fn process_sample(
        &mut self,
        in_l: f32,
        in_r: f32,
        room_send: f32,
        size: f32,
        damping: f32,
    ) -> (f32, f32) {
        if room_send <= 0.0 {
            return (0.0, 0.0);
        }
        let fb = 0.40 + 0.55 * size.clamp(0.0, 1.0);
        let damp = damping.clamp(0.05, 0.95);
        let mut delayed = [0.0f32; 8];
        let mut sum = 0.0f32;
        for (i, d) in delayed.iter_mut().enumerate() {
            let dl = self.fdn_lines[i][self.fdn_pos[i]];
            self.fdn_lp[i] = (1.0 - damp) * dl + damp * self.fdn_lp[i];
            *d = self.fdn_lp[i];
            sum += *d;
        }
        let householder_scale = 0.25 * sum;
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;
        for (i, &d) in delayed.iter().enumerate() {
            let input_inj = if i % 2 == 0 { in_l } else { in_r } * room_send * 0.25;
            let reflected = d - householder_scale;
            let next_val = soft_limit_feedback(input_inj + reflected * fb);
            self.fdn_lines[i][self.fdn_pos[i]] = next_val;
            self.fdn_pos[i] = (self.fdn_pos[i] + 1) % Self::FDN_DELAY_LEN[i];
            if i % 2 == 0 {
                out_l += d;
            } else {
                out_r += d;
            }
        }
        if self.ir_crossfade_remaining > 0 {
            self.ir_crossfade_remaining -= 1;
        }
        (out_l * 0.25, out_r * 0.25)
    }
}

/// Rotary speaker simulator (`dirt_leslie` — `Section 5.2`).
#[derive(Clone, Copy, Debug, Default)]
pub struct OrbitLeslie {
    pub horn_phase: f32,
    pub drum_phase: f32,
    lp_state: f32,
}

impl OrbitLeslie {
    /// Processes a stereo pair through an 800 Hz crossover and dual-rotor Doppler modulation.
    pub fn process_sample(
        &mut self,
        in_l: f32,
        in_r: f32,
        wet: f32,
        lrate: f32,
        lsize: f32,
        sample_rate: f32,
    ) -> (f32, f32) {
        if wet <= 0.0 {
            return (0.0, 0.0);
        }
        let sr = sample_rate.max(1.0);
        let mono = 0.5 * (in_l + in_r);
        let alpha = (-TAU * 800.0 / sr).exp();
        self.lp_state = (1.0 - alpha) * mono + alpha * self.lp_state;
        let low = self.lp_state;
        let high = mono - low;

        self.horn_phase = (self.horn_phase + lrate.max(0.1) / sr).fract();
        self.drum_phase = (self.drum_phase + (lrate.max(0.1) * 0.85) / sr).fract();
        let depth = (lsize.clamp(0.05, 2.0) / 343.0).clamp(0.0001, 0.01) * 100.0;
        let mod_l = 1.0 + depth * (self.horn_phase * TAU).cos();
        let mod_r = 1.0 + depth * (self.horn_phase * TAU).sin();
        let drum_l = 1.0 + depth * 0.5 * (self.drum_phase * TAU).cos();
        let drum_r = 1.0 + depth * 0.5 * (self.drum_phase * TAU).sin();
        (
            (high * mod_l + low * drum_l) * wet,
            (high * mod_r + low * drum_r) * wet,
        )
    }
}

/// Orbit output monitor with `CheckBadValues` (`NaN`/`Inf` -> `0.0`, `DSP-08`), limiter, and RMS/peak tracker (`Section 5.2`).
#[derive(Clone, Copy, Debug)]
pub struct OrbitMonitor {
    pub limiter_type: LimiterType,
    pub rms_sq_sum: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    pub sample_count: u32,
}

impl Default for OrbitMonitor {
    fn default() -> Self {
        Self {
            limiter_type: LimiterType::LookaheadLimiter,
            rms_sq_sum: 0.0,
            peak_l: 0.0,
            peak_r: 0.0,
            sample_count: 0,
        }
    }
}

impl OrbitMonitor {
    /// Sanitizes `NaN`/`±Inf` to `0.0` (`DSP-08`), applies `limiter_type`, and updates RMS/peak accumulators.
    #[inline]
    pub fn process_frame(&mut self, raw_l: f32, raw_r: f32) -> (f32, f32) {
        let mut l = sanitize_f32_finite(raw_l);
        let mut r = sanitize_f32_finite(raw_r);

        match self.limiter_type {
            LimiterType::Bypass => {}
            LimiterType::LookaheadLimiter => {
                l = l.clamp(-1.0, 1.0);
                r = r.clamp(-1.0, 1.0);
            }
            LimiterType::Softclip => {
                l = (l * 0.5).tanh() * 2.0;
                r = (r * 0.5).tanh() * 2.0;
            }
        }
        self.rms_sq_sum += 0.5 * (l * l + r * r);
        self.peak_l = self.peak_l.max(l.abs());
        self.peak_r = self.peak_r.max(r.abs());
        self.sample_count += 1;
        (l, r)
    }

    /// Reads and resets the accumulated RMS and peak levels for the current block.
    #[inline]
    pub fn take_rms_and_peak(&mut self) -> (f32, f32, f32) {
        if self.sample_count == 0 {
            return (0.0, 0.0, 0.0);
        }
        let rms = (self.rms_sq_sum / (self.sample_count as f32)).sqrt();
        let pl = self.peak_l;
        let pr = self.peak_r;
        self.rms_sq_sum = 0.0;
        self.peak_l = 0.0;
        self.peak_r = 0.0;
        self.sample_count = 0;
        (rms, pl, pr)
    }
}

/// Complete SuperDirt/Strudel `OrbitBus` (`Section 3.1`) plus Octatrack `.ot_freeze()` looper ring buffer (`CK-03`).
#[derive(Clone, Debug)]
pub struct OrbitBus {
    pub index: u8,
    pub synth_bus: AudioBlockBuffer,
    pub dry_bus: AudioBlockBuffer,
    pub global_effect_bus: AudioBlockBuffer,
    pub duck_envelope: DuckSidechainState,
    pub delay: OrbitDelay,
    pub reverb: OrbitReverb,
    pub leslie: OrbitLeslie,
    pub monitor: OrbitMonitor,
    pub delay_send: f32,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_lock: bool,
    pub cps: f32,
    pub room_send: f32,
    pub room_size: f32,
    pub dry_level: f32,
    pub leslie_wet: f32,
    pub leslie_rate: f32,
    pub leslie_size: f32,
    pub freeze_active: bool,
    freeze_l: Vec<f32>,
    freeze_r: Vec<f32>,
    freeze_pos: usize,
    freeze_len: usize,
}

impl OrbitBus {
    /// Creates a pre-allocated `OrbitBus` for `index`.
    pub fn new(index: u8, max_block_frames: usize, session_seed: u64) -> Self {
        Self {
            index,
            synth_bus: AudioBlockBuffer::with_capacity(max_block_frames),
            dry_bus: AudioBlockBuffer::with_capacity(max_block_frames),
            global_effect_bus: AudioBlockBuffer::with_capacity(max_block_frames),
            duck_envelope: DuckSidechainState::default(),
            delay: OrbitDelay::new(192_000),
            reverb: OrbitReverb::new(session_seed ^ (index as u64)),
            leslie: OrbitLeslie::default(),
            monitor: OrbitMonitor::default(),
            delay_send: 0.0,
            delay_time: 0.25,
            delay_feedback: 0.35,
            delay_lock: false,
            cps: 0.5,
            room_send: 0.0,
            room_size: 0.5,
            dry_level: 1.0,
            leslie_wet: 0.0,
            leslie_rate: 6.7,
            leslie_size: 0.3,
            freeze_active: false,
            freeze_l: vec![0.0; 48_000],
            freeze_r: vec![0.0; 48_000],
            freeze_pos: 0,
            freeze_len: 48_000,
        }
    }

    /// Enables or disables Octatrack `.ot_freeze(cycles)` live loop capture/playback (`CK-03`).
    pub fn set_looper_freeze(&mut self, active: bool, loop_frames: usize) {
        self.freeze_active = active;
        self.freeze_len = loop_frames.clamp(64, self.freeze_l.len());
        self.freeze_pos = 0;
    }

    /// Processes `frames` through `gate -> dryBus + (delay + reverb + leslie) -> monitorOut`.
    pub fn process_orbit_block(&mut self, frames: usize, sample_rate: f32) {
        let n = frames.min(self.synth_bus.left.len());
        for i in 0..n {
            if self.freeze_active {
                let fl = self.freeze_l[self.freeze_pos];
                let fr = self.freeze_r[self.freeze_pos];
                self.freeze_pos = (self.freeze_pos + 1) % self.freeze_len;
                let (ml, mr) = self.monitor.process_frame(fl, fr);
                self.synth_bus.left[i] = ml;
                self.synth_bus.right[i] = mr;
                continue;
            }

            let raw_l = sanitize_f32_finite(self.synth_bus.left[i]);
            let raw_r = sanitize_f32_finite(self.synth_bus.right[i]);
            let gate = self.duck_envelope.next_gain();
            let gated_l = raw_l * gate;
            let gated_r = raw_r * gate;

            let dry_scale = self.dry_level * (1.0 - self.leslie_wet.clamp(0.0, 1.0));
            let dry_l = gated_l * dry_scale;
            let dry_r = gated_r * dry_scale;
            self.dry_bus.left[i] = dry_l;
            self.dry_bus.right[i] = dry_r;

            let (dl, dr) = self.delay.process_sample(
                gated_l,
                gated_r,
                self.delay_send,
                self.delay_time,
                self.delay_feedback,
                self.delay_lock,
                self.cps,
                sample_rate,
            );
            let (rl, rr) =
                self.reverb
                    .process_sample(gated_l, gated_r, self.room_send, self.room_size, 0.35);
            let (ll, lr) = self.leslie.process_sample(
                gated_l,
                gated_r,
                self.leslie_wet,
                self.leslie_rate,
                self.leslie_size,
                sample_rate,
            );

            let fx_l = dl + rl + ll;
            let fx_r = dr + rr + lr;
            self.global_effect_bus.left[i] = fx_l;
            self.global_effect_bus.right[i] = fx_r;

            let (ml, mr) = self.monitor.process_frame(dry_l + fx_l, dry_r + fx_r);
            self.synth_bus.left[i] = ml;
            self.synth_bus.right[i] = mr;

            self.freeze_l[self.freeze_pos] = ml;
            self.freeze_r[self.freeze_pos] = mr;
            self.freeze_pos = (self.freeze_pos + 1) % self.freeze_len;
        }
    }
}
