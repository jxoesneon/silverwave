//! Pattern Algebra (`PatternFunctor`, `PatternApplicative`, `PatternMonad`, `PatternTrigJoin`)
//! and the $O((N + M) \log(N + M))$ Monotonic Sweep-Line `Hap` Intersection (`PP-05`).

use crate::combinators::silence;
use crate::pattern::Pattern;
use crate::rational::{Time, TimeExt};
use crate::span::{Hap, TimeSpan};
use crate::value::State;
use alloc::sync::Arc as StdArc;
use alloc::vec::Vec;

/// $O((N + M) \log(N + M))$ Sweep-Line `Hap` intersection engine (`PP-05`, resolving Tidal #636 / #641).
/// Sorts `left` and `right` by `part.begin` and sweeps with active-interval pruning so non-overlapping
/// sub-cycle events never pay $O(N \cdot M)$ pairwise scan cost.
pub fn sweep_line_join<A, B, C, F, W>(
    mut left: Vec<Hap<A>>,
    mut right: Vec<Hap<B>>,
    mut combine_val: F,
    mut resolve_whole: W,
) -> Vec<Hap<C>>
where
    F: FnMut(&A, &B) -> C,
    W: FnMut(Option<TimeSpan>, Option<TimeSpan>) -> (bool, Option<TimeSpan>),
{
    if left.is_empty() || right.is_empty() {
        return Vec::new();
    }
    left.sort_by(|a, b| a.part.begin.cmp(&b.part.begin).then(a.part.end.cmp(&b.part.end)));
    right.sort_by(|a, b| a.part.begin.cmp(&b.part.begin).then(a.part.end.cmp(&b.part.end)));

    let mut out = Vec::with_capacity(left.len().max(right.len()));
    let mut r_start = 0usize;

    for l_hap in &left {
        // Advance `r_start` past any right haps whose `part.end <= l_hap.part.begin`
        // whenever right haps up to `r_start` have completely finished before `l_hap.part.begin`.
        while r_start < right.len() && right[r_start].part.end <= l_hap.part.begin {
            r_start += 1;
        }
        let mut r_idx = r_start;
        while r_idx < right.len() {
            let r_hap = &right[r_idx];
            if r_hap.part.begin >= l_hap.part.end {
                break;
            }
            if let Some(part) = l_hap.part.intersection(r_hap.part) {
                let (valid, whole) = resolve_whole(l_hap.whole, r_hap.whole);
                if valid {
                    out.push(Hap {
                        whole,
                        part,
                        value: combine_val(&l_hap.value, &r_hap.value),
                        context: l_hap.context.combine(&r_hap.context),
                    });
                }
            }
            r_idx += 1;
        }
    }
    out
}

/// Functor mapping over `Pattern<T>`.
pub trait PatternFunctor<T: 'static> {
    /// Maps `f` over every `Hap<T>`'s value without altering `whole`, `part`, or `context`.
    fn fmap<U, F>(&self, f: F) -> Pattern<U>
    where
        U: 'static,
        F: Fn(&T) -> U + Send + Sync + 'static;
}

impl<T: 'static> PatternFunctor<T> for Pattern<T> {
    fn fmap<U, F>(&self, f: F) -> Pattern<U>
    where
        U: 'static,
        F: Fn(&T) -> U + Send + Sync + 'static,
    {
        let pat = self.clone();
        Pattern::new(move |state| {
            pat.query(state)
                .into_iter()
                .map(|h| h.with_value(|v| f(&v)))
                .collect()
        })
    }
}

/// Boxed function type for `PatternApplicative`.
pub type BoxedFn<A, B> = StdArc<dyn Fn(&A) -> B + Send + Sync + 'static>;

/// Applicative operations (`<*>`, `<*`, `*>`) on `Pattern<BoxedFn<A, B>>`.
pub trait PatternApplicative<A: 'static, B: 'static> {
    /// `<*>`: Applies a pattern of functions to a pattern of values, intersecting both `whole` and `part`.
    fn app_both(&self, values: &Pattern<A>) -> Pattern<B>;

    /// `<*`: Applies a pattern of functions to a pattern of values, preserving `self`'s `whole` structure.
    fn app_left(&self, values: &Pattern<A>) -> Pattern<B>;

    /// `*>`: Applies a pattern of functions to a pattern of values, preserving `values`'s `whole` structure.
    fn app_right(&self, values: &Pattern<A>) -> Pattern<B>;
}

impl<A: 'static, B: 'static> PatternApplicative<A, B> for Pattern<BoxedFn<A, B>> {
    fn app_both(&self, values: &Pattern<A>) -> Pattern<B> {
        lift_a2_both(self, values, |f, x| f(x))
    }

    fn app_left(&self, values: &Pattern<A>) -> Pattern<B> {
        lift_a2_left(self, values, |f, x| f(x))
    }

    fn app_right(&self, values: &Pattern<A>) -> Pattern<B> {
        lift_a2_right(self, values, |f, x| f(x))
    }
}

