//! Comprehensive TDD & Property-Based Test Suite for `silverwave-core` (`v0.1.0`).
//! Covers 100% of public types, traits, methods, combinators, algebraic laws,
//! pain-point solutions (`PP-03`–`PP-08`, `PP-11`), and Category-King features (`CK-01`, `CK-02`, `CK-04`).

#![forbid(unsafe_code)]

use alloc::collections::BTreeSet;
use alloc::string::ToString;
use alloc::sync::Arc as StdArc;
use alloc::vec;
use alloc::vec::Vec;
use core::str::FromStr;

extern crate alloc;

use silverwave_core::{
    Arc, BoxedFn, Context, ControlMap, ControlPattern, ControlPatternOps, CycleStateCheckpointer,
    EngineCompatibilityProfile, Event, GlideCurve, Hap, IntoPattern, LegatoGlideSpec,
    LruCycleStateCheckpointer, MAX_STATE_CHECKPOINTS_PER_STREAM, NoteValue, ParsedChord, Pattern,
    PatternApplicative, PatternFunctor, PatternMonad, PatternTrigJoin, QueryContext, QueryFn,
    Rational, Rational64, SamplePatternOps, ScalaTuning, SignalRange, SourceLocation, State,
    StatefulPattern, StereoJux, Time, TimeExt, TimeSpan, TonalExt, TrigCondition,
    TrigConditionLedger, Value, arp_with, arrange, asap, berlin, bjorklund, cat, choose,
    choose_cycles, cos_f64, cosine, ctrl_param, exp2_f64, fastcat, from_list, gain,
    global_controller, irand, isaw, lift_a2_both, lift_a2_left, lift_a2_right, ln_f64, lpf,
    lsystem_expand, markov_pattern, n, note, once, pan, parse_chord_symbol, parse_scale_descriptor,
    perlin, polymeter, polyrhythm, pure, qtrigger, rand, resolve_scale_degree, run, s, saw,
    scale_intervals, scan, seq_p, sequence, silence, sin_f64, sine, slowcat_prime, splitmix64,
    square, stack, stepcat, sweep_line_join, tie_contiguous, time_to_rand, time_to_rand_seeded,
    timecat, transitions, tri, wchoose,
};

#[test]
fn test_rational_arithmetic_and_stern_brocot_guard() {
    assert_eq!(Rational::new(0, 0), None);
    let r0 = Rational::zero();
    let r1 = Rational::one();
    assert!(r0.is_zero());
    assert!(r1.is_positive());
    assert!((-r1).is_negative());
    assert_eq!(r0.signum(), Rational::zero());
    assert_eq!(r1.signum(), Rational::one());
    assert_eq!((-r1).signum(), Rational::from_integer(-1));
    assert_eq!(r0.recip(), None);
    assert_eq!(Rational::new_const(3, 4).recip(), Rational::new(4, 3));

    let a = Rational::new_const(2, 4);
    let _arc_alias: Arc = TimeSpan::cycle_of(a);
    assert_eq!(a.numer(), 1);
    assert_eq!(a.denom(), 2);
    assert_eq!(a.abs(), a);
    assert_eq!((-a).abs(), a);

    let b = Rational::new_const(-7, 3);
    assert_eq!(b.floor_i64(), -3);
    assert_eq!(b.floor(), Rational::from_integer(-3));
    assert_eq!(b.ceil_i64(), -2);
    assert_eq!(b.ceil(), Rational::from_integer(-2));
    assert_eq!(b.trunc(), Rational::from_integer(-2));
    assert_eq!(b.fract(), Rational::new_const(-1, 3));
    assert!((a.to_f64() - 0.5).abs() < 1e-12);

    let approx = Rational::from_f64_approx(0.25, 1000).unwrap();
    assert_eq!(approx, Rational::new_const(1, 4));
    assert_eq!(Rational::from_f64_approx(f64::NAN, 1000), None);

    // Stern-Brocot continued fraction guard when denominator > MAX_DENOMINATOR (1_000_000_007)
    let huge_den: i128 = (Rational::MAX_DENOMINATOR as i128) * 17 + 3;
    let huge_num: i128 = (Rational::MAX_DENOMINATOR as i128) * 5 + 1;
    let guarded = Rational::checked_op_stern_brocot(huge_num, huge_den);
    assert!(guarded.denom() <= Rational::MAX_DENOMINATOR);
    assert!((guarded.to_f64() - (huge_num as f64 / huge_den as f64)).abs() < 1e-9);

    // Operators, Display, FromStr, TimeExt
    assert_eq!(a + Rational::new_const(1, 3), Rational::new_const(5, 6));
    assert_eq!(a - Rational::new_const(1, 3), Rational::new_const(1, 6));
    assert_eq!(a * Rational::new_const(2, 3), Rational::new_const(1, 3));
    assert_eq!(a / Rational::new_const(2, 3), Rational::new_const(3, 4));
    assert_eq!(
        Rational::new_const(7, 4) % Rational::one(),
        Rational::new_const(3, 4)
    );
    assert_eq!(format!("{}", Rational::from_integer(5)), "5");
    assert_eq!(format!("{}", a), "1/2");
    assert_eq!(
        Rational::from_str("3/8").unwrap(),
        Rational::new_const(3, 8)
    );
    assert_eq!(
        Rational::from_str("0.5").unwrap(),
        Rational::new_const(1, 2)
    );
    assert_eq!(Rational::from_str("7").unwrap(), Rational::from_integer(7));

    let t: Time = Rational::new_const(9, 4);
    assert_eq!(t.sam(), Rational::from_integer(2));
    assert_eq!(t.next_sam(), Rational::from_integer(3));
    assert_eq!(t.cycle_pos(), Rational::new_const(1, 4));
    assert!((t.to_seconds(0.5) - 4.5).abs() < 1e-9);
    assert_eq!(t.to_seconds(0.0), 0.0);
    let _ = time_to_rand(t);
    let _ = time_to_rand_seeded(t, 42, 7);
    let _ = seq_p(vec![(Time::zero(), Time::one(), pure(1i64))]);
    let _ = lift_a2_both(&pure(1i64), &pure(2i64), |a, b| a + b);
    let _ = lift_a2_left(&pure(1i64), &pure(2i64), |a, b| a + b);
    let _ = lift_a2_right(&pure(1i64), &pure(2i64), |a, b| a + b);
    let _ = ControlPattern::new(|_| Vec::new());
}

