# `07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` — SilverWave Model Context Protocol (MCP) Server & Agent Surface Specification

| Field | Value |
| :--- | :--- |
| **Document ID** | `07-SPEC-MCP-SERVER-AND-AGENT-SURFACE` |
| **Version** | `1.0.0` |
| **Status** | Normative Specification (DocDD / TDD Baseline) |
| **Target Crates** | `silverwave-mcp` (library), `silverwave-cli` (`silverwave mcp` subcommand) |
| **Protocol Standard** | [Model Context Protocol (MCP) Specification (2026-07-28)](https://modelcontextprotocol.io/specification/2026-07-28) over JSON-RPC 2.0 |
| **Upstream Parity Baselines** | [Strudel](https://codeberg.org/uzu/strudel) ([Strudel REPL](https://strudel.cc/)) & [TidalCycles](https://tidalcycles.org/docs/) ([Tidal Transitions](https://tidalcycles.org/docs/reference/transitions/)) |
| **Clock Sync Standard** | [Ableton Link](https://ableton.github.io/link/) ([Ableton Link Repository](https://github.com/ableton/link)) |

## 1. Architectural Overview, Transports & Co-Performance State Synchronization

### 1.1 System Purpose & 1:1 Human-Agent Surface Parity

`silverwave-mcp` exposes the complete live-coding, pattern-querying, stream-transition, clock-synchronization, interactive widget, DSP/orbit, external MIDI/OSC/Csound, visualization, telemetry, and audio-export capabilities of SilverWave as a compliant [Model Context Protocol (MCP)](https://modelcontextprotocol.io/specification/2026-07-28) server.

Every action a human performer can execute inside the native desktop editor (`silverwave-gui`), the browser/WASM REPL ([Strudel REPL](https://strudel.cc/) parity surface), the Language Server Protocol daemon (`silverwave-lsp`), or the CLI (`silverwave-cli`) has a 1:1 deterministic counterpart in `silverwave-mcp`. Conversely, every mutation performed by an MCP client is reflected in real time within attached human editors, visualizers, and hardware controllers without interrupting audio continuity or resetting cycle phase.

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                  HUMAN & AGENT CLIENT SURFACES                                  │
│                                                                                                 │
│  ┌──────────────────────────────┐  ┌──────────────────────────────┐  ┌───────────────────────┐  │
│  │   MCP Client (stdio)         │  │   MCP Client (Streamable     │  │ Human Editors & REPL  │  │
│  │   JSON-RPC 2.0 over          │  │   HTTP POST + GET SSE)       │  │ - Native GUI (wgpu)   │  │
│  │   stdin / stdout             │  │   http://127.0.0.1:8765/mcp  │  │ - Browser REPL        │  │
│  └──────────────┬───────────────┘  └──────────────┬───────────────┘  │ - LSP (silverwave-lsp)│  │
│                 │                                 │                  └───────────┬───────────┘  │
└─────────────────┼─────────────────────────────────┼──────────────────────────────┼──────────────┘
                  │                                 │                              │
                  ▼                                 ▼                              ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│                              silverwave-mcp & CoPerformanceSessionHub                           │
│                                                                                                 │
│  ┌───────────────────────────────────────────────────────────────────────────────────────────┐  │
│  │ JSON-RPC 2.0 Router, Schema Validator, Token-Bucket Rate Limiter & Path Sandbox Guard     │  │
│  └──────────────┬─────────────────────────────────┬──────────────────────────────┬───────────┘  │
│                 │ Tools (42)                      │ Resources (10)               │ Prompts (5)  │
│                 ▼                                 ▼                              ▼              │
│  ┌───────────────────────────────────────────────────────────────────────────────────────────┐  │
│  │ Shared Co-Performance Session State (Epoch-Versioned Buffer, Streams, Widgets, Controls)  │  │
│  │ - Atomic Document Revisioning & Surgical AST Source Patching (slider / control literals)  │  │
│  │ - Broadcast Fan-Out (notifications/resources/updated <-> Editor / LSP / WebSockets)       │  │
│  └──────────────┬─────────────────────────────────┬──────────────────────────────┬───────────┘  │
└─────────────────┼─────────────────────────────────┼──────────────────────────────┼──────────────┘
                  │ Lock-Free SPSC/MPSC             │ Async Worker Pool            │ Lock-Free
                  │ AudioCommand Ring (rtrb)        │ (Samples / SF2 / Csound /    │ Telemetry &
                  ▼                                 ▼  Offline WAV/FLAC Bounce)    │ Viz TripleBuf
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│                         REAL-TIME SCHEDULER, LINK CLOCK & DSP ENGINE                            │
│                                                                                                 │
│  ┌──────────────────────────────┐  ┌──────────────────────────────┐  ┌───────────────────────┐  │
│  │ Rational Cycle Scheduler     │  │ Ableton Link & MIDI/OSC      │  │ Real-Time Audio Graph │  │
│  │ - d1..d16 & $: Streams       │  │ I/O Engine                   │  │ - Orbits 0..15        │  │
│  │ - 11 Tidal Transitions       │  │ - Link Peers / Phase / BPM   │  │ - Synths / Sampler    │  │
│  │ - Active Span Highlights     │  │ - MIDI Ports / SuperDirt OSC │  │ - FFT / RMS / LUFS    │  │
│  └──────────────────────────────┘  └──────────────────────────────┘  └───────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Human-to-MCP 1:1 Surface Parity Matrix

| Human Action Surface | Human Interaction / Shortcut / API | MCP Tool(s) | MCP Resource(s) Updated |
| :--- | :--- | :--- | :--- |
| Full Buffer Evaluation | `Ctrl+Shift+Enter` / `Evaluate All` button | `evaluate_buffer` | `silverwave://session/buffer`, `silverwave://session/streams`, `silverwave://session/widgets` |
| Block / Line Evaluation | `Ctrl+Enter` / `Alt+Enter` on cursor block | `evaluate_block` | `silverwave://session/streams`, `silverwave://session/highlights` |
| Mini-Notation Inspection | Hover AST inspector / `_pianoroll` span preview | `parse_mini_notation`, `query_pattern_arc` | `silverwave://session/highlights` |
| Live Syntax Diagnostics | Editor squiggles / LSP `textDocument/publishDiagnostics` | `validate_syntax` | `silverwave://session/buffer` |
| Stream Slot Assignment | `d1 $ ...` .. `d16 $ ...` or `$: ...` | `set_stream` | `silverwave://session/streams` |
| Tidal Pattern Transitions | `xfade`, `clutch`, `anticipate`, `interpolate`, `jump`, `jumpIn`, `jumpMod`, `histpan`, `wait`, `wash`, `mortal` | `transition_stream` | `silverwave://session/streams` |
| Stream Mute / Solo | Editor gutter toggle / MIDI mute / `mute 1` / `solo 1` | `mute_stream`, `unmute_stream`, `solo_stream`, `unsolo_stream` | `silverwave://session/streams` |
| Graceful Stop / Hard Panic | `Ctrl+.` (`hush`) / `Ctrl+Shift+.` (`panic`) | `hush`, `panic` | `silverwave://session/streams`, `silverwave://dsp/orbits` |
| Tempo & Metric Control | `setcps(0.5)`, `setbpm(120)`, tap tempo, phase nudge | `get_clock_snapshot`, `set_cps`, `set_bpm`, `set_beats_per_cycle`, `nudge_clock` | `silverwave://session/clock` |
| Ableton Link Sync | Link toggle button / quantum selector | `configure_ableton_link` | `silverwave://session/clock` |
| Inline Sliders & Widgets | Dragging `slider(0.5, 0, 1)` handle in code editor | `list_sliders`, `update_slider`, `set_control_state`, `get_control_state` | `silverwave://session/widgets`, `silverwave://session/buffer` |
| Sample & SoundFont Loading | `samples('github:...')`, `samples('strudel.json')`, `.sf2` drag-and-drop | `list_sample_banks`, `load_sample_source` | `silverwave://dsp/samples` |
| Orbit Mixing & Bus FX | Orbit mixer strip (`0..15`), reverb/delay/ducking | `configure_orbit`, `get_orbit_telemetry` | `silverwave://dsp/orbits` |
| External MIDI / OSC / Csound | `.midi()`, MIDI learn, SuperDirt OSC, `.csound()` | `list_midi_ports`, `configure_midi_map`, `send_midi_command`, `configure_osc_target`, `load_csound_orc` | `silverwave://session/widgets`, `silverwave://dsp/orbits` |
| Inline & Docked Visualizers | `.pianoroll()`, `.punchcard()`, `.spiral()`, `.scope()`, `.pitchwheel()`, `.spectrum()` | `capture_visualizer_frame`, `read_audio_telemetry` | `silverwave://viz/{kind}`, `silverwave://dsp/orbits` |
| Audio Bounce & Share Links | `Export WAV/FLAC` dialog, `Share` URL button, `silverwave doctor` | `export_audio`, `encode_share_url`, `decode_share_url`, `run_doctor` | `silverwave://session/buffer` |

### 1.3 Transport Layer Specifications

`silverwave-mcp` implements both standard transports defined in the [Model Context Protocol Architecture Specification](https://modelcontextprotocol.io/docs/2026-07-28/learn/architecture):

1. **Standard Input/Output (`stdio`) Transport**:
   - **Framing**: UTF-8 encoded, newline-delimited (`\n`) JSON-RPC 2.0 messages read from `stdin` and written to `stdout`.
   - **`stdout` Purity Mandate**: The server process MUST NOT write any non-JSON-RPC bytes, banners, panics, or audio library diagnostic messages to `stdout`. All structured diagnostics and tracing logs MUST be routed exclusively to `stderr` or a configured log file.
   - **Lifecycle**: Starts in standalone embedded-engine mode or attaches to an existing local human editor session via shared memory / Unix domain socket (`--attach-session`). Terminates cleanly when `stdin` reaches `EOF` or upon receiving `SIGTERM`/`SIGINT`.

2. **Streamable HTTP & Server-Sent Events (`http+sse`) Transport**:
   - **Single Endpoint Topology**: Exposes a unified MCP endpoint (default `http://127.0.0.1:8765/mcp`).
   - **Client-to-Server (`POST /mcp`)**: Accepts JSON-RPC 2.0 requests, notifications, or batch arrays with `Content-Type: application/json` and `Accept: application/json, text/event-stream`. Returns `200 OK` with `application/json` for immediate synchronous tool responses or `text/event-stream` for streaming progress (`notifications/progress`) and long-running operations (`export_audio`, `load_sample_source`).
   - **Server-to-Client (`GET /mcp`)**: Opens a persistent SSE stream (`Content-Type: text/event-stream`) identified by the `Mcp-Session-Id` header. Pushes real-time resource notifications (`notifications/resources/updated`), tool list updates (`notifications/tools/list_changed`), and log events (`notifications/message`). Supports stream resumption via the `Last-Event-ID` header.
   - **Session Termination (`DELETE /mcp`)**: Closes the session associated with `Mcp-Session-Id` and unsubscribes all registered resource watches.
   - **Origin & Loopback Binding**: By default, binds strictly to `127.0.0.1` / `[::1]` and validates `Origin` headers to prevent DNS-rebinding and cross-origin browser attacks. When non-loopback binding is explicitly enabled, Bearer token authentication (`Authorization: Bearer <token>`) is mandatory.

### 1.4 CLI Subcommand & Core Runtime Trait Contracts (`silverwave mcp`)

```rust
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;

/// CLI arguments for `silverwave mcp`.
#[derive(Debug, Clone, PartialEq)]
pub struct McpCliArgs {
    /// Transport selection: `stdio` (default) or `http` (Streamable HTTP + SSE).
    pub transport: McpTransportMode,
    /// Bind address when `transport == McpTransportMode::HttpSse` (default: `127.0.0.1:8765`).
    pub bind: SocketAddr,
    /// Optional session ID or IPC socket path to attach to a running human GUI/REPL instance.
    /// If `None`, spawns a new co-performance session hub and audio engine (unless `--headless-no-audio`).
    pub attach_session: Option<SessionAttachmentTarget>,
    /// Allowed filesystem root directories for sample loading and audio export.
    pub sandbox_roots: Vec<PathBuf>,
    /// Permits fetching remote sample manifests (`strudel.json`, `github:`) over HTTPS.
    pub allow_external_network: bool,
    /// Per-client sustained request rate limit (requests per second, default: 120).
    pub rate_limit_rps: u32,
    /// Maximum burst size for token-bucket rate limiter (default: 240).
    pub rate_limit_burst: u32,
    /// Restricts MCP clients to read-only inspection tools and resources (disables state mutation).
    pub read_only: bool,
    /// Runs pattern scheduler and DSP graph in deterministic virtual-clock headless mode.
    pub headless_deterministic: bool,
    /// Maximum frequency (Hz) for `silverwave://session/highlights` resource update notifications (default: 30).
    pub highlight_notify_hz: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransportMode {
    Stdio,
    HttpSse {
        endpoint_path: String,
        bearer_token: Option<String>,
        allowed_origins: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAttachmentTarget {
    DefaultLocalSession,
    SessionId(String),
    IpcSocketPath(PathBuf),
}

/// Origin actor tag attached to every state mutation for co-performance provenance.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSurfaceKind {
    NativeGui,
    BrowserWasmRepl,
    LspClient,
    CliRepl,
}

/// Atomic surgical edit broadcast between MCP tools (`update_slider`, `evaluate_buffer`)
/// and connected human code editors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurgicalTextEdit {
    pub base_revision: u64,
    pub new_revision: u64,
    pub byte_range: std::ops::Range<usize>,
    pub line_col_start: (u32, u32),
    pub line_col_end: (u32, u32),
    pub replacement_text: String,
    pub origin: ActorOrigin,
}

/// Trait governing shared co-performance state between human editors and MCP agents.
pub trait CoPerformanceBridge: Send + Sync + 'static {
    /// Returns the current monotonic buffer revision and UTF-8 source text.
    fn snapshot_buffer(&self) -> (u64, String);

    /// Applies either a full buffer replacement or a surgical AST literal patch
    /// (`update_slider`) with optional optimistic concurrency check (`expected_revision`).
    fn apply_buffer_mutation(
        &self,
        edits: Vec<SurgicalTextEdit>,
        expected_revision: Option<u64>,
        origin: ActorOrigin,
    ) -> Result<u64, McpProtocolError>;

    /// Subscribes to real-time resource invalidation events triggered by either
    /// human performers, the cycle scheduler, or other MCP clients.
    fn subscribe_resource_updates(
        &self,
        uri_pattern: &str,
    ) -> Result<ResourceSubscriptionHandle, McpProtocolError>;
}

/// Top-level asynchronous MCP server runtime contract.
pub trait McpServerRuntime: Send + Sync + 'static {
    fn handle_jsonrpc_message<'a>(
        &'a self,
        session_id: &'a str,
        raw_payload: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;

    fn shutdown<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
```

## 2. Complete MCP Tool Catalog, JSON Schemas & Rust Trait Contracts

Alltools follow the [MCP Tools Specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools). Every tool defines:
- A deterministic **Rust Contract** (input/output structs and trait method signature).
- A strict **JSON Schema (`inputSchema`)** with `additionalProperties: false`.
- A structured **Result Payload Contract (`structuredContent`)** paired with a human-readable summary block in `content`.
- Standardized **JSON-RPC / MCP Error Codes**:
  - `-32700`: Parse error (malformed JSON).
  - `-32600`: Invalid Request.
  - `-32601`: Method/Tool not found.
  - `-32602`: Invalid tool arguments (schema validation failure).
  - `-32001`: `RevisionConflict` (`expected_revision` mismatch during co-performance).
  - `-32002`: `DslSyntaxError` (mini-notation or pattern DSL syntax failure).
  - `-32003`: `StreamNotFound` (unknown stream identifier).
  - `-32004`: `WidgetNotFound` (unknown `widget_id` or stale source span).
  - `-32005`: `SampleLoadError` (missing bank, unsupported codec, or network disabled).
  - `-32006`: `SandboxViolation` (path traversal or unauthorized directory access).
  - `-32007`: `ExternalIoError` (MIDI port, OSC socket, or Csound compilation failure).
  - `-32008`: `ReadOnlyModeViolation` (mutating tool invoked while `--read-only` is active).
  - `-32029`: `RateLimitExceeded` (token bucket exhausted).

### 2.0 Master Tool Dispatch Trait

```rust
use std::collections::BTreeMap;

/// Rational number representation `(numerator, denominator)` for exact cycle time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rational64 {
    pub num: i64,
    pub den: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantizeGrid {
    Immediate,
    NextCycle,
    NextBar,
    CycleMultiple(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpProtocolError {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

/// Complete trait surface providing 1:1 parity with the human user surface.
pub trait SilverWaveMcpToolSurface: Send + Sync + 'static {
    // 2.1 Pattern & DSL Tools
    fn evaluate_buffer(&self, req: EvaluateBufferRequest) -> Result<EvaluateBufferResponse, McpProtocolError>;
    fn evaluate_block(&self, req: EvaluateBlockRequest) -> Result<EvaluateBlockResponse, McpProtocolError>;
    fn parse_mini_notation(&self, req: ParseMiniNotationRequest) -> Result<ParseMiniNotationResponse, McpProtocolError>;
    fn query_pattern_arc(&self, req: QueryPatternArcRequest) -> Result<QueryPatternArcResponse, McpProtocolError>;
    fn validate_syntax(&self, req: ValidateSyntaxRequest) -> Result<ValidateSyntaxResponse, McpProtocolError>;

    // 2.2 Streams & Tidal Transitions Tools
    fn set_stream(&self, req: SetStreamRequest) -> Result<SetStreamResponse, McpProtocolError>;
    fn transition_stream(&self, req: TransitionStreamRequest) -> Result<TransitionStreamResponse, McpProtocolError>;
    fn mute_stream(&self, req: StreamToggleRequest) -> Result<StreamToggleResponse, McpProtocolError>;
    fn unmute_stream(&self, req: StreamToggleRequest) -> Result<StreamToggleResponse, McpProtocolError>;
    fn solo_stream(&self, req: StreamToggleRequest) -> Result<StreamToggleResponse, McpProtocolError>;
    fn unsolo_stream(&self, req: StreamToggleRequest) -> Result<StreamToggleResponse, McpProtocolError>;
    fn hush(&self, req: HushRequest) -> Result<HushResponse, McpProtocolError>;
    fn panic(&self, req: PanicRequest) -> Result<PanicResponse, McpProtocolError>;

    // 2.3 Clock & Ableton Link Tools
    fn get_clock_snapshot(&self, req: GetClockSnapshotRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_cps(&self, req: SetCpsRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_bpm(&self, req: SetBpmRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_beats_per_cycle(&self, req: SetBeatsPerCycleRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn nudge_clock(&self, req: NudgeClockRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn configure_ableton_link(&self, req: ConfigureAbletonLinkRequest) -> Result<AbletonLinkStatusResponse, McpProtocolError>;

    // 2.4 Interactive Widgets & Controls Tools
    fn list_sliders(&self, req: ListSlidersRequest) -> Result<ListSlidersResponse, McpProtocolError>;
    fn update_slider(&self, req: UpdateSliderRequest) -> Result<UpdateSliderResponse, McpProtocolError>;
    fn set_control_state(&self, req: SetControlStateRequest) -> Result<SetControlStateResponse, McpProtocolError>;
    fn get_control_state(&self, req: GetControlStateRequest) -> Result<GetControlStateResponse, McpProtocolError>;

    // 2.5 DSP, Orbits & Samples Tools
    fn list_sample_banks(&self, req: ListSampleBanksRequest) -> Result<ListSampleBanksResponse, McpProtocolError>;
    fn load_sample_source(&self, req: LoadSampleSourceRequest) -> Result<LoadSampleSourceResponse, McpProtocolError>;
    fn configure_orbit(&self, req: ConfigureOrbitRequest) -> Result<ConfigureOrbitResponse, McpProtocolError>;
    fn get_orbit_telemetry(&self, req: GetOrbitTelemetryRequest) -> Result<GetOrbitTelemetryResponse, McpProtocolError>;

    // 2.6 External I/O Tools (MIDI, OSC & Csound)
    fn list_midi_ports(&self, req: ListMidiPortsRequest) -> Result<ListMidiPortsResponse, McpProtocolError>;
    fn configure_midi_map(&self, req: ConfigureMidiMapRequest) -> Result<ConfigureMidiMapResponse, McpProtocolError>;
    fn send_midi_command(&self, req: SendMidiCommandRequest) -> Result<SendMidiCommandResponse, McpProtocolError>;
    fn configure_osc_target(&self, req: ConfigureOscTargetRequest) -> Result<ConfigureOscTargetResponse, McpProtocolError>;
    fn load_csound_orc(&self, req: LoadCsoundOrcRequest) -> Result<LoadCsoundOrcResponse, McpProtocolError>;

    // 2.7 Visualizers, Telemetry & Export Tools
    fn capture_visualizer_frame(&self, req: CaptureVisualizerFrameRequest) -> Result<CaptureVisualizerFrameResponse, McpProtocolError>;
    fn read_audio_telemetry(&self, req: ReadAudioTelemetryRequest) -> Result<ReadAudioTelemetryResponse, McpProtocolError>;
    fn export_audio(&self, req: ExportAudioRequest) -> Result<ExportAudioResponse, McpProtocolError>;
    fn encode_share_url(&self, req: EncodeShareUrlRequest) -> Result<EncodeShareUrlResponse, McpProtocolError>;
    fn decode_share_url(&self, req: DecodeShareUrlRequest) -> Result<DecodeShareUrlResponse, McpProtocolError>;
    fn run_doctor(&self, req: RunDoctorRequest) -> Result<RunDoctorResponse, McpProtocolError>;
}
```

### 2.1 Domain 1: Pattern & DSL Tools

#### 2.1.1 `evaluate_buffer`
Evaluates a complete pattern buffer (or evaluates the currently shared editor buffer if `code` is omitted), atomically updating active streams (`d1..d16`, `$:`), inline widgets, and the shared human editor buffer.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBufferRequest {
    pub code: Option<String>,
    pub sync_to_editor: bool,
    pub expected_revision: Option<u64>,
    pub quantize: QuantizeGrid,
    pub auto_start_clock: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBufferResponse {
    pub revision: u64,
    pub scheduled_at_cycle: Rational64,
    pub active_streams: Vec<String>,
    pub discovered_widgets: Vec<SliderWidgetDescriptor>,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "properties": {
    "code": {
      "type": "string",
      "description": "Complete Strudel/Tidal DSL buffer to evaluate. If omitted, evaluates the current shared session buffer."
    },
    "sync_to_editor": {
      "type": "boolean",
      "default": true,
      "description": "When true, replaces the shared human editor buffer and broadcasts notifications/resources/updated for silverwave://session/buffer."
    },
    "expected_revision": {
      "type": "integer",
      "minimum": 0,
      "description": "Optional optimistic concurrency revision check against silverwave://session/buffer."
    },
    "quantize": {
      "type": "string",
      "enum": ["immediate", "next_cycle", "next_bar"],
      "default": "next_cycle"
    },
    "auto_start_clock": {
      "type": "boolean",
      "default": true
    }
  },
  "additionalProperties": false
}
```

#### 2.1.2 `evaluate_block`
Evaluates a single top-level expression block identified by line number, line range, or byte span in the shared buffer (1:1 parity with `Ctrl+Enter` in the human editor).

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBlockRequest {
    pub cursor_line: Option<u32>,
    pub line_range: Option<(u32, u32)>,
    pub byte_range: Option<(usize, usize)>,
    pub target_stream_override: Option<String>,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateBlockResponse {
    pub buffer_revision: u64,
    pub evaluated_span: SourceSpan,
    pub target_stream: String,
    pub scheduled_at_cycle: Rational64,
    pub highlight_spans: Vec<SourceSpan>,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "properties": {
    "cursor_line": {
      "type": "integer",
      "minimum": 1,
      "description": "1-indexed line number; automatically expands to the enclosing top-level pattern block."
    },
    "line_range": {
      "type": "array",
      "items": { "type": "integer", "minimum": 1 },
      "minItems": 2,
      "maxItems": 2,
      "description": "Inclusive [start_line, end_line] 1-indexed range."
    },
    "byte_range": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0 },
      "minItems": 2,
      "maxItems": 2,
      "description": "Half-open UTF-8 [start_byte, end_byte) span."
    },
    "target_stream_override": {
      "type": "string",
      "description": "Optional stream slot (e.g. 'd1'..'d16' or '$lead') overriding block assignment."
    },
    "quantize": {
      "type": "string",
      "enum": ["immediate", "next_cycle", "next_bar"],
      "default": "next_cycle"
    }
  },
  "additionalProperties": false
}
```

#### 2.1.3 `parse_mini_notation`
Parses a [Strudel](https://codeberg.org/uzu/strudel) / [TidalCycles](https://tidalcycles.org/docs/reference/pattern_structure/) mini-notation string (e.g., `"bd*2 [sd cp] <hh oh>(3,8,1) ~ @2 !3 ?0.25"`) into a normalized AST with exact leaf source spans for highlight tracking.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ParseMiniNotationRequest {
    pub notation: String,
    pub base_byte_offset: usize,
    pub base_line: u32,
    pub base_col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MiniAstNode {
    Atom { value: String, span: SourceSpan },
    Rest { span: SourceSpan },
    Sequence { children: Vec<MiniAstNode>, span: SourceSpan },
    Stack { layers: Vec<MiniAstNode>, span: SourceSpan },
    Slowcat { alternatives: Vec<MiniAstNode>, span: SourceSpan },
    Polymeter { children: Vec<MiniAstNode>, steps_per_cycle: Option<u32>, span: SourceSpan },
    Bjorklund { child: Box<MiniAstNode>, pulses: u32, steps: u32, rotation: i32, span: SourceSpan },
    Fast { child: Box<MiniAstNode>, factor: Rational64, span: SourceSpan },
    Slow { child: Box<MiniAstNode>, factor: Rational64, span: SourceSpan },
    Weight { child: Box<MiniAstNode>, weight: f64, span: SourceSpan },
    Replicate { child: Box<MiniAstNode>, count: u32, span: SourceSpan },
    Degrade { child: Box<MiniAstNode>, probability: f64, span: SourceSpan },
    RandomChoice { options: Vec<(MiniAstNode, f64)>, span: SourceSpan },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseMiniNotationResponse {
    pub ast: MiniAstNode,
    pub leaf_spans: Vec<SourceSpan>,
    pub implied_steps_per_cycle: Rational64,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["notation"],
  "properties": {
    "notation": {
      "type": "string",
      "description": "Mini-notation string without enclosing quotes or backticks."
    },
    "base_byte_offset": { "type": "integer", "minimum": 0, "default": 0 },
    "base_line": { "type": "integer", "minimum": 1, "default": 1 },
    "base_col": { "type": "integer", "minimum": 1, "default": 1 }
  },
  "additionalProperties": false
}
```

#### 2.1.4 `query_pattern_arc`
Queries a pattern expression or currently active stream over a rational cycle interval `[begin_cycle, end_cycle)`, returning every discrete `Hap` event with rational `whole` and `part` spans, control parameters, and active source highlight spans.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct QueryPatternArcRequest {
    pub pattern_code: Option<String>,
    pub stream_id: Option<String>,
    pub begin_cycle: Rational64,
    pub end_cycle: Rational64,
    pub max_events: u32,
    pub deterministic_seed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HapEventRecord {
    pub whole: Option<(Rational64, Rational64)>,
    pub part: (Rational64, Rational64),
    pub controls: BTreeMap<String, ControlValue>,
    pub source_spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlValue {
    Float(f64),
    Int(i64),
    Rational(Rational64),
    Text(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPatternArcResponse {
    pub query_arc: (Rational64, Rational64),
    pub total_events: usize,
    pub truncated: bool,
    pub events: Vec<HapEventRecord>,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["begin_cycle", "end_cycle"],
  "properties": {
    "pattern_code": {
      "type": "string",
      "description": "Standalone pattern expression to evaluate and query (mutually exclusive with stream_id)."
    },
    "stream_id": {
      "type": "string",
      "description": "Active stream ID (e.g. 'd1'..'d16') to query over the requested arc."
    },
    "begin_cycle": {
      "type": "object",
      "required": ["num", "den"],
      "properties": {
        "num": { "type": "integer" },
        "den": { "type": "integer", "minimum": 1 }
      },
      "additionalProperties": false
    },
    "end_cycle": {
      "type": "object",
      "required": ["num", "den"],
      "properties": {
        "num": { "type": "integer" },
        "den": { "type": "integer", "minimum": 1 }
      },
      "additionalProperties": false
    },
    "max_events": {
      "type": "integer",
      "minimum": 1,
      "maximum": 10000,
      "default": 1024
    },
    "deterministic_seed": {
      "type": "integer",
      "minimum": 0,
      "default": 42
    }
  },
  "additionalProperties": false
}
```

#### 2.1.5 `validate_syntax`
Validates a pattern buffer or expression without mutating any session or audio state, checking DSL syntax, mini-notation grammar, control parameter types, scale/voicing names, and loaded sample bank references.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ValidateSyntaxRequest {
    pub code: String,
    pub verify_sample_banks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Hint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub span: SourceSpan,
    pub suggested_replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidateSyntaxResponse {
    pub valid: bool,
    pub diagnostics: Vec<SyntaxDiagnostic>,
    pub referenced_streams: Vec<String>,
    pub referenced_samples: Vec<String>,
    pub sliders: Vec<SliderWidgetDescriptor>,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["code"],
  "properties": {
    "code": { "type": "string" },
    "verify_sample_banks": {
      "type": "boolean",
      "default": true,
      "description": "When true, emits warnings for sample names not currently registered in silverwave://dsp/samples."
    }
  },
  "additionalProperties": false
}
```

### 2.2 Domain 2: Streams & Tidal Transitions Tools

#### 2.2.1 `set_stream`
Assigns or replaces a pattern on a numbered stream (`d1`..`d16`), an indexed anonymous `$:` slot (`$0`..`$63`), or a custom named stream (`$bass`, `$drums`) with optional orbit binding and cycle quantization.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SetStreamRequest {
    pub stream_id: String,
    pub pattern_code: String,
    pub orbit: Option<u8>,
    pub quantize: QuantizeGrid,
    pub sync_to_editor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetStreamResponse {
    pub stream_id: String,
    pub orbit: u8,
    pub active_from_cycle: Rational64,
    pub buffer_revision: u64,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["stream_id", "pattern_code"],
  "properties": {
    "stream_id": {
      "type": "string",
      "pattern": "^(d([1-9]|1[0-6])|\\$:[0-9]+|\\$[a-zA-Z_][a-zA-Z0-9_]*)$",
      "description": "Canonical stream identifier: 'd1'..'d16', '$:0'..'$:63', or '$name'."
    },
    "pattern_code": {
      "type": "string",
      "description": "Pattern expression to bind to the stream."
    },
    "orbit": {
      "type": "integer",
      "minimum": 0,
      "maximum": 15,
      "description": "Optional audio orbit override (defaults to stream index modulo 16)."
    },
    "quantize": {
      "type": "string",
      "enum": ["immediate", "next_cycle", "next_bar"],
      "default": "next_cycle"
    },
    "sync_to_editor": {
      "type": "boolean",
      "default": true
    }
  },
  "additionalProperties": false
}
```

#### 2.2.2 `transition_stream`
Executes any of the 11 canonical [TidalCycles transitions](https://tidalcycles.org/docs/reference/transitions/) (`xfade`, `clutch`, `anticipate`, `interpolate`, `jump`, `jumpIn`, `jumpMod`, `histpan`, `wait`, `wash`, `mortal`) from a stream's currently playing pattern to a new target pattern.

| Transition Kind | Mathematical / Scheduling Semantics | Required / Default Parameters |
| :--- | :--- | :--- |
| `xfade` | Equal-power cosine crossfade of `gain` between old and new pattern over `cycles` (default `4.0`). | `cycles: f64` (default `4.0`) |
| `clutch` | Probabilistic step-by-step event replacement (`degradeBy` complementary ramps) over `cycles` (default `2.0`). | `cycles: f64` (default `2.0`) |
| `anticipate` | Builds tension (comb/delay/filter sweep accumulation) over `cycles` (default `8.0`) before dropping into the new pattern. | `cycles: f64` (default `8.0`) |
| `interpolate` | Continuously interpolates shared numeric control parameters (`lpf`, `note`, `speed`, `pan`, `gain`) over `cycles` (default `4.0`). | `cycles: f64` (default `4.0`) |
| `jump` | Immediate phase-aligned switch to the new pattern without resetting cycle phase. | *(none)* |
| `jumpIn` | Switches to the new pattern after exactly `cycles` integer cycles have elapsed. | `cycles: f64` (default `4.0`) |
| `jumpMod` | Switches to the new pattern at the next cycle boundary where `cycle_number % modulo == 0`. | `modulo: u32` (default `4`) |
| `histpan` | Pushes historical patterns across `pan_slots` stereo positions (`0.0..1.0`) before retiring the oldest layer. | `pan_slots: u8` (default `4`) |
| `wait` | Silences the stream for `delay_cycles` cycles before activating the new pattern. | `delay_cycles: f64` (default `4.0`) |
| `wash` | Applies an effect transformation expression (`wash_expr`, e.g., `".room(0.9).lpf(400).every(2, rev)"`) to the outgoing pattern for `in_cycles`, then cross-switches and decays over `out_cycles`. | `wash_expr: String`, `in_cycles: f64`, `out_cycles: f64` |
| `mortal` | Plays the new pattern for `lifespan_cycles` cycles, then progressively degrades it to silence over `degrade_cycles` cycles. | `lifespan_cycles: f64`, `degrade_cycles: f64` |

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TidalTransitionSpec {
    Xfade { cycles: f64 },
    Clutch { cycles: f64 },
    Anticipate { cycles: f64 },
    Interpolate { cycles: f64 },
    Jump,
    JumpIn { cycles: u32 },
    JumpMod { modulo: u32, offset: u32 },
    Histpan { pan_slots: u8 },
    Wait { delay_cycles: f64 },
    Wash {
        wash_expr: String,
        in_cycles: f64,
        out_cycles: f64,
    },
    Mortal {
        lifespan_cycles: f64,
        degrade_cycles: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionStreamRequest {
    pub stream_id: String,
    pub new_pattern_code: String,
    pub transition: TidalTransitionSpec,
    pub sync_to_editor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionStreamResponse {
    pub stream_id: String,
    pub transition_kind: String,
    pub start_cycle: Rational64,
    pub completion_cycle: Rational64,
    pub buffer_revision: u64,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["stream_id", "new_pattern_code", "transition_kind"],
  "properties": {
    "stream_id": {
      "type": "string",
      "pattern": "^(d([1-9]|1[0-6])|\\$:[0-9]+|\\$[a-zA-Z_][a-zA-Z0-9_]*)$"
    },
    "new_pattern_code": { "type": "string" },
    "transition_kind": {
      "type": "string",
      "enum": [
        "xfade",
        "clutch",
        "anticipate",
        "interpolate",
        "jump",
        "jumpIn",
        "jumpMod",
        "histpan",
        "wait",
        "wash",
        "mortal"
      ]
    },
    "cycles": { "type": "number", "exclusiveMinimum": 0.0, "default": 4.0 },
    "modulo": { "type": "integer", "minimum": 1, "default": 4 },
    "offset": { "type": "integer", "minimum": 0, "default": 0 },
    "pan_slots": { "type": "integer", "minimum": 2, "maximum": 16, "default": 4 },
    "delay_cycles": { "type": "number", "exclusiveMinimum": 0.0, "default": 4.0 },
    "wash_expr": { "type": "string", "default": ".room(0.8).delay(0.6)" },
    "in_cycles": { "type": "number", "minimum": 0.0, "default": 4.0 },
    "out_cycles": { "type": "number", "minimum": 0.0, "default": 2.0 },
    "lifespan_cycles": { "type": "number", "exclusiveMinimum": 0.0, "default": 8.0 },
    "degrade_cycles": { "type": "number", "exclusiveMinimum": 0.0, "default": 4.0 },
    "sync_to_editor": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```

#### 2.2.3 `mute_stream`, `unmute_stream`, `solo_stream`, `unsolo_stream`
Controls stream audibility masks without altering pattern evaluation phase.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct StreamToggleRequest {
    pub stream_ids: Vec<String>,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamStateSummary {
    pub stream_id: String,
    pub muted: bool,
    pub soloed: bool,
    pub effectively_audible: bool,
    pub orbit: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamToggleResponse {
    pub applied_at_cycle: Rational64,
    pub streams: Vec<StreamStateSummary>,
}
```
- **JSON Schema (`inputSchema`)** (shared across `mute_stream`, `unmute_stream`, `solo_stream`, `unsolo_stream`):
```json
{
  "type": "object",
  "required": ["stream_ids"],
  "properties": {
    "stream_ids": {
      "type": "array",
      "minItems": 1,
      "items": { "type": "string" },
      "description": "List of stream IDs ('d1'..'d16', '$:0', '$name') or ['all']."
    },
    "quantize": {
      "type": "string",
      "enum": ["immediate", "next_cycle", "next_bar"],
      "default": "immediate"
    }
  },
  "additionalProperties": false
}
```

#### 2.2.4 `hush` & `panic`
- `hush`: Stops scheduling new `Hap` events across all streams (`d1..d16`, `$:`) while allowing active synth envelopes and orbit reverb/delay tails to decay naturally.
- `panic`: Immediate emergency cutoff within a single audio callback block — clears all streams, zeroes all synth voices and orbit feedback buffers, and transmits MIDI CC 120 (`All Sound Off`) and CC 123 (`All Notes Off`) across all open MIDI output ports.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct HushRequest {
    pub quantize: QuantizeGrid,
    pub release_solo_and_mute: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HushResponse {
    pub silenced_streams: Vec<String>,
    pub effective_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanicRequest {
    pub send_midi_all_notes_off: bool,
    pub clear_delay_reverb_tails: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanicResponse {
    pub killed_voices: u32,
    pub cleared_orbits: u8,
    pub midi_ports_flushed: usize,
    pub sample_frame: u64,
}
```
- **JSON Schema (`inputSchema` for `hush`)**:
```json
{
  "type": "object",
  "properties": {
    "quantize": {
      "type": "string",
      "enum": ["immediate", "next_cycle", "next_bar"],
      "default": "immediate"
    },
    "release_solo_and_mute": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```
- **JSON Schema (`inputSchema` for `panic`)**:
```json
{
  "type": "object",
  "properties": {
    "send_midi_all_notes_off": { "type": "boolean", "default": true },
    "clear_delay_reverb_tails": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```

### 2.3 Domain 3: Clock & Ableton Link Tools

#### 2.3.1 `get_clock_snapshot`
Reads an atomic snapshot of the rational cycle clock, sample frame counter, metric signature, and [Ableton Link](https://ableton.github.io/link/) session state.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetClockSnapshotRequest {}

#[derive(Debug, Clone, PartialEq)]
pub struct AbletonLinkStatusResponse {
    pub enabled: bool,
    pub num_peers: usize,
    pub quantum_beats: f64,
    pub start_stop_sync: bool,
    pub link_beat: f64,
    pub link_phase: f64,
    pub latency_offset_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClockSnapshotResponse {
    pub is_playing: bool,
    pub cycle_rational: Rational64,
    pub cycle_f64: f64,
    pub cps: f64,
    pub bpm: f64,
    pub beats_per_cycle: f64,
    pub bar_index: u64,
    pub beat_in_bar: f64,
    pub sample_frame: u64,
    pub sample_rate: u32,
    pub audio_buffer_frames: u32,
    pub xrun_count: u64,
    pub link: AbletonLinkStatusResponse,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

#### 2.3.2 `set_cps`, `set_bpm`, `set_beats_per_cycle`, `nudge_clock`, `configure_ableton_link`
- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SetCpsRequest {
    pub cps: f64,
    pub ramp_cycles: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetBpmRequest {
    pub bpm: f64,
    pub beats_per_cycle: Option<f64>,
    pub ramp_cycles: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetBeatsPerCycleRequest {
    pub beats_per_cycle: f64,
    pub preserve_bpm: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NudgeClockRequest {
    pub phase_delta_cycles: Option<f64>,
    pub offset_ms: Option<f64>,
    pub slew_duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureAbletonLinkRequest {
    pub enabled: bool,
    pub quantum_beats: Option<f64>,
    pub start_stop_sync: Option<bool>,
    pub latency_offset_ms: Option<f64>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `set_cps`:
```json
{
  "type": "object",
  "required": ["cps"],
  "properties": {
    "cps": { "type": "number", "minimum": 0.001, "maximum": 100.0 },
    "ramp_cycles": { "type": "number", "minimum": 0.0 }
  },
  "additionalProperties": false
}
```
  - `set_bpm`:
```json
{
  "type": "object",
  "required": ["bpm"],
  "properties": {
    "bpm": { "type": "number", "minimum": 1.0, "maximum": 999.0 },
    "beats_per_cycle": { "type": "number", "minimum": 0.25, "maximum": 64.0 },
    "ramp_cycles": { "type": "number", "minimum": 0.0 }
  },
  "additionalProperties": false
}
```
  - `set_beats_per_cycle`:
```json
{
  "type": "object",
  "required": ["beats_per_cycle"],
  "properties": {
    "beats_per_cycle": { "type": "number", "minimum": 0.25, "maximum": 64.0 },
    "preserve_bpm": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```
  - `nudge_clock`:
```json
{
  "type": "object",
  "properties": {
    "phase_delta_cycles": { "type": "number", "minimum": -1.0, "maximum": 1.0 },
    "offset_ms": { "type": "number", "minimum": -500.0, "maximum": 500.0 },
    "slew_duration_ms": { "type": "number", "minimum": 10.0, "maximum": 5000.0, "default": 250.0 }
  },
  "additionalProperties": false
}
```
  - `configure_ableton_link`:
```json
{
  "type": "object",
  "required": ["enabled"],
  "properties": {
    "enabled": { "type": "boolean" },
    "quantum_beats": { "type": "number", "minimum": 1.0, "maximum": 64.0, "default": 4.0 },
    "start_stop_sync": { "type": "boolean", "default": true },
    "latency_offset_ms": { "type": "number", "minimum": -200.0, "maximum": 200.0, "default": 0.0 }
  },
  "additionalProperties": false
}
```

### 2.4 Domain 4: Interactive Widgets & Controls Tools

#### 2.4.1 `list_sliders` & `update_slider`
In [Strudel](https://strudel.cc/blog/), inline sliders (`slider(value, min, max, step)`) embed draggable UI handles directly inside the live-coded text buffer and rewrite their first literal argument in place when dragged.
- `list_sliders` enumerates every inline slider, ribbon, toggle, or select widget parsed in the current buffer with its deterministic `widget_id` and exact `value_literal_span`.
- `update_slider` updates the live control value on the audio engine AND performs an atomic, surgical AST literal patch in `silverwave://session/buffer` (when `patch_source_in_place == true`), preserving human cursor offsets outside `value_literal_span` and emitting `notifications/resources/updated` for both `silverwave://session/widgets` and `silverwave://session/buffer`.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSlidersRequest {}

#[derive(Debug, Clone, PartialEq)]
pub struct SliderWidgetDescriptor {
    pub widget_id: String,
    pub index_in_buffer: usize,
    pub label: Option<String>,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
    pub call_span: SourceSpan,
    pub value_literal_span: SourceSpan,
    pub owning_stream_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListSlidersResponse {
    pub buffer_revision: u64,
    pub sliders: Vec<SliderWidgetDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateSliderRequest {
    pub widget_id: String,
    pub value: f64,
    pub patch_source_in_place: bool,
    pub slew_ms: f64,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateSliderResponse {
    pub widget_id: String,
    pub clamped_value: f64,
    pub new_buffer_revision: u64,
    pub patched_span: Option<SourceSpan>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `list_sliders`:
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```
  - `update_slider`:
```json
{
  "type": "object",
  "required": ["widget_id", "value"],
  "properties": {
    "widget_id": {
      "type": "string",
      "description": "Stable widget identifier returned by list_sliders (e.g. 'slider:0' or labeled ID)."
    },
    "value": { "type": "number" },
    "patch_source_in_place": {
      "type": "boolean",
      "default": true,
      "description": "Rewrites the first literal argument of slider(val, min, max, step) in the shared buffer."
    },
    "slew_ms": {
      "type": "number",
      "minimum": 0.0,
      "maximum": 10000.0,
      "default": 15.0
    },
    "expected_revision": { "type": "integer", "minimum": 0 }
  },
  "additionalProperties": false
}
```

#### 2.4.2 `set_control_state` & `get_control_state`
Manages named global state values ([TidalCycles State Values](https://tidalcycles.org/docs/reference/state_values/) parity: `setF`, `setI`, `setR`, `setS`, `setB` read via `cF`, `cI`, `cR`, `cS`, `cB` inside patterns).

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SetControlStateRequest {
    pub controls: BTreeMap<String, ControlValue>,
    pub slew_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetControlStateResponse {
    pub updated_keys: Vec<String>,
    pub applied_at_cycle: Rational64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetControlStateRequest {
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlStateEntry {
    pub key: String,
    pub value: ControlValue,
    pub updated_at_cycle: Rational64,
    pub origin: ActorOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetControlStateResponse {
    pub entries: BTreeMap<String, ControlStateEntry>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `set_control_state`:
```json
{
  "type": "object",
  "required": ["controls"],
  "properties": {
    "controls": {
      "type": "object",
      "additionalProperties": {
        "oneOf": [
          { "type": "number" },
          { "type": "string" },
          { "type": "boolean" }
        ]
      }
    },
    "slew_ms": { "type": "number", "minimum": 0.0, "maximum": 10000.0, "default": 0.0 }
  },
  "additionalProperties": false
}
```
  - `get_control_state`:
```json
{
  "type": "object",
  "properties": {
    "keys": {
      "type": "array",
      "items": { "type": "string" }
    }
  },
  "additionalProperties": false
}
```

### 2.5 Domain 5: DSP, Orbits & Samples Tools

#### 2.5.1 `list_sample_banks` & `load_sample_source`
Supports loading sample banks from `strudel.json` manifests ([Strudel Dough](https://dough.strudel.cc/) parity), `github:owner/repo[/ref]` repositories, sandboxed local directories, and SoundFont (`.sf2` / `.sf3`) archives.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ListSampleBanksRequest {
    pub filter_prefix: Option<String>,
    pub include_variants: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleBankEntry {
    pub bank_name: String,
    pub variant_count: usize,
    pub loaded_variants: usize,
    pub source_kind: String,
    pub memory_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListSampleBanksResponse {
    pub total_banks: usize,
    pub total_memory_bytes: usize,
    pub banks: Vec<SampleBankEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SampleSourceDescriptor {
    StrudelJsonManifest { url_or_path: String },
    GitHubRepo { owner_repo: String, git_ref: Option<String>, subpath: Option<String> },
    LocalDirectory { path: PathBuf },
    SoundFont2 { path: PathBuf, bank_prefix: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadSampleSourceRequest {
    pub source: SampleSourceDescriptor,
    pub bank_prefix: Option<String>,
    pub eager_decode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadSampleSourceResponse {
    pub registered_banks: Vec<String>,
    pub total_samples_registered: usize,
    pub decoded_immediately: usize,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `list_sample_banks`:
```json
{
  "type": "object",
  "properties": {
    "filter_prefix": { "type": "string" },
    "include_variants": { "type": "boolean", "default": false }
  },
  "additionalProperties": false
}
```
  - `load_sample_source`:
```json
{
  "type": "object",
  "required": ["source_type", "location"],
  "properties": {
    "source_type": {
      "type": "string",
      "enum": ["strudel_json", "github", "local_dir", "sf2"]
    },
    "location": {
      "type": "string",
      "description": "URL/path to strudel.json, 'github:owner/repo/branch', sandboxed directory path, or .sf2 path."
    },
    "bank_prefix": {
      "type": "string",
      "description": "Optional prefix prepended to loaded bank names."
    },
    "eager_decode": {
      "type": "boolean",
      "default": false,
      "description": "When true, decodes all audio files into RAM on the background pool before returning."
    }
  },
  "additionalProperties": false
}
```

#### 2.5.2 `configure_orbit` & `get_orbit_telemetry`
Configures and monitors SilverWave's 16 independent audio orbits (`0..15`), each carrying its own stereo reverb processor, feedback delay line, sidechain ducker (`duckorbit`), master compressor, and hardware output pair assignment.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOrbitRequest {
    pub orbit_index: u8,
    pub gain_db: Option<f64>,
    pub pan: Option<f64>,
    pub muted: Option<bool>,
    pub reverb_room: Option<f64>,
    pub reverb_size: Option<f64>,
    pub reverb_ir_sample: Option<String>,
    pub delay_send: Option<f64>,
    pub delay_time_sec: Option<f64>,
    pub delay_feedback: Option<f64>,
    pub duck_target_orbit: Option<u8>,
    pub duck_attack_sec: Option<f64>,
    pub duck_depth: Option<f64>,
    pub output_channel_pair: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrbitStateSnapshot {
    pub orbit_index: u8,
    pub gain_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub active_voices: u32,
    pub rms_dbfs_l: f32,
    pub rms_dbfs_r: f32,
    pub peak_dbfs_l: f32,
    pub peak_dbfs_r: f32,
    pub compressor_gr_db: f32,
    pub reverb_room: f64,
    pub delay_send: f64,
    pub delay_feedback: f64,
    pub duck_target_orbit: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOrbitResponse {
    pub orbit: OrbitStateSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetOrbitTelemetryRequest {
    pub orbit_indices: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetOrbitTelemetryResponse {
    pub dsp_cpu_load_percent: f32,
    pub total_active_voices: u32,
    pub orbits: Vec<OrbitStateSnapshot>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `configure_orbit`:
```json
{
  "type": "object",
  "required": ["orbit_index"],
  "properties": {
    "orbit_index": { "type": "integer", "minimum": 0, "maximum": 15 },
    "gain_db": { "type": "number", "minimum": -96.0, "maximum": 18.0 },
    "pan": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "muted": { "type": "boolean" },
    "reverb_room": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reverb_size": { "type": "number", "minimum": 0.0, "maximum": 10.0 },
    "reverb_ir_sample": { "type": "string" },
    "delay_send": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "delay_time_sec": { "type": "number", "minimum": 0.001, "maximum": 10.0 },
    "delay_feedback": { "type": "number", "minimum": 0.0, "maximum": 0.99 },
    "duck_target_orbit": { "type": "integer", "minimum": 0, "maximum": 15 },
    "duck_attack_sec": { "type": "number", "minimum": 0.0005, "maximum": 2.0 },
    "duck_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "output_channel_pair": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0 },
      "minItems": 2,
      "maxItems": 2
    }
  },
  "additionalProperties": false
}
```
  - `get_orbit_telemetry`:
```json
{
  "type": "object",
  "properties": {
    "orbit_indices": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0, "maximum": 15 }
    }
  },
  "additionalProperties": false
}
```

### 2.6 Domain 6: External I/O Tools (MIDI, OSC & Csound)

#### 2.6.1 `list_midi_ports`, `configure_midi_map`, `send_midi_command`
- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMidiPortsRequest {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortDescriptor {
    pub port_id: String,
    pub display_name: String,
    pub direction: MidiPortDirection,
    pub connected: bool,
    pub clock_sync_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiPortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMidiPortsResponse {
    pub inputs: Vec<MidiPortDescriptor>,
    pub outputs: Vec<MidiPortDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureMidiMapRequest {
    pub mapping_id: String,
    pub input_port_id: String,
    pub channel: Option<u8>,
    pub cc_or_note: u8,
    pub is_note_trigger: bool,
    pub target_kind: MidiMappingTarget,
    pub range_min: f64,
    pub range_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MidiMappingTarget {
    SliderWidget { widget_id: String },
    ControlState { key: String },
    StreamMuteToggle { stream_id: String },
    StreamSoloToggle { stream_id: String },
    HushTrigger,
    TapTempo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureMidiMapResponse {
    pub mapping_id: String,
    pub active_mappings_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendMidiCommandRequest {
    pub output_port_id: String,
    pub channel: u8,
    pub command: MidiOutboundCommand,
    pub schedule_at_cycle: Option<Rational64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MidiOutboundCommand {
    NoteOn { note: u8, velocity: u8, duration_cycles: Option<f64> },
    NoteOff { note: u8, velocity: u8 },
    ControlChange { cc: u8, value: u8 },
    ProgramChange { program: u8 },
    PitchBend { value_14bit: u16 },
    ClockStart,
    ClockStop,
    ClockContinue,
    SysEx { bytes: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendMidiCommandResponse {
    pub dispatched: bool,
    pub scheduled_cycle: Rational64,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `list_midi_ports`:
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```
  - `configure_midi_map`:
```json
{
  "type": "object",
  "required": ["mapping_id", "input_port_id", "cc_or_note", "target_type", "target_id"],
  "properties": {
    "mapping_id": { "type": "string" },
    "input_port_id": { "type": "string" },
    "channel": { "type": "integer", "minimum": 1, "maximum": 16 },
    "cc_or_note": { "type": "integer", "minimum": 0, "maximum": 127 },
    "is_note_trigger": { "type": "boolean", "default": false },
    "target_type": {
      "type": "string",
      "enum": ["slider", "control_state", "stream_mute", "stream_solo", "hush", "tap_tempo"]
    },
    "target_id": { "type": "string" },
    "range_min": { "type": "number", "default": 0.0 },
    "range_max": { "type": "number", "default": 1.0 }
  },
  "additionalProperties": false
}
```
  - `send_midi_command`:
```json
{
  "type": "object",
  "required": ["output_port_id", "command_type"],
  "properties": {
    "output_port_id": { "type": "string" },
    "channel": { "type": "integer", "minimum": 1, "maximum": 16, "default": 1 },
    "command_type": {
      "type": "string",
      "enum": ["note_on", "note_off", "cc", "program_change", "pitch_bend", "clock_start", "clock_stop", "clock_continue", "sysex"]
    },
    "note": { "type": "integer", "minimum": 0, "maximum": 127 },
    "velocity": { "type": "integer", "minimum": 0, "maximum": 127, "default": 100 },
    "duration_cycles": { "type": "number", "exclusiveMinimum": 0.0 },
    "cc": { "type": "integer", "minimum": 0, "maximum": 127 },
    "value": { "type": "integer", "minimum": 0, "maximum": 16383 },
    "sysex_bytes": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0, "maximum": 255 },
      "maxItems": 1024
    }
  },
  "additionalProperties": false
}
```

#### 2.6.2 `configure_osc_target` & `load_csound_orc`
- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOscTargetRequest {
    pub target_name: String,
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub address_path: String,
    pub lookahead_latency_ms: f64,
    pub superdirt_compatible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureOscTargetResponse {
    pub target_name: String,
    pub active: bool,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadCsoundOrcRequest {
    pub orc_code: Option<String>,
    pub orc_file_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadCsoundOrcResponse {
    pub compiled_successfully: bool,
    pub registered_instruments: Vec<String>,
    pub compiler_messages: Vec<String>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `configure_osc_target`:
```json
{
  "type": "object",
  "required": ["target_name", "enabled", "host", "port"],
  "properties": {
    "target_name": { "type": "string", "default": "superdirt" },
    "enabled": { "type": "boolean" },
    "host": { "type": "string", "default": "127.0.0.1" },
    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 57120 },
    "address_path": { "type": "string", "default": "/dirt/play" },
    "lookahead_latency_ms": { "type": "number", "minimum": 0.0, "maximum": 2000.0, "default": 100.0 },
    "superdirt_compatible": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```
  - `load_csound_orc`:
```json
{
  "type": "object",
  "properties": {
    "orc_code": { "type": "string", "description": "Inline Csound orchestra (.orc) instrument definitions." },
    "orc_file_path": { "type": "string", "description": "Sandboxed file path to a .orc file." }
  },
  "additionalProperties": false
}
```

### 2.7 Domain 7: Visualizers, Telemetry & Export Tools

#### 2.7.1 `capture_visualizer_frame`
Captures a deterministic frame of any of SilverWave's 6 visualizers (`pianoroll`, `punchcard`, `spiral`, `scope`, `pitchwheel`, `spectrum`), returning either the strongly-typed `DrawCmd` scene graph AST (`draw_ast`), a vector `svg` document, or a base64 `png` image content block.

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerKind {
    Pianoroll,
    Punchcard,
    Spiral,
    Scope,
    Pitchwheel,
    Spectrum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerOutputFormat {
    DrawAstJson,
    Svg,
    PngBase64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Clear { rgba: [f32; 4] },
    Rect { x: f32, y: f32, w: f32, h: f32, corner_radius: f32, fill_rgba: [f32; 4], stroke_rgba: Option<([f32; 4], f32)>, active_hap: bool },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, rgba: [f32; 4] },
    Polyline { points: Vec<(f32, f32)>, width: f32, rgba: [f32; 4], closed: bool },
    ArcSegment { cx: f32, cy: f32, r_inner: f32, r_outer: f32, start_rad: f32, end_rad: f32, fill_rgba: [f32; 4] },
    Text { x: f32, y: f32, size_px: f32, rgba: [f32; 4], label: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureVisualizerFrameRequest {
    pub kind: VisualizerKind,
    pub format: VisualizerOutputFormat,
    pub stream_id: Option<String>,
    pub window_cycles: f64,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureVisualizerFrameResponse {
    pub kind: VisualizerKind,
    pub playhead_cycle: Rational64,
    pub draw_commands: Option<Vec<DrawCmd>>,
    pub svg_markup: Option<String>,
    pub png_base64: Option<String>,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "required": ["kind"],
  "properties": {
    "kind": {
      "type": "string",
      "enum": ["pianoroll", "punchcard", "spiral", "scope", "pitchwheel", "spectrum"]
    },
    "format": {
      "type": "string",
      "enum": ["draw_ast", "svg", "png"],
      "default": "draw_ast"
    },
    "stream_id": { "type": "string" },
    "window_cycles": { "type": "number", "minimum": 0.25, "maximum": 64.0, "default": 4.0 },
    "width_px": { "type": "integer", "minimum": 64, "maximum": 4096, "default": 800 },
    "height_px": { "type": "integer", "minimum": 64, "maximum": 2160, "default": 240 }
  },
  "additionalProperties": false
}
```

#### 2.7.2 `read_audio_telemetry`
Reads real-time signal analysis metrics from the lock-free master/orbit telemetry ring buffer: RMS, true peak, short-term LUFS, FFT spectrum bins in dBFS, spectral centroid, and Hydra-compatible normalized uniforms (`a.fft[0..3]`).

- **Rust Contracts**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ReadAudioTelemetryRequest {
    pub orbit_index: Option<u8>,
    pub fft_bins: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HydraAudioUniforms {
    pub fft_bands: [f32; 4],
    pub rms_normalized: f32,
    pub peak_normalized: f32,
    pub bpm: f32,
    pub cycle_time: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadAudioTelemetryResponse {
    pub sample_frame: u64,
    pub rms_dbfs_l: f32,
    pub rms_dbfs_r: f32,
    pub peak_dbfs_l: f32,
    pub peak_dbfs_r: f32,
    pub momentary_lufs: f32,
    pub spectral_centroid_hz: f32,
    pub zero_crossing_rate: f32,
    pub fft_magnitude_db: Vec<f32>,
    pub hydra_uniforms: HydraAudioUniforms,
}
```
- **JSON Schema (`inputSchema`)**:
```json
{
  "type": "object",
  "properties": {
    "orbit_index": {
      "type": "integer",
      "minimum": 0,
      "maximum": 15,
      "description": "Optional orbit index; if omitted, reads master bus telemetry."
    },
    "fft_bins": {
      "type": "integer",
      "enum": [32, 64, 128, 256, 512, 1024, 2048],
      "default": 128
    }
  },
  "additionalProperties": false
}
```

#### 2.7.3 `export_audio`, `encode_share_url`, `decode_share_url`, `run_doctor`
- **Rust Contracts**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioExportFormat {
    WavPcm16,
    WavPcm24,
    WavFloat32,
    Flac16,
    Flac24,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportAudioRequest {
    pub output_path: PathBuf,
    pub format: AudioExportFormat,
    pub code_override: Option<String>,
    pub cycles: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub sample_rate: u32,
    pub split_orbit_stems: bool,
    pub normalize_lufs: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportAudioResponse {
    pub written_files: Vec<PathBuf>,
    pub rendered_frames: u64,
    pub duration_seconds: f64,
    pub peak_dbfs: f32,
    pub integrated_lufs: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodeShareUrlRequest {
    pub code: Option<String>,
    pub base_url: String,
    pub include_session_metadata: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodeShareUrlResponse {
    pub share_url: String,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeShareUrlRequest {
    pub share_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeShareUrlResponse {
    pub code: String,
    pub cps: Option<f64>,
    pub referenced_sample_sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDoctorRequest {
    pub run_audio_loopback_probe: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorSubsystemCheck {
    pub subsystem: String,
    pub healthy: bool,
    pub latency_ms: Option<f64>,
    pub summary: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunDoctorResponse {
    pub overall_healthy: bool,
    pub checks: Vec<DoctorSubsystemCheck>,
}
```
- **JSON Schemas (`inputSchema`)**:
  - `export_audio`:
```json
{
  "type": "object",
  "required": ["output_path"],
  "properties": {
    "output_path": { "type": "string", "description": "Sandboxed output file path (.wav or .flac)." },
    "format": {
      "type": "string",
      "enum": ["wav_pcm16", "wav_pcm24", "wav_float32", "flac16", "flac24"],
      "default": "wav_pcm24"
    },
    "code_override": { "type": "string" },
    "cycles": { "type": "number", "exclusiveMinimum": 0.0, "maximum": 1024.0, "default": 16.0 },
    "duration_seconds": { "type": "number", "exclusiveMinimum": 0.0, "maximum": 3600.0 },
    "sample_rate": { "type": "integer", "enum": [44100, 48000, 88200, 96000], "default": 48000 },
    "split_orbit_stems": { "type": "boolean", "default": false },
    "normalize_lufs": { "type": "number", "minimum": -36.0, "maximum": -6.0 }
  },
  "additionalProperties": false
}
```
  - `encode_share_url`:
```json
{
  "type": "object",
  "properties": {
    "code": { "type": "string" },
    "base_url": { "type": "string", "default": "https://strudel.cc/#" },
    "include_session_metadata": { "type": "boolean", "default": true }
  },
  "additionalProperties": false
}
```
  - `decode_share_url`:
```json
{
  "type": "object",
  "required": ["share_url"],
  "properties": {
    "share_url": { "type": "string" }
  },
  "additionalProperties": false
}
```
  - `run_doctor`:
```json
{
  "type": "object",
  "properties": {
    "run_audio_loopback_probe": { "type": "boolean", "default": false }
  },
  "additionalProperties": false
}
```

## 3. Complete MCP Resources & Live Subscription Notifications

`silverwave-mcp` advertises `"resources": { "subscribe": true, "listChanged": true }` during the MCP `initialize` handshake. Clients read resource snapshots via `resources/read` and subscribe to live updates via `resources/subscribe`. Whenever a human editor, MIDI controller, cycle transition, or MCP tool mutates a subscribed state domain, the server emits a JSON-RPC notification:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/resources/updated",
  "params": {
    "uri": "silverwave://session/buffer"
  }
}
```

### 3.1 Normative Resource Catalog

| Resource URI / Template | MIME Type | Snapshot Payload Schema Summary | Update Notification Trigger (`notifications/resources/updated`) |
| :--- | :--- | :--- | :--- |
| `silverwave://session/buffer` | `application/json` | `{ revision: u64, code: String, dirty: bool, human_cursors: Vec<CursorSelection>, last_evaluated_revision: Option<u64>, last_actor: ActorOrigin }` | Human keystroke batch (debounced `50ms`), `evaluate_buffer`, `set_stream` with `sync_to_editor`, or `update_slider` in-place patch. |
| `silverwave://session/streams` | `application/json` | `{ cycle: Rational64, streams: Vec<{ id, code, orbit, muted, soloed, audible, active_transition: Option<TransitionProgress> }> }` | `evaluate_buffer`, `evaluate_block`, `set_stream`, `transition_stream`, `mute_stream`, `unmute_stream`, `solo_stream`, `unsolo_stream`, `hush`, `panic`, or transition completion. |
| `silverwave://session/clock` | `application/json` | `ClockSnapshotResponse` (`is_playing`, `cps`, `bpm`, `beats_per_cycle`, `cycle_rational`, `bar_index`, `link` status) | `set_cps`, `set_bpm`, `set_beats_per_cycle`, `nudge_clock`, `configure_ableton_link`, or external Ableton Link peer tempo change. |
| `silverwave://session/highlights` | `application/json` | `{ cycle: Rational64, active_spans: Vec<{ stream_id, span: SourceSpan, controls: BTreeMap<String, ControlValue> }> }` | Active `Hap` boundary change during playback, rate-limited by `--highlight-notify-hz` (default `30 Hz`). |
| `silverwave://session/widgets` | `application/json` | `{ buffer_revision: u64, sliders: Vec<SliderWidgetDescriptor>, controls: BTreeMap<String, ControlStateEntry>, midi_maps: Vec<MidiMapEntry> }` | Human slider drag, `update_slider`, `set_control_state`, incoming mapped MIDI CC, or buffer re-evaluation. |
| `silverwave://dsp/orbits` | `application/json` | `GetOrbitTelemetryResponse` (`dsp_cpu_load_percent`, `total_active_voices`, `orbits[0..15]`) | `configure_orbit`, `panic`, or periodic meter tick (`10 Hz` while subscribed). |
| `silverwave://dsp/samples` | `application/json` | `ListSampleBanksResponse` + background loader queue status (`pending_downloads`, `failed_sources`) | `load_sample_source` completion, `.sf2` registration, or Csound `.orc` hot-reload. |
| `silverwave://theory/scales` | `application/json` | `{ scales: BTreeMap<String, { intervals_semitones: Vec<f64>, edo: u16, aliases: Vec<String> }> }` | Static built-in dictionary + custom microtonal scale registration. |
| `silverwave://theory/voicings` | `application/json` | `{ dictionaries: BTreeMap<String, BTreeMap<String, Vec<String>>> }` ([Strudel Voicings](https://strudel.patternclub.org/blog/) parity) | Static built-in chord voicing dictionary + custom voicing registration. |
| `silverwave://viz/{kind}` | `application/json` | `CaptureVisualizerFrameResponse` (`kind` $\in$ `{pianoroll, punchcard, spiral, scope, pitchwheel, spectrum}`) | Frame tick (`15 Hz` while subscribed) or pattern evaluation. |

## 4. Complete MCP Prompts Catalog (`prompts/list` & `prompts/get`)

`silverwave-mcp` advertises `"prompts": { "listChanged": false }` and exposes 5 structured workflow prompts that automatically embed live MCP resources as context blocks.

### 4.1 Prompt Specifications & Rust Contracts

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilverWavePromptId {
    LiveCodeSession,
    ComposePattern,
    TransitionArrangement,
    SoundDesignSynth,
    DiagnoseAudioSetup,
}

pub trait SilverWaveMcpPromptSurface: Send + Sync + 'static {
    fn list_prompts(&self) -> Vec<PromptDescriptor>;
    fn get_prompt(
        &self,
        name: SilverWavePromptId,
        arguments: BTreeMap<String, String>,
    ) -> Result<RenderedPromptPayload, McpProtocolError>;
}
```

| Prompt Name | Arguments (`required` / `optional`) | Auto-Embedded Resource Context URIs | Behavioural Contract & Tool Orchestration Guidance |
| :--- | :--- | :--- | :--- |
| `live_code_session` | `role` *(optional: `"co_performer"` \| `"accompanist"` \| `"conductor"`)*, `style` *(optional: e.g., `"algorave_breakcore"`, `"dub_techno"`, `"ambient"`)* | `silverwave://session/buffer`, `silverwave://session/streams`, `silverwave://session/clock`, `silverwave://dsp/samples` | Instructs the agent on co-performance etiquette: always inspect `silverwave://session/buffer` revision before editing, use `update_slider` for non-disruptive parameter modulation, quantize stream mutations to `next_cycle` or `next_bar`, and avoid overwriting streams currently under human cursor focus. |
| `compose_pattern` | `target_stream` *(required: e.g., `"d1"`)*, `genre` *(required)*, `scale` *(optional: e.g., `"C:dorian"`)*, `meter` *(optional: default `"4/4"`)* | `silverwave://theory/scales`, `silverwave://theory/voicings`, `silverwave://dsp/samples`, `silverwave://session/clock` | Guides the agent to verify syntax with `validate_syntax`, verify rhythmic density over `[0, 4]` cycles with `query_pattern_arc`, and deploy the verified pattern via `set_stream`. |
| `transition_arrangement` | `target_section` *(required: `"intro"` \| `"build"` \| `"drop"` \| `"breakdown"` \| `"outro"`)*, `transition_bars` *(optional: default `"4"`)* | `silverwave://session/streams`, `silverwave://session/clock`, `silverwave://dsp/orbits` | Guides multi-stream choreography using `transition_stream` (`anticipate` for builds, `xfade` or `clutch` for blends, `wash` or `mortal` for breakdowns/outros) and `configure_orbit` for filter/reverb sweeps. |
| `sound_design_synth` | `timbre_goal` *(required)*, `engine` *(optional: `"superdough"` \| `"fm"` \| `"wavetable"` \| `"csound"`)*, `orbit` *(optional: `"0".."15"`)* | `silverwave://dsp/orbits`, `silverwave://dsp/samples`, `silverwave://session/widgets` | Guides synth parameter sculpting (`s("supersaw")`, FM index/ratio, filter envelopes, vowel formants, or `load_csound_orc`) and exposes macro parameters via inline `slider(val, min, max, step)` definitions. |
| `diagnose_audio_setup` | `symptom` *(optional: `"no_sound"` \| `"xruns_crackling"` \| `"midi_drift"` \| `"link_desync"`)* | `silverwave://session/clock`, `silverwave://dsp/orbits`, `silverwave://dsp/samples` | Instructs the agent to invoke `run_doctor`, `get_clock_snapshot`, `get_orbit_telemetry`, and `list_midi_ports` to pinpoint XRUNs, muted orbits, missing sample manifests, or Link/MIDI sync offsets. |

## 5. Security, Sandboxing & Real-Time Audio Safety

### 5.1 Real-Time Audio Thread Non-Blocking Invariant

The real-time audio callback thread (`silverwave-dsp`) operates under strict hard-real-time constraints:

1. **Zero Locks, Zero Allocations, Zero Syscalls on the Audio Thread**:
   - No MCP tool or resource read may ever acquire a `Mutex`, `RwLock`, heap allocator lock, or filesystem handle shared with the audio thread.
   - All mutations from `silverwave-mcp` (`set_stream`, `transition_stream`, `update_slider`, `configure_orbit`, `panic`, `nudge_clock`) are serialized into fixed-size `AudioEngineCommand` value structs and pushed onto pre-allocated, wait-free ring buffers (`rtrb` SPSC/MPSC queues).
   - All heavy operations — parsing `strudel.json`, decoding WAV/FLAC/MP3/OGG samples, parsing `.sf2` SoundFonts, compiling Csound `.orc` instruments, rendering PNG visualizer frames, and bouncing offline `export_audio` files — execute exclusively on dedicated background worker threads.
   - Decoded sample buffers (`Arc<AlignedAudioBuffer>`) and compiled Csound instrument graphs are swapped into the audio engine via atomic pointer exchange, and retired handles are reclaimed on a background garbage-collection thread so `drop()` never executes inside the audio callback.
2. **Bounded Command Ingestion Budget**:
   - The audio callback drains at most `MAX_COMMANDS_PER_AUDIO_BLOCK = 64` commands per callback frame, guaranteeing a worst-case command-ingestion execution time $< 25\ \mu\text{s}$ at `48 kHz / 128 frames`.

### 5.2 Filesystem Sandboxing & Path-Traversal Guards

Tools that accept filesystem paths or URIs (`load_sample_source`, `load_csound_orc`, `export_audio`) are enforced by `PathSandboxGuard`:

1. **Canonicalization & Root Containment**:
   - Every candidate path is checked for NUL bytes, parent-directory traversal components (`..`), Windows device namespaces (`\\.\`, `\\?\`), and unapproved URI schemes before resolution.
   - Existing input paths (`load_sample_source`, `load_csound_orc`) are canonicalized via `dunce::canonicalize` and verified to reside strictly beneath one of the configured `--sandbox-root` directories (defaulting to the user's configured sample directory and current project workspace). Symlinks pointing outside `--sandbox-root` are rejected with `-32006 SandboxViolation`.
   - Output paths (`export_audio`) canonicalize the parent directory, verify containment within an authorized export sandbox directory, enforce `.wav` or `.flac` extensions, and refuse to overwrite existing non-audio files or hidden dotfiles.
2. **Network & Decompression Quotas**:
   - Remote sample fetches (`strudel_json`, `github`) are permitted only when `--allow-external-network` is enabled, enforce HTTPS-only TLS 1.3 connections, block RFC 1918 / loopback / link-local IP ranges (SSRF guard), and cap individual sample downloads at `64 MiB` and total manifest bank size at `512 MiB`.

### 5.3 Token-Bucket Rate Limiting & Resource Quotas

```rust
pub trait PathSandboxGuard: Send + Sync + 'static {
    fn validate_read_path(&self, raw_path: &std::path::Path) -> Result<PathBuf, McpProtocolError>;
    fn validate_export_path(&self, raw_path: &std::path::Path) -> Result<PathBuf, McpProtocolError>;
    fn validate_remote_url(&self, raw_url: &str) -> Result<String, McpProtocolError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRateCategory {
    /// High-frequency real-time parameter tweaks (`update_slider`, `set_control_state`, `nudge_clock`): 120 RPS default.
    RealtimeControl,
    /// Pattern evaluation & stream transitions (`evaluate_buffer`, `set_stream`, `transition_stream`): 30 RPS default.
    PatternEvaluation,
    /// Heavy background jobs (`load_sample_source`, `export_audio`, `load_csound_orc`): 2 concurrent jobs, 4 RPM default.
    HeavyBackgroundIo,
    /// Read-only snapshots (`get_clock_snapshot`, `read_audio_telemetry`, `query_pattern_arc`): 120 RPS default.
    ReadOnlyQuery,
}

pub trait RateLimiterPolicy: Send + Sync + 'static {
    fn check_and_consume(
        &self,
        client_id: &str,
        category: ToolRateCategory,
    ) -> Result<(), McpProtocolError>;
}
```

## 6. TDD Verification Matrix — Protocol, Tool-Parity & Co-Performance Test Contracts

All implementations of `silverwave-mcp` MUST pass the following normative test contracts prior to release gating.

### 6.1 Tier 1: JSON-RPC 2.0 & MCP Transport Conformance Tests

```rust
#[cfg(test)]
pub mod tier1_protocol_conformance_contracts {
    use super::*;

    /// Verifies MCP `initialize` -> `notifications/initialized` handshake over `stdio`
    /// advertises all 42 tools, 10 resources (`subscribe: true`), and 5 prompts.
    pub fn test_mcp_initialize_capabilities_contract();

    /// Verifies `stdio` transport never emits non-JSON bytes on `stdout` even when
    /// DSP warnings, sample load errors, or tracing events are logged.
    pub fn test_stdio_stdout_purity_under_tracing_and_errors();

    /// Verifies Streamable HTTP `POST /mcp`, `GET /mcp` (SSE stream with `Mcp-Session-Id`
    /// and `Last-Event-ID` resumption), and `DELETE /mcp` session teardown.
    pub fn test_streamable_http_sse_lifecycle_and_resumption();

    /// Verifies Origin header validation and Bearer token enforcement on HTTP transport.
    pub fn test_http_transport_origin_and_bearer_auth_guard();

    /// Verifies JSON-RPC 2.0 error codes (-32700, -32600, -32601, -32602) on malformed inputs.
    pub fn test_jsonrpc_standard_error_codes_contract();
}
```

### 6.2 Tier 2: 1:1 Human-Agent Tool Parity Tests

```rust
#[cfg(test)]
pub mod tier2_human_agent_parity_contracts {
    use super::*;

    /// Asserts `evaluate_buffer` and `evaluate_block` produce the exact same scheduled `Hap`
    /// stream state and active highlight spans as human editor `Ctrl+Shift+Enter` and `Ctrl+Enter`.
    pub fn test_parity_evaluate_buffer_and_evaluate_block();

    /// Asserts `parse_mini_notation`, `query_pattern_arc`, and `validate_syntax` match
    /// the internal compiler AST, rational event spans, and LSP diagnostics byte-for-byte.
    pub fn test_parity_dsl_parse_query_and_validate();

    /// Asserts `set_stream` and all 11 `transition_stream` variants (`xfade`, `clutch`,
    /// `anticipate`, `interpolate`, `jump`, `jumpIn`, `jumpMod`, `histpan`, `wait`, `wash`, `mortal`)
    /// produce identical gain/pan/control envelopes to inline DSL transition invocations.
    pub fn test_parity_all_eleven_tidal_transitions();

    /// Asserts `mute_stream`, `unmute_stream`, `solo_stream`, `unsolo_stream`, `hush`, and `panic`
    /// match human UI transport controls and preserve non-soloed mute masks on `unsolo_stream`.
    pub fn test_parity_stream_mute_solo_hush_and_panic();

    /// Asserts `get_clock_snapshot`, `set_cps`, `set_bpm`, `set_beats_per_cycle`, `nudge_clock`,
    /// and `configure_ableton_link` maintain exact rational identity `cps == bpm / 60.0 / beats_per_cycle`.
    pub fn test_parity_clock_and_ableton_link_tools();

    /// Asserts `list_sliders`, `update_slider`, `set_control_state`, and `get_control_state`
    /// match human GUI slider drags and Tidal state value lookups (`cF`, `cI`, `cS`).
    pub fn test_parity_sliders_and_control_state_tools();

    /// Asserts `list_sample_banks`, `load_sample_source` (`strudel_json`, `github`, `local_dir`, `sf2`),
    /// `configure_orbit`, and `get_orbit_telemetry` match human sample & mixer actions.
    pub fn test_parity_dsp_orbits_and_sample_loader_tools();

    /// Asserts `list_midi_ports`, `configure_midi_map`, `send_midi_command`,
    /// `configure_osc_target`, and `load_csound_orc` match human external I/O bindings.
    pub fn test_parity_midi_osc_and_csound_tools();

    /// Asserts `capture_visualizer_frame` (`pianoroll`, `punchcard`, `spiral`, `scope`,
    /// `pitchwheel`, `spectrum` across `draw_ast`, `svg`, `png`), `read_audio_telemetry`,
    /// `export_audio` (WAV/FLAC), `encode_share_url`, `decode_share_url`, and `run_doctor`.
    pub fn test_parity_visualizers_telemetry_export_share_and_doctor();
}
```

### 6.3 Tier 3: Real-Time Human + MCP Co-Performance Synchronization Tests

```rust
#[cfg(test)]
pub mod tier3_coperformance_sync_contracts {
    use super::*;

    /// Verifies that calling `update_slider` with `patch_source_in_place: true` rewrites only
    /// the numeric literal span inside `slider(0.25, 0.0, 1.0, 0.01)` in `silverwave://session/buffer`,
    /// increments `revision`, preserves human cursor positions, and does not reset cycle phase.
    pub fn test_update_slider_surgical_in_place_ast_patch_preserves_human_cursor();

    /// Verifies optimistic concurrency control: when a human edits the buffer incrementing
    /// `revision` from N to N+1, a stale MCP `evaluate_buffer` or `update_slider` carrying
    /// `expected_revision: Some(N)` fails deterministically with `-32001 RevisionConflict`.
    pub fn test_optimistic_concurrency_expected_revision_conflict_detection();

    /// Verifies that subscribing to `silverwave://session/buffer`, `silverwave://session/streams`,
    /// `silverwave://session/clock`, `silverwave://session/highlights`, and `silverwave://session/widgets`
    /// delivers `notifications/resources/updated` within 16ms of a human or agent state mutation.
    pub fn test_live_resource_subscription_notification_latency_and_ordering();
}
```

### 6.4 Tier 4: Real-Time Audio Safety & Adversarial Security Tests

```rust
#[cfg(test)]
pub mod tier4_realtime_safety_and_security_contracts {
    use super::*;

    /// Runs a custom global allocator counter on the audio callback thread while saturating
    /// `silverwave-mcp` with 1,000 concurrent tool requests/sec;
    /// asserts 0 bytes allocated, 0 deallocations, and 0 mutex contentions on the audio thread.
    pub fn test_zero_audio_thread_allocations_or_locks_under_mcp_load();

    /// Verifies `PathSandboxGuard` rejects `../`, symlink escapes outside `--sandbox-root`,
    /// NUL bytes, and disallowed extensions on `load_sample_source`, `load_csound_orc`, and `export_audio`
    /// with `-32006 SandboxViolation`.
    pub fn test_path_traversal_and_symlink_escape_rejected();

    /// Verifies SSRF guard blocks loopback (`127.0.0.1`, `[::1]`) and RFC 1918 private network URLs
    /// when `load_sample_source` fetches `strudel.json` manifests.
    pub fn test_ssrf_private_network_rejection_on_remote_sample_fetch();

    /// Verifies `--read-only` mode blocks all mutating tools with `-32008 ReadOnlyModeViolation`
    /// while permitting all inspection tools and resource subscriptions.
    pub fn test_read_only_spectator_mode_enforcement();

    /// Verifies token-bucket rate limiter returns `-32029 RateLimitExceeded` when burst capacity
    /// is exceeded and recovers cleanly after token replenishment.
    pub fn test_token_bucket_rate_limiter_enforcement_and_recovery();
}
```

---

## 7. Category-King & 1:1+ Pain-Point MCP Extensions (`CK-01`–`CK-10` & `PP-01`–`PP-12`)

To preserve strict 1:1 surface parity with the enhancements specified in `08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md` and `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md`, `silverwave-mcp` exposes four additional tools (bringing the total MCP tool surface to **42 tools**) and enriches `read_audio_telemetry` with closed-loop acoustic mix diagnostics (`AcousticMixDiagnostic`):

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct AcousticMixDiagnostic {
    pub integrated_lufs: f32,
    pub short_term_lufs: f32,
    pub true_peak_dbtp: f32,
    pub crest_factor_db: f32,
    pub stereo_phase_correlation: f32,
    pub third_octave_spectrum_db: [f32; 31],
    pub masking_conflicts: Vec<InterOrbitConflictAlert>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterOrbitConflictAlert {
    pub orbit_a: u8,
    pub orbit_b: u8,
    pub center_freq_hz: f32,
    pub severity_db: f32,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    SubBassPhaseCancellation,
    LowMidSpectralMasking,
    TruePeakClippingRisk,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetCrossfaderSceneRequest {
    pub scene_a_id: Option<u8>,
    pub scene_b_id: Option<u8>,
    pub xfader_position: f32,
    pub slew_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerLooperFreezeRequest {
    pub enabled: bool,
    pub freeze_cycles: Rational64,
    pub target_orbit: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetFillLatchRequest {
    pub fill_active: bool,
    pub quantize: QuantizeGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderVideoRequest {
    pub output_path: PathBuf,
    pub preset: String, // "vertical_9x16_mp4", "horizontal_16x9_mp4", "alpha_overlay_webm"
    pub fps: u16,
    pub cycles: f64,
    pub embed_qr_permalink: bool,
}

pub trait SilverWaveMcpCategoryKingExtension: SilverWaveMcpToolSurface {
    fn set_crossfader_scene(&self, req: SetCrossfaderSceneRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn trigger_looper_freeze(&self, req: TriggerLooperFreezeRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn set_fill_latch(&self, req: SetFillLatchRequest) -> Result<ClockSnapshotResponse, McpProtocolError>;
    fn render_video(&self, req: RenderVideoRequest) -> Result<ExportAudioResponse, McpProtocolError>;
}
```
