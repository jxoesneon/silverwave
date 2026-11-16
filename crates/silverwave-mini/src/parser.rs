//! Zero-panic recursive-descent Mini-Notation + Mondo + Inline DSP Pipe parser
//! with `DelimiterStack` opening-delimiter blame (`PP-06`) and DoS recursion guards.

use crate::ast::{DspPipeStage, NoteSpec, TPat};
use crate::diagnostics::{DelimiterKind, DelimiterStack, ParseError, RichCompilerDiagnostic};
use crate::span::{
    ByteSpan, MAX_AST_NESTING_DEPTH, MAX_EUCLID_STEPS, MAX_LSYSTEM_EXPANDED_TOKENS,
    MAX_REPLICATE_OR_ELONGATE, SourceSpan,
};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::str::FromStr;
use silverwave_core::{NoteValue, Rational, TrigCondition, Value, parse_chord_symbol};

/// Trait for converting raw mini-notation atom strings (`"bd"`, `"12"`, `"c4"`, `"0.5"`, `"q"`)
/// into the typed target `T` of a `TPat<T>`.
pub trait FromMiniAtom: Clone + Sized + 'static {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError>;

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let _ = (start, step, end);
        vec![start.clone(), end.clone()]
    }
}

impl FromMiniAtom for String {
    fn from_mini_atom(token: &str, _span: ByteSpan, _src: &str) -> Result<Self, ParseError> {
        Ok(token.to_string())
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        if let (Ok(a), Ok(b)) = (start.parse::<i64>(), end.parse::<i64>()) {
            let st = step.and_then(|s| s.parse::<i64>().ok());
            return i64::step_range(&a, st.as_ref(), &b)
                .into_iter()
                .map(|x| x.to_string())
                .collect();
        }
        vec![start.clone(), end.clone()]
    }
}

impl FromMiniAtom for i64 {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError> {
        if let Ok(v) = token.parse::<i64>() {
            return Ok(v);
        }
        if let Some((pc, oct)) = NoteValue::parse_pitch_name(token) {
            return Ok((12 * (oct + 1) + pc) as i64);
        }
        Err(RichCompilerDiagnostic::new(
            "SW-E010-INVALID-INT",
            alloc::format!("expected integer or pitch literal, found '{}'", token),
            src,
            span,
        ))
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let a = *start;
        let b = *end;
        let raw_step = step
            .copied()
            .map(|s| s.abs())
            .unwrap_or_else(|| if b >= a { 1 } else { -1 });
        let step_val = if a <= b {
            raw_step.abs().max(1)
        } else {
            -(raw_step.abs().max(1))
        };
        let mut out = Vec::new();
        let mut cur = a;
        while (step_val > 0 && cur <= b) || (step_val < 0 && cur >= b) {
            out.push(cur);
            if out.len() >= MAX_LSYSTEM_EXPANDED_TOKENS {
                break;
            }
            cur += step_val;
        }
        out
    }
}

impl FromMiniAtom for isize {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError> {
        i64::from_mini_atom(token, span, src).map(|v| v as isize)
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let a = *start as i64;
        let b = *end as i64;
        let s = step.map(|&v| v as i64);
        i64::step_range(&a, s.as_ref(), &b)
            .into_iter()
            .map(|v| v as isize)
            .collect()
    }
}

impl FromMiniAtom for usize {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError> {
        let v = i64::from_mini_atom(token, span.clone(), src)?;
        if v < 0 {
            return Err(RichCompilerDiagnostic::new(
                "SW-E011-NEGATIVE-USIZE",
                alloc::format!("expected non-negative integer, found '{}'", token),
                src,
                span,
            ));
        }
        Ok(v as usize)
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let a = *start as i64;
        let b = *end as i64;
        let s = step.map(|&v| v as i64);
        i64::step_range(&a, s.as_ref(), &b)
            .into_iter()
            .filter(|&v| v >= 0)
            .map(|v| v as usize)
            .collect()
    }
}

impl FromMiniAtom for f64 {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError> {
        if let Some(r) = parse_ratio_shorthand(token) {
            return Ok(r.to_f64());
        }
        if let Ok(v) = token.parse::<f64>()
            && v.is_finite()
        {
            return Ok(v);
        }
        if let Ok(r) = Rational::from_str(token) {
            return Ok(r.to_f64());
        }
        if let Some((pc, oct)) = NoteValue::parse_pitch_name(token) {
            return Ok((12 * (oct + 1) + pc) as f64);
        }
        Err(RichCompilerDiagnostic::new(
            "SW-E012-INVALID-FLOAT",
            alloc::format!("expected number, ratio, or note literal, found '{}'", token),
            src,
            span,
        ))
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let a = *start as i64;
        let b = *end as i64;
        let s = step.map(|&v| v as i64);
        i64::step_range(&a, s.as_ref(), &b)
            .into_iter()
            .map(|v| v as f64)
            .collect()
    }
}

