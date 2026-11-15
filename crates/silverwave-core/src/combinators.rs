//! Complete 1:1 Strudel + TidalCycles pattern constructors, time modifiers,
//! conditional/structural combinators, stereo juxtaposition, and `ControlPatternOps`.

use crate::algebra::{lift_a2_both, lift_a2_left, lift_a2_right, PatternFunctor, PatternMonad};
use crate::pattern::{ControlPattern, IntoPattern, Pattern};
use crate::rational::{Time, TimeExt};
use crate::signals::time_to_rand;
use crate::span::{Context, Hap, TimeSpan};
use crate::value::{ControlMap, NoteValue, State, Value};
use alloc::string::{String, ToString};
use alloc::sync::Arc as StdArc;
use alloc::vec;
use alloc::vec::Vec;

/// Returns a discrete pattern repeating `value` once per integer cycle `[n, n + 1)`.
pub fn pure<T: Clone + Send + Sync + 'static>(value: T) -> Pattern<T> {
    Pattern::new(move |state: &State| {
        state
            .span
            .split_cycles()
            .into_iter()
            .map(|sub| {
                let whole = TimeSpan::cycle_of(sub.begin);
                Hap::discrete(whole, sub, value.clone(), Context::empty())
            })
            .collect()
    })
}

/// Returns the empty pattern yielding `vec![]` for every query.
pub fn silence<T: 'static>() -> Pattern<T> {
    Pattern::new(|_| Vec::new())
}

/// Overlays all patterns simultaneously in time, concatenating their `Hap`s.
pub fn stack<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    if patterns.is_empty() {
        return silence();
    }
    Pattern::new(move |state: &State| {
        let mut out = Vec::new();
        for p in &patterns {
            out.extend(p.query(state));
        }
        out
    })
}

/// Plays `patterns` sequentially, one pattern per cycle, cycling modulo `patterns.len()`.
pub fn slowcat<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    if patterns.is_empty() {
        return silence();
    }
    Pattern::new(move |state: &State| {
        let n = patterns.len() as i64;
        let mut out = Vec::new();
        for sub in state.span.split_cycles() {
            let cyc = sub.begin.sam().numer();
            let idx = cyc.rem_euclid(n) as usize;
            let pat_cyc = cyc.div_euclid(n);
            let offset = Time::from_integer(cyc - pat_cyc);
            let shifted_span = sub.shift(-offset);
            let sub_state = state.with_span(shifted_span);
            for hap in patterns[idx].query(&sub_state) {
                out.push(hap.with_span(|s| s.shift(offset)));
            }
        }
        out
    })
}

/// Alias for `slowcat` (`cat` in Strudel/Tidal).
#[inline]
pub fn cat<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    slowcat(patterns)
}

/// TidalCycles `slowcatPrime`: queries each selected pattern at the original cycle time `c`.
pub fn slowcat_prime<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    if patterns.is_empty() {
        return silence();
    }
    Pattern::new(move |state: &State| {
        let n = patterns.len() as i64;
        let mut out = Vec::new();
        for sub in state.span.split_cycles() {
            let cyc = sub.begin.sam().numer();
            let idx = cyc.rem_euclid(n) as usize;
            let sub_state = state.with_span(sub);
            out.extend(patterns[idx].query(&sub_state));
        }
        out
    })
}

/// Concatenates all `patterns` within a single cycle (`slowcat(patterns).fast(patterns.len())`).
pub fn fastcat<T: Clone + 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    if patterns.is_empty() {
        return silence();
    }
    let len = Time::from_integer(patterns.len() as i64);
    slowcat(patterns).fast(len)
}

/// Alias for `fastcat` (`sequence` in Tidal).
#[inline]
pub fn sequence<T: Clone + 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T> {
    fastcat(patterns)
}

/// Converts a list of values into a single-cycle equal subdivision pattern (`fromList`).
pub fn from_list<T: Clone + Send + Sync + 'static>(items: Vec<T>) -> Pattern<T> {
    fastcat(items.into_iter().map(pure).collect())
}

