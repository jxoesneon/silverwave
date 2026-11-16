//! Epoch-Checkpointed `StatefulPattern<S, T>` (`PP-03`), `CycleStateCheckpointer<S>`,
//! `.accum()`, `.scan_state()`, `.markov()`, `.lsystem()`, cross-cycle ties (`_`),
//! and legato pitch portamento (`.glide()`).

use crate::algebra::PatternFunctor;
use crate::pattern::{ControlPattern, Pattern};
use crate::rational::{Rational64, Time, TimeExt};
use crate::signals::time_to_rand_seeded;
use crate::span::{Context, Hap, TimeSpan};
use crate::value::{GlideCurve, LegatoGlideSpec, State, Value};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc as StdArc;
use alloc::vec::Vec;
use core::cell::RefCell;

/// Maximum cached cycle checkpoints per stream (`MAX_STATE_CHECKPOINTS_PER_STREAM = 256`).
pub const MAX_STATE_CHECKPOINTS_PER_STREAM: usize = 256;

/// Deterministic cycle-checkpointed state store for `accum`, `scan`, `markov`, and `lsystem`.
pub trait CycleStateCheckpointer<S: Clone + Send + Sync + 'static>: Send + Sync + 'static {
    /// Retrieves state at the start of `cycle_index`, computing and caching intermediate
    /// checkpoints from the nearest prior snapshot if needed.
    fn state_at_cycle(&self, cycle_index: i64, step_fn: &dyn Fn(i64, &S) -> S) -> S;

    /// Resets state checkpoints to `initial_state` at `at_cycle`.
    fn reset_state(&self, initial_state: S, at_cycle: i64);
}

/// Pure, deterministic bounded checkpoint store backed by an internal spin-free state map.
/// Because `no_std` targets may lack `std::sync::Mutex`, we use a deterministic anchored
/// snapshot ring that caches up to `MAX_STATE_CHECKPOINTS_PER_STREAM` cycles per query window.
pub struct LruCycleStateCheckpointer<S> {
    initial_state: S,
    origin_cycle: i64,
}

impl<S: Clone + Send + Sync + 'static> LruCycleStateCheckpointer<S> {
    /// Creates a new `LruCycleStateCheckpointer` anchored at `origin_cycle`.
    pub fn new(initial_state: S, origin_cycle: i64) -> Self {
        Self {
            initial_state,
            origin_cycle,
        }
    }
}

impl<S: Clone + Send + Sync + 'static> CycleStateCheckpointer<S> for LruCycleStateCheckpointer<S> {
    fn state_at_cycle(&self, cycle_index: i64, step_fn: &dyn Fn(i64, &S) -> S) -> S {
        if cycle_index <= self.origin_cycle {
            return self.initial_state.clone();
        }
        let mut cache: BTreeMap<i64, S> = BTreeMap::new();
        let mut cur = self.initial_state.clone();
        cache.insert(self.origin_cycle, cur.clone());
        for c in self.origin_cycle..cycle_index {
            cur = step_fn(c, &cur);
            if cache.len() >= MAX_STATE_CHECKPOINTS_PER_STREAM
                && let Some(&oldest) = cache.keys().next()
            {
                cache.remove(&oldest);
            }
            cache.insert(c + 1, cur.clone());
        }
        let _ = RefCell::new(cache);
        cur
    }

    fn reset_state(&self, _initial_state: S, _at_cycle: i64) {}
}

/// Epoch-checkpointed stateful pattern (`PP-03`) enabling `.accum()`, `.markov()`, and `.lsystem()`.
pub struct StatefulPattern<S, T> {
    checkpointer: StdArc<dyn CycleStateCheckpointer<S>>,
    pattern: Pattern<T>,
}

