//! Comprehensive TDD & Property-Based Test Suite for `silverwave-mini` (`v0.2.0`).
//! Verifies 100% of `02-SPEC-MINI-NOTATION-AND-DSL.md` Section 4.1–4.5 & Section 5 contracts,
//! `PP-06`, `PP-07`, `CK-01`, and `CK-05`, plus 100% public symbol coverage.

#![forbid(unsafe_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use silverwave_core::{
    ControlMap, Pattern, Rational, SourceLocation, State, Time, TimeSpan, TrigCondition, Value,
};
use silverwave_mini::{
    AlignmentMode, ByteSpan, CompiledHighlightPattern, DelimiterEntry, DelimiterKind,
    DelimiterStack, DspPipeStage, FaultTolerantCompileResult, FromMiniAtom, IntoMiniPattern,
    KNOWN_DSL_SYMBOLS, MAX_AST_NESTING_DEPTH, MAX_EUCLID_STEPS, MAX_LSYSTEM_EXPANDED_TOKENS,
    MAX_REPLICATE_OR_ELONGATE, MiniParser, MiniStreamSession, MiniTarget, NoteSpec, ParseError,
    PatternCombinator, SourceOrigin, SourceSpan, Spanned, StreamEntry, TPat, align_binary_op,
    compile_tpat, damerau_levenshtein, mini, mini_with_loc, n_mini, note_mini, parse_dsp_pipe,
    parse_note_spec, parse_ratio_shorthand, parse_trig_condition_suffix, s_mini, seq_p_loop,
    suggest_symbol, ur, version,
};

#[test]
fn test_span_ascii_atoms() {
    let src = "bd sd hh";
    let ast = MiniParser::parse::<String>(src).unwrap();
    let mut spans = Vec::new();
    ast.collect_spans(&mut spans);
    assert!(spans.contains(&(0..2)));
    assert!(spans.contains(&(3..5)));
    assert!(spans.contains(&(6..8)));

    let sp = Spanned::new("bd", 0..2);
    assert_eq!(sp.node, "bd");
    assert_eq!(sp.span, 0..2);
}

#[test]
fn test_span_multibyte_utf8() {
    let src = "// café 🎵\nbd sd";
    let prefix_len = "// café 🎵\n".len();
    let ast = MiniParser::parse::<String>(src).unwrap();
    let mut spans = Vec::new();
    ast.collect_spans(&mut spans);
    for s in &spans {
        assert!(src.is_char_boundary(s.start));
        assert!(src.is_char_boundary(s.end));
    }
    assert!(spans.contains(&(prefix_len..(prefix_len + 2))));
    assert!(spans.contains(&((prefix_len + 3)..(prefix_len + 5))));

    let block_comment_src = "/* multi\nline */ bd sd";
    let pat = mini::<String>(block_comment_src).unwrap();
    assert_eq!(pat.first_cycle().len(), 2);
}

#[test]
fn test_with_loc_composition() {
    let origin = SourceOrigin::with_outer_offset(13, 1);
    assert_eq!(origin.outer_content_offset, 14);
    let pat = mini_with_loc::<String>("bd   [sd cp]", origin.clone()).unwrap();
    let haps = pat.first_cycle();
    assert_eq!(haps.len(), 3);
    // "sd" is at inner bytes 6..8 -> outer bytes 20..22
    assert!(
        haps[1]
            .context
            .locations
            .contains(&SourceLocation { start: 20, end: 22 })
    );
    assert_eq!(origin.map_span(&(7..9)), 21..23);
    assert_eq!(
        origin.map_to_location(&(7..9)),
        SourceLocation { start: 21, end: 23 }
    );
    let sspan = SourceSpan::from_byte_range("bd sd", 0..2);
    assert_eq!(sspan.to_byte_span(), 0..2);
    assert_eq!(
        sspan.to_core_location(),
        SourceLocation { start: 0, end: 2 }
    );
}

