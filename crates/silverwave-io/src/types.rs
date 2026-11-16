//! Core shared I/O types (`CycleTime`, `CycleArc`, `ControlValue`, `DispatchEvent`,
//! `StreamId`, `IoError`) and the non-blocking `OutputSink` trait (`04-SPEC` Section 1.1 & 1.2).

use silverwave_core::{Rational64, TimeSpan};
use std::collections::BTreeMap;
use std::fmt;

/// Rational cycle time inherited from `silverwave-core` (`Section 1.1`).
pub type CycleTime = Rational64;

/// Cycle-domain span `[start, stop)` (`Section 1.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CycleArc {
    pub start: CycleTime,
    pub stop: CycleTime,
}

impl CycleArc {
    /// Creates a new `CycleArc`.
    #[inline]
    pub const fn new(start: CycleTime, stop: CycleTime) -> Self {
        Self { start, stop }
    }

    /// Converts a `silverwave_core::TimeSpan` into a `CycleArc`.
    #[inline]
    pub const fn from_timespan(span: TimeSpan) -> Self {
        Self {
            start: span.begin,
            stop: span.end,
        }
    }

    /// Converts `self` into a `silverwave_core::TimeSpan`.
    #[inline]
    pub fn to_timespan(self) -> TimeSpan {
        TimeSpan::new_clamped(self.start, self.stop)
    }
}

/// MIDI note value carried inside `ControlValue::Note` (`Section 1.1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiNoteValue {
    pub midi_note: u8,
    pub detune_cents: f32,
}

/// Typed control value carried in `ControlMap` (`Section 1.1`).
#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Float(f64),
    Int(i64),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Note(MidiNoteValue),
}

impl ControlValue {
    /// Extracts a floating-point value if numeric.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            Self::Note(n) => Some(n.midi_note as f64),
            _ => None,
        }
    }

    /// Extracts an integer value if numeric.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            Self::Note(n) => Some(n.midi_note as i64),
            _ => None,
        }
    }

    /// Extracts a string slice if `Self::Str`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Per-event control parameter dictionary (`Section 1.1`).
pub type ControlMap = BTreeMap<String, ControlValue>;

/// Global controller state dictionary (`cF`, `cI`, `cS`, `/ctrl` — `Section 1.1`).
pub type StateMap = BTreeMap<String, ControlValue>;

/// Stream identifier (`"1"`..`"16"` for `d1`..`d16`, `"$:0"`, or named `p("drums")` — `Section 1.1`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(pub String);

impl StreamId {
    /// Creates a numbered stream ID for `d1..d16` (`1..=16`).
    pub fn numbered(index: u8) -> Self {
        Self(index.to_string())
    }
}

/// Triggered pattern event ready for I/O dispatch (`Section 1.1`).
#[derive(Clone, Debug, PartialEq)]
pub struct DispatchEvent {
    pub whole: Option<CycleArc>,
    pub part: CycleArc,
    pub value: ControlMap,
    pub onset_secs: f64,
    pub duration_secs: f64,
    pub cps: f64,
    pub stream_id: StreamId,
}

/// Unified error type for `silverwave-io` (`Section 1.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoError {
    Audio(String),
    Plugin(String),
    Csound(String),
    Osc(String),
    Midi(String),
    Link(String),
    StreamNotFound(String),
    Config(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Audio(s) => write!(f, "audio backend error: {}", s),
            Self::Plugin(s) => write!(f, "plugin host error: {}", s),
            Self::Csound(s) => write!(f, "csound engine error: {}", s),
            Self::Osc(s) => write!(f, "osc transport error: {}", s),
            Self::Midi(s) => write!(f, "midi port error: {}", s),
            Self::Link(s) => write!(f, "ableton link error: {}", s),
            Self::StreamNotFound(s) => write!(f, "invalid stream id: {}", s),
            Self::Config(s) => write!(f, "configuration error: {}", s),
        }
    }
}

impl std::error::Error for IoError {}

/// Unified non-blocking output sink trait (`Section 1.2`).
pub trait OutputSink: Send + Sync {
    fn name(&self) -> &str;
    fn send_batch(&self, events: &[DispatchEvent]) -> Result<(), IoError>;
    fn panic(&self) -> Result<(), IoError>;
    fn close(&self) -> Result<(), IoError>;
}