/// Concatenates weighted `(duration, pattern)` pairs within a single cycle.
pub fn timecat<T: Clone + 'static>(weighted: Vec<(Time, Pattern<T>)>) -> Pattern<T> {
    let active: Vec<(Time, Pattern<T>)> = weighted
        .into_iter()
        .filter(|(w, _)| *w > Time::zero())
        .collect();
    if active.is_empty() {
        return silence();
    }
    let total: Time = active.iter().fold(Time::zero(), |acc, (w, _)| acc + *w);
    if total <= Time::zero() {
        return silence();
    }
    let mut segments = Vec::with_capacity(active.len());
    let mut cursor = Time::zero();
    for (w, pat) in active {
        let begin = cursor / total;
        let end = (cursor + w) / total;
        cursor = cursor + w;
        segments.push(pat.compress(begin, end));
    }
    stack(segments)
}

/// Generates `[0, 1, ..., n - 1]` subdivided evenly across each cycle (`run`).
pub fn run(n: i64) -> Pattern<i64> {
    if n <= 0 {
        return silence();
    }
    from_list((0..n).collect())
}

/// Generates `[0..1]`, `[0..2]`, ..., `[0..n]` across consecutive cycles (`scan`).
pub fn scan(n: i64) -> Pattern<i64> {
    if n <= 0 {
        return silence();
    }
    slowcat((1..=n).map(run).collect())
}