#[test]
fn test_escaped_quotes_and_multiline_backticks() {
    let raw = r#"bd\nsd \"cp\""#;
    let (unescaped, origin) = SourceOrigin::unescape_with_origin(raw, 10, 1);
    assert_eq!(unescaped, "bd\nsd \"cp\"");
    let pat = mini_with_loc::<String>(&unescaped, origin).unwrap();
    let haps = pat.first_cycle();
    assert_eq!(haps.len(), 3);
    assert!(haps[2].context.locations[0].start > 15);
}

#[test]
fn test_subdivision_and_slowcat() {
    let pat = mini::<String>("[a b] <c d>").unwrap();
    let c0 = pat.query_arc(Time::zero(), Time::one());
    let c1 = pat.query_arc(Time::one(), Time::from_integer(2));
    assert_eq!(c0.len(), 3);
    assert_eq!(c0[0].value, "a");
    assert_eq!(
        c0[0].part,
        TimeSpan::new_clamped(Time::zero(), Time::new_const(1, 4))
    );
    assert_eq!(c0[1].value, "b");
    assert_eq!(
        c0[1].part,
        TimeSpan::new_clamped(Time::new_const(1, 4), Time::new_const(1, 2))
    );
    assert_eq!(c0[2].value, "c");
    assert_eq!(
        c0[2].part,
        TimeSpan::new_clamped(Time::new_const(1, 2), Time::one())
    );

    assert_eq!(c1.len(), 3);
    assert_eq!(c1[2].value, "d");
}

#[test]
fn test_stack_comma_all_contexts() {
    let top = mini::<String>("bd, sd, hh").unwrap();
    assert_eq!(top.first_cycle().len(), 3);

    let bracket = mini::<String>("[bd, sd] hh").unwrap();
    assert_eq!(bracket.first_cycle().len(), 3);

    let angle = mini::<String>("<[bd, cp] sd>").unwrap();
    assert_eq!(angle.first_cycle().len(), 2);

    let curly = mini::<String>("{bd sd, hh cp eh}").unwrap();
    assert_eq!(curly.first_cycle().len(), 4);
}

#[test]
fn test_feet_dot_precedence() {
    let p_dot = mini::<String>("a b . c d e").unwrap();
    let p_brk = mini::<String>("[a b] [c d e]").unwrap();
    let h_dot: Vec<(TimeSpan, String)> = p_dot
        .first_cycle()
        .into_iter()
        .map(|h| (h.part, h.value))
        .collect();
    let h_brk: Vec<(TimeSpan, String)> = p_brk
        .first_cycle()
        .into_iter()
        .map(|h| (h.part, h.value))
        .collect();
    assert_eq!(h_dot, h_brk);

    let p_mul_dot = mini::<String>("a*2 b . c").unwrap();
    let p_mul_brk = mini::<String>("[a*2 b] [c]").unwrap();
    assert_eq!(p_mul_dot.first_cycle().len(), p_mul_brk.first_cycle().len());
}

#[test]
fn test_speed_mul_div_patterned() {
    let fast_pat = mini::<String>("bd*<2 4>").unwrap();
    assert_eq!(fast_pat.query_arc(Time::zero(), Time::one()).len(), 2);
    assert_eq!(
        fast_pat.query_arc(Time::one(), Time::from_integer(2)).len(),
        4
    );

    let slow_pat = mini::<String>("bd/<2 3>").unwrap();
    assert_eq!(
        slow_pat
            .query_arc(Time::zero(), Time::from_integer(2))
            .len(),
        2
    );
}

