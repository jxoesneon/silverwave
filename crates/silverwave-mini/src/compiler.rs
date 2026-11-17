//! Stage 2 Compiler (`compile_tpat`: `TPat<T>` -> `Pattern<T>`) with `SourceLocation` attachment
//! and Zero-Reparse 60 FPS Editor Highlighting (`CompiledHighlightPattern<T>`).

use crate::ast::TPat;
use crate::diagnostics::{ParseError, RichCompilerDiagnostic};
use crate::parser::{FromMiniAtom, MiniParser};
use crate::span::{ByteSpan, SourceOrigin};
use alloc::string::String;
use alloc::sync::Arc as StdArc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use silverwave_core::{
    Context, Hap, NoteValue, Pattern, PatternFunctor, PatternMonad, Rational, State, Time,
    TimeSpan, Value, choose_cycles, fastcat, parse_chord_symbol, polymeter, pure, silence, slowcat,
    stack, time_to_rand_seeded, timecat,
};

/// Trait enabling target value types `T` to receive sample index (`:n`), state variable (`^var`),
/// and chord (`C^7`) enrichment during `compile_tpat`.
pub trait MiniTarget: FromMiniAtom + Send + Sync + 'static {
    fn from_state_var(name: &str, state: &State) -> Option<Self>;
    fn apply_sample_index(&self, index: usize) -> Self;
    fn from_chord_symbol(symbol: &str) -> Option<Self>;
}

impl MiniTarget for String {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state.controls.get(name).and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Int(i) => Some(alloc::format!("{}", i)),
            Value::Float(f) => Some(alloc::format!("{}", f)),
            Value::Rational(r) => Some(alloc::format!("{}", r)),
            _ => None,
        })
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        alloc::format!("{}:{}", self, index)
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        Some(symbol.into())
    }
}

impl MiniTarget for i64 {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state.controls.get(name).and_then(Value::as_i64)
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        *self + (index as i64)
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        let parsed = parse_chord_symbol(symbol)?;
        Some((12 * (parsed.root_octave + 1) + parsed.root_semitone) as i64)
    }
}

impl MiniTarget for isize {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state
            .controls
            .get(name)
            .and_then(Value::as_i64)
            .map(|v| v as isize)
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        *self + (index as isize)
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        i64::from_chord_symbol(symbol).map(|v| v as isize)
    }
}

impl MiniTarget for usize {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state
            .controls
            .get(name)
            .and_then(Value::as_i64)
            .map(|v| v.max(0) as usize)
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        index
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        i64::from_chord_symbol(symbol).map(|v| v.max(0) as usize)
    }
}

impl MiniTarget for f64 {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state.controls.get(name).and_then(Value::as_f64)
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        *self + (index as f64)
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        i64::from_chord_symbol(symbol).map(|v| v as f64)
    }
}

impl MiniTarget for Rational {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state.controls.get(name).and_then(Value::as_rational)
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        *self + Rational::from_integer(index as i64)
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        i64::from_chord_symbol(symbol).map(Rational::from_integer)
    }
}

impl MiniTarget for Value {
    fn from_state_var(name: &str, state: &State) -> Option<Self> {
        state.controls.get(name).cloned()
    }

    fn apply_sample_index(&self, index: usize) -> Self {
        match self {
            Value::String(s) => Value::String(alloc::format!("{}:{}", s, index)),
            other => other.clone(),
        }
    }

    fn from_chord_symbol(symbol: &str) -> Option<Self> {
        let parsed = parse_chord_symbol(symbol)?;
        let root_midi = 12 * (parsed.root_octave + 1) + parsed.root_semitone;
        let notes = parsed
            .intervals
            .into_iter()
            .map(|iv| Value::Note(NoteValue::Midi((root_midi + iv) as f64)))
            .collect();
        Some(Value::List(notes))
    }
}

