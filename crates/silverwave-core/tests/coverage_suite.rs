//! Targeted line-coverage suite for previously uncovered branches in `silverwave-core`.
//! Exercises guard clauses, rarely-used combinator paths, tonal parsing tables,
//! Scala SCL/KBM edge cases, and algebraic join fallback branches.

use std::collections::HashSet;

use silverwave_core::{
    Context, ControlMap, ControlPattern, CycleStateCheckpointer, EngineCompatibilityProfile,
    GlideCurve, Hap, IntoPattern, LruCycleStateCheckpointer, NoteValue, Pattern, PatternMonad,
    PatternTrigJoin, QueryContext, Rational, SamplePatternOps, ScalaTuning, SignalRange,
    SourceLocation, State, StatefulPattern, Time, TimeSpan, TonalExt, TrigCondition,
    TrigConditionLedger, Value, arp_with, bjorklund, choose, exp2_f64, fastcat, irand,
    lift_a2_both, lift_a2_left, lift_a2_right, ln_f64, lpf, lsystem_expand, markov_pattern, note,
    once, parse_chord_symbol, parse_scale_descriptor, polymeter, pure, qtrigger, run, s,
    scale_intervals, scan, silence, sin_f64, sine, slowcat, slowcat_prime, stack, sweep_line_join,
    tie_contiguous, time_to_rand, timecat, transitions, wchoose,
};

fn t(n: i64, d: i64) -> Time {
    Time::new_const(n, d)
}

fn ti(n: i64) -> Time {
    Time::from_integer(n)
}

fn sp(b: Time, e: Time) -> TimeSpan {
    TimeSpan::new_clamped(b, e)
}

fn dhap<T>(w: TimeSpan, p: TimeSpan, v: T) -> Hap<T> {
    Hap::discrete(w, p, v, Context::empty())
}

#[test]
fn rational_edge_branches() {
    assert_eq!(Rational::new_const(5, 0), Rational::zero());
    let neg_den = Rational::new_const(3, -4);
    assert_eq!(neg_den, Rational::new_const(-3, 4));
    assert_eq!(Rational::checked_op_stern_brocot(7, 0), Rational::zero());
    // Stern-Brocot convergent path where the partial-quotient bound still permits k > 0.
    let conv = Rational::checked_op_stern_brocot(1, 2 * (Rational::MAX_DENOMINATOR as i128) + 1);
    assert!(conv.denom() <= Rational::MAX_DENOMINATOR);
    assert!(conv.to_f64() > 0.0);
    // Stern-Brocot path where no further step fits (`k == 0`): F46/F45 has an all-1s
    // continued fraction whose convergent denominator crosses MAX_DENOMINATOR exactly when
    // `q0 + q1 > MAX_DENOMINATOR`, so `k_q = 0` and the loop exits without updating (236-239).
    let phi = Rational::checked_op_stern_brocot(1_836_311_903, 1_134_903_170);
    assert!(phi.denom() <= Rational::MAX_DENOMINATOR);
    assert!((phi.to_f64() - core::f64::consts::GOLDEN_RATIO).abs() < 1e-6);
    assert_eq!(Rational::from_f64_approx(-0.5, 1000), Rational::new(-1, 2));
    assert_eq!(Rational::default(), Rational::zero());
    let mut hs = HashSet::new();
    hs.insert(Rational::new_const(3, 8));
    assert!(hs.contains(&Rational::new_const(6, 16)));
    assert_eq!(Rational::one() / Rational::zero(), Rational::zero());
    assert_eq!(
        Rational::new_const(5, 2) % Rational::zero(),
        Rational::zero()
    );
    let from_int: Rational = 5i64.into();
    assert_eq!(from_int, Rational::from_integer(5));
    let from_pair: Rational = (3i64, 4i64).into();
    assert_eq!(from_pair, Rational::new_const(3, 4));
    assert!("x/2".parse::<Rational>().is_err());
    assert!("2/x".parse::<Rational>().is_err());
    assert!("1.2.3".parse::<Rational>().is_err());
    assert!("0.5e999".parse::<Rational>().is_err());
}

#[test]
fn span_helper_edge_branches() {
    // new_clamped with equal endpoints (42-45).
    let eq = TimeSpan::new_clamped(Time::one(), Time::one());
    assert_eq!(eq.duration(), Time::new_const(1, 1_000_000));
    // with_time swap branch (109) and scale swap branch (130).
    let wt = sp(Time::zero(), Time::one()).with_time(|x| -x);
    assert_eq!(wt.begin, -Time::one());
    assert_eq!(wt.end, Time::zero());
    let sc = sp(Time::zero(), Time::one()).scale(Time::from_integer(-2));
    assert_eq!(sc.begin, Time::from_integer(-2));
    assert_eq!(sc.end, Time::zero());
}

#[test]
fn pattern_query_and_into_pattern_branches() {
    // query_arc invalid span -> empty (65).
    assert!(pure(1i64).query_arc(Time::one(), Time::zero()).is_empty());
    // split_queries with fewer global cycles than local cycles (92-94 fallback).
    let pat = pure(7i64).split_queries();
    let mut ctx = QueryContext::new(sp(Time::zero(), Time::one()));
    ctx.local_span = sp(Time::zero(), ti(3));
    let st = State {
        span: sp(Time::zero(), ti(3)),
        controls: ControlMap::new(),
        query_context: ctx,
    };
    assert_eq!(pat.query(&st).len(), 3);
    // split_queries with a degenerate `global_span` (begin >= end) yields an empty global
    // cycle list, so the `if let Some(&g_sub)` body is skipped for every sub (94).
    let mut empty_global_ctx = QueryContext::new(sp(Time::zero(), Time::one()));
    empty_global_ctx.global_span = TimeSpan {
        begin: ti(2),
        end: ti(1),
    };
    let empty_global_state = State {
        span: sp(Time::zero(), ti(2)),
        controls: ControlMap::new(),
        query_context: empty_global_ctx,
    };
    assert_eq!(
        pure(7i64).split_queries().query(&empty_global_state).len(),
        2
    );
    // with_query_span inverted result -> empty (120).
    let inv = pure(1i64).with_query_span(|s| TimeSpan {
        begin: s.end,
        end: s.begin,
    });
    assert!(inv.first_cycle().is_empty());
    // with_loc: second identical location is deduplicated (180-182).
    let loc = SourceLocation { start: 0, end: 2 };
    let located = pure(9i64).with_loc(loc).with_loc(loc);
    assert_eq!(located.first_cycle()[0].context.locations.len(), 1);
    // IntoPattern impls: &Pattern (239-241), usize (253-255), isize (260-262),
    // bool (274-276), String (302-304), empty Vec (317).
    let pref = pure(3usize);
    let by_ref: Pattern<usize> = (&pref).into_pattern();
    assert_eq!(by_ref.first_cycle()[0].value, 3);
    let p_usize: Pattern<usize> = 5usize.into_pattern();
    let p_isize: Pattern<isize> = (-2isize).into_pattern();
    let p_bool: Pattern<bool> = true.into_pattern();
    let p_string: Pattern<String> = String::from("bd").into_pattern();
    let p_empty: Pattern<i64> = Vec::<i64>::new().into_pattern();
    assert_eq!(p_usize.first_cycle()[0].value, 5);
    assert_eq!(p_isize.first_cycle()[0].value, -2);
    assert!(p_bool.first_cycle()[0].value);
    assert_eq!(p_string.first_cycle()[0].value, "bd");
    assert!(p_empty.first_cycle().is_empty());
}