impl FromMiniAtom for Rational {
    fn from_mini_atom(token: &str, span: ByteSpan, src: &str) -> Result<Self, ParseError> {
        if let Some(r) = parse_ratio_shorthand(token) {
            return Ok(r);
        }
        if let Ok(r) = Rational::from_str(token) {
            return Ok(r);
        }
        Err(RichCompilerDiagnostic::new(
            "SW-E013-INVALID-RATIONAL",
            alloc::format!(
                "expected rational or shorthand (w/h/q/e/s/t/f/x), found '{}'",
                token
            ),
            src,
            span,
        ))
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        let a = start.floor_i64();
        let b = end.floor_i64();
        let s = step.map(|v| v.floor_i64());
        i64::step_range(&a, s.as_ref(), &b)
            .into_iter()
            .map(Rational::from_integer)
            .collect()
    }
}

impl FromMiniAtom for Value {
    fn from_mini_atom(token: &str, _span: ByteSpan, _src: &str) -> Result<Self, ParseError> {
        if let Some(r) = parse_ratio_shorthand(token) {
            return Ok(Value::Rational(r));
        }
        if let Ok(i) = token.parse::<i64>() {
            return Ok(Value::Int(i));
        }
        if let Ok(f) = token.parse::<f64>()
            && f.is_finite()
        {
            return Ok(Value::Float(f));
        }
        if let Some(ns) = parse_note_spec(token) {
            return Ok(Value::Note(NoteValue::Midi(ns.to_midi())));
        }
        Ok(Value::String(token.to_string()))
    }

    fn step_range(start: &Self, step: Option<&Self>, end: &Self) -> Vec<Self> {
        if let (Some(a), Some(b)) = (start.as_i64(), end.as_i64()) {
            let s = step.and_then(Value::as_i64);
            return i64::step_range(&a, s.as_ref(), &b)
                .into_iter()
                .map(Value::Int)
                .collect();
        }
        vec![start.clone(), end.clone()]
    }
}

/// Parses cycle fraction shorthands: `w=1`, `h=1/2`, `q=1/4`, `e=1/8`, `s=1/16`, `t=1/3`, `f=1/5`, `x=1/6`.
pub fn parse_ratio_shorthand(token: &str) -> Option<Rational> {
    match token {
        "w" => Some(Rational::one()),
        "h" => Some(Rational::new_const(1, 2)),
        "q" => Some(Rational::new_const(1, 4)),
        "e" => Some(Rational::new_const(1, 8)),
        "s" => Some(Rational::new_const(1, 16)),
        "t" => Some(Rational::new_const(1, 3)),
        "f" => Some(Rational::new_const(1, 5)),
        "x" => Some(Rational::new_const(1, 6)),
        _ => None,
    }
}

/// Parses a scientific pitch note token (`c4`, `eb3`, `f#5`, `cs4`, `bf2`, `cn4`).
pub fn parse_note_spec(token: &str) -> Option<NoteSpec> {
    let bytes = token.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0].to_ascii_lowercase();
    let base = match first {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };
    // Avoid misclassifying single-letter ratio shorthands `f`, `e`, `s`, etc. unless followed by accidental or octave digit!
    if bytes.len() == 1 {
        return None;
    }
    let mut idx = 1usize;
    let mut acc = 0i32;
    while idx < bytes.len() {
        match bytes[idx] {
            b'#' | b's' => {
                acc += 1;
                idx += 1;
            }
            b'b' | b'f' => {
                acc -= 1;
                idx += 1;
            }
            b'n' => {
                idx += 1;
            }
            _ => break,
        }
    }
    let rem = &token[idx..];
    if rem.is_empty() {
        if idx > 1 {
            return Some(NoteSpec {
                pitch_class: base + acc,
                octave: 3,
                explicit_octave: false,
            });
        }
        return None;
    }
    let oct = rem.parse::<i32>().ok()?;
    Some(NoteSpec {
        pitch_class: base + acc,
        octave: oct,
        explicit_octave: true,
    })
}

/// Parses an Elektron conditional trig suffix (`@1:4`, `@1st`, `@!1st`, `@fill`, `@!fill`, `@pre`, `@!pre`, `@nei`, `@!nei`).
pub fn parse_trig_condition_suffix(suffix: &str) -> Option<TrigCondition> {
    match suffix {
        "1st" => Some(TrigCondition::FirstCycle { expected: true }),
        "!1st" => Some(TrigCondition::FirstCycle { expected: false }),
        "fill" => Some(TrigCondition::FillLatch { active: true }),
        "!fill" => Some(TrigCondition::FillLatch { active: false }),
        "pre" => Some(TrigCondition::PreviousConditionalOnSameStream { expected: true }),
        "!pre" => Some(TrigCondition::PreviousConditionalOnSameStream { expected: false }),
        "nei" => Some(TrigCondition::PreviousConditionalOnNeighborStream { expected: true }),
        "!nei" => Some(TrigCondition::PreviousConditionalOnNeighborStream { expected: false }),
        _ => {
            if let Some((a_str, b_str)) = suffix.split_once(':') {
                let a = a_str.parse::<u16>().ok()?;
                let b = b_str.parse::<u16>().ok()?;
                if a >= 1 && b >= 1 && a <= b {
                    return Some(TrigCondition::Ratio { a, b });
                }
            }
            None
        }
    }
}

