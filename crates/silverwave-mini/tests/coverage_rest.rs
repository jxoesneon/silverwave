//! Supplementary coverage tests for `silverwave-mini` (`v0.2.0`).
//!
//! Exercises previously-uncovered lines in:
//! - `ast.rs`: `TPat::collect_spans` traversal arms (`Polymeter`, `Elongate`, `TrigCond`,
//!   `Repeat`, `DegradeBy`, `EnumFromTo`, `Euclid`, `SampleIndex`)
//! - `diagnostics.rs`: `DelimiterKind` char tables, `DelimiterStack::pop_matching` empty-stack
//!   blame (`SW-E002`), `damerau_levenshtein` empty-input fast paths, `suggest_symbol` edges
//! - `span.rs`: `SourceSpan::from_byte_range` UTF-8 boundary clamping and the
//!   `SourceOrigin::unescape_with_origin` escape table
//! - `dsl.rs`: `IntoMiniPattern` impls, `Reset`/`Restart` partial-`whole` alignment arms,
//!   `ur`/`seq_p_loop`/`s_mini` edge cases
//! - `transpiler.rs`: `MiniStreamSession` stream bookkeeping, chained-method dispatch, and
//!   `split_method_chain`/`extract_mini_literal` error paths

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use silverwave_core::{
    Context, Hap, Pattern, Rational, State, Time, TimeSpan, TrigCondition, Value,
};
use silverwave_mini::{
    AlignmentMode, ByteSpan, DelimiterKind, DelimiterStack, IntoMiniPattern, MiniStreamSession,
    SourceOrigin, SourceSpan, TPat, align_binary_op, damerau_levenshtein, mini, n_mini, note_mini,
    s_mini, seq_p_loop, suggest_symbol, ur,
};

/// Collects all spans of a `TPat` tree in pre-order (helper for `collect_spans` assertions).
fn spans_of<T>(pat: &TPat<T>) -> Vec<ByteSpan> {
    let mut out = Vec::new();
    pat.collect_spans(&mut out);
    out
}

fn atom<T>(value: T, span: ByteSpan) -> TPat<T> {
    TPat::Atom { value, span }
}

#[test]
fn test_collect_spans_polymeter_elongate_trigcond() {
    // ast.rs 180-187: Polymeter walks every layer, then the optional step count pattern.
    let pm_steps: TPat<i32> = TPat::Polymeter {
        layers: vec![atom(1, 0..1), atom(2, 2..3)],
        steps: Some(Box::new(atom(Rational::from_integer(4), 5..6))),
        span: 0..7,
    };
    assert_eq!(spans_of(&pm_steps), vec![0..7, 0..1, 2..3, 5..6]);

    let pm_no_steps: TPat<i32> = TPat::Polymeter {
        layers: vec![atom(1, 0..1)],
        steps: None,
        span: 0..4,
    };
    assert_eq!(spans_of(&pm_no_steps), vec![0..4, 0..1]);

    // ast.rs 192-194: Elongate / TrigCond recurse into `inner` only.
    let elongate: TPat<i32> = TPat::Elongate {
        weight: Rational::new_const(3, 1),
        inner: Box::new(atom(7, 0..2)),
        span: 0..4,
    };
    assert_eq!(spans_of(&elongate), vec![0..4, 0..2]);

    let trig: TPat<i32> = TPat::TrigCond {
        cond: TrigCondition::FillLatch { active: true },
        inner: Box::new(atom(8, 0..2)),
        span: 0..6,
    };
    assert_eq!(spans_of(&trig), vec![0..6, 0..2]);
}