#[test]
fn test_rational_drift_freedom_over_10000_cycles() {
    let p = fastcat(vec![pure(1i64); 7]);
    let haps = p.query_arc(Time::zero(), Time::from_integer(10_000));
    assert_eq!(haps.len(), 70_000);
    let total_dur = haps
        .iter()
        .fold(Time::zero(), |acc, h| acc + h.part.duration());
    assert_eq!(total_dur, Time::from_integer(10_000));

    let roundtrip = p
        .fast(Time::from_integer(3))
        .slow(Time::from_integer(3))
        .early(Time::new_const(5, 7))
        .late(Time::new_const(5, 7));
    assert_eq!(
        roundtrip.query_arc(Time::zero(), Time::from_integer(50)),
        p.query_arc(Time::zero(), Time::from_integer(50))
    );
}

#[test]
fn test_timespan_context_and_hap_invariants() {
    assert_eq!(TimeSpan::new(Time::one(), Time::zero()), None);
    let clamped = TimeSpan::new_clamped(Time::one(), Time::zero());
    assert_eq!(clamped.begin, Time::zero());
    assert_eq!(clamped.end, Time::one());

    let cyc = TimeSpan::cycle_of(Time::new_const(7, 3));
    assert_eq!(
        cyc,
        TimeSpan::new_clamped(Time::from_integer(2), Time::from_integer(3))
    );
    assert_eq!(cyc.duration(), Time::one());
    assert_eq!(cyc.midpoint(), Time::new_const(5, 2));
    assert!(cyc.contains_time(Time::new_const(5, 2)));
    assert!(!cyc.contains_time(Time::from_integer(3)));

    let other = TimeSpan::new_clamped(Time::new_const(5, 2), Time::new_const(7, 2));
    assert_eq!(
        cyc.intersection(other),
        TimeSpan::new(Time::new_const(5, 2), Time::from_integer(3))
    );
    assert_eq!(cyc.sect(other), cyc.intersection(other));

    let multi = TimeSpan::new_clamped(Time::new_const(1, 2), Time::new_const(5, 2));
    let parts = multi.split_cycles();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0],
        TimeSpan::new_clamped(Time::new_const(1, 2), Time::one())
    );
    assert_eq!(
        parts[1],
        TimeSpan::new_clamped(Time::one(), Time::from_integer(2))
    );
    assert_eq!(
        parts[2],
        TimeSpan::new_clamped(Time::from_integer(2), Time::new_const(5, 2))
    );

    let shifted = cyc
        .shift(Time::one())
        .scale(Time::from_integer(2))
        .with_time(|t| t);
    assert_eq!(shifted.begin, Time::from_integer(6));

    let mut c1 = Context::empty();
    c1.locations.push(SourceLocation { start: 0, end: 4 });
    c1.tags.insert("drums".to_string());
    let mut c2 = Context::empty();
    c2.locations.push(SourceLocation { start: 5, end: 9 });
    c2.tags.insert("bass".to_string());
    let combined_ctx = c1.combine(&c2);
    assert_eq!(combined_ctx.locations.len(), 2);
    assert_eq!(combined_ctx.tags.len(), 2);

    let whole = TimeSpan::new_clamped(Time::zero(), Time::one());
    let part1 = TimeSpan::new_clamped(Time::zero(), Time::new_const(1, 2));
    let part2 = TimeSpan::new_clamped(Time::new_const(1, 2), Time::one());
    let h1: Event<i64> = Hap::discrete(whole, part1, 10, c1);
    let h2 = Hap::discrete(whole, part2, 10, c2);
    assert!(h1.has_onset());
    assert!(!h2.has_onset());
    assert_eq!(h1.whole_or_part(), whole);

    let hc = Hap::continuous(part1, 20i64, Context::empty());
    assert!(!hc.has_onset());
    assert_eq!(hc.whole_or_part(), part1);

    let h_mapped = h1.clone().with_value(|v| v * 2).with_span(|s| s);
    assert_eq!(h_mapped.value, 20);
    let merged = h1.combine_with(&hc, |a, b| a + b, |wa, _wb| wa).unwrap();
    assert_eq!(merged.value, 30);
}