impl<T: Clone + 'static> Pattern<T> {
    /// Speeds up the pattern by `factor`. If `factor < 0`, reverses and speeds up by `|factor|`.
    pub fn fast(&self, factor: Time) -> Self {
        if factor == Time::zero() {
            return silence();
        }
        if factor < Time::zero() {
            return self.rev().fast(factor.abs());
        }
        let inv = Time::one() / factor;
        self.with_query_time(move |t| t * factor)
            .with_hap_time(move |t| t * inv)
    }

    /// Patterned `fast` accepting a `Pattern<Time>` (or any `IntoPattern<Time>`).
    pub fn fast_pat<P: IntoPattern<Time>>(&self, factor: P) -> Self {
        let base = self.clone();
        let fpat = factor.into_pattern();
        let nested = fpat.fmap(move |&f| base.fast(f));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Slows down the pattern by `factor` (`fast(1 / factor)`).
    pub fn slow(&self, factor: Time) -> Self {
        if factor == Time::zero() {
            return silence();
        }
        self.fast(Time::one() / factor)
    }

    /// Patterned `slow` accepting a `Pattern<Time>` (or any `IntoPattern<Time>`).
    pub fn slow_pat<P: IntoPattern<Time>>(&self, factor: P) -> Self {
        let base = self.clone();
        let fpat = factor.into_pattern();
        let nested = fpat.fmap(move |&f| base.slow(f));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Reverses the order of events within each integer cycle `[n, n + 1)`.
    pub fn rev(&self) -> Self {
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                let mirror = |t: Time| cyc + Time::one() - (t - cyc);
                let reflected_query = TimeSpan::new_clamped(mirror(sub.end), mirror(sub.begin));
                let sub_state = state.with_span(reflected_query);
                for hap in pat.query(&sub_state) {
                    let mapped_whole = hap
                        .whole
                        .map(|w| TimeSpan::new_clamped(mirror(w.end), mirror(w.begin)));
                    let mapped_part = TimeSpan::new_clamped(mirror(hap.part.end), mirror(hap.part.begin));
                    if let Some(part) = sub.intersection(mapped_part) {
                        out.push(Hap {
                            whole: mapped_whole,
                            part,
                            value: hap.value,
                            context: hap.context,
                        });
                    }
                }
            }
            out
        })
    }

    /// Alternates normal and reversed cycles (`every(2, |p| p.rev())`).
    pub fn palindrome(&self) -> Self {
        self.every(2, |p| p.rev())
    }

    /// Shifts the pattern earlier in time by `offset` cycles (`<~`).
    pub fn early(&self, offset: Time) -> Self {
        self.with_query_time(move |t| t + offset)
            .with_hap_time(move |t| t - offset)
    }

    /// Patterned `early`.
    pub fn early_pat<P: IntoPattern<Time>>(&self, offset: P) -> Self {
        let base = self.clone();
        let opat = offset.into_pattern();
        let nested = opat.fmap(move |&o| base.early(o));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Shifts the pattern later in time by `offset` cycles (`~>`).
    pub fn late(&self, offset: Time) -> Self {
        self.early(-offset)
    }

    /// Patterned `late`.
    pub fn late_pat<P: IntoPattern<Time>>(&self, offset: P) -> Self {
        let base = self.clone();
        let opat = offset.into_pattern();
        let nested = opat.fmap(move |&o| base.late(o));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Rotates the pattern start by `1 / n` of a cycle each cycle (`iter`).
    pub fn iter(&self, n: i64) -> Self {
        if n <= 0 {
            return silence();
        }
        let shifts: Vec<Self> = (0..n)
            .map(|i| self.early(Time::new_const(i, n)))
            .collect();
        slowcat_prime(shifts)
    }

    /// Rotates the pattern start backwards by `1 / n` of a cycle each cycle (`iterBack` / `iter'`).
    pub fn iter_back(&self, n: i64) -> Self {
        if n <= 0 {
            return silence();
        }
        let shifts: Vec<Self> = (0..n)
            .map(|i| self.late(Time::new_const(i, n)))
            .collect();
        slowcat_prime(shifts)
    }

    /// Samples a (possibly continuous) pattern at `n` evenly spaced discrete steps per cycle.
    pub fn segment(&self, n: Time) -> Self {
        if n <= Time::zero() {
            return silence();
        }
        let pat = self.clone();
        let step_dur = Time::one() / n;
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                let steps = n.floor_i64().max(1);
                for k in 0..steps {
                    let sb = cyc + Time::from_integer(k) * step_dur;
                    let se = (sb + step_dur).min(cyc + Time::one());
                    let Some(step_whole) = TimeSpan::new(sb, se) else {
                        continue;
                    };
                    let Some(step_part) = sub.intersection(step_whole) else {
                        continue;
                    };
                    let mid = step_whole.midpoint();
                    let sample_span = TimeSpan::new_clamped(mid, mid + Time::new_const(1, 1_000_000));
                    let mut sample_state = state.with_span(sample_span);
                    sample_state.query_context.global_span = sample_span;
                    for h in pat.query(&sample_state) {
                        out.push(Hap::discrete(step_whole, step_part, h.value, h.context));
                    }
                }
            }
            out
        })
    }

    /// Compresses each cycle of `self` into `[begin, end)` within the cycle (`0 <= begin < end <= 1`).
    pub fn compress(&self, begin: Time, end: Time) -> Self {
        if begin < Time::zero() || end > Time::one() || begin >= end {
            return silence();
        }
        let dur = end - begin;
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                let Some(win) = TimeSpan::new(cyc + begin, cyc + end) else {
                    continue;
                };
                let Some(active) = sub.intersection(win) else {
                    continue;
                };
                let qb = cyc + (active.begin - win.begin) / dur;
                let qe = cyc + (active.end - win.begin) / dur;
                let Some(qspan) = TimeSpan::new(qb, qe) else {
                    continue;
                };
                for hap in pat.query(&state.with_span(qspan)) {
                    let map_t = |t: Time| cyc + begin + (t - cyc) * dur;
                    let mapped = hap.with_span(|s| s.with_time(map_t));
                    if let Some(p) = active.intersection(mapped.part) {
                        out.push(Hap {
                            whole: mapped.whole,
                            part: p,
                            value: mapped.value,
                            context: mapped.context,
                        });
                    }
                }
            }
            out
        })
    }

    /// Zooms into `[begin, end)` of each cycle, expanding it to fill the full cycle `[0, 1)`.
    pub fn zoom(&self, begin: Time, end: Time) -> Self {
        if begin < Time::zero() || end > Time::one() || begin >= end {
            return silence();
        }
        let dur = end - begin;
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                let qb = cyc + begin + (sub.begin - cyc) * dur;
                let qe = cyc + begin + (sub.end - cyc) * dur;
                let Some(qspan) = TimeSpan::new(qb, qe) else {
                    continue;
                };
                for hap in pat.query(&state.with_span(qspan)) {
                    let map_t = |t: Time| cyc + (t - (cyc + begin)) / dur;
                    let mapped = hap.with_span(|s| s.with_time(map_t));
                    if let Some(p) = sub.intersection(mapped.part) {
                        out.push(Hap {
                            whole: mapped.whole,
                            part: p,
                            value: mapped.value,
                            context: mapped.context,
                        });
                    }
                }
            }
            out
        })
    }

    /// Applies `f` inside cycles scaled by `factor`: `self.fast(factor)` -> `f` -> `.slow(factor)`.
    pub fn inside<F>(&self, factor: Time, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        if factor == Time::zero() {
            return silence();
        }
        f(&self.fast(factor)).slow(factor)
    }

    /// Applies `f` outside cycles scaled by `factor`: `self.slow(factor)` -> `f` -> `.fast(factor)`.
    pub fn outside<F>(&self, factor: Time, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        if factor == Time::zero() {
            return silence();
        }
        f(&self.slow(factor)).fast(factor)
    }

    /// Applies `f` only within the sub-cycle window `[begin, end)` of each cycle (`within`).
    pub fn within<F>(&self, begin: Time, end: Time, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        if begin >= end || begin < Time::zero() || end > Time::one() {
            return self.clone();
        }
        let transformed = f(self);
        let orig = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam();
                if begin > Time::zero() {
                    if let Some(pre) = TimeSpan::new(cyc, cyc + begin).and_then(|w| sub.intersection(w)) {
                        out.extend(orig.query(&state.with_span(pre)));
                    }
                }
                if let Some(mid) = TimeSpan::new(cyc + begin, cyc + end).and_then(|w| sub.intersection(w)) {
                    out.extend(transformed.query(&state.with_span(mid)));
                }
                if end < Time::one() {
                    if let Some(post) = TimeSpan::new(cyc + end, cyc + Time::one()).and_then(|w| sub.intersection(w)) {
                        out.extend(orig.query(&state.with_span(post)));
                    }
                }
            }
            out
        })
    }

    /// Loops the first `fraction` of each cycle across the entire cycle.
    pub fn linger(&self, fraction: Time) -> Self {
        if fraction <= Time::zero() {
            return silence();
        }
        self.zoom(Time::zero(), fraction.min(Time::one())).fast(Time::one() / fraction)
    }

    /// Delays every second subdivision of `n` per cycle by `offset` (`0 <= offset <= 1`).
    pub fn swing_by(&self, offset: Time, n: Time) -> Self {
        if n <= Time::zero() {
            return self.clone();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let step = Time::one() / n;
            let shift = step * offset / Time::from_integer(2);
            let mut out = Vec::new();
            for hap in pat.query(state) {
                let onset = hap.whole_or_part().begin;
                let idx = (onset.cycle_pos() * n).floor_i64();
                if idx.rem_euclid(2) == 1 {
                    out.push(hap.with_span(|s| s.shift(shift)));
                } else {
                    out.push(hap);
                }
            }
            out
        })
    }

    /// Shorthand for `swing_by(Time::new_const(1, 3), n)`.
    pub fn swing(&self, n: Time) -> Self {
        self.swing_by(Time::new_const(1, 3), n)
    }

    /// Loops a slice of `cycles` length starting at `offset` cycles indefinitely (`ribbon`).
    pub fn ribbon(&self, offset: Time, cycles: Time) -> Self {
        if cycles <= Time::zero() {
            return silence();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let rel = sub.begin % cycles;
                let wrapped_begin = offset + if rel < Time::zero() { rel + cycles } else { rel };
                let delta = sub.begin - wrapped_begin;
                let qspan = sub.shift(-delta);
                for hap in pat.query(&state.with_span(qspan)) {
                    out.push(hap.with_span(|s| s.shift(delta)));
                }
            }
            out
        })
    }

    /// Applies `f` every `n` cycles (`cycle % n == 0`).
    pub fn every<F>(&self, n: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.every_offset(n, 0, f)
    }

    /// Applies `f` to every `n`-th cycle with a cycle offset `offset` (`(cycle - offset) % n == 0`).
    pub fn every_offset<F>(&self, n: i64, offset: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        if n <= 0 {
            return self.clone();
        }
        self.when(move |cyc| (cyc - offset).rem_euclid(n) == 0, f)
    }

    /// Applies each function in `fs` across consecutive cycles (`each`).
    pub fn each<F>(&self, fs: Vec<F>) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        if fs.is_empty() {
            return self.clone();
        }
        let variants: Vec<Self> = fs.into_iter().map(|f| f(self)).collect();
        slowcat_prime(variants)
    }

    /// Applies `f` only during cycles where `cond(cycle_index)` returns `true`.
    pub fn when<C, F>(&self, cond: C, f: F) -> Self
    where
        C: Fn(i64) -> bool + Send + Sync + 'static,
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        let base = self.clone();
        let transformed = f(self);
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam().numer();
                let s = state.with_span(sub);
                if cond(cyc) {
                    out.extend(transformed.query(&s));
                } else {
                    out.extend(base.query(&s));
                }
            }
            out
        })
    }

    /// Applies `f` when `(cycle_index % a) >= b`.
    pub fn whenmod<F>(&self, a: i64, b: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        if a <= 0 {
            return self.clone();
        }
        self.when(move |cyc| cyc.rem_euclid(a) >= b, f)
    }

    /// Patterned `whenmod` accepting `IntoPattern<i64>` for both `a` and `b` (`PP-06`, Uzu #2722).
    pub fn whenmod_pat<PA, PB, F>(&self, a: PA, b: PB, f: F) -> Self
    where
        PA: IntoPattern<i64>,
        PB: IntoPattern<i64>,
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        let base = self.clone();
        let f = StdArc::new(f);
        let pat_a = a.into_pattern();
        let pat_b = b.into_pattern();
        let combined = lift_a2_both(&pat_a, &pat_b, move |&va, &vb| {
            let f_ref = StdArc::clone(&f);
            base.whenmod(va, vb, move |p| f_ref(p))
        });
        <Self as PatternMonad<T>>::inner_join(&combined)
    }

    /// Applies `f` with probability `prob` per event (`sometimesBy`).
    pub fn sometimes_by<F>(&self, prob: f64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        let kept = self.degrade_by(prob);
        let transformed = f(&self.undegrade_by(prob));
        stack(vec![kept, transformed])
    }

    /// Applies `f` 50% of the time (`sometimes`).
    pub fn sometimes<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.sometimes_by(0.5, f)
    }

    /// Applies `f` 75% of the time (`often`).
    pub fn often<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.sometimes_by(0.75, f)
    }

    /// Applies `f` 25% of the time (`rarely`).
    pub fn rarely<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.sometimes_by(0.25, f)
    }

    /// Applies `f` 90% of the time (`almostAlways`).
    pub fn almost_always<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.sometimes_by(0.9, f)
    }

    /// Applies `f` 10% of the time (`almostNever`).
    pub fn almost_never<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        self.sometimes_by(0.1, f)
    }

    /// Always applies `f` (`always`).
    pub fn always<F>(&self, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        f(self)
    }

    /// Never applies `f` (`never`).
    pub fn never<F>(&self, _f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        self.clone()
    }

    /// Divides each cycle into `n` chunks and applies `f` to a different chunk each cycle (`chunk`).
    pub fn chunk<F>(&self, n: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        if n <= 0 {
            return self.clone();
        }
        let variants: Vec<Self> = (0..n)
            .map(|i| {
                let b = Time::new_const(i, n);
                let e = Time::new_const(i + 1, n);
                self.within(b, e, &f)
            })
            .collect();
        slowcat_prime(variants)
    }

    /// Divides each cycle into `n` chunks and applies `f` to chunks in reverse cycle order (`chunkBack`).
    pub fn chunk_back<F>(&self, n: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static,
    {
        if n <= 0 {
            return self.clone();
        }
        let variants: Vec<Self> = (0..n)
            .rev()
            .map(|i| {
                let b = Time::new_const(i, n);
                let e = Time::new_const(i + 1, n);
                self.within(b, e, &f)
            })
            .collect();
        slowcat_prime(variants)
    }

    /// Repeats each event `n` times within its original `whole` duration (`ply`).
    pub fn ply(&self, n: Time) -> Self {
        let count = n.floor_i64();
        if count <= 0 {
            return silence();
        }
        let pat = self.clone();
        Self::new(move |state: &State| {
            let mut out = Vec::new();
            for hap in pat.query(state) {
                let w = hap.whole_or_part();
                let sub_dur = w.duration() / Time::from_integer(count);
                for k in 0..count {
                    let sb = w.begin + Time::from_integer(k) * sub_dur;
                    let se = sb + sub_dur;
                    if let Some(sub_whole) = TimeSpan::new(sb, se) {
                        if let Some(part) = hap.part.intersection(sub_whole) {
                            out.push(Hap::discrete(
                                sub_whole,
                                part,
                                hap.value.clone(),
                                hap.context.clone(),
                            ));
                        }
                    }
                }
            }
            out
        })
    }

    /// Patterned `ply`.
    pub fn ply_pat<P: IntoPattern<Time>>(&self, n: P) -> Self {
        let base = self.clone();
        let npat = n.into_pattern();
        let nested = npat.fmap(move |&factor| base.ply(factor));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Stacks `self` with a time-shifted (`late(offset)`) and transformed (`f`) copy (`off`).
    pub fn off<F>(&self, offset: Time, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        stack(vec![self.clone(), f(&self.late(offset))])
    }

    /// Stacks `self` with `f(self)` (`superimpose`).
    pub fn superimpose<F>(&self, f: F) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        stack(vec![self.clone(), f(self)])
    }

    /// Applies each function in `fs` to `self` and stacks the results (`layer`).
    pub fn layer<F>(&self, fs: Vec<F>) -> Self
    where
        F: FnOnce(&Self) -> Self,
    {
        stack(fs.into_iter().map(|f| f(self)).collect())
    }
}

