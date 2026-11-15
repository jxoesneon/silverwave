//! UTF-8 byte spans (`ByteSpan`), line/column source spans (`SourceSpan`),
//! two-level `with_loc` offset composition (`SourceOrigin`), and compile-time DoS quotas.

use alloc::vec::Vec;
use core::ops::Range;
use silverwave_core::SourceLocation;

/// Maximum allowed nesting depth for `[]`, `<>`, `{}`, and `()` groups (`Section 5.2`).
pub const MAX_AST_NESTING_DEPTH: usize = 64;

/// Maximum allowed steps in a Euclidean `(k, n, o)` tuple (`Section 5.2`).
pub const MAX_EUCLID_STEPS: usize = 1024;

/// Maximum allowed `!n` replication or `@w` elongation count (`Section 5.2`).
pub const MAX_REPLICATE_OR_ELONGATE: usize = 1024;

/// Maximum allowed expanded tokens in an L-system or range expansion (`Section 5.2`).
pub const MAX_LSYSTEM_EXPANDED_TOKENS: usize = 4096;

/// UTF-8 0-indexed byte range `[start, end)` within a source string.
pub type ByteSpan = Range<usize>;

/// Wrapper pairing an AST node `T` with its exact `ByteSpan`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: ByteSpan,
}

impl<T> Spanned<T> {
    /// Creates a new `Spanned<T>`.
    #[inline]
    pub fn new(node: T, span: ByteSpan) -> Self {
        Self { node, span }
    }
}

/// Rich 0-indexed UTF-8 byte and 1-indexed line/column source span (`PP-06`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

impl SourceSpan {
    /// Computes line and column numbers (1-indexed) for `byte_range` inside `source`.
    /// Clamps indices to valid UTF-8 character boundaries of `source`.
    pub fn from_byte_range(source: &str, byte_range: ByteSpan) -> Self {
        let len = source.len();
        let mut start = byte_range.start.min(len);
        while start > 0 && !source.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = byte_range.end.min(len).max(start);
        while end < len && !source.is_char_boundary(end) {
            end += 1;
        }

        let prefix = &source[..start];
        let mut line = 1u32;
        let mut col = 1u32;
        for ch in prefix.chars() {
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        Self {
            start,
            end,
            line,
            col,
        }
    }

    /// Converts `self` into a `silverwave_core::SourceLocation`.
    #[inline]
    pub const fn to_core_location(self) -> SourceLocation {
        SourceLocation {
            start: self.start,
            end: self.end,
        }
    }

    /// Converts `self` into a `ByteSpan`.
    #[inline]
    pub const fn to_byte_span(self) -> ByteSpan {
        self.start..self.end
    }
}

/// Origin metadata for two-level `with_loc` span composition between outer DSL buffers
/// and inner mini-notation string literals (`Section 2.1`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceOrigin {
    /// Byte offset of the string literal's content start in the outer editor buffer
    /// (already accounting for opening quote `"` or backtick `` ` `` width).
    pub outer_content_offset: usize,
    /// Optional per-byte escape-shift map `(unescaped_offset, extra_escaped_bytes)` for `\"` or `\n`.
    pub escape_shifts: Vec<(usize, usize)>,
}

impl SourceOrigin {
    /// Origin at byte 0 with no outer offset.
    #[inline]
    pub fn root() -> Self {
        Self::default()
    }

    /// Origin for a quoted literal whose opening quote is at `outer_quote_byte` with delimiter width `quote_width` (`1` for `"` or `` ` ``).
    #[inline]
    pub fn with_outer_offset(outer_quote_byte: usize, quote_width: usize) -> Self {
        Self {
            outer_content_offset: outer_quote_byte + quote_width,
            escape_shifts: Vec::new(),
        }
    }

    /// Unescapes a raw literal slice (handling `\n`, `\t`, `\"`, `\\`) while recording exact byte shifts
    /// so inner `ByteSpan`s map back to the exact outer editor buffer offsets (`test_escaped_quotes_and_multiline_backticks`).
    pub fn unescape_with_origin(
        raw_literal_inner: &str,
        outer_quote_byte: usize,
        quote_width: usize,
    ) -> (alloc::string::String, Self) {
        let mut out = alloc::string::String::with_capacity(raw_literal_inner.len());
        let mut shifts = Vec::new();
        let mut extra = 0usize;
        let mut chars = raw_literal_inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next_c) = chars.peek() {
                    chars.next();
                    let unescaped = match next_c {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '"' => '"',
                        '\'' => '\'',
                        '`' => '`',
                        '\\' => '\\',
                        other => other,
                    };
                    out.push(unescaped);
                    extra += 1;
                    shifts.push((out.len(), extra));
                    continue;
                }
            }
            out.push(c);
        }
        (
            out,
            Self {
                outer_content_offset: outer_quote_byte + quote_width,
                escape_shifts: shifts,
            },
        )
    }

    /// Maps an inner `ByteSpan` into the outer editor buffer's `ByteSpan`.
    pub fn map_span(&self, inner: &ByteSpan) -> ByteSpan {
        let map_pos = |pos: usize| -> usize {
            let mut shift = 0usize;
            for &(unesc_idx, extra) in &self.escape_shifts {
                if pos >= unesc_idx {
                    shift = extra;
                } else {
                    break;
                }
            }
            self.outer_content_offset + pos + shift
        };
        map_pos(inner.start)..map_pos(inner.end)
    }

    /// Maps an inner `ByteSpan` into a `silverwave_core::SourceLocation`.
    #[inline]
    pub fn map_to_location(&self, inner: &ByteSpan) -> SourceLocation {
        let mapped = self.map_span(inner);
        SourceLocation {
            start: mapped.start,
            end: mapped.end,
        }
    }
}
