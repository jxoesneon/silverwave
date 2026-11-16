//! Dual-Layer Rust DSL (`silverwave_mini::dsl`), all 7 Strudel Alignment Suffixes
//! (`.in`, `.out`, `.mix`, `.squeeze`, `.squeezeout`, `.reset`, `.restart`),
//! and macro-composition (`ur`, `seq_p_loop`).

use crate::compiler::{MiniTarget, mini};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc as StdArc;
use alloc::vec::Vec;
use core::ops::{Add, Div, Mul, Sub};
use silverwave_core::{
    ControlMap, ControlPattern, Hap, Pattern, PatternFunctor, PatternMonad, Rational, State, Time,
    TimeExt, Value, lift_a2_both, lift_a2_left, lift_a2_right, seq_p, silence,
};

/// Trait allowing mini-notation string literals (`"0 1 2"`) or typed patterns to be lifted
/// into `Pattern<T>` inside DSL combinators.
pub trait IntoMiniPattern<T: MiniTarget> {
    fn into_mini_pattern(self) -> Pattern<T>;
}

impl<T: MiniTarget> IntoMiniPattern<T> for Pattern<T> {
    #[inline]
    fn into_mini_pattern(self) -> Pattern<T> {
        self
    }
}

impl<T: MiniTarget> IntoMiniPattern<T> for &Pattern<T> {
    #[inline]
    fn into_mini_pattern(self) -> Pattern<T> {
        self.clone()
    }
}

impl<T: MiniTarget> IntoMiniPattern<T> for &str {
    #[inline]
    fn into_mini_pattern(self) -> Pattern<T> {
        mini::<T>(self).unwrap_or_else(|_| silence())
    }
}

impl<T: MiniTarget> IntoMiniPattern<T> for String {
    #[inline]
    fn into_mini_pattern(self) -> Pattern<T> {
        mini::<T>(&self).unwrap_or_else(|_| silence())
    }
}

/// Helper for applying a binary operation `op(left, right)` across all 7 Strudel alignment modes (`Section 3.3`).
pub fn align_binary_op<T, F>(
    lhs: &Pattern<T>,
    rhs: &Pattern<T>,
    mode: AlignmentMode,
    op: F,
) -> Pattern<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(&T, &T) -> T + Send + Sync + 'static,
{
    let op = StdArc::new(op);
    match mode {
        AlignmentMode::In => lift_a2_left(lhs, rhs, move |a, b| op(a, b)),
        AlignmentMode::Out => lift_a2_right(lhs, rhs, move |a, b| op(a, b)),
        AlignmentMode::Mix => lift_a2_both(lhs, rhs, move |a, b| op(a, b)),
        AlignmentMode::Squeeze => {
            let rhs_c = rhs.clone();
            let nested = lhs.fmap(move |a| {
                let a_val = a.clone();
                let op_c = StdArc::clone(&op);
                rhs_c.fmap(move |b| op_c(&a_val, b))
            });
            <Pattern<T> as PatternMonad<T>>::squeeze_join(&nested)
        }
        AlignmentMode::SqueezeOut => {
            let lhs_c = lhs.clone();
            let nested = rhs.fmap(move |b| {
                let b_val = b.clone();
                let op_c = StdArc::clone(&op);
                lhs_c.fmap(move |a| op_c(a, &b_val))
            });
            <Pattern<T> as PatternMonad<T>>::squeeze_join(&nested)
        }
        AlignmentMode::Reset => {
            // Each event onset in `rhs` resets `lhs` to the start of its current cycle,
            // truncating `lhs` to fit `rhs`'s event span (`test_alignment_reset_and_restart`).
            let lhs_c = lhs.clone();
            let rhs_c = rhs.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for r_hap in rhs_c.query(state) {
                    let onset = r_hap.whole_or_part().begin;
                    let offset = onset.cycle_pos();
                    let shifted_lhs = lhs_c.late(offset);
                    let sub_state = state.with_span(r_hap.part);
                    for l_hap in shifted_lhs.query(&sub_state) {
                        if let Some(part) = r_hap.part.intersection(l_hap.part) {
                            let whole = match (r_hap.whole, l_hap.whole) {
                                (Some(wr), Some(wl)) => wr.intersection(wl),
                                _ => l_hap.whole,
                            };
                            out.push(Hap {
                                whole,
                                part,
                                value: op(&l_hap.value, &r_hap.value),
                                context: l_hap.context.combine(&r_hap.context),
                            });
                        }
                    }
                }
                out
            })
        }
        AlignmentMode::Restart => {
            // Each event onset in `rhs` restarts `lhs` from absolute cycle 0 (`[0, 1)`).
            let lhs_c = lhs.clone();
            let rhs_c = rhs.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for r_hap in rhs_c.query(state) {
                    let onset = r_hap.whole_or_part().begin;
                    let shifted_lhs = lhs_c.late(onset);
                    let sub_state = state.with_span(r_hap.part);
                    for l_hap in shifted_lhs.query(&sub_state) {
                        if let Some(part) = r_hap.part.intersection(l_hap.part) {
                            let whole = match (r_hap.whole, l_hap.whole) {
                                (Some(wr), Some(wl)) => wr.intersection(wl),
                                _ => l_hap.whole,
                            };
                            out.push(Hap {
                                whole,
                                part,
                                value: op(&l_hap.value, &r_hap.value),
                                context: l_hap.context.combine(&r_hap.context),
                            });
                        }
                    }
                }
                out
            })
        }
    }
}

