//! Bjorklund Euclidean rhythms (`euclid`, `euclid_rot`, `euclid_inv`, `euclid_full`),
//! Strudel Stepwise combinators (`step`, `expand`, `pace`, `take_steps`, `drop_steps`, `stepcat`),
//! and sample-slicing / permutation combinators (`striate`, `chop`, `slice`, `splice`, `randslice`, `scramble`, `shuffle`, `rot`).

use crate::algebra::{lift_a2_left, PatternFunctor, PatternMonad};
use crate::combinators::{fastcat, silence, slowcat, stack};
use crate::pattern::{ControlPattern, Pattern};
use crate::rational::{Time, TimeExt};
use crate::signals::time_to_rand_seeded;
use crate::span::{Hap, TimeSpan};
use crate::value::{State, Value};
use alloc::string::ToString;
use alloc::vec::Vec;

/// Exact Bjorklund Euclidean rhythm generator returning a boolean step vector of length `steps`
/// with `pulses` evenly distributed onsets rotated by `rotation`.
pub fn bjorklund(pulses: isize, steps: isize, rotation: isize) -> Vec<bool> {
    if steps <= 0 {
        return Vec::new();
    }
    let n = steps as usize;
    if pulses <= 0 {
        return alloc::vec![false; n];
    }
    if pulses >= steps {
        return alloc::vec![true; n];
    }
    let k = pulses as isize;
    let mut pattern = Vec::with_capacity(n);
    for i in 0..steps {
        let hit = ((i * k).rem_euclid(steps)) < k;
        pattern.push(hit);
    }
    let rot = rotation.rem_euclid(steps) as usize;
    if rot != 0 {
        pattern.rotate_left(rot);
    }
    pattern
}

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Distributes `pulses` onsets across `steps` subdivisions using Bjorklund's algorithm (`euclid`).
    pub fn euclid(&self, pulses: isize, steps: isize) -> Self {
        self.euclid_rot(pulses, steps, 0)
    }

    /// Bjorklund Euclidean rhythm rotated by `rotation` steps (`euclidRot`).
    pub fn euclid_rot(&self, pulses: isize, steps: isize, rotation: isize) -> Self {
        let rhythm = bjorklund(pulses, steps, rotation);
        if rhythm.is_empty() {
            return silence();
        }
        let step_pats: Vec<Self> = rhythm
            .into_iter()
            .map(|hit| if hit { self.clone() } else { silence() })
            .collect();
        fastcat(step_pats)
    }

    /// Inverted Euclidean rhythm (`euclidInv`): plays on the rests of `euclid(pulses, steps)`.
    pub fn euclid_inv(&self, pulses: isize, steps: isize) -> Self {
        let rhythm = bjorklund(pulses, steps, 0);
        if rhythm.is_empty() {
            return silence();
        }
        let step_pats: Vec<Self> = rhythm
            .into_iter()
            .map(|hit| if !hit { self.clone() } else { silence() })
            .collect();
        fastcat(step_pats)
    }

    /// Alternates `self` on Euclidean hits and `other` on Euclidean rests (`euclidFull`).
    pub fn euclid_full(&self, pulses: isize, steps: isize, other: &Self) -> Self {
        let rhythm = bjorklund(pulses, steps, 0);
        if rhythm.is_empty() {
            return silence();
        }
        let step_pats: Vec<Self> = rhythm
            .into_iter()
            .map(|hit| if hit { self.clone() } else { other.clone() })
            .collect();
        fastcat(step_pats)
    }

    /// Intrinsic metrical step count in the first cycle `[0, 1)`.
    pub fn intrinsic_steps(&self) -> usize {
        self.first_cycle()
            .into_iter()
            .filter(|h| h.has_onset())
            .count()
            .max(1)
    }

    /// Strudel `step`: sets the playback rate to `steps_per_cycle` steps per cycle.
    pub fn step(&self, steps_per_cycle: Time) -> Self {
        if steps_per_cycle <= Time::zero() {
            return silence();
        }
        let intrinsic = Time::from_integer(self.intrinsic_steps() as i64);
        self.fast(steps_per_cycle / intrinsic)
    }

    /// Strudel `expand`: multiplies the temporal duration of each step by `factor` (`slow(factor)`).
    pub fn expand(&self, factor: Time) -> Self {
        self.slow(factor)
    }

    /// Strudel `pace`: adjusts playback speed so `target_steps` fit inside each cycle.
    pub fn pace(&self, target_steps: Time) -> Self {
        self.step(target_steps)
    }

    /// Strudel `take`: keeps the first `count` steps (or last `|count|` steps if negative) per cycle.
    pub fn take_steps(&self, count: isize) -> Self {
        if count == 0 {
            return silence();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let mut haps = pat.query(&state.with_span(sub));
                haps.sort_by(|a, b| a.part.begin.cmp(&b.part.begin));
                let n = haps.len();
                let k = count.unsigned_abs().min(n);
                if count > 0 {
                    out.extend(haps.into_iter().take(k));
                } else {
                    out.extend(haps.into_iter().skip(n - k));
                }
            }
            out
        })
    }

    /// Strudel `drop`: drops the first `count` steps (or last `|count|` steps if negative) per cycle.
    pub fn drop_steps(&self, count: isize) -> Self {
        if count == 0 {
            return self.clone();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let mut haps = pat.query(&state.with_span(sub));
                haps.sort_by(|a, b| a.part.begin.cmp(&b.part.begin));
                let n = haps.len();
                let k = count.unsigned_abs().min(n);
                if count > 0 {
                    out.extend(haps.into_iter().skip(k));
                } else {
                    out.extend(haps.into_iter().take(n - k));
                }
            }
            out
        })
    }

    /// Rotates step values within each cycle by `shift` positions (`rot`).
    pub fn rot(&self, shift: isize) -> Self {
        if shift == 0 {
            return self.clone();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc_span = TimeSpan::cycle_of(sub.begin);
                let mut full_haps = pat.query(&state.with_span(cyc_span));
                full_haps.sort_by(|a, b| a.part.begin.cmp(&b.part.begin));
                let len = full_haps.len();
                if len == 0 {
                    continue;
                }
                let values: Vec<T> = full_haps.iter().map(|h| h.value.clone()).collect();
                for (i, mut hap) in full_haps.into_iter().enumerate() {
                    let src_idx = (i as isize + shift).rem_euclid(len as isize) as usize;
                    hap.value = values[src_idx].clone();
                    if let Some(p) = sub.intersection(hap.part) {
                        hap.part = p;
                        out.push(hap);
                    }
                }
            }
            out
        })
    }
}

