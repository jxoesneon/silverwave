//! Coverage-targeted tests for `parser.rs` and `compiler.rs` uncovered lines:
//! `FromMiniAtom` impls for every target type, `step_range` expansions, DSP pipe
//! diagnostics, delimiter/operator error paths, `MiniTarget` state-var / sample-index /
//! chord enrichment, `compile_seq_items` folding, and all `compile_mondo_call` arms.

#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use silverwave_core::{
    ControlMap, NoteValue, Rational, State, Time, TimeSpan, TrigCondition, Value,
};
use silverwave_mini::{
    ByteSpan, CompiledHighlightPattern, DspPipeStage, FromMiniAtom, MAX_LSYSTEM_EXPANDED_TOKENS,
    MiniParser, MiniTarget, NoteSpec, ParseError, SourceOrigin, TPat, compile_tpat, mini,
    mini_with_loc, parse_dsp_pipe, parse_note_spec, parse_trig_condition_suffix,
};

/// Parses `src` as a `Pattern<String>` and returns the expected `ParseError`.
fn parse_err(src: &str) -> ParseError {
    match mini::<String>(src) {
        Err(e) => e,
        Ok(_) => panic!("expected parse error for {src:?}"),
    }
}

/// Builds a query `State` for cycle 0 with a single named control value.
fn state_with(name: &str, v: Value) -> State {
    let mut controls = ControlMap::new();
    controls.insert(name.to_string(), v);
    State::new(TimeSpan::cycle_of(Time::zero())).with_controls(controls)
}

/// `FromMiniAtom` implementor that does NOT override `step_range`, exercising the
/// trait's default two-element fallback implementation (parser.rs:22-25).
#[derive(Clone, Debug, PartialEq)]
struct Passthrough(String);

impl FromMiniAtom for Passthrough {
    fn from_mini_atom(token: &str, _span: ByteSpan, _src: &str) -> Result<Self, ParseError> {
        Ok(Self(token.to_string()))
    }
}

#[test]
fn from_mini_atom_all_target_types() {
    // i64: plain integer, pitch-name literal, and invalid-integer error (parser.rs:47-58).
    assert_eq!(
        <i64 as FromMiniAtom>::from_mini_atom("42", 0..2, "42").unwrap(),
        42
    );
    assert_eq!(
        <i64 as FromMiniAtom>::from_mini_atom("c4", 0..2, "c4").unwrap(),
        60
    );
    let e = <i64 as FromMiniAtom>::from_mini_atom("zz", 0..2, "zz").unwrap_err();
    assert_eq!(e.error_code, "SW-E010-INVALID-INT");
    assert!(e.message.contains("zz"));

    // isize delegates to the i64 implementation (parser.rs:87-89).
    assert_eq!(
        <isize as FromMiniAtom>::from_mini_atom("-2", 0..2, "-2").unwrap(),
        -2isize
    );

    // usize rejects negative values (parser.rs:105-113).
    assert_eq!(
        <usize as FromMiniAtom>::from_mini_atom("3", 0..1, "3").unwrap(),
        3usize
    );
    let e = <usize as FromMiniAtom>::from_mini_atom("-1", 0..2, "-1").unwrap_err();
    assert_eq!(e.error_code, "SW-E011-NEGATIVE-USIZE");

    // f64: ratio shorthand, finite float, rational text, pitch name, and error
    // (parser.rs:130-149). `1e400` parses as non-finite and falls through to the error.
    assert_eq!(
        <f64 as FromMiniAtom>::from_mini_atom("q", 0..1, "q").unwrap(),
        0.25
    );
    assert_eq!(
        <f64 as FromMiniAtom>::from_mini_atom("0.5", 0..3, "0.5").unwrap(),
        0.5
    );
    assert_eq!(
        <f64 as FromMiniAtom>::from_mini_atom("3/4", 0..3, "3/4").unwrap(),
        0.75
    );
    assert_eq!(
        <f64 as FromMiniAtom>::from_mini_atom("c4", 0..2, "c4").unwrap(),
        60.0
    );
    let e = <f64 as FromMiniAtom>::from_mini_atom("nope", 0..4, "nope").unwrap_err();
    assert_eq!(e.error_code, "SW-E012-INVALID-FLOAT");
    let e = <f64 as FromMiniAtom>::from_mini_atom("1e400", 0..5, "1e400").unwrap_err();
    assert_eq!(e.error_code, "SW-E012-INVALID-FLOAT");

    // Rational: shorthand, `FromStr` text, and error (parser.rs:164-179).
    assert_eq!(
        <Rational as FromMiniAtom>::from_mini_atom("h", 0..1, "h").unwrap(),
        Rational::new_const(1, 2)
    );
    assert_eq!(
        <Rational as FromMiniAtom>::from_mini_atom("5", 0..1, "5").unwrap(),
        Rational::from_integer(5)
    );
    let e = <Rational as FromMiniAtom>::from_mini_atom("zz", 0..2, "zz").unwrap_err();
    assert_eq!(e.error_code, "SW-E013-INVALID-RATIONAL");

    // Value: shorthand, int, float, note, and string fallback (parser.rs:194-209).
    assert_eq!(
        <Value as FromMiniAtom>::from_mini_atom("q", 0..1, "q").unwrap(),
        Value::Rational(Rational::new_const(1, 4))
    );
    assert_eq!(
        <Value as FromMiniAtom>::from_mini_atom("7", 0..1, "7").unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        <Value as FromMiniAtom>::from_mini_atom("0.5", 0..3, "0.5").unwrap(),
        Value::Float(0.5)
    );
    match <Value as FromMiniAtom>::from_mini_atom("c4", 0..2, "c4").unwrap() {
        Value::Note(NoteValue::Midi(m)) => assert_eq!(m, 60.0),
        other => panic!("expected note value, got {other:?}"),
    }
    assert_eq!(
        <Value as FromMiniAtom>::from_mini_atom("bd", 0..2, "bd").unwrap(),
        Value::String("bd".to_string())
    );
}

