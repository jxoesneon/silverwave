//! Live-Eval Transpiler (`@strudel/transpiler` Parity, `Section 3.2`) and Fault-Tolerant
//! Per-Stream Session Evaluator (`FaultTolerantCompileResult`, `PP-07`).

use crate::compiler::CompiledHighlightPattern;
use crate::diagnostics::{RichCompilerDiagnostic, suggest_symbol};
use crate::span::{ByteSpan, SourceOrigin};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use silverwave_core::{ControlMap, ControlPattern, PatternFunctor, Time, Value, silence, stack};

/// Fault-isolated compilation output allowing valid streams to update even when sibling blocks fail (`PP-07`).
#[derive(Debug, Clone)]
pub struct FaultTolerantCompileResult {
    pub updated_streams: Vec<String>,
    pub retained_previous_streams: Vec<String>,
    pub muted_streams: Vec<String>,
    pub diagnostics: Vec<RichCompilerDiagnostic>,
}

/// Active stream slot entry inside `MiniStreamSession`.
#[derive(Clone)]
pub struct StreamEntry {
    pub id: String,
    pub pattern: ControlPattern,
    pub highlight: CompiledHighlightPattern<String>,
    pub muted: bool,
    pub soloed: bool,
}

/// Multi-stream live-coding session supporting `d1..d16`, anonymous `$:` streams,
/// instant-muted `_$:` streams (`Section 3.2`), `hush()`, `solo()`, `mute()`, `all()`,
/// and Fault-Tolerant Per-Stream Evaluation (`PP-07`).
#[derive(Default)]
pub struct MiniStreamSession {
    streams: BTreeMap<String, StreamEntry>,
    muted_diagnostics: BTreeMap<String, CompiledHighlightPattern<String>>,
    anon_counter: usize,
}

impl MiniStreamSession {
    /// Creates a new empty `MiniStreamSession`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates a multi-line script containing `d1: ...` .. `d16: ...`, `$: ...`, and `_$: ...` stream declarations.
    /// If one stream contains a syntax error or unknown method typo, all valid streams update
    /// while the broken stream retains its last known valid pattern (`PP-07`).
    pub fn eval_script(&mut self, script: &str) -> FaultTolerantCompileResult {
        let mut updated_streams = Vec::new();
        let mut retained_previous_streams = Vec::new();
        let mut muted_streams = Vec::new();
        let mut diagnostics = Vec::new();

        self.anon_counter = 0;
        let mut offset = 0usize;

        for raw_line in script.split('\n') {
            let line_start = offset;
            offset += raw_line.len() + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            let (stream_id, is_muted_prefix, rhs, rhs_offset) =
                if let Some(rest) = trimmed.strip_prefix("_$:") {
                    self.anon_counter += 1;
                    let id = alloc::format!("_anon_{}", self.anon_counter);
                    let rhs_idx = raw_line.find("_$:").unwrap_or(0) + 3;
                    (id, true, rest.trim(), line_start + rhs_idx)
                } else if let Some(rest) = trimmed.strip_prefix("$:") {
                    self.anon_counter += 1;
                    let id = alloc::format!("anon_{}", self.anon_counter);
                    let rhs_idx = raw_line.find("$:").unwrap_or(0) + 2;
                    (id, false, rest.trim(), line_start + rhs_idx)
                } else if let Some((lhs, rest)) = trimmed.split_once(':') {
                    let id = lhs.trim().to_string();
                    let colon_idx = raw_line.find(':').unwrap_or(0) + 1;
                    (id, false, rest.trim(), line_start + colon_idx)
                } else {
                    self.anon_counter += 1;
                    let id = alloc::format!("anon_{}", self.anon_counter);
                    (id, false, trimmed, line_start)
                };

            match eval_stream_expr(script, rhs, rhs_offset, &stream_id) {
                Ok((pat, hl)) => {
                    if is_muted_prefix {
                        self.muted_diagnostics.insert(stream_id.clone(), hl);
                        muted_streams.push(stream_id);
                    } else {
                        self.streams.insert(
                            stream_id.clone(),
                            StreamEntry {
                                id: stream_id.clone(),
                                pattern: pat,
                                highlight: hl,
                                muted: false,
                                soloed: false,
                            },
                        );
                        updated_streams.push(stream_id);
                    }
                }
                Err(mut diag) => {
                    diag.isolated_stream_id = Some(stream_id.clone());
                    diagnostics.push(diag);
                    if self.streams.contains_key(&stream_id) {
                        retained_previous_streams.push(stream_id);
                    }
                }
            }
        }

        FaultTolerantCompileResult {
            updated_streams,
            retained_previous_streams,
            muted_streams,
            diagnostics,
        }
    }