/// Concatenates patterns proportionally by their intrinsic metrical step counts (`stepcat`).
pub fn stepcat<T: Clone + Send + Sync + 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    let weighted: Vec<(Time, Pattern<T>)> = patterns
        .into_iter()
        .map(|p| {
            let w = Time::from_integer(p.intrinsic_steps() as i64);
            (w, p)
        })
        .collect();
    crate::combinators::timecat(weighted)
}

/// Polymeter: aligns multiple patterns to the step count of the first pattern (or `steps_override`).
pub fn polymeter<T: Clone + Send + Sync + 'static>(
    patterns: Vec<Pattern<T>>,
    steps_override: Option<usize>,
) -> Pattern<T> {
    if patterns.is_empty() {
        return silence();
    }
    let ref_steps = steps_override.unwrap_or_else(|| patterns[0].intrinsic_steps());
    let target = Time::from_integer(ref_steps.max(1) as i64);
    stack(patterns.into_iter().map(|p| p.step(target)).collect())
}

/// Polyrhythm: stacks patterns in parallel over the same cycle (`stack`).
pub fn polyrhythm<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    stack(patterns)
}

/// Sample-slicing and permutation combinators on `ControlPattern`.
pub trait SamplePatternOps {
    /// Cuts each sample into `n` parts and interleaves them across cycles (`striate`).
    fn striate(&self, n: usize) -> ControlPattern;
    /// Cuts each event into `n` granular sub-events with `begin`/`end` spans (`chop`).
    fn chop(&self, n: usize) -> ControlPattern;
    /// Selects slice `indices` out of `n` equal subdivisions of the sample (`slice`).
    fn slice(&self, n: usize, indices: &Pattern<i64>) -> ControlPattern;
    /// Like `slice`, but scales `speed` so each slice fits its event duration (`splice`).
    fn splice(&self, n: usize, indices: &Pattern<i64>) -> ControlPattern;
    /// Randomly selects one of `n` slices per cycle/event (`randslice`).
    fn randslice(&self, n: usize) -> ControlPattern;
    /// Subdivides cycle into `n` parts and randomly permutes them with replacement (`scramble`).
    fn scramble(&self, n: usize) -> ControlPattern;
    /// Subdivides cycle into `n` parts and randomly permutes them without replacement (`shuffle`).
    fn shuffle(&self, n: usize) -> ControlPattern;
}