#[test]
fn test_collect_spans_repeat_degrade_enum() {
    // ast.rs 195-198: Repeat walks `inner` then the `count` sub-pattern.
    let repeat: TPat<i32> = TPat::Repeat {
        count: Box::new(atom(3usize, 3..4)),
        inner: Box::new(atom(9, 0..2)),
        span: 0..5,
    };
    assert_eq!(spans_of(&repeat), vec![0..5, 0..2, 3..4]);

    // ast.rs 199-202: DegradeBy walks `inner` then the `prob` sub-pattern.
    let degrade: TPat<i32> = TPat::DegradeBy {
        prob: Box::new(atom(0.25f64, 4..7)),
        seed_salt: 99,
        inner: Box::new(atom(10, 0..2)),
        span: 0..8,
    };
    assert_eq!(spans_of(&degrade), vec![0..8, 0..2, 4..7]);

    // ast.rs 203-211: EnumFromTo walks `start`, optional `step`, then `end`.
    let stepped: TPat<i32> = TPat::EnumFromTo {
        start: Box::new(atom(0, 0..1)),
        step: Some(Box::new(atom(2, 3..4))),
        end: Box::new(atom(6, 6..7)),
        span: 0..8,
    };
    assert_eq!(spans_of(&stepped), vec![0..8, 0..1, 3..4, 6..7]);

    let unstepped: TPat<i32> = TPat::EnumFromTo {
        start: Box::new(atom(0, 0..1)),
        step: None,
        end: Box::new(atom(3, 4..5)),
        span: 0..5,
    };
    assert_eq!(spans_of(&unstepped), vec![0..5, 0..1, 4..5]);
}

#[test]
fn test_collect_spans_euclid_sampleindex() {
    // ast.rs 212-225: Euclid walks `inner`, `pulses`, `steps`, then optional `offset`.
    let euclid_off: TPat<i32> = TPat::Euclid {
        pulses: Box::new(atom(3isize, 3..4)),
        steps: Box::new(atom(8isize, 5..6)),
        offset: Some(Box::new(atom(2isize, 7..8))),
        inner: Box::new(atom(1, 0..2)),
        span: 0..10,
    };
    assert_eq!(spans_of(&euclid_off), vec![0..10, 0..2, 3..4, 5..6, 7..8]);

    let euclid_no_off: TPat<i32> = TPat::Euclid {
        pulses: Box::new(atom(3isize, 3..4)),
        steps: Box::new(atom(8isize, 5..6)),
        offset: None,
        inner: Box::new(atom(1, 0..2)),
        span: 0..10,
    };
    assert_eq!(spans_of(&euclid_no_off), vec![0..10, 0..2, 3..4, 5..6]);

    // ast.rs 226-229: SampleIndex walks `target` then `index`.
    let sample: TPat<i32> = TPat::SampleIndex {
        target: Box::new(atom(5, 0..2)),
        index: Box::new(atom(3usize, 3..4)),
        span: 0..5,
    };
    assert_eq!(spans_of(&sample), vec![0..5, 0..2, 3..4]);
}

#[test]
fn test_delimiter_kind_tables_and_unexpected_close() {
    // diagnostics.rs 25-29, 37, 39-41: remaining open/close chars.
    assert_eq!(DelimiterKind::AngleBracket.open_char(), '<');
    assert_eq!(DelimiterKind::CurlyBrace.open_char(), '{');
    assert_eq!(DelimiterKind::Paren.open_char(), '(');
    assert_eq!(DelimiterKind::DoubleQuote.open_char(), '"');
    assert_eq!(DelimiterKind::Backtick.open_char(), '`');
    assert_eq!(DelimiterKind::AngleBracket.close_char(), '>');
    assert_eq!(DelimiterKind::Paren.close_char(), ')');
    assert_eq!(DelimiterKind::DoubleQuote.close_char(), '"');
    assert_eq!(DelimiterKind::Backtick.close_char(), '`');

    // diagnostics.rs 49-52: from_open for '<', '{', '(', and the `None` fallthrough.
    assert_eq!(
        DelimiterKind::from_open('<'),
        Some(DelimiterKind::AngleBracket)
    );
    assert_eq!(
        DelimiterKind::from_open('{'),
        Some(DelimiterKind::CurlyBrace)
    );
    assert_eq!(DelimiterKind::from_open('('), Some(DelimiterKind::Paren));
    assert_eq!(DelimiterKind::from_open(']'), None);
    assert_eq!(
        DelimiterKind::from_close('>'),
        Some(DelimiterKind::AngleBracket)
    );
    assert_eq!(
        DelimiterKind::from_close('}'),
        Some(DelimiterKind::CurlyBrace)
    );
    assert_eq!(DelimiterKind::from_close(')'), Some(DelimiterKind::Paren));
    assert_eq!(DelimiterKind::from_close('"'), None);

    // diagnostics.rs 126-137: pop on an empty stack blames the stray closer (SW-E002).
    let mut ds = DelimiterStack::new();
    let close_span = SourceSpan::from_byte_range("]", 0..1);
    let err = ds
        .pop_matching(DelimiterKind::SquareBracket, close_span)
        .unwrap_err();
    assert_eq!(err.error_code, "SW-E002-UNEXPECTED-CLOSING-DELIMITER");
    assert!(err.message.contains("unexpected closing ']'"));
    assert_eq!(err.primary_span, close_span);
    assert!(err.secondary_span.is_none());
    assert!(err.did_you_mean.is_none());
    assert!(ds.check_empty_at_eof(close_span).is_ok());
}

