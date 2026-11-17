//! Category-King & Pain-Point Extensions:
//! - `CK-01`: Elektron Conditional Trigs (`TrigCondition`: `@1:4`, `@1st`, `@fill`, `@pre`, `@nei`)
//! - `CK-02`: Step-Indexed Parameter Locks (`.plock()`)
//! - `CK-04`: Dirtywave M8 Multi-Rate Sub-Tables (`.table()`) & Groove Swing (`.groove()`)
//! - `PP-08`: Zero-Safe `arrange`, `seq_p`, `once`, `asap`, `qtrigger`, `fade_in`, `fade_out`,
//!   and all 11 pure pattern-level Tidal transitions.

use crate::algebra::{PatternFunctor, lift_a2_left};
use crate::combinators::{ControlPatternOps, silence, stack};
use crate::pattern::{ControlPattern, Pattern};
use crate::rational::{Rational64, Time, TimeExt};
use crate::signals::cos_f64;
use crate::span::TimeSpan;
use crate::value::{State, Value};
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::FRAC_PI_2;

/// `[CK-01]` Elektron-grade conditional trig predicates evaluated per step via `StatefulPattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrigCondition {
    /// Fires on cycle `a` of every `b` cycles (`@a:b`, e.g. `@1:4`, 1-indexed `a in 1..=b`).
    Ratio { a: u16, b: u16 },
    /// Fires only on (`true`) or after (`false`) the first cycle of stream activation (`@1st` / `@!1st`).
    FirstCycle { expected: bool },
    /// Fires only when performance FILL mode is active (`true`) or inactive (`false`) (`@fill` / `@!fill`).
    FillLatch { active: bool },
    /// Fires only if the most recent conditional trig on the SAME stream evaluated to `expected` (`@pre` / `@!pre`).
    PreviousConditionalOnSameStream { expected: bool },
    /// Fires only if the most recent conditional trig on the NEIGHBOR stream (`d_{k-1}`) evaluated to `expected` (`@nei` / `@!nei`).
    PreviousConditionalOnNeighborStream { expected: bool },
}

/// Causal step-evaluation ledger matching Elektron Digitakt II / Syntakt truth tables (`CK-01`).
/// Note: Unconditional steps do NOT overwrite `last_same_stream_cond`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrigConditionLedger {
    pub activation_cycle: i64,
    pub fill_latch: bool,
    pub last_same_stream_cond: bool,
    pub last_neighbor_stream_cond: bool,
}

impl TrigCondition {
    /// Evaluates `self` at `cycle_index` and updates `ledger.last_same_stream_cond`.
    pub fn evaluate(&self, cycle_index: i64, ledger: &mut TrigConditionLedger) -> bool {
        let result = match *self {
            TrigCondition::Ratio { a, b } => {
                if b == 0 {
                    false
                } else {
                    let rel = (cycle_index - ledger.activation_cycle).rem_euclid(b as i64) + 1;
                    rel == (a as i64)
                }
            }
            TrigCondition::FirstCycle { expected } => {
                let is_first = cycle_index == ledger.activation_cycle;
                is_first == expected
            }
            TrigCondition::FillLatch { active } => ledger.fill_latch == active,
            TrigCondition::PreviousConditionalOnSameStream { expected } => {
                ledger.last_same_stream_cond == expected
            }
            TrigCondition::PreviousConditionalOnNeighborStream { expected } => {
                ledger.last_neighbor_stream_cond == expected
            }
        };
        ledger.last_same_stream_cond = result;
        result
    }
}

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Attaches an Elektron conditional trig (`CK-01`) to `self`.
    pub fn trig_cond(&self, cond: TrigCondition, activation_cycle: i64) -> Self {
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam().numer();
                let mut ledger = TrigConditionLedger {
                    activation_cycle,
                    fill_latch: state.query_context.fill_active,
                    last_same_stream_cond: false,
                    last_neighbor_stream_cond: false,
                };
                if cond.evaluate(cyc, &mut ledger) {
                    out.extend(pat.query(&state.with_span(sub)));
                }
            }
            out
        })
    }
}