/// The 7 Strudel pattern alignment modes (`Section 3.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignmentMode {
    In,
    Out,
    Mix,
    Squeeze,
    SqueezeOut,
    Reset,
    Restart,
}

/// Fluent chaining trait exposing `.add`, `.sub`, `.mul`, `.div`, `.set` across all 7 alignment suffixes (`Section 3.3`).
pub trait PatternCombinator<T: MiniTarget> {
    fn add_pat(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_out(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_mix(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_squeeze(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_squeezeout(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_reset(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn add_restart(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;

    fn sub_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn mul_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
    fn div_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T>;
}

impl<T> PatternCombinator<T> for Pattern<T>
where
    T: MiniTarget + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Div<Output = T> + Copy,
{
    fn add_pat(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        self.add_in(rhs)
    }

    fn add_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::In, |&a, &b| a + b)
    }

    fn add_out(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::Out, |&a, &b| a + b)
    }

    fn add_mix(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::Mix, |&a, &b| a + b)
    }

    fn add_squeeze(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::Squeeze, |&a, &b| a + b)
    }

    fn add_squeezeout(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::SqueezeOut, |&a, &b| a + b)
    }

    fn add_reset(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::Reset, |&a, &b| a + b)
    }

    fn add_restart(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::Restart, |&a, &b| a + b)
    }

    fn sub_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::In, |&a, &b| a - b)
    }

    fn mul_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::In, |&a, &b| a * b)
    }

    fn div_in(&self, rhs: impl IntoMiniPattern<T>) -> Pattern<T> {
        let r = rhs.into_mini_pattern();
        align_binary_op(self, &r, AlignmentMode::In, |&a, &b| a / b)
    }
}

/// TidalCycles `ur` song-level macro-combinator (`Section 3.4`):
/// Stretches `structure_pat` over `length_cycles` cycles, resolves named sub-patterns in `pats`,
/// and applies optional `:fx_name` transformations from `fx`.
pub fn ur<T, F>(
    length_cycles: Rational,
    structure_pat: &Pattern<String>,
    pats: BTreeMap<String, Pattern<T>>,
    fx: BTreeMap<String, F>,
) -> Pattern<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(&Pattern<T>) -> Pattern<T> + Send + Sync + 'static,
{
    if length_cycles <= Time::zero() {
        return silence();
    }
    let pats = StdArc::new(pats);
    let fx = StdArc::new(fx);
    let stretched = structure_pat.slow(length_cycles);
    let nested = stretched.fmap(move |token| {
        let (pat_name, fx_name) = match token.split_once(':') {
            Some((p, f)) => (p.trim(), Some(f.trim())),
            None => (token.trim(), None),
        };
        let Some(base_pat) = pats.get(pat_name) else {
            return silence();
        };
        if let Some(f_key) = fx_name
            && let Some(transform) = fx.get(f_key)
        {
            return transform(base_pat);
        }
        base_pat.clone()
    });
    <Pattern<T> as PatternMonad<T>>::inner_join(&nested)
}

/// Schedules sub-patterns across `[start, end)` cycle windows and loops modulo the maximum `end` cycle (`seqPLoop`).
pub fn seq_p_loop<T: Clone + 'static>(
    entries: Vec<(Rational, Rational, Pattern<T>)>,
) -> Pattern<T> {
    let max_end = entries
        .iter()
        .map(|(_, e, _)| *e)
        .fold(Time::zero(), |a, b| a.max(b));
    if max_end <= Time::zero() {
        return silence();
    }
    seq_p(entries).ribbon(Time::zero(), max_end)
}

/// Mini-notation `s("bd sd")` helper parsing mini-notation directly into a `ControlPattern`.
pub fn s_mini(src: &str) -> ControlPattern {
    let pat = mini::<String>(src).unwrap_or_else(|_| silence());
    pat.fmap(|token| {
        let mut m = ControlMap::new();
        if let Some((bank, idx_str)) = token.split_once(':') {
            m.insert("s".to_string(), Value::String(bank.to_string()));
            if let Ok(idx) = idx_str.parse::<i64>() {
                m.insert("n".to_string(), Value::Int(idx));
            }
        } else {
            m.insert("s".to_string(), Value::String(token.clone()));
        }
        m
    })
}

/// Mini-notation `note("c4 eb4 g4")` helper parsing mini-notation directly into a `ControlPattern`.
pub fn note_mini(src: &str) -> ControlPattern {
    let pat = mini::<Value>(src).unwrap_or_else(|_| silence());
    pat.fmap(|v| {
        let mut m = ControlMap::new();
        m.insert("note".to_string(), v.clone());
        m
    })
}

/// Mini-notation `n("0 1 2 3")` helper parsing mini-notation directly into a `ControlPattern`.
pub fn n_mini(src: &str) -> ControlPattern {
    let pat = mini::<f64>(src).unwrap_or_else(|_| silence());
    pat.fmap(|&v| {
        let mut m = ControlMap::new();
        m.insert("n".to_string(), Value::Float(v));
        m
    })
}

/// Re-export `IntoPattern` and `PatternTrigJoin` for ergonomic DSL use.
pub use silverwave_core::{IntoPattern as _, PatternTrigJoin as _};
