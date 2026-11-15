//! Compiler diagnostics (`RichCompilerDiagnostic`, `ParseError`), `DelimiterStack` opening-bracket
//! blame (`PP-06`), and Damerau-Levenshtein typo suggestions (`PP-06`).

use crate::span::{ByteSpan, SourceSpan};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// Delimiter kind tracked by `DelimiterStack` (`PP-06`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelimiterKind {
    SquareBracket,
    AngleBracket,
    CurlyBrace,
    Paren,
    DoubleQuote,
    Backtick,
}

impl DelimiterKind {
    /// Opening character for this delimiter.
    pub const fn open_char(self) -> char {
        match self {
            Self::SquareBracket => '[',
            Self::AngleBracket => '<',
            Self::CurlyBrace => '{',
            Self::Paren => '(',
            Self::DoubleQuote => '"',
            Self::Backtick => '`',
        }
    }

    /// Expected closing character for this delimiter.
    pub const fn close_char(self) -> char {
        match self {
            Self::SquareBracket => ']',
            Self::AngleBracket => '>',
            Self::CurlyBrace => '}',
            Self::Paren => ')',
            Self::DoubleQuote => '"',
            Self::Backtick => '`',
        }
    }

    /// Maps an opening delimiter character to `Some(DelimiterKind)`.
    pub const fn from_open(ch: char) -> Option<Self> {
        match ch {
            '[' => Some(Self::SquareBracket),
            '<' => Some(Self::AngleBracket),
            '{' => Some(Self::CurlyBrace),
            '(' => Some(Self::Paren),
            _ => None,
        }
    }

    /// Maps a closing delimiter character to `Some(DelimiterKind)`.
    pub const fn from_close(ch: char) -> Option<Self> {
        match ch {
            ']' => Some(Self::SquareBracket),
            '>' => Some(Self::AngleBracket),
            '}' => Some(Self::CurlyBrace),
            ')' => Some(Self::Paren),
            _ => None,
        }
    }
}

/// Entry on the `DelimiterStack` recording the opening token and its exact `SourceSpan`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelimiterEntry {
    pub kind: DelimiterKind,
    pub open_span: SourceSpan,
}

/// Explicit delimiter stack that blames the unclosed opening delimiter on EOF or mismatch (`PP-06`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DelimiterStack {
    stack: Vec<DelimiterEntry>,
}

impl DelimiterStack {
    /// Creates an empty `DelimiterStack`.
    #[inline]
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Current nesting depth.
    #[inline]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Pushes an opening delimiter onto the stack.
    #[inline]
    pub fn push(&mut self, kind: DelimiterKind, open_span: SourceSpan) {
        self.stack.push(DelimiterEntry { kind, open_span });
    }

    /// Pops and verifies a closing delimiter against the top of the stack.
    /// If mismatched, blames the opening delimiter as `primary_span` and the mismatched close as `secondary_span`.
    pub fn pop_matching(
        &mut self,
        close_kind: DelimiterKind,
        close_span: SourceSpan,
    ) -> Result<DelimiterEntry, RichCompilerDiagnostic> {
        match self.stack.pop() {
            Some(entry) if entry.kind == close_kind => Ok(entry),
            Some(entry) => Err(RichCompilerDiagnostic {
                error_code: "SW-E001-MISMATCHED-DELIMITER",
                message: alloc::format!(
                    "unclosed '{}' opened at byte {}: expected '{}', found '{}'",
                    entry.kind.open_char(),
                    entry.open_span.start,
                    entry.kind.close_char(),
                    close_kind.close_char()
                ),
                primary_span: entry.open_span,
                secondary_span: Some((
                    close_span,
                    alloc::format!("mismatched closing '{}'", close_kind.close_char()),
                )),
                did_you_mean: Some(entry.kind.close_char().to_string()),
                isolated_stream_id: None,
            }),
            None => Err(RichCompilerDiagnostic {
                error_code: "SW-E002-UNEXPECTED-CLOSING-DELIMITER",
                message: alloc::format!(
                    "unexpected closing '{}' with no matching opening '{}'",
                    close_kind.close_char(),
                    close_kind.open_char()
                ),
                primary_span: close_span,
                secondary_span: None,
                did_you_mean: None,
                isolated_stream_id: None,
            }),
        }
    }

    /// Verifies that all opened delimiters were closed at EOF; if not, blames the most recent
    /// unclosed opening delimiter as `primary_span` (`PP-06`).
    pub fn check_empty_at_eof(&self, eof_span: SourceSpan) -> Result<(), RichCompilerDiagnostic> {
        if let Some(entry) = self.stack.last() {
            Err(RichCompilerDiagnostic {
                error_code: "SW-E003-UNCLOSED-OPENING-DELIMITER",
                message: alloc::format!(
                    "unclosed '{}' opened at byte {}: expected closing '{}' before end of input",
                    entry.kind.open_char(),
                    entry.open_span.start,
                    entry.kind.close_char()
                ),
                primary_span: entry.open_span,
                secondary_span: Some((
                    eof_span,
                    alloc::format!("expected '{}' here", entry.kind.close_char()),
                )),
                did_you_mean: Some(entry.kind.close_char().to_string()),
                isolated_stream_id: None,
            })
        } else {
            Ok(())
        }
    }
}