#[test]
fn test_damerau_levenshtein_and_suggest_symbol_edges() {
    // diagnostics.rs 329, 332: empty-input fast paths.
    assert_eq!(damerau_levenshtein("", "abc"), 3);
    assert_eq!(damerau_levenshtein("abc", ""), 3);
    assert_eq!(damerau_levenshtein("", ""), 0);

    // diagnostics.rs 371: a bare "." (or empty input) yields no suggestion.
    assert_eq!(suggest_symbol("."), None);
    assert_eq!(suggest_symbol(""), None);

    // diagnostics.rs 384-390: without a leading dot, the bare symbol is returned.
    assert_eq!(suggest_symbol("rom").as_deref(), Some("room"));
    assert_eq!(suggest_symbol(".rom").as_deref(), Some(".room"));

    // Nothing within edit distance <= 2 -> None.
    assert_eq!(suggest_symbol("xyzzy"), None);
}

#[test]
fn test_source_span_utf8_clamping() {
    // span.rs 53-55: a `start` landing inside a multi-byte char backs up to its boundary.
    // "aé" -> 'a' at [0,1), 'é' at [1,3).
    let clamped_start = SourceSpan::from_byte_range("aé", 2..3);
    assert_eq!((clamped_start.start, clamped_start.end), (1, 3));

    // span.rs 57-59: an `end` landing inside a multi-byte char extends to its boundary.
    let clamped_end = SourceSpan::from_byte_range("aé", 0..2);
    assert_eq!((clamped_end.start, clamped_end.end), (0, 3));

    // Out-of-range spans clamp to `source.len()`.
    let oob = SourceSpan::from_byte_range("ab", 5..99);
    assert_eq!((oob.start, oob.end), (2, 2));
}

