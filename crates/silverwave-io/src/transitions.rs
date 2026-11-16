//! Complete TidalCycles Transition Engine (`xfade`, `xfadeIn`, `clutch`, `clutchIn`,
//! `anticipate`, `anticipateIn`, `interpolate`, `interpolateIn`, `jump`, `jumpIn`,
//! `jumpIn'`, `jumpMod`, `histpan`, `wait`, `waitT`, `wash`, `washIn`, `mortal` —
//! `04-SPEC` Section 6.2, `TRN-01`..`TRN-08`, `PP-08`).

use crate::types::{CycleTime, IoError, StreamId};
use silverwave_core::{
    silence, stack, time_to_rand_seeded, ControlMap as CoreControlMap, ControlPattern, Hap,
    Pattern, PatternFunctor, State, TimeSpan, Value,
};

/// Fade building-block primitives (`Sound.Tidal.Transition` parity — `Section 6.2`).
pub trait FadePrimitives<P> {
    fn fade_in(cycles: CycleTime, pat: P) -> P;
    fn fade_in_from(start: CycleTime, cycles: CycleTime, pat: P) -> P;
    fn fade_out(cycles: CycleTime, pat: P) -> P;
    fn fade_out_from(start: CycleTime, cycles: CycleTime, pat: P) -> P;
}

impl FadePrimitives<ControlPattern> for ControlPattern {
    fn fade_in(cycles: CycleTime, pat: ControlPattern) -> ControlPattern {
        Self::fade_in_from(CycleTime::zero(), cycles, pat)
    }

    fn fade_in_from(start: CycleTime, cycles: CycleTime, pat: ControlPattern) -> ControlPattern {
        if cycles <= CycleTime::zero() {
            return pat;
        }
        let end = start + cycles;
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for mut h in pat.query(state) {
                let t = h.whole_or_part().begin;
                let g = if t <= start {
                    0.0
                } else if t >= end {
                    1.0
                } else {
                    ((t - start) / cycles).to_f64()
                };
                if g > 0.0 {
                    scale_hap_gain(&mut h.value, g);
                    out.push(h);
                }
            }
            out
        })
    }

    fn fade_out(cycles: CycleTime, pat: ControlPattern) -> ControlPattern {
        Self::fade_out_from(CycleTime::zero(), cycles, pat)
    }

    fn fade_out_from(start: CycleTime, cycles: CycleTime, pat: ControlPattern) -> ControlPattern {
        if cycles <= CycleTime::zero() {
            return silence();
        }
        let end = start + cycles;
        Pattern::new(move |state: &State| {
            let mut out = Vec::new();
            for mut h in pat.query(state) {
                let t = h.whole_or_part().begin;
                if t >= end {
                    continue;
                }
                let g = if t <= start {
                    1.0
                } else {
                    1.0 - ((t - start) / cycles).to_f64()
                };
                if g > 0.0 {
                    scale_hap_gain(&mut h.value, g);
                    out.push(h);
                }
            }
            out
        })
    }
}

fn scale_hap_gain(map: &mut CoreControlMap, factor: f64) {
    let base = map.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
    map.insert("gain".to_string(), Value::Float(base * factor));
}

/// Complete TidalCycles transition specification (`Section 6.2`).
#[derive(Clone, Debug)]
pub enum TransitionSpec<P> {
    XfadeIn {
        cycles: CycleTime,
    },
    ClutchIn {
        cycles: CycleTime,
    },
    AnticipateIn {
        cycles: CycleTime,
    },
    InterpolateIn {
        cycles: CycleTime,
    },
    Jump,
    JumpIn {
        cycles: i64,
    },
    JumpInPrime {
        cycles: i64,
    },
    JumpMod {
        cycle_mod: i64,
    },
    Histpan {
        count: usize,
    },
    Wait {
        cycles: CycleTime,
    },
    WaitT {
        cycles: CycleTime,
        inner: Box<TransitionSpec<P>>,
    },
    Wash {
        delay: CycleTime,
        duration: CycleTime,
        out_transform: fn(P) -> P,
        in_transform: fn(P) -> P,
    },
    WashIn {
        delay: CycleTime,
        duration: CycleTime,
        transform: fn(P) -> P,
    },
    Mortal {
        lifespan: CycleTime,
        release: CycleTime,
    },
}

/// Trait for applying a `TransitionSpec<P>` to a stream (`Section 6.2`).
pub trait TransitionEngine<P> {
    fn apply_transition(
        &self,
        id: &StreamId,
        spec: TransitionSpec<P>,
        next: P,
        eval_cycle: CycleTime,
    ) -> Result<(), IoError>;
}

