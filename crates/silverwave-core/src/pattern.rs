//! Core `Pattern<T>` functional representation, query transformations, Dual-Timeframe
//! controller queries (`PP-04`), and Universal `IntoPattern<T>` argument lifting (`PP-06`).

use crate::combinators::{fastcat, pure, silence};
use crate::rational::Time;
use crate::span::{Context, Hap, SourceLocation, TimeSpan};
use crate::value::{ControlMap, QueryContext, State, Value};
use alloc::string::{String, ToString};
use alloc::sync::Arc as StdArc;
use alloc::vec;
use alloc::vec::Vec;

/// Query function type: pure, deterministic mapping from `State` to `Vec<Hap<T>>`.
pub type QueryFn<T> = dyn Fn(&State) -> Vec<Hap<T>> + Send + Sync + 'static;

/// A pattern of control parameter maps, matching TidalCycles `ControlPattern`.
pub type ControlPattern = Pattern<ControlMap>;

/// Core pattern representation. Cloning is `O(1)` via atomic reference counting.
pub struct Pattern<T> {
    query_fn: StdArc<QueryFn<T>>,
}

impl<T> Clone for Pattern<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            query_fn: StdArc::clone(&self.query_fn),
        }
    }
}

impl<T: 'static> Pattern<T> {
    /// Constructs a `Pattern<T>` from a pure query function.
    #[inline]
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&State) -> Vec<Hap<T>> + Send + Sync + 'static,
    {
        Self {
            query_fn: StdArc::new(f),
        }
    }

    /// Executes the pattern query for the given `State`.
    #[inline]
    pub fn query(&self, state: &State) -> Vec<Hap<T>> {
        (self.query_fn)(state)
    }

    /// Executes the pattern query with an explicit `QueryContext` (`PP-04`).
    #[inline]
    pub fn query_with_context(&self, ctx: &QueryContext) -> Vec<Hap<T>> {
        let state = State {
            span: ctx.local_span,
            controls: ControlMap::new(),
            query_context: ctx.clone(),
        };
        self.query(&state)
    }

    /// Convenience helper: queries `[begin, end)` with an empty `ControlMap`.
    pub fn query_arc(&self, begin: Time, end: Time) -> Vec<Hap<T>> {
        let Some(span) = TimeSpan::new(begin, end) else {
            return Vec::new();
        };
        let mut haps = Vec::new();
        let base_state = State::new(span);
        for sub in span.split_cycles() {
            let sub_state = base_state.with_span(sub);
            haps.extend(self.query(&sub_state));
        }
        haps
    }

    /// Queries `[0, 1)` (the first cycle) with an empty `ControlMap`.
    #[inline]
    pub fn first_cycle(&self) -> Vec<Hap<T>> {
        self.query_arc(Time::zero(), Time::one())
    }

    /// Splits the query `state.span` at cycle boundaries, queries each cycle independently,
    /// and concatenates the results while preserving `state.query_context.global_span` (`PP-04`).
    pub fn split_queries(&self) -> Self {
        let pat = self.clone();
        Self::new(move |state: &State| {
            let local_cycles = state.span.split_cycles();
            let global_cycles = state.query_context.global_span.split_cycles();
            let mut out = Vec::new();
            for (idx, sub) in local_cycles.into_iter().enumerate() {
                let mut sub_state = state.with_span(sub);
                if let Some(&g_sub) = global_cycles.get(idx).or_else(|| global_cycles.last()) {
                    sub_state.query_context.global_span = g_sub;
                }
                out.extend(pat.query(&sub_state));
            }
            out
        })
    }

    /// Transforms the `State` before passing it to `self.query`.
    pub fn with_query_state<F>(&self, f: F) -> Self
    where
        F: Fn(&State) -> State + Send + Sync + 'static,
    {
        let pat = self.clone();
        Self::new(move |state| pat.query(&f(state)))
    }

    /// Transforms the query `TimeSpan` (`local_span`) before passing it to `self.query`
    /// while preserving `global_span` (`PP-04`).
    pub fn with_query_span<F>(&self, f: F) -> Self
    where
        F: Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static,
    {
        let pat = self.clone();
        Self::new(move |state| {
            let new_span = f(state.span);
            if new_span.begin >= new_span.end {
                return Vec::new();
            }
            pat.query(&state.with_span(new_span))
        })
    }

    /// Transforms both `begin` and `end` of the query `TimeSpan` using `f`.
    pub fn with_query_time<F>(&self, f: F) -> Self
    where
        F: Fn(Time) -> Time + Send + Sync + 'static,
    {
        self.with_query_span(move |s| s.with_time(&f))
    }

    /// Transforms each `Hap<T>` returned by `self.query`.
    pub fn with_hap<U, F>(&self, f: F) -> Pattern<U>
    where
        U: 'static,
        F: Fn(Hap<T>) -> Hap<U> + Send + Sync + 'static,
    {
        let pat = self.clone();
        Pattern::new(move |state| pat.query(state).into_iter().map(&f).collect())
    }

    /// Transforms both `whole` and `part` spans of every returned `Hap<T>`.
    pub fn with_hap_span<F>(&self, f: F) -> Self
    where
        F: Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static,
    {
        self.with_hap(move |hap| hap.with_span(&f))
    }

    /// Transforms both `begin` and `end` of `whole` and `part` spans of every returned `Hap<T>`.
    pub fn with_hap_time<F>(&self, f: F) -> Self
    where
        F: Fn(Time) -> Time + Send + Sync + 'static,
    {
        self.with_hap_span(move |s| s.with_time(&f))
    }

    /// Filters returned `Hap<T>`s by predicate `f`.
    pub fn filter_haps<F>(&self, f: F) -> Self
    where
        F: Fn(&Hap<T>) -> bool + Send + Sync + 'static,
    {
        let pat = self.clone();
        Self::new(move |state| pat.query(state).into_iter().filter(|h| f(h)).collect())
    }

    /// Filters returned `Hap<T>`s by their values.
    pub fn filter_values<F>(&self, f: F) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.filter_haps(move |h| f(&h.value))
    }

    /// Attaches a `SourceLocation` to every `Hap` emitted by `self`.
    pub fn with_loc(&self, loc: SourceLocation) -> Self {
        self.with_hap(move |mut h| {
            if !h.context.locations.contains(&loc) {
                h.context.locations.push(loc);
            }
            h
        })
    }

    /// Attaches a string tag to every `Hap` emitted by `self`.
    pub fn tag(&self, label: &str) -> Self {
        let label_str = label.to_string();
        self.with_hap(move |mut h| {
            h.context.tags.insert(label_str.clone());
            h
        })
    }

    /// Opts a controller/signal pattern into reading pattern-warped `local_span` rather than
    /// `global_span` (`PP-04` `.localTime()` / `^^var`).
    pub fn local_time(&self) -> Self {
        let pat = self.clone();
        Self::new(move |state| {
            let mut local_ctx = state.query_context.clone();
            local_ctx.global_span = state.span;
            let s = state.with_query_context(local_ctx);
            pat.query(&s)
        })
    }
}

