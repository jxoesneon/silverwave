//! Dynamic control values (`Value`, `NoteValue`, `ControlMap`), compatibility profiles
//! (`EngineCompatibilityProfile` solving `PP-11`), and dual-timeframe query states (`PP-04`).

use crate::rational::{Rational64, Time};
use crate::span::TimeSpan;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Canonical musical pitch representation supporting MIDI, frequency, and note names.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum NoteValue {
    /// Pitch name with accidental and octave (e.g. "c4", "f#3", "bb2").
    Name(String),
    /// MIDI note number (may be fractional for microtonal pitch).
    Midi(f64),
}

impl NoteValue {
    /// Parses a pitch name like `"c4"`, `"f#3"`, `"bb2"`, `"eb5"`, `"g#-1"` into a semitone offset
    /// relative to `C0` (`MIDI 12`).
    pub fn parse_pitch_name(name: &str) -> Option<(i32, i32)> {
        let s = name.trim();
        if s.is_empty() {
            return None;
        }
        let mut chars = s.chars().peekable();
        let base = match chars.next()?.to_ascii_lowercase() {
            'c' => 0,
            'd' => 2,
            'e' => 4,
            'f' => 5,
            'g' => 7,
            'a' => 9,
            'b' => 11,
            _ => return None,
        };
        let mut accidental = 0i32;
        while let Some(&c) = chars.peek() {
            match c {
                '#' | 's' | 'S' => {
                    accidental += 1;
                    chars.next();
                }
                'b' | 'f' | 'F' => {
                    accidental -= 1;
                    chars.next();
                }
                _ => break,
            }
        }
        let rest: String = chars.collect();
        let octave = if rest.is_empty() {
            4
        } else {
            rest.parse::<i32>().ok()?
        };
        Some((base + accidental, octave))
    }

    /// Converts `self` to a fractional MIDI note number under the specified compatibility profile.
    pub fn to_midi(&self, profile: EngineCompatibilityProfile) -> Option<f64> {
        match self {
            NoteValue::Midi(m) => Some(*m),
            NoteValue::Name(s) => {
                let (pc, oct) = Self::parse_pitch_name(s)?;
                let c_zero_midi = match profile {
                    EngineCompatibilityProfile::SuperDoughC4 => 12.0,
                    EngineCompatibilityProfile::SuperDirtC5 => 0.0,
                };
                Some(c_zero_midi + (oct as f64) * 12.0 + (pc as f64))
            }
        }
    }

    /// Converts `self` to frequency in Hz (`A4 = 440.0 Hz`, MIDI 69).
    pub fn to_freq_hz(&self, profile: EngineCompatibilityProfile) -> Option<f64> {
        let midi = self.to_midi(profile)?;
        Some(440.0 * exp2_f64((midi - 69.0) / 12.0))
    }
}

/// Fast, dependency-free $2^x$ approximation accurate to $< 10^{-13}$ across musical pitch ranges.
#[inline]
pub fn exp2_f64(x: f64) -> f64 {
    let ln2 = core::f64::consts::LN_2;
    let k = if x >= 0.0 {
        (x + 0.5) as i32
    } else {
        (x - 0.5) as i32
    };
    let r = (x - (k as f64)) * ln2;
    // 11-term Taylor series for exp(r) where |r| <= ln(2)/2
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut i = 1;
    while i <= 14 {
        term *= r / (i as f64);
        sum += term;
        i += 1;
    }
    let scale = if k >= 0 {
        if k < 62 {
            (1u64 << k) as f64
        } else {
            f64::from_bits(((1023 + k) as u64) << 52)
        }
    } else if k > -62 {
        1.0 / ((1u64 << (-k)) as f64)
    } else {
        f64::from_bits(((1023 + k).max(1) as u64) << 52)
    };
    sum * scale
}

/// Engine compatibility profile resolving `StrudelDirt` (`superdough`) vs `SuperDirt` semantic
/// divergences (`PP-11`, SuperDirt Issue #302).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineCompatibilityProfile {
    /// Strudel (`superdough`) defaults: `note 0` = `C4` (`261.63 Hz`), filter env inherits amp ADSR,
    /// sustained synths scaled by `0.5` (`-6.02 dB`).
    SuperDoughC4,
    /// TidalCycles (`SuperDirt`) defaults: `note 0` = `C5` (`523.25 Hz`), independent filter env,
    /// unity synth gain (`1.0`).
    SuperDirtC5,
}