#[test]
fn algebra_join_edge_branches() {
    // sweep_line_join: empty inputs (26).
    let haps = pure(1i64).first_cycle();
    let empty: Vec<Hap<i64>> = Vec::new();
    assert!(
        sweep_line_join(
            empty.clone(),
            haps.clone(),
            |a: &i64, b: &i64| a + b,
            |wa, _wb| (true, wa)
        )
        .is_empty()
    );
    assert!(
        sweep_line_join(haps, empty, |a: &i64, b: &i64| a + b, |wa, _wb| (true, wa)).is_empty()
    );
    // Active-interval pruning: right haps skipped by r_start and a right hap whose
    // part ends before l_hap.part.begin but sorts after r_start (intersection None, 65-66).
    let l = dhap(sp(t(1, 2), t(3, 5)), sp(t(1, 2), t(3, 5)), 1i64);
    let ra = dhap(
        sp(Time::zero(), t(1, 100)),
        sp(Time::zero(), t(1, 100)),
        10i64,
    );
    let rb = dhap(sp(t(1, 50), t(19, 20)), sp(t(1, 50), t(19, 20)), 20i64);
    let rc = dhap(sp(t(3, 100), t(1, 25)), sp(t(3, 100), t(1, 25)), 30i64);
    let joined = sweep_line_join(
        vec![l],
        vec![ra, rb, rc],
        |a, b| a + b,
        |_wa, _wb| (true, None),
    );
    assert_eq!(joined.len(), 1);
    // valid = false from resolve_whole (58).
    let both = dhap(sp(Time::zero(), t(1, 2)), sp(Time::zero(), t(1, 2)), 1i64);
    assert!(
        sweep_line_join(
            vec![both.clone()],
            vec![both],
            |a, b| a + b,
            |_wa, _wb| (false, None)
        )
        .is_empty()
    );
    // lift_a2_both: both wholes present but disjoint (149).
    let left_whole: Pattern<i64> = Pattern::new(|_| {
        vec![Hap {
            whole: Some(sp(Time::zero(), t(1, 2))),
            part: sp(Time::zero(), Time::one()),
            value: 1i64,
            context: Context::empty(),
        }]
    });
    let right_whole: Pattern<i64> = Pattern::new(|_| {
        vec![Hap {
            whole: Some(sp(t(3, 4), Time::one())),
            part: sp(Time::zero(), Time::one()),
            value: 2i64,
            context: Context::empty(),
        }]
    });
    assert!(
        lift_a2_both(&left_whole, &right_whole, |a, b| a + b)
            .first_cycle()
            .is_empty()
    );
    // lift_a2_left: `_ => true` when a whole is None (180).
    let la = lift_a2_left(&pure(1i64), &sine(), |a: &i64, b: &f64| *a + (*b as i64));
    assert_eq!(la.first_cycle().len(), 1);
    // lift_a2_left: both wholes present but disjoint -> valid = false (179, 189-190).
    let right_outer: Pattern<i64> = Pattern::new(|st| {
        vec![Hap {
            whole: Some(sp(ti(5), ti(6))),
            part: st.span,
            value: 7i64,
            context: Context::empty(),
        }]
    });
    assert!(
        lift_a2_left(&pure(1i64), &right_outer, |a, b| a + b)
            .first_cycle()
            .is_empty()
    );
    // lift_a2_right: `_ => true` (219) and disjoint wholes -> valid = false (228-229).
    let rb2 = lift_a2_right(&pure(1i64), &sine(), |a: &i64, b: &f64| *a + (*b as i64));
    assert_eq!(rb2.first_cycle().len(), 1);
    assert!(
        lift_a2_right(&pure(1i64), &right_outer, |a, b| a + b)
            .first_cycle()
            .is_empty()
    );
    // lift_a2_left / lift_a2_right: inner-queried haps whose `part` misses the outer
    // `part` entirely -> `intersection` returns `None` and the pair is skipped (190, 229).
    let far: Pattern<i64> =
        Pattern::new(|_| vec![dhap(sp(ti(10), ti(11)), sp(ti(10), ti(11)), 7i64)]);
    assert!(
        lift_a2_left(&pure(1i64), &far, |a, b| a + b)
            .first_cycle()
            .is_empty()
    );
    assert!(
        lift_a2_right(&far, &pure(1i64), |a, b| a + b)
            .first_cycle()
            .is_empty()
    );
    // Monad joins.
    let inner_whole_far: Pattern<i64> =
        Pattern::new(|_| vec![dhap(sp(ti(5), ti(6)), sp(Time::zero(), Time::one()), 9i64)]);
    let inner_part_far: Pattern<i64> =
        Pattern::new(|_| vec![dhap(sp(ti(5), ti(6)), sp(ti(5), ti(6)), 9i64)]);
    // unwrap: outer/inner wholes disjoint -> `continue` (279).
    let nested_wm = pure(inner_whole_far);
    assert!(
        <Pattern<i64> as PatternMonad<i64>>::unwrap(&nested_wm)
            .first_cycle()
            .is_empty()
    );
    // inner_join ignores whole mismatch; still emits (normal path).
    assert_eq!(
        <Pattern<i64> as PatternMonad<i64>>::inner_join(&nested_wm)
            .first_cycle()
            .len(),
        1
    );
    // unwrap / inner_join: inner part outside outer part -> no intersection (291, 312).
    let nested_pm = pure(inner_part_far.clone());
    assert!(
        <Pattern<i64> as PatternMonad<i64>>::unwrap(&nested_pm)
            .first_cycle()
            .is_empty()
    );
    assert!(
        <Pattern<i64> as PatternMonad<i64>>::inner_join(&nested_pm)
            .first_cycle()
            .is_empty()
    );
    // unwrap: continuous outer (whole None) -> `_ => None` (283).
    let nested_cont: Pattern<Pattern<i64>> =
        Pattern::new(|st| vec![Hap::continuous(st.span, pure(3i64), Context::empty())]);
    let uw = <Pattern<i64> as PatternMonad<i64>>::unwrap(&nested_cont).first_cycle();
    assert_eq!(uw.len(), 1);
    assert!(uw[0].whole.is_none());
    // squeeze_join: zero-duration outer (351) and degenerate relative span (357).
    let nested_degen: Pattern<Pattern<i64>> = Pattern::new(|_| {
        vec![
            Hap {
                whole: None,
                part: TimeSpan {
                    begin: t(1, 2),
                    end: t(1, 2),
                },
                value: pure(5i64),
                context: Context::empty(),
            },
            Hap {
                whole: Some(sp(Time::zero(), Time::one())),
                part: TimeSpan {
                    begin: t(1, 2),
                    end: t(1, 2),
                },
                value: pure(5i64),
                context: Context::empty(),
            },
        ]
    });
    assert!(
        <Pattern<i64> as PatternMonad<i64>>::squeeze_join(&nested_degen)
            .first_cycle()
            .is_empty()
    );
    // squeeze_join: mapped inner part does not intersect outer part (376).
    let nested_far_inner = pure(inner_part_far.clone());
    assert!(
        <Pattern<i64> as PatternMonad<i64>>::squeeze_join(&nested_far_inner)
            .first_cycle()
            .is_empty()
    );
    // Trig joins: `_ => inner_hap.whole` (413, 440) and part mismatch (421, 448).
    let nested_trig_cont: Pattern<Pattern<i64>> =
        Pattern::new(|st| vec![Hap::continuous(st.span, pure(4i64), Context::empty())]);
    let tj = <Pattern<i64> as PatternTrigJoin<i64>>::trig_join(&nested_trig_cont).first_cycle();
    assert_eq!(tj.len(), 1);
    let tzj =
        <Pattern<i64> as PatternTrigJoin<i64>>::trig_zero_join(&nested_trig_cont).first_cycle();
    assert_eq!(tzj.len(), 1);
    let nested_trig_far = pure(inner_part_far);
    assert!(
        <Pattern<i64> as PatternTrigJoin<i64>>::trig_join(&nested_trig_far)
            .first_cycle()
            .is_empty()
    );
    assert!(
        <Pattern<i64> as PatternTrigJoin<i64>>::trig_zero_join(&nested_trig_far)
            .first_cycle()
            .is_empty()
    );
}