    /// Binds a `ControlPattern` directly to `id` (e.g. `"d1"`).
    pub fn bind_stream(&mut self, id: &str, pattern: ControlPattern) {
        let hl = CompiledHighlightPattern::compile_once("bd", SourceOrigin::root()).unwrap();
        self.streams.insert(
            id.to_string(),
            StreamEntry {
                id: id.to_string(),
                pattern,
                highlight: hl,
                muted: false,
                soloed: false,
            },
        );
    }

    /// Silences and clears all active streams (`hush()`).
    pub fn hush(&mut self) {
        self.streams.clear();
    }

    /// Mutes stream `id`.
    pub fn mute(&mut self, id: &str) {
        if let Some(entry) = self.streams.get_mut(id) {
            entry.muted = true;
        }
    }

    /// Unmutes stream `id`.
    pub fn unmute(&mut self, id: &str) {
        if let Some(entry) = self.streams.get_mut(id) {
            entry.muted = false;
        }
    }

    /// Solos stream `id`.
    pub fn solo(&mut self, id: &str) {
        if let Some(entry) = self.streams.get_mut(id) {
            entry.soloed = true;
        }
    }

    /// Unsolos stream `id`.
    pub fn unsolo(&mut self, id: &str) {
        if let Some(entry) = self.streams.get_mut(id) {
            entry.soloed = false;
        }
    }

    /// Applies global transformation `f` to every active stream (`all`).
    pub fn all<F>(&mut self, f: F)
    where
        F: Fn(&ControlPattern) -> ControlPattern,
    {
        for entry in self.streams.values_mut() {
            entry.pattern = f(&entry.pattern);
        }
    }

    /// Returns the combined `ControlPattern` across all audible (non-muted, solo-filtered) streams.
    pub fn combined_pattern(&self) -> ControlPattern {
        let any_solo = self.streams.values().any(|s| s.soloed);
        let active: Vec<ControlPattern> = self
            .streams
            .values()
            .filter(|s| !s.muted && (!any_solo || s.soloed))
            .map(|s| s.pattern.clone())
            .collect();
        if active.is_empty() {
            silence()
        } else {
            stack(active)
        }
    }

    /// Returns all highlight spans for a muted `_$:` stream (`test_transpiler_streams_and_mute`).
    pub fn muted_highlight_spans(&self, id: &str) -> Vec<ByteSpan> {
        self.muted_diagnostics
            .get(id)
            .map(|hl| hl.highlight_table().to_vec())
            .unwrap_or_default()
    }

    /// Returns the active stream IDs.
    pub fn active_stream_ids(&self) -> BTreeSet<String> {
        self.streams.keys().cloned().collect()
    }
}