impl Pattern<ControlMap> {
    /// Echoes `self` `n` times, each delayed by `time` cycles with `gain` attenuated by `feedback^k` (`stut`).
    pub fn stut(&self, n: usize, feedback: f64, time: Time) -> ControlPattern {
        if n == 0 {
            return silence();
        }
        let mut layers = Vec::with_capacity(n);
        let mut amp = 1.0;
        for k in 0..n {
            let offset = Time::from_integer(k as i64) * time;
            let cur_amp = amp;
            layers.push(self.late(offset).fmap(move |m| {
                let mut next = m.clone();
                let base_gain = next.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
                next.insert("gain".to_string(), Value::Float(base_gain * cur_amp));
                next
            }));
            amp *= feedback;
        }
        stack(layers)
    }

    /// Strudel `echo`: `stut` with `(times, time, feedback)` parameter order.
    pub fn echo(&self, times: usize, time: Time, feedback: f64) -> ControlPattern {
        self.stut(times, feedback, time)
    }

    /// Echoes `self` `times` times, applying `f` cumulatively to each delayed tap (`echoWith`).
    pub fn echo_with<F>(&self, times: usize, time: Time, f: F) -> ControlPattern
    where
        F: Fn(&ControlPattern) -> ControlPattern,
    {
        if times == 0 {
            return silence();
        }
        let mut layers = Vec::with_capacity(times);
        let mut current = self.clone();
        for k in 0..times {
            let offset = Time::from_integer(k as i64) * time;
            layers.push(current.late(offset));
            current = f(&current);
        }
        stack(layers)
    }
}