/// Pure function compiling any `TransitionSpec<ControlPattern>` over `history` (`[prev, older...]`)
/// and incoming `next` at `eval_cycle` into a single `ControlPattern` (`TRN-01`..`TRN-08`).
pub fn compile_transition_pattern(
    spec: &TransitionSpec<ControlPattern>,
    history: &[ControlPattern],
    next: &ControlPattern,
    eval_cycle: CycleTime,
) -> ControlPattern {
    let prev = history.first().cloned().unwrap_or_else(silence);
    match spec {
        TransitionSpec::Jump => next.clone(),
        TransitionSpec::XfadeIn { cycles } => {
            let dur = (*cycles).max(CycleTime::new_const(1, 1000));
            let p_out = ControlPattern::fade_out_from(eval_cycle, dur, prev);
            let p_in = ControlPattern::fade_in_from(eval_cycle, dur, next.clone());
            stack(vec![p_out, p_in])
        }
        TransitionSpec::ClutchIn { cycles } => {
            let dur = (*cycles).max(CycleTime::new_const(1, 1000));
            let end = eval_cycle + dur;
            let next_c = next.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for h in prev.query(state) {
                    let t = h.whole_or_part().begin;
                    if t < eval_cycle {
                        out.push(h);
                    } else if t < end {
                        let prog = ((t - eval_cycle) / dur).to_f64();
                        if time_to_rand_seeded(t, 0, 0xC107_C401) >= prog {
                            out.push(h);
                        }
                    }
                }
                for h in next_c.query(state) {
                    let t = h.whole_or_part().begin;
                    if t >= end {
                        out.push(h);
                    } else if t >= eval_cycle {
                        let prog = ((t - eval_cycle) / dur).to_f64();
                        if time_to_rand_seeded(t, 0, 0xC107_C402) <= prog {
                            out.push(h);
                        }
                    }
                }
                out
            })
        }
        TransitionSpec::AnticipateIn { cycles } => {
            let dur = (*cycles).max(CycleTime::new_const(1, 1000));
            let switch_at = eval_cycle + dur;
            let next_c = next.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for mut h in prev.query(state) {
                    let t = h.whole_or_part().begin;
                    if t < switch_at {
                        let prog = ((t - eval_cycle) / dur).to_f64().clamp(0.0, 1.0);
                        h.value
                            .insert("hpf".to_string(), Value::Float(100.0 + 2000.0 * prog));
                        out.push(h);
                    }
                }
                for h in next_c.query(state) {
                    if h.whole_or_part().begin >= switch_at {
                        out.push(h);
                    }
                }
                out
            })
        }
        TransitionSpec::InterpolateIn { cycles } => {
            let dur = (*cycles).max(CycleTime::new_const(1, 1000));
            let end = eval_cycle + dur;
            let next_c = next.clone();
            Pattern::new(move |state: &State| {
                let prev_haps = prev.query(state);
                let next_haps = next_c.query(state);
                let mut out = Vec::new();
                for nh in next_haps {
                    let t = nh.whole_or_part().begin;
                    let alpha = if t <= eval_cycle {
                        0.0
                    } else if t >= end {
                        1.0
                    } else {
                        ((t - eval_cycle) / dur).to_f64()
                    };
                    let mut merged = nh.value.clone();
                    if let Some(ph) = prev_haps.first() {
                        for (k, nv) in &nh.value {
                            if let (Some(pf), Some(nf)) =
                                (ph.value.get(k).and_then(Value::as_f64), nv.as_f64())
                            {
                                merged.insert(
                                    k.clone(),
                                    Value::Float(pf + (nf - pf) * alpha),
                                );
                            }
                        }
                    }
                    out.push(Hap {
                        whole: nh.whole,
                        part: nh.part,
                        value: merged,
                        context: nh.context,
                    });
                }
                out
            })
        }
        TransitionSpec::JumpIn { cycles } => {
            let switch_at = eval_cycle + CycleTime::from_integer(*cycles);
            switch_at_time(prev, next.clone(), switch_at)
        }
        TransitionSpec::JumpInPrime { cycles } => {
            let ceil_eval = CycleTime::from_integer(eval_cycle.ceil_i64());
            let switch_at = ceil_eval + CycleTime::from_integer(*cycles);
            switch_at_time(prev, next.clone(), switch_at)
        }
        TransitionSpec::JumpMod { cycle_mod } => {
            let m = (*cycle_mod).max(1);
            let mut cand = eval_cycle.ceil_i64();
            if CycleTime::from_integer(cand) <= eval_cycle {
                cand += 1;
            }
            while cand.rem_euclid(m) != 0 {
                cand += 1;
            }
            let switch_at = CycleTime::from_integer(cand);
            switch_at_time(prev, next.clone(), switch_at)
        }
        TransitionSpec::Histpan { count } => {
            let n = (*count).max(1);
            let mut list = Vec::with_capacity(n);
            list.push(next.clone());
            for old in history.iter().take(n.saturating_sub(1)) {
                list.push(old.clone());
            }
            let total = list.len();
            let panned: Vec<ControlPattern> = list
                .into_iter()
                .enumerate()
                .map(|(idx, pat)| {
                    let pan_val = if total <= 1 {
                        0.5
                    } else {
                        (idx as f64) / ((total - 1) as f64)
                    };
                    pat.fmap(move |m| {
                        let mut c = m.clone();
                        c.insert("pan".to_string(), Value::Float(pan_val));
                        c
                    })
                })
                .collect();
            stack(panned)
        }
        TransitionSpec::Wait { cycles } => {
            let resume_at = eval_cycle + *cycles;
            let next_c = next.clone();
            Pattern::new(move |state: &State| {
                next_c
                    .query(state)
                    .into_iter()
                    .filter(|h| h.whole_or_part().begin >= resume_at)
                    .collect()
            })
        }
        TransitionSpec::WaitT { cycles, inner } => {
            let resume_at = eval_cycle + *cycles;
            let inner_pat = compile_transition_pattern(inner, history, next, resume_at);
            Pattern::new(move |state: &State| {
                inner_pat
                    .query(state)
                    .into_iter()
                    .filter(|h| h.whole_or_part().begin >= resume_at)
                    .collect()
            })
        }
        TransitionSpec::Wash {
            delay,
            duration,
            out_transform,
            in_transform,
        } => {
            let t1 = eval_cycle + *delay;
            let t2 = t1 + *duration;
            let prev_washed = (out_transform)(prev.clone());
            let next_washed = (in_transform)(next.clone());
            let next_clean = next.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for h in prev.query(state) {
                    if h.whole_or_part().begin < t1 {
                        out.push(h);
                    }
                }
                for h in prev_washed.query(state) {
                    let t = h.whole_or_part().begin;
                    if t >= t1 && t < t2 {
                        out.push(h);
                    }
                }
                for h in next_washed.query(state) {
                    let t = h.whole_or_part().begin;
                    if t >= t1 && t < t2 {
                        out.push(h);
                    }
                }
                for h in next_clean.query(state) {
                    if h.whole_or_part().begin >= t2 {
                        out.push(h);
                    }
                }
                out
            })
        }
        TransitionSpec::WashIn {
            delay,
            duration,
            transform,
        } => {
            let t1 = eval_cycle + *delay;
            let t2 = t1 + *duration;
            let next_washed = (transform)(next.clone());
            let next_clean = next.clone();
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for h in next_clean.query(state) {
                    let t = h.whole_or_part().begin;
                    if t < t1 || t >= t2 {
                        out.push(h);
                    }
                }
                for h in next_washed.query(state) {
                    let t = h.whole_or_part().begin;
                    if t >= t1 && t < t2 {
                        out.push(h);
                    }
                }
                out
            })
        }
        TransitionSpec::Mortal { lifespan, release } => {
            let fade_start = eval_cycle + *lifespan;
            let silence_at = fade_start + *release;
            let next_c = next.clone();
            let rel = (*release).max(CycleTime::new_const(1, 1000));
            Pattern::new(move |state: &State| {
                let mut out = Vec::new();
                for mut h in next_c.query(state) {
                    let t = h.whole_or_part().begin;
                    if t < fade_start {
                        out.push(h);
                    } else if t < silence_at {
                        let g = 1.0 - ((t - fade_start) / rel).to_f64();
                        if g > 0.0 {
                            scale_hap_gain(&mut h.value, g);
                            out.push(h);
                        }
                    }
                }
                out
            })
        }
    }
}

fn switch_at_time(
    prev: ControlPattern,
    next: ControlPattern,
    switch_at: CycleTime,
) -> ControlPattern {
    Pattern::new(move |state: &State| {
        let mut out = Vec::new();
        for h in prev.query(state) {
            if h.whole_or_part().begin < switch_at {
                out.push(h);
            }
        }
        for h in next.query(state) {
            if h.whole_or_part().begin >= switch_at {
                out.push(h);
            }
        }
        out
    })
}

/// Helper querying a `ControlPattern` over `[start, stop)` cycles.
pub fn query_transition_arc(
    pat: &ControlPattern,
    start: CycleTime,
    stop: CycleTime,
) -> Vec<Hap<CoreControlMap>> {
    pat.query(&State::new(TimeSpan::new_clamped(start, stop)))
}