#[test]
fn test_unescape_with_origin_full_escape_table() {
    // span.rs 140-147: \n, \t, \r, \", \', \`, \\, and unknown-escape passthrough.
    let (unescaped, origin) =
        SourceOrigin::unescape_with_origin(r#"x\ny\tz\rw\"v\'u\`t\\s\qr\"#, 40, 1);
    assert_eq!(unescaped, "x\ny\tz\rw\"v'u`t\\sqr\\");
    assert_eq!(origin.outer_content_offset, 41);
    // Each escape consumed one extra source byte; shifts are recorded per escape.
    // (The trailing lone '\' has no successor, so it is not recorded as a shift.)
    assert_eq!(origin.escape_shifts.len(), 8);
    assert_eq!(origin.escape_shifts[0].1, 1);
    assert_eq!(origin.escape_shifts[7].1, 8);
}

#[test]
fn test_into_mini_pattern_impls() {
    // dsl.rs 24-26: lifting a `Pattern<T>` returns it unchanged.
    let base: Pattern<i64> = "0 1".into_mini_pattern();
    let lifted: Pattern<i64> = base.clone().into_mini_pattern();
    assert_eq!(lifted.first_cycle().len(), 2);

    // dsl.rs 31-32: `&Pattern<T>` clones.
    let from_ref: Pattern<i64> = (&base).into_mini_pattern();
    assert_eq!(from_ref.first_cycle().len(), 2);

    // dsl.rs 45-47: `String` parses via `mini`, falling back to silence on error.
    let from_string: Pattern<i64> = String::from("2 3 4").into_mini_pattern();
    assert_eq!(from_string.first_cycle().len(), 3);
    let bad: Pattern<i64> = String::from("[").into_mini_pattern();
    assert!(bad.first_cycle().is_empty());
}

#[test]
fn test_alignment_reset_restart_partial_whole() {
    // dsl.rs 100, 128: `_ => l_hap.whole` arm when one side has `whole: None`
    // (a continuous `Hap` produced by `Hap::continuous`).
    let continuous_five: Pattern<i64> =
        Pattern::new(|state: &State| vec![Hap::continuous(state.span, 5, Context::empty())]);
    let trigger: Pattern<i64> = "10".into_mini_pattern();

    let reset = align_binary_op(&continuous_five, &trigger, AlignmentMode::Reset, |a, b| {
        a + b
    });
    let reset_haps = reset.first_cycle();
    assert_eq!(reset_haps.len(), 1);
    assert_eq!(reset_haps[0].value, 15);
    assert!(reset_haps[0].whole.is_none());

    let restart = align_binary_op(
        &continuous_five,
        &trigger,
        AlignmentMode::Restart,
        |a, b| a + b,
    );
    let restart_haps = restart.first_cycle();
    assert_eq!(restart_haps.len(), 1);
    assert_eq!(restart_haps[0].value, 15);
    assert!(restart_haps[0].whole.is_none());
}

#[test]
fn test_ur_and_seq_p_loop_edge_cases() {
    type Fx = BTreeMap<String, fn(&Pattern<String>) -> Pattern<String>>;
    let mut pats: BTreeMap<String, Pattern<String>> = BTreeMap::new();
    pats.insert("a".to_string(), mini::<String>("bd").unwrap());

    // dsl.rs 245-246: non-positive length -> silence.
    let fx: Fx = BTreeMap::new();
    let quiet = ur(
        Rational::zero(),
        &mini::<String>("a").unwrap(),
        pats.clone(),
        fx,
    );
    assert!(quiet.first_cycle().is_empty());

    // dsl.rs 256-257: unknown structure token -> silence for that slot.
    let song = ur(
        Rational::one(),
        &mini::<String>("a missing").unwrap(),
        pats.clone(),
        Fx::new(),
    );
    assert_eq!(song.first_cycle().len(), 1);
    assert_eq!(song.first_cycle()[0].value, "bd");

    // dsl.rs 259-264: `:fx` suffix names an fx key absent from the map -> base pattern.
    let nofx = ur(
        Rational::one(),
        &mini::<String>("a:nofx").unwrap(),
        pats,
        Fx::new(),
    );
    assert_eq!(nofx.first_cycle().len(), 1);

    // dsl.rs 277-278: empty schedule / non-positive max end -> silence.
    let empty: Pattern<String> = seq_p_loop(Vec::new());
    assert!(empty.first_cycle().is_empty());
    let zero_end = seq_p_loop(vec![(
        Rational::zero(),
        Rational::zero(),
        mini::<String>("bd").unwrap(),
    )]);
    assert!(zero_end.first_cycle().is_empty());
}

#[test]
fn test_s_mini_bank_index_branch() {
    // dsl.rs 288-292: `bank:idx` splits into "s" + parsed "n" control values.
    let haps = s_mini("bd:3 sd").first_cycle();
    assert_eq!(haps.len(), 2);
    assert_eq!(
        haps[0].value.get("s"),
        Some(&Value::String("bd".to_string()))
    );
    assert_eq!(haps[0].value.get("n"), Some(&Value::Int(3)));
    assert_eq!(
        haps[1].value.get("s"),
        Some(&Value::String("sd".to_string()))
    );
    assert!(!haps[1].value.contains_key("n"));
}

#[test]
fn test_transpiler_anonymous_and_retained_streams() {
    let mut session = MiniStreamSession::new();

    // transpiler.rs 83-85: a bare line (no `id:` prefix) becomes `anon_N`,
    // and the unquoted fallback in `extract_mini_literal` (335-341) handles it.
    let res = session.eval_script("bd sd");
    assert_eq!(res.updated_streams, vec!["anon_1".to_string()]);
    assert!(res.diagnostics.is_empty());
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);

    // transpiler.rs 110-112, 236-237: a stream that previously compiled retains
    // its pattern when a later eval hits an unknown-method diagnostic.
    let res = session.eval_script("d1: s(\"bd hh\")");
    assert_eq!(res.updated_streams, vec!["d1".to_string()]);
    let res = session.eval_script("d1: s(\"bd hh\").rom(0.5)");
    assert!(res.updated_streams.is_empty());
    assert_eq!(res.retained_previous_streams, vec!["d1".to_string()]);
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].error_code, "SW-E007-UNKNOWN-METHOD");
    assert_eq!(res.diagnostics[0].did_you_mean.as_deref(), Some(".room"));
    assert_eq!(res.diagnostics[0].isolated_stream_id.as_deref(), Some("d1"));
    // The previously-bound "bd hh" pattern is still active.
    let d1_haps = session.combined_pattern().first_cycle();
    assert!(
        d1_haps
            .iter()
            .any(|h| h.value.get("s") == Some(&Value::String("hh".to_string())))
    );
}