#[test]
fn combinator_guard_and_fallback_branches() {
    // Empty-input guards (38, 52, 81, 99, 123).
    assert!(stack::<i64>(vec![]).first_cycle().is_empty());
    assert!(slowcat::<i64>(vec![]).first_cycle().is_empty());
    assert!(slowcat_prime::<i64>(vec![]).first_cycle().is_empty());
    assert!(fastcat::<i64>(vec![]).first_cycle().is_empty());
    assert!(timecat::<i64>(vec![]).first_cycle().is_empty());
    assert!(
        timecat(vec![(Time::zero(), pure(1i64)), (ti(-2), pure(2i64))])
            .first_cycle()
            .is_empty()
    );
    // run/scan guards (143, 151).
    assert!(run(0).first_cycle().is_empty());
    assert!(run(-3).first_cycle().is_empty());
    assert!(scan(0).first_cycle().is_empty());
    // fast zero (160), fast negative -> rev + |factor| (163), slow zero (181).
    assert!(pure(1i64).fast(Time::zero()).first_cycle().is_empty());
    assert_eq!(run(2).fast(ti(-2)).first_cycle().len(), 4);
    assert!(pure(1i64).slow(Time::zero()).first_cycle().is_empty());
    // rev: haps mapping outside the query sub-span are dropped (217).
    let static_pat: Pattern<i64> = Pattern::new(|_| {
        vec![dhap(
            sp(Time::zero(), t(1, 2)),
            sp(Time::zero(), t(1, 2)),
            7i64,
        )]
    });
    assert!(static_pat.rev().query_arc(ti(5), ti(6)).is_empty());
    // iter/iter_back/segment guards (259, 268, 277).
    assert!(run(2).iter(0).first_cycle().is_empty());
    assert!(run(2).iter_back(0).first_cycle().is_empty());
    assert!(run(2).segment(Time::zero()).first_cycle().is_empty());
    // segment: step windows that miss the query sub-span (293).
    let seg = run(4).segment(ti(2));
    assert_eq!(seg.query_arc(t(3, 5), t(9, 10)).len(), 1);
    // compress/zoom guards (312, 351) and mapped-part misses (341, 374).
    assert!(run(2).compress(t(1, 2), t(1, 4)).first_cycle().is_empty());
    assert!(run(2).zoom(t(1, 2), t(1, 4)).first_cycle().is_empty());
    let fixed_far: Pattern<i64> =
        Pattern::new(|_| vec![dhap(sp(ti(5), ti(6)), sp(ti(5), ti(6)), 3i64)]);
    assert!(
        fixed_far
            .compress(Time::zero(), t(1, 2))
            .first_cycle()
            .is_empty()
    );
    assert!(
        fixed_far
            .zoom(Time::zero(), t(1, 2))
            .first_cycle()
            .is_empty()
    );
    // inside/outside zero-factor guards (387, 398).
    assert!(
        run(2)
            .inside(Time::zero(), |p| p.rev())
            .first_cycle()
            .is_empty()
    );
    assert!(
        run(2)
            .outside(Time::zero(), |p| p.rev())
            .first_cycle()
            .is_empty()
    );
    // within: invalid window returns self (409); partial arc hits pre/post/mid-None arms (427).
    let w = run(2);
    assert_eq!(
        w.within(t(1, 2), t(1, 4), |p| p.rev()).first_cycle(),
        w.first_cycle()
    );
    let wv = run(2).within(t(1, 4), t(3, 4), |p| p.fast(ti(2)));
    assert_eq!(wv.query_arc(Time::zero(), t(1, 8)).len(), 1);
    // linger/swing_by/ribbon guards (442, 451, 479) and ribbon negative-rel wrap (488).
    assert!(run(2).linger(Time::zero()).first_cycle().is_empty());
    let sw = run(2);
    assert_eq!(sw.swing_by(t(1, 3), Time::zero()).first_cycle().len(), 2);
    assert!(
        run(2)
            .ribbon(Time::zero(), Time::zero())
            .first_cycle()
            .is_empty()
    );
    let rib = run(2).ribbon(Time::zero(), ti(2));
    assert_eq!(rib.query_arc(ti(-5), ti(-3)).len(), 4);
    // every_offset/each/whenmod/chunk/chunk_back/ply guards (516, 527, 562, 657, 675, 692).
    let ev = run(2);
    assert_eq!(
        ev.every_offset(0, 0, |p| p.rev()).first_cycle(),
        ev.first_cycle()
    );
    type I64Transform = fn(&Pattern<i64>) -> Pattern<i64>;
    let fs: Vec<I64Transform> = Vec::new();
    let each_p = run(2);
    assert_eq!(each_p.each(fs).first_cycle(), each_p.first_cycle());
    assert_eq!(run(2).whenmod(0, 1, |p| p.rev()).first_cycle().len(), 2);
    assert_eq!(run(2).chunk(0, |p| p.rev()).first_cycle().len(), 2);
    assert_eq!(run(2).chunk_back(0, |p| p.rev()).first_cycle().len(), 2);
    assert!(run(2).ply(Time::zero()).first_cycle().is_empty());
    // ply: a sub-whole that misses a hap's shorter part (712).
    let partial: ControlPattern = Pattern::new(|_| {
        vec![Hap::discrete(
            sp(Time::zero(), Time::one()),
            sp(Time::zero(), t(2, 5)),
            ControlMap::new(),
            Context::empty(),
        )]
    });
    assert_eq!(partial.ply(ti(3)).first_cycle().len(), 2);
    // stut/echo_with zero-count guards (756, 785); undegrade (1033-1035).
    assert!(s("bd").stut(0, 0.5, t(1, 8)).first_cycle().is_empty());
    assert!(
        s("bd")
            .echo_with(0, t(1, 4), |p| p.clone())
            .first_cycle()
            .is_empty()
    );
    let base = run(8);
    assert_eq!(
        base.degrade().first_cycle().len() + base.undegrade().first_cycle().len(),
        8
    );
}