#[test]
fn step_range_all_target_types() {
    // Default trait `step_range`: `[start, end]` with no numeric expansion (parser.rs:22-25).
    let a = Passthrough("a".to_string());
    let b = Passthrough("b".to_string());
    assert_eq!(
        Passthrough::step_range(&a, None, &b),
        vec![a.clone(), b.clone()]
    );
    assert_eq!(
        Passthrough::step_range(&a, Some(&Passthrough("x".to_string())), &b),
        vec![a.clone(), b]
    );

    // `Passthrough::from_mini_atom` success arm.
    assert_eq!(
        Passthrough::from_mini_atom("zz", 0..2, "zz"),
        Ok(Passthrough("zz".to_string()))
    );

    // String: numeric expansion (with and without step) + non-numeric fallback
    // (parser.rs:33-42).
    let haps = mini::<String>("1..3").unwrap().first_cycle();
    let vals: Vec<&str> = haps.iter().map(|h| h.value.as_str()).collect();
    assert_eq!(vals, vec!["1", "2", "3"]);
    let haps = mini::<String>("1..2..5").unwrap().first_cycle();
    let vals: Vec<&str> = haps.iter().map(|h| h.value.as_str()).collect();
    assert_eq!(vals, vec!["1", "3", "5"]);
    let haps = mini::<String>("a..c").unwrap().first_cycle();
    let vals: Vec<&str> = haps.iter().map(|h| h.value.as_str()).collect();
    assert_eq!(vals, vec!["a", "c"]);

    // i64: large range hits the MAX_LSYSTEM_EXPANDED_TOKENS cap (parser.rs:77-78).
    assert_eq!(
        mini::<i64>("0..5000").unwrap().first_cycle().len(),
        MAX_LSYSTEM_EXPANDED_TOKENS
    );

    // isize delegates to i64::step_range (parser.rs:91-99).
    let vals: Vec<isize> = mini::<isize>("0..2")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![0isize, 1, 2]);

    // usize filters out negatives (parser.rs:116-125).
    let vals: Vec<usize> = mini::<usize>("1..3")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![1usize, 2, 3]);
    let vals: Vec<usize> = mini::<usize>("3..0")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![3usize, 2, 1, 0]);

    // f64 floors bounds into an integer range (parser.rs:152-160).
    let vals: Vec<f64> = mini::<f64>("1..3")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![1.0, 2.0, 3.0]);

    // Rational floors bounds into an integer range (parser.rs:182-190).
    let vals: Vec<Rational> = mini::<Rational>("1..3")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(
        vals,
        vec![
            Rational::from_integer(1),
            Rational::from_integer(2),
            Rational::from_integer(3)
        ]
    );

    // Value: integer-coercible expansion + string fallback (parser.rs:212-221).
    let vals: Vec<Value> = mini::<Value>("1..3")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let vals: Vec<Value> = mini::<Value>("a..b")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(
        vals,
        vec![
            Value::String("a".to_string()),
            Value::String("b".to_string())
        ]
    );
}

#[test]
fn note_spec_and_trig_suffixes() {
    // parse_note_spec edge branches (parser.rs:243, 248, 252-254, 258, 273, 280-287).
    assert_eq!(parse_note_spec(""), None);
    assert_eq!(parse_note_spec("c"), None);
    assert_eq!(parse_note_spec("z4"), None);
    assert_eq!(
        parse_note_spec("d4"),
        Some(NoteSpec {
            pitch_class: 2,
            octave: 4,
            explicit_octave: true
        })
    );
    assert_eq!(
        parse_note_spec("b4"),
        Some(NoteSpec {
            pitch_class: 11,
            octave: 4,
            explicit_octave: true
        })
    );
    // `n` natural accidental (parser.rs:273).
    assert_eq!(
        parse_note_spec("cn4"),
        Some(NoteSpec {
            pitch_class: 0,
            octave: 4,
            explicit_octave: true
        })
    );
    // Accidentals with no octave digit default to octave 3 (parser.rs:280-286).
    assert_eq!(
        parse_note_spec("cs"),
        Some(NoteSpec {
            pitch_class: 1,
            octave: 3,
            explicit_octave: false
        })
    );
    assert_eq!(
        parse_note_spec("eb"),
        Some(NoteSpec {
            pitch_class: 3,
            octave: 3,
            explicit_octave: false
        })
    );

    // Elektron conditional trig suffixes incl. negated variants and `a:b` ratios
    // (parser.rs:300-317).
    assert_eq!(
        parse_trig_condition_suffix("1st"),
        Some(TrigCondition::FirstCycle { expected: true })
    );
    assert_eq!(
        parse_trig_condition_suffix("!1st"),
        Some(TrigCondition::FirstCycle { expected: false })
    );
    assert_eq!(
        parse_trig_condition_suffix("!fill"),
        Some(TrigCondition::FillLatch { active: false })
    );
    assert_eq!(
        parse_trig_condition_suffix("!pre"),
        Some(TrigCondition::PreviousConditionalOnSameStream { expected: false })
    );
    assert_eq!(
        parse_trig_condition_suffix("!nei"),
        Some(TrigCondition::PreviousConditionalOnNeighborStream { expected: false })
    );
    assert_eq!(
        parse_trig_condition_suffix("2:3"),
        Some(TrigCondition::Ratio { a: 2, b: 3 })
    );
    assert_eq!(parse_trig_condition_suffix("0:3"), None);
    assert_eq!(parse_trig_condition_suffix("3:2"), None);
    assert_eq!(parse_trig_condition_suffix("a:b"), None);
}

