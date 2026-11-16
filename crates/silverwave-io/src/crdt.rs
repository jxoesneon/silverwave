//! Cycle-Quantized CRDT Ensemble Sync & Dual-Cursor `[HUMAN]` + `[MCP AGENT]` B2B Mode
//! (`04-SPEC` Section 8.6, `CK-09`, `09-SPEC` Section 3.9).

use crate::types::CycleTime;
use std::collections::BTreeMap;

/// Identifies whether an edit or cursor belongs to a human performer or an MCP co-performer agent (`CK-09`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    HumanPerformer,
    McpAgent,
}

/// Real-time collaborative cursor & evaluation badge (`[HUMAN: Performer]` vs `[MCP AGENT]` — `CK-09`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorCursorBadge {
    pub peer_id: String,
    pub display_name: String,
    pub kind: ActorKind,
    pub byte_offset: usize,
    pub color_rgba: [u8; 4],
}

/// Cycle-Quantized AST Commit Epoch synchronizing ensemble code activation on the exact same downbeat (`CK-09`).
#[derive(Clone, Debug, PartialEq)]
pub struct CommitEpoch {
    pub epoch_id: u64,
    pub author_peer_id: String,
    pub actor_kind: ActorKind,
    pub target_cycle: CycleTime,
    pub dac_latency_offset_ms: f64,
    pub stream_id: String,
    pub code_patch: String,
}

/// Multi-peer CRDT Ensemble Session coordinating quantized `CommitEpoch` activations and DAC latency offsets (`CK-09`).
#[derive(Default)]
pub struct CrdtEnsembleSession {
    pub local_peer_id: String,
    pub local_dac_latency_ms: f64,
    pub cursors: BTreeMap<String, ActorCursorBadge>,
    pub staged_epochs: Vec<CommitEpoch>,
    pub active_code_by_stream: BTreeMap<String, String>,
}

impl CrdtEnsembleSession {
    /// Creates a new `CrdtEnsembleSession` for `local_peer_id` with `local_dac_latency_ms`.
    pub fn new(local_peer_id: &str, local_dac_latency_ms: f64) -> Self {
        Self {
            local_peer_id: local_peer_id.to_string(),
            local_dac_latency_ms,
            cursors: BTreeMap::new(),
            staged_epochs: Vec::new(),
            active_code_by_stream: BTreeMap::new(),
        }
    }

    /// Updates a peer's cursor badge (`[HUMAN]` or `[MCP AGENT]`).
    pub fn update_cursor(&mut self, badge: ActorCursorBadge) {
        self.cursors.insert(badge.peer_id.clone(), badge);
    }

    /// Stages a cycle-quantized `CommitEpoch` to activate on `target_cycle`.
    pub fn stage_commit(&mut self, epoch: CommitEpoch) {
        self.staged_epochs.push(epoch);
        self.staged_epochs.sort_by(|a, b| {
            a.target_cycle
                .cmp(&b.target_cycle)
                .then(a.epoch_id.cmp(&b.epoch_id))
        });
    }

    /// Computes the local wall-clock activation time in seconds for `epoch`, compensating for
    /// `local_dac_latency_ms` so acoustic downbeats align across peers (`CK-09`).
    pub fn acoustic_activation_wall_secs(&self, epoch: &CommitEpoch, cps: f64) -> f64 {
        let ideal_wall = epoch.target_cycle.to_f64() / cps.max(0.01);
        ideal_wall - (self.local_dac_latency_ms * 0.001)
    }

    /// Commits all staged epochs whose `target_cycle <= current_cycle` and returns the activated epochs.
    pub fn poll_due_epochs(&mut self, current_cycle: CycleTime) -> Vec<CommitEpoch> {
        let mut due = Vec::new();
        let mut remaining = Vec::new();
        for ep in self.staged_epochs.drain(..) {
            if ep.target_cycle <= current_cycle {
                self.active_code_by_stream
                    .insert(ep.stream_id.clone(), ep.code_patch.clone());
                due.push(ep);
            } else {
                remaining.push(ep);
            }
        }
        self.staged_epochs = remaining;
        due
    }
}