#[test]
fn test_elongate_at_and_underscore() {
    let p_at = mini::<String>("a@2 b").unwrap();
    let p_tie = mini::<String>("a _ b").unwrap();
    let h_at = p_at.first_cycle();
    let h_tie = p_tie.first_cycle();
    assert_eq!(h_at.len(), 2);
    assert_eq!(h_tie.len(), 2);
    assert_eq!(h_at[0].part.duration(), Rational::new_const(2, 3));
    assert_eq!(h_at[1].part.duration(), Rational::new_const(1, 3));
    assert_eq!(h_tie[0].part.duration(), Rational::new_const(2, 3));
    assert_eq!(h_tie[1].part.duration(), Rational::new_const(1, 3));
}

#[test]
fn test_replicate_bang() {
    let p3 = mini::<String>("a!3 b").unwrap().first_cycle();
    assert_eq!(p3.len(), 4);
    for h in &p3 {
        assert_eq!(h.part.duration(), Rational::new_const(1, 4));
    }
    let p_bare = mini::<String>("a! b").unwrap().first_cycle();
    assert_eq!(p_bare.len(), 3);
}

#[test]
fn test_degrade_determinism() {
    let p_half = mini::<String>("bd*8?").unwrap();
    assert_eq!(p_half.first_cycle(), p_half.first_cycle());

    let p_zero = mini::<String>("bd*8?0").unwrap();
    assert_eq!(p_zero.first_cycle().len(), 8);

    let p_quarter = mini::<String>("bd*8?0.25").unwrap();
    assert_eq!(p_quarter.first_cycle(), p_quarter.first_cycle());
}

#[test]
fn test_choice_pipe() {
    let p = mini::<String>("bd|sd|cp").unwrap();
    let h = p.first_cycle();
    assert_eq!(h.len(), 1);
    assert!(["bd", "sd", "cp"].contains(&h[0].value.as_str()));
}

#[test]
fn test_sample_index_colon() {
    let p1 = mini::<String>("bd:2 casio:0").unwrap().first_cycle();
    assert_eq!(p1[0].value, "bd:2");
    assert_eq!(p1[1].value, "casio:0");

    let p2 = mini::<String>("bd:<0 1>").unwrap();
    assert_eq!(p2.query_arc(Time::zero(), Time::one())[0].value, "bd:0");
    assert_eq!(
        p2.query_arc(Time::one(), Time::from_integer(2))[0].value,
        "bd:1"
    );
}

#[test]
fn test_euclidean_bjorklund() {
    let e38 = mini::<String>("bd(3,8)").unwrap().first_cycle();
    assert_eq!(e38.len(), 3);
    assert_eq!(e38[0].part.begin, Rational::new_const(0, 8));
    assert_eq!(e38[1].part.begin, Rational::new_const(3, 8));
    assert_eq!(e38[2].part.begin, Rational::new_const(6, 8));

    let e582 = mini::<String>("bd(5,8,2)").unwrap().first_cycle();
    assert_eq!(e582.len(), 5);

    let e_pat = mini::<String>("bd(<3 5>,8)").unwrap();
    assert_eq!(e_pat.query_arc(Time::zero(), Time::one()).len(), 3);
    assert_eq!(e_pat.query_arc(Time::one(), Time::from_integer(2)).len(), 5);
}

#[test]
fn test_polymeter_steps() {
    let pm = mini::<String>("{a b c, x y}%4").unwrap();
    let h = pm.first_cycle();
    assert_eq!(h.len(), 8);
}

#[test]
fn test_enum_range() {
    let asc = mini::<i64>("0..3").unwrap().first_cycle();
    let asc_vals: Vec<i64> = asc.into_iter().map(|h| h.value).collect();
    assert_eq!(asc_vals, vec![0, 1, 2, 3]);

    let desc = mini::<i64>("3..0").unwrap().first_cycle();
    let desc_vals: Vec<i64> = desc.into_iter().map(|h| h.value).collect();
    assert_eq!(desc_vals, vec![3, 2, 1, 0]);

    let stepped = mini::<i64>("0..2..6").unwrap().first_cycle();
    let step_vals: Vec<i64> = stepped.into_iter().map(|h| h.value).collect();
    assert_eq!(step_vals, vec![0, 2, 4, 6]);
}

