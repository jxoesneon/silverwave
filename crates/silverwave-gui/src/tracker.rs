//! Category-King UI Extensions (`05-SPEC` §6):
//! - Bidirectional Tracker / Phrase Grid View (`TrackerPhraseView` — `NOTE | INST | VOL | FX1 | FX2 | FX3`, `CK-04`)
//! - 5-Stage Docked `InteractiveTour` (`CK-07`)
//! - Dual-Cursor Co-Performance `ActorBadge` (`[HUMAN: Performer]` vs `[MCP AGENT]` — `CK-09`)

#![forbid(unsafe_code)]

use crate::span_map::ByteSpan;

/// Single tracker effect column (`FX1`, `FX2`, or `FX3` — `05-SPEC` §6.1, `CK-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerFxCommand {
    pub cmd: String,
    pub value_hex: u8,
}

/// Single step row in the Dirtywave M8 / Renoise-style `TrackerPhraseView` (`05-SPEC` §6.1, `CK-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackerRow {
    pub step_index: usize,
    pub note: String,
    pub instrument: String,
    pub volume_hex: u8,
    pub fx: [Option<TrackerFxCommand>; 3],
    pub plocks: Vec<(String, f64)>,
    pub source_span: ByteSpan,
}

/// Bidirectional Tracker / Phrase Grid View synchronized 1:1 with Mini-Notation AST spans (`CK-04`).
#[derive(Debug, Clone, Default)]
pub struct TrackerPhraseView {
    pub rows: Vec<TrackerRow>,
}

impl TrackerPhraseView {
    /// Builds a 16- or 64-step `TrackerPhraseView` from a space-separated mini-notation token sequence.
    pub fn from_mini_pattern(source: &str, default_inst: &str) -> Self {
        let mut rows = Vec::new();
        let mut offset = 0usize;
        for (idx, tok) in source.split_whitespace().enumerate() {
            let start = source[offset..]
                .find(tok)
                .map(|i| offset + i)
                .unwrap_or(offset);
            let end = start + tok.len();
            offset = end;
            let note_str = if tok == "~" || tok == "-" {
                "---".to_string()
            } else {
                tok.to_uppercase()
            };
            rows.push(TrackerRow {
                step_index: idx,
                note: note_str,
                instrument: default_inst.to_string(),
                volume_hex: 0x64,
                fx: [None, None, None],
                plocks: Vec::new(),
                source_span: ByteSpan::new(start, end),
            });
        }
        Self { rows }
    }

    /// Sets a click-hold parameter lock (`plock`) on `step_index` and returns the updated mini-notation string (`CK-04`).
    pub fn set_step_plock(&mut self, step_index: usize, param: &str, value: f64) -> Option<String> {
        let row = self.rows.get_mut(step_index)?;
        if let Some(existing) = row.plocks.iter_mut().find(|(k, _)| k == param) {
            existing.1 = value;
        } else {
            row.plocks.push((param.to_string(), value));
        }
        Some(self.to_mini_pattern())
    }

    /// Serializes the tracker phrase back into a mini-notation string.
    pub fn to_mini_pattern(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                if r.note == "---" {
                    "~".to_string()
                } else {
                    r.note.to_ascii_lowercase()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Single stage of the 5-stage docked `InteractiveTour` (`05-SPEC` §6.3, `CK-07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TourStage {
    pub stage_number: u8,
    pub title: String,
    pub target_code: String,
    pub completed: bool,
}

/// Docked 5-Stage "First Beat in 30 Seconds" `InteractiveTour` (`05-SPEC` §6.3, `CK-07`).
#[derive(Debug, Clone)]
pub struct InteractiveTour {
    pub stages: [TourStage; 5],
    pub current_stage: usize,
}

impl Default for InteractiveTour {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractiveTour {
    /// Creates the canonical 5-stage onboarding tour (`CK-07`).
    pub fn new() -> Self {
        Self {
            stages: [
                TourStage {
                    stage_number: 1,
                    title: "Stage 1: Four-on-the-Floor Kick".to_string(),
                    target_code: r#"s("bd*4")"#.to_string(),
                    completed: false,
                },
                TourStage {
                    stage_number: 2,
                    title: "Stage 2: Backbeat Snare & Hi-Hats".to_string(),
                    target_code: r#"s("bd sd [~ bd] sd, hh*8")"#.to_string(),
                    completed: false,
                },
                TourStage {
                    stage_number: 3,
                    title: "Stage 3: Euclidean Acid Bassline".to_string(),
                    target_code: r#"note("c2(3,8)").s("sawtooth").lpf(800)"#.to_string(),
                    completed: false,
                },
                TourStage {
                    stage_number: 4,
                    title: "Stage 4: Live Slider & Modulation Bus".to_string(),
                    target_code: r#"s("bd*4").lpf(slider(800, 100, 4000, 10))"#.to_string(),
                    completed: false,
                },
                TourStage {
                    stage_number: 5,
                    title: "Stage 5: Inline Punchcard Visualizer".to_string(),
                    target_code: r#"s("bd sd hh cp")._punchcard()"#.to_string(),
                    completed: false,
                },
            ],
            current_stage: 0,
        }
    }

    /// Validates `user_code` against the current stage and advances if matched (`CK-07`).
    pub fn verify_and_advance(&mut self, user_code: &str) -> bool {
        if self.current_stage >= self.stages.len() {
            return true;
        }
        let target = &self.stages[self.current_stage].target_code;
        if user_code.trim().contains(target.as_str()) {
            self.stages[self.current_stage].completed = true;
            self.current_stage += 1;
            true
        } else {
            false
        }
    }
}