/// Parses an inline modular DSP pipe expression (`"saw(freq) >> lpf(1200, 0.8) >> tanh(2.0)"`, `CK-05`).
pub fn parse_dsp_pipe(src: &str) -> Result<Vec<DspPipeStage>, ParseError> {
    let mut stages = Vec::new();
    let mut cursor = 0usize;
    let body = src.strip_prefix("pipe:").unwrap_or(src);
    let base_offset = src.len() - body.len();

    for raw_part in body.split(">>") {
        let part_offset = base_offset + cursor;
        cursor += raw_part.len() + 2;
        let trimmed = raw_part.trim();
        if trimmed.is_empty() {
            return Err(RichCompilerDiagnostic::new(
                "SW-E030-EMPTY-DSP-STAGE",
                "empty stage in '>>' modular DSP pipe",
                src,
                part_offset..(part_offset + raw_part.len()),
            ));
        }
        let lead_ws = raw_part.len() - raw_part.trim_start().len();
        let stage_start = part_offset + lead_ws;
        let stage_end = stage_start + trimmed.len();

        if let Some(open_idx) = trimmed.find('(') {
            if !trimmed.ends_with(')') {
                return Err(RichCompilerDiagnostic::new(
                    "SW-E003-UNCLOSED-OPENING-DELIMITER",
                    "unclosed '(' in DSP pipe stage",
                    src,
                    (stage_start + open_idx)..(stage_start + open_idx + 1),
                ));
            }
            let op = trimmed[..open_idx].trim().to_string();
            let args_inner = &trimmed[(open_idx + 1)..(trimmed.len() - 1)];
            let args = if args_inner.trim().is_empty() {
                Vec::new()
            } else {
                args_inner
                    .split(',')
                    .map(|a| a.trim().to_string())
                    .collect()
            };
            stages.push(DspPipeStage {
                op,
                args,
                span: stage_start..stage_end,
            });
        } else {
            stages.push(DspPipeStage {
                op: trimmed.to_string(),
                args: Vec::new(),
                span: stage_start..stage_end,
            });
        }
    }
    Ok(stages)
}

/// Zero-panic recursive-descent Mini-Notation + Mondo parser (`silverwave-mini`).
pub struct MiniParser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    delimiters: DelimiterStack,
    salt_counter: u64,
}