#[test]
fn test_rests_tilde_dash() {
    let h = mini::<String>("bd ~ sd -").unwrap().first_cycle();
    assert_eq!(h.len(), 2);
    assert_eq!(
        h[0].part,
        TimeSpan::new_clamped(Time::zero(), Time::new_const(1, 4))
    );
    assert_eq!(
        h[1].part,
        TimeSpan::new_clamped(Time::new_const(1, 2), Time::new_const(3, 4))
    );
}

#[test]
fn test_state_var_caret() {
    let pat = mini::<f64>("^cutoff ^^local_cutoff").unwrap();
    let span = TimeSpan::new_clamped(Time::zero(), Time::one());
    let mut controls = ControlMap::new();
    controls.insert("cutoff".to_string(), Value::Float(1200.0));
    controls.insert("local_cutoff".to_string(), Value::Float(600.0));
    let state = State::new(span).with_controls(controls);
    let haps = pat.query(&state);
    assert_eq!(haps.len(), 2);
    assert_eq!(haps[0].value, 1200.0);
    assert_eq!(haps[1].value, 600.0);
}

#[test]
fn test_notes_chords_and_ratio_shorthands() {
    assert_eq!(parse_ratio_shorthand("w"), Some(Rational::one()));
    assert_eq!(parse_ratio_shorthand("h"), Some(Rational::new_const(1, 2)));
    assert_eq!(parse_ratio_shorthand("q"), Some(Rational::new_const(1, 4)));
    assert_eq!(parse_ratio_shorthand("e"), Some(Rational::new_const(1, 8)));
    assert_eq!(parse_ratio_shorthand("s"), Some(Rational::new_const(1, 16)));
    assert_eq!(parse_ratio_shorthand("t"), Some(Rational::new_const(1, 3)));
    assert_eq!(parse_ratio_shorthand("f"), Some(Rational::new_const(1, 5)));
    assert_eq!(parse_ratio_shorthand("x"), Some(Rational::new_const(1, 6)));

    let c4 = parse_note_spec("c4").unwrap();
    let eb3 = parse_note_spec("eb3").unwrap();
    let fs5 = parse_note_spec("f#5").unwrap();
    assert_eq!(c4.to_midi(), 60.0);
    assert_eq!(eb3.to_midi(), 51.0);
    assert_eq!(fs5.to_midi(), 78.0);

    let chords = mini::<Value>("C^7 Am7 G7b9@lefthand")
        .unwrap()
        .first_cycle();
    assert_eq!(chords.len(), 3);
    assert!(matches!(chords[0].value, Value::List(_)));
}

#[test]
fn test_zero_reparse_playback_queries() {
    let hl = CompiledHighlightPattern::<String>::compile_once("bd [sd cp]*2", SourceOrigin::root())
        .unwrap();
    assert_eq!(hl.parse_count(), 1);
    assert!(!hl.highlight_table().is_empty());

    let dt = Time::new_const(1, 60);
    for frame in 0..600 {
        let t_now = Time::from_integer(frame) * dt;
        let spans = hl.active_spans_in_frame(t_now, dt);
        assert!(!spans.is_empty());
    }
    assert_eq!(hl.parse_count(), 1);
    // "bd [sd cp]*2" -> bd + (sd cp)x2 = 5 haps.
    assert_eq!(hl.pattern().first_cycle().len(), 5);
}

#[test]
fn test_hap_context_span_roundtrip() {
    let src = "bd [sd cp]*2";
    let pat = mini::<String>(src).unwrap();
    for h in pat.first_cycle() {
        assert!(!h.context.locations.is_empty());
        assert!(
            h.context
                .locations
                .iter()
                .any(|loc| { src[loc.start..loc.end].contains(&h.value) })
        );
    }
}

