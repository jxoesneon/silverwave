//! `silverwave-plugin` — Native 16-Stereo-Bus (32-Channel) CLAP (`org.silverwave.live`) and
//! VST3 (`SilverWave.vst3`) DAW Plugin via `nih-plug` architecture, Sample-Accurate Host Transport
//! Synchronizer, 64 DAW-Automatable Macro Parameters with CLAP Polyphonic Modulation,
//! Tonal.js JSON Theory Bridge, and Embedded GUI/LSP/MCP Co-Performance Bridge (`v0.6.0`).

#![forbid(unsafe_code)]

pub mod layout;
pub mod params;
pub mod processor;
pub mod transport;

pub use layout::*;
pub use params::*;
pub use processor::*;
pub use transport::*;

/// Returns the crate version of `silverwave-plugin`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