/// Curried lift helper for binary functions `f(a, b) -> c` using `app_both` (`<*>`) alignment.
pub fn lift_a2_both<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static,
{
    let pa = pa.clone().split_queries();
    let pb = pb.clone().split_queries();
    let f = StdArc::new(f);
    Pattern::new(move |state: &State| {
        let f = StdArc::clone(&f);
        let left = pa.query(state);
        let right = pb.query(state);
        sweep_line_join(
            left,
            right,
            |a, b| f(a, b),
            |wa, wb| match (wa, wb) {
                (Some(a), Some(b)) => match a.intersection(b) {
                    Some(w) => (true, Some(w)),
                    None => (false, None),
                },
                _ => (true, None),
            },
        )
    })
}

/// Curried lift helper for binary functions `f(a, b) -> c` using `app_left` (`<*`) alignment.
pub fn lift_a2_left<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static,
{
    let pa = pa.clone().split_queries();
    let pb = pb.clone().split_queries();
    let f = StdArc::new(f);
    Pattern::new(move |state: &State| {
        let f = StdArc::clone(&f);
        let left = pa.query(state);
        let mut out = Vec::new();
        for l_hap in left {
            // Query right on `l_hap.part` (keeping `global_span` unwarped)
            let sub_state = state.with_span(l_hap.part);
            let right = pb.query(&sub_state);
            for r_hap in right {
                if let Some(part) = l_hap.part.intersection(r_hap.part) {
                    let valid = match (l_hap.whole, r_hap.whole) {
                        (Some(wl), Some(wr)) => wl.intersection(wr).is_some(),
                        _ => true,
                    };
                    if valid {
                        out.push(Hap {
                            whole: l_hap.whole,
                            part,
                            value: f(&l_hap.value, &r_hap.value),
                            context: l_hap.context.combine(&r_hap.context),
                        });
                    }
                }
            }
        }
        out
    })
}

/// Curried lift helper for binary functions `f(a, b) -> c` using `app_right` (`*>`) alignment.
pub fn lift_a2_right<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static,
{
    let pa = pa.clone().split_queries();
    let pb = pb.clone().split_queries();
    let f = StdArc::new(f);
    Pattern::new(move |state: &State| {
        let f = StdArc::clone(&f);
        let right = pb.query(state);
        let mut out = Vec::new();
        for r_hap in right {
            let sub_state = state.with_span(r_hap.part);
            let left = pa.query(&sub_state);
            for l_hap in left {
                if let Some(part) = l_hap.part.intersection(r_hap.part) {
                    let valid = match (l_hap.whole, r_hap.whole) {
                        (Some(wl), Some(wr)) => wl.intersection(wr).is_some(),
                        _ => true,
                    };
                    if valid {
                        out.push(Hap {
                            whole: r_hap.whole,
                            part,
                            value: f(&l_hap.value, &r_hap.value),
                            context: l_hap.context.combine(&r_hap.context),
                        });
                    }
                }
            }
        }
        out
    })
}

/// Monad & All Four Joins (`unwrap`, `inner_join`, `outer_join`, `squeeze_join`).
pub trait PatternMonad<T: Clone + 'static> {
    /// Monadic bind using `unwrap` (`>>=`): maps `f` and flattens via `unwrap`.
    fn bind<U, F>(&self, f: F) -> Pattern<U>
    where
        U: Clone + 'static,
        F: Fn(&T) -> Pattern<U> + Send + Sync + 'static;

    /// `unwrap`: intersects `whole`s when both are `Some` (otherwise `None`) and intersects `part`s.
    fn unwrap(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `inner_join`: structure (`whole`) comes from the INNER pattern.
    fn inner_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `outer_join`: structure (`whole`) comes from the OUTER pattern.
    fn outer_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `squeeze_join`: time-compresses one full cycle `[0, 1)` of each inner pattern into the
    /// `whole` (or `part` if `whole` is `None`) of its corresponding outer `Hap`.
    fn squeeze_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;
}

