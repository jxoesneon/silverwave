//! `silverwave-gui` — Shared REPL Engine (`ReplEngine`), Adaptive UTF-8/UTF-16 `SpanMap`,
//! Zero-Re-Eval Inline `slider()` `WidgetRegistry`, `.color()` / `.markcss()` Sanitizer,
//! `@title` `DocMetadata` & `#code=` `ShareCodec`, Tracker Phrase Grid, and Interactive Tour.

#![forbid(unsafe_code)]

pub mod metadata;
pub mod repl;
pub mod span_map;
pub mod tracker;
pub mod widgets;

pub use metadata::*;
pub use repl::*;
pub use span_map::*;
pub use tracker::*;
pub use widgets::*;

/// Returns the crate version of `silverwave-gui`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
