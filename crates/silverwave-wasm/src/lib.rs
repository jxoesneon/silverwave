//! `silverwave-wasm` — Browser PWA & CodeMirror 6 Frontend Handle (`SilverwaveWasmHandle`,
//! `FrameUpdate`, Service Worker Offline Asset Manifest & `IndexedDB` Sketch Store —
//! `05-SPEC` §2.4 & §5.5, `PWA-01`).

#![forbid(unsafe_code)]

use silverwave_gui::{
    ByteSpan, EvalOutcome, ReplCommand, ReplEngine, ReplError, ShareCodec, SilverWaveShareCodec,
    SliderPatch, WidgetId,
};
use silverwave_viz::{AudioExportFormat, DrawCmd, HydraUniformFrame};
use std::collections::BTreeMap;

/// Highlight entry projected into UTF-16 CodeMirror 6 offsets (`05-SPEC` §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmHighlightEntry {
    pub from_utf16: usize,
    pub to_utf16: usize,
    pub color: String,
    pub mark_css: Option<String>,
}

/// Inline visualizer frame entry for CodeMirror 6 block widgets (`05-SPEC` §2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct WasmInlineVizEntry {
    pub anchor_line: u32,
    pub viz_id: u32,
    pub commands: Vec<DrawCmd>,
}

/// 60 FPS frame update payload returned by `SilverwaveWasmHandle::tick_frame` (`05-SPEC` §2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameUpdate {
    pub highlights: Vec<WasmHighlightEntry>,
    pub global_viz: Vec<DrawCmd>,
    pub inline_viz: Vec<WasmInlineVizEntry>,
    pub hydra_uniforms: Option<HydraUniformFrame>,
}

/// PWA Service Worker cache strategy (`05-SPEC` §2.4, `PWA-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PwaCacheStrategy {
    CacheFirstImmutable,
    StaleWhileRevalidate,
}

/// Offline PWA asset manifest & simulated `IndexedDB` sketch store (`05-SPEC` §2.4, `PWA-01`).
#[derive(Debug, Clone)]
pub struct PwaOfflineStore {
    pub online: bool,
    pub cached_assets: BTreeMap<String, PwaCacheStrategy>,
    pub indexed_db_sketches: BTreeMap<String, String>,
    pub last_sketch_key: Option<String>,
}

impl Default for PwaOfflineStore {
    fn default() -> Self {
        let mut assets = BTreeMap::new();
        assets.insert("index.html".to_string(), PwaCacheStrategy::CacheFirstImmutable);
        assets.insert(
            "silverwave_bg.wasm".to_string(),
            PwaCacheStrategy::CacheFirstImmutable,
        );
        assets.insert(
            "codemirror-bundle.js".to_string(),
            PwaCacheStrategy::CacheFirstImmutable,
        );
        assets.insert(
            "shaders/hydra_bridge.wgsl".to_string(),
            PwaCacheStrategy::CacheFirstImmutable,
        );
        assets.insert(
            "samples/strudel.json".to_string(),
            PwaCacheStrategy::StaleWhileRevalidate,
        );
        Self {
            online: true,
            cached_assets: assets,
            indexed_db_sketches: BTreeMap::new(),
            last_sketch_key: None,
        }
    }
}

impl PwaOfflineStore {
    /// Saves a sketch into `IndexedDB` so offline reloads restore the user's code (`PWA-01`).
    pub fn save_sketch(&mut self, key: &str, code: &str) {
        self.indexed_db_sketches
            .insert(key.to_string(), code.to_string());
        self.last_sketch_key = Some(key.to_string());
    }

    /// Restores the most recently saved sketch from `IndexedDB` (`PWA-01`).
    pub fn restore_last_sketch(&self) -> Option<String> {
        let k = self.last_sketch_key.as_ref()?;
        self.indexed_db_sketches.get(k).cloned()
    }
}