#[test]
fn stepwise_guards_and_partial_arcs() {
    assert!(bjorklund(3, 0, 0).is_empty());
    assert!(bjorklund(0, 4, 0).iter().all(|b| !b));
    assert!(bjorklund(4, 4, 0).iter().all(|b| *b));
    // euclid family empty-rhythm guards (51, 64, 77).
    assert!(pure(1i64).euclid_rot(3, 0, 0).first_cycle().is_empty());
    assert!(pure(1i64).euclid_inv(3, 0).first_cycle().is_empty());
    assert!(
        pure(1i64)
            .euclid_full(3, 0, &pure(2i64))
            .first_cycle()
            .is_empty()
    );
    // step/take/drop/rot guards (98, 117, 140, 163).
    assert!(run(2).step(Time::zero()).first_cycle().is_empty());
    assert!(run(4).take_steps(0).first_cycle().is_empty());
    assert_eq!(run(4).drop_steps(0).first_cycle().len(), 4);
    let r = run(4);
    assert_eq!(r.rot(0).first_cycle(), r.first_cycle());
    // rot: empty inner pattern (174) and parts missing partial sub (183).
    let sil: Pattern<i64> = silence();
    assert!(sil.rot(1).first_cycle().is_empty());
    assert_eq!(run(4).rot(1).query_arc(t(1, 2), t(3, 4)).len(), 1);
    // polymeter empty (209).
    assert!(polymeter::<i64>(vec![], None).first_cycle().is_empty());
    // SamplePatternOps zero-n guards (242, 261, 292, 315, 335, 343, 378).
    let ctrl = s("bd");
    assert!(ctrl.striate(0).first_cycle().is_empty());
    assert!(ctrl.chop(0).first_cycle().is_empty());
    assert!(ctrl.slice(0, &run(2)).first_cycle().is_empty());
    assert!(ctrl.splice(0, &run(2)).first_cycle().is_empty());
    assert!(ctrl.randslice(0).first_cycle().is_empty());
    assert!(ctrl.scramble(0).first_cycle().is_empty());
    assert!(ctrl.shuffle(0).first_cycle().is_empty());
    // chop: sub-whole missing a shorter hap part (283).
    let partial: ControlPattern = Pattern::new(|_| {
        vec![Hap::discrete(
            sp(Time::zero(), Time::one()),
            sp(Time::zero(), t(2, 5)),
            ControlMap::new(),
            Context::empty(),
        )]
    });
    assert_eq!(partial.chop(3).first_cycle().len(), 2);
    // scramble/shuffle: destination windows that miss the query sub-span (359, 401).
    assert!(!ctrl.scramble(4).query_arc(t(3, 5), t(4, 5)).is_empty());
    assert!(!ctrl.shuffle(4).query_arc(t(3, 5), t(4, 5)).is_empty());
}