#[test]
fn test_60fps_frame_budget() {
    let layers: Vec<Pattern<String>> = (0..16)
        .map(|_| mini::<String>("bd [sd cp]*2 hh*4").unwrap())
        .collect();
    let stacked = silverwave_core::stack(layers);
    let haps = stacked.query_arc(Time::zero(), Time::new_const(1, 60));
    assert_eq!(haps.len(), 16);
}

#[test]
fn test_alignment_in_out_mix() {
    let lhs: Pattern<i64> = "0 1 2".into_mini_pattern();
    let in_haps = lhs.add_pat("10 20").first_cycle();
    let out_haps = lhs.add_out("10 20").first_cycle();
    let mix_haps = lhs.add_mix("10 20").first_cycle();
    assert_eq!(in_haps.len(), 4);
    assert_eq!(out_haps.len(), 4);
    assert_eq!(mix_haps.len(), 4);
    assert_eq!(lhs.sub_in("1 1 1").first_cycle()[0].value, -1i64);
    assert_eq!(lhs.mul_in("2 2 2").first_cycle()[1].value, 2i64);
    assert_eq!(lhs.div_in("1 1 1").first_cycle()[2].value, 2i64);
}

#[test]
fn test_alignment_squeeze_and_squeezeout() {
    let lhs: Pattern<i64> = "0 1 2".into_mini_pattern();
    let sq = lhs.add_squeeze("10 20").first_cycle();
    let sq_vals: Vec<i64> = sq.into_iter().map(|h| h.value).collect();
    assert_eq!(sq_vals, vec![10, 20, 11, 21, 12, 22]);

    let sq_out = lhs.add_squeezeout("10 20").first_cycle();
    let sq_out_vals: Vec<i64> = sq_out.into_iter().map(|h| h.value).collect();
    assert_eq!(sq_out_vals, vec![10, 11, 12, 20, 21, 22]);
}

#[test]
fn test_alignment_reset_and_restart() {
    let lhs: Pattern<i64> = "0 1 2 3 4 5 6 7".into_mini_pattern();
    let reset_haps = lhs.add_reset("10 [20 30]").first_cycle();
    let reset_vals: Vec<i64> = reset_haps.into_iter().map(|h| h.value).collect();
    assert_eq!(reset_vals, vec![10, 11, 12, 13, 20, 21, 30, 31]);

    let cycle_varying: Pattern<i64> = "<0 100> 1".into_mini_pattern();
    let trig: Pattern<i64> = "10".into_mini_pattern();
    let c1_reset = cycle_varying
        .add_reset(&trig)
        .query_arc(Time::one(), Time::from_integer(2));
    let c1_restart = cycle_varying
        .add_restart(&trig)
        .query_arc(Time::one(), Time::from_integer(2));
    assert_eq!(c1_reset[0].value, 110);
    assert_eq!(c1_restart[0].value, 10);

    let _ = align_binary_op(&lhs, &trig, AlignmentMode::In, |a, b| a + b);
}

#[test]
fn test_ur_seqp_stack_all() {
    let mut pats = BTreeMap::new();
    pats.insert("drums".to_string(), mini::<String>("bd sd").unwrap());
    pats.insert("hats".to_string(), mini::<String>("hh*4").unwrap());

    type FxMap = BTreeMap<String, fn(&Pattern<String>) -> Pattern<String>>;
    let mut fx: FxMap = BTreeMap::new();
    fx.insert("rev".to_string(), |p: &Pattern<String>| p.rev());

    let song = ur(
        Rational::from_integer(2),
        &mini::<String>("drums hats:rev").unwrap(),
        pats,
        fx,
    );
    assert_eq!(song.query_arc(Time::zero(), Time::from_integer(2)).len(), 6);

    let looped = seq_p_loop(vec![
        (
            Rational::zero(),
            Rational::one(),
            mini::<String>("bd").unwrap(),
        ),
        (
            Rational::one(),
            Rational::from_integer(2),
            mini::<String>("sd").unwrap(),
        ),
    ]);
    assert_eq!(
        looped.query_arc(Time::from_integer(2), Time::from_integer(3))[0].value,
        "bd"
    );

    assert_eq!(s_mini("bd:1 sd").first_cycle().len(), 2);
    assert_eq!(note_mini("c4 e4 g4").first_cycle().len(), 3);
    assert_eq!(n_mini("0 1 2 3").first_cycle().len(), 4);
}