/// Stereo juxtaposition combinators for `ControlPattern`.
pub trait StereoJux {
    /// Pans original pattern left (`pan = 0.0`) and `f(self)` right (`pan = 1.0`).
    fn jux<F>(&self, f: F) -> ControlPattern
    where
        F: FnOnce(&ControlPattern) -> ControlPattern;

    /// Pans original pattern to `0.5 - amount / 2` and `f(self)` to `0.5 + amount / 2`.
    fn jux_by<F>(&self, amount: f64, f: F) -> ControlPattern
    where
        F: FnOnce(&ControlPattern) -> ControlPattern;
}

impl StereoJux for ControlPattern {
    fn jux<F>(&self, f: F) -> ControlPattern
    where
        F: FnOnce(&ControlPattern) -> ControlPattern,
    {
        self.jux_by(1.0, f)
    }

    fn jux_by<F>(&self, amount: f64, f: F) -> ControlPattern
    where
        F: FnOnce(&ControlPattern) -> ControlPattern,
    {
        let amt = amount.clamp(0.0, 1.0);
        let left_pan = 0.5 - amt * 0.5;
        let right_pan = 0.5 + amt * 0.5;
        let left = self.fmap(move |m| {
            let mut next = m.clone();
            next.insert("pan".to_string(), Value::Float(left_pan));
            next
        });
        let right = f(self).fmap(move |m| {
            let mut next = m.clone();
            next.insert("pan".to_string(), Value::Float(right_pan));
            next
        });
        stack(vec![left, right])
    }
}