impl ControlPattern {
    /// `[CK-02]` Step-indexed parameter lock (`.plock(step_idx, total_steps, patch)`):
    /// Overrides control parameters strictly on step `step_idx` (`0..total_steps`) without
    /// splitting or mutating adjacent steps.
    pub fn plock(&self, step_idx: u16, total_steps: u16, patch: &ControlPattern) -> ControlPattern {
        if total_steps == 0 || step_idx >= total_steps {
            return self.clone();
        }
        let base = self.clone();
        let patch = patch.clone();
        let step_dur = Time::new_const(1, total_steps as i64);
        let step_offset = Time::from_integer(step_idx as i64) * step_dur;

        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                let lock_win =
                    TimeSpan::new_clamped(cyc + step_offset, cyc + step_offset + step_dur);
                for mut hap in base.query(&state.with_span(sub)) {
                    let onset = hap.whole_or_part().begin;
                    if lock_win.contains_time(onset) {
                        for p_hap in patch.query(&state.with_span(hap.part)) {
                            for (k, v) in p_hap.value {
                                hap.value.insert(k, v);
                            }
                        }
                    }
                    out.push(hap);
                }
            }
            out
        })
    }

    /// `[CK-04]` Dirtywave M8 multi-rate sub-table (`.table(tick_rate, sub_pattern)`):
    /// Attaches an independent sub-pattern that restarts at `t = 0` on every note onset
    /// and steps at `tick_rate` subdivisions per cycle.
    pub fn table(&self, tick_rate: Rational64, sub_pattern: &ControlPattern) -> ControlPattern {
        if tick_rate <= Time::zero() {
            return self.clone();
        }
        let intrinsic = sub_pattern.intrinsic_steps().max(1) as i64;
        let fast_sub = sub_pattern.fast(tick_rate / Time::from_integer(intrinsic));
        let nested = self.fmap(move |base_map| {
            let bmap = base_map.clone();
            fast_sub.fmap(move |sub_map| {
                let mut merged = bmap.clone();
                for (k, v) in sub_map {
                    merged.insert(k.clone(), v.clone());
                }
                merged
            })
        });
        <ControlPattern as crate::algebra::PatternTrigJoin<crate::value::ControlMap>>::trig_zero_join(&nested)
    }
}

impl<T: Clone + 'static> Pattern<T> {
    /// `[CK-04]` Dirtywave M8 hex groove swing table (`.groove(&[u8])`):
    /// Warps equal cycle subdivisions onto proportional tick weights (e.g. `&[7, 5, 7, 5]`).
    pub fn groove(&self, tick_weights: &[u8]) -> Self {
        let weights: Vec<i64> = tick_weights
            .iter()
            .copied()
            .filter(|&w| w > 0)
            .map(|w| w as i64)
            .collect();
        if weights.is_empty() {
            return self.clone();
        }
        let n = weights.len() as i64;
        let total_ticks: i64 = weights.iter().sum();
        let mut boundaries = Vec::with_capacity(weights.len() + 1);
        let mut acc = 0i64;
        boundaries.push(Time::zero());
        for &w in &weights {
            acc += w;
            boundaries.push(Time::new_const(acc, total_ticks));
        }

        let pat = self.clone();
        Self::new(move |state: &State| {
            let map_pos = |pos: Time| -> Time {
                let scaled = pos * Time::from_integer(n);
                let idx = (scaled.floor_i64().clamp(0, n - 1)) as usize;
                let frac = scaled - Time::from_integer(idx as i64);
                let b0 = boundaries[idx];
                let b1 = boundaries[idx + 1];
                b0 + frac * (b1 - b0)
            };
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                for hap in pat.query(&state.with_span(sub)) {
                    let mapped = hap.with_span(|s| TimeSpan {
                        begin: cyc + map_pos(s.begin - cyc),
                        end: cyc + map_pos(s.end - cyc),
                    });
                    out.push(mapped);
                }
            }
            out
        })
    }
}