#[test]
fn dsp_pipe_stages_and_errors() {
    // Bare stage without parentheses (parser.rs:368-373).
    let stages: Vec<DspPipeStage> = parse_dsp_pipe("pipe:reverb").unwrap();
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].op, "reverb");
    assert!(stages[0].args.is_empty());

    // Empty argument list produces no args (parser.rs:355-356); works without
    // the `pipe:` prefix too.
    let stages = parse_dsp_pipe("saw() >> gain").unwrap();
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0].op, "saw");
    assert!(stages[0].args.is_empty());
    assert_eq!(stages[1].op, "gain");

    // Empty `>>` stage (parser.rs:332-338).
    let e = parse_dsp_pipe("pipe:a >> >> b").unwrap_err();
    assert_eq!(e.error_code, "SW-E030-EMPTY-DSP-STAGE");

    // Stage with '(' not terminated by ')' (parser.rs:345-351).
    let e = parse_dsp_pipe("saw(440").unwrap_err();
    assert_eq!(e.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");
}

#[test]
fn parser_eof_comments_and_trailing_close() {
    // Empty / whitespace-only / comment-only input parses to Silence (parser.rs:404-405).
    assert_eq!(mini::<String>("").unwrap().first_cycle().len(), 0);
    assert!(matches!(
        MiniParser::parse::<String>("   "),
        Ok(TPat::Silence { .. })
    ));
    assert_eq!(
        mini::<String>("// nothing but a comment")
            .unwrap()
            .first_cycle()
            .len(),
        0
    );
    assert_eq!(
        mini::<String>("/* block */ /* another */")
            .unwrap()
            .first_cycle()
            .len(),
        0
    );

    // Unclosed block comment (parser.rs:462-475).
    let e = parse_err("bd /* unterminated");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");

    // Stray closing delimiter after a complete sequence (parser.rs:409-413).
    let e = parse_err("bd ]");
    assert_eq!(e.error_code, "SW-E002-UNEXPECTED-CLOSING-DELIMITER");
}

#[test]
fn parser_max_depth_quota() {
    // Nesting deeper than MAX_AST_NESTING_DEPTH (parser.rs:482-494). The parse is
    // run on a dedicated thread with a large stack so the recursion reaches the
    // SW-E020 quota guard instead of overflowing the (small) test-thread stack.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let src = "[".repeat(70) + "bd";
            match mini::<String>(&src) {
                Err(e) => e,
                Ok(_) => panic!("expected depth error"),
            }
        })
        .expect("spawn depth probe");
    let e = match handle.join() {
        Ok(e) => e,
        Err(_) => panic!("depth probe thread panicked"),
    };
    assert_eq!(e.error_code, "SW-E020-MAX-DEPTH-EXCEEDED");
    assert!(e.message.contains("MAX_AST_NESTING_DEPTH"));
}

#[test]
fn parser_at_modifier_paths() {
    // '@' with no following token (parser.rs:664-675).
    let e = parse_err("bd@");
    assert_eq!(e.error_code, "SW-E014-MISSING-WEIGHT");

    // '@' modifier that is neither a trig condition, a rational weight, nor
    // applicable (target is not a chord) (parser.rs:699-715).
    let e = parse_err("bd@xyz");
    assert_eq!(e.error_code, "SW-E014-INVALID-AT-MODIFIER");

    // Negated trig condition: `!` is allowed at the very start of the postfix
    // token (parser.rs:904 + 677-682).
    assert_eq!(mini::<String>("bd@!fill").unwrap().first_cycle().len(), 1);
    // `!1st` = "not on the first cycle": silent in cycle 0, plays from cycle 1.
    let not_first = mini::<String>("bd@!1st").unwrap();
    assert!(not_first.first_cycle().is_empty());
    assert_eq!(
        not_first
            .query_arc(Time::one(), Time::from_integer(2))
            .len(),
        1
    );

    // Voicing dictionary tag on a chord rewrites `dict_tag` (parser.rs:700-708).
    let p = mini::<i64>("C^7@drop2").unwrap();
    assert_eq!(p.first_cycle()[0].value, 60);
}