/// ControlPattern merge and arithmetic operators (`|>`, `#`, `<|`, `|+`, `|-`, `|*`, `|/`).
pub trait ControlPatternOps {
    /// `|>` and `#`: merges `other` into `self`, preferring `other`'s values on key collision (`app_left`).
    fn union_right(&self, other: &ControlPattern) -> ControlPattern;
    /// `<|`: merges `other` into `self`, preferring `self`'s values on key collision (`app_left`).
    fn union_left(&self, other: &ControlPattern) -> ControlPattern;

    /// `|+`, `|-`, `|*`, `|/`: structure-preserving (`app_left`) arithmetic on matching keys.
    fn add_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn sub_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn mul_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn div_ctrl(&self, other: &ControlPattern) -> ControlPattern;

    /// Variants with explicit alignment (`app_left`, `app_right`, `app_both`).
    fn add_ctrl_in(&self, other: &ControlPattern) -> ControlPattern;
    fn add_ctrl_out(&self, other: &ControlPattern) -> ControlPattern;
    fn add_ctrl_both(&self, other: &ControlPattern) -> ControlPattern;

    /// Imposes rhythmic structure of a boolean pattern onto `self` (`struct`).
    fn struct_pat(&self, mask: &Pattern<bool>) -> ControlPattern;
}

