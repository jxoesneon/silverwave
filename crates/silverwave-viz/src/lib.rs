//! `silverwave-viz` — 1:1 Global & Inline Visualizers (`pianoroll`, `punchcard`, `spiral`,
//! `scope`, `pitchwheel`, `spectrum`), Hydra & WGSL Shader Bridge (`initHydra`),
//! Syphon/Spout/NDI VJ Output, Unicode Braille TUI Renderer, and Deterministic WAV/FLAC Exporter.

#![forbid(unsafe_code)]

pub mod bridges;
pub mod renderers;
pub mod types;

pub use bridges::*;
pub use renderers::*;
pub use types::*;

/// Returns the crate version of `silverwave-viz`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
