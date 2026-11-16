//! # `silverwave-core` (`v0.1.0`)
//!
//! Pure-Rust, `#![no_std]`-compatible (`alloc`) core pattern, rational time, signal,
//! tonal/harmony/microtonal, and stateful live-coding engine for **SilverWave**.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod algebra;
pub mod combinators;
pub mod extensions;
pub mod pattern;
pub mod rational;
pub mod signals;
pub mod span;
pub mod stateful;
pub mod stepwise;
pub mod tonal;
pub mod value;

pub use algebra::{
    BoxedFn, PatternApplicative, PatternFunctor, PatternMonad, PatternTrigJoin, lift_a2_both,
    lift_a2_left, lift_a2_right, sweep_line_join,
};
pub use combinators::{
    ControlPatternOps, StereoJux, cat, ctrl_param, fastcat, from_list, gain, lpf, n, note, pan,
    pure, run, s, scan, sequence, silence, slowcat, slowcat_prime, stack, timecat,
};
pub use extensions::{
    TrigCondition, TrigConditionLedger, arrange, asap, once, qtrigger, seq_p, transitions,
};
pub use pattern::{ControlPattern, IntoPattern, Pattern, QueryFn, global_controller};
pub use rational::{Rational, Rational64, Time, TimeExt};
pub use signals::{
    SignalRange, berlin, choose, choose_cycles, cos_f64, cosine, irand, isaw, ln_f64, perlin, rand,
    saw, sin_f64, sine, splitmix64, square, time_to_rand, time_to_rand_seeded, tri, wchoose,
};
pub use span::{Arc, Context, Event, Hap, SourceLocation, TimeSpan};
pub use stateful::{
    CycleStateCheckpointer, LruCycleStateCheckpointer, MAX_STATE_CHECKPOINTS_PER_STREAM,
    StatefulPattern, lsystem_expand, markov_pattern, tie_contiguous,
};
pub use stepwise::{SamplePatternOps, bjorklund, polymeter, polyrhythm, stepcat};
pub use tonal::{
    ParsedChord, ScalaTuning, TonalExt, arp_with, parse_chord_symbol, parse_scale_descriptor,
    resolve_scale_degree, scale_intervals,
};
pub use value::{
    ControlMap, EngineCompatibilityProfile, GlideCurve, LegatoGlideSpec, NoteValue, QueryContext,
    State, Value, exp2_f64,
};
