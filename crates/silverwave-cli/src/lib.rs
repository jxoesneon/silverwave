//! `silverwave-cli` — Unified 11-Subcommand CLI (`repl`, `daemon`, `lsp`, `eval`, `render`,
//! `osc`, `sampler`, `plugin-bundler`, `doctor`, `completions`, `mcp`), 4-Stage
//! Crash/Panic-Safe Live Performance Recovery Supervisor, Multi-Platform Packaging,
//! `v0.9.0` RC Soak/Fuzz/Supply-Chain Verifier, and `v1.0.0` GA Parity Matrix Sign-Off.

#![forbid(unsafe_code)]

pub mod cli;
pub mod ga_parity;
pub mod packaging;
pub mod rc;
pub mod supervisor;

pub use cli::*;
pub use ga_parity::*;
pub use packaging::*;
pub use rc::*;
pub use supervisor::*;

/// Returns the crate version of `silverwave-cli`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
