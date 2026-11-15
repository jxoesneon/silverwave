# `01-SPEC-CORE-PATTERN-ENGINE.md` — `silverwave-core` Specification

**Version:** 1.0.0  
**Crate:** `silverwave-core`  
**Scope:** Exact 1:1 feature parity with the union of Strudel (`@strudel/core` + `@strudel/tonal`, see [Strudel Internals](https://strudel.cc/technical-manual/internals/) and [Strudel Signals](https://strudel.cc/learn/signals/)) and TidalCycles (`tidal-core`).  
**Constraint:** All public APIs, traits, and type contracts are specified as Rust signatures and behavioral invariants without implementation bodies.

---

## 1. Core Type & Trait Architecture

### 1.1 Exact Rational Time (`Time`)

All pattern scheduling, cycle boundaries, and structural subdivisions MUST use exact rational arithmetic to eliminate floating-point drift across arbitrary cycle depths.

```rust
use num_rational::Ratio;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Add, Div, Mul, Sub};
use std::sync::Arc as StdArc;

/// Exact rational cycle time. `0` is the origin of cycle 0; `[n, n + 1)` is cycle `n`.
pub type Time = Ratio<i64>;

/// Conversion and cycle-boundary helpers for `Time`.
pub trait TimeExt {
    /// Returns the floor integer cycle index (`sam(t)` in Tidal / `wholeCycle` start in Strudel).
    fn sam(self) -> Time;

    /// Returns `sam(t) + 1`, the start of the next cycle (`nextSam(t)`).
    fn next_sam(self) -> Time;

    /// Returns the fractional position within the current cycle: `t - t.sam()`, in `[0, 1)`.
    fn cycle_pos(self) -> Time;

    /// Converts cycle time to wall-clock seconds given cycles-per-second (`cps`).
    fn to_seconds(self, cps: f64) -> f64;
}
```

### 1.2 Time Intervals (`TimeSpan` / `Arc`)

A `TimeSpan` (aliased as `Arc` for TidalCycles parity) represents a half-open rational time interval `[begin, end)`.

```rust
/// Half-open rational time interval `[begin, end)`.
/// Invariant: For any valid queried or emitted span, `begin < end` (non-empty).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeSpan {
    pub begin: Time,
    pub end: Time,
}

/// TidalCycles compatibility alias.
pub type Arc = TimeSpan;

impl TimeSpan {
    /// Constructs a `TimeSpan` if `begin < end`; returns `None` if `begin >= end`.
    pub fn new(begin: Time, end: Time) -> Option<Self>;

    /// Constructs a `TimeSpan` representing the single cycle `[sam(t), next_sam(t))`.
    pub fn cycle_of(t: Time) -> Self;

    /// Returns the duration `self.end - self.begin`.
    pub fn duration(self) -> Time;

    /// Returns the exact rational midpoint `(self.begin + self.end) / 2`.
    pub fn midpoint(self) -> Time;

    /// Returns the intersection `[max(a.begin, b.begin), min(a.end, b.end))` if non-empty.
    pub fn intersection(self, other: Self) -> Option<Self>;

    /// TidalCycles alias for `intersection`.
    pub fn sect(self, other: Self) -> Option<Self>;

    /// Splits `self` at every integer cycle boundary, yielding contiguous sub-spans
    /// where each sub-span lies strictly within a single integer cycle `[n, n + 1)`.
    pub fn split_cycles(self) -> Vec<Self>;

    /// Applies a monotonic time-mapping function `f` to both `begin` and `end`.
    /// Contract: If `f(end) <= f(begin)`, callers must handle inversion or filter empty spans.
    pub fn with_time<F>(self, f: F) -> Self
    where
        F: FnMut(Time) -> Time;

    /// Shifts both `begin` and `end` by `offset`.
    pub fn shift(self, offset: Time) -> Self;

    /// Scales both `begin` and `end` by `factor`.
    pub fn scale(self, factor: Time) -> Self;

    /// Returns `true` iff `t >= self.begin && t < self.end`.
    pub fn contains_time(self, t: Time) -> bool;
}
```

### 1.3 Source Context & Events (`Context`, `Hap<T>`, `Event<T>`)

```rust
/// Source-code location span for REPL highlighting and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceLocation {
    pub start: usize,
    pub end: usize,
}

/// Metadata attached to a `Hap<T>`, tracking mini-notation source locations and tags.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Context {
    pub locations: Vec<SourceLocation>,
    pub tags: BTreeSet<String>,
}

impl Context {
    pub fn empty() -> Self;
    /// Combines two contexts by concatenating locations and unioning tags.
    pub fn combine(&self, other: &Self) -> Self;
}

/// A discrete or continuous event ("Hap" in Strudel, "Event" in TidalCycles).
///
/// Contracts:
/// 1. `part` is always non-empty (`part.begin < part.end`).
/// 2. If `whole` is `Some(w)`, then `w.begin <= part.begin && part.end <= w.end`.
/// 3. If `whole` is `None`, the event is continuous (e.g., sampled from a signal) and has no onset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hap<T> {
    pub whole: Option<TimeSpan>,
    pub part: TimeSpan,
    pub value: T,
    pub context: Context,
}

/// TidalCycles compatibility alias.
pub type Event<T> = Hap<T>;

impl<T> Hap<T> {
    /// Constructs a discrete `Hap` with `whole: Some(whole)`.
    pub fn discrete(whole: TimeSpan, part: TimeSpan, value: T, context: Context) -> Self;

    /// Constructs a continuous `Hap` with `whole: None`.
    pub fn continuous(part: TimeSpan, value: T, context: Context) -> Self;

    /// Returns `true` iff `whole` is `Some(w)` and `part.contains_time(w.begin)`
    /// (equivalently `w.begin >= part.begin && w.begin < part.end`).
    /// Guarantees an event triggers at most once across any partition of time.
    pub fn has_onset(&self) -> bool;

    /// Maps `value` using `f`, preserving `whole`, `part`, and `context`.
    pub fn with_value<U, F>(self, f: F) -> Hap<U>
    where
        F: FnOnce(T) -> U;

    /// Transforms both `whole` (if `Some`) and `part` using `f`.
    pub fn with_span<F>(self, f: F) -> Self
    where
        F: FnMut(TimeSpan) -> TimeSpan;

    /// Combines `self` and `other` using `f` on their values, intersecting `part`s,
    /// combining `context`s, and applying a caller-specified `whole` resolution rule.
    pub fn combine_with<U, V, F, W>(
        &self,
        other: &Hap<U>,
        combine_val: F,
        resolve_whole: W,
    ) -> Option<Hap<V>>
    where
        F: FnOnce(&T, &U) -> V,
        W: FnOnce(Option<TimeSpan>, Option<TimeSpan>) -> Option<TimeSpan>;
}
```

### 1.4 Dynamic Control Values, Control Maps, and Query State (`Value`, `ControlMap`, `State`)

```rust
/// Canonical musical pitch representation supporting MIDI, frequency, and note names.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum NoteValue {
    /// Pitch name with accidental and octave (e.g. "c4", "f#3", "bb2").
    Name(String),
    /// MIDI note number (may be fractional for microtonal pitch).
    Midi(f64),
}

/// Unified dynamic value representation for TidalCycles `Value` and Strudel control parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Rational(Time),
    String(String),
    Bool(bool),
    Note(NoteValue),
    List(Vec<Value>),
    Null,
}

impl Value {
    /// Numeric coercion for arithmetic control operators (`|+`, `|-`, `|*`, `|/`).
    pub fn as_f64(&self) -> Option<f64>;
    pub fn as_i64(&self) -> Option<i64>;
    pub fn as_rational(&self) -> Option<Time>;
    pub fn as_str(&self) -> Option<&str>;
}

/// Deterministic ordered map of control parameter names to `Value`s
/// (e.g. `"s"`, `"note"`, `"n"`, `"gain"`, `"pan"`, `"cutoff"`, `"speed"`).
pub type ControlMap = BTreeMap<String, Value>;

/// A pattern of control parameter maps, matching TidalCycles `ControlPattern`.
pub type ControlPattern = Pattern<ControlMap>;

/// Query context passed into every `Pattern::query` invocation.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub span: TimeSpan,
    pub controls: ControlMap,
}

impl State {
    pub fn new(span: TimeSpan) -> Self;
    pub fn with_span(&self, span: TimeSpan) -> Self;
    pub fn with_controls(&self, controls: ControlMap) -> Self;
}
```

### 1.5 Core Pattern Type (`Pattern<T>`)

A `Pattern<T>` is an immutable, thread-safe, pure function from `State` to `Vec<Hap<T>>`.

```rust
/// Query function type: pure, deterministic mapping from `State` to `Vec<Hap<T>>`.
pub type QueryFn<T> = dyn Fn(&State) -> Vec<Hap<T>> + Send + Sync + 'static;

/// Core pattern representation. Cloning is `O(1)` via reference counting.
#[derive(Clone)]
pub struct Pattern<T> {
    query_fn: StdArc<QueryFn<T>>,
}

impl<T: 'static> Pattern<T> {
    /// Constructs a `Pattern<T>` from a pure query function.
    /// Contract: Every returned `Hap<T>` must have `hap.part` contained within `state.span`.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&State) -> Vec<Hap<T>> + Send + Sync + 'static;

    /// Executes the pattern query for the given `State`.
    pub fn query(&self, state: &State) -> Vec<Hap<T>>;

    /// Convenience helper: queries `[begin, end)` with an empty `ControlMap`.
    pub fn query_arc(&self, begin: Time, end: Time) -> Vec<Hap<T>>;

    /// Queries `[0, 1)` (the first cycle) with an empty `ControlMap`.
    pub fn first_cycle(&self) -> Vec<Hap<T>>;

    /// Splits the query `state.span` at cycle boundaries, queries each cycle independently,
    /// and concatenates the results.
    pub fn split_queries(&self) -> Self;

    /// Transforms the `State` before passing it to `self.query`.
    pub fn with_query_state<F>(&self, f: F) -> Self
    where
        F: Fn(&State) -> State + Send + Sync + 'static;

    /// Transforms the query `TimeSpan` before passing it to `self.query`.
    pub fn with_query_span<F>(&self, f: F) -> Self
    where
        F: Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static;

    /// Transforms both `begin` and `end` of the query `TimeSpan` using `f`.
    pub fn with_query_time<F>(&self, f: F) -> Self
    where
        F: Fn(Time) -> Time + Send + Sync + 'static;

    /// Transforms each `Hap<T>` returned by `self.query`.
    pub fn with_hap<U, F>(&self, f: F) -> Pattern<U>
    where
        U: 'static,
        F: Fn(Hap<T>) -> Hap<U> + Send + Sync + 'static;

    /// Transforms both `whole` and `part` spans of every returned `Hap<T>`.
    pub fn with_hap_span<F>(&self, f: F) -> Self
    where
        F: Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static;

    /// Transforms both `begin` and `end` of `whole` and `part` spans of every returned `Hap<T>`.
    pub fn with_hap_time<F>(&self, f: F) -> Self
    where
        F: Fn(Time) -> Time + Send + Sync + 'static;

    /// Filters returned `Hap<T>`s by predicate `f`.
    pub fn filter_haps<F>(&self, f: F) -> Self
    where
        F: Fn(&Hap<T>) -> bool + Send + Sync + 'static;

    /// Filters returned `Hap<T>`s by their values.
    pub fn filter_values<F>(&self, f: F) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static;

    /// Attaches a `SourceLocation` to every `Hap` emitted by `self`.
    pub fn with_loc(&self, loc: SourceLocation) -> Self;
}
```

---

## 2. Pattern Algebra

### 2.1 Functor

```rust
pub trait PatternFunctor<T: 'static> {
    /// Maps `f` over every `Hap<T>`'s value without altering `whole`, `part`, or `context`.
    /// Law (Identity): `p.fmap(|x| x) == p`
    /// Law (Composition): `p.fmap(f).fmap(g) == p.fmap(|x| g(f(x)))`
    fn fmap<U, F>(&self, f: F) -> Pattern<U>
    where
        U: 'static,
        F: Fn(&T) -> U + Send + Sync + 'static;
}
```

### 2.2 Applicative (`app_both` `<*>`, `app_left` `<*`, `app_right` `*>`)

All three applicative combinators intersect `part` spans (`a.part.intersection(b.part)`), discard non-intersecting pairs, and combine `context`s. They differ exclusively in how `whole` is computed:

| Method | Operator | `whole` Resolution Rule | Semantic Meaning |
| :--- | :--- | :--- | :--- |
| `app_both` | `<*>` | `match (a.whole, b.whole) { (Some(wa), Some(wb)) => wa.intersection(wb), _ => None }` | Structure from both patterns; continuous if either is continuous. |
| `app_left` | `<*` | `a.whole` (left/function pattern's `whole`) | Preserves rhythmic structure & onsets of `self` (left). |
| `app_right` | `*>` | `b.whole` (right/value pattern's `whole`) | Preserves rhythmic structure & onsets of `other` (right). |

```rust
pub trait PatternApplicative<A: 'static, B: 'static> {
    /// `<*>`: Applies a pattern of functions to a pattern of values, intersecting both `whole` and `part`.
    fn app_both<F>(&self, values: &Pattern<A>) -> Pattern<B>
    where
        Self: Sized;

    /// `<*`: Applies a pattern of functions to a pattern of values, preserving `self`'s `whole` structure.
    fn app_left(&self, values: &Pattern<A>) -> Pattern<B>;

    /// `*>`: Applies a pattern of functions to a pattern of values, preserving `values`'s `whole` structure.
    fn app_right(&self, values: &Pattern<A>) -> Pattern<B>;
}

/// Curried lift helper for binary functions `f(a, b) -> c`.
pub fn lift_a2_both<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static;

pub fn lift_a2_left<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static;

pub fn lift_a2_right<A, B, C, F>(pa: &Pattern<A>, pb: &Pattern<B>, f: F) -> Pattern<C>
where
    A: 'static,
    B: 'static,
    C: 'static,
    F: Fn(&A, &B) -> C + Send + Sync + 'static;
```

### 2.3 Monad & All Four Joins (`unwrap`, `inner_join`, `outer_join`, `squeeze_join`)

Given a nested `Pattern<Pattern<T>>`:

```rust
pub trait PatternMonad<T: Clone + 'static> {
    /// Monadic bind using `unwrap` (`>>=`): maps `f` and flattens via `unwrap`.
    fn bind<U, F>(&self, f: F) -> Pattern<U>
    where
        U: Clone + 'static,
        F: Fn(&T) -> Pattern<U> + Send + Sync + 'static;

    /// `unwrap` (Tidal `unwrap` / Strudel `unwrap`):
    /// Queries outer pattern on `state.span`. For each outer `Hap` with `part`,
    /// queries inner pattern on `outer.part`. For each inner `Hap`, intersects `whole`s
    /// when both are `Some` (otherwise `None`) and intersects `part`s.
    fn unwrap(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `inner_join` (Tidal `innerJoin`):
    /// Structure comes from the INNER pattern.
    /// Queries outer on `state.span`, queries inner on `outer.part`,
    /// sets `whole = inner.whole`, and `part = outer.part.intersection(inner.part)`.
    fn inner_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `outer_join` (Tidal `outerJoin`):
    /// Structure comes from the OUTER pattern.
    /// Queries outer on `state.span`, queries inner on `outer.part`,
    /// sets `whole = outer.whole`, and `part = outer.part.intersection(inner.part)`.
    fn outer_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `squeeze_join` (Tidal `squeezeJoin` / Strudel `squeezeJoin`):
    /// Time-compresses one full cycle `[0, 1)` of each inner pattern into the
    /// `whole` (or `part` if `whole` is `None`) of its corresponding outer `Hap`.
    /// Inner `whole` and `part` are scaled and shifted into the outer span,
    /// then intersected with `outer.part`.
    fn squeeze_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;
}
```

### 2.4 Triggered Reset & Restart Joins (`_trigJoin` / `_TrigzeroJoin`)

```rust
pub trait PatternTrigJoin<T: Clone + 'static> {
    /// `_trig_join` (Tidal `_trigJoin` — `restart` semantics):
    /// Restarts the inner pattern from cycle `0` relative to each outer trigger event's
    /// onset (`outer.whole.begin`), preserving cycle-aligned phase within each trigger window.
    /// Non-onset fragments of an outer event are shifted identically using their `whole.begin`.
    fn trig_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `_trig_zero_join` (Tidal `_TrigzeroJoin` — `reset` semantics):
    /// Resets the inner pattern to absolute cycle `0` at each trigger event's onset,
    /// clamping/aligning the inner pattern to `[0, outer_duration)` so that each trigger
    /// replays the inner pattern strictly from time `0`.
    fn trig_zero_join(nested: &Pattern<Pattern<T>>) -> Pattern<T>;

    /// `reset`: Resets `self` whenever `trigger` emits an onset (`bool == true`).
    fn reset(&self, trigger: &Pattern<bool>) -> Pattern<T>;

    /// `restart`: Restarts `self` from cycle 0 whenever `trigger` emits an onset (`bool == true`).
    fn restart(&self, trigger: &Pattern<bool>) -> Pattern<T>;
}
```

---

## 3. Complete 1:1 Pattern & Combinator Catalog

### 3.1 Constructors

```rust
/// Returns a discrete pattern repeating `value` once per integer cycle `[n, n + 1)`.
pub fn pure<T: Clone + Send + Sync + 'static>(value: T) -> Pattern<T>;

/// Returns the empty pattern yielding `vec![]` for every query.
pub fn silence<T: 'static>() -> Pattern<T>;

/// Overlays all patterns simultaneously in time, concatenating their `Hap`s.
/// Identity element: `silence()`.
pub fn stack<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T>;

/// Plays `patterns` sequentially, one pattern per cycle, cycling modulo `patterns.len()`.
/// For cycle `c`, queries `patterns[c.rem_euclid(n)]` at time adjusted by `(c / n)`.
pub fn slowcat<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T>;

/// TidalCycles `slowcatPrime`: like `slowcat`, but queries each selected pattern at
/// the original cycle time `c` without dividing `c` by `patterns.len()`.
pub fn slowcat_prime<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T>;

/// Concatenates all `patterns` within a single cycle (`slowcat(patterns).fast(patterns.len())`).
pub fn fastcat<T: 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T>;

/// Concatenates weighted `(duration, pattern)` pairs within a single cycle,
/// allocating each pattern a proportional fraction `duration / sum(durations)` of the cycle.
pub fn timecat<T: 'static>(weighted: Vec<(Time, Pattern<T>)>) -> Pattern<T>;
```

### 3.2 Time Modifiers

```rust
impl<T: Clone + 'static> Pattern<T> {
    /// Speeds up the pattern by `factor` (`> 0`). If `factor < 0`, reverses and speeds up by `|factor|`.
    /// Supports patterned factors via `Pattern<Time>` (`fast_pat`).
    pub fn fast(&self, factor: Time) -> Self;
    pub fn fast_pat(&self, factor: &Pattern<Time>) -> Self;

    /// Slows down the pattern by `factor` (`fast(1 / factor)`).
    pub fn slow(&self, factor: Time) -> Self;
    pub fn slow_pat(&self, factor: &Pattern<Time>) -> Self;

    /// Reverses the order of events within each integer cycle `[n, n + 1)`.
    /// Maps time `t` to `sam(t) + 1 - cycle_pos(t)` (with `begin`/`end` swapped to preserve `begin < end`).
    pub fn rev(&self) -> Self;

    /// Shifts the pattern earlier in time by `offset` cycles (`with_query_time(|t| t + offset)`).
    pub fn early(&self, offset: Time) -> Self;
    pub fn early_pat(&self, offset: &Pattern<Time>) -> Self;

    /// Shifts the pattern later in time by `offset` cycles (`with_query_time(|t| t - offset)`).
    pub fn late(&self, offset: Time) -> Self;
    pub fn late_pat(&self, offset: &Pattern<Time>) -> Self;

    /// Samples a (possibly continuous) pattern at `n` evenly spaced discrete steps per cycle.
    pub fn segment(&self, n: Time) -> Self;

    /// Compresses each cycle of `self` into `[begin, end)` within the cycle (`0 <= begin < end <= 1`).
    pub fn compress(&self, begin: Time, end: Time) -> Self;

    /// Zooms into `[begin, end)` of each cycle, expanding it to fill the full cycle `[0, 1)`.
    pub fn zoom(&self, begin: Time, end: Time) -> Self;

    /// Applies `f` inside cycles scaled by `factor`: `self.fast(factor)` -> `f` -> `.slow(factor)`.
    pub fn inside<F>(&self, factor: Time, f: F) -> Self
    where
        F: Fn(&Self) -> Self;

    /// Applies `f` outside cycles scaled by `factor`: `self.slow(factor)` -> `f` -> `.fast(factor)`.
    pub fn outside<F>(&self, factor: Time, f: F) -> Self
    where
        F: Fn(&Self) -> Self;

    /// Loops the first `fraction` of each cycle across the entire cycle.
    pub fn linger(&self, fraction: Time) -> Self;

    /// Delays every second subdivision of `n` per cycle by `offset` (`0 <= offset <= 1`).
    pub fn swing_by(&self, offset: Time, n: Time) -> Self;

    /// Shorthand for `swing_by(Time::new(1, 3), n)`.
    pub fn swing(&self, n: Time) -> Self;
}
```

### 3.3 Conditional & Structural Combinators

```rust
impl<T: Clone + 'static> Pattern<T> {
    /// Applies `f` every `n` cycles (when `cycle % n == 0`), leaving other cycles unchanged.
    pub fn every<F>(&self, n: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Applies `f` to every `n`-th cycle with a cycle offset `offset` (`(cycle - offset) % n == 0`).
    pub fn every_offset<F>(&self, n: i64, offset: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Strudel `each` / Tidal `forEach`: applies each function in `fs` across consecutive cycles
    /// or applies `f` to each element/layer.
    pub fn each<F>(&self, fs: Vec<F>) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Applies `f` only during cycles where `cond(cycle_index)` returns `true`.
    pub fn when<C, F>(&self, cond: C, f: F) -> Self
    where
        C: Fn(i64) -> bool + Send + Sync + 'static,
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Applies `f` when `(cycle_index % a) >= b`.
    pub fn whenmod<F>(&self, a: i64, b: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Divides each cycle into `n` chunks and applies `f` to a different chunk each cycle.
    pub fn chunk<F>(&self, n: i64, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Repeats each event `n` times within its original `whole` duration.
    pub fn ply(&self, n: Time) -> Self;
    pub fn ply_pat(&self, n: &Pattern<Time>) -> Self;

    /// Stacks `self` with a time-shifted (`late(offset)`) and transformed (`f`) copy.
    pub fn off<F>(&self, offset: Time, f: F) -> Self
    where
        F: Fn(&Self) -> Self + Send + Sync + 'static;

    /// Stacks `self` with `f(self)`.
    pub fn superimpose<F>(&self, f: F) -> Self
    where
        F: Fn(&Self) -> Self;

    /// Echoes `self` `n` times, each delayed by `time` cycles with `gain` attenuated by `feedback^k`.
    pub fn stut(&self, n: usize, feedback: f64, time: Time) -> ControlPattern
    where
        Self: Sized;

    /// Strudel `echo`: `stut` with `(times, time, feedback)` parameter order.
    pub fn echo(&self, times: usize, time: Time, feedback: f64) -> ControlPattern
    where
        Self: Sized;

    /// Loops a slice of `cycles` length starting at `offset` cycles indefinitely.
    pub fn ribbon(&self, offset: Time, cycles: Time) -> Self;
}

/// Stereo juxtaposition combinators for `ControlPattern`.
pub trait StereoJux {
    /// Pans original pattern left (`pan = 0.0`) and `f(self)` right (`pan = 1.0`).
    fn jux<F>(&self, f: F) -> ControlPattern
    where
        F: Fn(&ControlPattern) -> ControlPattern;

    /// Pans original pattern to `0.5 - amount / 2` and `f(self)` to `0.5 + amount / 2`.
    fn jux_by<F>(&self, amount: f64, f: F) -> ControlPattern
    where
        F: Fn(&ControlPattern) -> ControlPattern;
}
```

### 3.4 Deterministic PRNG, Random Modifiers, and Continuous Signals

All randomness in `silverwave-core` MUST be a pure, deterministic function of `Time` (specifically `hap.whole_or_part().begin`), ensuring identical queries always produce identical results without mutable RNG state.

```rust
/// Deterministic time-to-unit-interval hash in `[0.0, 1.0)`.
/// Contract: Pure function of `Time`; uniform distribution over `[0.0, 1.0)`.
pub fn time_to_rand(t: Time) -> f64;

impl<T: Clone + 'static> Pattern<T> {
    /// Randomly drops events with probability `0.5` (`degrade_by(0.5)`).
    pub fn degrade(&self) -> Self;

    /// Randomly drops events with probability `prob` in `[0.0, 1.0]`,
    /// keyed deterministically by each `Hap`'s onset time.
    pub fn degrade_by(&self, prob: f64) -> Self;
    pub fn degrade_by_pat(&self, prob: &Pattern<f64>) -> Self;

    /// Inverse of `degrade`: retains the complementary 50% of events.
    pub fn undegrade(&self) -> Self;

    /// Inverse of `degrade_by(prob)`: retains events that `degrade_by(prob)` would drop.
    pub fn undegrade_by(&self, prob: f64) -> Self;
}

/// Continuous random signal in `[0.0, 1.0)` (`whole == None`).
pub fn rand() -> Pattern<f64>;

/// Continuous integer random signal in `0..n` (`whole == None`).
pub fn irand(n: i64) -> Pattern<i64>;

/// Continuous 1D smooth Perlin noise signal in `[0.0, 1.0]` (`whole == None`).
/// C1-continuous across integer cycle boundaries using smoothstep/quintic interpolation.
pub fn perlin() -> Pattern<f64>;

/// Continuous Berlin noise signal in `[0.0, 1.0]` (`whole == None`).
/// Piecewise-linear interpolation between deterministic per-cycle random anchors.
pub fn berlin() -> Pattern<f64>;

/// Continuous bipolar/unipolar oscillator signals in `[0.0, 1.0]` (`whole == None`).
pub fn sine() -> Pattern<f64>;
pub fn cosine() -> Pattern<f64>;
pub fn saw() -> Pattern<f64>;
pub fn tri() -> Pattern<f64>;
pub fn square() -> Pattern<f64>;

/// Scales a `[0.0, 1.0]` signal pattern into `[min, max]`.
pub trait SignalRange {
    fn range(&self, min: f64, max: f64) -> Pattern<f64>;
    fn rangex(&self, min: f64, max: f64) -> Pattern<f64>;
}
```

### 3.5 Tonal & Harmony (`@strudel/tonal` Parity)

```rust
/// Tonal scale/chord/voicing operations on note/degree/symbol patterns.
pub trait TonalExt {
    /// Maps integer scale degrees (or note offsets) through a named scale
    /// (e.g. `"C:major"`, `"A2:minor:pentatonic"`, `"D:dorian"`).
    fn scale(&self, scale_name: &Pattern<String>) -> Pattern<Value>;

    /// Transposes note values by `steps` within the active scale context.
    fn scale_transpose(&self, steps: &Pattern<i64>) -> Pattern<Value>;

    /// Parses chord symbols (e.g. `"Cmaj7"`, `"Dm7"`, `"G7b9"`) into chordal pitch sets.
    fn chord(&self, chord_pat: &Pattern<String>) -> Pattern<Value>;

    /// Expands chord symbols into voice-led voicings using the selected dictionary
    /// (e.g. `"ireal"`, `"lefthand"`, `"guidetones"`, `"triads"`).
    fn voicing(&self) -> Pattern<Value>;

    /// Sets the voicing dictionary used by `voicing()`.
    fn dict(&self, dictionary: &Pattern<String>) -> Pattern<Value>;

    /// Extracts the root note of each chord in a chord pattern.
    fn root_notes(&self, octave: i32) -> Pattern<Value>;

    /// Anchors voicing register around a target pitch (e.g. `"c4"`).
    fn anchor(&self, pitch: &Pattern<String>) -> Pattern<Value>;
}
```

### 3.6 `ControlPattern` Operators

ControlPattern operators combine two `ControlPattern`s (`Pattern<ControlMap>`) using one of the three applicative alignment strategies (`app_left`, `app_right`, `app_both`) and a key-level merge rule:

| Operator | Method Name | Alignment | Key Merge Semantics |
| :--- | :--- | :--- | :--- |
| `\|>` / `#` | `set_right` / `union_right` | `app_left` (default) or `app_both` | Union of maps; right-hand key overwrites left-hand key on collision. |
| `<\|` | `set_left` / `union_left` | `app_left` | Union of maps; left-hand key is preserved on collision. |
| `\|+` | `add` | `app_left` | Numeric addition for shared keys (`a + b`); non-shared keys preserved. |
| `\|-` | `sub` | `app_left` | Numeric subtraction for shared keys (`a - b`); non-shared keys preserved. |
| `\|*` | `mul` | `app_left` | Numeric multiplication for shared keys (`a * b`); non-shared keys preserved. |
| `\|/` | `div` | `app_left` | Numeric division for shared keys (`a / b`); division by zero yields `0.0` or omits. |

```rust
pub trait ControlPatternOps {
    /// `|>` and `#`: merges `other` into `self`, preferring `other`'s values on key collision.
    fn union_right(&self, other: &ControlPattern) -> ControlPattern;
    /// `<|`: merges `other` into `self`, preferring `self`'s values on key collision.
    fn union_left(&self, other: &ControlPattern) -> ControlPattern;

    /// `|+`, `|-`, `|*`, `|/1`: structure-preserving (`app_left`) arithmetic on matching keys.
    fn add_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn sub_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn mul_ctrl(&self, other: &ControlPattern) -> ControlPattern;
    fn div_ctrl(&self, other: &ControlPattern) -> ControlPattern;

    /// Variants with explicit alignment (`app_left`, `app_right`, `app_both`).
    fn add_ctrl_in(&self, other: &ControlPattern) -> ControlPattern;
    fn add_ctrl_out(&self, other: &ControlPattern) -> ControlPattern;
    fn add_ctrl_both(&self, other: &ControlPattern) -> ControlPattern;
}
```


### 3.7 Euclidean, Stepwise, Sample-Slicing & Microtonal (`ScalaTuning`) Combinators

```rust
impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Distributes `pulses` onsets across `steps` subdivisions using Bjorklund's algorithm,
    /// rotated by `rotation` steps (`euclid`, `euclidRot`).
    pub fn euclid(&self, pulses: isize, steps: isize) -> Self;
    pub fn euclid_rot(&self, pulses: isize, steps: isize, rotation: isize) -> Self;

    /// Inverted Euclidean rhythm (`euclidInv`): plays on the rests of `euclid(pulses, steps)`.
    pub fn euclid_inv(&self, pulses: isize, steps: isize) -> Self;

    /// Alternates `self` on Euclidean hits and `other` on Euclidean rests (`euclidFull`).
    pub fn euclid_full(&self, pulses: isize, steps: isize, other: &Self) -> Self;

    /// Strudel Stepwise combinators: metrical step-counting & proportional expansion.
    pub fn step(&self, steps_per_cycle: Time) -> Self;
    pub fn expand(&self, factor: Time) -> Self;
    pub fn pace(&self, target_steps: Time) -> Self;
    pub fn take_steps(&self, count: isize) -> Self;
    pub fn drop_steps(&self, count: isize) -> Self;
}

/// Concatenate patterns proportionally by their intrinsic metrical step counts (`stepcat`).
pub fn stepcat<T: Clone + Send + Sync + 'static>(patterns: Vec<Pattern<T>>) -> Pattern<T>;

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

/// Microtonal Scala (`.scl` / `.kbm`) & N-EDO tuning contract.
#[derive(Clone, Debug, PartialEq)]
pub struct ScalaTuning {
    pub description: String,
    pub pitches_cents: Vec<f64>,
    pub reference_midi: i32,
    pub reference_freq_hz: f64,
}

impl ScalaTuning {
    /// Parses standard Scala `.scl` and optional `.kbm` keyboard mapping text into a tuning table.
    pub fn from_scl_kbm(scl_text: &str, kbm_text: Option<&str>) -> Result<Self, String>;
    /// Constructs an `n`-division equal temperament of the `octave_ratio` (default `2.0`).
    pub fn edo(divisions: u32, octave_ratio: f64) -> Self;
    /// Resolves scale step `degree` to exact frequency in Hz.
    pub fn degree_to_freq_hz(&self, degree: f64) -> f64;
}
```

---

## 4. TDD Test Contract & Property-Based Invariants

Every implementation of `silverwave-core` MUST satisfy the following deterministic unit tests and `proptest` property-based invariants before release.

### 4.1 Rational Drift-Freedom

- **Contract:** Repeated `fast(n)`, `slow(n)`, `early(d)`, `late(d)`, and `split_cycles()` operations over `n >= 10_000` cycles MUST produce exact rational `Time` values with zero floating-point rounding error.
- **Property Test:**
  - `∀ p: Pattern<T>, ∀ k ∈ [1..10_000], ∀ q ∈ TimeSpan`: `p.fast(3).slow(3).query_arc(q.begin, q.end) == p.query_arc(q.begin, q.end)`.
  - `∀ p: Pattern<T>, ∀ d: Time`: `p.early(d).late(d).query_arc(0, 100) == p.query_arc(0, 100)`.
  - `sum(hap.part.duration() for hap in fastcat(vec![pure(1); 7]).query_arc(0, 10_000)) == Ratio::from_integer(10_000)`.

### 4.2 Onset Uniqueness & `whole`/`part` Structural Invariants

- **Contract:** For any `Pattern<T>` and any query `span`:
  1. **Span Containment:** Every returned `hap` satisfies `hap.part.begin >= span.begin && hap.part.end <= span.end && hap.part.begin < hap.part.end`.
  2. **Whole Containment:** If `hap.whole == Some(w)`, then `w.begin <= hap.part.begin && hap.part.end <= w.end`.
  3. **Onset Uniqueness Across Partitions:** For any partition of `[0, N)` into contiguous sub-spans `[t_0, t_1), [t_1, t_2), ..., [t_{m-1}, t_m)`, the multiset of `Hap`s where `hap.has_onset() == true` is identical to the onset `Hap`s from querying `[0, N)` in a single call. No event may trigger twice or be missed across a scheduler tick boundary.
  4. **Continuous Signal Contract:** `rand()`, `perlin()`, `berlin()`, `sine()`, `cosine()`, `saw()`, `tri()`, `square()` always emit `Hap`s with `whole: None` and `has_onset() == false` until discretized via `segment(n)` or combined via `app_left`/`app_right`.

### 4.3 Algebraic Laws

- **Functor Laws:**
  - Identity: `p.fmap(|x| x.clone()) == p` over arbitrary query spans.
  - Composition: `p.fmap(f).fmap(g) == p.fmap(|x| g(&f(x)))`.
- **Applicative Laws:**
  - Identity: `pure(id).app_both(&p) == p` (and for `app_left`, `app_right`).
  - Homomorphism: `pure(f).app_both(&pure(x)) == pure(f(x))`.
  - Interchange: `u.app_both(&pure(y)) == pure(|f| f(y)).app_both(&u)`.
- **Monad Laws (`unwrap`, `inner_join`, `outer_join`, `squeeze_join`):**
  - Left Identity: `pure(x).bind(f) == f(&x)`.
  - Right Identity: `p.bind(pure) == p`.
  - Associativity: `p.bind(f).bind(g) == p.bind(|x| f(x).bind(g))`.
  - Join Whole-Preservation:
    - `inner_join` preserves inner `whole`.
    - `outer_join` preserves outer `whole`.
    - `squeeze_join` maps `[0, 1)` of inner into each outer `whole` without dropping inner subdivisions.
- **Combinator Identities:**
  - `stack(vec![p, silence()]) == p`.
  - `p.rev().rev() == p`.
  - `p.fast(1) == p` and `p.slow(1) == p`.
  - `trig_join` vs `trig_zero_join`: `restart` preserves cycle-relative phase on trigger; `reset` always starts inner pattern at `t = 0`.

### 4.4 Deterministic Time-Hashed PRNG & Signal Invariants

- **Determinism:** `∀ t_span: TimeSpan`: `rand().query_arc(t_span.begin, t_span.end)` called twice (or on different threads) yields bit-for-bit identical `f64` values.
- **Bounds:**
  - `rand()`, `perlin()`, `berlin()`, `sine()`, `cosine()`, `saw()`, `tri()`, `square()` always emit values in `[0.0, 1.0]`.
  - `irand(n)` always emits integers in `0..n` for `n > 0`.
- **Uniformity & Distribution:** Sampling `rand().segment(1)` over `10_000` cycles yields empirical mean within `0.5 ± 0.02` and passes chi-squared uniformity across 10 bins.
- **Continuity:** `perlin()` and `berlin()` satisfy `|val(t + ε) - val(t)| -> 0` as `ε -> 0` across integer cycle boundaries.
- **Complementarity:** For any discrete pattern `p` and `prob ∈ [0.0, 1.0]`, `p.degrade_by(prob)` and `p.undegrade_by(prob)` partition `p`'s events disjointly: their union equals `p` and their intersection is empty.

---

## 5. 1:1+ & Category-King Core Pattern Extensions (`PP-03`–`PP-08`, `PP-11`, `CK-01`, `CK-02`, `CK-04`)

### 5.1 Canonical Rational Type Aliases & `i128` Overflow Safety
To maintain type identity across `silverwave-core`, `silverwave-mini`, `silverwave-io`, and `silverwave-mcp`, `Rational64`, `Rational`, and `Time` are unified aliases backed by checked `i128` intermediate arithmetic:

```rust
pub type Rational64 = Rational;
pub type Time = Rational;

impl Rational {
    /// Computes exact arithmetic in `i128` space before GCD reduction; if a deeply nested
    /// polymetric denominator exceeds `MAX_DENOMINATOR = 1_000_000_007`, deterministically
    /// rationalizes via Stern-Brocot continued fraction approximation without panicking.
    pub const MAX_DENOMINATOR: i64 = 1_000_000_007;
    pub fn checked_op_stern_brocot(num: i128, den: i128) -> Self;
}
```

### 5.2 Epoch-Checkpointed `StatefulPattern<S, T>`, Dual-Timeframe `QueryContext` & Elektron `TrigCondition`
- **`StatefulPattern<S, T>` & `CycleStateCheckpointer<S>`**: Maintains a bounded LRU ring (`MAX_STATE_CHECKPOINTS_PER_STREAM = 256`) of cycle-boundary states `S` so `.accum()`, `.scan()`, `.markov()`, and `.lsystem()` evaluate in $O(1)$ amortized time per cycle while preserving referential transparency.
- **Dual-Timeframe `QueryContext` (`local_span` vs `global_span`)**: Hardware/UI controllers (`cF`, `^var`, `slider`, `cc`) query `global_span` by default (resolving [Tidal Issue #1029](https://github.com/tidalcycles/Tidal/issues/1029)), while `cF_local` / `^^var` queries `local_span`.
- **Elektron Conditional Trigs (`TrigCondition`)**: Native support for `TrigCondition::Ratio { a, b }` (`@1:4`), `FirstCycle { expected }` (`@1st`/`@!1st`), `FillLatch { active }` (`@fill`/`@!fill`), `PreviousConditionalOnSameStream { expected }` (`@pre`/`@!pre`), and `PreviousConditionalOnNeighborStream { expected }` (`@nei`/`@!nei`).
- **Step-Indexed Parameter Locks, M8 Sub-Tables, Groove Swing, Legato Glides & Timeline One-Shots**:
  - `.plock(step_idx: u16, total_steps: u16, patch: Pattern<ControlMap>)`
  - `.table(tick_rate: Rational, sub_pattern: Pattern<ControlMap>)`
  - `.groove(tick_weights: &[u8])`
  - `.glide(duration_cycles: Rational, curve: GlideCurve)`
  - `once(pat)`, `asap(pat)`, `qtrigger(stream_id, pat)`, `.fade_in(cycles)`, `.fade_out(cycles)`, and zero-division-safe `arrange(&[(Pattern<Rational>, Pattern<T>)])`.
- **$O((N + M) \log(N + M))$ Sweep-Line `Hap` Intersection**: All control merge operators (`#`, `|>`, `|<`, `struct`) sort `SmallVec<[Hap<T>; 16]>` spans by `part.begin` and execute a monotonic sweep-line join.
- **Unified Superset Chord & Voicing Dictionary**: Simultaneously parses `ireal` (`C^7`, `Co7`), standard (`Cmaj7`, `Cdim7`), and Tidal (`c'maj7`, `gm13`, `a7s5`) symbols with per-token `@dict` voicing tags.