#[test]
fn signals_and_stateful_edges() {
    // irand guard (53); sin_f64 r < -pi range-reduction (97); ln_f64 non-positive (272).
    assert!(irand(0).first_cycle().is_empty());
    assert!((sin_f64(-4.0) - (-4.0f64).sin()).abs() < 1e-6);
    assert_eq!(ln_f64(0.0), -1e9);
    // choose/wchoose empty guards (175, 193).
    assert!(choose::<i64>(vec![]).first_cycle().is_empty());
    assert!(wchoose::<i64>(vec![]).first_cycle().is_empty());
    assert!(
        wchoose(vec![(0.0, 1i64), (-1.0, 2i64)])
            .first_cycle()
            .is_empty()
    );
    let w = wchoose(vec![(1.0, 5i64)])
        .segment(Time::one())
        .first_cycle();
    assert_eq!(w[0].value, 5);
    // wchoose: `target > acc` after the first item skips it and continues the scan (203).
    // The first weight is derived from the deterministic draw at the cycle midpoint
    // (`time_to_rand(1/2)` with the default session seed) so the second item always wins.
    let r_mid = time_to_rand(t(1, 2));
    assert!(r_mid > 0.0);
    let wc = wchoose(vec![(r_mid / 2.0, 0i64), (1.0, 1i64)]).first_cycle();
    assert_eq!(wc[0].value, 1);
    // quantize: non-positive steps -> self (239); negative scaled round (246).
    assert_eq!(sine().quantize(0.0).first_cycle(), sine().first_cycle());
    let q = pure(-0.3f64).quantize(4.0).first_cycle();
    assert_eq!(q[0].value, -0.25);
    // smooth: non-positive window -> self (254).
    assert_eq!(sine().smooth(Time::zero()).first_cycle().len(), 1);
    // Checkpoint store: eviction beyond 256 cached cycles (59-61); reset_state (69).
    let cp = LruCycleStateCheckpointer::new(0i64, 0);
    assert_eq!(cp.state_at_cycle(300, &|_c, s| s + 1), 300);
    cp.reset_state(7, 3);
    // StatefulPattern::reset (120-122).
    let sp_pat = StatefulPattern::new(
        0i64,
        |_c, s| s + 1,
        |_c, &s, sub| {
            vec![Hap::discrete(
                TimeSpan::cycle_of(sub.begin),
                sub,
                s,
                Context::empty(),
            )]
        },
    );
    sp_pat.reset(5, 2);
    assert_eq!(sp_pat.as_pattern().first_cycle().len(), 1);
    // accum: emits against per-cycle state; partial arc drops non-overlapping parts (148, 168-169).
    let acc = run(2).accum(0i64, |s, x| (*s + *x, *s + *x));
    let vals: Vec<i64> = acc
        .query_arc(ti(1), ti(2))
        .into_iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(vals, vec![1, 2]);
    assert_eq!(acc.query_arc(t(3, 5), t(4, 5)).len(), 1);
    // accum on a continuous source: `whole` is `None`, so `has_onset()` is false and the
    // step/emit closures skip every hap (148, 169).
    let cont_acc = sine().accum(0i64, |s: &i64, x: &f64| (*s + (*x as i64), *s));
    assert!(cont_acc.first_cycle().is_empty());
    assert!(cont_acc.query_arc(ti(1), ti(2)).is_empty());
    // markov: step function runs for cycles > origin (211-219); out-of-range state (190);
    // zero-weight row (194); partial arc intersection misses (232 region).
    let mk = markov_pattern(0, 4, vec![vec![0.0, 1.0], vec![1.0, 0.0]]);
    assert_eq!(mk.query_arc(ti(1), ti(2)).len(), 4);
    let mk_oob = markov_pattern(9, 2, vec![vec![0.5, 0.5], vec![0.5, 0.5]]);
    assert_eq!(mk_oob.first_cycle().len(), 2);
    let mk_zero = markov_pattern(0, 2, vec![vec![0.0, 0.0], vec![1.0, 0.0]]);
    assert_eq!(mk_zero.first_cycle().len(), 2);
    let mk_part = markov_pattern(0, 4, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    assert!(!mk_part.query_arc(t(3, 10), t(2, 5)).is_empty());
    // lsystem: characters without rules pass through (251).
    assert_eq!(lsystem_expand("AX", &[('A', "AB")], 1), "ABX");
    // tie_contiguous: a continuous (whole=None) tie hap extends part but not whole (284-285).
    let tie_pat: Pattern<Option<i64>> = Pattern::new(|_| {
        vec![
            dhap(
                sp(Time::zero(), t(1, 2)),
                sp(Time::zero(), t(1, 2)),
                Some(1i64),
            ),
            Hap {
                whole: None,
                part: sp(t(1, 2), t(3, 4)),
                value: None,
                context: Context::empty(),
            },
        ]
    });
    let tied = tie_contiguous(&tie_pat).first_cycle();
    assert_eq!(tied.len(), 1);
    assert_eq!(tied[0].part.end, t(3, 4));
    assert_eq!(tied[0].whole.map(|w| w.end), Some(t(1, 2)));
    // tie_contiguous: a leading `None` has no previous merged hap to extend (285).
    let tie_none_first: Pattern<Option<i64>> = Pattern::new(|_| {
        vec![
            Hap {
                whole: None,
                part: sp(Time::zero(), t(1, 2)),
                value: None,
                context: Context::empty(),
            },
            dhap(
                sp(t(1, 2), Time::one()),
                sp(t(1, 2), Time::one()),
                Some(2i64),
            ),
        ]
    });
    let tied2 = tie_contiguous(&tie_none_first).first_cycle();
    assert_eq!(tied2.len(), 1);
    assert_eq!(tied2[0].part, sp(t(1, 2), Time::one()));
    // glide curve names (303-304).
    let lin = note("c4").glide(t(1, 8), GlideCurve::Linear);
    assert_eq!(
        lin.first_cycle()[0]
            .value
            .get("glide_curve")
            .and_then(Value::as_str),
        Some("linear")
    );
    let expg = note("c4").glide(t(1, 8), GlideCurve::Exponential);
    assert_eq!(
        expg.first_cycle()[0]
            .value
            .get("glide_curve")
            .and_then(Value::as_str),
        Some("exponential")
    );
}

#[test]
fn extension_guards_and_transition_branches() {
    // TrigCondition::Ratio with b == 0 (51).
    let mut ledger = TrigConditionLedger::default();
    assert!(!TrigCondition::Ratio { a: 1, b: 0 }.evaluate(0, &mut ledger));
    // plock / table / groove guards (103, 137, 166).
    let bd = s("bd");
    assert_eq!(bd.plock(0, 0, &lpf(300.0)).first_cycle().len(), 1);
    assert_eq!(bd.plock(4, 4, &lpf(300.0)).first_cycle().len(), 1);
    assert_eq!(bd.table(Time::zero(), &lpf(300.0)).first_cycle().len(), 1);
    let g = run(2);
    assert_eq!(g.groove(&[]).first_cycle(), g.first_cycle());
    assert_eq!(g.groove(&[0, 0]).first_cycle(), g.first_cycle());
    // once at an exact cycle boundary (246).
    assert_eq!(
        once(&pure(1i64), ti(2))
            .query_arc(Time::zero(), ti(4))
            .len(),
        1
    );
    // qtrigger: a degenerate (inverted) query `span` reaches `TimeSpan::new(begin, end)`
    // with `begin >= end` -> `None` -> empty result (263).
    let qt = qtrigger(&pure(1i64), Time::zero());
    let deg_state = State {
        span: TimeSpan {
            begin: ti(5),
            end: ti(1),
        },
        controls: ControlMap::new(),
        query_context: QueryContext::new(sp(Time::zero(), Time::one())),
    };
    assert!(qt.query(&deg_state).is_empty());
    // fade_in: non-positive cycles (273) and pre-start drop (281); fade_out guard (297).
    assert_eq!(bd.fade_in(ti(1), Time::zero()).first_cycle().len(), 1);
    let fade = bd.fade_in(ti(2), ti(2));
    assert!(fade.query_arc(Time::zero(), ti(1)).is_empty());
    assert!(!fade.query_arc(ti(2), ti(3)).is_empty());
    assert!(
        s("bd")
            .fade_out(Time::zero(), Time::zero())
            .first_cycle()
            .is_empty()
    );
    let old = s("bd");
    let new = s("sd");
    // clutch_in: u >= 1 -> new stream only (405).
    let cl = transitions::clutch_in(&old, &new, Time::zero(), ti(1));
    let late_haps = cl.query_arc(ti(2), ti(3));
    assert_eq!(
        late_haps[0].value.get("s"),
        Some(&Value::String("sd".to_string()))
    );
    // anticipate_in: old before start (438), hpf build-up (441-445), new past cut (436).
    let ant = transitions::anticipate_in(&old, &new, ti(2), ti(2));
    let all = ant.query_arc(Time::zero(), ti(5));
    assert!(all.iter().any(|h| h.value.contains_key("hpf")));
    assert_eq!(
        ant.query_arc(ti(4), ti(5))[0].value.get("s"),
        Some(&Value::String("sd".to_string()))
    );
    // interpolate: numeric blend insert (482).
    let interp = transitions::interpolate(&lpf(200.0), &lpf(1200.0), Time::zero(), ti(4));
    assert_eq!(interp.query_arc(ti(1), ti(2)).len(), 1);
    // interpolate: `new` keys absent from `old` or non-numeric fail the `if let`
    // numeric-pair check and pass through unblended (482 else).
    let interp_missing = transitions::interpolate(&s("bd"), &lpf(1200.0), Time::zero(), ti(4));
    let ih = interp_missing.query_arc(ti(1), ti(2));
    assert_eq!(ih.len(), 1);
    assert_eq!(ih[0].value.get("lpf"), Some(&Value::Float(1200.0)));
    let interp_nonnum = transitions::interpolate(&lpf(200.0), &s("sd"), Time::zero(), ti(4));
    assert_eq!(
        interp_nonnum.query_arc(ti(1), ti(2))[0].value.get("s"),
        Some(&Value::String("sd".to_string()))
    );
    // histpan: empty history (496) and single-pattern center pan (504).
    assert!(transitions::histpan(vec![], 2).first_cycle().is_empty());
    let hp = transitions::histpan(vec![old.clone()], 1).first_cycle();
    assert_eq!(hp[0].value.get("pan"), Some(&Value::Float(0.5)));
    // mortal: silence beyond deg_end (571) plus normal lifespan window.
    let m = transitions::mortal(&new, Time::zero(), ti(1), ti(1));
    assert!(m.query_arc(ti(3), ti(4)).is_empty());
    assert_eq!(m.query_arc(Time::zero(), ti(1)).len(), 1);
}

#[test]
fn tonal_scales_chords_dict_and_scala() {
    // scale_intervals dictionary arms (17-40).
    for name in [
        "ionian",
        "minor",
        "aeolian",
        "natural:minor",
        "dorian",
        "phrygian",
        "lydian",
        "mixolydian",
        "locrian",
        "harmonic:minor",
        "harmonicminor",
        "melodic:minor",
        "melodicminor",
        "major:pentatonic",
        "pentatonic",
        "majpent",
        "minor:pentatonic",
        "minpent",
        "blues",
        "whole:tone",
        "wholetone",
        "diminished",
        "octatonic",
        "augmented",
        "bebop",
        "hirajoshi",
        "iwato",
        "kumoi",
        "pelog",
        "prometheus",
        "enigmatic",
        "phrygian:dominant",
        "spanish",
        "chromatic",
        "nonexistent-scale",
    ] {
        assert!(!scale_intervals(name).is_empty(), "{name}");
    }
    // parse_scale_descriptor no-colon branch (53).
    assert_eq!(parse_scale_descriptor("  phrygian ").0, 60);
    assert_eq!(parse_scale_descriptor("").1, scale_intervals("major"));
    // parse_chord_symbol: empty (84), invalid root (101), all root letters + accidentals
    // including 's'-as-sharp vs "sus" guard (109-114) and 'b'/'f' flats (117-118).
    assert!(parse_chord_symbol("").is_none());
    assert!(parse_chord_symbol("   ").is_none());
    assert!(parse_chord_symbol("z9").is_none());
    for sym in [
        "Fmaj7", "G7", "Am7", "Bm", "Cb", "Cf4", "Cs7", "Eb7", "Csus4",
    ] {
        assert!(parse_chord_symbol(sym).is_some(), "{sym}");
    }
    // Every quality-table arm (129-155), quoting qualities that start with
    // accidental characters so the accidental loop leaves them intact.
    for q in [
        "C", "CM", "Cmaj", "C^", "Cm", "Cmin", "C-", "Cdim", "Co", "C0", "Cdim7", "Co7", "Cm7b5",
        "Ch7", "Ch", "C-7b5", "Caug", "C+", "C's5", "C'#5", "C'sus2", "C'sus", "C6", "CM6",
        "Cmaj6", "Cm6", "Cmin6", "C-6", "C7", "Cdom7", "Cmaj7", "C^7", "CM7", "Cm7", "Cmin7",
        "C-7", "CmMaj7", "C-^7", "CminMaj7", "C7s5", "C7#5", "C+7", "Caug7", "C7b5", "C7b9",
        "C7#9", "C7s9", "C9", "Cdom9", "Cmaj9", "C^9", "CM9", "Cm9", "Cmin9", "C-9", "C11", "Cm11",
        "Cmin11", "C-11", "C13", "Cm13", "Cmin13", "C-13", "Cmaj13", "C^13", "Cbogus",
    ] {
        assert!(parse_chord_symbol(q).is_some(), "{q}");
    }
    // @dict voicing arms: guidetones (161-164), lefthand (168-171), drop2 (174-183), unknown (185).
    assert_eq!(
        parse_chord_symbol("C7@guidetones").unwrap().intervals,
        vec![4, 10]
    );
    assert_eq!(
        parse_chord_symbol("C@guidetones").unwrap().intervals,
        vec![0, 4, 7]
    );
    assert_eq!(
        parse_chord_symbol("C7@lefthand").unwrap().intervals,
        vec![4, 7, 10, 14]
    );
    assert_eq!(
        parse_chord_symbol("C@lefthand").unwrap().intervals,
        vec![0, 4, 7]
    );
    assert_eq!(
        parse_chord_symbol("C@drop2").unwrap().intervals,
        vec![0, 4, 7]
    );
    assert_eq!(
        parse_chord_symbol("Cmaj7@drop2").unwrap().intervals.len(),
        4
    );
    assert_eq!(
        parse_chord_symbol("Cmaj7@bogus").unwrap().intervals,
        vec![0, 4, 7, 11]
    );
    // chord(): parse -> Value::List (250-257), parse failure -> Value::Null (259).
    let chorded = pure(Value::Int(0)).chord(&pure("Cmaj7".to_string()));
    match &chorded.first_cycle()[0].value {
        Value::List(ns) => assert_eq!(ns.len(), 4),
        other => panic!("expected List, got {other:?}"),
    }
    assert_eq!(
        pure(Value::Int(0))
            .chord(&pure("z9".to_string()))
            .first_cycle()[0]
            .value,
        Value::Null
    );
    // voicing() fallbacks (276, 279).
    assert_eq!(
        pure(Value::String("z9".to_string()))
            .voicing()
            .first_cycle()[0]
            .value,
        Value::String("z9".to_string())
    );
    assert_eq!(
        pure(Value::Int(3)).voicing().first_cycle()[0].value,
        Value::Int(3)
    );
    // dict(): already-tagged passthrough (287), unparseable -> tagged String (300), non-String (303).
    let tagged = pure(Value::String("Cmaj7@triad".to_string())).dict(&pure("drop2".to_string()));
    assert!(matches!(tagged.first_cycle()[0].value, Value::List(_)));
    assert_eq!(
        pure(Value::String("z9".to_string()))
            .dict(&pure("drop2".to_string()))
            .first_cycle()[0]
            .value,
        Value::String("z9@drop2".to_string())
    );
    assert_eq!(
        pure(Value::Int(2))
            .dict(&pure("x".to_string()))
            .first_cycle()[0]
            .value,
        Value::Int(2)
    );
    // root_notes(): parse failure (314), List head (317), scalar passthrough (318).
    assert_eq!(
        pure(Value::String("z9".to_string()))
            .root_notes(4)
            .first_cycle()[0]
            .value,
        Value::String("z9".to_string())
    );
    assert_eq!(
        pure(Value::List(vec![Value::Int(7)]))
            .root_notes(4)
            .first_cycle()[0]
            .value,
        Value::Int(7)
    );
    assert_eq!(
        pure(Value::Int(9)).root_notes(4).first_cycle()[0].value,
        Value::Int(9)
    );
    // anchor(): raise a far-below note into the anchor window (337); non-List passthrough (344).
    let low = pure(Value::List(vec![Value::Note(NoteValue::Midi(30.0))]));
    let anch = low.anchor(&pure("c6".to_string()));
    match &anch.first_cycle()[0].value {
        Value::List(ns) => assert_eq!(ns[0].as_f64(), Some(78.0)),
        _ => panic!("expected List"),
    }
    assert_eq!(
        pure(Value::Int(4))
            .anchor(&pure("c4".to_string()))
            .first_cycle()[0]
            .value,
        Value::Int(4)
    );
    // arp_with: non-List value -> pure passthrough (360).
    let arp_scalar = arp_with(
        &pure(Value::Int(5)),
        &pure(|notes: &[Value]| notes.to_vec()),
    );
    assert_eq!(arp_scalar.first_cycle()[0].value, Value::Int(5));
    // Scala .scl/.kbm parser errors and integer-ratio arm.
    assert!(ScalaTuning::from_scl_kbm("", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("desc", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\noops\n1\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n1\nzz.top\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n1\nn/2\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n1\n2/n\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n1\n0/2\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n1\n-2\n", None).is_err());
    // Integer-ratio token that fails `f64` parse (no '.', no '/') -> error arm (424).
    assert!(ScalaTuning::from_scl_kbm("d\n1\nabc\n", None).is_err());
    assert!(ScalaTuning::from_scl_kbm("d\n5\n1\n2\n", None).is_err());
    let blank = ScalaTuning::from_scl_kbm("d\n3\n\n100.0\n3/2\n2/1\n", None).unwrap();
    assert_eq!(blank.pitches_cents.len(), 3);
    let int_ratios = ScalaTuning::from_scl_kbm("d\n2\n3\n2\n", None).unwrap();
    assert!((int_ratios.pitches_cents[1] - 1200.0).abs() < 1e-9);
    // kbm: unparseable / non-positive reference values keep defaults (453, 462-464).
    let kbm_bad = "1\n0\n127\n60\nxx\n-5\n";
    let tk = ScalaTuning::from_scl_kbm("d\n1\n2\n", Some(kbm_bad)).unwrap();
    assert_eq!(tk.reference_midi, 69);
    assert_eq!(tk.reference_freq_hz, 440.0);
    let tk_short = ScalaTuning::from_scl_kbm("d\n1\n2\n", Some("1\n2\n")).unwrap();
    assert_eq!(tk_short.reference_midi, 69);
    // degree_to_freq_hz: empty table -> reference (492), negative fractional degree (499-500),
    // interior scale step (508).
    let mut empty_t = ScalaTuning::edo(4, 2.0);
    empty_t.pitches_cents.clear();
    assert_eq!(empty_t.degree_to_freq_hz(3.0), 440.0);
    let e19 = ScalaTuning::edo(19, 2.0);
    assert!(e19.degree_to_freq_hz(-0.5) > 0.0);
    let step_one = e19.degree_to_freq_hz(1.0);
    assert!(step_one > 440.0 && step_one < 880.0);
}

#[test]
fn value_coercion_and_numeric_edges() {
    // parse_pitch_name: empty (25), letters e/f/g/a/b (30-35), invalid (36), flat (46-47),
    // octave-less default (54).
    assert_eq!(NoteValue::parse_pitch_name(""), None);
    for (nm, pc) in [("e2", 4), ("f2", 5), ("g2", 7), ("a2", 9), ("b2", 11)] {
        assert_eq!(NoteValue::parse_pitch_name(nm), Some((pc, 2)));
    }
    assert_eq!(NoteValue::parse_pitch_name("z2"), None);
    assert_eq!(NoteValue::parse_pitch_name("df4"), Some((1, 4)));
    assert_eq!(NoteValue::parse_pitch_name("c"), Some((0, 4)));
    // to_midi Midi arm (64) and SuperDirtC5 zero base (69).
    assert_eq!(
        NoteValue::Midi(61.5).to_midi(EngineCompatibilityProfile::SuperDoughC4),
        Some(61.5)
    );
    assert_eq!(
        NoteValue::Name("c5".to_string()).to_midi(EngineCompatibilityProfile::SuperDirtC5),
        Some(60.0)
    );
    // exp2_f64: negative x (90), k >= 62 (106), -62 < k < 0 (109), k <= -62 (111).
    assert!((exp2_f64(-0.5) - core::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
    assert!((exp2_f64(70.0) / 1.1805916207174113e21 - 1.0).abs() < 1e-9);
    assert!((exp2_f64(-30.0) / 9.313225746154785e-10 - 1.0).abs() < 1e-9);
    assert!((exp2_f64(-70.0) / 8.470329472543003e-22 - 1.0).abs() < 1e-9);
    // from_name unknown (134).
    assert!(EngineCompatibilityProfile::from_name("bogus").is_none());
    // as_f64 Rational/Bool/Note::Name/None arms (189-196).
    assert_eq!(Value::Rational(t(1, 2)).as_f64(), Some(0.5));
    assert_eq!(Value::Bool(true).as_f64(), Some(1.0));
    assert_eq!(
        Value::Note(NoteValue::Name("a4".to_string())).as_f64(),
        Some(69.0)
    );
    assert_eq!(Value::Null.as_f64(), None);
    // as_i64 Float/Rational/Bool/None arms (204-207).
    assert_eq!(Value::Float(3.9).as_i64(), Some(3));
    assert_eq!(Value::Rational(t(3, 2)).as_i64(), Some(1));
    assert_eq!(Value::Bool(false).as_i64(), Some(0));
    assert_eq!(Value::Null.as_i64(), None);
    // as_rational Int/Float/None arms (215-217).
    assert_eq!(Value::Int(5).as_rational(), Some(ti(5)));
    assert_eq!(Value::Float(0.25).as_rational(), Some(t(1, 4)));
    assert_eq!(Value::Null.as_rational(), None);
    // as_str Note::Name arm (225) and the `_ => None` fallback for non-string values.
    assert_eq!(
        Value::Note(NoteValue::Name("c4".to_string())).as_str(),
        Some("c4")
    );
    assert_eq!(Value::Int(4).as_str(), None);
    // as_bool Int/Float/Rational/None arms (233-236).
    assert_eq!(Value::Int(0).as_bool(), Some(false));
    assert_eq!(Value::Float(2.5).as_bool(), Some(true));
    assert_eq!(Value::Rational(t(1, 2)).as_bool(), Some(true));
    assert_eq!(Value::Null.as_bool(), None);
    // add/sub/mul/div Rational arms and fallbacks (244, 247, 256, 259, 268, 271, 280-283, 294).
    let half = Value::Rational(t(1, 2));
    let quarter = Value::Rational(t(1, 4));
    assert_eq!(half.add_numeric(&quarter), Value::Rational(t(3, 4)));
    assert_eq!(half.sub_numeric(&quarter), Value::Rational(t(1, 4)));
    assert_eq!(half.mul_numeric(&quarter), Value::Rational(t(1, 8)));
    assert_eq!(
        half.div_numeric(&Value::Rational(Time::zero())),
        Value::Float(0.0)
    );
    assert_eq!(half.div_numeric(&quarter), Value::Rational(ti(2)));
    assert_eq!(
        Value::Float(1.5).add_numeric(&Value::Int(2)),
        Value::Float(3.5)
    );
    let xs = Value::from("x");
    assert_eq!(xs.add_numeric(&Value::Int(2)), Value::Int(2));
    assert_eq!(xs.sub_numeric(&Value::Int(2)), Value::from("x"));
    assert_eq!(xs.mul_numeric(&Value::Int(2)), Value::from("x"));
    assert_eq!(xs.div_numeric(&Value::from("y")), Value::from("x"));
    // From<String> for Value (325-327).
    let sv = Value::from(String::from("hi"));
    assert_eq!(sv.as_str(), Some("hi"));
}