impl EngineCompatibilityProfile {
    /// Parses `"strudel"` / `"superdough"` vs `"superdirt"` / `"tidal"`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "strudel" | "superdough" | "c4" => Some(Self::SuperDoughC4),
            "superdirt" | "tidal" | "tidalcycles" | "c5" => Some(Self::SuperDirtC5),
            _ => None,
        }
    }

    /// Octave assigned to `note 0`.
    pub const fn middle_c_octave(self) -> i32 {
        match self {
            Self::SuperDoughC4 => 4,
            Self::SuperDirtC5 => 5,
        }
    }

    /// Gain scalar applied to sustained synthesizers relative to percussion samples.
    pub const fn sustained_synth_gain_scale(self) -> f64 {
        match self {
            Self::SuperDoughC4 => 0.5,
            Self::SuperDirtC5 => 1.0,
        }
    }

    /// Whether the filter envelope inherits the amplitude ADSR when unset.
    pub const fn filter_env_inherits_amp_adsr(self) -> bool {
        match self {
            Self::SuperDoughC4 => true,
            Self::SuperDirtC5 => false,
        }
    }

    /// Converts a semitone offset (`note 0` = middle C) into MIDI note number.
    pub const fn note_offset_to_midi(self, semitones: f64) -> f64 {
        match self {
            Self::SuperDoughC4 => 60.0 + semitones,
            Self::SuperDirtC5 => 72.0 + semitones,
        }
    }
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
    /// Coerces numeric values (`Float`, `Int`, `Rational`, `Bool`, `Note`) to `f64`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            Value::Rational(r) => Some(r.to_f64()),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Note(NoteValue::Midi(m)) => Some(*m),
            Value::Note(NoteValue::Name(s)) => {
                NoteValue::Name(s.clone()).to_midi(EngineCompatibilityProfile::SuperDoughC4)
            }
            _ => None,
        }
    }

    /// Coerces integer-compatible values to `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            Value::Rational(r) => Some(r.floor_i64()),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    /// Coerces rational-compatible values to `Time`.
    pub fn as_rational(&self) -> Option<Time> {
        match self {
            Value::Rational(r) => Some(*r),
            Value::Int(v) => Some(Time::from_integer(*v)),
            Value::Float(v) => Time::from_f64_approx(*v, 1_000_000),
            _ => None,
        }
    }

    /// Returns `&str` if `Value::String` or `Value::Note(NoteValue::Name)`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::Note(NoteValue::Name(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Coerces boolean or non-zero numeric values to `bool` (`PP-06`).
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            Value::Float(f) => Some(*f != 0.0),
            Value::Rational(r) => Some(!r.is_zero()),
            _ => None,
        }
    }

    /// Arithmetic addition for `ControlPatternOps::add_ctrl` (`|+`).
    pub fn add_numeric(&self, other: &Self) -> Self {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_add(*b)),
            (Value::Rational(a), Value::Rational(b)) => Value::Rational(*a + *b),
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => Value::Float(a + b),
                _ => other.clone(),
            },
        }
    }

    /// Arithmetic subtraction for `ControlPatternOps::sub_ctrl` (`|-`).
    pub fn sub_numeric(&self, other: &Self) -> Self {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_sub(*b)),
            (Value::Rational(a), Value::Rational(b)) => Value::Rational(*a - *b),
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => Value::Float(a - b),
                _ => self.clone(),
            },
        }
    }

    /// Arithmetic multiplication for `ControlPatternOps::mul_ctrl` (`|*`).
    pub fn mul_numeric(&self, other: &Self) -> Self {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_mul(*b)),
            (Value::Rational(a), Value::Rational(b)) => Value::Rational(*a * *b),
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => Value::Float(a * b),
                _ => self.clone(),
            },
        }
    }

    /// Arithmetic division for `ControlPatternOps::div_ctrl` (`|/`), zero-division safe (`0.0`).
    pub fn div_numeric(&self, other: &Self) -> Self {
        match (self, other) {
            (Value::Rational(a), Value::Rational(b)) => {
                if b.is_zero() {
                    Value::Float(0.0)
                } else {
                    Value::Rational(*a / *b)
                }
            }
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => {
                    if b.abs() < 1e-15 {
                        Value::Float(0.0)
                    } else {
                        Value::Float(a / b)
                    }
                }
                _ => self.clone(),
            },
        }
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<Time> for Value {
    fn from(v: Time) -> Self {
        Value::Rational(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

/// Deterministic ordered map of control parameter names to `Value`s.
pub type ControlMap = BTreeMap<String, Value>;

/// Execution context passed through `Pattern::query` (`PP-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryContext {
    /// TimeSpan after pattern-local time transformations (`fast`, `slow`, `rev`, `late`).
    pub local_span: TimeSpan,
    /// Unwarped global scheduler cycle TimeSpan for hardware CC / slider / Link synchronization.
    pub global_span: TimeSpan,
    /// Deterministic PRNG seed for reproducible randomness and offline bounces.
    pub session_seed: u64,
    /// Stream slot index (`0..64`) for voice-continuation & state checkpoint lookup.
    pub stream_slot: u16,
    /// Performance FILL latch state (`@fill` / `@!fill` in `CK-01`).
    pub fill_active: bool,
}

impl QueryContext {
    /// Creates a default `QueryContext` where `local_span` and `global_span` start identical.
    #[inline]
    pub fn new(span: TimeSpan) -> Self {
        Self {
            local_span: span,
            global_span: span,
            session_seed: 0,
            stream_slot: 0,
            fill_active: false,
        }
    }
}

/// Query context passed into every `Pattern::query` invocation.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub span: TimeSpan,
    pub controls: ControlMap,
    pub query_context: QueryContext,
}

impl State {
    /// Constructs a `State` for `span` with empty controls and unwarped `global_span = span`.
    #[inline]
    pub fn new(span: TimeSpan) -> Self {
        Self {
            span,
            controls: ControlMap::new(),
            query_context: QueryContext::new(span),
        }
    }

    /// Updates the local query `span` (`local_span`) while preserving `query_context.global_span` (`PP-04`).
    #[inline]
    pub fn with_span(&self, span: TimeSpan) -> Self {
        let mut ctx = self.query_context.clone();
        ctx.local_span = span;
        Self {
            span,
            controls: self.controls.clone(),
            query_context: ctx,
        }
    }

    /// Replaces `controls`.
    #[inline]
    pub fn with_controls(&self, controls: ControlMap) -> Self {
        Self {
            span: self.span,
            controls,
            query_context: self.query_context.clone(),
        }
    }

    /// Replaces `query_context`.
    #[inline]
    pub fn with_query_context(&self, query_context: QueryContext) -> Self {
        Self {
            span: query_context.local_span,
            controls: self.controls.clone(),
            query_context,
        }
    }
}

/// Pitch glide curve shape (`PP-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlideCurve {
    Linear,
    Exponential,
    EqualPowerScurve,
}

/// Legato portamento specification (`PP-03`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegatoGlideSpec {
    pub duration_cycles: Rational64,
    pub curve: GlideCurve,
    pub legato_tie: bool,
}
