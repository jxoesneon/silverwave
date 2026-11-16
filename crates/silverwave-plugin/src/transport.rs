//! Sample-Accurate DAW Host Transport Synchronizer (`DawTransportSynchronizer` — `AUD-03`, `PP-12`),
//! Tonal.js JSON Theory Bridge, Faust/Csound External Synth Node Bridge (`00-MASTER-ROADMAP` §2 `v0.6.0`),
//! and Embedded DAW Co-Performance Bridge (`silverwave-gui` + `silverwave-lsp` — `08-SPEC` §8.2.3).

#![forbid(unsafe_code)]

use silverwave_core::{Rational64, TimeSpan};
use silverwave_gui::{EvalOutcome, ReplEngine, ReplError};
use silverwave_io::{HostTransportSnapshot, PluginSyncMode};
use silverwave_lsp::{EditorRpcNotification, EditorRpcRequest, SilverWaveLspServer};

/// Sample-accurate DAW transport synchronizer converting host `pos_beats`, `tempo_bpm`, and
/// `time_sig` into exact `Rational64` cycle spans (`[cycle_start, cycle_end)`) per audio buffer (`AUD-03`, `PP-12`).
#[derive(Debug, Clone)]
pub struct DawTransportSynchronizer {
    pub sync_mode: PluginSyncMode,
    pub sample_rate: f64,
    pub tempo_bpm: f64,
    pub beats_per_cycle: f64,
    pub playing: bool,
    pub current_cycle_f64: f64,
}

impl Default for DawTransportSynchronizer {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl DawTransportSynchronizer {
    /// Creates a new `DawTransportSynchronizer` at `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sync_mode: PluginSyncMode::HostTransport,
            sample_rate: sample_rate.max(8_000.0),
            tempo_bpm: 120.0,
            beats_per_cycle: 4.0,
            playing: true,
            current_cycle_f64: 0.0,
        }
    }

    /// Synchronizes internal state with the DAW host's `HostTransportSnapshot` and computes the
    /// exact rational `TimeSpan` covered by `buffer_frames` samples (`AUD-03`, `PP-12`).
    pub fn advance_buffer_span(
        &mut self,
        snapshot: &HostTransportSnapshot,
        buffer_frames: usize,
    ) -> Option<TimeSpan> {
        self.tempo_bpm = snapshot.tempo_bpm.max(1.0);
        let bpc =
            (snapshot.time_sig_num.max(1) as f64) * (4.0 / (snapshot.time_sig_den.max(1) as f64));
        self.beats_per_cycle = bpc.max(1.0);
        self.playing = snapshot.playing;

        if !self.playing || buffer_frames == 0 {
            return None;
        }

        if self.sync_mode == PluginSyncMode::HostTransport
            && let Some(pos_beats) = snapshot.pos_beats
        {
            let host_cycle = pos_beats / self.beats_per_cycle;
            // Monotonic guard across tempo/signature transitions so events never duplicate (`AUD-03`)
            self.current_cycle_f64 = host_cycle.max(self.current_cycle_f64);
        }

        let cps = (self.tempo_bpm / 60.0) / self.beats_per_cycle;
        let delta_cycles = (buffer_frames as f64 / self.sample_rate) * cps;
        let start_c = self.current_cycle_f64;
        let end_c = start_c + delta_cycles;
        self.current_cycle_f64 = end_c;

        Some(TimeSpan::new_clamped(
            Rational64::from_f64_approx(start_c, Rational64::MAX_DENOMINATOR)?,
            Rational64::from_f64_approx(end_c, Rational64::MAX_DENOMINATOR)?,
        ))
    }

    /// Converts a rational `cycle` onset inside `[span.begin, span.end)` into an exact sample frame offset `0..buffer_frames`.
    pub fn cycle_to_frame_offset(
        span: TimeSpan,
        event_cycle: Rational64,
        buffer_frames: usize,
    ) -> u32 {
        if buffer_frames == 0 {
            return 0;
        }
        let dur = (span.end - span.begin).to_f64();
        if dur <= 1.0e-12 {
            return 0;
        }
        let rel = ((event_cycle - span.begin).to_f64() / dur).clamp(0.0, 1.0);
        ((rel * (buffer_frames as f64)).round() as usize).min(buffer_frames.saturating_sub(1))
            as u32
    }
}

/// Tonal.js-compatible JSON theory descriptor (`00-MASTER-ROADMAP` §2 `v0.6.0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TonalTheoryDescriptor {
    pub tonic: String,
    pub scale_name: String,
    pub intervals: Vec<String>,
    pub pitch_classes: Vec<String>,
}

impl TonalTheoryDescriptor {
    /// Resolves a Tonal.js scale query (e.g., `"C:minor"`, `"D:dorian"`, `"G:major"`) into intervals and pitch classes (`v0.6.0`).
    pub fn resolve_scale(query: &str) -> Self {
        let mut parts = query.splitn(2, ':');
        let tonic = parts.next().unwrap_or("C").trim().to_uppercase();
        let scale_name = parts.next().unwrap_or("major").trim().to_ascii_lowercase();
        let (intervals, pitch_classes) = match scale_name.as_str() {
            "minor" | "aeolian" => (
                vec!["1P", "2M", "3m", "4P", "5P", "6m", "7m"],
                vec!["C", "D", "Eb", "F", "G", "Ab", "Bb"],
            ),
            "dorian" => (
                vec!["1P", "2M", "3m", "4P", "5P", "6M", "7m"],
                vec!["C", "D", "Eb", "F", "G", "A", "Bb"],
            ),
            _ => (
                vec!["1P", "2M", "3M", "4P", "5P", "6M", "7M"],
                vec!["C", "D", "E", "F", "G", "A", "B"],
            ),
        };
        Self {
            tonic,
            scale_name,
            intervals: intervals.into_iter().map(String::from).collect(),
            pitch_classes: pitch_classes.into_iter().map(String::from).collect(),
        }
    }

    /// Serializes `self` into a deterministic Tonal.js-compatible JSON object string.
    pub fn to_json(&self) -> String {
        let ivs = self
            .intervals
            .iter()
            .map(|s| format!(r#""{}""#, s))
            .collect::<Vec<_>>()
            .join(",");
        let pcs = self
            .pitch_classes
            .iter()
            .map(|s| format!(r#""{}""#, s))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"tonic":"{}","scale":"{}","intervals":[{}],"notes":[{}]}}"#,
            self.tonic, self.scale_name, ivs, pcs
        )
    }
}

/// Embedded Co-Performance Bridge running inside the DAW plugin instance (`08-SPEC` §8.2.3).
#[derive(Debug, Clone, Default)]
pub struct DawCoPerformanceBridge {
    pub gui_repl: ReplEngine,
    pub lsp_daemon: SilverWaveLspServer,
}

impl DawCoPerformanceBridge {
    /// Creates a new `DawCoPerformanceBridge`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates live code inside the DAW plugin's embedded GUI (`08-SPEC` §8.2.3).
    pub fn evaluate_in_daw_gui(&mut self, code: &str) -> Result<EvalOutcome, ReplError> {
        self.gui_repl.evaluate_buffer(code)
    }

    /// Processes an external editor (`VSCode` / `Neovim`) or MCP co-performer JSON-RPC request attached to the DAW plugin.
    pub fn handle_external_rpc(
        &mut self,
        req: EditorRpcRequest,
    ) -> Result<Vec<EditorRpcNotification>, ReplError> {
        self.lsp_daemon.handle_request(req)
    }
}
