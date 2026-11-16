//! REPL Lifecycle State Machine (`ReplEngine`, `ReplCommand`, `ReplViewState`, `InlineVizAnchor`,
//! `NativeFrontend` — `05-SPEC` §1.1, §2.1–2.2, §4.1, `UI-01`..`UI-04`).

#![forbid(unsafe_code)]

use crate::metadata::{DocMetadata, MetadataParser, ReplError, SilverWaveMetadataParser};
use crate::span_map::{ByteSpan, LspRange, SpanMap, SpanMapper, Utf16Span};
use crate::widgets::{
    ActiveHighlight, HighlightStyle, SliderPatch, WidgetDescriptor, WidgetId, WidgetRegistry,
};
use silverwave_viz::{
    AudioExportFormat, CycleTime, DeterministicAudioExporter, ExportConfig, RgbaColor, VizFrame,
    VizKind,
};

/// Clock source contract for `ReplEngine` (`05-SPEC` §2.1).
pub trait ClockSource: Send + Sync {
    fn now_cycles(&self) -> CycleTime;
    fn cps(&self) -> f64;
    fn is_playing(&self) -> bool;
}

/// Inline `_`-prefixed visualizer anchor extracted on evaluation (`05-SPEC` §2.3 & §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineVizAnchor {
    pub id: u32,
    pub kind: VizKind,
    pub anchor_line: u32,
    pub call_span: ByteSpan,
}

/// Transport & Editor commands dispatched to `ReplEngine` (`05-SPEC` §4.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ReplCommand {
    EvaluateBuffer {
        code: String,
    },
    EvaluateBlock {
        code: String,
        block_span: ByteSpan,
    },
    Halt,
    SetCps(f64),
    DragSlider {
        widget_id: WidgetId,
        new_value: f64,
    },
    ExportAudio {
        format: AudioExportFormat,
        cycles: f64,
    },
}

/// Flash overlay emitted on successful evaluation (`05-SPEC` §2.3 & §4.1, `UI-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalFlashEvent {
    pub span: ByteSpan,
    pub range: LspRange,
    pub duration_ms: u32,
}

/// Result of `ReplEngine::evaluate_buffer` or `evaluate_block` (`UI-02`, `UI-03`).
#[derive(Debug, Clone, PartialEq)]
pub struct EvalOutcome {
    pub eval_generation: u64,
    pub flash: EvalFlashEvent,
    pub metadata: DocMetadata,
    pub widgets: Vec<WidgetDescriptor>,
    pub inline_viz_anchors: Vec<InlineVizAnchor>,
}

/// Snapshot of the REPL state for native (`egui` / TUI) rendering (`05-SPEC` §2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplViewState {
    pub source: String,
    pub playing: bool,
    pub muted: bool,
    pub cps: f64,
    pub cycle: CycleTime,
    pub eval_generation: u64,
    pub metadata: DocMetadata,
    pub active_highlights: Vec<ActiveHighlight>,
    pub widgets: Vec<WidgetDescriptor>,
    pub inline_viz_anchors: Vec<InlineVizAnchor>,
    pub last_flash: Option<EvalFlashEvent>,
    pub all_notes_off_count: u64,
}

/// Capabilities reported by a `NativeFrontend` (`05-SPEC` §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontendCapabilities {
    pub supports_inline_widgets: bool,
    pub supports_inline_viz: bool,
    pub supports_gpu_shaders: bool,
    pub supports_css_mark: bool,
    pub pixel_ratio_milli: u32,
}

impl Default for FrontendCapabilities {
    fn default() -> Self {
        Self {
            supports_inline_widgets: true,
            supports_inline_viz: true,
            supports_gpu_shaders: true,
            supports_css_mark: true,
            pixel_ratio_milli: 2000,
        }
    }
}

/// Native Desktop (`egui`/`wgpu`) and Terminal TUI frontend trait (`05-SPEC` §2.2).
pub trait NativeFrontend {
    fn poll_events(&mut self) -> Vec<ReplCommand>;
    fn render_frame(&mut self, state: &ReplViewState, draws: &[VizFrame]) -> Result<(), ReplError>;
    fn capabilities(&self) -> FrontendCapabilities;
}

/// Core REPL state machine driving evaluation, hot-swap preservation on error, inline sliders,
/// active highlights, and `hush()` / All-Notes-Off (`05-SPEC` §4.1, `UI-01`..`UI-04`, `SLIDER-03`).
#[derive(Debug, Clone)]
pub struct ReplEngine {
    pub state: ReplViewState,
    pub span_map: SpanMap,
    pub widget_registry: WidgetRegistry,
}