#[test]
fn test_transpiler_mute_solo_bookkeeping() {
    // transpiler.rs 147-170: mute/unmute/solo/unsolo on existing stream entries.
    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd\")\nd2: s(\"sd\")");

    session.mute("d1");
    assert_eq!(session.combined_pattern().first_cycle().len(), 1);
    session.unmute("d1");
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);

    session.solo("d1");
    let soloed = session.combined_pattern().first_cycle();
    assert_eq!(soloed.len(), 1);
    assert_eq!(
        soloed[0].value.get("s"),
        Some(&Value::String("bd".to_string()))
    );
    session.unsolo("d1");
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);

    // Muting/soloing unknown IDs is a no-op.
    session.mute("nope");
    session.unmute("nope");
    session.solo("nope");
    session.unsolo("nope");
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);
}

#[test]
fn test_transpiler_method_chain_dispatch() {
    // transpiler.rs 251-275: the chained-method dispatch table.
    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd sd\").fast(2)");
    assert_eq!(session.combined_pattern().first_cycle().len(), 4);

    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd sd\").slow(2)");
    let haps = session
        .combined_pattern()
        .query_arc(Time::zero(), Time::from_integer(2));
    assert_eq!(haps.len(), 2);
    assert_eq!(
        session.combined_pattern().first_cycle().len(),
        1,
        "slow(2) stretches 'bd sd' across two cycles; only 'bd' lands in cycle 0"
    );

    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd sd\").rev()");
    let haps = session.combined_pattern().first_cycle();
    assert_eq!(haps.len(), 2);
    let mut values: Vec<String> = haps
        .iter()
        .map(|h| match h.value.get("s") {
            Some(Value::String(s)) => s.clone(),
            other => panic!("expected s-control string, got {other:?}"),
        })
        .collect();
    values.sort();
    assert_eq!(values, vec!["bd".to_string(), "sd".to_string()]);

    // Chained control setters (`.room`/`.lpf`) insert float controls per hap.
    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd\").room(0.2).lpf(900)");
    let haps = session.combined_pattern().first_cycle();
    assert_eq!(haps.len(), 1);
    assert_eq!(haps[0].value.get("room"), Some(&Value::Float(0.2)));
    assert_eq!(haps[0].value.get("lpf"), Some(&Value::Float(900.0)));

    // `.gain` covers the same arm; `.note` is a known symbol hitting `_ => {}`.
    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd\").gain(0.5)");
    assert_eq!(
        session.combined_pattern().first_cycle()[0]
            .value
            .get("gain"),
        Some(&Value::Float(0.5))
    );
    let res = session.eval_script("d2: s(\"sd\").note(3)");
    assert_eq!(res.updated_streams, vec!["d2".to_string()]);
    assert!(res.diagnostics.is_empty());
}

#[test]
fn test_transpiler_literal_and_chain_error_paths() {
    let mut session = MiniStreamSession::new();

    // transpiler.rs 319-326: unclosed quoted literal -> SW-E003 diagnostic.
    let res = session.eval_script("d1: s(\"bd");
    assert!(res.updated_streams.is_empty());
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(
        res.diagnostics[0].error_code,
        "SW-E003-UNCLOSED-OPENING-DELIMITER"
    );

    // Backtick-delimited literals take the second iteration of the quote loop (316-333).
    let res = session.eval_script("d1: s(`oh`)");
    assert_eq!(res.updated_streams, vec!["d1".to_string()]);

    // transpiler.rs 304-306: `.method` tails without a closing `)` break the
    // chain scan without producing a validated call.
    let res = session.eval_script("d2: s(\"bd\").fast(");
    assert_eq!(res.updated_streams, vec!["d2".to_string()]);
    let res = session.eval_script("d3: s(\"sd\").rev");
    assert_eq!(res.updated_streams, vec!["d3".to_string()]);

    // A quoted literal whose inner mini-notation is invalid propagates the
    // `compile_once` error through `?` (transpiler.rs 241-242).
    let res = session.eval_script("d4: s(\"bd [\")");
    assert_eq!(res.diagnostics.len(), 1);
    assert!(res.updated_streams.is_empty());
}

