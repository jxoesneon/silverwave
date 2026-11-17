//! Csound Backend (`@strudel/csound` Parity — `04-SPEC` Section 2.5, `AUD-04`).

use crate::types::{DispatchEvent, IoError, OutputSink};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Csound `.orc` orchestra source descriptor (`Section 2.5`, `AUD-04`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrcSource {
    Inline(String),
    Url(String),
    /// Supports Strudel's `github:user/repo/branch/path.orc` shorthand (`AUD-04`).
    GithubShorthand(String),
    FilePath(String),
}

impl OrcSource {
    /// Resolves `github:user/repo/branch/path.orc` into its raw GitHub URL (`AUD-04`).
    pub fn resolve_url(&self) -> Option<String> {
        match self {
            Self::Url(u) => Some(u.clone()),
            Self::GithubShorthand(s) => {
                let rest = s.strip_prefix("github:").unwrap_or(s);
                Some(format!("https://raw.githubusercontent.com/{}", rest))
            }
            _ => None,
        }
    }
}

/// Csound engine contract (`Section 2.5`, `AUD-04`).
pub trait CsoundEngine: OutputSink {
    fn init_csound(&mut self, sample_rate: u32) -> Result<(), IoError>;
    fn load_orc(&mut self, source: OrcSource) -> Result<(), IoError>;
    fn start_instrument(&self, instrument: &str) -> Result<(), IoError>;
    fn stop_instrument(&self, instrument: &str) -> Result<(), IoError>;
    fn send_score_event(&self, score_line: &str) -> Result<(), IoError>;
    fn set_control_channel(&self, channel: &str, value: f64) -> Result<(), IoError>;
}

/// In-process `@strudel/csound` engine recording score lines and `chnset` control channels (`AUD-04`).
#[derive(Default)]
pub struct InProcessCsoundEngine {
    pub sample_rate: u32,
    pub loaded_orcs: Vec<String>,
    pub score_log: Mutex<Vec<String>>,
    pub channels: Mutex<BTreeMap<String, f64>>,
}

impl InProcessCsoundEngine {
    /// Creates a new `InProcessCsoundEngine`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns all emitted Csound score lines (`i "Instr" ...`).
    pub fn emitted_score_lines(&self) -> Vec<String> {
        self.score_log.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Reads a Csound `chnset` channel value (`AUD-04`).
    pub fn get_control_channel(&self, channel: &str) -> Option<f64> {
        self.channels
            .lock()
            .ok()
            .and_then(|g| g.get(channel).copied())
    }
}

impl OutputSink for InProcessCsoundEngine {
    fn name(&self) -> &str {
        "csound"
    }

    fn send_batch(&self, events: &[DispatchEvent]) -> Result<(), IoError> {
        for ev in events {
            let instr = ev
                .value
                .get("csound")
                .or_else(|| ev.value.get("s"))
                .and_then(|v| v.as_str())
                .unwrap_or("Mono");
            let freq = ev
                .value
                .get("freq")
                .and_then(|v| v.as_f64())
                .unwrap_or(440.0);
            let gain = ev.value.get("gain").and_then(|v| v.as_f64()).unwrap_or(0.8);
            let line = format!(
                "i \"{}\" {:.6} {:.6} {:.2} {:.3}",
                instr, ev.onset_secs, ev.duration_secs, freq, gain
            );
            self.send_score_event(&line)?;
        }
        Ok(())
    }

    fn panic(&self) -> Result<(), IoError> {
        self.send_score_event("e")
    }

    fn close(&self) -> Result<(), IoError> {
        Ok(())
    }
}

impl CsoundEngine for InProcessCsoundEngine {
    fn init_csound(&mut self, sample_rate: u32) -> Result<(), IoError> {
        self.sample_rate = sample_rate.max(8_000);
        Ok(())
    }

    fn load_orc(&mut self, source: OrcSource) -> Result<(), IoError> {
        let resolved = match &source {
            OrcSource::Inline(s) => s.clone(),
            OrcSource::FilePath(p) => p.clone(),
            // Only `Url` and `GithubShorthand` reach this arm, and both always
            // resolve to `Some`, so an invalid-source error cannot occur here.
            other => other
                .resolve_url()
                .expect("Url and GithubShorthand orc sources always resolve"),
        };
        self.loaded_orcs.push(resolved);
        Ok(())
    }

    fn start_instrument(&self, instrument: &str) -> Result<(), IoError> {
        self.send_score_event(&format!("i \"{}\" 0 -1", instrument))
    }

    fn stop_instrument(&self, instrument: &str) -> Result<(), IoError> {
        self.send_score_event(&format!("i -\"{}\" 0 0", instrument))
    }

    fn send_score_event(&self, score_line: &str) -> Result<(), IoError> {
        if let Ok(mut guard) = self.score_log.lock() {
            guard.push(score_line.to_string());
        }
        Ok(())
    }

    fn set_control_channel(&self, channel: &str, value: f64) -> Result<(), IoError> {
        if let Ok(mut guard) = self.channels.lock() {
            guard.insert(channel.to_string(), value);
        }
        Ok(())
    }
}
