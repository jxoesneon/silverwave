//! `silverwave-io` — Real-Time Stream Manager (`d1..d16`, `$:`), 11 TidalCycles Transitions,
//! Ableton Link Phase-Continuous Clock, SuperDirt `rosc` OSC Engine, `@strudel/midi` + MPE + PLL + Note-Off Safety,
//! Csound Backend, Audio/Plugin Bridges, and Cycle-Quantized CRDT Ensemble Sync.

#![forbid(unsafe_code)]

pub mod backends;
pub mod clock;
pub mod crdt;
pub mod csound;
pub mod midi;
pub mod osc;
pub mod streams;
pub mod transitions;
pub mod types;

pub use backends::*;
pub use clock::*;
pub use crdt::*;
pub use csound::*;
pub use midi::*;
pub use osc::*;
pub use streams::*;
pub use transitions::*;
pub use types::*;

/// Returns the crate version of `silverwave-io`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
