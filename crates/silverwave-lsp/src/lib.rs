//! `silverwave-lsp` — Unified Editor Protocol & JSON-RPC 2.0 Daemon for Neovim (`vim-tidal`),
//! VSCode (`vscode-tidalcycles`), Emacs (`tidal.el`), and Pulsar (`pulsar-tidalcycles`)
//! (`05-SPEC` §2.3 & §5.5, `LSP-01`, `LSP-02`).

#![forbid(unsafe_code)]

use silverwave_gui::{
    ActiveHighlight, DocMetadata, InlineVizAnchor, LspPosition, LspRange, ReplCommand, ReplEngine,
    ReplError, WidgetDescriptor, WidgetId,
};
use silverwave_viz::{AudioExportFormat, DrawCmd, VizTarget};

/// Negotiated LSP position encoding (`UTF-16` default for VSCode/Pulsar, `UTF-8` for Neovim/Emacs — `LSP-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionEncoding {
    Utf16,
    Utf8,
}

/// Incoming JSON-RPC 2.0 editor request (`05-SPEC` §2.3, `LSP-01`, `LSP-02`).
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRpcRequest {
    Eval {
        uri: String,
        text: String,
        range: Option<LspRange>,
        version: i32,
    },
    Halt {
        uri: Option<String>,
    },
    UpdateWidget {
        uri: String,
        widget_id: WidgetId,
        value: f64,
    },
    ExportAudio {
        uri: String,
        format: AudioExportFormat,
        cycles: f64,
        output_path: String,
    },
}

/// Outbound JSON-RPC 2.0 notification pushed to the editor client (`05-SPEC` §2.3, `LSP-01`, `LSP-02`).
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRpcNotification {
    ActiveHighlights {
        uri: String,
        version: i32,
        spans: Vec<ActiveHighlight>,
    },
    DocumentDecorations {
        uri: String,
        version: i32,
        widgets: Vec<WidgetDescriptor>,
        inline_viz_anchors: Vec<InlineVizAnchor>,
        metadata: DocMetadata,
    },
    EvalFlash {
        uri: String,
        range: LspRange,
        duration_ms: u32,
    },
    VizFrameReady {
        target: VizTarget,
        commands: Vec<DrawCmd>,
    },
}

/// Headless LSP / JSON-RPC 2.0 server state machine (`LSP-01`, `LSP-02`).
#[derive(Debug, Clone)]
pub struct SilverWaveLspServer {
    pub encoding: PositionEncoding,
    pub engine: ReplEngine,
    pub last_uri: String,
    pub last_version: i32,
}

impl Default for SilverWaveLspServer {
    fn default() -> Self {
        Self::new(PositionEncoding::Utf16)
    }
}

impl SilverWaveLspServer {
    /// Creates a new `SilverWaveLspServer` with the negotiated `PositionEncoding`.
    pub fn new(encoding: PositionEncoding) -> Self {
        Self {
            encoding,
            engine: ReplEngine::new(),
            last_uri: "file:///sketch.sw".to_string(),
            last_version: 1,
        }
    }

    /// Processes an `EditorRpcRequest` and returns the ordered list of `EditorRpcNotification`s (`LSP-01`, `LSP-02`).
    pub fn handle_request(
        &mut self,
        request: EditorRpcRequest,
    ) -> Result<Vec<EditorRpcNotification>, ReplError> {
        match request {
            EditorRpcRequest::Eval {
                uri,
                text,
                range,
                version,
            } => {
                self.last_uri = uri.clone();
                self.last_version = version;
                let outcome = self.engine.evaluate_buffer(&text)?;
                let flash_range = range.unwrap_or(LspRange {
                    start: LspPosition {
                        line: outcome.flash.range.start.line,
                        character: outcome.flash.range.start.character,
                    },
                    end: LspPosition {
                        line: outcome.flash.range.end.line,
                        character: outcome.flash.range.end.character,
                    },
                });
                Ok(vec![
                    EditorRpcNotification::EvalFlash {
                        uri: uri.clone(),
                        range: flash_range,
                        duration_ms: outcome.flash.duration_ms,
                    },
                    EditorRpcNotification::DocumentDecorations {
                        uri: uri.clone(),
                        version,
                        widgets: outcome.widgets,
                        inline_viz_anchors: outcome.inline_viz_anchors,
                        metadata: outcome.metadata,
                    },
                    EditorRpcNotification::ActiveHighlights {
                        uri,
                        version,
                        spans: self.engine.state.active_highlights.clone(),
                    },
                ])
            }
            EditorRpcRequest::Halt { uri } => {
                let target_uri = uri.unwrap_or_else(|| self.last_uri.clone());
                self.engine.halt();
                Ok(vec![EditorRpcNotification::ActiveHighlights {
                    uri: target_uri,
                    version: self.last_version,
                    spans: Vec::new(),
                }])
            }
            EditorRpcRequest::UpdateWidget {
                uri,
                widget_id,
                value,
            } => {
                let _ = self.engine.drag_slider(widget_id, value);
                Ok(vec![EditorRpcNotification::DocumentDecorations {
                    uri,
                    version: self.last_version,
                    widgets: self.engine.state.widgets.clone(),
                    inline_viz_anchors: self.engine.state.inline_viz_anchors.clone(),
                    metadata: self.engine.state.metadata.clone(),
                }])
            }
            EditorRpcRequest::ExportAudio {
                uri: _,
                format,
                cycles,
                output_path: _,
            } => {
                let _ = self
                    .engine
                    .dispatch_command(ReplCommand::ExportAudio { format, cycles })?;
                Ok(Vec::new())
            }
        }
    }
}

/// Returns the crate version of `silverwave-lsp`.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