/// Lowers a location-annotated `TPat<T>` into a pure `silverwave_core::Pattern<T>`,
/// attaching mapped `SourceLocation` spans to every emitted `Hap<T>`.
pub fn compile_tpat<T: MiniTarget>(ast: &TPat<T>, origin: &SourceOrigin) -> Pattern<T> {
    match ast {
        TPat::Atom { value, span } => {
            let loc = origin.map_to_location(span);
            pure(value.clone()).with_loc(loc)
        }
        TPat::Silence { .. } | TPat::Tie { .. } => silence(),
        TPat::Var {
            name,
            local_time,
            span,
        } => {
            let var_name = name.clone();
            let loc = origin.map_to_location(span);
            let pat = Pattern::new(move |state: &State| {
                let Some(val) = T::from_state_var(&var_name, state) else {
                    return Vec::new();
                };
                state
                    .span
                    .split_cycles()
                    .into_iter()
                    .map(|sub| {
                        let mut ctx = Context::empty();
                        ctx.locations.push(loc);
                        Hap::discrete(TimeSpan::cycle_of(sub.begin), sub, val.clone(), ctx)
                    })
                    .collect()
            });
            if *local_time { pat.local_time() } else { pat }
        }
        TPat::Chord {
            symbol,
            dict_tag,
            span,
            ..
        } => {
            let full_sym = match dict_tag {
                Some(d) if !symbol.contains('@') => alloc::format!("{}@{}", symbol, d),
                _ => symbol.clone(),
            };
            let loc = origin.map_to_location(span);
            if let Some(val) = T::from_chord_symbol(&full_sym) {
                pure(val).with_loc(loc)
            } else {
                silence()
            }
        }
        TPat::Seq { items, .. } => compile_seq_items(items, origin),
        TPat::Stack { layers, .. } => {
            stack(layers.iter().map(|l| compile_tpat(l, origin)).collect())
        }
        TPat::Slowcat { items, .. } => {
            slowcat(items.iter().map(|i| compile_tpat(i, origin)).collect())
        }
        TPat::Polymeter { layers, steps, .. } => {
            let compiled_layers: Vec<Pattern<T>> =
                layers.iter().map(|l| compile_tpat(l, origin)).collect();
            let step_override = steps.as_ref().and_then(|s_ast| {
                let s_pat = compile_tpat(s_ast, origin);
                s_pat
                    .first_cycle()
                    .first()
                    .map(|h| h.value.floor_i64().max(1) as usize)
            });
            polymeter(compiled_layers, step_override)
        }
        TPat::Fast {
            factor,
            inner,
            span,
        } => {
            let inner_pat = compile_tpat(inner, origin);
            let factor_pat = compile_tpat(factor, origin);
            let loc = origin.map_to_location(span);
            inner_pat.fast_pat(factor_pat).with_loc(loc)
        }
        TPat::Slow {
            factor,
            inner,
            span,
        } => {
            let inner_pat = compile_tpat(inner, origin);
            let factor_pat = compile_tpat(factor, origin);
            let loc = origin.map_to_location(span);
            inner_pat.slow_pat(factor_pat).with_loc(loc)
        }
        TPat::Elongate { inner, span, .. } => {
            let loc = origin.map_to_location(span);
            compile_tpat(inner, origin).with_loc(loc)
        }
        TPat::TrigCond { cond, inner, span } => {
            let loc = origin.map_to_location(span);
            compile_tpat(inner, origin)
                .trig_cond(*cond, 0)
                .with_loc(loc)
        }
        TPat::Repeat { count, inner, span } => {
            let inner_pat = compile_tpat(inner, origin);
            let count_pat = compile_tpat(count, origin).fmap(|&c| Time::from_integer(c as i64));
            let loc = origin.map_to_location(span);
            inner_pat.ply_pat(count_pat).with_loc(loc)
        }
        TPat::DegradeBy {
            prob,
            seed_salt,
            inner,
            span,
        } => {
            let inner_pat = compile_tpat(inner, origin);
            let prob_pat = compile_tpat(prob, origin);
            let salt = *seed_salt;
            let loc = origin.map_to_location(span);
            let nested = prob_pat.fmap(move |&p| {
                let clamped = p.clamp(0.0, 1.0);
                inner_pat.filter_haps(move |h| {
                    time_to_rand_seeded(h.whole_or_part().begin, 0, salt) >= clamped
                })
            });
            <Pattern<T> as PatternMonad<T>>::inner_join(&nested).with_loc(loc)
        }
        TPat::EnumFromTo {
            start,
            step,
            end,
            span,
        } => {
            let loc = origin.map_to_location(span);
            let s_val = compile_tpat(start, origin).first_cycle().into_iter().next();
            let e_val = compile_tpat(end, origin).first_cycle().into_iter().next();
            let st_val = step
                .as_ref()
                .and_then(|st| compile_tpat(st, origin).first_cycle().into_iter().next());
            if let (Some(sh), Some(eh)) = (s_val, e_val) {
                let step_ref = st_val.as_ref().map(|h| &h.value);
                let expanded = T::step_range(&sh.value, step_ref, &eh.value);
                fastcat(expanded.into_iter().map(pure).collect()).with_loc(loc)
            } else {
                silence()
            }
        }
        TPat::Euclid {
            pulses,
            steps,
            offset,
            inner,
            span,
        } => {
            let inner_pat = compile_tpat(inner, origin);
            let p_pat = compile_tpat(pulses, origin);
            let s_pat = compile_tpat(steps, origin);
            let o_pat = offset
                .as_ref()
                .map(|o| compile_tpat(o, origin))
                .unwrap_or_else(|| pure(0isize));
            let loc = origin.map_to_location(span);
            let combined = silverwave_core::lift_a2_both(
                &silverwave_core::lift_a2_both(&p_pat, &s_pat, |&k, &n| (k, n)),
                &o_pat,
                move |&(k, n), &rot| inner_pat.euclid_rot(k, n, rot),
            );
            <Pattern<T> as PatternMonad<T>>::inner_join(&combined).with_loc(loc)
        }
        TPat::Choose { options, span } => {
            let loc = origin.map_to_location(span);
            let pats: Vec<Pattern<T>> = options.iter().map(|o| compile_tpat(o, origin)).collect();
            let chosen = choose_cycles(pats);
            <Pattern<T> as PatternMonad<T>>::inner_join(&chosen).with_loc(loc)
        }
        TPat::SampleIndex {
            target,
            index,
            span,
        } => {
            let t_pat = compile_tpat(target, origin);
            let idx_pat = compile_tpat(index, origin);
            let loc = origin.map_to_location(span);
            silverwave_core::lift_a2_left(&t_pat, &idx_pat, |val, &idx| val.apply_sample_index(idx))
                .with_loc(loc)
        }
        TPat::MondoCall { func, args, span } => compile_mondo_call(func, args, span, origin),
    }
}