/// `[PP-08]` Zero-division-safe linear song arrangement (`arrange`).
/// Sections whose cycle count evaluates to `<= 0` are cleanly skipped. If all sections
/// have `cycles <= 0`, returns `silence()`.
pub fn arrange<T: Clone + 'static>(sections: Vec<(Time, Pattern<T>)>) -> Pattern<T> {
    let active: Vec<(Time, Pattern<T>)> = sections
        .into_iter()
        .filter(|(c, _)| *c > Time::zero())
        .collect();
    if active.is_empty() {
        return silence();
    }
    // `active` is non-empty and every section passed the `c > 0` filter, so `total > 0`.
    let total: Time = active.iter().fold(Time::zero(), |acc, (c, _)| acc + *c);
    crate::combinators::timecat(active).slow(total)
}

/// `[PP-08]` Plays `(start_cycle, end_cycle, pattern)` blocks along a linear timeline (`seqP`).
pub fn seq_p<T: Clone + 'static>(blocks: Vec<(Time, Time, Pattern<T>)>) -> Pattern<T> {
    Pattern::new(move |state: &State| {
        let mut out = Vec::new();
        for (b, e, pat) in &blocks {
            if let Some(win) = TimeSpan::new(*b, *e)
                && let Some(active) = state.span.intersection(win)
            {
                let shifted = pat.late(*b);
                out.extend(shifted.query(&state.with_span(active)));
            }
        }
        out
    })
}

/// `[PP-08]` Plays `pat` for 1 cycle starting immediately at fractional cycle `now` (`asap`).
pub fn asap<T: Clone + 'static>(pat: &Pattern<T>, now: Time) -> Pattern<T> {
    seq_p(vec![(now, now + Time::one(), pat.clone())])
}

/// `[PP-08]` Plays `pat` for 1 cycle quantized to the next cycle boundary (`once`).
pub fn once<T: Clone + 'static>(pat: &Pattern<T>, now: Time) -> Pattern<T> {
    let start = if now.cycle_pos() == Time::zero() {
        now
    } else {
        now.next_sam()
    };
    seq_p(vec![(start, start + Time::one(), pat.clone())])
}

/// `[PP-08]` Aligns pattern local cycle `0` to `activation_cycle` (`qtrigger`).
pub fn qtrigger<T: Clone + 'static>(pat: &Pattern<T>, activation_cycle: Time) -> Pattern<T> {
    let start = activation_cycle.sam();
    let shifted = pat.late(start);
    Pattern::new(move |state: &State| {
        if state.span.end <= start {
            return Vec::new();
        }
        let begin = state.span.begin.max(start);
        let Some(active) = TimeSpan::new(begin, state.span.end) else {
            return Vec::new();
        };
        shifted.query(&state.with_span(active))
    })
}

impl ControlPattern {
    /// `[PP-08]` One-shot equal-power cosine fade-in over `cycles` starting at `start_cycle`.
    pub fn fade_in(&self, start_cycle: Time, cycles: Time) -> ControlPattern {
        if cycles <= Time::zero() {
            return self.clone();
        }
        let pat = self.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for mut hap in pat.query(state) {
                let t = hap.whole_or_part().begin;
                if t < start_cycle {
                    continue;
                }
                let u = ((t - start_cycle) / cycles).to_f64().clamp(0.0, 1.0);
                let env = cos_f64((1.0 - u) * FRAC_PI_2).clamp(0.0, 1.0);
                let g = hap.value.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
                hap.value.insert("gain".to_string(), Value::Float(g * env));
                out.push(hap);
            }
            out
        })
    }

    /// `[PP-08]` One-shot equal-power cosine fade-out over `cycles` starting at `start_cycle`,
    /// automatically silencing the stream once `t >= start_cycle + cycles`.
    pub fn fade_out(&self, start_cycle: Time, cycles: Time) -> ControlPattern {
        if cycles <= Time::zero() {
            return silence();
        }
        let pat = self.clone();
        let end_cycle = start_cycle + cycles;
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for mut hap in pat.query(state) {
                let t = hap.whole_or_part().begin;
                if t >= end_cycle {
                    continue;
                }
                let env = if t <= start_cycle {
                    1.0
                } else {
                    let u = ((t - start_cycle) / cycles).to_f64().clamp(0.0, 1.0);
                    cos_f64(u * FRAC_PI_2).clamp(0.0, 1.0)
                };
                let g = hap.value.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
                hap.value.insert("gain".to_string(), Value::Float(g * env));
                out.push(hap);
            }
            out
        })
    }
}