fn eval_stream_expr(
    full_script: &str,
    expr: &str,
    expr_byte_offset: usize,
    stream_id: &str,
) -> Result<(ControlPattern, CompiledHighlightPattern<String>), RichCompilerDiagnostic> {
    // Check for chained method calls (`s("bd sd").rom(0.5)` or `s("bd sd").fast(2)`)
    let (primary_call, chain_calls) = split_method_chain(expr);

    // Validate chained method names against KNOWN_DSL_SYMBOLS using Damerau-Levenshtein (`PP-06`)
    for (m_name, m_arg, m_rel_offset) in &chain_calls {
        let abs_start = expr_byte_offset + *m_rel_offset;
        let abs_end = abs_start + m_name.len() + 1;
        if !crate::diagnostics::KNOWN_DSL_SYMBOLS.contains(&m_name.as_str()) {
            let mut diag = RichCompilerDiagnostic::new(
                "SW-E007-UNKNOWN-METHOD",
                alloc::format!("unknown method '.{}' on stream '{}'", m_name, stream_id),
                full_script,
                abs_start..abs_end,
            );
            diag.did_you_mean = suggest_symbol(&alloc::format!(".{}", m_name));
            diag.isolated_stream_id = Some(stream_id.to_string());
            let _ = m_arg;
            return Err(diag);
        }
    }

    // Extract quoted mini-notation literal (`"..."` or `` `...` ``) or raw mini string
    let (mini_str, origin) = extract_mini_literal(full_script, primary_call, expr_byte_offset)?;
    let hl = CompiledHighlightPattern::<String>::compile_once(&mini_str, origin)?;
    // `compile_once` already parsed and compiled `mini_str` with this `origin`
    // (deterministic parser), so re-parsing could never fail — reuse the
    // cached pattern instead of a fallible second parse.
    let str_pat = hl.pattern().clone();
    let mut ctrl_pat: ControlPattern = str_pat.fmap(|tok| {
        let mut m = ControlMap::new();
        m.insert("s".to_string(), Value::String(tok.clone()));
        m
    });

    for (m_name, m_arg, _) in chain_calls {
        match m_name.as_str() {
            "fast" => {
                if let Ok(f) = m_arg.parse::<i64>() {
                    ctrl_pat = ctrl_pat.fast(Time::from_integer(f));
                }
            }
            "slow" => {
                if let Ok(f) = m_arg.parse::<i64>() {
                    ctrl_pat = ctrl_pat.slow(Time::from_integer(f));
                }
            }
            "rev" => {
                ctrl_pat = ctrl_pat.rev();
            }
            "room" | "lpf" | "hpf" | "gain" | "pan" => {
                if let Ok(val) = m_arg.parse::<f64>() {
                    let key = m_name.clone();
                    ctrl_pat = ctrl_pat.fmap(move |m| {
                        let mut next = m.clone();
                        next.insert(key.clone(), Value::Float(val));
                        next
                    });
                }
            }
            _ => {}
        }
    }

    Ok((ctrl_pat, hl))
}

fn split_method_chain(expr: &str) -> (&str, Vec<(String, String, usize)>) {
    // Find `.method(arg)` suffixes after the initial `s("...")` or `"..."`
    let mut chains = Vec::new();
    let mut main_end = expr.len();
    let mut search_from = 0usize;
    if let Some(first_close) = expr.find(')') {
        search_from = first_close + 1;
        main_end = search_from;
    }
    let tail = &expr[search_from..];
    let mut rel = 0usize;
    while let Some(dot_idx) = tail[rel..].find('.') {
        let m_start = rel + dot_idx;
        let after_dot = &tail[(m_start + 1)..];
        if let Some(open_p) = after_dot.find('(') {
            let m_name = after_dot[..open_p].trim().to_string();
            let after_open = &after_dot[(open_p + 1)..];
            if let Some(close_p) = after_open.find(')') {
                let m_arg = after_open[..close_p].trim().to_string();
                chains.push((m_name, m_arg, search_from + m_start));
                rel = m_start + 1 + open_p + 1 + close_p + 1;
                continue;
            }
        }
        break;
    }
    (&expr[..main_end], chains)
}

fn extract_mini_literal(
    full_script: &str,
    primary: &str,
    primary_offset: usize,
) -> Result<(String, SourceOrigin), RichCompilerDiagnostic> {
    for quote in ['"', '`'] {
        if let Some(q_start) = primary.find(quote) {
            let rest = &primary[(q_start + 1)..];
            let Some(q_end) = rest.rfind(quote) else {
                let abs_q = primary_offset + q_start;
                return Err(RichCompilerDiagnostic::new(
                    "SW-E003-UNCLOSED-OPENING-DELIMITER",
                    alloc::format!("unclosed quote '{}' at byte {}", quote, abs_q),
                    full_script,
                    abs_q..(abs_q + 1),
                ));
            };
            let raw_inner = &rest[..q_end];
            let abs_quote_byte = primary_offset + q_start;
            let (unescaped, origin) =
                SourceOrigin::unescape_with_origin(raw_inner, abs_quote_byte, 1);
            return Ok((unescaped, origin));
        }
    }
    Ok((
        primary.trim().to_string(),
        SourceOrigin {
            outer_content_offset: primary_offset,
            escape_shifts: Vec::new(),
        },
    ))
}