/// Structured compiler diagnostic with primary opening-delimiter blame and typo suggestions (`PP-06`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichCompilerDiagnostic {
    pub error_code: &'static str,
    pub message: String,
    /// Primary span (e.g. the exact opening `[` for an unclosed sub-cycle group).
    pub primary_span: SourceSpan,
    /// Optional secondary span (e.g. where a mismatched `}` or EOF was encountered).
    pub secondary_span: Option<(SourceSpan, String)>,
    /// Damerau-Levenshtein symbol suggestion (e.g. `Some(".room")`).
    pub did_you_mean: Option<String>,
    /// Affected stream ID if isolated to a single stream block (`Some("d2")`).
    pub isolated_stream_id: Option<String>,
}

/// Canonical parse/compile error alias for `silverwave-mini`.
pub type ParseError = RichCompilerDiagnostic;

impl RichCompilerDiagnostic {
    /// Creates a new diagnostic at `span` inside `source`.
    pub fn new(error_code: &'static str, message: impl Into<String>, source: &str, span: ByteSpan) -> Self {
        Self {
            error_code,
            message: message.into(),
            primary_span: SourceSpan::from_byte_range(source, span),
            secondary_span: None,
            did_you_mean: None,
            isolated_stream_id: None,
        }
    }
}

impl fmt::Display for RichCompilerDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (at {}..{}, line {}, col {})",
            self.error_code,
            self.message,
            self.primary_span.start,
            self.primary_span.end,
            self.primary_span.line,
            self.primary_span.col
        )
    }
}

/// Canonical SilverWave method and control parameter symbol table for typo suggestions.
pub const KNOWN_DSL_SYMBOLS: &[&str] = &[
    "s", "sound", "n", "note", "freq", "midinote", "gain", "pan", "speed", "begin", "end",
    "lpf", "cutoff", "hpf", "hcutoff", "bpf", "bandf", "lpq", "resonance", "room", "size",
    "delay", "delaytime", "delayfeedback", "crush", "coarse", "shape", "distort", "vowel",
    "cut", "orbit", "fast", "slow", "rev", "palindrome", "early", "late", "iter", "iterBack",
    "segment", "compress", "zoom", "inside", "outside", "within", "linger", "swing", "swingBy",
    "ribbon", "every", "whenmod", "sometimes", "often", "rarely", "almostAlways", "almostNever",
    "chunk", "ply", "off", "superimpose", "layer", "stut", "echo", "jux", "juxBy", "degrade",
    "degradeBy", "undegrade", "euclid", "euclidRot", "euclidInv", "euclidFull", "step", "expand",
    "pace", "striate", "chop", "slice", "splice", "randslice", "scramble", "shuffle", "scale",
    "chord", "voicing", "dict", "anchor", "rootNotes", "accum", "markov", "lsystem", "glide",
    "plock", "table", "groove", "arrange", "once", "asap",
];

/// Computes optimal string alignment Damerau-Levenshtein distance between `a` and `b`
/// (treating adjacent transpositions like `"lfp"` <-> `"lpf"` as distance `1`).
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let width = n + 1;
    let mut dp = alloc::vec![0usize; (m + 1) * width];
    for i in 0..=m {
        dp[i * width] = i;
    }
    for j in 0..=n {
        dp[j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            let del = dp[(i - 1) * width + j] + 1;
            let ins = dp[i * width + (j - 1)] + 1;
            let sub = dp[(i - 1) * width + (j - 1)] + cost;
            let mut best = del.min(ins).min(sub);
            if i > 1
                && j > 1
                && a_chars[i - 1] == b_chars[j - 2]
                && a_chars[i - 2] == b_chars[j - 1]
            {
                best = best.min(dp[(i - 2) * width + (j - 2)] + 1);
            }
            dp[i * width + j] = best;
        }
    }
    dp[m * width + n]
}

/// Suggests the closest known DSL method/control symbol within Damerau-Levenshtein distance `<= 2` (`PP-06`).
pub fn suggest_symbol(unknown: &str) -> Option<String> {
    let has_dot = unknown.starts_with('.');
    let clean = unknown.trim_start_matches('.');
    if clean.is_empty() {
        return None;
    }
    let mut best: Option<(&str, usize)> = None;
    for &cand in KNOWN_DSL_SYMBOLS {
        let dist = damerau_levenshtein(&clean.to_ascii_lowercase(), &cand.to_ascii_lowercase());
        if dist <= 2 {
            match best {
                None => best = Some((cand, dist)),
                Some((_, b_dist)) if dist < b_dist => best = Some((cand, dist)),
                _ => {}
            }
        }
    }
    best.map(|(s, _)| {
        if has_dot {
            alloc::format!(".{}", s)
        } else {
            s.to_string()
        }
    })
}