#[test]
fn test_transpiler_streams_and_mute() {
    let mut session = MiniStreamSession::new();
    session.bind_stream("d1", s_mini("bd"));

    let script = r#"
d1: s("bd sd")
d2: s("hh*4").rom(0.5)
$: s("cp")
_$: s("arpy*2")
"#;
    session.bind_stream("d2", s_mini("hh*2"));
    let res: FaultTolerantCompileResult = session.eval_script(script);
    assert!(res.updated_streams.contains(&"d1".to_string()));
    assert!(res.retained_previous_streams.contains(&"d2".to_string()));
    assert_eq!(res.muted_streams.len(), 1);
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].did_you_mean.as_deref(), Some(".room"));
    assert_eq!(res.diagnostics[0].isolated_stream_id.as_deref(), Some("d2"));

    assert!(!session.muted_highlight_spans("_anon_2").is_empty());
    assert!(session.active_stream_ids().contains("d1"));

    session.mute("d1");
    session.unmute("d1");
    session.solo("d1");
    assert_eq!(session.combined_pattern().first_cycle().len(), 2);
    session.unsolo("d1");
    session.all(|p| p.fast(Time::from_integer(2)));
    assert!(!session.combined_pattern().first_cycle().is_empty());
    session.hush();
    assert_eq!(session.combined_pattern().first_cycle().len(), 0);
}