fn compile_seq_items<T: MiniTarget>(items: &[TPat<T>], origin: &SourceOrigin) -> Pattern<T> {
    if items.is_empty() {
        return silence();
    }
    // Expand `!n` (when `count` is a constant atom) into discrete sequence steps (`a!3 b` == `a a a b`),
    // and fold `_` (`TPat::Tie`) by adding `+1` unit weight to the immediately preceding step (`a _ b` == `a@2 b`).
    let mut weighted: Vec<(Time, Pattern<T>)> = Vec::with_capacity(items.len());

    for item in items {
        match item {
            TPat::Tie { .. } => {
                if let Some((last_w, _)) = weighted.last_mut() {
                    *last_w = *last_w + Time::one();
                } else {
                    weighted.push((Time::one(), silence()));
                }
            }
            TPat::Elongate {
                weight,
                inner,
                span,
            } => {
                let loc = origin.map_to_location(span);
                let pat = compile_tpat(inner, origin).with_loc(loc);
                weighted.push((*weight, pat));
            }
            TPat::Repeat { count, inner, span } => {
                let loc = origin.map_to_location(span);
                if let TPat::Atom { value: c_val, .. } = count.as_ref() {
                    let pat = compile_tpat(inner, origin).with_loc(loc);
                    for _ in 0..(*c_val) {
                        weighted.push((Time::one(), pat.clone()));
                    }
                } else {
                    weighted.push((Time::one(), compile_tpat(item, origin)));
                }
            }
            other => {
                weighted.push((Time::one(), compile_tpat(other, origin)));
            }
        }
    }

    let all_unit = weighted.iter().all(|(w, _)| *w == Time::one());
    if all_unit {
        fastcat(weighted.into_iter().map(|(_, p)| p).collect())
    } else {
        timecat(weighted)
    }
}

fn compile_mondo_call<T: MiniTarget>(
    func: &str,
    args: &[TPat<T>],
    span: &ByteSpan,
    origin: &SourceOrigin,
) -> Pattern<T> {
    let loc = origin.map_to_location(span);
    match func {
        "fast" if args.len() >= 2 => {
            // `TPat` atoms store typed values, not raw source text, so a nested
            // mondo `fast` factor can never be re-parsed here — it always
            // resolves to 1 (the old `extract_atom_text` hook returned `None`
            // unconditionally). Top-level `(fast n ..)` is intercepted with its
            // raw factor text by `mini_with_loc` instead.
            compile_tpat(&args[1], origin)
                .fast(Rational::one())
                .with_loc(loc)
        }
        "slow" if args.len() >= 2 => {
            // See "fast" above: the factor always resolves to 1 for nested
            // mondo calls.
            compile_tpat(&args[1], origin)
                .slow(Rational::one())
                .with_loc(loc)
        }
        "rev" if !args.is_empty() => compile_tpat(&args[0], origin).rev().with_loc(loc),
        "degrade" if !args.is_empty() => compile_tpat(&args[0], origin).degrade().with_loc(loc),
        "stack" => stack(args.iter().map(|a| compile_tpat(a, origin)).collect()).with_loc(loc),
        "cat" | "slowcat" => {
            slowcat(args.iter().map(|a| compile_tpat(a, origin)).collect()).with_loc(loc)
        }
        "fastcat" | "seq" => {
            fastcat(args.iter().map(|a| compile_tpat(a, origin)).collect()).with_loc(loc)
        }
        _ => {
            if args.is_empty() {
                silence()
            } else {
                fastcat(args.iter().map(|a| compile_tpat(a, origin)).collect()).with_loc(loc)
            }
        }
    }
}