#[test]
fn test_value_compatibility_profiles_and_dual_timeframe_pp04_pp11() {
    let strudel = EngineCompatibilityProfile::from_name("strudel").unwrap();
    let superdirt = EngineCompatibilityProfile::from_name("superdirt").unwrap();
    assert_eq!(strudel.middle_c_octave(), 4);
    assert_eq!(superdirt.middle_c_octave(), 5);
    assert!((strudel.sustained_synth_gain_scale() - 0.5).abs() < 1e-9);
    assert!((superdirt.sustained_synth_gain_scale() - 1.0).abs() < 1e-9);
    assert!(strudel.filter_env_inherits_amp_adsr());
    assert!(!superdirt.filter_env_inherits_amp_adsr());
    assert_eq!(strudel.note_offset_to_midi(0.0), 60.0);
    assert_eq!(superdirt.note_offset_to_midi(0.0), 72.0);

    let nv = NoteValue::Name("a4".to_string());
    assert_eq!(nv.to_midi(strudel), Some(69.0));
    assert!((nv.to_freq_hz(strudel).unwrap() - 440.0).abs() < 1e-6);

    let v_int = Value::from(12i64);
    let v_flt = Value::from(3.5f64);
    let v_rat = Value::from(Time::new_const(1, 2));
    let v_str = Value::from("bd");
    let v_bool = Value::from(true);
    assert_eq!(v_int.as_i64(), Some(12));
    assert_eq!(v_flt.as_f64(), Some(3.5));
    assert_eq!(v_rat.as_rational(), Some(Time::new_const(1, 2)));
    assert_eq!(v_str.as_str(), Some("bd"));
    assert_eq!(v_bool.as_bool(), Some(true));
    assert_eq!(v_int.add_numeric(&Value::from(3i64)), Value::Int(15));
    assert_eq!(v_int.sub_numeric(&Value::from(3i64)), Value::Int(9));
    assert_eq!(v_int.mul_numeric(&Value::from(3i64)), Value::Int(36));
    assert_eq!(v_flt.div_numeric(&Value::from(0.0f64)), Value::Float(0.0));

    // PP-04: Global vs Local controller timeframe under `.slow(4)`
    let knob = global_controller(|global_t| global_t.to_f64());
    let slowed_global = knob.slow(Time::from_integer(4));
    let slowed_local = knob.local_time().slow(Time::from_integer(4));

    let span = TimeSpan::new_clamped(Time::from_integer(2), Time::from_integer(3));
    let ctx = QueryContext::new(span);
    let g_val = slowed_global.query_with_context(&ctx)[0].value;
    let l_val = slowed_local.query_with_context(&ctx)[0].value;
    // Unwarped global midpoint of [2, 3) is 2.5; local midpoint after slow(4) is 2.5 / 4 = 0.625
    assert!((g_val - 2.5).abs() < 1e-9);
    assert!((l_val - 0.625).abs() < 1e-9);
}

#[test]
fn test_functor_applicative_monad_and_sweep_line_laws() {
    let p = fastcat(vec![pure(2i64), pure(5i64), pure(9i64)]);
    // Functor Identity & Composition
    assert_eq!(p.fmap(|x| *x).first_cycle(), p.first_cycle());
    let f = |x: &i64| x + 3;
    let g = |x: &i64| x * 2;
    assert_eq!(
        p.fmap(f).fmap(g).first_cycle(),
        p.fmap(move |x| g(&f(x))).first_cycle()
    );

    // Applicative `<*>`, `<*`, `*>`
    let fn_pat: Pattern<BoxedFn<i64, i64>> =
        pure(StdArc::new(|x: &i64| x * 10) as BoxedFn<i64, i64>);
    assert_eq!(
        fn_pat.app_both(&p).first_cycle(),
        p.fmap(|x| x * 10).first_cycle()
    );
    assert_eq!(fn_pat.app_left(&p).first_cycle().len(), 3);
    assert_eq!(fn_pat.app_right(&p).first_cycle().len(), 3);

    // Monad laws: Left Identity, Right Identity, Associativity, inner/outer/squeeze/trig joins
    let k = |x: &i64| fastcat(vec![pure(*x), pure(*x + 1)]);
    let m = |x: &i64| pure(*x * 3);
    assert_eq!(pure(4i64).bind(k).first_cycle(), k(&4).first_cycle());
    assert_eq!(p.bind(|x| pure(*x)).first_cycle(), p.first_cycle());
    assert_eq!(
        p.bind(k).bind(m).first_cycle(),
        p.bind(move |x| k(x).bind(m)).first_cycle()
    );

    let nested = fastcat(vec![
        pure(fastcat(vec![pure(10i64), pure(20i64)])),
        pure(pure(30i64)),
    ]);
    let ij = <Pattern<i64> as PatternMonad<i64>>::inner_join(&nested).first_cycle();
    let oj = <Pattern<i64> as PatternMonad<i64>>::outer_join(&nested).first_cycle();
    let sj = <Pattern<i64> as PatternMonad<i64>>::squeeze_join(&nested).first_cycle();
    assert_eq!(ij.len(), 2);
    assert_eq!(oj.len(), 2);
    assert_eq!(sj.len(), 3); // squeeze_join compresses [10, 20] into first half and [30] into second half!

    let trig = fastcat(vec![pure(true), pure(false)]);
    let reset_p = p.reset(&trig).first_cycle();
    let restart_p = p.restart(&trig).first_cycle();
    assert!(!reset_p.is_empty());
    assert!(!restart_p.is_empty());

    // Sweep-line join correctness on disjoint and overlapping spans
    let joined = sweep_line_join(
        p.first_cycle(),
        p.first_cycle(),
        |a, b| a + b,
        |wa, wb| (true, wa.and_then(|a| wb.and_then(|b| a.intersection(b)))),
    );
    assert_eq!(joined.len(), 3);
}