#[test]
fn parser_postfix_operand_errors() {
    // Postfix operator at EOF (parser.rs:860-867).
    let e = parse_err("bd*");
    assert_eq!(e.error_code, "SW-E016-MISSING-OPERAND");

    // Postfix operator followed by a non-token character (parser.rs:874-880).
    let e = parse_err("bd* sd");
    assert_eq!(e.error_code, "SW-E016-MISSING-OPERAND");

    // Euclidean group missing the `,` separator (parser.rs:796-802).
    let e = parse_err("bd(3)");
    assert_eq!(e.error_code, "SW-E015-INVALID-EUCLID");

    // Euclidean group hitting EOF before its close (parser.rs:826-829).
    let e = parse_err("bd(3,8");
    assert_eq!(e.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");

    // Euclidean group closed by the wrong delimiter kind (parser.rs:831-837).
    let e = parse_err("bd(3,8}");
    assert_eq!(e.error_code, "SW-E001-MISMATCHED-DELIMITER");

    // Postfix token stops at `!` mid-token: `bd*2!3` == fast-by-2 then replicate-by-3
    // (parser.rs:903-906).
    assert_eq!(mini::<String>("bd*2!3").unwrap().first_cycle().len(), 6);
}

#[test]
fn parser_eof_and_unexpected_token_errors() {
    // EOF inside `|` choice and inside `..` range (parser.rs:917-923).
    let e = parse_err("bd|");
    assert_eq!(e.error_code, "SW-E006-UNEXPECTED-EOF");
    let e = parse_err("a..");
    assert_eq!(e.error_code, "SW-E006-UNEXPECTED-EOF");

    // Mondo call with empty function identifier (parser.rs:1005-1011).
    let e = parse_err("()");
    assert_eq!(e.error_code, "SW-E017-EMPTY-MONDO-CALL");

    // EOF inside Mondo args -> unclosed '(' blame (parser.rs:1016-1017, 1115-1117).
    let e = parse_err("(fast 2");
    assert_eq!(e.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");
    assert_eq!(e.primary_span.start, 0);

    // '^' / '^^' with no identifier (parser.rs:1052-1058).
    let e = parse_err("^");
    assert_eq!(e.error_code, "SW-E018-EMPTY-STATE-VAR");
    let e = parse_err("^^ ");
    assert_eq!(e.error_code, "SW-E018-EMPTY-STATE-VAR");

    // Unexpected non-delimiter character reaching `parse_primary` default arm
    // (parser.rs:1070-1081 + advance_char 437-441).
    let e = parse_err("*");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");
    let e = parse_err("a..*");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");
    // `..` inside a postfix token halts the token; the dangling range then errors
    // (parser.rs:899-901, 1177, 1070-1081).
    let e = parse_err("bd*2..5");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");

    // Unexpected CLOSE char at `parse_primary` pops the delimiter stack:
    // empty stack -> SW-E002; mismatched top -> SW-E001 (parser.rs:1073-1075).
    let e = parse_err("a..]");
    assert_eq!(e.error_code, "SW-E002-UNEXPECTED-CLOSING-DELIMITER");
    let e = parse_err("[a..)");
    assert_eq!(e.error_code, "SW-E001-MISMATCHED-DELIMITER");
}

#[test]
fn parser_structural_branches() {
    // Empty foot collapses to Silence via fallback span (parser.rs:577-580).
    assert_eq!(mini::<String>("[,a]").unwrap().first_cycle().len(), 1);

    // Slowcat with a single non-Seq inner node wraps it in one item (parser.rs:963).
    assert_eq!(mini::<String>("<a>").unwrap().first_cycle().len(), 1);
    // Slowcat whose inner is a Stack stays a single slowcat item too.
    let p = mini::<String>("<a,b>").unwrap();
    assert!(!p.first_cycle().is_empty());

    // Polymeter with non-Stack inner wraps it as one layer (parser.rs:980),
    // and `{...}%n` sets the step override (parser.rs:982-984).
    assert!(!mini::<String>("{a b}").unwrap().first_cycle().is_empty());
    assert!(
        !mini::<String>("{a b c}%2")
            .unwrap()
            .first_cycle()
            .is_empty()
    );
    assert!(
        !mini::<String>("{a b, c d}%2")
            .unwrap()
            .first_cycle()
            .is_empty()
    );

    // `bd*2..5`-style break of `..` inside atoms + decimal `0.5` inside atoms and
    // `.` foot separator when not digit-bracketed (parser.rs:1173-1188).
    let h = mini::<f64>("0.5").unwrap().first_cycle();
    assert_eq!(h[0].value, 0.5);
    assert_eq!(mini::<String>("a.b").unwrap().first_cycle().len(), 2);
}

#[test]
fn parser_unexpected_close_returns_diagnostic() {
    // These inputs reach the previously-panicking `pop_matching(...).unwrap_err()`
    // sites (parser.rs unexpected-close paths); they must now yield diagnostics.
    for src in ["bd(3,8;", "[a..]", "(f a..)"] {
        assert!(
            mini::<String>(src).is_err(),
            "expected parse error for {src:?}"
        );
    }
}

#[test]
fn chord_literal_quality_markers() {
    // Upper-case roots with `m`, `7`, `9`, `11`, `13`, `6`, `o` quality markers
    // (parser.rs:1207-1220).
    let p = mini::<Value>("Cm C7 C9 C11 C13 C6 Co").unwrap();
    let haps = p.first_cycle();
    assert_eq!(haps.len(), 7);
    assert!(haps.iter().all(|h| matches!(h.value, Value::List(_))));

    // Lower-case tokens without `/^` are NOT chord literals (parser.rs:1199-1207).
    assert_eq!(mini::<String>("cm7").unwrap().first_cycle()[0].value, "cm7");
}

#[test]
fn compile_seq_items_edge_cases() {
    // Tie as the first sequence item folds into a silent unit weight (compiler.rs:346-349).
    let haps = mini::<String>("_ a").unwrap().first_cycle();
    assert_eq!(haps.len(), 1);
    assert_eq!(haps[0].value, "a");

    // Standalone Elongate compiles through the dedicated TPat::Elongate arm
    // (compiler.rs:238-240).
    assert_eq!(mini::<String>("a@2").unwrap().first_cycle().len(), 1);

    // Standalone `!` replicate compiles through the TPat::Repeat arm via `ply_pat`
    // (compiler.rs:248-252).
    assert_eq!(mini::<String>("a!").unwrap().first_cycle().len(), 2);

    // `!` with a non-atom (patterned) count inside a sequence goes through the
    // generic Repeat arm rather than unrolling (compiler.rs:368-369 + 248-252).
    assert_eq!(mini::<String>("a!<2 3> b").unwrap().first_cycle().len(), 3);
    assert!(mini::<String>("a![2 3] b").unwrap().first_cycle().len() >= 3);

    // `?` with a patterned probability arg parses the `<`/`[` postfix-arg path
    // (parser.rs:754-765, 868-869).
    assert!(mini::<String>("bd?<0.5 0.9>").unwrap().first_cycle().len() <= 1);
    assert!(
        mini::<String>("bd?[0.25 0.75]")
            .unwrap()
            .first_cycle()
            .len()
            <= 1
    );

    // Empty `TPat::Seq` compiles to silence (compiler.rs:336-337).
    let ast = TPat::<String>::Seq {
        items: Vec::new(),
        span: core::ops::Range { start: 0, end: 0 },
    };
    assert!(
        compile_tpat(&ast, &SourceOrigin::root())
            .first_cycle()
            .is_empty()
    );

    // EnumFromTo with a silence-producing bound degrades to silence (compiler.rs:288-289).
    assert!(mini::<i64>("~..3").unwrap().first_cycle().is_empty());
    assert!(mini::<String>("~..3").unwrap().first_cycle().is_empty());
}

#[test]
fn compile_chord_targets() {
    // String target echoes the (possibly dict-tagged) symbol (compiler.rs:41-43, 188-190).
    assert_eq!(mini::<String>("C^7").unwrap().first_cycle()[0].value, "C^7");
    assert_eq!(
        mini::<String>("C^7@drop2").unwrap().first_cycle()[0].value,
        "C^7@drop2"
    );

    // Numeric targets take the root MIDI note (compiler.rs:55-58, 74-76, 92-94,
    // 106-108, 120-122). C root at octave 4 -> 60.
    assert_eq!(mini::<i64>("C^7").unwrap().first_cycle()[0].value, 60);
    assert_eq!(mini::<isize>("C^7").unwrap().first_cycle()[0].value, 60);
    assert_eq!(mini::<usize>("C^7").unwrap().first_cycle()[0].value, 60);
    assert_eq!(mini::<f64>("C^7").unwrap().first_cycle()[0].value, 60.0);
    assert_eq!(
        mini::<Rational>("C^7").unwrap().first_cycle()[0].value,
        Rational::from_integer(60)
    );

    // A `TPat::Chord` whose full symbol fails `from_chord_symbol` -> silence
    // (compiler.rs:193-197).
    let bad = TPat::<i64>::Chord {
        symbol: "zzz".to_string(),
        root: NoteSpec {
            pitch_class: 0,
            octave: 4,
            explicit_octave: true,
        },
        quality: "maj7".to_string(),
        inversion: None,
        dict_tag: None,
        span: 0..3,
    };
    assert!(
        compile_tpat(&bad, &SourceOrigin::root())
            .first_cycle()
            .is_empty()
    );

    // `dict_tag` present but symbol already contains '@' -> symbol kept as-is
    // (compiler.rs:190).
    let tagged = TPat::<i64>::Chord {
        symbol: "C^7@triad".to_string(),
        root: NoteSpec {
            pitch_class: 0,
            octave: 4,
            explicit_octave: true,
        },
        quality: "maj7".to_string(),
        inversion: None,
        dict_tag: Some("ignored".to_string()),
        span: 0..9,
    };
    assert_eq!(
        compile_tpat(&tagged, &SourceOrigin::root()).first_cycle()[0].value,
        60
    );
}

#[test]
fn sample_index_all_targets() {
    // apply_sample_index per MiniTarget impl (compiler.rs:37-39, 51-53, 70-72,
    // 88-90, 102-104, 116-118, 130-134).
    assert_eq!(mini::<i64>("5:2").unwrap().first_cycle()[0].value, 7);
    assert_eq!(mini::<isize>("5:2").unwrap().first_cycle()[0].value, 7);
    // usize ignores `self` and returns the index itself.
    assert_eq!(mini::<usize>("7:2").unwrap().first_cycle()[0].value, 2);
    assert_eq!(mini::<f64>("1.5:2").unwrap().first_cycle()[0].value, 3.5);
    assert_eq!(
        mini::<Rational>("1:2").unwrap().first_cycle()[0].value,
        Rational::from_integer(3)
    );
    assert_eq!(
        mini::<Value>("bd:2").unwrap().first_cycle()[0].value,
        Value::String("bd:2".to_string())
    );
    // Non-string Value targets are returned unchanged.
    match mini::<Value>("c4:2").unwrap().first_cycle()[0].value {
        Value::Note(NoteValue::Midi(m)) => assert_eq!(m, 60.0),
        ref other => panic!("expected note value, got {other:?}"),
    }
}

#[test]
fn state_vars_all_targets() {
    // String target formats each control Value variant (compiler.rs:27-35).
    let p = mini::<String>("^v").unwrap();
    assert_eq!(
        p.query(&state_with("v", Value::String("hit".to_string())))[0].value,
        "hit"
    );
    assert_eq!(p.query(&state_with("v", Value::Int(7)))[0].value, "7");
    assert_eq!(p.query(&state_with("v", Value::Float(2.5)))[0].value, "2.5");
    assert_eq!(
        p.query(&state_with("v", Value::Rational(Rational::new_const(1, 2))))[0].value,
        "1/2"
    );
    // Unsupported variants yield no haps (compiler.rs:33 + 166-167).
    assert!(p.query(&state_with("v", Value::Bool(true))).is_empty());

    // i64 coerces via Value::as_i64; unrepresentable values yield no haps
    // (compiler.rs:47-49, 166-167).
    let p = mini::<i64>("^v").unwrap();
    assert_eq!(p.query(&state_with("v", Value::Int(41)))[0].value, 41);
    assert_eq!(p.query(&state_with("v", Value::Float(2.9)))[0].value, 2);
    assert!(
        p.query(&state_with("v", Value::String("x".to_string())))
            .is_empty()
    );

    // isize maps i64 coercion (compiler.rs:62-68).
    let p = mini::<isize>("^v").unwrap();
    assert_eq!(p.query(&state_with("v", Value::Int(-3)))[0].value, -3);

    // usize clamps negatives to 0 (compiler.rs:80-86).
    let p = mini::<usize>("^v").unwrap();
    assert_eq!(p.query(&state_with("v", Value::Int(-3)))[0].value, 0);
    assert_eq!(p.query(&state_with("v", Value::Int(4)))[0].value, 4);

    // f64 coerces via Value::as_f64 (compiler.rs:98-100).
    let p = mini::<f64>("^v").unwrap();
    assert_eq!(p.query(&state_with("v", Value::Int(3)))[0].value, 3.0);

    // Rational coerces via Value::as_rational (compiler.rs:112-114).
    let p = mini::<Rational>("^v").unwrap();
    assert_eq!(
        p.query(&state_with("v", Value::Int(3)))[0].value,
        Rational::from_integer(3)
    );
    assert_eq!(
        p.query(&state_with("v", Value::Rational(Rational::new_const(1, 2))))[0].value,
        Rational::new_const(1, 2)
    );

    // Value clones the control verbatim (compiler.rs:126-128).
    let p = mini::<Value>("^v").unwrap();
    assert_eq!(
        p.query(&state_with("v", Value::Int(9)))[0].value,
        Value::Int(9)
    );
    assert_eq!(
        p.query(&state_with("v", Value::List(vec![Value::Int(1)])))[0].value,
        Value::List(vec![Value::Int(1)])
    );

    // Missing control produces no haps (compiler.rs:166-167), and `^^v` marks the
    // var `local_time` (compiler.rs:180).
    let p = mini::<f64>("^missing").unwrap();
    assert!(
        p.query(&State::new(TimeSpan::cycle_of(Time::zero())))
            .is_empty()
    );
    let p = mini::<f64>("^^v").unwrap();
    assert_eq!(p.query(&state_with("v", Value::Float(1.0)))[0].value, 1.0);
}

#[test]
fn mondo_call_compile_arms() {
    // Top-level `(fast n pat)` / `(slow n pat)` are intercepted by `mini_with_loc`
    // and compiled with the raw factor text (compiler.rs:443-461).
    assert_eq!(mini::<String>("(fast 2 a)").unwrap().first_cycle().len(), 2);
    let slow2 = mini_with_loc::<String>("(slow 2 a)", SourceOrigin::root()).unwrap();
    // The slowed hap spans two whole cycles and is reported once per cycle.
    assert_eq!(
        slow2.query_arc(Time::zero(), Time::from_integer(2)).len(),
        2
    );

    // Nested Mondo calls route through `compile_mondo_call` instead, where the
    // factor cannot be recovered from a typed `TPat` atom and defaults to 1
    // (compiler.rs mondo "fast"/"slow" arms).
    assert_eq!(
        mini::<String>("[x (fast 2 [a b])]")
            .unwrap()
            .first_cycle()
            .len(),
        3
    );
    // Top-level `(fast bad pat)` fails the interception parse and falls through
    // to the compile_mondo_call arm (factor defaults to 1).
    assert_eq!(
        mini::<String>("(fast xx a)").unwrap().first_cycle().len(),
        1
    );
    assert_eq!(
        mini::<String>("(slow xx a)").unwrap().first_cycle().len(),
        1
    );

    // Remaining compile_mondo_call arms (compiler.rs:408-425).
    assert_eq!(
        mini::<String>("(rev [a b])").unwrap().first_cycle().len(),
        2
    );
    assert!(mini::<String>("(degrade a)").unwrap().first_cycle().len() <= 1);
    assert_eq!(
        mini::<String>("(stack a b)").unwrap().first_cycle().len(),
        2
    );
    assert_eq!(mini::<String>("(cat a b)").unwrap().first_cycle().len(), 1);
    assert_eq!(
        mini::<String>("(slowcat a b)").unwrap().first_cycle().len(),
        1
    );
    assert_eq!(
        mini::<String>("(fastcat a b)").unwrap().first_cycle().len(),
        2
    );
    assert_eq!(mini::<String>("(seq a b)").unwrap().first_cycle().len(), 2);
    // Unknown function with no args -> silence.
    assert!(
        mini::<String>("(unknownfn)")
            .unwrap()
            .first_cycle()
            .is_empty()
    );
    // `(fast)`/`(slow)` with too few args fall into the default arm too.
    assert!(mini::<String>("(fast)").unwrap().first_cycle().is_empty());
    // Unknown function with args -> fastcat of the args.
    assert_eq!(
        mini::<String>("(unknownfn a b)")
            .unwrap()
            .first_cycle()
            .len(),
        2
    );
}

#[test]
fn highlight_span_invariant_error() {
    // A crafted `SourceOrigin` whose escape_shifts invert a mapped span triggers
    // the SW-E099 span-invariant guard (compiler.rs:489-497).
    let origin = SourceOrigin {
        outer_content_offset: 0,
        escape_shifts: vec![(2, 10), (4, 0)],
    };
    let err = CompiledHighlightPattern::<String>::compile_once("ab cd", origin)
        .err()
        .unwrap();
    assert_eq!(err.error_code, "SW-E099-SPAN-INVARIANT");

    // Parse errors propagate through compile_once as-is.
    assert!(
        CompiledHighlightPattern::<String>::compile_once("[", SourceOrigin::root())
            .err()
            .is_some()
    );
}

#[test]
fn parse_error_edges_skip_comments_and_commas() {
    // Unclosed block comment at byte 0 fails `parse`'s first whitespace/comment
    // skip before any AST work (parser.rs `p.skip_ws_and_comments()?`).
    let e = parse_err("/*");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");

    // Unclosed block comment inside a nested `[ ... ]` stack: the inner
    // `parse_stack` re-skip propagates SW-E005 (parser.rs:497).
    let e = parse_err("[ /* unterminated");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");

    // After a `,` layer separator the comment skip and the next layer's
    // `parse_dot_sequence` are both fallible (parser.rs:506-507).
    let e = parse_err("bd, /* unterminated");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");
    let e = parse_err("bd, *");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");
}

#[test]
fn usize_atom_and_stepped_range_targets() {
    // usize::from_mini_atom propagates the inner i64 parse error via `?`
    // (parser.rs:104).
    let e = <usize as FromMiniAtom>::from_mini_atom("zz", 0..2, "zz").unwrap_err();
    assert_eq!(e.error_code, "SW-E010-INVALID-INT");

    // Stepped ranges exercise the `step.map(..)` Some arm of each numeric
    // `step_range` (parser.rs:94, 155, 185).
    let vals: Vec<isize> = mini::<isize>("0..2..6")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![0isize, 2, 4, 6]);
    let vals: Vec<f64> = mini::<f64>("0..2..6")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![0.0, 2.0, 4.0, 6.0]);
    let vals: Vec<Rational> = mini::<Rational>("0..2..6")
        .unwrap()
        .first_cycle()
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(
        vals,
        vec![
            Rational::from_integer(0),
            Rational::from_integer(2),
            Rational::from_integer(4),
            Rational::from_integer(6)
        ]
    );

    // `a:b` trig ratio where `b` fails u16 parsing (parser.rs:310).
    assert_eq!(parse_trig_condition_suffix("2:x"), None);
}