impl<'a> MiniParser<'a> {
    /// Creates a new `MiniParser` over UTF-8 `src`.
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            delimiters: DelimiterStack::new(),
            salt_counter: 1,
        }
    }

    /// Parses the full mini-notation string into a `TPat<T>`.
    pub fn parse<T: FromMiniAtom>(src: &'a str) -> Result<TPat<T>, ParseError> {
        let mut p = Self::new(src);
        p.skip_ws_and_comments()?;
        if p.is_eof() {
            return Ok(TPat::Silence { span: 0..src.len() });
        }
        let ast = p.parse_stack(None)?;
        p.skip_ws_and_comments()?;
        if !p.is_eof() {
            let ch = p.peek_char().unwrap_or(' ');
            let c_span = SourceSpan::from_byte_range(p.src, p.pos..(p.pos + ch.len_utf8()));
            if let Some(close_kind) = DelimiterKind::from_close(ch) {
                return Err(p.delimiters.pop_matching(close_kind, c_span).unwrap_err());
            }
            return Err(RichCompilerDiagnostic::new(
                "SW-E004-UNEXPECTED-TOKEN",
                alloc::format!("unexpected token '{}' at byte {}", ch, p.pos),
                p.src,
                p.pos..(p.pos + ch.len_utf8()),
            ));
        }
        let eof_span = SourceSpan::from_byte_range(p.src, p.src.len()..p.src.len());
        p.delimiters.check_empty_at_eof(eof_span)?;
        Ok(ast)
    }

    #[inline]
    fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    #[inline]
    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), ParseError> {
        loop {
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_whitespace() {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            if self.src[self.pos..].starts_with("//") {
                self.pos += 2;
                while let Some(ch) = self.peek_char() {
                    self.pos += ch.len_utf8();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            if self.src[self.pos..].starts_with("/*") {
                let open_start = self.pos;
                self.pos += 2;
                if let Some(end_offset) = self.src[self.pos..].find("*/") {
                    self.pos += end_offset + 2;
                    continue;
                } else {
                    return Err(RichCompilerDiagnostic::new(
                        "SW-E005-UNCLOSED-COMMENT",
                        "unclosed block comment '/*'",
                        self.src,
                        open_start..(open_start + 2),
                    ));
                }
            }
            break;
        }
        Ok(())
    }

    fn check_depth(&self, span: ByteSpan) -> Result<(), ParseError> {
        if self.delimiters.depth() > MAX_AST_NESTING_DEPTH {
            return Err(RichCompilerDiagnostic::new(
                "SW-E020-MAX-DEPTH-EXCEEDED",
                alloc::format!(
                    "nesting depth exceeds MAX_AST_NESTING_DEPTH ({})",
                    MAX_AST_NESTING_DEPTH
                ),
                self.src,
                span,
            ));
        }
        Ok(())
    }

    fn parse_stack<T: FromMiniAtom>(
        &mut self,
        stop_close: Option<char>,
    ) -> Result<TPat<T>, ParseError> {
        self.skip_ws_and_comments()?;
        let start_pos = self.pos;
        let first = self.parse_dot_sequence(stop_close)?;
        let mut layers = vec![first];

        loop {
            self.skip_ws_and_comments()?;
            if self.peek_char() == Some(',') {
                self.pos += 1;
                self.skip_ws_and_comments()?;
                let next_layer = self.parse_dot_sequence(stop_close)?;
                layers.push(next_layer);
            } else {
                break;
            }
        }

        let end_pos = layers.last().map(|l| l.span().end).unwrap_or(self.pos);
        if layers.len() == 1 {
            Ok(layers.into_iter().next().unwrap())
        } else {
            Ok(TPat::Stack {
                layers,
                span: start_pos..end_pos,
            })
        }
    }

    fn parse_dot_sequence<T: FromMiniAtom>(
        &mut self,
        stop_close: Option<char>,
    ) -> Result<TPat<T>, ParseError> {
        self.skip_ws_and_comments()?;
        let start_pos = self.pos;
        let mut feet: Vec<Vec<TPat<T>>> = vec![Vec::new()];

        loop {
            self.skip_ws_and_comments()?;
            let Some(ch) = self.peek_char() else {
                break;
            };
            if ch == ',' || Some(ch) == stop_close {
                break;
            }
            if DelimiterKind::from_close(ch).is_some() {
                break;
            }
            // Standalone `.` foot separator (not part of `..` range)
            if ch == '.' && !self.src[self.pos..].starts_with("..") {
                self.pos += 1;
                feet.push(Vec::new());
                continue;
            }
            let item = self.parse_choice_item()?;
            feet.last_mut().unwrap().push(item);
        }

        if feet.len() == 1 {
            let items = feet.into_iter().next().unwrap();
            return Ok(Self::collapse_seq(items, start_pos..self.pos));
        }

        let foot_nodes: Vec<TPat<T>> = feet
            .into_iter()
            .filter(|f| !f.is_empty())
            .map(|items| {
                let s = items.first().unwrap().span().start;
                let e = items.last().unwrap().span().end;
                Self::collapse_seq(items, s..e)
            })
            .collect();
        let end_pos = foot_nodes.last().map(|n| n.span().end).unwrap_or(self.pos);
        Ok(Self::collapse_seq(foot_nodes, start_pos..end_pos))
    }

    fn collapse_seq<T>(mut items: Vec<TPat<T>>, fallback_span: ByteSpan) -> TPat<T> {
        if items.is_empty() {
            TPat::Silence {
                span: fallback_span,
            }
        } else if items.len() == 1 {
            items.pop().unwrap()
        } else {
            let s = items.first().unwrap().span().start;
            let e = items.last().unwrap().span().end;
            TPat::Seq { items, span: s..e }
        }
    }

    fn parse_choice_item<T: FromMiniAtom>(&mut self) -> Result<TPat<T>, ParseError> {
        let first = self.parse_modulated()?;
        let start_pos = first.span().start;
        let mut options = vec![first];

        while self.peek_char() == Some('|') {
            self.pos += 1;
            let next_opt = self.parse_modulated()?;
            options.push(next_opt);
        }

        if options.len() == 1 {
            Ok(options.pop().unwrap())
        } else {
            let end_pos = options.last().unwrap().span().end;
            Ok(TPat::Choose {
                options,
                span: start_pos..end_pos,
            })
        }
    }

    fn parse_modulated<T: FromMiniAtom>(&mut self) -> Result<TPat<T>, ParseError> {
        let mut node = self.parse_primary()?;
        let start_pos = node.span().start;

        // Check for range `..` (`a..b` or `a..step..b`)
        if self.src[self.pos..].starts_with("..") {
            self.pos += 2;
            let second = self.parse_primary()?;
            if self.src[self.pos..].starts_with("..") {
                self.pos += 2;
                let third = self.parse_primary()?;
                let end_pos = third.span().end;
                node = TPat::EnumFromTo {
                    start: Box::new(node),
                    step: Some(Box::new(second)),
                    end: Box::new(third),
                    span: start_pos..end_pos,
                };
            } else {
                let end_pos = second.span().end;
                node = TPat::EnumFromTo {
                    start: Box::new(node),
                    step: None,
                    end: Box::new(second),
                    span: start_pos..end_pos,
                };
            }
        }

        // Postfix operators: `*`, `/`, `@`, `!`, `?`, `:`, `(k,n,o)`
        while let Some(ch) = self.peek_char() {
            match ch {
                '*' => {
                    self.pos += 1;
                    let factor = self.parse_postfix_arg::<Rational>()?;
                    let end_pos = factor.span().end;
                    node = TPat::Fast {
                        factor: Box::new(factor),
                        inner: Box::new(node),
                        span: start_pos..end_pos,
                    };
                }
                '/' => {
                    self.pos += 1;
                    let factor = self.parse_postfix_arg::<Rational>()?;
                    let end_pos = factor.span().end;
                    node = TPat::Slow {
                        factor: Box::new(factor),
                        inner: Box::new(node),
                        span: start_pos..end_pos,
                    };
                }
                '@' => {
                    self.pos += 1;
                    let tok_start = self.pos;
                    let raw = self.read_postfix_token_str();
                    let tok_end = self.pos;
                    if raw.is_empty() {
                        return Err(RichCompilerDiagnostic::new(
                            "SW-E014-MISSING-WEIGHT",
                            "expected weight or trig condition after '@'",
                            self.src,
                            (tok_start - 1)..tok_end,
                        ));
                    }
                    if let Some(cond) = parse_trig_condition_suffix(raw) {
                        node = TPat::TrigCond {
                            cond,
                            inner: Box::new(node),
                            span: start_pos..tok_end,
                        };
                    } else if let Ok(w) =
                        Rational::from_mini_atom(raw, tok_start..tok_end, self.src)
                    {
                        if w.floor_i64() > (MAX_REPLICATE_OR_ELONGATE as i64) {
                            return Err(RichCompilerDiagnostic::new(
                                "SW-E021-MAX-ELONGATE-EXCEEDED",
                                "elongation weight exceeds MAX_REPLICATE_OR_ELONGATE",
                                self.src,
                                tok_start..tok_end,
                            ));
                        }
                        node = TPat::Elongate {
                            weight: w,
                            inner: Box::new(node),
                            span: start_pos..tok_end,
                        };
                    } else {
                        // Voicing dictionary tag like `@lefthand` or `@drop2` on an atom/chord
                        if let TPat::Chord {
                            ref mut dict_tag,
                            ref mut span,
                            ..
                        } = node
                        {
                            *dict_tag = Some(raw.to_string());
                            span.end = tok_end;
                        } else {
                            return Err(RichCompilerDiagnostic::new(
                                "SW-E014-INVALID-AT-MODIFIER",
                                alloc::format!("invalid '@' modifier '{}'", raw),
                                self.src,
                                tok_start..tok_end,
                            ));
                        }
                    }
                }
                '!' => {
                    self.pos += 1;
                    let bang_end = self.pos;
                    let count_pat = if self
                        .peek_char()
                        .map(|c| c.is_ascii_digit() || c == '<' || c == '[')
                        .unwrap_or(false)
                    {
                        self.parse_postfix_arg::<usize>()?
                    } else {
                        TPat::Atom {
                            value: 2usize,
                            span: (bang_end - 1)..bang_end,
                        }
                    };
                    if let TPat::Atom { value, ref span } = count_pat
                        && value > MAX_REPLICATE_OR_ELONGATE
                    {
                        return Err(RichCompilerDiagnostic::new(
                            "SW-E021-MAX-REPLICATE-EXCEEDED",
                            "replicate count exceeds MAX_REPLICATE_OR_ELONGATE",
                            self.src,
                            span.clone(),
                        ));
                    }
                    let end_pos = count_pat.span().end;
                    node = TPat::Repeat {
                        count: Box::new(count_pat),
                        inner: Box::new(node),
                        span: start_pos..end_pos,
                    };
                }
                '?' => {
                    self.pos += 1;
                    let q_end = self.pos;
                    let prob_pat = if self
                        .peek_char()
                        .map(|c| c.is_ascii_digit() || c == '.' || c == '<' || c == '[')
                        .unwrap_or(false)
                    {
                        self.parse_postfix_arg::<f64>()?
                    } else {
                        TPat::Atom {
                            value: 0.5f64,
                            span: (q_end - 1)..q_end,
                        }
                    };
                    let salt = self.salt_counter;
                    self.salt_counter += 1;
                    let end_pos = prob_pat.span().end;
                    node = TPat::DegradeBy {
                        prob: Box::new(prob_pat),
                        seed_salt: salt,
                        inner: Box::new(node),
                        span: start_pos..end_pos,
                    };
                }
                ':' => {
                    self.pos += 1;
                    let index = self.parse_postfix_arg::<usize>()?;
                    let end_pos = index.span().end;
                    node = TPat::SampleIndex {
                        target: Box::new(node),
                        index: Box::new(index),
                        span: start_pos..end_pos,
                    };
                }
                '(' => {
                    let open_pos = self.pos;
                    let open_span = SourceSpan::from_byte_range(self.src, open_pos..(open_pos + 1));
                    self.delimiters.push(DelimiterKind::Paren, open_span);
                    self.pos += 1;
                    self.check_depth(open_pos..(open_pos + 1))?;

                    self.skip_ws_and_comments()?;
                    let pulses = self.parse_postfix_arg::<isize>()?;
                    self.skip_ws_and_comments()?;
                    if self.peek_char() != Some(',') {
                        return Err(RichCompilerDiagnostic::new(
                            "SW-E015-INVALID-EUCLID",
                            "expected ',' separating pulses and steps in Euclidean '(k,n)'",
                            self.src,
                            open_pos..self.pos,
                        ));
                    }
                    self.pos += 1;
                    self.skip_ws_and_comments()?;
                    let steps = self.parse_postfix_arg::<isize>()?;
                    if let TPat::Atom { value, ref span } = steps
                        && value.unsigned_abs() > MAX_EUCLID_STEPS
                    {
                        return Err(RichCompilerDiagnostic::new(
                            "SW-E022-MAX-EUCLID-EXCEEDED",
                            "Euclidean step count exceeds MAX_EUCLID_STEPS (1024)",
                            self.src,
                            span.clone(),
                        ));
                    }
                    self.skip_ws_and_comments()?;
                    let offset = if self.peek_char() == Some(',') {
                        self.pos += 1;
                        self.skip_ws_and_comments()?;
                        Some(Box::new(self.parse_postfix_arg::<isize>()?))
                    } else {
                        None
                    };
                    self.skip_ws_and_comments()?;
                    let Some(close_ch) = self.peek_char() else {
                        let eof_span =
                            SourceSpan::from_byte_range(self.src, self.src.len()..self.src.len());
                        return Err(self.delimiters.check_empty_at_eof(eof_span).unwrap_err());
                    };
                    let close_span = SourceSpan::from_byte_range(
                        self.src,
                        self.pos..(self.pos + close_ch.len_utf8()),
                    );
                    if let Some(ck) = DelimiterKind::from_close(close_ch) {
                        self.pos += close_ch.len_utf8();
                        self.delimiters.pop_matching(ck, close_span)?;
                    } else {
                        return Err(self
                            .delimiters
                            .pop_matching(DelimiterKind::Paren, close_span)
                            .unwrap_err());
                    }
                    let end_pos = self.pos;
                    node = TPat::Euclid {
                        pulses: Box::new(pulses),
                        steps: Box::new(steps),
                        offset,
                        inner: Box::new(node),
                        span: start_pos..end_pos,
                    };
                }
                _ => break,
            }
        }
        Ok(node)
    }

    fn parse_postfix_arg<U: FromMiniAtom>(&mut self) -> Result<TPat<U>, ParseError> {
        let Some(ch) = self.peek_char() else {
            return Err(RichCompilerDiagnostic::new(
                "SW-E016-MISSING-OPERAND",
                "missing operand after postfix operator",
                self.src,
                self.pos..self.pos,
            ));
        };
        if ch == '[' || ch == '<' {
            return self.parse_primary::<U>();
        }
        let start = self.pos;
        let raw = self.read_postfix_token_str();
        let end = self.pos;
        if raw.is_empty() {
            return Err(RichCompilerDiagnostic::new(
                "SW-E016-MISSING-OPERAND",
                "expected value after operator",
                self.src,
                start..end,
            ));
        }
        let val = U::from_mini_atom(raw, start..end, self.src)?;
        Ok(TPat::Atom {
            value: val,
            span: start..end,
        })
    }

    fn read_postfix_token_str(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric()
                || ch == '.'
                || ch == '/'
                || ch == '-'
                || ch == ':'
                || ch == '!'
            {
                // Stop before `..` range operator
                if ch == '.' && self.src[self.pos..].starts_with("..") {
                    break;
                }
                // Stop before `!` replicate operator unless at the very start (`@!1st`, `@!fill`)
                if ch == '!' && self.pos > start {
                    break;
                }
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        &self.src[start..self.pos]
    }

    fn parse_primary<T: FromMiniAtom>(&mut self) -> Result<TPat<T>, ParseError> {
        self.skip_ws_and_comments()?;
        let Some(ch) = self.peek_char() else {
            return Err(RichCompilerDiagnostic::new(
                "SW-E006-UNEXPECTED-EOF",
                "unexpected end of mini-notation input",
                self.src,
                self.pos..self.pos,
            ));
        };

        match ch {
            '[' => {
                let open_pos = self.pos;
                let open_span = SourceSpan::from_byte_range(self.src, open_pos..(open_pos + 1));
                self.delimiters
                    .push(DelimiterKind::SquareBracket, open_span);
                self.pos += 1;
                self.check_depth(open_pos..(open_pos + 1))?;
                let inner = self.parse_stack(Some(']'))?;
                self.expect_closing_delimiter(DelimiterKind::SquareBracket)?;
                let end_pos = self.pos;
                Ok(match inner {
                    TPat::Seq { items, .. } => TPat::Seq {
                        items,
                        span: open_pos..end_pos,
                    },
                    TPat::Stack { layers, .. } => TPat::Stack {
                        layers,
                        span: open_pos..end_pos,
                    },
                    other => TPat::Seq {
                        items: vec![other],
                        span: open_pos..end_pos,
                    },
                })
            }
            '<' => {
                let open_pos = self.pos;
                let open_span = SourceSpan::from_byte_range(self.src, open_pos..(open_pos + 1));
                self.delimiters.push(DelimiterKind::AngleBracket, open_span);
                self.pos += 1;
                self.check_depth(open_pos..(open_pos + 1))?;
                let inner = self.parse_stack(Some('>'))?;
                self.expect_closing_delimiter(DelimiterKind::AngleBracket)?;
                let end_pos = self.pos;
                let items = match inner {
                    TPat::Seq { items, .. } => items,
                    other => vec![other],
                };
                Ok(TPat::Slowcat {
                    items,
                    span: open_pos..end_pos,
                })
            }
            '{' => {
                let open_pos = self.pos;
                let open_span = SourceSpan::from_byte_range(self.src, open_pos..(open_pos + 1));
                self.delimiters.push(DelimiterKind::CurlyBrace, open_span);
                self.pos += 1;
                self.check_depth(open_pos..(open_pos + 1))?;
                let inner = self.parse_stack(Some('}'))?;
                self.expect_closing_delimiter(DelimiterKind::CurlyBrace)?;
                let layers = match inner {
                    TPat::Stack { layers, .. } => layers,
                    other => vec![other],
                };
                let steps = if self.peek_char() == Some('%') {
                    self.pos += 1;
                    Some(Box::new(self.parse_postfix_arg::<Rational>()?))
                } else {
                    None
                };
                let end_pos = self.pos;
                Ok(TPat::Polymeter {
                    layers,
                    steps,
                    span: open_pos..end_pos,
                })
            }
            '(' => {
                // Mondo notation S-expression `(func arg1 arg2 ...)`
                let open_pos = self.pos;
                let open_span = SourceSpan::from_byte_range(self.src, open_pos..(open_pos + 1));
                self.delimiters.push(DelimiterKind::Paren, open_span);
                self.pos += 1;
                self.check_depth(open_pos..(open_pos + 1))?;
                self.skip_ws_and_comments()?;
                let fn_start = self.pos;
                let func = self.read_atom_str().to_string();
                if func.is_empty() {
                    return Err(RichCompilerDiagnostic::new(
                        "SW-E017-EMPTY-MONDO-CALL",
                        "expected function identifier after '(' in Mondo expression",
                        self.src,
                        fn_start..self.pos,
                    ));
                }
                let mut args = Vec::new();
                loop {
                    self.skip_ws_and_comments()?;
                    let Some(c) = self.peek_char() else {
                        break;
                    };
                    if c == ')' || DelimiterKind::from_close(c).is_some() {
                        break;
                    }
                    args.push(self.parse_choice_item()?);
                }
                self.expect_closing_delimiter(DelimiterKind::Paren)?;
                let end_pos = self.pos;
                Ok(TPat::MondoCall {
                    func,
                    args,
                    span: open_pos..end_pos,
                })
            }
            '~' => {
                let s = self.pos;
                self.pos += 1;
                Ok(TPat::Silence { span: s..self.pos })
            }
            '_' => {
                let s = self.pos;
                self.pos += 1;
                Ok(TPat::Tie { span: s..self.pos })
            }
            '^' => {
                let s = self.pos;
                self.pos += 1;
                let local_time = if self.peek_char() == Some('^') {
                    self.pos += 1;
                    true
                } else {
                    false
                };
                let name = self.read_atom_str().to_string();
                if name.is_empty() {
                    return Err(RichCompilerDiagnostic::new(
                        "SW-E018-EMPTY-STATE-VAR",
                        "expected state variable identifier after '^'",
                        self.src,
                        s..self.pos,
                    ));
                }
                Ok(TPat::Var {
                    name,
                    local_time,
                    span: s..self.pos,
                })
            }
            _ => {
                let start = self.pos;
                let raw = self.read_atom_str();
                let end = self.pos;
                if raw.is_empty() {
                    let bad_ch = self.advance_char().unwrap_or(' ');
                    let bad_span = SourceSpan::from_byte_range(self.src, start..self.pos);
                    if let Some(ck) = DelimiterKind::from_close(bad_ch) {
                        return Err(self.delimiters.pop_matching(ck, bad_span).unwrap_err());
                    }
                    return Err(RichCompilerDiagnostic::new(
                        "SW-E004-UNEXPECTED-TOKEN",
                        alloc::format!("unexpected character '{}'", bad_ch),
                        self.src,
                        start..self.pos,
                    ));
                }
                if raw == "-" {
                    return Ok(TPat::Silence { span: start..end });
                }
                // Check if chord symbol (e.g., `C^7`, `Am7`, `G7b9`, `c'maj7`) when `raw` has chord quality markers
                if is_chord_literal(raw)
                    && let Some(parsed) = parse_chord_symbol(raw)
                {
                    let root = NoteSpec {
                        pitch_class: parsed.root_semitone,
                        octave: parsed.root_octave,
                        explicit_octave: false,
                    };
                    return Ok(TPat::Chord {
                        symbol: raw.to_string(),
                        root,
                        quality: parsed.quality,
                        inversion: None,
                        dict_tag: parsed.inline_dict,
                        span: start..end,
                    });
                }
                let val = T::from_mini_atom(raw, start..end, self.src)?;
                Ok(TPat::Atom {
                    value: val,
                    span: start..end,
                })
            }
        }
    }

    fn expect_closing_delimiter(&mut self, expected: DelimiterKind) -> Result<(), ParseError> {
        self.skip_ws_and_comments()?;
        let Some(ch) = self.peek_char() else {
            let eof_span = SourceSpan::from_byte_range(self.src, self.src.len()..self.src.len());
            return Err(self.delimiters.check_empty_at_eof(eof_span).unwrap_err());
        };
        let c_span = SourceSpan::from_byte_range(self.src, self.pos..(self.pos + ch.len_utf8()));
        if let Some(actual_close) = DelimiterKind::from_close(ch) {
            self.pos += ch.len_utf8();
            self.delimiters.pop_matching(actual_close, c_span)?;
            if actual_close != expected {
                return Err(RichCompilerDiagnostic::new(
                    "SW-E001-MISMATCHED-DELIMITER",
                    alloc::format!("expected '{}', found '{}'", expected.close_char(), ch),
                    self.src,
                    c_span.to_byte_span(),
                ));
            }
            Ok(())
        } else {
            Err(self.delimiters.pop_matching(expected, c_span).unwrap_err())
        }
    }

    fn read_atom_str(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '[' | ']'
                        | '<'
                        | '>'
                        | '{'
                        | '}'
                        | '('
                        | ')'
                        | ','
                        | '|'
                        | '*'
                        | '/'
                        | '@'
                        | '!'
                        | '?'
                )
            {
                break;
            }
            if ch == ':' {
                // `name:index`/`name:<a b>` SampleIndex breaks the atom; `name:fx`
                // (alphabetic suffix) stays atom-internal for `ur` transforms.
                let next_is_alpha = self.src[self.pos + 1..]
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false);
                if !next_is_alpha {
                    break;
                }
            }
            if ch == '.' {
                // Allow decimal point inside numbers (e.g., `0.25`), but stop before `..` range or `.` foot separator
                if self.src[self.pos..].starts_with("..") {
                    break;
                }
                let prev_is_digit =
                    self.pos > start && self.src.as_bytes()[self.pos - 1].is_ascii_digit();
                let next_is_digit = self.src[self.pos + 1..]
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false);
                if !(prev_is_digit && next_is_digit) {
                    break;
                }
            }
            self.pos += ch.len_utf8();
        }
        &self.src[start..self.pos]
    }
}

fn is_chord_literal(token: &str) -> bool {
    if token.len() < 2 {
        return false;
    }
    let first = token.as_bytes()[0];
    if !matches!(first, b'A'..=b'G' | b'a'..=b'g') {
        return false;
    }
    if token.contains('\'') || token.contains('^') {
        return true;
    }
    // Upper-case root with chord quality (`Cmaj7`, `Am7`, `G7b9`, `Dm9`, `Cdim7`)
    if first.is_ascii_uppercase() {
        let rest = &token[1..];
        return rest.contains("maj")
            || rest.contains("min")
            || rest.contains("dim")
            || rest.contains("aug")
            || rest.contains("sus")
            || rest.contains('m')
            || rest.contains('7')
            || rest.contains('9')
            || rest.contains("11")
            || rest.contains("13")
            || rest.contains('6')
            || rest.contains('o');
    }
    false
}