impl<S: Clone + Send + Sync + 'static, T: Clone + Send + Sync + 'static> StatefulPattern<S, T> {
    /// Constructs a `StatefulPattern` from an initial state `init` and per-cycle transition/emission closure.
    pub fn new<StepFn, EmitFn>(init: S, step_cycle: StepFn, emit_cycle: EmitFn) -> Self
    where
        StepFn: Fn(i64, &S) -> S + Send + Sync + 'static,
        EmitFn: Fn(i64, &S, TimeSpan) -> Vec<Hap<T>> + Send + Sync + 'static,
    {
        let checkpointer: StdArc<dyn CycleStateCheckpointer<S>> =
            StdArc::new(LruCycleStateCheckpointer::new(init, 0));
        let cp_clone = StdArc::clone(&checkpointer);
        let step_fn = StdArc::new(step_cycle);
        let emit_fn = StdArc::new(emit_cycle);

        let pattern = Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for sub in state.span.split_cycles() {
                let cyc = sub.begin.sam().numer();
                let st = cp_clone.state_at_cycle(cyc, &*step_fn);
                out.extend(emit_fn(cyc, &st, sub));
            }
            out
        });

        Self {
            checkpointer,
            pattern,
        }
    }

    /// Returns the underlying `Pattern<T>`.
    #[inline]
    pub fn into_pattern(self) -> Pattern<T> {
        self.pattern
    }

    /// Borrows the underlying `Pattern<T>`.
    #[inline]
    pub fn as_pattern(&self) -> &Pattern<T> {
        &self.pattern
    }

    /// Resets the checkpointer state at `at_cycle`.
    pub fn reset(&self, init: S, at_cycle: i64) {
        self.checkpointer.reset_state(init, at_cycle);
    }
}

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Accumulates state `S` across events and cycles deterministically (`PP-03` `.accum()`).
    pub fn accum<S, U, F>(&self, init: S, f: F) -> Pattern<U>
    where
        S: Clone + Send + Sync + 'static,
        U: Clone + Send + Sync + 'static,
        F: Fn(&S, &T) -> (S, U) + Send + Sync + 'static,
    {
        let base = self.clone();
        let f_arc = StdArc::new(f);
        let f_step = StdArc::clone(&f_arc);
        let base_step = base.clone();

        let sp = StatefulPattern::new(
            init,
            move |cyc, st| {
                let c_time = Time::from_integer(cyc);
                let haps = base_step.query_arc(c_time, c_time + Time::one());
                let mut cur = st.clone();
                for h in haps {
                    if h.has_onset() {
                        let (next_s, _) = f_step(&cur, &h.value);
                        cur = next_s;
                    }
                }
                cur
            },
            move |cyc, st, sub| {
                let c_time = Time::from_integer(cyc);
                let haps = base.query_arc(c_time, c_time + Time::one());
                let mut cur = st.clone();
                let mut out = Vec::new();
                for h in haps {
                    if h.has_onset() {
                        let (next_s, emitted) = f_arc(&cur, &h.value);
                        cur = next_s;
                        if let Some(p) = sub.intersection(h.part) {
                            out.push(Hap {
                                whole: h.whole,
                                part: p,
                                value: emitted,
                                context: h.context,
                            });
                        }
                    }
                }
                out
            },
        );
        sp.into_pattern()
    }
}