#[test]
fn parser_range_postfix_and_slow_operands() {
    // Third `..` operand errors propagate through the stepped-range `?`
    // (parser.rs:618).
    let e = parse_err("a..b..*");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");

    // `/` slow postfix with no operand hits the EOF arm of `parse_postfix_arg`
    // (parser.rs:652).
    let e = parse_err("bd/");
    assert_eq!(e.error_code, "SW-E016-MISSING-OPERAND");

    // `!`, `?`, and `:` postfix operands that open a group whose delimiter is
    // never closed propagate the inner error via `?` (parser.rs:723, 755, 774).
    for src in ["bd![", "bd?[", "bd:["] {
        let e = parse_err(src);
        assert_eq!(e.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");
    }

    // Postfix token that fails `U::from_mini_atom` propagates via `?`
    // (parser.rs:888).
    let e = parse_err("bd*zz");
    assert_eq!(e.error_code, "SW-E013-INVALID-RATIONAL");

    // `..` followed by an unclosed block comment: `parse_primary`'s own
    // whitespace/comment skip fails (parser.rs:922).
    let e = parse_err("a.. /* unterminated");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");

    // Atom text that fails `T::from_mini_atom` propagates via `?`
    // (parser.rs:1110).
    let e = mini::<i64>("zz").err().unwrap();
    assert_eq!(e.error_code, "SW-E010-INVALID-INT");
}

#[test]
fn parser_euclid_error_edges() {
    // Unclosed `/*` comment at each whitespace skip inside `(k,n,o)`
    // (parser.rs:789, 791, 801, 813, 816, 821).
    for src in [
        "a( /*x",
        "a(1 /*x",
        "a(1, /*x",
        "a(1,2 /*x",
        "a(1,2, /*x",
        "a(1,2,3 /*x",
    ] {
        let e = parse_err(src);
        assert_eq!(
            e.error_code, "SW-E005-UNCLOSED-COMMENT",
            "expected comment error for {src:?}"
        );
    }

    // Non-integer operands for pulses / steps / offset propagate via `?`
    // (parser.rs:790, 802, 817).
    for src in ["a(x,2)", "a(1,x)", "a(1,2,x)"] {
        let e = parse_err(src);
        assert_eq!(
            e.error_code, "SW-E010-INVALID-INT",
            "expected int error for {src:?}"
        );
    }
}

#[test]
fn parser_angle_curly_paren_error_edges() {
    // `<` inner stack errors propagate; expect_closing reports mismatches and
    // EOF (parser.rs:964-965).
    let e = parse_err("<*");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");
    let e = parse_err("<a]");
    assert_eq!(e.error_code, "SW-E001-MISMATCHED-DELIMITER");

    // `{` inner stack errors, unclosed `{`, and a `%` step override whose
    // operand fails Rational parsing (parser.rs:982-983, 990).
    let e = parse_err("{*");
    assert_eq!(e.error_code, "SW-E004-UNEXPECTED-TOKEN");
    let e = parse_err("{a");
    assert_eq!(e.error_code, "SW-E003-UNCLOSED-OPENING-DELIMITER");
    let e = parse_err("{a}%zz");
    assert_eq!(e.error_code, "SW-E013-INVALID-RATIONAL");

    // `(func ...)` Mondo call: unclosed comment in the leading skip and inside
    // the args loop (parser.rs:1008, 1021).
    let e = parse_err("( /*x");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");
    let e = parse_err("(f /*x");
    assert_eq!(e.error_code, "SW-E005-UNCLOSED-COMMENT");
}

#[test]
fn parser_depth_quota_on_postfix_and_alt_groups() {
    // check_depth error arms for the `(...)` euclid postfix and the `<`, `{`,
    // `(` primary openers (parser.rs:787, 963, 981, 1007). Run on a dedicated
    // thread with a large stack so recursion reaches the SW-E020 quota guard.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            [
                "[".repeat(64) + "a(1,2)",
                "<".repeat(65),
                "{".repeat(65),
                "(f ".repeat(65),
            ]
            .into_iter()
            .map(|src| {
                mini::<String>(&src)
                    .err()
                    .map(|e| e.error_code)
                    .unwrap_or("NO-ERROR")
            })
            .collect::<Vec<_>>()
        })
        .expect("spawn depth probe");
    let codes = match handle.join() {
        Ok(c) => c,
        Err(_) => panic!("depth probe thread panicked"),
    };
    assert_eq!(codes, vec!["SW-E020-MAX-DEPTH-EXCEEDED"; 4]);
}

