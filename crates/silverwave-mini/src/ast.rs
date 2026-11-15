//! Location-annotated two-stage `TPat<T>` AST (`02-SPEC-MINI-NOTATION-AND-DSL.md` Section 2.1),
//! scientific pitch `NoteSpec`, and inline modular DSP pipe AST (`CK-05`).

use crate::span::ByteSpan;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use silverwave_core::{Rational, TrigCondition};

/// Parsed scientific pitch specification (`c4`, `eb3`, `f#5`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSpec {
    /// Semitone offset from C (`0..12`, or signed with accidentals).
    pub pitch_class: i32,
    /// Octave number (default `3` or `4` per context).
    pub octave: i32,
    /// Whether the input string explicitly included an octave digit.
    pub explicit_octave: bool,
}

impl NoteSpec {
    /// Converts `self` to a MIDI note number (`C4 = 60`).
    #[inline]
    pub const fn to_midi(&self) -> f64 {
        (12 * (self.octave + 1) + self.pitch_class) as f64
    }
}

/// Inline modular DSP pipe stage (`CK-05`: `"saw(440) >> lpf(1200, 0.8) >> tanh(2.0)"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DspPipeStage {
    pub op: String,
    pub args: Vec<String>,
    pub span: ByteSpan,
}

/// Location-annotated Two-Stage Mini-Notation + Mondo AST (`TPat<T>`).
#[derive(Clone, Debug, PartialEq)]
pub enum TPat<T> {
    Atom {
        value: T,
        span: ByteSpan,
    },
    Var {
        name: String,
        local_time: bool,
        span: ByteSpan,
    },
    Silence {
        span: ByteSpan,
    },
    Tie {
        span: ByteSpan,
    },
    Seq {
        items: Vec<TPat<T>>,
        span: ByteSpan,
    },
    Stack {
        layers: Vec<TPat<T>>,
        span: ByteSpan,
    },
    Slowcat {
        items: Vec<TPat<T>>,
        span: ByteSpan,
    },
    Polymeter {
        layers: Vec<TPat<T>>,
        steps: Option<Box<TPat<Rational>>>,
        span: ByteSpan,
    },
    Fast {
        factor: Box<TPat<Rational>>,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    Slow {
        factor: Box<TPat<Rational>>,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    Elongate {
        weight: Rational,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    TrigCond {
        cond: TrigCondition,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    Repeat {
        count: Box<TPat<usize>>,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    DegradeBy {
        prob: Box<TPat<f64>>,
        seed_salt: u64,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    EnumFromTo {
        start: Box<TPat<T>>,
        step: Option<Box<TPat<T>>>,
        end: Box<TPat<T>>,
        span: ByteSpan,
    },
    Euclid {
        pulses: Box<TPat<isize>>,
        steps: Box<TPat<isize>>,
        offset: Option<Box<TPat<isize>>>,
        inner: Box<TPat<T>>,
        span: ByteSpan,
    },
    Choose {
        options: Vec<TPat<T>>,
        span: ByteSpan,
    },
    SampleIndex {
        target: Box<TPat<T>>,
        index: Box<TPat<usize>>,
        span: ByteSpan,
    },
    Chord {
        symbol: String,
        root: NoteSpec,
        quality: String,
        inversion: Option<NoteSpec>,
        dict_tag: Option<String>,
        span: ByteSpan,
    },
    MondoCall {
        func: String,
        args: Vec<TPat<T>>,
        span: ByteSpan,
    },
}

impl<T> TPat<T> {
    /// Returns the `ByteSpan` of this AST node.
    pub fn span(&self) -> ByteSpan {
        match self {
            TPat::Atom { span, .. }
            | TPat::Var { span, .. }
            | TPat::Silence { span }
            | TPat::Tie { span }
            | TPat::Seq { span, .. }
            | TPat::Stack { span, .. }
            | TPat::Slowcat { span, .. }
            | TPat::Polymeter { span, .. }
            | TPat::Fast { span, .. }
            | TPat::Slow { span, .. }
            | TPat::Elongate { span, .. }
            | TPat::TrigCond { span, .. }
            | TPat::Repeat { span, .. }
            | TPat::DegradeBy { span, .. }
            | TPat::EnumFromTo { span, .. }
            | TPat::Euclid { span, .. }
            | TPat::Choose { span, .. }
            | TPat::SampleIndex { span, .. }
            | TPat::Chord { span, .. }
            | TPat::MondoCall { span, .. } => span.clone(),
        }
    }

    /// Collects all `ByteSpan`s in the AST in pre-order traversal.
    pub fn collect_spans(&self, out: &mut Vec<ByteSpan>) {
        out.push(self.span());
        match self {
            TPat::Seq { items, .. }
            | TPat::Stack { layers: items, .. }
            | TPat::Slowcat { items, .. }
            | TPat::Choose { options: items, .. }
            | TPat::MondoCall { args: items, .. } => {
                for item in items {
                    item.collect_spans(out);
                }
            }
            TPat::Polymeter { layers, steps, .. } => {
                for l in layers {
                    l.collect_spans(out);
                }
                if let Some(s) = steps {
                    s.collect_spans(out);
                }
            }
            TPat::Fast { factor, inner, .. } | TPat::Slow { factor, inner, .. } => {
                inner.collect_spans(out);
                factor.collect_spans(out);
            }
            TPat::Elongate { inner, .. } | TPat::TrigCond { inner, .. } => {
                inner.collect_spans(out);
            }
            TPat::Repeat { count, inner, .. } => {
                inner.collect_spans(out);
                count.collect_spans(out);
            }
            TPat::DegradeBy { prob, inner, .. } => {
                inner.collect_spans(out);
                prob.collect_spans(out);
            }
            TPat::EnumFromTo {
                start, step, end, ..
            } => {
                start.collect_spans(out);
                if let Some(s) = step {
                    s.collect_spans(out);
                }
                end.collect_spans(out);
            }
            TPat::Euclid {
                pulses,
                steps,
                offset,
                inner,
                ..
            } => {
                inner.collect_spans(out);
                pulses.collect_spans(out);
                steps.collect_spans(out);
                if let Some(o) = offset {
                    o.collect_spans(out);
                }
            }
            TPat::SampleIndex { target, index, .. } => {
                target.collect_spans(out);
                index.collect_spans(out);
            }
            _ => {}
        }
    }
}