fn merge_maps_arith<F>(a: &ControlMap, b: &ControlMap, op: F) -> ControlMap
where
    F: Fn(&Value, &Value) -> Value,
{
    let mut out = a.clone();
    for (k, vb) in b {
        if let Some(va) = out.get(k) {
            out.insert(k.clone(), op(va, vb));
        } else {
            out.insert(k.clone(), vb.clone());
        }
    }
    out
}

impl ControlPatternOps for ControlPattern {
    fn union_right(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| {
            let mut out = a.clone();
            for (k, v) in b {
                out.insert(k.clone(), v.clone());
            }
            out
        })
    }

    fn union_left(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| {
            let mut out = b.clone();
            for (k, v) in a {
                out.insert(k.clone(), v.clone());
            }
            out
        })
    }

    fn add_ctrl(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| merge_maps_arith(a, b, Value::add_numeric))
    }

    fn sub_ctrl(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| merge_maps_arith(a, b, Value::sub_numeric))
    }

    fn mul_ctrl(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| merge_maps_arith(a, b, Value::mul_numeric))
    }

    fn div_ctrl(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_left(self, other, |a, b| merge_maps_arith(a, b, Value::div_numeric))
    }

    fn add_ctrl_in(&self, other: &ControlPattern) -> ControlPattern {
        self.add_ctrl(other)
    }

    fn add_ctrl_out(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_right(self, other, |a, b| merge_maps_arith(a, b, Value::add_numeric))
    }

    fn add_ctrl_both(&self, other: &ControlPattern) -> ControlPattern {
        lift_a2_both(self, other, |a, b| merge_maps_arith(a, b, Value::add_numeric))
    }

    fn struct_pat(&self, mask: &Pattern<bool>) -> ControlPattern {
        let filtered_mask = mask.filter_values(|&b| b);
        lift_a2_right(self, &filtered_mask, |m, _| m.clone())
    }
}

