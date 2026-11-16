//! `silverwave-cli` — Unified 11-Subcommand CLI (`repl`, `daemon`, `lsp`, `eval`, `render`,
//! `osc`, `sampler`, `plugin-bundler`, `doctor`, `completions`, `mcp`) and 4-Stage
//! Crash/Panic-Safe Live Performance Recovery Supervisor (`v0.7.0`).

#![forbid(unsafe_code)]

pub mod cli;
pub mod supervisor;

pub use cli::*;
pub use supervisor::*;

/// Returns the crate version of `silverwave-cli`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