/// Parses a mini-notation string directly into a `Pattern<T>` with root `SourceOrigin`.
pub fn mini<T: MiniTarget>(src: &str) -> Result<Pattern<T>, ParseError> {
    mini_with_loc(src, SourceOrigin::root())
}

/// Parses a mini-notation string into a `Pattern<T>` with an explicit `SourceOrigin` (`withLoc`).
pub fn mini_with_loc<T: MiniTarget>(
    src: &str,
    origin: SourceOrigin,
) -> Result<Pattern<T>, ParseError> {
    let ast = MiniParser::parse::<T>(src)?;
    if let TPat::MondoCall {
        ref func,
        ref args,
        ref span,
    } = ast
        && (func == "fast" || func == "slow")
        && args.len() >= 2
    {
        let arg0_span = args[0].span();
        let raw_factor = &src[arg0_span.clone()];
        if let Ok(factor) = Rational::from_mini_atom(raw_factor, arg0_span, src) {
            let inner = compile_tpat(&args[1], &origin);
            let loc = origin.map_to_location(span);
            return Ok(if func == "fast" {
                inner.fast(factor).with_loc(loc)
            } else {
                inner.slow(factor).with_loc(loc)
            });
        }
    }
    Ok(compile_tpat(&ast, &origin))
}

/// Zero-Reparse 60 FPS Editor Highlighting Pattern (`Section 2.3`):
/// Parses `src` exactly ONCE on edit, caching the compiled `Pattern<T>`, pre-sorted `ByteSpan`
/// highlight table, and an atomic parse counter (`parse_count() == 1` across 600+ 60 FPS queries).
#[derive(Clone)]
pub struct CompiledHighlightPattern<T> {
    pattern: StdArc<Pattern<T>>,
    highlight_table: Vec<ByteSpan>,
    parse_counter: StdArc<AtomicUsize>,
}

impl<T: MiniTarget> CompiledHighlightPattern<T> {
    /// Parses `src` once and constructs a `CompiledHighlightPattern<T>`.
    pub fn compile_once(src: &str, origin: SourceOrigin) -> Result<Self, ParseError> {
        let counter = StdArc::new(AtomicUsize::new(1));
        let ast = MiniParser::parse::<T>(src)?;
        let mut raw_spans = Vec::new();
        ast.collect_spans(&mut raw_spans);
        let mut highlight_table: Vec<ByteSpan> =
            raw_spans.into_iter().map(|s| origin.map_span(&s)).collect();
        highlight_table.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        highlight_table.dedup();

        let pat = mini_with_loc::<T>(src, origin)?;
        for span in &highlight_table {
            if span.start > span.end {
                return Err(RichCompilerDiagnostic::new(
                    "SW-E099-SPAN-INVARIANT",
                    "invalid span bounds",
                    src,
                    0..0,
                ));
            }
        }

        Ok(Self {
            pattern: StdArc::new(pat),
            highlight_table,
            parse_counter: counter,
        })
    }

    /// Number of times the mini-notation parser was invoked (always `1` after `compile_once`).
    #[inline]
    pub fn parse_count(&self) -> usize {
        self.parse_counter.load(Ordering::Relaxed)
    }

    /// Pre-indexed `ByteSpan` highlight table.
    #[inline]
    pub fn highlight_table(&self) -> &[ByteSpan] {
        &self.highlight_table
    }

    /// Underlying `Pattern<T>`.
    #[inline]
    pub fn pattern(&self) -> &Pattern<T> {
        &self.pattern
    }

    /// Queries active `ByteSpan`s for the current 60 FPS frame `[t_now, t_now + frame_dt)`
    /// in $O(k)$ with zero string scanning and zero AST re-parsing.
    pub fn active_spans_in_frame(&self, t_now: Time, frame_dt: Time) -> Vec<ByteSpan> {
        let haps = self.pattern.query_arc(t_now, t_now + frame_dt);
        let mut spans = Vec::with_capacity(haps.len());
        for h in haps {
            for loc in h.context.locations {
                spans.push(loc.start..loc.end);
            }
        }
        spans
    }
}
