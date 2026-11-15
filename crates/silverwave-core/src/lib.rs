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
    lift_a2_both, lift_a2_left, lift_a2_right, sweep_line_join, BoxedFn, PatternApplicative,
    PatternFunctor, PatternMonad, PatternTrigJoin,
};
pub use combinators::{
    cat, ctrl_param, fastcat, from_list, gain, lpf, n, note, pan, pure, run, s, scan, sequence,
    silence, slowcat, slowcat_prime, stack, timecat, ControlPatternOps, StereoJux,
};
pub use extensions::{
    arrange, asap, once, qtrigger, seq_p, transitions, TrigCondition, TrigConditionLedger,
};
pub use pattern::{global_controller, ControlPattern, IntoPattern, Pattern, QueryFn};
pub use rational::{Rational, Rational64, Time, TimeExt};
pub use signals::{
    berlin, choose, choose_cycles, cos_f64, cosine, irand, isaw, ln_f64, perlin, rand, saw,
    sin_f64, sine, splitmix64, square, time_to_rand, time_to_rand_seeded, tri, wchoose,
    SignalRange,
};
pub use span::{Arc, Context, Event, Hap, SourceLocation, TimeSpan};
pub use stateful::{
    lsystem_expand, markov_pattern, tie_contiguous, CycleStateCheckpointer,
    LruCycleStateCheckpointer, StatefulPattern, MAX_STATE_CHECKPOINTS_PER_STREAM,
};
pub use stepwise::{bjorklund, polymeter, polyrhythm, stepcat, SamplePatternOps};
pub use tonal::{
    arp_with, parse_chord_symbol, parse_scale_descriptor, resolve_scale_degree, scale_intervals,
    ParsedChord, ScalaTuning, TonalExt,
};
pub use value::{
    exp2_f64, ControlMap, EngineCompatibilityProfile, GlideCurve, LegatoGlideSpec, NoteValue,
    QueryContext, State, Value,
};