impl SamplePatternOps for ControlPattern {
    fn striate(&self, n: usize) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let slices: Vec<ControlPattern> = (0..n)
            .map(|i| {
                let b = (i as f64) / (n as f64);
                let e = ((i + 1) as f64) / (n as f64);
                self.fmap(move |m| {
                    let mut next = m.clone();
                    next.insert("begin".to_string(), Value::Float(b));
                    next.insert("end".to_string(), Value::Float(e));
                    next
                })
            })
            .collect();
        slowcat(slices).fast(Time::from_integer(n as i64))
    }

    fn chop(&self, n: usize) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let pat = self.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            let count = n as i64;
            for hap in pat.query(state) {
                let w = hap.whole_or_part();
                let sub_dur = w.duration() / Time::from_integer(count);
                for k in 0..count {
                    let sb = w.begin + Time::from_integer(k) * sub_dur;
                    let se = sb + sub_dur;
                    if let Some(sub_whole) = TimeSpan::new(sb, se) {
                        if let Some(part) = hap.part.intersection(sub_whole) {
                            let mut m = hap.value.clone();
                            m.insert("begin".to_string(), Value::Float((k as f64) / (n as f64)));
                            m.insert(
                                "end".to_string(),
                                Value::Float(((k + 1) as f64) / (n as f64)),
                            );
                            out.push(Hap::discrete(sub_whole, part, m, hap.context.clone()));
                        }
                    }
                }
            }
            out
        })
    }

    fn slice(&self, n: usize, indices: &Pattern<i64>) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let n_i64 = n as i64;
        let slice_ctrl = indices.fmap(move |&idx| {
            let k = idx.rem_euclid(n_i64);
            let b = (k as f64) / (n as f64);
            let e = ((k + 1) as f64) / (n as f64);
            let mut m = crate::value::ControlMap::new();
            m.insert("begin".to_string(), Value::Float(b));
            m.insert("end".to_string(), Value::Float(e));
            m
        });
        lift_a2_left(&slice_ctrl, self, |s_map, base_map| {
            let mut out = base_map.clone();
            for (k, v) in s_map {
                out.insert(k.clone(), v.clone());
            }
            out
        })
    }

    fn splice(&self, n: usize, indices: &Pattern<i64>) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let sliced = self.slice(n, indices);
        let n_f64 = n as f64;
        Pattern::new(move |state: &State| {
            sliced
                .query(state)
                .into_iter()
                .map(|mut hap| {
                    let dur = hap.whole_or_part().duration().to_f64().max(1e-9);
                    let speed = 1.0 / (n_f64 * dur);
                    hap.value.insert("speed".to_string(), Value::Float(speed));
                    hap
                })
                .collect()
        })
    }

    fn randslice(&self, n: usize) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let idx_pat = crate::signals::irand(n as i64).segment(Time::one());
        self.slice(n, &idx_pat)
    }

    fn scramble(&self, n: usize) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let pat = self.clone();
        let count = n as i64;
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            let sub_dur = Time::new_const(1, count);
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                for k in 0..count {
                    let dst_begin = cyc + Time::from_integer(k) * sub_dur;
                    let dst_end = dst_begin + sub_dur;
                    let Some(dst_win) = TimeSpan::new(dst_begin, dst_end) else {
                        continue;
                    };
                    let Some(active) = sub.intersection(dst_win) else {
                        continue;
                    };
                    let r = time_to_rand_seeded(dst_begin, state.query_context.session_seed, k as u64);
                    let src_idx = ((r * (count as f64)) as i64).clamp(0, count - 1);
                    let src_begin = cyc + Time::from_integer(src_idx) * sub_dur;
                    let shift = dst_begin - src_begin;
                    let qspan = active.shift(-shift);
                    for hap in pat.query(&state.with_span(qspan)) {
                        out.push(hap.with_span(|s| s.shift(shift)));
                    }
                }
            }
            out
        })
    }

    fn shuffle(&self, n: usize) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let pat = self.clone();
        let count = n as i64;
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            let sub_dur = Time::new_const(1, count);
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                // Deterministic Fisher-Yates permutation per cycle
                let mut perm: Vec<i64> = (0..count).collect();
                for i in (1..n).rev() {
                    let r = time_to_rand_seeded(cyc, state.query_context.session_seed, i as u64);
                    let j = ((r * ((i + 1) as f64)) as usize).min(i);
                    perm.swap(i, j);
                }
                for (k, &src_idx) in perm.iter().enumerate() {
                    let dst_begin = cyc + Time::from_integer(k as i64) * sub_dur;
                    let dst_end = dst_begin + sub_dur;
                    let Some(dst_win) = TimeSpan::new(dst_begin, dst_end) else {
                        continue;
                    };
                    let Some(active) = sub.intersection(dst_win) else {
                        continue;
                    };
                    let src_begin = cyc + Time::from_integer(src_idx) * sub_dur;
                    let shift = dst_begin - src_begin;
                    let qspan = active.shift(-shift);
                    for hap in pat.query(&state.with_span(qspan)) {
                        out.push(hap.with_span(|s| s.shift(shift)));
                    }
                }
            }
            out
        })
    }
}
