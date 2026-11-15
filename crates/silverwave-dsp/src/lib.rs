//! `silverwave-dsp` crate skeleton linked to `silverwave-core` `v0.1.0`.
#![forbid(unsafe_code)]

pub use silverwave_core as core;

/// Returns the crate version synchronized with `silverwave-core`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