/// All 11 pure pattern-level TidalCycles transitions (`PP-08`, resolving Tidal #716).
pub mod transitions {
    use super::*;

    /// Equal-power crossfade from `old` to `new` over `cycles` starting at `start_cycle` (`xfadeIn`).
    pub fn xfade_in(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles: Time,
    ) -> ControlPattern {
        stack(vec![
            old.fade_out(start_cycle, cycles),
            new.fade_in(start_cycle, cycles),
        ])
    }

    /// Equal-power 4-cycle crossfade (`xfade`).
    pub fn xfade(old: &ControlPattern, new: &ControlPattern, start_cycle: Time) -> ControlPattern {
        xfade_in(old, new, start_cycle, Time::from_integer(4))
    }

    /// Instant cutover (`jump`) at `start_cycle`.
    pub fn jump(old: &ControlPattern, new: &ControlPattern, start_cycle: Time) -> ControlPattern {
        jump_in(old, new, start_cycle, 0)
    }

    /// Cutover after `cycles_delay` integer cycles (`jumpIn`).
    pub fn jump_in(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles_delay: i64,
    ) -> ControlPattern {
        let cut = start_cycle.sam() + Time::from_integer(cycles_delay.max(0));
        let old_p = old.clone();
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                if sub.begin < cut {
                    out.extend(old_p.query(&state.with_span(sub)));
                } else {
                    out.extend(new_p.query(&state.with_span(sub)));
                }
            }
            out
        })
    }

    /// Cutover on the next cycle where `cycle % modulus == 0` (`jumpMod`).
    pub fn jump_mod(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        modulus: i64,
    ) -> ControlPattern {
        let m = modulus.max(1);
        let cur = start_cycle.sam().numer();
        let rem = cur.rem_euclid(m);
        let delay = if rem == 0 { 0 } else { m - rem };
        jump_in(old, new, start_cycle, delay)
    }

    /// Probabilistic event-by-event cross-degrade over `cycles` (`clutchIn`).
    pub fn clutch_in(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles: Time,
    ) -> ControlPattern {
        let dur = cycles.max(Time::one());
        let old_p = old.clone();
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let u = ((sub.begin - start_cycle) / dur).to_f64().clamp(0.0, 1.0);
                let s = state.with_span(sub);
                if u <= 0.0 {
                    out.extend(old_p.query(&s));
                } else if u >= 1.0 {
                    out.extend(new_p.query(&s));
                } else {
                    out.extend(old_p.degrade_by(u).query(&s));
                    out.extend(new_p.undegrade_by(u).query(&s));
                }
            }
            out
        })
    }

    /// 2-cycle probabilistic clutch transition (`clutch`).
    pub fn clutch(old: &ControlPattern, new: &ControlPattern, start_cycle: Time) -> ControlPattern {
        clutch_in(old, new, start_cycle, Time::from_integer(2))
    }

    /// Combines low-pass filter sweep buildup on `old` before dropping into `new` (`anticipateIn`).
    pub fn anticipate_in(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles: Time,
    ) -> ControlPattern {
        let dur = cycles.max(Time::one());
        let cut = start_cycle + dur;
        let old_p = old.clone();
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let s = state.with_span(sub);
                if sub.begin >= cut {
                    out.extend(new_p.query(&s));
                } else if sub.begin < start_cycle {
                    out.extend(old_p.query(&s));
                } else {
                    let u = ((sub.begin - start_cycle) / dur).to_f64().clamp(0.0, 1.0);
                    for mut h in old_p.query(&s) {
                        h.value
                            .insert("hpf".to_string(), Value::Float(100.0 + u * 2400.0));
                        out.push(h);
                    }
                }
            }
            out
        })
    }

    /// 8-cycle build-up transition (`anticipate`).
    pub fn anticipate(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
    ) -> ControlPattern {
        anticipate_in(old, new, start_cycle, Time::from_integer(8))
    }

    /// Linearly interpolates numeric control parameters from `old` to `new` over `cycles` (`interpolate`).
    pub fn interpolate(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles: Time,
    ) -> ControlPattern {
        let dur = cycles.max(Time::one());
        let old_p = old.clone();
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let u = ((sub.begin - start_cycle) / dur).to_f64().clamp(0.0, 1.0);
                let blended = lift_a2_left(&new_p, &old_p, move |nmap, omap| {
                    let mut m = nmap.clone();
                    for (k, v_new) in nmap {
                        if let (Some(fn_val), Some(fo_val)) =
                            (v_new.as_f64(), omap.get(k).and_then(Value::as_f64))
                        {
                            m.insert(k.clone(), Value::Float(fo_val + u * (fn_val - fo_val)));
                        }
                    }
                    m
                });
                out.extend(blended.query(&state.with_span(sub)));
            }
            out
        })
    }

    /// Pans historical patterns across stereo stage (`histpan`).
    pub fn histpan(history: Vec<ControlPattern>, n: usize) -> ControlPattern {
        let count = n.max(1).min(history.len());
        if count == 0 {
            return silence();
        }
        let layers: Vec<ControlPattern> = history
            .into_iter()
            .take(count)
            .enumerate()
            .map(|(i, pat)| {
                let p = if count == 1 {
                    0.5
                } else {
                    (i as f64) / ((count - 1) as f64)
                };
                pat.union_right(&crate::combinators::pan(p))
            })
            .collect();
        stack(layers)
    }

    /// Silences stream for `wait_cycles` cycles before activating `new` (`wait`).
    pub fn wait(new: &ControlPattern, start_cycle: Time, wait_cycles: Time) -> ControlPattern {
        let cut = start_cycle + wait_cycles.max(Time::zero());
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                if sub.begin >= cut {
                    out.extend(new_p.query(&state.with_span(sub)));
                }
            }
            out
        })
    }

    /// Applies a temporary FX transformation `fx` to `old` for `cycles` before cutting to `new` (`wash`).
    pub fn wash<F>(
        old: &ControlPattern,
        new: &ControlPattern,
        start_cycle: Time,
        cycles: Time,
        fx: F,
    ) -> ControlPattern
    where
        F: FnOnce(&ControlPattern) -> ControlPattern,
    {
        let washed = fx(old);
        let cut = start_cycle + cycles.max(Time::zero());
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let s = state.with_span(sub);
                if sub.begin < cut {
                    out.extend(washed.query(&s));
                } else {
                    out.extend(new_p.query(&s));
                }
            }
            out
        })
    }

    /// Plays `new` for `lifespan_cycles` while degrading it to silence over `degrade_cycles` (`mortal`).
    pub fn mortal(
        new: &ControlPattern,
        start_cycle: Time,
        lifespan_cycles: Time,
        degrade_cycles: Time,
    ) -> ControlPattern {
        let deg_start = start_cycle + lifespan_cycles.max(Time::zero());
        let deg_end = deg_start + degrade_cycles.max(Time::one());
        let new_p = new.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                if sub.begin >= deg_end {
                    continue;
                }
                let s = state.with_span(sub);
                if sub.begin < deg_start {
                    out.extend(new_p.query(&s));
                } else {
                    let u = ((sub.begin - deg_start) / (deg_end - deg_start))
                        .to_f64()
                        .clamp(0.0, 1.0);
                    out.extend(new_p.degrade_by(u).query(&s));
                }
            }
            out
        })
    }
}

/// Keeps StereoJux imported and re-exported for transition ergonomics.
pub use crate::combinators::StereoJux as _;