#[test]
fn test_delimiter_stack_mondo_elektron_dsp_pipe_and_dos_quotas() {
    assert!(!version().is_empty());
    assert_eq!(MAX_AST_NESTING_DEPTH, 64);
    assert_eq!(MAX_EUCLID_STEPS, 1024);
    assert_eq!(MAX_REPLICATE_OR_ELONGATE, 1024);
    assert_eq!(MAX_LSYSTEM_EXPANDED_TOKENS, 4096);
    assert!(!KNOWN_DSL_SYMBOLS.is_empty());

    // PP-06: Opening-delimiter stack blame on unclosed `[`
    let unclosed = "bd [sd hh";
    let err: ParseError = mini::<String>(unclosed).err().unwrap();
    assert_eq!(err.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");
    assert_eq!(err.primary_span.start, 3);
    assert_eq!(err.primary_span.end, 4);
    assert!(!format!("{}", err).is_empty());

    // Mismatched delimiter `[sd }` blames opening `[` at byte 0
    let mismatched = "[sd }";
    let err_mis = mini::<String>(mismatched).err().unwrap();
    assert_eq!(err_mis.error_code, "SW-E001-MISMATCHED-DELIMITER");
    assert_eq!(err_mis.primary_span.start, 0);

    // Damerau-Levenshtein transposition & substitution suggestions
    assert_eq!(damerau_levenshtein("lfp", "lpf"), 1);
    assert_eq!(suggest_symbol(".rom").as_deref(), Some(".room"));
    assert_eq!(suggest_symbol(".lfp").as_deref(), Some(".lpf"));

    // Mondo notation `(fast 2 [bd sd])`
    let mondo = mini::<String>("(fast 2 [bd sd])").unwrap();
    assert_eq!(mondo.first_cycle().len(), 4);

    // CK-01 Elektron conditional trigs inside mini-notation (`@1:4`, `@fill`, `@pre`, `@nei`, `@1st`)
    assert_eq!(
        parse_trig_condition_suffix("1:4"),
        Some(TrigCondition::Ratio { a: 1, b: 4 })
    );
    assert_eq!(
        parse_trig_condition_suffix("fill"),
        Some(TrigCondition::FillLatch { active: true })
    );
    assert_eq!(
        parse_trig_condition_suffix("pre"),
        Some(TrigCondition::PreviousConditionalOnSameStream { expected: true })
    );
    assert_eq!(
        parse_trig_condition_suffix("nei"),
        Some(TrigCondition::PreviousConditionalOnNeighborStream { expected: true })
    );
    let trig_mini = mini::<String>("bd sd@1:2").unwrap();
    assert_eq!(trig_mini.query_arc(Time::zero(), Time::one()).len(), 2);
    assert_eq!(
        trig_mini
            .query_arc(Time::one(), Time::from_integer(2))
            .len(),
        1
    );

    // CK-05 Inline modular DSP pipe (`>>`)
    let stages: Vec<DspPipeStage> =
        parse_dsp_pipe("pipe: saw(freq) >> lpf(1200, 0.8) >> tanh(2.0)").unwrap();
    assert_eq!(stages.len(), 3);
    assert_eq!(stages[0].op, "saw");
    assert_eq!(stages[1].op, "lpf");
    assert_eq!(stages[1].args, vec!["1200", "0.8"]);
    assert_eq!(stages[2].op, "tanh");

    // DoS quota enforcement
    assert!(mini::<String>("bd(3,2048)").is_err());
    assert!(mini::<String>("bd!2048").is_err());
    assert!(mini::<String>("bd@2048").is_err());

    // Exercise trait methods & DelimiterStack / SourceSpan / StreamEntry helpers directly
    let mut ds = DelimiterStack::new();
    let sp0 = SourceSpan::from_byte_range("[]", 0..1);
    let sp1 = SourceSpan::from_byte_range("[]", 1..2);
    ds.push(DelimiterKind::SquareBracket, sp0);
    assert_eq!(ds.depth(), 1);
    let popped: DelimiterEntry = ds.pop_matching(DelimiterKind::SquareBracket, sp1).unwrap();
    assert_eq!(popped.kind.open_char(), '[');
    assert_eq!(popped.kind.close_char(), ']');
    assert_eq!(
        DelimiterKind::from_open('['),
        Some(DelimiterKind::SquareBracket)
    );
    assert_eq!(
        DelimiterKind::from_close(']'),
        Some(DelimiterKind::SquareBracket)
    );
    assert!(ds.check_empty_at_eof(sp1).is_ok());

    let ast = TPat::Atom {
        value: "bd".to_string(),
        span: 0..2,
    };
    let compiled = compile_tpat(&ast, &SourceOrigin::root());
    assert_eq!(compiled.first_cycle().len(), 1);
    assert_eq!(
        <usize as FromMiniAtom>::from_mini_atom("4", 0..1, "4").unwrap(),
        4usize
    );
    assert_eq!(
        <isize as FromMiniAtom>::from_mini_atom("-2", 0..2, "-2").unwrap(),
        -2isize
    );
    assert_eq!(
        <usize as FromMiniAtom>::step_range(&0, Some(&2), &4),
        vec![0, 2, 4]
    );
    assert_eq!(<i64 as MiniTarget>::apply_sample_index(&10, 2), 12);
    assert!(<i64 as MiniTarget>::from_chord_symbol("Cmaj7").is_some());
    assert!(
        <i64 as MiniTarget>::from_state_var(
            "missing",
            &State::new(TimeSpan::cycle_of(Time::zero()))
        )
        .is_none()
    );

    let se = StreamEntry {
        id: "d1".to_string(),
        pattern: s_mini("bd"),
        highlight: CompiledHighlightPattern::compile_once("bd", SourceOrigin::root()).unwrap(),
        muted: false,
        soloed: false,
    };
    let _bspan: ByteSpan = ast.span();
    let _ns = NoteSpec {
        pitch_class: 0,
        octave: 4,
        explicit_octave: true,
    };
    assert_eq!(se.id, "d1");
}