/// Constructs a hardware/UI controller continuous signal (`cF`, `slider`, `cc`) that reads from
/// unwarped `global_span` by default (`PP-04`, resolving Tidal Issue #1029), so `.slow(2)` on a
/// pattern never time-warps live hardware knob reads unless `.local_time()` is called.
pub fn global_controller<T, F>(eval_at_global_midpoint: F) -> Pattern<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(Time) -> T + Send + Sync + 'static,
{
    Pattern::new(move |state: &State| {
        let g_mid = state.query_context.global_span.midpoint();
        let val = eval_at_global_midpoint(g_mid);
        vec![Hap::continuous(state.span, val, Context::empty())]
    })
}

/// Universal argument-lifting trait (`PP-06`): allows any combinator parameter to accept
/// scalar literals (`4`, `0.5`, `"bd"`), vectors, or dynamic `Pattern<T>`s uniformly.
pub trait IntoPattern<T: 'static> {
    fn into_pattern(self) -> Pattern<T>;
}

impl<T: 'static> IntoPattern<T> for Pattern<T> {
    #[inline]
    fn into_pattern(self) -> Pattern<T> {
        self
    }
}

impl<T: Clone + 'static> IntoPattern<T> for &Pattern<T> {
    #[inline]
    fn into_pattern(self) -> Pattern<T> {
        self.clone()
    }
}

impl IntoPattern<i64> for i64 {
    #[inline]
    fn into_pattern(self) -> Pattern<i64> {
        pure(self)
    }
}

impl IntoPattern<usize> for usize {
    #[inline]
    fn into_pattern(self) -> Pattern<usize> {
        pure(self)
    }
}

impl IntoPattern<isize> for isize {
    #[inline]
    fn into_pattern(self) -> Pattern<isize> {
        pure(self)
    }
}

impl IntoPattern<f64> for f64 {
    #[inline]
    fn into_pattern(self) -> Pattern<f64> {
        pure(self)
    }
}

impl IntoPattern<bool> for bool {
    #[inline]
    fn into_pattern(self) -> Pattern<bool> {
        pure(self)
    }
}

impl IntoPattern<Time> for Time {
    #[inline]
    fn into_pattern(self) -> Pattern<Time> {
        pure(self)
    }
}

impl IntoPattern<Time> for i64 {
    #[inline]
    fn into_pattern(self) -> Pattern<Time> {
        pure(Time::from_integer(self))
    }
}

impl IntoPattern<String> for &str {
    #[inline]
    fn into_pattern(self) -> Pattern<String> {
        pure(self.to_string())
    }
}

impl IntoPattern<String> for String {
    #[inline]
    fn into_pattern(self) -> Pattern<String> {
        pure(self)
    }
}

impl IntoPattern<Value> for Value {
    #[inline]
    fn into_pattern(self) -> Pattern<Value> {
        pure(self)
    }
}

impl<T: Clone + Send + Sync + 'static> IntoPattern<T> for Vec<T> {
    fn into_pattern(self) -> Pattern<T> {
        if self.is_empty() {
            silence()
        } else {
            fastcat(self.into_iter().map(pure).collect())
        }
    }
}