impl Default for ReplEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplEngine {
    /// Creates a new `ReplEngine` in `Stopped` state with Strudel v1.0+ `cps == 0.5` (`UI-01`).
    pub fn new() -> Self {
        Self {
            state: ReplViewState {
                source: String::new(),
                playing: false,
                muted: false,
                cps: 0.5,
                cycle: 0.0,
                eval_generation: 0,
                metadata: DocMetadata::default(),
                active_highlights: Vec::new(),
                widgets: Vec::new(),
                inline_viz_anchors: Vec::new(),
                last_flash: None,
                all_notes_off_count: 0,
            },
            span_map: SpanMap::new(""),
            widget_registry: WidgetRegistry::new(),
        }
    }

    /// Evaluates the full buffer (`Ctrl+Enter`). If a syntax error occurs (e.g. unclosed delimiter)
    /// while `playing == true`, returns `Err(ReplError::ParseError)` while preserving the previous
    /// playing pattern and `is_playing() == true` (`UI-02`, `UI-03`).
    pub fn evaluate_buffer(&mut self, code: &str) -> Result<EvalOutcome, ReplError> {
        let span = ByteSpan::new(0, code.len());
        self.evaluate_span(code, span)
    }

    /// Evaluates a specific block span (`Shift+Enter`).
    pub fn evaluate_block(
        &mut self,
        code: &str,
        block_span: ByteSpan,
    ) -> Result<EvalOutcome, ReplError> {
        self.evaluate_span(code, block_span)
    }

    fn evaluate_span(&mut self, code: &str, span: ByteSpan) -> Result<EvalOutcome, ReplError> {
        // Validate balanced delimiters and quotes (`UI-03` fault tolerance)
        check_balanced_syntax(code)?;

        let new_map = SpanMap::new(code);
        let range = new_map
            .byte_span_to_lsp_range(span)
            .unwrap_or_else(|| LspRange {
                start: crate::span_map::LspPosition {
                    line: 0,
                    character: 0,
                },
                end: crate::span_map::LspPosition {
                    line: 0,
                    character: 0,
                },
            });
        let metadata = SilverWaveMetadataParser::parse_metadata(code);
        let widgets = self.widget_registry.extract_from_source(code);
        let inline_viz_anchors = extract_inline_viz_anchors(code);
        let highlights = extract_quoted_token_highlights(code, &new_map);

        self.span_map = new_map;
        self.state.source = code.to_string();
        self.state.playing = true;
        self.state.eval_generation += 1;
        self.state.metadata = metadata.clone();
        self.state.widgets = widgets.clone();
        self.state.inline_viz_anchors = inline_viz_anchors.clone();
        self.state.active_highlights = highlights;

        let flash = EvalFlashEvent {
            span,
            range,
            duration_ms: 200,
        };
        self.state.last_flash = Some(flash.clone());

        Ok(EvalOutcome {
            eval_generation: self.state.eval_generation,
            flash,
            metadata,
            widgets,
            inline_viz_anchors,
        })
    }

    /// Halts playback (`Ctrl+.`), clears `active_highlights`, and increments `all_notes_off_count` (`UI-04`).
    pub fn halt(&mut self) {
        self.state.playing = false;
        self.state.active_highlights.clear();
        self.state.all_notes_off_count += 1;
    }

    /// Updates CPS (`setCps`).
    pub fn set_cps(&mut self, cps: f64) {
        if cps > 0.0 && cps.is_finite() {
            self.state.cps = cps;
        }
    }

    /// Toggles Mute/Unmute (`Ctrl+Alt+Enter` — `05-SPEC` §4.1) while preserving scheduler phase.
    pub fn toggle_mute(&mut self) -> bool {
        self.state.muted = !self.state.muted;
        self.state.muted
    }

    /// Updates an inline slider (`DragSlider`) without incrementing `eval_generation` (`SLIDER-03`).
    pub fn drag_slider(&mut self, widget_id: WidgetId, new_value: f64) -> Option<SliderPatch> {
        let patch = self.widget_registry.commit_slider_drag(
            widget_id,
            new_value,
            &mut self.state.source,
            &mut self.span_map,
        )?;
        self.state.widgets = self.widget_registry.all_widgets();
        Some(patch)
    }