#[test]
fn test_alignment_reset_restart_full_whole_and_miss() {
    // dsl.rs `(Some(wr), Some(wl)) => wr.intersection(wl)` arms: BOTH haps must
    // carry `whole` (`test_alignment_reset_restart_partial_whole` only covers
    // the `_ => l_hap.whole` fallback via a `Hap::continuous` lhs). A second,
    // part-disjoint lhs hap covers the `intersection -> None` skip edge.
    let whole_cycle = TimeSpan::cycle_of(Time::zero());
    let off_cycle = TimeSpan::new_clamped(Time::from_integer(5), Time::from_integer(6));
    let lhs: Pattern<i64> = Pattern::new(move |_state: &State| {
        vec![
            Hap::discrete(whole_cycle, whole_cycle, 5, Context::empty()),
            Hap::discrete(off_cycle, off_cycle, 7, Context::empty()),
        ]
    });
    let trigger: Pattern<i64> = "10".into_mini_pattern();

    for mode in [AlignmentMode::Reset, AlignmentMode::Restart] {
        let pat = align_binary_op(&lhs, &trigger, mode, |a, b| a + b);
        let haps = pat.first_cycle();
        assert_eq!(haps.len(), 1, "mode {mode:?} should emit a single hap");
        assert_eq!(haps[0].value, 15);
        // Both sides carried `whole`, so the result whole is their intersection.
        assert_eq!(haps[0].whole, Some(whole_cycle));
    }
}

#[test]
fn test_s_mini_index_parse_failure_and_mini_helpers() {
    // dsl.rs: a `bank:tag` whose index is not an integer skips the `n` insert
    // (the `Ok(idx)` edge is covered by `test_s_mini_bank_index_branch`).
    let haps = s_mini("bd:zz").first_cycle();
    assert_eq!(haps.len(), 1);
    assert_eq!(
        haps[0].value.get("s"),
        Some(&Value::String("bd".to_string()))
    );
    assert!(!haps[0].value.contains_key("n"));

    // `note_mini`/`n_mini` fmap bodies only execute when the pattern is queried.
    let note_haps = note_mini("c4").first_cycle();
    assert_eq!(note_haps.len(), 1);
    assert!(matches!(
        note_haps[0].value.get("note"),
        Some(Value::Note(_))
    ));

    let n_haps = n_mini("3").first_cycle();
    assert_eq!(n_haps.len(), 1);
    assert_eq!(n_haps[0].value.get("n"), Some(&Value::Float(3.0)));
}

#[test]
fn test_transpiler_chain_bad_args_are_ignored() {
    // transpiler.rs: chained args that fail numeric parsing leave the pattern
    // untouched (the `if let Ok(..)` failure edge for `fast`/`slow`/setters).
    let mut session = MiniStreamSession::new();
    let res = session.eval_script("d1: s(\"bd sd\").fast(xx)");
    assert_eq!(res.updated_streams, vec!["d1".to_string()]);
    assert!(res.diagnostics.is_empty());
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);

    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd sd\").slow(1.5)");
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);

    let mut session = MiniStreamSession::new();
    session.eval_script("d1: s(\"bd\").room(huge).gain(na).hpf(x).pan(left)");
    let haps = session.combined_pattern().first_cycle();
    assert_eq!(haps.len(), 1);
    for key in ["room", "gain", "hpf", "pan"] {
        assert!(!haps[0].value.contains_key(key), "{key} should be unset");
    }
}

#[test]
fn test_into_mini_pattern_str_parse_failure() {
    // dsl.rs 39: a `&str` that fails to parse falls back to `silence()` via
    // `unwrap_or_else`.
    let bad: Pattern<i64> = "[".into_mini_pattern();
    assert!(bad.first_cycle().is_empty());
}

#[test]
fn test_mini_helper_parse_failures_fall_back_to_silence() {
    // dsl.rs 285, 302, 312: `unwrap_or_else(|_| silence())` fallback arms of
    // `s_mini`/`note_mini`/`n_mini` when the mini-notation cannot be parsed.
    assert!(s_mini("[").first_cycle().is_empty());
    assert!(note_mini("[").first_cycle().is_empty());
    assert!(n_mini("[").first_cycle().is_empty());
}

#[test]
fn test_suggest_symbol_improves_best_candidate() {
    // diagnostics.rs 421: a later candidate strictly improves the best-so-far
    // distance ("s" is distance 2 from "fas"; "fast" is distance 1).
    assert_eq!(suggest_symbol("fas").as_deref(), Some("fast"));
}

