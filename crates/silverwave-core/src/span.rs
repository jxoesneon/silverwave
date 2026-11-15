//! Half-open rational time intervals (`TimeSpan` / `Arc`), source locations (`SourceLocation`),
//! event context (`Context`), and discrete/continuous events (`Hap<T>` / `Event<T>`).

use crate::rational::{Time, TimeExt};
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

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
    #[inline]
    pub fn new(begin: Time, end: Time) -> Option<Self> {
        if begin < end {
            Some(Self { begin, end })
        } else {
            None
        }
    }

    /// Constructs a `TimeSpan` without returning `Option`, ensuring `end > begin` if equal/inverted.
    #[inline]
    pub fn new_clamped(begin: Time, end: Time) -> Self {
        if begin < end {
            Self { begin, end }
        } else if end < begin {
            Self {
                begin: end,
                end: begin,
            }
        } else {
            Self {
                begin,
                end: begin + Time::new_const(1, 1_000_000),
            }
        }
    }

    /// Constructs a `TimeSpan` representing the single cycle `[sam(t), next_sam(t))`.
    #[inline]
    pub fn cycle_of(t: Time) -> Self {
        Self {
            begin: t.sam(),
            end: t.next_sam(),
        }
    }

    /// Returns the duration `self.end - self.begin`.
    #[inline]
    pub fn duration(self) -> Time {
        self.end - self.begin
    }

    /// Returns the exact rational midpoint `(self.begin + self.end) / 2`.
    #[inline]
    pub fn midpoint(self) -> Time {
        (self.begin + self.end) / Time::from_integer(2)
    }

    /// Returns the intersection `[max(a.begin, b.begin), min(a.end, b.end))` if non-empty.
    #[inline]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let b = self.begin.max(other.begin);
        let e = self.end.min(other.end);
        Self::new(b, e)
    }

    /// TidalCycles alias for `intersection`.
    #[inline]
    pub fn sect(self, other: Self) -> Option<Self> {
        self.intersection(other)
    }

    /// Splits `self` at every integer cycle boundary, yielding contiguous sub-spans
    /// where each sub-span lies strictly within a single integer cycle `[n, n + 1)`.
    pub fn split_cycles(self) -> Vec<Self> {
        let mut out = Vec::new();
        let mut cur = self.begin;
        while cur < self.end {
            let next = cur.next_sam().min(self.end);
            if let Some(sub) = Self::new(cur, next) {
                out.push(sub);
            }
            cur = next;
        }
        out
    }

    /// Applies a time-mapping function `f` to both `begin` and `end`.
    pub fn with_time<F>(self, mut f: F) -> Self
    where
        F: FnMut(Time) -> Time,
    {
        let b = f(self.begin);
        let e = f(self.end);
        if b <= e {
            Self { begin: b, end: e }
        } else {
            Self { begin: e, end: b }
        }
    }

    /// Shifts both `begin` and `end` by `offset`.
    #[inline]
    pub fn shift(self, offset: Time) -> Self {
        Self {
            begin: self.begin + offset,
            end: self.end + offset,
        }
    }

    /// Scales both `begin` and `end` by `factor`.
    #[inline]
    pub fn scale(self, factor: Time) -> Self {
        let b = self.begin * factor;
        let e = self.end * factor;
        if b <= e {
            Self { begin: b, end: e }
        } else {
            Self { begin: e, end: b }
        }
    }

    /// Returns `true` iff `t >= self.begin && t < self.end`.
    #[inline]
    pub fn contains_time(self, t: Time) -> bool {
        t >= self.begin && t < self.end
    }
}

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
    /// Creates an empty `Context`.
    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Combines two contexts by concatenating deduplicated locations and unioning tags.
    pub fn combine(&self, other: &Self) -> Self {
        let mut locations = self.locations.clone();
        for loc in &other.locations {
            if !locations.contains(loc) {
                locations.push(*loc);
            }
        }
        let mut tags = self.tags.clone();
        for tag in &other.tags {
            tags.insert(tag.clone());
        }
        Self { locations, tags }
    }
}

/// A discrete or continuous event ("Hap" in Strudel, "Event" in TidalCycles).
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
    #[inline]
    pub fn discrete(whole: TimeSpan, part: TimeSpan, value: T, context: Context) -> Self {
        Self {
            whole: Some(whole),
            part,
            value,
            context,
        }
    }

    /// Constructs a continuous `Hap` with `whole: None`.
    #[inline]
    pub fn continuous(part: TimeSpan, value: T, context: Context) -> Self {
        Self {
            whole: None,
            part,
            value,
            context,
        }
    }

    /// Returns `whole` if `Some`, otherwise `part`.
    #[inline]
    pub fn whole_or_part(&self) -> TimeSpan {
        self.whole.unwrap_or(self.part)
    }

    /// Returns `true` iff `whole` is `Some(w)` and `part.contains_time(w.begin)`.
    #[inline]
    pub fn has_onset(&self) -> bool {
        match self.whole {
            Some(w) => self.part.contains_time(w.begin),
            None => false,
        }
    }

    /// Maps `value` using `f`, preserving `whole`, `part`, and `context`.
    #[inline]
    pub fn with_value<U, F>(self, f: F) -> Hap<U>
    where
        F: FnOnce(T) -> U,
    {
        Hap {
            whole: self.whole,
            part: self.part,
            value: f(self.value),
            context: self.context,
        }
    }

    /// Transforms both `whole` (if `Some`) and `part` using `f`.
    #[inline]
    pub fn with_span<F>(self, mut f: F) -> Self
    where
        F: FnMut(TimeSpan) -> TimeSpan,
    {
        Self {
            whole: self.whole.map(&mut f),
            part: f(self.part),
            value: self.value,
            context: self.context,
        }
    }

    /// Combines `self` and `other` using `combine_val` on their values, intersecting `part`s,
    /// combining `context`s, and applying a caller-specified `whole` resolution rule.
    pub fn combine_with<U, V, F, W>(
        &self,
        other: &Hap<U>,
        combine_val: F,
        resolve_whole: W,
    ) -> Option<Hap<V>>
    where
        F: FnOnce(&T, &U) -> V,
        W: FnOnce(Option<TimeSpan>, Option<TimeSpan>) -> Option<TimeSpan>,
    {
        let part = self.part.intersection(other.part)?;
        let whole = resolve_whole(self.whole, other.whole);
        // If `self` or `other` was discrete and `resolve_whole` expects an intersection that became empty,
        // `whole` may be `None` when both inputs had `Some` and failed to overlap.
        Some(Hap {
            whole,
            part,
            value: combine_val(&self.value, &other.value),
            context: self.context.combine(&other.context),
        })
    }
}
