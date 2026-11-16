//! 64 DAW-Automatable Macro Parameters (`slider(0..63)`), CLAP Polyphonic Parameter Modulation
//! (`CLAP_EVENT_PARAM_MOD` per `note_id` / voice), and Sample-Accurate Parameter Smoothing
//! (`08-SPEC` §8.2.3, `PP-12`).

#![forbid(unsafe_code)]

use crate::layout::NUM_DAW_MACRO_PARAMS;
use std::collections::BTreeMap;

/// Unique 32-bit CLAP/VST3 parameter ID (`0..63` for macro sliders, `100` for Octatrack `xfader`).
pub type ParamId = u32;

/// Parameter ID reserved for the Octatrack Scene A/B Crossfader (`xfader` — `CK-03`).
pub const PARAM_ID_XFADER: ParamId = 100;

/// Single DAW-automatable parameter descriptor & smoothed state (`08-SPEC` §8.2.3).
#[derive(Debug, Clone, PartialEq)]
pub struct MacroParamSlot {
    pub id: ParamId,
    pub name: String,
    pub default_value: f32,
    pub current_value: f32,
    pub target_value: f32,
    pub min_value: f32,
    pub max_value: f32,
    pub supports_poly_modulation: bool,
}

impl MacroParamSlot {
    /// Creates a normalized `[0.0, 1.0]` macro slider parameter slot.
    pub fn new_normalized(id: ParamId, name: &str, default_value: f32) -> Self {
        let clamped = default_value.clamp(0.0, 1.0);
        Self {
            id,
            name: name.to_string(),
            default_value: clamped,
            current_value: clamped,
            target_value: clamped,
            min_value: 0.0,
            max_value: 1.0,
            supports_poly_modulation: true,
        }
    }

    /// Advances 1-pole exponential smoothing toward `target_value` over `frames` samples (zero zipper noise).
    #[inline]
    pub fn step_smoothing(&mut self, frames: usize, coeff: f32) -> f32 {
        let c = coeff.clamp(0.001, 1.0);
        for _ in 0..frames.max(1) {
            self.current_value += (self.target_value - self.current_value) * c;
        }
        self.current_value
    }
}

/// Sample-accurate DAW host parameter automation or CLAP polyphonic modulation event (`08-SPEC` §8.2.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DawAutomationEvent {
    /// Monophonic DAW automation lane update (`CLAP_EVENT_PARAM_VALUE`).
    ParamValue {
        sample_offset: u32,
        param_id: ParamId,
        normalized_value: f32,
    },
    /// CLAP non-destructive polyphonic modulation offset (`CLAP_EVENT_PARAM_MOD`).
    PolyParamMod {
        sample_offset: u32,
        param_id: ParamId,
        note_id: i32,
        amount: f32,
    },
}

/// Bank of 64 DAW-automatable macro parameters (`slider(0..63)`) + `xfader` (`100`) and CLAP polyphonic offsets.
#[derive(Debug, Clone)]
pub struct DawMacroParamBank {
    pub slots: Vec<MacroParamSlot>,
    pub xfader: MacroParamSlot,
    poly_mod_by_note: BTreeMap<(ParamId, i32), f32>,
}

impl Default for DawMacroParamBank {
    fn default() -> Self {
        Self::new()
    }
}

impl DawMacroParamBank {
    /// Initializes all 64 macro slider slots (`0..63`) plus the `xfader` (`100`) slot.
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(NUM_DAW_MACRO_PARAMS);
        for i in 0..NUM_DAW_MACRO_PARAMS {
            slots.push(MacroParamSlot::new_normalized(
                i as ParamId,
                &format!("Macro Slider {}", i),
                0.5,
            ));
        }
        Self {
            slots,
            xfader: MacroParamSlot::new_normalized(PARAM_ID_XFADER, "Scene A/B Xfader", 0.0),
            poly_mod_by_note: BTreeMap::new(),
        }
    }

    /// Applies a `DawAutomationEvent` from the DAW host.
    pub fn apply_automation(&mut self, event: DawAutomationEvent) {
        match event {
            DawAutomationEvent::ParamValue {
                param_id,
                normalized_value,
                ..
            } => {
                let v = normalized_value.clamp(0.0, 1.0);
                if param_id == PARAM_ID_XFADER {
                    self.xfader.target_value = v;
                    self.xfader.current_value = v;
                } else if let Some(slot) = self.slots.get_mut(param_id as usize) {
                    slot.target_value = v;
                    slot.current_value = v;
                }
            }
            DawAutomationEvent::PolyParamMod {
                param_id,
                note_id,
                amount,
                ..
            } => {
                self.poly_mod_by_note
                    .insert((param_id, note_id), amount.clamp(-1.0, 1.0));
            }
        }
    }

    /// Returns the effective value for `(param_id, note_id)` combining base automation + CLAP polyphonic modulation.
    pub fn effective_value(&self, param_id: ParamId, note_id: Option<i32>) -> f32 {
        let base = if param_id == PARAM_ID_XFADER {
            self.xfader.current_value
        } else {
            self.slots
                .get(param_id as usize)
                .map(|s| s.current_value)
                .unwrap_or(0.0)
        };
        let poly_offset = note_id
            .and_then(|nid| self.poly_mod_by_note.get(&(param_id, nid)).copied())
            .unwrap_or(0.0);
        (base + poly_offset).clamp(0.0, 1.0)
    }

    /// Clears polyphonic modulation state when a voice's `note_id` ends.
    pub fn clear_note_modulation(&mut self, note_id: i32) {
        self.poly_mod_by_note.retain(|&(_, nid), _| nid != note_id);
    }
}
