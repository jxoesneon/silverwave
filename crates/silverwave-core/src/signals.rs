//! Deterministic time-hashed SplitMix64 PRNG (`time_to_rand`), stochastic combinators
//! (`choose`, `wchoose`, `choose_cycles`), and continuous signals (`rand`, `irand`,
//! $C^2$-continuous quintic `perlin`, `berlin`, `sine`, `cosine`, `saw`, `isaw`, `tri`, `square`).

use crate::algebra::PatternFunctor;
use crate::combinators::silence;
use crate::pattern::Pattern;
use crate::rational::{Time, TimeExt};
use crate::span::{Context, Hap};
use crate::value::State;
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::TAU;

/// SplitMix64 avalanche mixer producing uniform 64-bit hashes from rational time coordinates.
#[inline]
pub const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Deterministic seeded time-to-unit-interval hash in `[0.0, 1.0)`.
#[inline]
pub fn time_to_rand_seeded(t: Time, seed: u64, salt: u64) -> f64 {
    let n = t.numer() as u64;
    let d = t.denom() as u64;
    let h = splitmix64(n ^ splitmix64(d) ^ splitmix64(seed) ^ salt);
    // 53-bit mantissa in `[0.0, 1.0)`
    ((h >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

/// Deterministic time-to-unit-interval hash in `[0.0, 1.0)`.
/// Contract: Pure function of `Time`; uniform distribution over `[0.0, 1.0)`.
#[inline]
pub fn time_to_rand(t: Time) -> f64 {
    time_to_rand_seeded(t, 0, 0x517C_C1B7_2722_0A95)
}

/// Continuous random signal in `[0.0, 1.0)` (`whole == None`).
pub fn rand() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let mid = state.span.midpoint();
        let v = time_to_rand_seeded(mid, state.query_context.session_seed, 0x517C_C1B7_2722_0A95);
        vec![Hap::continuous(state.span, v, Context::empty())]
    })
}

/// Continuous integer random signal in `0..n` (`whole == None`).
pub fn irand(n: i64) -> Pattern<i64> {
    if n <= 0 {
        return silence();
    }
    rand().fmap(move |&r| ((r * (n as f64)) as i64).clamp(0, n - 1))
}

/// Continuous 1D smooth Perlin noise signal in `[0.0, 1.0]` (`whole == None`).
/// $C^2$-continuous across integer cycle boundaries using Ken Perlin's quintic curve $6t^5 - 15t^4 + 10t^3$.
pub fn perlin() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let t = state.span.midpoint();
        let c0 = t.sam();
        let c1 = t.next_sam();
        let u = t.cycle_pos().to_f64().clamp(0.0, 1.0);
        let fade = u * u * u * (u * (u * 6.0 - 15.0) + 10.0);
        let y0 = time_to_rand_seeded(c0, state.query_context.session_seed, 0xA24B_AED4_963E_E407);
        let y1 = time_to_rand_seeded(c1, state.query_context.session_seed, 0xA24B_AED4_963E_E407);
        let val = (y0 + fade * (y1 - y0)).clamp(0.0, 1.0);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Continuous Berlin noise signal in `[0.0, 1.0]` (`whole == None`).
/// Piecewise-linear interpolation between deterministic per-cycle random anchors.
pub fn berlin() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let t = state.span.midpoint();
        let c0 = t.sam();
        let c1 = t.next_sam();
        let u = t.cycle_pos().to_f64().clamp(0.0, 1.0);
        let y0 = time_to_rand_seeded(c0, state.query_context.session_seed, 0x9E37_79B9_7F4A_7C15);
        let y1 = time_to_rand_seeded(c1, state.query_context.session_seed, 0x9E37_79B9_7F4A_7C15);
        let val = (y0 + u * (y1 - y0)).clamp(0.0, 1.0);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Dependency-free, high-accuracy $\sin(x)$ using range reduction to $[-\pi, \pi]$ and 7-term minimax Taylor polynomial.
#[inline]
pub fn sin_f64(x: f64) -> f64 {
    let pi = core::f64::consts::PI;
    let mut r = x % TAU;
    if r > pi {
        r -= TAU;
    } else if r < -pi {
        r += TAU;
    }
    if r > core::f64::consts::FRAC_PI_2 {
        r = pi - r;
    } else if r < -core::f64::consts::FRAC_PI_2 {
        r = -pi - r;
    }
    let r2 = r * r;
    r * (1.0
        - r2 / 6.0
        + (r2 * r2) / 120.0
        - (r2 * r2 * r2) / 5040.0
        + (r2 * r2 * r2 * r2) / 362_880.0
        - (r2 * r2 * r2 * r2 * r2) / 39_916_800.0)
}

#[inline]
pub fn cos_f64(x: f64) -> f64 {
    sin_f64(x + core::f64::consts::FRAC_PI_2)
}

/// Continuous unipolar sine oscillator in `[0.0, 1.0]` (`whole == None`).
pub fn sine() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let phase = state.span.midpoint().cycle_pos().to_f64();
        let val = ((sin_f64(phase * TAU) + 1.0) * 0.5).clamp(0.0, 1.0);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Continuous unipolar cosine oscillator in `[0.0, 1.0]` (`whole == None`).
pub fn cosine() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let phase = state.span.midpoint().cycle_pos().to_f64();
        let val = ((cos_f64(phase * TAU) + 1.0) * 0.5).clamp(0.0, 1.0);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Continuous rising sawtooth signal in `[0.0, 1.0)` (`whole == None`).
