//! Model Context Protocol (MCP) Transport Contracts (`stdio` & Streamable `http+sse`),
//! Shared Co-Performance Session Hub, Surgical AST Slider Patching (`SurgicalTextEdit`),
//! and Optimistic Concurrency Control (`07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` §1.1–1.4).

#![forbid(unsafe_code)]

use silverwave_core::Rational64;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::ops::Range;
use std::path::PathBuf;

/// Source span with UTF-8 byte offsets and 1-indexed line/column coordinates (`07-SPEC` §2.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// Cycle quantization alignment grid (`07-SPEC` §2.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantizeGrid {
    Immediate,
    NextCycle,
    NextBar,
    CycleMultiple(f64),
}

/// Standardized JSON-RPC 2.0 & MCP protocol error (`07-SPEC` §2.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProtocolError {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

/// JSON-RPC 2.0 parse error (`-32700`).
pub const ERR_PARSE_ERROR: i32 = -32700;
/// JSON-RPC 2.0 invalid request (`-32600`).
pub const ERR_INVALID_REQUEST: i32 = -32600;
/// JSON-RPC 2.0 method/tool not found (`-32601`).
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC 2.0 invalid tool arguments (`-32602`).
pub const ERR_INVALID_PARAMS: i32 = -32602;
/// Optimistic concurrency revision mismatch (`-32001`).
pub const ERR_REVISION_CONFLICT: i32 = -32001;
/// Mini-notation or DSL syntax failure (`-32002`).
pub const ERR_DSL_SYNTAX: i32 = -32002;
/// Unknown stream identifier (`-32003`).
pub const ERR_STREAM_NOT_FOUND: i32 = -32003;
/// Unknown widget identifier or stale source span (`-32004`).
pub const ERR_WIDGET_NOT_FOUND: i32 = -32004;
/// Sample load or network failure (`-32005`).
pub const ERR_SAMPLE_LOAD: i32 = -32005;
/// Path traversal, symlink escape, or SSRF violation (`-32006`).
pub const ERR_SANDBOX_VIOLATION: i32 = -32006;
/// External MIDI/OSC/Csound I/O error (`-32007`).
pub const ERR_EXTERNAL_IO: i32 = -32007;
/// Mutating tool invoked while `--read-only` is active (`-32008`).
pub const ERR_READ_ONLY_VIOLATION: i32 = -32008;
/// Token-bucket rate limit exceeded (`-32029`).
pub const ERR_RATE_LIMIT_EXCEEDED: i32 = -32029;

impl McpProtocolError {
    /// Creates a new `McpProtocolError`.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attaches structured diagnostic data to `self`.
    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }
}

/// Transport mode for `silverwave mcp` (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransportMode {
    Stdio,
    HttpSse {
        endpoint_path: String,
        bearer_token: Option<String>,
        allowed_origins: Vec<String>,
    },
}

/// Target human editor session to attach to (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAttachmentTarget {
    DefaultLocalSession,
    SessionId(String),
    IpcSocketPath(PathBuf),
}

/// CLI arguments for `silverwave mcp` (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCliArgs {
    pub transport: McpTransportMode,
    pub bind: SocketAddr,
    pub attach_session: Option<SessionAttachmentTarget>,
    pub sandbox_roots: Vec<PathBuf>,
    pub allow_external_network: bool,
    pub rate_limit_rps: u32,
    pub rate_limit_burst: u32,
    pub read_only: bool,
    pub headless_deterministic: bool,
    pub highlight_notify_hz: u16,
}

impl Default for McpCliArgs {
    fn default() -> Self {
        Self {
            transport: McpTransportMode::Stdio,
            bind: SocketAddr::from(([127, 0, 0, 1], 8765)),
            attach_session: None,
            sandbox_roots: vec![PathBuf::from("/tmp/silverwave_sandbox")],
            allow_external_network: false,
            rate_limit_rps: 120,
            rate_limit_burst: 240,
            read_only: false,
            headless_deterministic: true,
            highlight_notify_hz: 30,
        }
    }
}

/// Human editor surface kind (`07-SPEC` §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSurfaceKind {
    NativeGui,
    BrowserWasmRepl,
    LspClient,
    CliRepl,
}

/// Provenance actor tag for co-performance buffer/control mutations (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorOrigin {
    HumanEditor {
        editor_id: String,
        surface: EditorSurfaceKind,
    },
    McpAgent {
        session_id: String,
        client_name: String,
        request_id: String,
    },
    MidiController {
        port_id: String,
        channel: u8,
    },
    AbletonLinkPeer {
        peer_count: usize,
    },
}

/// Atomic surgical AST edit broadcast between MCP tools and human editors (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurgicalTextEdit {
    pub base_revision: u64,
    pub new_revision: u64,
    pub byte_range: Range<usize>,
    pub line_col_start: (u32, u32),
    pub line_col_end: (u32, u32),
    pub replacement_text: String,
    pub origin: ActorOrigin,
}

/// Handle returned when subscribing to live `silverwave://` resource updates (`07-SPEC` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSubscriptionHandle {
    pub subscription_id: u64,
    pub uri_pattern: String,
}