#[test]
fn test_collect_spans_items_and_factor_arms() {
    // ast.rs 171-178: the `items` arm also serves Stack/Slowcat/Choose/
    // MondoCall; ast.rs 188-191: Fast/Slow walk `inner` then `factor`.
    // A single traversal that exercises every remaining `collect_spans` arm.
    let rich: TPat<i32> = TPat::Seq {
        items: vec![
            TPat::Stack {
                layers: vec![atom(1, 0..1), atom(2, 1..2)],
                span: 0..3,
            },
            TPat::Slowcat {
                items: vec![atom(3, 0..1)],
                span: 0..2,
            },
            TPat::Choose {
                options: vec![atom(4, 0..1), atom(5, 1..2)],
                span: 0..3,
            },
            TPat::MondoCall {
                func: "fx".to_string(),
                args: vec![atom(6, 2..3)],
                span: 0..4,
            },
            TPat::Fast {
                factor: Box::new(atom(Rational::from_integer(2), 1..2)),
                inner: Box::new(atom(7, 0..1)),
                span: 0..3,
            },
            TPat::Slow {
                factor: Box::new(atom(Rational::from_integer(2), 1..2)),
                inner: Box::new(atom(8, 0..1)),
                span: 0..3,
            },
        ],
        span: 0..20,
    };
    assert_eq!(
        spans_of(&rich),
        vec![
            0..20,
            0..3,
            0..1,
            1..2,
            0..2,
            0..1,
            0..3,
            0..1,
            1..2,
            0..4,
            2..3,
            0..3,
            0..1,
            1..2,
            0..3,
            0..1,
            1..2,
        ]
    );
}

#[test]
fn test_align_binary_op_all_modes_with_whole_edges() {
    // dsl.rs 63-82: every alignment-mode arm of `align_binary_op` under a
    // single `op`; dsl.rs 99-100/127-128: the Reset/Restart inner-closure
    // arms — `(Some, Some)` intersection, the `_ => l_hap.whole` fallback
    // (a `Hap::continuous` lhs hap with `whole: None`), and the part-disjoint
    // intersection skip.
    let whole_cycle = TimeSpan::cycle_of(Time::zero());
    let off_cycle = TimeSpan::new_clamped(Time::from_integer(5), Time::from_integer(6));
    let lhs: Pattern<i64> = Pattern::new(move |state: &State| {
        vec![
            Hap::discrete(whole_cycle, whole_cycle, 5, Context::empty()),
            Hap::continuous(state.span, 7, Context::empty()),
            Hap::discrete(off_cycle, off_cycle, 11, Context::empty()),
        ]
    });
    let trigger: Pattern<i64> = "10".into_mini_pattern();
    let op = |a: &i64, b: &i64| a + b;
    for mode in [
        AlignmentMode::In,
        AlignmentMode::Out,
        AlignmentMode::Mix,
        AlignmentMode::Squeeze,
        AlignmentMode::SqueezeOut,
        AlignmentMode::Reset,
        AlignmentMode::Restart,
    ] {
        let haps = align_binary_op(&lhs, &trigger, mode, op).first_cycle();
        assert!(!haps.is_empty(), "mode {mode:?} produced no haps");
    }
}

#[test]
fn test_ur_fx_transform_hit_and_seq_p_loop_positive() {
    type Fx = BTreeMap<String, fn(&Pattern<String>) -> Pattern<String>>;
    // dsl.rs 259-264: a `token:fx` whose fx key exists applies the transform
    // (mixed with an unknown token and a plain token in one structure).
    let mut pats: BTreeMap<String, Pattern<String>> = BTreeMap::new();
    pats.insert("a".to_string(), mini::<String>("bd").unwrap());
    let mut fx: Fx = BTreeMap::new();
    fx.insert("fx".to_string(), |p: &Pattern<String>| p.clone());
    let song = ur(
        Rational::one(),
        &mini::<String>("a:fx missing a").unwrap(),
        pats,
        fx,
    );
    let haps = song.first_cycle();
    assert_eq!(haps.len(), 2);
    assert!(haps.iter().all(|h| h.value == "bd"));

    // dsl.rs 279-280: a positive max end schedules the entries on the ribbon.
    let looped = seq_p_loop(vec![(
        Rational::zero(),
        Rational::one(),
        mini::<String>("bd sd").unwrap(),
    )]);
    assert_eq!(looped.first_cycle().len(), 2);
}
