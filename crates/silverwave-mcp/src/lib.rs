//! `silverwave-mcp` — 1:1 Human-Agent Surface Parity Model Context Protocol (MCP) Server
//! exposing 42 tools (38 core + 4 Category-King extensions), 10 live-subscribed `silverwave://`
//! resources, 5 co-performance workflow prompts, surgical AST `slider()` patching,
//! filesystem/SSRF sandboxing, token-bucket rate limiting, and zero-allocation audio command queues (`v0.7.0`).

#![forbid(unsafe_code)]

pub mod protocol;
pub mod resources;
pub mod security;
pub mod tools;

pub use protocol::*;
pub use resources::*;
pub use security::*;
pub use tools::*;

/// Returns the crate version of `silverwave-mcp`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