/// Helper to build a single-key `ControlPattern` from any `IntoPattern<Value>`.
pub fn ctrl_param<P: IntoPattern<Value>>(key: &str, pat: P) -> ControlPattern {
    let k = key.to_string();
    pat.into_pattern().fmap(move |v| {
        let mut m = ControlMap::new();
        m.insert(k.clone(), v.clone());
        m
    })
}

/// Sample / synth name control pattern (`s("bd")` / `sound("bd")`).
pub fn s<P: IntoPattern<String>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|name| {
        let mut m = ControlMap::new();
        m.insert("s".to_string(), Value::String(name.clone()));
        m
    })
}

/// Sample index control pattern (`n(0)`).
pub fn n<P: IntoPattern<i64>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|&idx| {
        let mut m = ControlMap::new();
        m.insert("n".to_string(), Value::Int(idx));
        m
    })
}

/// Note name or MIDI pitch control pattern (`note("c4")`).
pub fn note<P: IntoPattern<String>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|pitch| {
        let mut m = ControlMap::new();
        m.insert("note".to_string(), Value::Note(NoteValue::Name(pitch.clone())));
        m
    })
}

/// Gain control pattern (`gain(0.8)`).
pub fn gain<P: IntoPattern<f64>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|&g| {
        let mut m = ControlMap::new();
        m.insert("gain".to_string(), Value::Float(g));
        m
    })
}

/// Pan control pattern (`pan(0.5)`).
pub fn pan<P: IntoPattern<f64>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|&p| {
        let mut m = ControlMap::new();
        m.insert("pan".to_string(), Value::Float(p));
        m
    })
}

/// Low-pass filter cutoff control pattern (`lpf(800.0)`).
pub fn lpf<P: IntoPattern<f64>>(pat: P) -> ControlPattern {
    pat.into_pattern().fmap(|&hz| {
        let mut m = ControlMap::new();
        m.insert("lpf".to_string(), Value::Float(hz));
        m
    })
}

impl<T: Clone + 'static> Pattern<T> {
    /// Randomly drops events with probability `0.5` (`degrade_by(0.5)`).
    pub fn degrade(&self) -> Self {
        self.degrade_by(0.5)
    }

    /// Randomly drops events with probability `prob` in `[0.0, 1.0]`, keyed deterministically
    /// by each `Hap`'s onset time.
    pub fn degrade_by(&self, prob: f64) -> Self {
        let p = prob.clamp(0.0, 1.0);
        self.filter_haps(move |hap| time_to_rand(hap.whole_or_part().begin) >= p)
    }

    /// Patterned `degrade_by`.
    pub fn degrade_by_pat<P: IntoPattern<f64>>(&self, prob: P) -> Self {
        let base = self.clone();
        let ppat = prob.into_pattern();
        let nested = ppat.fmap(move |&p| base.degrade_by(p));
        <Self as PatternMonad<T>>::inner_join(&nested)
    }

    /// Inverse of `degrade`: retains the complementary 50% of events.
    pub fn undegrade(&self) -> Self {
        self.undegrade_by(0.5)
    }

    /// Inverse of `degrade_by(prob)`: retains events that `degrade_by(prob)` would drop.
    pub fn undegrade_by(&self, prob: f64) -> Self {
        let p = prob.clamp(0.0, 1.0);
        self.filter_haps(move |hap| time_to_rand(hap.whole_or_part().begin) < p)
    }
}