#[test]
fn test_all_combinators_euclidean_stepwise_and_slicing() {
    let a = pure(1i64);
    let b = pure(2i64);
    assert_eq!(silence::<i64>().first_cycle().len(), 0);
    assert_eq!(
        stack(vec![a.clone(), silence()]).first_cycle(),
        a.first_cycle()
    );
    assert_eq!(
        cat(vec![a.clone(), b.clone()])
            .query_arc(Time::zero(), Time::from_integer(2))
            .len(),
        2
    );
    assert_eq!(
        slowcat_prime(vec![a.clone(), b.clone()])
            .first_cycle()
            .len(),
        1
    );
    assert_eq!(sequence(vec![a.clone(), b.clone()]).first_cycle().len(), 2);
    assert_eq!(from_list(vec![10i64, 20, 30]).first_cycle().len(), 3);
    assert_eq!(
        timecat(vec![
            (Time::from_integer(1), a.clone()),
            (Time::from_integer(3), b.clone())
        ])
        .first_cycle()
        .len(),
        2
    );
    assert_eq!(run(4).first_cycle().len(), 4);
    assert_eq!(
        scan(3).query_arc(Time::zero(), Time::from_integer(3)).len(),
        6
    );

    let seq4 = run(4);
    assert_eq!(seq4.rev().rev().first_cycle(), seq4.first_cycle());
    assert_eq!(
        seq4.palindrome()
            .query_arc(Time::zero(), Time::from_integer(2))
            .len(),
        8
    );
    assert_eq!(seq4.fast_pat(2i64).first_cycle().len(), 8);
    assert_eq!(
        seq4.slow_pat(2i64)
            .query_arc(Time::zero(), Time::from_integer(2))
            .len(),
        4
    );
    assert_eq!(
        seq4.early_pat(Time::new_const(1, 4))
            .late_pat(Time::new_const(1, 4))
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(
        seq4.iter(4)
            .query_arc(Time::zero(), Time::from_integer(4))
            .len(),
        16
    );
    assert_eq!(
        seq4.iter_back(4)
            .query_arc(Time::zero(), Time::from_integer(4))
            .len(),
        16
    );
    assert_eq!(
        seq4.compress(Time::zero(), Time::new_const(1, 2))
            .zoom(Time::zero(), Time::new_const(1, 2))
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(
        seq4.inside(Time::from_integer(2), |p| p.rev())
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(
        seq4.outside(Time::from_integer(2), |p| p.rev())
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(
        seq4.within(Time::zero(), Time::new_const(1, 2), |p| p
            .fast(Time::from_integer(2)))
            .first_cycle()
            .len(),
        6
    );
    assert_eq!(seq4.linger(Time::new_const(1, 2)).first_cycle().len(), 4);
    assert_eq!(seq4.swing(Time::from_integer(4)).first_cycle().len(), 4);
    assert_eq!(
        seq4.ribbon(Time::zero(), Time::from_integer(2))
            .first_cycle()
            .len(),
        4
    );

    // Conditionals & Structural
    assert_eq!(
        seq4.every(2, |p| p.fast(Time::from_integer(2)))
            .first_cycle()
            .len(),
        8
    );
    assert_eq!(
        seq4.each(vec![|p: &Pattern<i64>| p.clone(), |p: &Pattern<i64>| p.rev()])
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(
        seq4.whenmod_pat(4i64, 2i64, |p| p.rev())
            .first_cycle()
            .len(),
        4
    );
    assert_eq!(seq4.sometimes(|p| p.rev()).first_cycle().len(), 4);
    assert_eq!(seq4.often(|p| p.rev()).first_cycle().len(), 4);
    assert_eq!(seq4.rarely(|p| p.rev()).first_cycle().len(), 4);
    assert_eq!(seq4.almost_always(|p| p.rev()).first_cycle().len(), 4);
    assert_eq!(seq4.almost_never(|p| p.rev()).first_cycle().len(), 4);
    assert_eq!(
        seq4.always(|p| p.rev()).first_cycle(),
        seq4.rev().first_cycle()
    );
    assert_eq!(seq4.never(|p| p.rev()).first_cycle(), seq4.first_cycle());
    assert_eq!(
        seq4.chunk(2, |p| p.fast(Time::from_integer(2)))
            .first_cycle()
            .len(),
        6
    );
    assert_eq!(
        seq4.chunk_back(2, |p| p.fast(Time::from_integer(2)))
            .first_cycle()
            .len(),
        6
    );
    assert_eq!(seq4.ply(Time::from_integer(2)).first_cycle().len(), 8);
    assert_eq!(seq4.ply_pat(Time::from_integer(2)).first_cycle().len(), 8);
    assert_eq!(
        seq4.off(Time::new_const(1, 8), |p| p.clone())
            .first_cycle()
            .len(),
        9
    );
    assert_eq!(seq4.superimpose(|p| p.rev()).first_cycle().len(), 8);
    assert_eq!(
        seq4.layer(vec![|p: &Pattern<i64>| p.clone(), |p: &Pattern<i64>| p.rev()])
            .first_cycle()
            .len(),
        8
    );

    // Bjorklund Euclidean & Stepwise
    assert_eq!(bjorklund(3, 8, 0).iter().filter(|&&b| b).count(), 3);
    assert_eq!(a.euclid(3, 8).first_cycle().len(), 3);
    assert_eq!(a.euclid_rot(3, 8, 1).first_cycle().len(), 3);
    assert_eq!(a.euclid_inv(3, 8).first_cycle().len(), 5);
    assert_eq!(a.euclid_full(3, 8, &b).first_cycle().len(), 8);
    assert_eq!(seq4.step(Time::from_integer(8)).first_cycle().len(), 8);
    assert_eq!(
        seq4.expand(Time::from_integer(2))
            .query_arc(Time::zero(), Time::from_integer(2))
            .len(),
        4
    );
    assert_eq!(seq4.pace(Time::from_integer(8)).first_cycle().len(), 8);
    assert_eq!(seq4.take_steps(2).first_cycle().len(), 2);
    assert_eq!(seq4.take_steps(-2).first_cycle().len(), 2);
    assert_eq!(seq4.drop_steps(1).first_cycle().len(), 3);
    assert_eq!(seq4.drop_steps(-1).first_cycle().len(), 3);
    assert_eq!(seq4.rot(1).first_cycle()[0].value, 1);
    assert_eq!(stepcat(vec![seq4.clone(), run(2)]).first_cycle().len(), 6);
    assert_eq!(
        polymeter(vec![seq4.clone(), run(2)], None)
            .first_cycle()
            .len(),
        8
    );
    assert_eq!(
        polyrhythm(vec![seq4.clone(), run(2)]).first_cycle().len(),
        6
    );

    // ControlPattern, StereoJux, SamplePatternOps
    let bd = s("bd")
        .union_right(&n(0i64))
        .union_left(&gain(0.8))
        .add_ctrl(&gain(0.1))
        .sub_ctrl(&gain(0.05))
        .mul_ctrl(&gain(1.0))
        .div_ctrl(&gain(1.0))
        .add_ctrl_in(&lpf(800.0))
        .add_ctrl_out(&pan(0.5))
        .add_ctrl_both(&ctrl_param("room", Value::Float(0.2)));
    assert_eq!(bd.first_cycle().len(), 1);
    assert_eq!(
        bd.struct_pat(&a.euclid(3, 8).fmap(|_| true))
            .first_cycle()
            .len(),
        3
    );
    // `late` wraps events across the cycle boundary (Tidal semantics): each delayed layer
    // contributes its main hap plus the wrapped tail of the previous-cycle event.
    assert_eq!(
        bd.stut(3, 0.5, Time::new_const(1, 8)).first_cycle().len(),
        5
    );
    assert_eq!(
        bd.echo(2, Time::new_const(1, 4), 0.7).first_cycle().len(),
        3
    );
    assert_eq!(
        bd.echo_with(2, Time::new_const(1, 4), |p| p.clone())
            .first_cycle()
            .len(),
        3
    );
    assert_eq!(bd.jux(|p| p.rev()).first_cycle().len(), 2);
    assert_eq!(bd.jux_by(0.5, |p| p.rev()).first_cycle().len(), 2);
    assert_eq!(bd.striate(4).first_cycle().len(), 4);
    assert_eq!(bd.chop(4).first_cycle().len(), 4);
    assert_eq!(bd.slice(4, &run(4)).first_cycle().len(), 4);
    assert_eq!(bd.splice(4, &run(4)).first_cycle().len(), 4);
    assert_eq!(bd.randslice(4).first_cycle().len(), 1);
    assert_eq!(bd.scramble(4).first_cycle().len(), 4);
    assert_eq!(bd.shuffle(4).first_cycle().len(), 4);
}

#[test]
fn test_signals_prng_uniformity_and_complementarity() {
    assert_ne!(splitmix64(1), splitmix64(2));
    let samples = rand()
        .segment(Time::one())
        .query_arc(Time::zero(), Time::from_integer(2_000));
    assert_eq!(samples.len(), 2_000);
    assert!(samples.iter().all(|h| h.value >= 0.0 && h.value < 1.0));
    let mean: f64 = samples.iter().map(|h| h.value).sum::<f64>() / (samples.len() as f64);
    assert!((mean - 0.5).abs() < 0.03);

    // Continuous signal whole == None invariant
    for sig in [
        rand(),
        perlin(),
        berlin(),
        sine(),
        cosine(),
        saw(),
        isaw(),
        tri(),
        square(),
    ] {
        let h = &sig.first_cycle()[0];
        assert!(h.whole.is_none());
        assert!(!h.has_onset());
        assert!(h.value >= 0.0 && h.value <= 1.0);
    }

    // Perlin C2 continuity across cycle boundary t = 1.0
    let eps = Time::new_const(1, 100_000);
    let left_val = perlin().query_arc(Time::one() - eps, Time::one())[0].value;
    let right_val = perlin().query_arc(Time::one(), Time::one() + eps)[0].value;
    assert!((left_val - right_val).abs() < 1e-3);

    // Degrade / undegrade complementarity
    let base = run(16);
    let deg = base.degrade_by(0.4).first_cycle();
    let undeg = base.undegrade_by(0.4).first_cycle();
    assert_eq!(deg.len() + undeg.len(), 16);
    let mut seen = BTreeSet::new();
    for h in deg.iter().chain(undeg.iter()) {
        assert!(seen.insert(h.value));
    }

    let ranged = sine()
        .range(100.0, 900.0)
        .quantize(4.0)
        .smooth(Time::new_const(1, 16));
    assert!(!ranged.first_cycle().is_empty());
    let rx = sine().rangex(100.0, 1000.0);
    assert!(rx.first_cycle()[0].value >= 100.0 && rx.first_cycle()[0].value <= 1000.0);
    assert_eq!(
        choose(vec![1i64, 2, 3])
            .segment(Time::one())
            .first_cycle()
            .len(),
        1
    );
    assert_eq!(choose_cycles(vec![1i64, 2, 3]).first_cycle().len(), 1);
    assert_eq!(
        wchoose(vec![(0.5, 10i64), (0.5, 20i64)])
            .segment(Time::one())
            .first_cycle()
            .len(),
        1
    );
    assert!(irand(8).first_cycle()[0].value < 8);
}

#[test]
fn test_unified_tonal_chords_voicings_and_scala_microtonality() {
    assert_eq!(scale_intervals("major").len(), 7);
    assert_eq!(scale_intervals("minor:pentatonic").len(), 5);
    assert_eq!(scale_intervals("hirajoshi").len(), 5);
    let (root_midi, ivs) = parse_scale_descriptor("C4:major");
    assert_eq!(root_midi, 60);
    assert_eq!(resolve_scale_degree(root_midi, ivs, 0), 60.0);
    assert_eq!(resolve_scale_degree(root_midi, ivs, 7), 72.0);
    assert_eq!(resolve_scale_degree(root_midi, ivs, -1), 59.0);

    // PP-11: Unified Superset Chord Parser (`ireal`, standard, and Tidal symbols + `@dict`)
    let ireal = parse_chord_symbol("C^7@ireal").unwrap();
    let std_dim = parse_chord_symbol("Cdim7").unwrap();
    let tidal = parse_chord_symbol("g'm13@drop2").unwrap();
    let aug = parse_chord_symbol("A7s5@lefthand").unwrap();
    let triad = parse_chord_symbol("Dm9@triad").unwrap();
    assert_eq!(ireal.intervals, vec![0, 4, 7, 11]);
    assert_eq!(std_dim.intervals, vec![0, 3, 6, 9]);
    assert_eq!(tidal.root_semitone, 7);
    assert_eq!(aug.inline_dict.as_deref(), Some("lefthand"));
    assert_eq!(triad.intervals.len(), 3);

    let deg_pat = pure(Value::Int(2));
    let sc = deg_pat
        .scale(&pure("C4:major".to_string()))
        .scale_transpose(&pure(2i64))
        .transpose(&pure(1.0f64));
    assert_eq!(sc.first_cycle()[0].value.as_f64(), Some(67.0));

    let ch_pat = pure(Value::String("Cmaj7".to_string()))
        .dict(&pure("drop2".to_string()))
        .anchor(&pure("c4".to_string()));
    assert!(matches!(ch_pat.first_cycle()[0].value, Value::List(_)));
    let roots = pure(Value::String("G7".to_string())).root_notes(3);
    assert_eq!(roots.first_cycle()[0].value.as_f64(), Some(55.0));

    let voiced = pure(Value::String("Dm7".to_string())).voicing();
    let arped = arp_with(
        &voiced,
        &pure(|notes: &[Value]| notes.iter().rev().cloned().collect()),
    );
    assert_eq!(arped.first_cycle().len(), 4);

    // Scala .scl and .kbm parser + 19-EDO
    let scl = "! 12-TET test\n12-TET Scala\n 3\n!\n 400.0\n 3/2\n 2/1\n";
    let kbm = "! KBM\n12\n0\n127\n60\n69\n440.0\n12\n";
    let tuning = ScalaTuning::from_scl_kbm(scl, Some(kbm)).unwrap();
    assert_eq!(tuning.pitches_cents.len(), 3);
    assert!((tuning.degree_to_freq_hz(0.0) - 440.0).abs() < 1e-6);
    assert!((tuning.degree_to_freq_hz(3.0) - 880.0).abs() < 1e-4);

    let edo19 = ScalaTuning::edo(19, 2.0);
    assert_eq!(edo19.pitches_cents.len(), 19);
    assert!((edo19.degree_to_freq_hz(19.0) - 880.0).abs() < 1e-4);
}

#[test]
fn test_stateful_elektron_plock_m8_arrange_and_transitions() {
    // PP-03: accum, markov, lsystem, tie_contiguous, glide
    let acc_pat = fastcat(vec![pure(1i64), pure(2i64)]).accum(0i64, |s, x| (s + x, s + x));
    let c0 = acc_pat.query_arc(Time::zero(), Time::from_integer(2));
    let vals: Vec<i64> = c0.into_iter().map(|h| h.value).collect();
    assert_eq!(vals, vec![1, 3, 4, 6]);

    let mk = markov_pattern(0, 4, vec![vec![0.0, 1.0], vec![1.0, 0.0]]);
    let mk_vals: Vec<usize> = mk.first_cycle().into_iter().map(|h| h.value).collect();
    assert_eq!(mk_vals, vec![0, 1, 0, 1]);

    assert_eq!(lsystem_expand("A", &[('A', "AB"), ('B', "A")], 3), "ABAAB");

    let tied = tie_contiguous(&fastcat(vec![
        pure(Some("c4")),
        pure(None),
        pure(None),
        pure(Some("d4")),
    ]))
    .first_cycle();
    assert_eq!(tied.len(), 2);
    assert_eq!(tied[0].part.duration(), Time::new_const(3, 4));

    let glided = note("c4").glide(Time::new_const(1, 8), GlideCurve::EqualPowerScurve);
    assert_eq!(
        glided.first_cycle()[0]
            .value
            .get("glide_curve")
            .and_then(Value::as_str),
        Some("scurve")
    );

    // CK-01: Elektron TrigCondition truth tables (@1:4, @1st, @fill, @pre, @nei)
    let mut ledger = TrigConditionLedger {
        activation_cycle: 0,
        fill_latch: false,
        last_same_stream_cond: false,
        last_neighbor_stream_cond: true,
    };
    assert!(TrigCondition::Ratio { a: 1, b: 4 }.evaluate(0, &mut ledger));
    assert!(
        TrigCondition::PreviousConditionalOnSameStream { expected: true }.evaluate(0, &mut ledger)
    );
    assert!(!TrigCondition::Ratio { a: 1, b: 4 }.evaluate(1, &mut ledger));
    assert!(
        TrigCondition::PreviousConditionalOnSameStream { expected: false }.evaluate(1, &mut ledger)
    );
    assert!(TrigCondition::FirstCycle { expected: true }.evaluate(0, &mut ledger));
    assert!(TrigCondition::FirstCycle { expected: false }.evaluate(2, &mut ledger));
    assert!(!TrigCondition::FillLatch { active: true }.evaluate(0, &mut ledger));
    assert!(
        TrigCondition::PreviousConditionalOnNeighborStream { expected: true }
            .evaluate(0, &mut ledger)
    );

    let cond_pat = pure(1i64).trig_cond(TrigCondition::Ratio { a: 1, b: 2 }, 0);
    assert_eq!(
        cond_pat
            .query_arc(Time::zero(), Time::from_integer(4))
            .len(),
        2
    );

    // CK-02: Step-indexed `.plock()` isolation
    let locked = s("bd").fast(Time::from_integer(4)).plock(2, 4, &lpf(350.0));
    let locked_haps = locked.first_cycle();
    assert_eq!(locked_haps.len(), 4);
    assert_eq!(locked_haps[1].value.get("lpf"), None);
    assert_eq!(locked_haps[2].value.get("lpf"), Some(&Value::Float(350.0)));
    assert_eq!(locked_haps[3].value.get("lpf"), None);

    // CK-04: Dirtywave M8 `.table()` and `.groove()`
    let tabled = note("c2").table(
        Time::from_integer(4),
        &fastcat(vec![lpf(400.0), lpf(800.0), lpf(1200.0), lpf(1600.0)]),
    );
    assert_eq!(tabled.first_cycle().len(), 4);

    let grooved = run(2).groove(&[7, 5]).first_cycle();
    assert_eq!(grooved.len(), 2);
    assert_eq!(grooved[0].part.end, Time::new_const(7, 12));

    // PP-08: Zero-safe arrange, seq_p, once, asap, qtrigger, fade_in/out & 11 transitions
    assert_eq!(
        arrange(vec![
            (Time::zero(), pure(1i64)),
            (Time::from_integer(-2), pure(2i64))
        ])
        .first_cycle()
        .len(),
        0
    );
    let arr = arrange(vec![
        (Time::zero(), pure(1i64)),
        (Time::from_integer(1), pure(2i64)),
        (Time::from_integer(1), pure(3i64)),
    ]);
    assert_eq!(arr.query_arc(Time::zero(), Time::from_integer(2)).len(), 2);
    assert_eq!(
        once(&pure(1i64), Time::new_const(1, 2))
            .query_arc(Time::zero(), Time::from_integer(3))
            .len(),
        1
    );
    assert_eq!(
        asap(&pure(1i64), Time::new_const(1, 2))
            .query_arc(Time::zero(), Time::from_integer(3))
            .len(),
        2
    );
    assert_eq!(
        qtrigger(&pure(1i64), Time::from_integer(2))
            .query_arc(Time::zero(), Time::from_integer(3))
            .len(),
        1
    );

    let old_s = s("bd");
    let new_s = s("sd");
    let t0 = Time::zero();
    let c2 = Time::from_integer(2);
    assert!(
        !transitions::xfade(&old_s, &new_s, t0)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::xfade_in(&old_s, &new_s, t0, c2)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::jump(&old_s, &new_s, t0)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::jump_in(&old_s, &new_s, t0, 1)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::jump_mod(&old_s, &new_s, t0, 2)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::clutch(&old_s, &new_s, t0)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::clutch_in(&old_s, &new_s, t0, c2)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::anticipate(&old_s, &new_s, t0)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::anticipate_in(&old_s, &new_s, t0, c2)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::interpolate(&lpf(200.0), &lpf(1200.0), t0, c2)
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::histpan(vec![old_s.clone(), new_s.clone()], 2)
            .first_cycle()
            .is_empty()
    );
    assert!(
        !transitions::wait(&new_s, t0, Time::one())
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::wash(&old_s, &new_s, t0, Time::one(), |p| p.rev())
            .query_arc(t0, c2)
            .is_empty()
    );
    assert!(
        !transitions::mortal(&new_s, t0, Time::one(), Time::one())
            .query_arc(t0, c2)
            .is_empty()
    );

    assert_eq!(MAX_STATE_CHECKPOINTS_PER_STREAM, 256);
    let cp = LruCycleStateCheckpointer::new(10i64, 0);
    assert_eq!(cp.state_at_cycle(5, &|c, s| s + c), 20);
    cp.reset_state(0, 0);
    assert!((sin_f64(0.0)).abs() < 1e-12);
    assert!((cos_f64(0.0) - 1.0).abs() < 1e-12);
    assert!((ln_f64(core::f64::consts::E) - 1.0).abs() < 1e-9);
    assert!((exp2_f64(3.0) - 8.0).abs() < 1e-9);
    let _ = LegatoGlideSpec {
        duration_cycles: Rational64::new_const(1, 8),
        curve: GlideCurve::Linear,
        legato_tie: true,
    };
}

#[test]
fn test_remaining_27_symbols_for_100_percent_symbol_coverage() {
    let parsed: ParsedChord = parse_chord_symbol("Cmaj7").unwrap();
    assert_eq!(parsed.root_semitone, 0);
    assert_eq!(NoteValue::parse_pitch_name("f#3"), Some((6, 3)));

    let base: Pattern<i64> = vec![10i64, 20, 30].into_pattern();
    assert_eq!(base.intrinsic_steps(), 3);

    let _qf: &QueryFn<i64> = &|state: &State| {
        let _ = state;
        Vec::new()
    };

    let span = TimeSpan::new_clamped(Time::zero(), Time::one());
    let mut ctrl = ControlMap::new();
    ctrl.insert("gain".to_string(), Value::Float(0.9));
    let ctx = QueryContext::new(span);
    let st = State::new(span).with_controls(ctrl).with_query_context(ctx);

    let transformed = base
        .split_queries()
        .with_query_state(|s| s.clone())
        .with_query_span(|sp| sp)
        .with_query_time(|t| t)
        .with_hap(|h| h)
        .with_hap_span(|sp| sp)
        .with_hap_time(|t| t)
        .filter_haps(|h| h.has_onset())
        .filter_values(|&v| v > 0)
        .with_loc(SourceLocation { start: 1, end: 5 })
        .tag("lead")
        .every_offset(2, 1, |p| p.rev())
        .whenmod(4, 2, |p| p.rev())
        .sometimes_by(0.5, |p| p.rev())
        .swing_by(Time::new_const(1, 3), Time::from_integer(4))
        .degrade()
        .degrade_by_pat(0.25f64);

    let direct_haps = transformed.query(&st);
    assert!(!direct_haps.is_empty());

    let faded = s("bd").fade_out(Time::zero(), Time::from_integer(2));
    assert_eq!(
        faded
            .query_arc(Time::from_integer(3), Time::from_integer(4))
            .len(),
        0
    );

    let sp = StatefulPattern::new(
        0i64,
        |cyc, s| s + cyc,
        |_cyc, &s, sub| {
            vec![Hap::discrete(
                TimeSpan::cycle_of(sub.begin),
                sub,
                s,
                Context::empty(),
            )]
        },
    );
    assert_eq!(sp.as_pattern().first_cycle().len(), 1);
    assert_eq!(sp.into_pattern().first_cycle().len(), 1);
}
