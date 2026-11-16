//! Adaptive UTF-8 ByteSpan <-> UTF-16 CodeMirror/LSP SpanMapper (`05-SPEC` §2.1 & §4.2, `SPAN-01`..`SPAN-06`).

#![forbid(unsafe_code)]

/// UTF-8 byte offset (`05-SPEC` §2.1).
pub type ByteOffset = usize;

/// UTF-16 code unit offset (`05-SPEC` §2.1).
pub type Utf16Offset = usize;

/// Half-open UTF-8 byte span `[start, end)` (`05-SPEC` §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl ByteSpan {
    /// Creates a new `ByteSpan`.
    #[inline]
    pub const fn new(start: ByteOffset, end: ByteOffset) -> Self {
        Self { start, end }
    }

    /// Length in bytes.
    #[inline]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns `true` if `start >= end`.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Half-open UTF-16 span `[start, end)` (`05-SPEC` §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf16Span {
    pub start: Utf16Offset,
    pub end: Utf16Offset,
}

impl Utf16Span {
    /// Creates a new `Utf16Span`.
    #[inline]
    pub const fn new(start: Utf16Offset, end: Utf16Offset) -> Self {
        Self { start, end }
    }
}

/// 0-indexed LSP line and character position (`05-SPEC` §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// Half-open LSP range `[start, end)` (`05-SPEC` §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// Live buffer edit applied prior to re-evaluation (`05-SPEC` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEdit {
    pub span: ByteSpan,
    pub new_len_bytes: usize,
    pub new_len_utf16: usize,
}

/// Validity status of a remapped `ByteSpan` after live buffer edits (`05-SPEC` §4.2, `SPAN-04`..`SPAN-06`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanValidity {
    Exact,
    Shifted,
    Clipped,
    Invalidated,
}

/// Trait for bidirectional UTF-8 <-> UTF-16 mapping and adaptive live-edit span remapping (`05-SPEC` §4.2).
pub trait SpanMapper {
    fn byte_to_utf16(&self, byte: ByteOffset) -> Option<Utf16Offset>;
    fn utf16_to_byte(&self, utf16: Utf16Offset) -> Option<ByteOffset>;
    fn byte_span_to_lsp_range(&self, span: ByteSpan) -> Option<LspRange>;
    fn apply_edit(&mut self, edit: TextEdit);
    fn remap_span(&self, original: ByteSpan) -> (Option<ByteSpan>, SpanValidity);
}

/// Concrete `SpanMap` maintaining exact UTF-8/UTF-16 prefix tables and live `TextEdit` history (`SPAN-01`..`SPAN-06`).
#[derive(Debug, Clone)]
pub struct SpanMap {
    source: String,
    edits: Vec<TextEdit>,
}

impl SpanMap {
    /// Builds a new `SpanMap` for `source` (`O(N)`).
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            edits: Vec::new(),
        }
    }

    /// Returns the underlying source slice.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Converts a `ByteSpan` into a `Utf16Span`.
    pub fn byte_span_to_utf16_span(&self, span: ByteSpan) -> Option<Utf16Span> {
        let s = self.byte_to_utf16(span.start)?;
        let e = self.byte_to_utf16(span.end)?;
        Some(Utf16Span::new(s, e))
    }
}

impl SpanMapper for SpanMap {
    fn byte_to_utf16(&self, byte: ByteOffset) -> Option<Utf16Offset> {
        if byte > self.source.len() || !self.source.is_char_boundary(byte) {
            return None;
        }
        Some(self.source[..byte].encode_utf16().count())
    }

    fn utf16_to_byte(&self, utf16: Utf16Offset) -> Option<ByteOffset> {
        let mut count_u16 = 0usize;
        for (byte_idx, ch) in self.source.char_indices() {
            if count_u16 == utf16 {
                return Some(byte_idx);
            }
            count_u16 += ch.len_utf16();
            if count_u16 > utf16 {
                // Mid-surrogate pair offset
                return None;
            }
        }
        if count_u16 == utf16 {
            Some(self.source.len())
        } else {
            None
        }
    }

    fn byte_span_to_lsp_range(&self, span: ByteSpan) -> Option<LspRange> {
        if span.start > span.end
            || span.end > self.source.len()
            || !self.source.is_char_boundary(span.start)
            || !self.source.is_char_boundary(span.end)
        {
            return None;
        }
        let pos_of = |target_byte: usize| -> LspPosition {
            let prefix = &self.source[..target_byte];
            let mut line = 0u32;
            let mut line_start_byte = 0usize;
            for (idx, b) in prefix.bytes().enumerate() {
                if b == b'\n' {
                    line += 1;
                    line_start_byte = idx + 1;
                }
            }
            let col_utf16 = prefix[line_start_byte..].encode_utf16().count() as u32;
            LspPosition {
                line,
                character: col_utf16,
            }
        };
        Some(LspRange {
            start: pos_of(span.start),
            end: pos_of(span.end),
        })
    }

    fn apply_edit(&mut self, edit: TextEdit) {
        self.edits.push(edit);
    }

    fn remap_span(&self, original: ByteSpan) -> (Option<ByteSpan>, SpanValidity) {
        let mut cur = original;
        let mut status = SpanValidity::Exact;

        for ed in &self.edits {
            let old_len = ed.span.len() as isize;
            let delta = (ed.new_len_bytes as isize) - old_len;

            if ed.span.end <= cur.start {
                // Edit strictly precedes `cur`: shift by `delta` (`SPAN-04`)
                cur.start = ((cur.start as isize) + delta).max(0) as usize;
                cur.end = ((cur.end as isize) + delta).max(0) as usize;
                if status == SpanValidity::Exact && delta != 0 {
                    status = SpanValidity::Shifted;
                }
            } else if ed.span.start >= cur.end {
                // Edit strictly follows `cur`: unchanged
            } else if ed.span.start <= cur.start && ed.span.end >= cur.end {
                // Edit completely covers/deletes `cur` (`SPAN-06`)
                return (None, SpanValidity::Invalidated);
            } else if ed.span.start <= cur.start && ed.span.end < cur.end {
                // Overlaps the start of `cur`: clip start to end of replacement (`SPAN-05`)
                let new_edit_end = ed.span.start + ed.new_len_bytes;
                let new_cur_end = ((cur.end as isize) + delta).max(new_edit_end as isize) as usize;
                cur.start = new_edit_end;
                cur.end = new_cur_end;
                if cur.start >= cur.end {
                    return (None, SpanValidity::Invalidated);
                }
                status = SpanValidity::Clipped;
            } else if ed.span.start > cur.start && ed.span.end >= cur.end {
                // Overlaps the end of `cur`: clip end to start of edit (`SPAN-05`)
                cur.end = ed.span.start;
                if cur.start >= cur.end {
                    return (None, SpanValidity::Invalidated);
                }
                status = SpanValidity::Clipped;
            } else {
                // Edit strictly inside `cur`: expand/contract `cur.end`
                cur.end = ((cur.end as isize) + delta).max((cur.start + 1) as isize) as usize;
                status = SpanValidity::Clipped;
            }
        }
        (Some(cur), status)
    }
}