pub fn saw() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let val = state.span.midpoint().cycle_pos().to_f64().clamp(0.0, 1.0);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Continuous inverted (falling) sawtooth signal in `(0.0, 1.0]` (`whole == None`).
pub fn isaw() -> Pattern<f64> {
    saw().fmap(|&v| (1.0 - v).clamp(0.0, 1.0))
}

/// Continuous triangle wave signal in `[0.0, 1.0]` (`whole == None`).
pub fn tri() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let p = state.span.midpoint().cycle_pos().to_f64();
        let val = if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 };
        vec![Hap::continuous(state.span, val.clamp(0.0, 1.0), Context::empty())]
    })
}

/// Continuous square wave signal (`1.0` for `[0, 0.5)`, `0.0` for `[0.5, 1.0)`).
pub fn square() -> Pattern<f64> {
    Pattern::new(|state: &State| {
        let p = state.span.midpoint().cycle_pos().to_f64();
        let val = if p < 0.5 { 1.0 } else { 0.0 };
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Uniformly chooses one item from `items` as a continuous random signal (`choose`).
pub fn choose<T: Clone + Send + Sync + 'static>(items: Vec<T>) -> Pattern<T> {
    if items.is_empty() {
        return silence();
    }
    let n = items.len();
    rand().fmap(move |&r| {
        let idx = ((r * (n as f64)) as usize).min(n - 1);
        items[idx].clone()
    })
}

/// Chooses one item per integer cycle deterministically (`chooseCycles`).
pub fn choose_cycles<T: Clone + Send + Sync + 'static>(items: Vec<T>) -> Pattern<T> {
    choose(items).segment(Time::one())
}

/// Weighted random choice (`wchoose`).
pub fn wchoose<T: Clone + Send + Sync + 'static>(weighted: Vec<(f64, T)>) -> Pattern<T> {
    let active: Vec<(f64, T)> = weighted.into_iter().filter(|(w, _)| *w > 0.0).collect();
    if active.is_empty() {
        return silence();
    }
    let total: f64 = active.iter().map(|(w, _)| *w).sum();
    rand().fmap(move |&r| {
        let target = r * total;
        let mut acc = 0.0;
        for (w, item) in &active {
            acc += *w;
            if target <= acc {
                return item.clone();
            }
        }
        active[active.len() - 1].1.clone()
    })
}

/// Scales a `[0.0, 1.0]` signal pattern into `[min, max]`.
pub trait SignalRange {
    /// Linearly maps `[0.0, 1.0]` into `[min, max]`.
    fn range(&self, min: f64, max: f64) -> Pattern<f64>;
    /// Exponentially maps `[0.0, 1.0]` into `[min, max]` (for positive frequency ranges).
    fn rangex(&self, min: f64, max: f64) -> Pattern<f64>;
    /// Quantizes signal values to steps of `1 / steps`.
    fn quantize(&self, steps: f64) -> Pattern<f64>;
    /// Applies symmetric temporal boxcar smoothing over `window` cycles.
    fn smooth(&self, window: Time) -> Pattern<f64>;
}

impl SignalRange for Pattern<f64> {
    fn range(&self, min: f64, max: f64) -> Pattern<f64> {
        self.fmap(move |&x| min + x * (max - min))
    }

    fn rangex(&self, min: f64, max: f64) -> Pattern<f64> {
        let safe_min = min.abs().max(1e-6);
        let safe_max = max.abs().max(1e-6);
        let ratio = safe_max / safe_min;
        self.fmap(move |&x| {
            // safe_min * (ratio ^ x) via base-2 log/exp
            let log2_ratio = ln_f64(ratio) / core::f64::consts::LN_2;
            safe_min * crate::value::exp2_f64(x * log2_ratio)
        })
    }

    fn quantize(&self, steps: f64) -> Pattern<f64> {
        if steps <= 0.0 {
            return self.clone();
        }
        self.fmap(move |&x| {
            let scaled = x * steps;
            let rounded = if scaled >= 0.0 {
                (scaled + 0.5) as i64 as f64
            } else {
                (scaled - 0.5) as i64 as f64
            };
            rounded / steps
        })
    }

    fn smooth(&self, window: Time) -> Pattern<f64> {
        if window <= Time::zero() {
            return self.clone();
        }
        let half = window / Time::from_integer(2);
        let a = self.early(half);
        let b = self.clone();
        let c = self.late(half);
        crate::algebra::lift_a2_both(
            &crate::algebra::lift_a2_both(&a, &b, |&va, &vb| va + vb),
            &c,
            |&vab, &vc| (vab + vc) / 3.0,
        )
    }
}

/// Natural logarithm helper for `rangex` and `ScalaTuning`.
#[inline]
pub fn ln_f64(x: f64) -> f64 {
    if x <= 0.0 {
        return -1e9;
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let m = f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52));
    // Pade/series for ln(m) where m in [1, 2)
    let y = (m - 1.0) / (m + 1.0);
    let y2 = y * y;
    let mut term = y;
    let mut sum = 0.0;
    let mut k = 1;
    while k <= 19 {
        sum += term / (k as f64);
        term *= y2;
        k += 2;
    }
    2.0 * sum + (exp as f64) * core::f64::consts::LN_2
}