/// Complete `SilverwaveWasmHandle` implementing the `05-SPEC` §2.4 TypeScript / `wasm-bindgen` contract.
#[derive(Debug, Clone, Default)]
pub struct SilverwaveWasmHandle {
    pub engine: ReplEngine,
    pub offline_store: PwaOfflineStore,
}

impl SilverwaveWasmHandle {
    /// Creates a new `SilverwaveWasmHandle`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates `code` (or optional `block_range` `[start_byte, end_byte]`) and persists to `IndexedDB` (`PWA-01`).
    pub fn evaluate(
        &mut self,
        code: &str,
        block_range: Option<(usize, usize)>,
    ) -> Result<EvalOutcome, ReplError> {
        let out = if let Some((s, e)) = block_range {
            self.engine.evaluate_block(code, ByteSpan::new(s, e))?
        } else {
            self.engine.evaluate_buffer(code)?
        };
        let title_key = out
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| "autosave".to_string());
        self.offline_store.save_sketch(&title_key, code);
        Ok(out)
    }

    /// Halts all active patterns (`Ctrl+.`).
    pub fn halt(&mut self) {
        self.engine.halt();
    }

    /// Updates CPS (`setCps`).
    pub fn set_cps(&mut self, cps: f64) {
        self.engine.set_cps(cps);
    }

    /// Updates an inline slider (`updateSlider`) without re-evaluation (`SLIDER-03`).
    pub fn update_slider(&mut self, widget_id: WidgetId, value: f64) -> Option<SliderPatch> {
        self.engine.drag_slider(widget_id, value)
    }

    /// Steps the 60 FPS animation frame (`tickFrame(nowMs)`).
    pub fn tick_frame(&mut self, now_ms: f64) -> FrameUpdate {
        let now_sec = (now_ms * 0.001).max(0.0);
        self.engine.state.cycle = now_sec * self.engine.state.cps;
        let highlights = self
            .engine
            .state
            .active_highlights
            .iter()
            .map(|h| WasmHighlightEntry {
                from_utf16: h.utf16_span.start,
                to_utf16: h.utf16_span.end,
                color: h.style.color.to_css_hex(),
                mark_css: h.style.mark_css_raw.clone(),
            })
            .collect();
        let inline_viz = self
            .engine
            .state
            .inline_viz_anchors
            .iter()
            .map(|a| WasmInlineVizEntry {
                anchor_line: a.anchor_line,
                viz_id: a.id,
                commands: Vec::new(),
            })
            .collect();
        FrameUpdate {
            highlights,
            global_viz: Vec::new(),
            inline_viz,
            hydra_uniforms: Some(HydraUniformFrame {
                time_sec: now_sec as f32,
                cycle: self.engine.state.cycle as f32,
                cps: self.engine.state.cps as f32,
                active_hap_count: self.engine.state.active_highlights.len() as u32,
                fft_bins: [0.0; 4],
                rms_level: 0.0,
                peak_level: 0.0,
                _padding: [0.0; 2],
            }),
        }
    }

    /// Encodes `code` into a `#code=<base64url>` share URL fragment (`UI-06`).
    pub fn encode_share_url(&self, code: &str) -> String {
        SilverWaveShareCodec::encode_url_fragment(code)
    }

    /// Decodes a `#code=<base64url>` fragment back into UTF-8 source (`UI-06`).
    pub fn decode_share_url(&self, hash: &str) -> Result<String, ReplError> {
        SilverWaveShareCodec::decode_url_fragment(hash)
    }

    /// Exports deterministic WAV or FLAC bytes (`EXP-01`).
    pub fn export_audio(&mut self, format: &str, cycles: f64) -> Result<Vec<u8>, ReplError> {
        let fmt = match format.to_ascii_lowercase().as_str() {
            "flac" => AudioExportFormat::Flac24,
            _ => AudioExportFormat::WavPcm16,
        };
        let bytes = self
            .engine
            .dispatch_command(ReplCommand::ExportAudio {
                format: fmt,
                cycles,
            })?
            .unwrap_or_default();
        Ok(bytes)
    }
}

/// Returns the crate version of `silverwave-wasm`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