#[test]
fn value_chord_symbol_failure_is_silence() {
    // `Value::from_chord_symbol` propagates `?` when the full symbol fails
    // `parse_chord_symbol`, yielding silence (compiler.rs:138).
    let bad = TPat::<Value>::Chord {
        symbol: "zzz".to_string(),
        root: NoteSpec {
            pitch_class: 0,
            octave: 4,
            explicit_octave: true,
        },
        quality: "maj7".to_string(),
        inversion: None,
        dict_tag: None,
        span: 0..3,
    };
    assert!(
        compile_tpat(&bad, &SourceOrigin::root())
            .first_cycle()
            .is_empty()
    );
}

#[test]
fn rational_mini_target_and_residual_parse_edges() {
    // parser.rs:165-178 — `FromMiniAtom for Rational` shorthand/float/error arms.
    assert_eq!(
        <Rational as FromMiniAtom>::from_mini_atom("1/2", 0..3, "1/2").unwrap(),
        Rational::new_const(1, 2)
    );
    assert_eq!(
        <Rational as FromMiniAtom>::from_mini_atom("3", 0..1, "3").unwrap(),
        Rational::from_integer(3)
    );
    assert!(<Rational as FromMiniAtom>::from_mini_atom("zz", 0..2, "zz").is_err());
    // parser.rs:182-189 — `step_range` with `Some` and `None` step.
    let a = Rational::from_integer(1);
    let s2 = Rational::from_integer(2);
    let b = Rational::from_integer(5);
    assert_eq!(
        <Rational as FromMiniAtom>::step_range(&a, Some(&s2), &b).len(),
        3
    );
    assert_eq!(
        <Rational as FromMiniAtom>::step_range(&a, None, &b).len(),
        5
    );
    // compiler.rs:112-122 — `MiniTarget for Rational` methods.
    let st = state_with("x", Value::Rational(Rational::new_const(3, 2)));
    assert_eq!(
        <Rational as MiniTarget>::from_state_var("x", &st),
        Some(Rational::new_const(3, 2))
    );
    assert_eq!(
        <Rational as MiniTarget>::apply_sample_index(&Rational::one(), 2),
        Rational::from_integer(3)
    );
    assert_eq!(
        <Rational as MiniTarget>::from_chord_symbol("c'maj"),
        Some(Rational::from_integer(60))
    );
    // parser.rs:404,411-418 — empty input `TPat::Silence` + unmatched-close error arms.
    assert!(mini::<Rational>("").is_ok());
    assert!(mini::<Rational>(")").is_err());
    // compiler.rs:157-167 — `compile_tpat` `Silence`/`Tie`-style leaf arms.
    assert!(mini::<Rational>("~").is_ok());
}