    /// Executes a `ReplCommand`.
    pub fn dispatch_command(&mut self, cmd: ReplCommand) -> Result<Option< Vec<u8> >, ReplError> {
        match cmd {
            ReplCommand::EvaluateBuffer { code } => {
                self.evaluate_buffer(&code)?;
                Ok(None)
            }
            ReplCommand::EvaluateBlock { code, block_span } => {
                self.evaluate_block(&code, block_span)?;
                Ok(None)
            }
            ReplCommand::Halt => {
                self.halt();
                Ok(None)
            }
            ReplCommand::SetCps(cps) => {
                self.set_cps(cps);
                Ok(None)
            }
            ReplCommand::DragSlider {
                widget_id,
                new_value,
            } => {
                let _ = self.drag_slider(widget_id, new_value);
                Ok(None)
            }
            ReplCommand::ExportAudio { format, cycles } => {
                let exporter = DeterministicAudioExporter;
                let cfg = ExportConfig {
                    format,
                    sample_rate: 48_000,
                    channels: 2,
                    cycles,
                    cps: self.state.cps,
                    normalize_peak_db: None,
                };
                let bytes = exporter
                    .export_bytes(&cfg)
                    .map_err(|m| ReplError::ExportError { message: m })?;
                Ok(Some(bytes))
            }
        }
    }
}

impl ClockSource for ReplEngine {
    fn now_cycles(&self) -> CycleTime {
        self.state.cycle
    }

    fn cps(&self) -> f64 {
        self.state.cps
    }

    fn is_playing(&self) -> bool {
        self.state.playing
    }
}

fn check_balanced_syntax(code: &str) -> Result<(), ReplError> {
    if code.trim().is_empty() {
        return Err(ReplError::ParseError {
            message: "cannot evaluate empty buffer".to_string(),
            span: Some(ByteSpan::new(0, 0)),
        });
    }
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut in_quotes = false;
    let mut prev_escape = false;
    for (idx, ch) in code.char_indices() {
        if in_quotes {
            if ch == '\\' && !prev_escape {
                prev_escape = true;
                continue;
            }
            if ch == '"' && !prev_escape {
                in_quotes = false;
            }
            prev_escape = false;
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            '(' | '[' | '{' => stack.push((ch, idx)),
            ')' | ']' | '}' => {
                let Some((open, open_idx)) = stack.pop() else {
                    return Err(ReplError::ParseError {
                        message: format!("unmatched closing delimiter `{}`", ch),
                        span: Some(ByteSpan::new(idx, idx + 1)),
                    });
                };
                let expected = match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    _ => ')',
                };
                if ch != expected {
                    return Err(ReplError::ParseError {
                        message: format!("mismatched delimiter `{}` for `{}`", ch, open),
                        span: Some(ByteSpan::new(open_idx, idx + 1)),
                    });
                }
            }
            _ => {}
        }
    }
    if in_quotes {
        return Err(ReplError::ParseError {
            message: "unclosed string literal".to_string(),
            span: Some(ByteSpan::new(0, code.len())),
        });
    }
    if let Some((open, open_idx)) = stack.pop() {
        return Err(ReplError::ParseError {
            message: format!("unclosed delimiter `{}`", open),
            span: Some(ByteSpan::new(open_idx, open_idx + 1)),
        });
    }
    Ok(())
}

fn extract_inline_viz_anchors(code: &str) -> Vec<InlineVizAnchor> {
    let patterns = [
        ("._pianoroll(", VizKind::Pianoroll),
        ("._punchcard(", VizKind::Punchcard),
        ("._spiral(", VizKind::Spiral),
        ("._scope(", VizKind::Scope),
        ("._tscope(", VizKind::Scope),
        ("._pitchwheel(", VizKind::Pitchwheel),
        ("._spectrum(", VizKind::Spectrum),
    ];
    let mut anchors = Vec::new();
    let mut next_id = 1u32;
    for (pat, kind) in patterns {
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(pat) {
            let start = from + rel + 1; // skip leading dot
            let end = (start + pat.len() - 1).min(code.len());
            let line = code[..start].bytes().filter(|&b| b == b'\n').count() as u32;
            anchors.push(InlineVizAnchor {
                id: next_id,
                kind,
                anchor_line: line,
                call_span: ByteSpan::new(start, end),
            });
            next_id += 1;
            from = end;
        }
    }
    anchors
}

fn extract_quoted_token_highlights(code: &str, span_map: &SpanMap) -> Vec<ActiveHighlight> {
    let mut out = Vec::new();
    if let Some(first_q) = code.find('"') {
        if let Some(rel_end) = code[first_q + 1..].find('"') {
            let start = first_q + 1;
            let end = start + rel_end;
            let span = ByteSpan::new(start, end);
            let utf16_span = span_map
                .byte_span_to_utf16_span(span)
                .unwrap_or(Utf16Span::new(start, end));
            out.push(ActiveHighlight {
                span,
                utf16_span,
                style: HighlightStyle::from_color_and_css(
                    RgbaColor::new(255, 202, 40, 255),
                    None,
                ),
            });
        }
    }
    out
}
