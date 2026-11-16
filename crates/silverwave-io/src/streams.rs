//! Stream Slot Registry (`d1..d16`, `$:`, `p`) & Mute/Solo/Hush/Panic Lifecycle
//! (`04-SPEC` Section 6.1, `TRN-09`, `PP-07`).

use crate::transitions::{compile_transition_pattern, TransitionEngine, TransitionSpec};
use crate::types::{CycleArc, CycleTime, DispatchEvent, IoError, OutputSink, StreamId};
use silverwave_core::{silence, stack, ControlPattern, State, TimeSpan, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Per-stream slot state retaining pattern history for `histpan` and transitions (`Section 6.1`).
#[derive(Clone, Debug)]
pub struct StreamSlotState<P> {
    pub id: StreamId,
    pub history: Vec<P>,
    pub muted: bool,
    pub soloed: bool,
    pub orbit: u16,
}

/// Stream lifecycle management contract (`Section 6.1`, `TRN-09`).
pub trait StreamManager<P>: Send + Sync {
    fn set_pattern(&self, id: &StreamId, pattern: P);
    fn apply_dollar_colon_block(&self, patterns: Vec<P>);
    fn silence(&self, id: &StreamId);
    fn mute(&self, id: &StreamId);
    fn unmute(&self, id: &StreamId);
    fn mute_all(&self);
    fn unmute_all(&self);
    fn solo(&self, id: &StreamId);
    fn unsolo(&self, id: &StreamId);
    fn hush(&self) -> Result<(), IoError>;
    fn panic(&self) -> Result<(), IoError>;
    fn active_streams(&self) -> Vec<StreamId>;
}

/// Concrete `StreamManager<ControlPattern>` + `TransitionEngine<ControlPattern>` (`TRN-01`..`TRN-09`).
#[derive(Default)]
pub struct SilverWaveStreamRegistry {
    slots: Mutex<BTreeMap<StreamId, StreamSlotState<ControlPattern>>>,
    sinks: Mutex<Vec<Arc<dyn OutputSink>>>,
    last_dollar_colon_count: Mutex<usize>,
}

impl SilverWaveStreamRegistry {
    /// Creates an empty `SilverWaveStreamRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an `OutputSink` (`SuperDirt`, `MidiEngine`, `CsoundEngine`) to receive events and `panic()`.
    pub fn add_sink(&self, sink: Arc<dyn OutputSink>) {
        if let Ok(mut g) = self.sinks.lock() {
            g.push(sink);
        }
    }

    /// Evaluates all audible (non-muted, solo-filtered) streams over `[start, stop)` into `DispatchEvent`s (`TRN-09`).
    pub fn query_audible_events(
        &self,
        start: CycleTime,
        stop: CycleTime,
        cps: f64,
    ) -> Vec<DispatchEvent> {
        let g = self.slots.lock().unwrap();
        let any_solo = g.values().any(|s| s.soloed);
        let mut events = Vec::new();
        let state = State::new(TimeSpan::new_clamped(start, stop));

        for slot in g.values() {
            if slot.muted || (any_solo && !slot.soloed) {
                continue;
            }
            let Some(pat) = slot.history.first() else {
                continue;
            };
            for h in pat.query(&state) {
                if !h.has_onset() {
                    continue;
                }
                let mut cmap = crate::types::ControlMap::new();
                for (k, v) in &h.value {
                    let cv = match v {
                        Value::Float(f) => crate::types::ControlValue::Float(*f),
                        Value::Int(i) => crate::types::ControlValue::Int(*i),
                        Value::String(s) => crate::types::ControlValue::Str(s.clone()),
                        Value::Rational(r) => crate::types::ControlValue::Float(r.to_f64()),
                        _ => continue,
                    };
                    cmap.insert(k.clone(), cv);
                }
                let whole_arc = h.whole.map(CycleArc::from_timespan);
                let part_arc = CycleArc::from_timespan(h.part);
                let onset_cycle = h.whole_or_part().begin.to_f64();
                let dur_cycles = h.whole_or_part().duration().to_f64();
                events.push(DispatchEvent {
                    whole: whole_arc,
                    part: part_arc,
                    value: cmap,
                    onset_secs: onset_cycle / cps.max(0.01),
                    duration_secs: dur_cycles / cps.max(0.01),
                    cps,
                    stream_id: slot.id.clone(),
                });
            }
        }
        events
    }

    /// Returns the combined `ControlPattern` across all audible streams.
    pub fn combined_audible_pattern(&self) -> ControlPattern {
        let g = self.slots.lock().unwrap();
        let any_solo = g.values().any(|s| s.soloed);
        let pats: Vec<ControlPattern> = g
            .values()
            .filter(|s| !s.muted && (!any_solo || s.soloed))
            .filter_map(|s| s.history.first().cloned())
            .collect();
        if pats.is_empty() {
            silence()
        } else {
            stack(pats)
        }
    }
}

impl StreamManager<ControlPattern> for SilverWaveStreamRegistry {
    fn set_pattern(&self, id: &StreamId, pattern: ControlPattern) {
        if let Ok(mut g) = self.slots.lock() {
            let entry = g.entry(id.clone()).or_insert_with(|| StreamSlotState {
                id: id.clone(),
                history: Vec::new(),
                muted: false,
                soloed: false,
                orbit: 0,
            });
            entry.history.insert(0, pattern);
            if entry.history.len() > 16 {
                entry.history.truncate(16);
            }
        }
    }

    fn apply_dollar_colon_block(&self, patterns: Vec<ControlPattern>) {
        let new_len = patterns.len();
        let prev_len = self
            .last_dollar_colon_count
            .lock()
            .map(|mut c| {
                let old = *c;
                *c = new_len;
                old
            })
            .unwrap_or(0);

        for (i, pat) in patterns.into_iter().enumerate() {
            self.set_pattern(&StreamId(format!("$:{}", i)), pat);
        }
        // Silence any orphaned `$:k` slots from previous evaluation (`Section 6.1`, `TRN-09`)
        for i in new_len..prev_len {
            self.silence(&StreamId(format!("$:{}", i)));
        }
    }

    fn silence(&self, id: &StreamId) {
        if let Ok(mut g) = self.slots.lock() {
            g.remove(id);
        }
    }

    fn mute(&self, id: &StreamId) {
        if let Ok(mut g) = self.slots.lock() {
            if let Some(s) = g.get_mut(id) {
                s.muted = true;
            }
        }
    }

    fn unmute(&self, id: &StreamId) {
        if let Ok(mut g) = self.slots.lock() {
            if let Some(s) = g.get_mut(id) {
                s.muted = false;
            }
        }
    }

    fn mute_all(&self) {
        if let Ok(mut g) = self.slots.lock() {
            for s in g.values_mut() {
                s.muted = true;
            }
        }
    }

    fn unmute_all(&self) {
        if let Ok(mut g) = self.slots.lock() {
            for s in g.values_mut() {
                s.muted = false;
            }
        }
    }

    fn solo(&self, id: &StreamId) {
        if let Ok(mut g) = self.slots.lock() {
            if let Some(s) = g.get_mut(id) {
                s.soloed = true;
            }
        }
    }

    fn unsolo(&self, id: &StreamId) {
        if let Ok(mut g) = self.slots.lock() {
            if let Some(s) = g.get_mut(id) {
                s.soloed = false;
            }
        }
    }

    fn hush(&self) -> Result<(), IoError> {
        if let Ok(mut g) = self.slots.lock() {
            g.clear();
        }
        self.panic()
    }

    fn panic(&self) -> Result<(), IoError> {
        if let Ok(sinks) = self.sinks.lock() {
            for sink in sinks.iter() {
                sink.panic()?;
            }
        }
        Ok(())
    }

    fn active_streams(&self) -> Vec<StreamId> {
        self.slots
            .lock()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl TransitionEngine<ControlPattern> for SilverWaveStreamRegistry {
    fn apply_transition(
        &self,
        id: &StreamId,
        spec: TransitionSpec<ControlPattern>,
        next: ControlPattern,
        eval_cycle: CycleTime,
    ) -> Result<(), IoError> {
        let mut g = self
            .slots
            .lock()
            .map_err(|_| IoError::Config("stream registry lock poisoned".to_string()))?;
        let history = g
            .get(id)
            .map(|s| s.history.clone())
            .unwrap_or_default();
        let transitioned = compile_transition_pattern(&spec, &history, &next, eval_cycle);
        let entry = g.entry(id.clone()).or_insert_with(|| StreamSlotState {
            id: id.clone(),
            history: Vec::new(),
            muted: false,
            soloed: false,
            orbit: 0,
        });
        entry.history.insert(0, transitioned);
        if entry.history.len() > 16 {
            entry.history.truncate(16);
        }
        Ok(())
    }
}