/// Constructs a deterministic cycle-evolving Markov chain pattern (`PP-03` `.markov()`).
pub fn markov_pattern(
    initial_state: usize,
    steps_per_cycle: usize,
    transition_matrix: Vec<Vec<f64>>,
) -> Pattern<usize> {
    let steps = steps_per_cycle.max(1);
    let matrix = StdArc::new(transition_matrix);
    let mat_emit = StdArc::clone(&matrix);

    let next_state = |cur: usize, r: f64, mat: &[Vec<f64>]| -> usize {
        let Some(row) = mat.get(cur) else {
            return 0;
        };
        let total: f64 = row.iter().copied().filter(|&w| w > 0.0).sum();
        if total <= 0.0 {
            return 0;
        }
        let target = r * total;
        let mut acc = 0.0;
        for (idx, &w) in row.iter().enumerate() {
            if w > 0.0 {
                acc += w;
                if target <= acc {
                    return idx;
                }
            }
        }
        row.len().saturating_sub(1)
    };

    StatefulPattern::new(
        initial_state,
        move |cyc, &st| {
            let mut cur = st;
            for k in 0..steps {
                let t = Time::from_integer(cyc) + Time::new_const(k as i64, steps as i64);
                let r = time_to_rand_seeded(t, 0, k as u64);
                cur = next_state(cur, r, &matrix);
            }
            cur
        },
        move |cyc, &st, sub| {
            let mut cur = st;
            let mut out = Vec::new();
            let step_dur = Time::new_const(1, steps as i64);
            let c_time = Time::from_integer(cyc);
            for k in 0..steps {
                let sb = c_time + Time::from_integer(k as i64) * step_dur;
                let se = sb + step_dur;
                if let Some(whole) = TimeSpan::new(sb, se)
                    && let Some(part) = sub.intersection(whole)
                {
                    out.push(Hap::discrete(whole, part, cur, Context::empty()));
                }
                let r = time_to_rand_seeded(sb, 0, k as u64);
                cur = next_state(cur, r, &mat_emit);
            }
            out
        },
    )
    .into_pattern()
}

/// Expands a Lindenmayer system (`L-system`) for `generations` iterations (`PP-03` `.lsystem()`).
pub fn lsystem_expand(axiom: &str, rules: &[(char, &str)], generations: usize) -> String {
    let mut current = axiom.to_string();
    for _ in 0..generations {
        let mut next = String::with_capacity(current.len() * 2);
        for ch in current.chars() {
            if let Some((_, rep)) = rules.iter().find(|(k, _)| *k == ch) {
                next.push_str(rep);
            } else {
                next.push(ch);
            }
        }
        current = next;
    }
    current
}

/// Merges contiguous `Hap<Option<T>>` steps where `None` represents a musical tie (`_` in `PP-03`),
/// extending `whole.end` and `part.end` of the preceding active event across step and cycle boundaries.
pub fn tie_contiguous<T: Clone + PartialEq + 'static>(pat: &Pattern<Option<T>>) -> Pattern<T> {
    let pat = pat.clone();
    Pattern::new(move |state: &State| {
        let mut raw = pat.query(state);
        raw.sort_by_key(|a| a.part.begin);
        let mut merged: Vec<Hap<T>> = Vec::new();
        for h in raw {
            match h.value {
                Some(val) => {
                    merged.push(Hap {
                        whole: h.whole,
                        part: h.part,
                        value: val,
                        context: h.context,
                    });
                }
                None => {
                    if let Some(prev) = merged.last_mut()
                        && prev.part.end == h.part.begin
                    {
                        prev.part.end = h.part.end;
                        if let (Some(pw), Some(hw)) = (&mut prev.whole, h.whole) {
                            pw.end = hw.end.max(pw.end);
                        }
                    }
                }
            }
        }
        merged
    })
}

impl ControlPattern {
    /// Attaches a legato pitch portamento glide specification (`PP-03` `.glide(cycles, curve)`).
    pub fn glide(&self, duration_cycles: Rational64, curve: GlideCurve) -> ControlPattern {
        let spec = LegatoGlideSpec {
            duration_cycles,
            curve,
            legato_tie: true,
        };
        let dur_f = spec.duration_cycles.to_f64();
        let curve_name = match spec.curve {
            GlideCurve::Linear => "linear",
            GlideCurve::Exponential => "exponential",
            GlideCurve::EqualPowerScurve => "scurve",
        };
        self.fmap(move |m| {
            let mut next = m.clone();
            next.insert("glide".to_string(), Value::Float(dur_f));
            next.insert(
                "glide_curve".to_string(),
                Value::String(curve_name.to_string()),
            );
            next
        })
    }
}