/// Trait governing shared co-performance state between human editors and MCP agents (`07-SPEC` §1.4).
pub trait CoPerformanceBridge: Send + Sync {
    fn snapshot_buffer(&self) -> (u64, String);
    fn apply_buffer_mutation(
        &mut self,
        edits: Vec<SurgicalTextEdit>,
        expected_revision: Option<u64>,
        origin: ActorOrigin,
    ) -> Result<u64, McpProtocolError>;
    fn subscribe_resource_updates(
        &mut self,
        uri_pattern: &str,
    ) -> Result<ResourceSubscriptionHandle, McpProtocolError>;
}

/// Human cursor position tracked in `silverwave://session/buffer` (`07-SPEC` §3.1 & §6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanCursorSelection {
    pub editor_id: String,
    pub cursor_byte: usize,
    pub selection_end_byte: usize,
}

/// Shared co-performance session state hub (`CoPerformanceSessionHub` — `07-SPEC` §1.1 & §6.3).
#[derive(Debug, Clone)]
pub struct CoPerformanceSessionHub {
    pub revision: u64,
    pub buffer_code: String,
    pub human_cursors: Vec<HumanCursorSelection>,
    pub last_actor: ActorOrigin,
    pub subscriptions: Vec<ResourceSubscriptionHandle>,
    pub pending_notifications: Vec<String>,
    pub active_cycle: Rational64,
    pub stream_expressions: BTreeMap<String, String>,
}

impl Default for CoPerformanceSessionHub {
    fn default() -> Self {
        Self::new("d1 $ s(\"bd sd\") .lpf(slider(0.5, 0.0, 1.0, 0.01))")
    }
}

impl CoPerformanceSessionHub {
    /// Creates a new `CoPerformanceSessionHub` seeded with `initial_code`.
    pub fn new(initial_code: &str) -> Self {
        Self {
            revision: 1,
            buffer_code: initial_code.to_string(),
            human_cursors: vec![HumanCursorSelection {
                editor_id: "human:main".to_string(),
                cursor_byte: 4,
                selection_end_byte: 4,
            }],
            last_actor: ActorOrigin::HumanEditor {
                editor_id: "human:main".to_string(),
                surface: EditorSurfaceKind::NativeGui,
            },
            subscriptions: Vec::new(),
            pending_notifications: Vec::new(),
            active_cycle: Rational64::from_integer(0),
            stream_expressions: BTreeMap::new(),
        }
    }

    /// Emits a `notifications/resources/updated` JSON-RPC message if any subscription matches `uri`.
    pub fn notify_resource_updated(&mut self, uri: &str) {
        let matched = self.subscriptions.iter().any(|s| {
            s.uri_pattern == uri
                || s.uri_pattern == "silverwave://*"
                || (s.uri_pattern.ends_with('*')
                    && uri.starts_with(s.uri_pattern.trim_end_matches('*')))
        });
        if matched {
            let msg = format!(
                "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/resources/updated\",\"params\":{{\"uri\":\"{}\"}}}}",
                uri
            );
            self.pending_notifications.push(msg);
        }
    }
}

impl CoPerformanceBridge for CoPerformanceSessionHub {
    fn snapshot_buffer(&self) -> (u64, String) {
        (self.revision, self.buffer_code.clone())
    }

    fn apply_buffer_mutation(
        &mut self,
        edits: Vec<SurgicalTextEdit>,
        expected_revision: Option<u64>,
        origin: ActorOrigin,
    ) -> Result<u64, McpProtocolError> {
        if let Some(exp) = expected_revision {
            if exp != self.revision {
                return Err(McpProtocolError::new(
                    ERR_REVISION_CONFLICT,
                    format!(
                        "Expected revision {}, but current buffer revision is {}",
                        exp, self.revision
                    ),
                ));
            }
        }
        for edit in edits {
            if edit.byte_range.end > self.buffer_code.len()
                || edit.byte_range.start > edit.byte_range.end
            {
                return Err(McpProtocolError::new(
                    ERR_WIDGET_NOT_FOUND,
                    "Surgical edit span out of bounds",
                ));
            }
            let old_len = edit.byte_range.end - edit.byte_range.start;
            let new_len = edit.replacement_text.len();
            let delta = new_len as isize - old_len as isize;
            self.buffer_code
                .replace_range(edit.byte_range.clone(), &edit.replacement_text);
            for cursor in &mut self.human_cursors {
                if cursor.cursor_byte >= edit.byte_range.end {
                    cursor.cursor_byte = (cursor.cursor_byte as isize + delta).max(0) as usize;
                    cursor.selection_end_byte =
                        (cursor.selection_end_byte as isize + delta).max(0) as usize;
                }
            }
        }
        self.revision += 1;
        self.last_actor = origin;
        self.notify_resource_updated("silverwave://session/buffer");
        Ok(self.revision)
    }

    fn subscribe_resource_updates(
        &mut self,
        uri_pattern: &str,
    ) -> Result<ResourceSubscriptionHandle, McpProtocolError> {
        let id = (self.subscriptions.len() as u64) + 1;
        let handle = ResourceSubscriptionHandle {
            subscription_id: id,
            uri_pattern: uri_pattern.to_string(),
        };
        self.subscriptions.push(handle.clone());
        Ok(handle)
    }
}