impl<T: Clone + 'static> PatternMonad<T> for Pattern<T> {
    fn bind<U, F>(&self, f: F) -> Pattern<U>
    where
        U: Clone + 'static,
        F: Fn(&T) -> Pattern<U> + Send + Sync + 'static,
    {
        let mapped = self.fmap(f);
        <Pattern<U> as PatternMonad<U>>::unwrap(&mapped)
    }

    fn unwrap(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let sub_state = state.with_span(outer_hap.part);
                for inner_hap in outer_hap.value.query(&sub_state) {
                    if let Some(part) = outer_hap.part.intersection(inner_hap.part) {
                        let whole = match (outer_hap.whole, inner_hap.whole) {
                            (Some(wo), Some(wi)) => {
                                let Some(w) = wo.intersection(wi) else {
                                    continue;
                                };
                                Some(w)
                            }
                            _ => None,
                        };
                        out.push(Hap {
                            whole,
                            part,
                            value: inner_hap.value,
                            context: outer_hap.context.combine(&inner_hap.context),
                        });
                    }
                }
            }
            out
        })
    }

    fn inner_join(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let sub_state = state.with_span(outer_hap.part);
                for inner_hap in outer_hap.value.query(&sub_state) {
                    if let Some(part) = outer_hap.part.intersection(inner_hap.part) {
                        out.push(Hap {
                            whole: inner_hap.whole,
                            part,
                            value: inner_hap.value,
                            context: outer_hap.context.combine(&inner_hap.context),
                        });
                    }
                }
            }
            out
        })
    }

    fn outer_join(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let query_span = outer_hap.whole_or_part();
                let sub_state = state.with_span(TimeSpan::new_clamped(
                    query_span.begin,
                    query_span.begin + Time::new_const(1, 1_000_000),
                ));
                let inner_haps = outer_hap.value.query(&sub_state);
                for inner_hap in inner_haps {
                    out.push(Hap {
                        whole: outer_hap.whole,
                        part: outer_hap.part,
                        value: inner_hap.value,
                        context: outer_hap.context.combine(&inner_hap.context),
                    });
                }
            }
            out
        })
    }

    fn squeeze_join(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let w = outer_hap.whole_or_part();
                let dur = w.duration();
                if dur <= Time::zero() {
                    continue;
                }
                // Map outer.part back into `[0, 1)` cycle coordinates
                let rel_begin = (outer_hap.part.begin - w.begin) / dur;
                let rel_end = (outer_hap.part.end - w.begin) / dur;
                let Some(inner_query_span) = TimeSpan::new(rel_begin, rel_end) else {
                    continue;
                };
                let sub_state = state.with_span(inner_query_span);
                for inner_hap in outer_hap.value.query(&sub_state) {
                    let mapped_whole = inner_hap.whole.map(|iw| TimeSpan {
                        begin: w.begin + iw.begin * dur,
                        end: w.begin + iw.end * dur,
                    });
                    let mapped_part = TimeSpan {
                        begin: w.begin + inner_hap.part.begin * dur,
                        end: w.begin + inner_hap.part.end * dur,
                    };
                    if let Some(part) = outer_hap.part.intersection(mapped_part) {
                        out.push(Hap {
                            whole: mapped_whole,
                            part,
                            value: inner_hap.value,
                            context: outer_hap.context.combine(&inner_hap.context),
                        });
                    }
                }
            }
            out
        })
    }
}

/// Triggered Reset & Restart Joins (`_trigJoin` / `_TrigzeroJoin`).
pub trait PatternTrigJoin<T: Clone + 'static> {
    /// `_trig_join` (`restart` semantics): shifts inner pattern by `outer.whole.begin.cycle_pos()`.
    fn trig_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `_trig_zero_join` (`reset` semantics): resets inner pattern strictly to `t = 0` at `outer.whole.begin`.
    fn trig_zero_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `reset`: Resets `self` to `t = 0` whenever `trigger` emits `true`.
    fn reset(&self, trigger: &Pattern<bool>) -> Pattern<T>;

    /// `restart`: Restarts `self` from cycle 0 (preserving cycle phase) whenever `trigger` emits `true`.
    fn restart(&self, trigger: &Pattern<bool>) -> Pattern<T>;
}

impl<T: Clone + 'static> PatternTrigJoin<T> for Pattern<T> {
    fn trig_join(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let onset = outer_hap.whole_or_part().begin;
                let offset = onset.cycle_pos();
                let shifted_pat = outer_hap.value.late(offset);
                let sub_state = state.with_span(outer_hap.part);
                for inner_hap in shifted_pat.query(&sub_state) {
                    if let Some(part) = outer_hap.part.intersection(inner_hap.part) {
                        let whole = match (outer_hap.whole, inner_hap.whole) {
                            (Some(wo), Some(wi)) => wo.intersection(wi),
                            _ => inner_hap.whole,
                        };
                        out.push(Hap {
                            whole,
                            part,
                            value: inner_hap.value,
                            context: outer_hap.context.combine(&inner_hap.context),
                        });
                    }
                }
            }
            out
        })
    }

    fn trig_zero_join(nested: &Pattern<Pattern<T>>) -> Pattern<T> {
        let nested = nested.clone();
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for outer_hap in nested.query(state) {
                let onset = outer_hap.whole_or_part().begin;
                let shifted_pat = outer_hap.value.late(onset);
                let sub_state = state.with_span(outer_hap.part);
                for inner_hap in shifted_pat.query(&sub_state) {
                    if let Some(part) = outer_hap.part.intersection(inner_hap.part) {
                        let whole = match (outer_hap.whole, inner_hap.whole) {
                            (Some(wo), Some(wi)) => wo.intersection(wi),
                            _ => inner_hap.whole,
                        };
                        out.push(Hap {
                            whole,
                            part,
                            value: inner_hap.value,
                            context: outer_hap.context.combine(&inner_hap.context),
                        });
                    }
                }
            }
            out
        })
    }

    fn reset(&self, trigger: &Pattern<bool>) -> Pattern<T> {
        let pat = self.clone();
        let nested = trigger.fmap(move |&b| if b { pat.clone() } else { silence() });
        Self::trig_zero_join(&nested)
    }

    fn restart(&self, trigger: &Pattern<bool>) -> Pattern<T> {
        let pat = self.clone();
        let nested = trigger.fmap(move |&b| if b { pat.clone() } else { silence() });
        Self::trig_join(&nested)
    }
}
