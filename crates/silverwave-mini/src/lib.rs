//! # `silverwave-mini` (`v0.2.0`)
//!
//! Pure-Rust `#![no_std]` + `alloc` TidalCycles & Strudel Krill Mini-Notation parser,
//! Mondo notation parser, Two-Stage `TPat<T>` AST with 60 FPS zero-reparse `with_loc`
//! highlighting, `DelimiterStack` opening-delimiter diagnostics (`PP-06`),
//! Damerau-Levenshtein symbol suggestions, fault-tolerant per-stream evaluation (`PP-07`),
//! Elektron conditional trigs (`CK-01`), inline `>>` DSP pipe parser (`CK-05`),
//! and Dual-Layer Rust DSL with all 7 Strudel alignment modes (`02-SPEC-MINI-NOTATION-AND-DSL.md`).

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod ast;
pub mod compiler;
pub mod diagnostics;
pub mod dsl;
pub mod parser;
pub mod span;
pub mod transpiler;

pub use ast::{DspPipeStage, NoteSpec, TPat};
pub use compiler::{
    compile_tpat, mini, mini_with_loc, CompiledHighlightPattern, MiniTarget,
};
pub use diagnostics::{
    damerau_levenshtein, suggest_symbol, DelimiterEntry, DelimiterKind, DelimiterStack,
    ParseError, RichCompilerDiagnostic, KNOWN_DSL_SYMBOLS,
};
pub use dsl::{
    align_binary_op, n_mini, note_mini, s_mini, seq_p_loop, ur, AlignmentMode, IntoMiniPattern,
    PatternCombinator,
};
pub use parser::{
    parse_dsp_pipe, parse_note_spec, parse_ratio_shorthand, parse_trig_condition_suffix,
    FromMiniAtom, MiniParser,
};
pub use silverwave_core as core;
pub use span::{
    ByteSpan, SourceOrigin, SourceSpan, Spanned, MAX_AST_NESTING_DEPTH, MAX_EUCLID_STEPS,
    MAX_LSYSTEM_EXPANDED_TOKENS, MAX_REPLICATE_OR_ELONGATE,
};
pub use transpiler::{FaultTolerantCompileResult, MiniStreamSession, StreamEntry};

/// Returns the crate version synchronized with `silverwave-core`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
