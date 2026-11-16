//! `silverwave-dsp` — Real-Time Zero-Allocation Audio Engine, 64-Sample Sub-Block Modulation,
//! Built-In Synths, Serial & Ordered Per-Voice FX, Orbit Routing, Lazy Sample Loader, and Acoustic Telemetry.

#![forbid(unsafe_code)]

pub mod engine;
pub mod events;
pub mod filters;
pub mod fx;
pub mod modulation;
pub mod orbit;
pub mod samples;
pub mod synths;
pub mod telemetry;

pub use engine::*;
pub use events::*;
pub use filters::*;
pub use fx::*;
pub use modulation::*;
pub use orbit::*;
pub use samples::*;
pub use synths::*;
pub use telemetry::*;

/// Returns the crate version of `silverwave-dsp`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
