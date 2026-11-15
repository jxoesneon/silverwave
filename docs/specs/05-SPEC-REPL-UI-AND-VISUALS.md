# `05-SPEC-REPL-UI-AND-VISUALS.md` — `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` & `silverwave-viz` Specification

**Status:** Publication-Ready Specification

**Crates:** `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-viz`

**Parity Targets:** [Strudel](https://codeberg.org/uzu/strudel) ([`@strudel/codemirror`, `@strudel/repl`](https://strudel.cc/technical-manual/repl/), [`@strudel/draw`](https://strudel.cc/learn/visual-feedback/), [`@strudel/hydra`](https://strudel.cc/learn/hydra/)) & [TidalCycles Editor Integrations](https://tidalcycles.org/docs/getting-started/editor/VS_Code/) (Pulsar, VSCode, Neovim, Emacs)

**Targets:** Native Desktop (`egui`/`wgpu`), Native TUI, Editor Protocol ([`LSP`](https://microsoft.github.io/language-server-protocol/)/JSON-RPC), Browser PWA (`wasm32-unknown-unknown` + CodeMirror 6 + WebAudio + Canvas/WebGPU)

## 1. Executive Scope & 1:1 Parity Matrix

`silverwave-gui` / `silverwave-wasm` and `silverwave-viz` provide the interactive live-coding environment, editor protocol bridge, and real-time audio/pattern visualization layer for the Silverwave ecosystem. Both crates share a single deterministic Rust core that compiles cleanly to native targets (`x86_64`, `aarch64`) and `wasm32-unknown-unknown`.

### 1.1 Parity Matrix

| Capability | Strudel / TidalCycles Reference | `silverwave-gui` / `silverwave-wasm` / `silverwave-viz` Contract | 
| ----- | ----- | ----- | 
| **REPL Lifecycle** | `@strudel/repl` (`repl()`, `evaluate()`, `stop()`, `setCps()`) | `ReplEngine` state machine driving scheduler, evaluator, and audio/MIDI transport | 
| **Browser Editor** | `@strudel/codemirror` (`StrudelMirror`, `highlight`, `flash`, `slider`) | `wasm32` bindings exposing `SilverwaveMirror` to CodeMirror 6 `StateField` / `ViewPlugin` | 
| **Native Desktop / TUI** | TidalCycles standalone / terminal workflows | `egui` + `wgpu` GPU-accelerated editor/canvas and ANSI/Kitty/Braille TUI frontend | 
| **External Editors** | TidalCycles Pulsar, VSCode (`vscode-tidalcycles`), Neovim (`vim-tidal`), Emacs (`tidal.el`) | Unified `silverwave-lsp` + JSON-RPC 2.0 stdio/TCP daemon for eval, halt, highlights, and inline viz | 
| **Pattern Visualizers** | `@strudel/draw` (`pianoroll`, `punchcard`, `spiral`, `pitchwheel`) | `silverwave-viz` vector command emitter (`DrawCmd`) with global & `_`-prefixed inline modes | 
| **Audio Visualizers** | `@strudel/draw` (`scope`/`tscope`, `spectrum`) | Lock-free ring-buffer time-domain & FFT frequency-domain `DrawCmd` emitters | 
| **Inline Widgets** | `@strudel/codemirror` (`slider(val, min, max, step)`) | `WidgetRegistry` + `RefValue` slot binding without full re-evaluation on drag | 
| **Token Highlighting** | `@strudel/codemirror` (`highlight.mjs`, `.color()`, `.markcss()`) | Adaptive `SpanMap` (UTF-8 byte spans <-> UTF-16 code units) with CSS/native style projection | 
| **Metadata Headers** | Strudel `@title`, `@by`, `@genre`, `@license`, `@details` | `DocMetadata` parser extracted prior to pattern evaluation | 
| **URL Sharing** | Strudel `#code=<base64>` URL state | `ShareCodec` (Deflate/Brotli + URL-safe Base64) round-trip serialization | 
| **Audio Export** | Offline bounce / stem export | Deterministic offline scheduler render to PCM WAV (16/24/32f) and lossless FLAC | 
| **Shader / Video Bridge** | `@strudel/hydra` (`initHydra({ detectAudio, feedStrudel })`) | `VisualBridge` trait exporting audio FFT bands, pattern uniforms, and WGSL/GLSL/Hydra hooks | 

## 2. Dual-Frontend Architecture & Shared Rust Core

The workspace enforces a strict separation between **pure state/rendering logic** (shared Rust core) and **platform I/O** (native `wgpu`/`egui`/TUI, editor LSP daemon, and `wasm32` browser PWA).

```
+-------------------------------------------------------------------------+
|                        Shared Rust Core (no_std + alloc)                |
|  +-----------------------------+   +---------------------------------+  |
|  |       silverwave-gui       |   |         silverwave-viz          |  |
|  | - ReplEngine & StateMachine |   | - DrawContext & DrawCmd AST     |  |
|  | - SpanMap (UTF-8 <-> UTF-16)|   | - pianoroll / punchcard / spiral|  |
|  | - WidgetRegistry (sliders)  |   | - scope / pitchwheel / spectrum |  |
|  | - DocMetadata & ShareCodec  |   | - VisualBridge (Hydra/Shaders)  |  |
|  +--------------+--------------+   +----------------+----------------+  |
+-----------------|-----------------------------------|-------------------+
                  |                                   |
      +-----------+-----------+           +-----------+-----------+
      |                       |           |                       |
      v                       v           v                       v
+-----------+           +-----------+ +-----------+         +-----------+
| Native UI |           | Editor    | | wasm32    |         | Offline   |
| egui/wgpu |           | Protocol  | | PWA       |         | Exporter  |
| & TUI     |           | LSP/RPC   | | CM6+Canvas|         | WAV/FLAC  |
+-----------+           +-----------+ +-----------+         +-----------+

```

### 2.1 Core Crate Trait & Error Contracts

```rust
pub type ByteOffset = usize;
pub type Utf16Offset = usize;
pub type CycleTime = f64;
pub type WidgetId = u32;
pub type PatternId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf16Span {
    pub start: Utf16Offset,
    pub end: Utf16Offset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplError {
    ParseError { message: String, span: Option<ByteSpan> },
    EvalError { message: String, span: Option<ByteSpan> },
    AudioBackendError { message: String },
    ExportError { message: String },
    ProtocolError { code: i32, message: String },
    ShareDecodeError { message: String },
}

pub trait ClockSource: Send + Sync {
    fn now_cycles(&self) -> CycleTime;
    fn cps(&self) -> f64;
    fn is_playing(&self) -> bool;
}

pub trait AudioTap: Send + Sync {
    /// Copies up to `out.len()` interleaved/mono time-domain samples into `out`.
    /// Returns the number of samples written.
    fn read_time_domain(&self, out: &mut [f32]) -> usize;

    /// Copies magnitude spectrum bins (in dBFS, typically -120.0..=0.0) into `out`.
    /// Returns the sample rate of the underlying audio stream.
    fn read_spectrum_db(&self, out: &mut [f32]) -> u32;
}
```

### 2.2 Native Desktop & TUI Frontend (`egui` / `wgpu` + Terminal)

The native binary (`silverwave-gui`) embeds both `silverwave-gui` / `silverwave-wasm` and `silverwave-viz`:

1. **Desktop GUI (`egui` + `wgpu`)**:

   * **Background Layer**: Full-viewport `wgpu` render pass executing global visualizers (`pianoroll`, `punchcard`, `spiral`, `scope`, `pitchwheel`, `spectrum`) and custom WGSL/Hydra-bridged shaders behind a translucent editor surface.

   * **Editor Layer**: Custom `egui::TextEdit` / `egui_code_editor` wrapper with UTF-8 `ByteSpan` highlight overlays, flash-on-eval animations, inline `egui::Slider` widgets anchored to `WidgetAnchor` line/column coordinates, and inline `_`-prefixed visualizer canvases rendered directly between code lines.

   * **Transport & Metadata Bar**: Displays `@title`, `@by`, `@genre`, active CPS, cycle counter, CPU/DSP load, Play/Halt toggle, and WAV/FLAC export modal.

2. **Terminal TUI (`ratatui` / `crossterm`)**:

   * **Code Pane**: Syntax-highlighted buffer with active-hap ANSI background/underline styling mapped from `HighlightStyle`.

   * **Visualizer Pane**: Renders `DrawCmd` streams via Unicode Braille (`U+2800..U+28FF`) sub-pixel canvases or Kitty/Sixel graphics protocol when supported by the terminal emulator.

```rust
pub trait NativeFrontend {
    fn poll_events(&mut self) -> Vec<ReplCommand>;
    fn render_frame(&mut self, state: &ReplViewState, draws: &[VizFrame]) -> Result<(), ReplError>;
    fn capabilities(&self) -> FrontendCapabilities;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontendCapabilities {
    pub supports_inline_widgets: bool,
    pub supports_inline_viz: bool,
    pub supports_gpu_shaders: bool,
    pub supports_css_mark: bool,
    pub pixel_ratio_milli: u32,
}
```

### 2.3 Editor Protocol & LSP / JSON-RPC Bridge (Neovim, VSCode, Emacs, Pulsar)

To achieve 1:1 parity with [TidalCycles' Editor Ecosystem](https://tidalcycles.org/docs/getting-started/editor/VS_Code/) (`vim-tidal`, `vscode-tidalcycles`, `tidal.el`) and [Language Server Protocol (LSP)](https://microsoft.github.io/language-server-protocol/), `silverwave-gui` / `silverwave-wasm` exposes a headless daemon (`silverwave-lsp`) communicating over **stdio** or **TCP/WebSocket** using **JSON-RPC 2.0** (compatible with standard LSP `textDocument/*` plus `silverwave/*` custom notifications/requests).

#### Supported Editor Integrations

| Editor | Transport | Highlighting Mechanism | Inline Viz / Sliders | 
| ----- | ----- | ----- | ----- | 
| **Neovim** | `stdio` LSP | `nvim_buf_set_extmark` with UTF-8 byte offsets | Virtual text / floating `wgpu` or Kitty/Sixel popup | 
| **VSCode** | `stdio` LSP | `TextEditorDecorationType` with UTF-16 ranges | Webview panel + CodeLens / `InlayHint` slider controls | 
| **Emacs** | `stdio` / TCP | `eglot` / `lsp-mode` overlays with UTF-8/char offsets | Side-window SVG/PNG frame or `posframe` | 
| **Pulsar** | `stdio` / WebSocket | `TextEditor.decorateMarker` with UTF-16 ranges | Embedded Chromium Canvas panel (shared `silverwave-viz` protocol) | 

#### Custom JSON-RPC 2.0 / LSP Contract

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRpcRequest {
    /// Evaluates either the full document or a delimited block/stanza.
    Eval {
        uri: String,
        text: String,
        range: Option<LspRange>,
        version: i32,
    },
    /// Halts all active patterns and triggers all-notes-off (`Ctrl+.`).
    Halt {
        uri: Option<String>,
    },
    /// Updates an inline slider value without triggering full re-evaluation.
    UpdateWidget {
        uri: String,
        widget_id: WidgetId,
        value: f64,
    },
    /// Exports current pattern to WAV or FLAC.
    ExportAudio {
        uri: String,
        format: AudioExportFormat,
        cycles: f64,
        output_path: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorRpcNotification {
    /// Pushed at 60 Hz (or configured frame rate) with active token highlights.
    ActiveHighlights {
        uri: String,
        version: i32,
        spans: Vec<ActiveHighlight>,
    },
    /// Pushed after eval to register/refresh inline widgets and inline viz anchors.
    DocumentDecorations {
        uri: String,
        version: i32,
        widgets: Vec<WidgetDescriptor>,
        inline_viz_anchors: Vec<InlineVizAnchor>,
        metadata: DocMetadata,
    },
    /// Pushed on evaluation flash.
    EvalFlash {
        uri: String,
        range: LspRange,
        duration_ms: u32,
    },
    /// Pushed when a viz frame is ready for external Webview/panel rendering.
    VizFrameReady {
        target: VizTarget,
        commands: Vec<DrawCmd>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32, // UTF-16 by default, negotiable to UTF-8 via LSP positionEncoding
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}
```

### 2.4 `wasm32` Browser PWA Frontend (`CodeMirror 6` + WebAudio + Canvas)

The browser target compiles `silverwave-gui` / `silverwave-wasm` and `silverwave-viz` into a single `wasm32-unknown-unknown` module loaded by a Progressive Web App (PWA) shell:

1. **CodeMirror 6 Integration (`@silverwave/codemirror`)**:

   * `StateField<DecorationSet>` subscribes to `ActiveHighlights` via `requestAnimationFrame`.

   * `ViewPlugin` intercepts `Ctrl+Enter` / `Mod+Enter` (eval) and `Ctrl+.` / `Mod+.` (halt), dispatching into WASM without blocking the UI thread.

   * `WidgetType` subclasses render HTML `<input type="range">` inline sliders at exact `Utf16Span` offsets.

   * Inline `<canvas>` block widgets (`Decoration.widget({ block: true })`) render `_`-prefixed visualizers directly below the line containing the `_viz()` call.

2. **WebAudio `AudioWorklet`**:

   * Audio synthesis executes inside a dedicated `AudioWorkletProcessor` backed by a `SharedArrayBuffer` (or fallback `MessagePort` ring buffer when COOP/COEP headers are absent) feeding `AudioTap` for `scope` and `spectrum`.

3. **Offline Caching (Service Worker PWA)**:

   * Pre-caches `index.html`, `silverwave_bg.wasm`, CodeMirror 6 bundle, built-in WebGL/WebGPU shaders, and default sample banks via Cache Storage API (`CacheFirst` for immutable hashed assets, `StaleWhileRevalidate` for sample manifests).

   * Stores unsaved user sketches and `@title` history in `IndexedDB` so offline live-coding sessions survive reloads without data loss.

#### TypeScript / `wasm-bindgen` Contract

```ts
export interface SilverwaveWasmHandle {
  evaluate(code: string, blockRange?: [number, number]): EvalResult;
  halt(): void;
  setCps(cps: number): void;
  updateSlider(widgetId: number, value: number): SliderPatch | null;
  tickFrame(nowMs: number): FrameUpdate;
  encodeShareUrl(code: string): string;
  decodeShareUrl(hash: string): string;
  exportAudio(format: "wav" | "flac", cycles: number): Uint8Array;
}

export interface FrameUpdate {
  highlights: Array<{ fromUtf16: number; toUtf16: number; color: string; markCss?: string }>;
  globalViz: DrawCmd[];
  inlineViz: Array<{ anchorLine: number; vizId: number; commands: DrawCmd[] }>;
  hydraUniforms: HydraUniformFrame | null;
}
```

## 3. 1:1 Global & Inline Visualizers (`silverwave-viz`)

All visualizers are pure functions from `(PatternSnapshot, AudioTap, CycleTime, Viewport)` to a platform-agnostic vector command stream `Vec<DrawCmd>`.

### 3.1 Global (`fn()`) vs Inline (`_fn()`) & `all(viz)` Dispatch Model

1. **Global Visualizers** (`pianoroll()`, `punchcard()`, `spiral()`, `scope()`, `pitchwheel()`, `spectrum()`):

   * Render onto the full-viewport background canvas behind the code editor.

   * `all(pianoroll)` (or `all(punchcard)`, etc.) attaches the visualizer to the composite output of all currently active patterns.

2. **Inline Visualizers** (`_pianoroll()`, `_punchcard()`, `_spiral()`, `_scope()`, `_pitchwheel()`, `_spectrum()`):

   * Prefixing with `_` binds the visualizer to the specific pattern expression where it is chained and anchors its output widget directly below that expression's source line (`InlineVizAnchor`).

   * Multiple inline visualizers may coexist simultaneously in a single document, each receiving an independent `VizTarget::Inline { id, anchor_line }` and independent query/audio tap context.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VizKind {
    Pianoroll,
    Punchcard,
    Spiral,
    Scope,
    Pitchwheel,
    Spectrum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VizTarget {
    Global { kind: VizKind },
    Inline { id: u32, kind: VizKind, anchor_line: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VizViewport {
    pub width: f32,
    pub height: f32,
    pub dpi_scale: f32,
    pub theme_fg: RgbaColor,
    pub theme_gutter_fg: RgbaColor,
    pub theme_bg: RgbaColor,
}
```

### 3.2 Draw Command AST (`DrawCmd`)

Every frontend (Canvas 2D, `egui::Painter`, `wgpu` instanced renderer, TUI Braille/Kitty, or SVG) consumes the same deterministic `DrawCmd` stream:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Clear { color: RgbaColor },
    FillRect { x: f32, y: f32, w: f32, h: f32, color: RgbaColor },
    StrokeRect { x: f32, y: f32, w: f32, h: f32, thickness: f32, color: RgbaColor },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, cap: LineCap, color: RgbaColor },
    Polyline { points: Vec<(f32, f32)>, thickness: f32, cap: LineCap, color: RgbaColor, closed: bool },
    Circle { cx: f32, cy: f32, radius: f32, fill: Option<RgbaColor>, stroke: Option<(f32, RgbaColor)> },
    Arc { cx: f32, cy: f32, radius: f32, start_rad: f32, end_rad: f32, thickness: f32, cap: LineCap, color: RgbaColor },
    Text { x: f32, y: f32, content: String, font_family: String, size_px: f32, color: RgbaColor },
}
```

### 3.3 `pianoroll` / `_pianoroll` & `punchcard` / `_punchcard`

**Semantic Distinction (1:1 Strudel Parity):**

* `pianoroll` / `_pianoroll`: Captures and queries the pattern **at the exact point in the method chain** where `.pianoroll()` is called; subsequent chain modifiers (e.g., `.color("cyan")`, `.fast(2)`) do **not** affect the rendered roll.

* `punchcard` / `_punchcard`: Reads from the **post-transform** hap stream (the same evaluated hap stream used for mini-notation token highlighting), so transformations applied *after* `.punchcard()` (such as `.color("cyan")` or `.transpose(12)`) **are** reflected in the visual.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PianorollOptions {
    pub cycles: u32,               // default: 4
    pub playhead: f32,             // 0.0..=1.0, default: 0.5
    pub vertical: bool,            // default: false
    pub labels: bool,              // default: false
    pub flip_time: bool,           // default: false
    pub flip_values: bool,         // default: false
    pub overscan: f32,             // default: 1.0
    pub hide_negative: bool,       // default: false
    pub smear: bool,               // default: false
    pub fold: bool,                // default: false
    pub active: RgbaColor,         // default: #FFCA28
    pub inactive: RgbaColor,       // default: #7491D2
    pub background: RgbaColor,     // default: transparent (0,0,0,0)
    pub playhead_color: RgbaColor, // default: #FFFFFF
    pub fill: bool,                // default: false
    pub fill_active: bool,         // default: false
    pub stroke: bool,              // default: false
    pub stroke_active: bool,       // default: false
    pub hide_inactive: bool,       // default: false
    pub colorize_inactive: bool,   // default: true
    pub font_family: String,       // default: "monospace"
    pub min_midi: i32,             // default: 10
    pub max_midi: i32,             // default: 90
    pub autorange: bool,           // default: false
}

pub trait PianorollRenderer {
    fn render_pianoroll(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}
```

### 3.4 `spiral` / `_spiral`

Renders pattern events along an Archimedean or logarithmic spiral.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SpiralOptions {
    pub stretch: f32,                 // rotations per cycle, default: 1.0
    pub size: Option<f32>,            // diameter in px; None = fit viewport
    pub thickness: f32,               // default: 3.0
    pub cap: LineCap,                 // default: LineCap::Butt
    pub inset: f32,                   // rotations before spiral starts, default: 3.0
    pub playhead_color: RgbaColor,    // default: #FFFFFF
    pub playhead_length: f32,         // in rotations, default: 0.02
    pub playhead_thickness: Option<f32>, // defaults to `thickness`
    pub padding: f32,                 // default: 0.0
    pub steady: f32,                  // 0.0..=1.0 (1.0 = spiral static, playhead moves), default: 0.0
    pub active_color: Option<RgbaColor>,   // defaults to viewport.theme_fg
    pub inactive_color: Option<RgbaColor>, // defaults to viewport.theme_gutter_fg
    pub colorize_inactive: bool,      // default: false
    pub fade: bool,                   // default: true
    pub log_spiral: bool,             // default: false
}

pub trait SpiralRenderer {
    fn render_spiral(
        haps: &[VizHap],
        now: CycleTime,
        opts: &SpiralOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}
```

### 3.5 `scope` / `_scope` (Synonym: `tscope`)

Renders a real-time time-domain oscilloscope from `AudioTap::read_time_domain`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeOptions {
    pub align: bool,       // align to first zero/trigger crossing, default: true
    pub color: RgbaColor,  // default: #FFFFFF
    pub thickness: f32,    // default: 3.0
    pub scale: f32,        // y-axis scale, default: 0.25
    pub pos: f32,          // 0.0 (top) ..= 1.0 (bottom), default: 0.5
    pub trigger: f32,      // amplitude threshold for alignment, default: 0.0
}

pub trait ScopeRenderer {
    fn render_scope(
        samples: &[f32],
        opts: &ScopeOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}
```

### 3.6 `pitchwheel` / `_pitchwheel`

Renders a circular pitch-class / microtonal wheel visualizing active and upcoming frequencies within one octave (supporting arbitrary `edo` divisions).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchwheelMode {
    Radians,
    Degrees,
    Steps,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PitchwheelOptions {
    pub hapcircles: u32,      // default: 1
    pub circle: u32,          // default: 1
    pub edo: u32,             // equal divisions of the octave, default: 12
    pub root: String,         // root pitch class, default: "C"
    pub thickness: f32,       // default: 2.0
    pub hap_radius: f32,      // default: 5.0
    pub mode: PitchwheelMode, // default: PitchwheelMode::Steps
    pub margin: f32,          // default: 10.0
}

pub trait PitchwheelRenderer {
    fn render_pitchwheel(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PitchwheelOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}
```

### 3.7 `spectrum` / `_spectrum`

Renders a scrolling spectrogram or frequency-domain analyzer from `AudioTap::read_spectrum_db`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumOptions {
    pub thickness: u32, // line thickness in px, default: 3
    pub speed: u32,     // scroll speed in px/frame, default: 1
    pub min: i32,       // min dB, default: -80
    pub max: i32,       // max dB, default: 0
}

pub trait SpectrumRenderer {
    fn render_spectrum(
        bins_db: &[f32],
        sample_rate: u32,
        opts: &SpectrumOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}
```

## 4. Live-Coding UX & Interactive Controls (`silverwave-gui` / `silverwave-wasm`)

### 4.1 Evaluation & Transport Keybindings

| Shortcut (Desktop / Browser) | TidalCycles / Strudel Action | Contract Behavior | 
| ----- | ----- | ----- | 
| `Ctrl+Enter` / `Cmd+Enter` | Evaluate entire buffer (Strudel) or current stanza/block (Tidal) | Parses `DocMetadata`, extracts `WidgetDescriptor`s, transpiles/evaluates pattern, flashes evaluated `ByteSpan` for `EvalFlash` duration (default `200ms`), and hot-swaps active `Pattern` on next scheduler boundary without audio clicks. | 
| `Shift+Enter` | Evaluate current single line or paragraph block | Isolates enclosing blank-line-delimited stanza (or single line) and evaluates into targeted slot (`d1`..`d16` or active expression). | 
| `Ctrl+.` / `Cmd+.` | Halt (`hush()`) | Immediately stops scheduler queries, emits MIDI All-Notes-Off (`CC 123`), silences active synth voices, and clears `ActiveHighlights`. | 
| `Ctrl+Alt+Enter` | Mute/Unmute toggle | Preserves scheduler phase while muting audio output. | 

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ReplCommand {
    EvaluateBuffer { code: String },
    EvaluateBlock { code: String, block_span: ByteSpan },
    Halt,
    SetCps(f64),
    DragSlider { widget_id: WidgetId, new_value: f64 },
    ExportAudio { format: AudioExportFormat, cycles: f64 },
}
```

### 4.2 Adaptive Byte-Span Token Highlighting & UTF-8 / UTF-16 Mapping

Pattern mini-notation strings (`"bd sd [hh hh]"` or `` `c3 e3 g3` ``) carry `SourceLocation` metadata attached to each `Hap` during parsing. Because Rust stores strings as UTF-8 (`ByteOffset`) while CodeMirror 6, VSCode, Pulsar, and LSP default to UTF-16 (`Utf16Offset`), `silverwave-gui` / `silverwave-wasm` maintains an immutable `SpanMap` per evaluated document version.

1. **Multi-byte & Emoji Resilience**: `SpanMap` builds a prefix index of line-starts and non-ASCII byte-to-UTF-16 deltas in `O(N)` on evaluation, enabling `O(log L)` translation between `(ByteOffset)`, `(Utf16Offset)`, and `(line, character)` coordinates.

2. **Adaptive Remapping on Live Edits**: When the user types *before* re-evaluating (`Ctrl+Enter`), active playing haps still reference the *previous* evaluated `ByteSpan`s. `SpanMap::apply_edit(edit: TextEdit)` shifts downstream spans by `delta_bytes` and marks overlapping spans as `Stale` (or clips them cleanly) so highlights never drift onto unrelated tokens or panic on out-of-bounds indices.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEdit {
    pub span: ByteSpan,
    pub new_len_bytes: usize,
    pub new_len_utf16: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanValidity {
    Exact,
    Shifted,
    Clipped,
    Invalidated,
}

pub trait SpanMapper {
    fn byte_to_utf16(&self, byte: ByteOffset) -> Option<Utf16Offset>;
    fn utf16_to_byte(&self, utf16: Utf16Offset) -> Option<ByteOffset>;
    fn byte_span_to_lsp_range(&self, span: ByteSpan) -> Option<LspRange>;
    fn apply_edit(&mut self, edit: TextEdit);
    fn remap_span(&self, original: ByteSpan) -> (Option<ByteSpan>, SpanValidity);
}
```

### 4.3 Inline Code Widgets: `slider(val, min, max, step)`

Achieves 1:1 parity with `@strudel/codemirror`'s `slider.mjs`:

1. **Evaluation Phase**: Calls to `slider(val, min, max, step)` (or `sliderWithID(id, val, min, max, step)`) register a `WidgetDescriptor` in `WidgetRegistry` and bind the pattern parameter to an atomic/shared `RefValue` slot rather than a static literal.

2. **Drag Phase (Zero Re-Evaluation)**: Dragging the UI slider updates the `RefValue` slot directly in memory—the running pattern reads the updated value on every subsequent scheduler query tick **without** triggering a full code re-evaluation or audio glitch.

3. **In-Place Source Mutation**: Simultaneously, `WidgetRegistry::commit_slider_drag` computes a `SliderPatch` containing the exact `ByteSpan` (and `Utf16Span`) of the numeric `val` literal in the editor buffer and the formatted replacement string, updating the source text in-place and shifting `SpanMap` by the byte-length delta.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetDescriptor {
    pub id: WidgetId,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub literal_span: ByteSpan,
    pub call_span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SliderPatch {
    pub widget_id: WidgetId,
    pub replace_span: ByteSpan,
    pub replace_utf16_span: Utf16Span,
    pub replacement_text: String,
    pub clamped_value: f64,
}
```

### 4.4 Patterned `.color()` & `.markcss()` Styling Contract

1. **`.color(pattern)`**: Accepts static CSS color names/hex/RGB strings or patterned color sequences (e.g., `.color("cyan magenta")`). Each active `Hap` carries its resolved `RgbaColor` into both `ActiveHighlight` (token outline/background) and `VizHap` (for `pianoroll`, `punchcard`, `spiral`).

2. **`.markcss(css_pattern)`**: Overrides the default token highlight style (`outline: solid 2px <color>`).

   * **Browser / Webview**: Sanitized against an allowlist of safe CSS visual properties (`outline`, `border`, `background`, `color`, `text-decoration`, `box-shadow`, `opacity`, `border-radius`)—disallowing `url(...)`, `expression(...)`, or layout-breaking properties (`position`, `display`, `width`, `height`).

   * **Native `egui` / TUI**: Parsed into `NativeHighlightStyle` (`stroke_width`, `stroke_color`, `fill_color`, `underline`, `text_color`) so `.markcss('text-decoration:underline')` and `.markcss('background:red')` render identically on desktop and terminal.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightStyle {
    pub color: RgbaColor,
    pub mark_css_raw: Option<String>,
    pub native: NativeHighlightStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeHighlightStyle {
    pub outline: Option<(f32, RgbaColor)>,
    pub background: Option<RgbaColor>,
    pub foreground: Option<RgbaColor>,
    pub underline: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveHighlight {
    pub span: ByteSpan,
    pub utf16_span: Utf16Span,
    pub style: HighlightStyle,
}
```

### 4.5 Metadata Directives (`@title`, `@by`, `@genre`)

Line comments at the top of a sketch (or anywhere in the header block) using `// @key value` or `/* @key value */` are extracted into `DocMetadata` prior to evaluation:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocMetadata {
    pub title: Option<String>,
    pub by: Option<String>,
    pub genre: Option<String>,
    pub license: Option<String>,
    pub details: Option<String>,
    pub extra: Vec<(String, String)>,
}

pub trait MetadataParser {
    fn parse_metadata(source: &str) -> DocMetadata;
}
```

### 4.6 Base64 URL Sharing & State Serialization

`ShareCodec` provides deterministic round-trip encoding for `#code=<payload>` URL fragments (compatible with Strudel's URL sharing workflow):

* Encodes UTF-8 source text using URL-safe Base64 (`RFC 4648 §5`, no padding `=`) with optional Deflate prefix tag (`z:`) when compressed length is strictly smaller than raw Base64.

* Rejects malformed or oversized payloads (`> 1 MiB` decompressed) with `ReplError::ShareDecodeError` to prevent zip-bomb DoS in the browser or LSP daemon.

```rust
pub trait ShareCodec {
    fn encode_url_fragment(source: &str) -> String;
    fn decode_url_fragment(fragment: &str) -> Result<String, ReplError>;
}
```

### 4.7 Offline Deterministic Audio Export (WAV / FLAC)

`AudioExporter` steps the scheduler deterministically from cycle `0.0` to `cycles` at a fixed sample rate (`44100`, `48000`, or `96000` Hz) without real-time wall-clock jitter:

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
pub struct ExportConfig {
    pub format: AudioExportFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub cycles: f64,
    pub normalize_peak_db: Option<f32>,
}

pub trait AudioExporter {
    fn export_bytes(&self, config: &ExportConfig) -> Result<Vec<u8>, ReplError>;
}
```

### 4.8 Hydra & Shader Visual Bridge (`initHydra`)

Achieves 1:1 parity with [`@strudel/hydra`](https://strudel.cc/learn/hydra/):

1. **`initHydra(options)`**:

   * `detectAudio: bool` — Routes Silverwave's master audio output into Hydra's `a.fft[0..3]` / `a.setBins()` analyzer.

   * `feedStrudel: u8` — Feeds the active Strudel/Silverwave pattern canvas (such as `pianoroll` or `scope`) into Hydra source slot `s0` (`s0.init({ src: canvas })`).

   * **Native `wgpu` Mode**: Maps the same `HydraUniformFrame` (`fft_bins: [f32; 4]`, `time`, `cycle`, `active_hap_count`, `pattern_params`) into a `wgpu` WGSL uniform buffer (`@group(0) @binding(0)`), allowing live-coded WGSL/GLSL shaders to run at 60+ FPS on desktop without a browser.

   * **Multi-Monitor / Detached Output**: Supports opening a secondary borderless `wgpu` window (native) or `BroadcastChannel` popup window (browser PWA) receiving `HydraUniformFrame` and `DrawCmd` streams at frame rate.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct HydraInitOptions {
    pub detect_audio: bool,
    pub feed_strudel: Option<u8>, // e.g., Some(1) -> s0
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub num_outputs: u8,          // default: 4 (o0..o3)
    pub num_sources: u8,          // default: 4 (s0..s3)
    pub detached_window: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HydraUniformFrame {
    pub time_sec: f32,
    pub cycle: f32,
    pub cps: f32,
    pub active_hap_count: u32,
    pub fft_bins: [f32; 4],
    pub rms_level: f32,
    pub peak_level: f32,
    pub _padding: [f32; 2],
}
```

## 5. TDD Contracts: UI State, Byte-Span Remapping & Deterministic Render Tests

All tests in `silverwave-gui` / `silverwave-wasm` and `silverwave-viz` MUST run deterministically without a live audio device, GPU driver, or DOM.

### 5.1 UI State Machine & Transport Contracts

| Test ID | Contract / Invariant | Verification Assertion | 
| ----- | ----- | ----- | 
| `UI-01` | Initial state is `Stopped`, `cps == 0.5`, `active_highlights.is_empty()` | Assert default `ReplEngine::new()` state matches Strudel v1.0+ `0.5` CPS default. | 
| `UI-02` | `EvaluateBuffer` with valid code transitions `Stopped -> Playing` and emits `EvalFlash` | Assert `EvalResult::Ok`, `is_playing() == true`, and `EvalFlash.range` covers evaluated span. | 
| `UI-03` | `EvaluateBuffer` with syntax error while `Playing` preserves **previous** playing pattern | Assert `ReplError::ParseError` is returned, `is_playing()` remains `true`, and audio continues uninterrupted. | 
| `UI-04` | `Halt` (`Ctrl+.`) transitions `Playing -> Stopped`, clears `ActiveHighlights`, emits All-Notes-Off | Assert `active_highlights.is_empty()` and transport queue receives `AllNotesOff`. | 
| `UI-05` | `MetadataParser` extracts `@title`, `@by`, `@genre` regardless of whitespace or `/* */` vs `//` | Assert round-trip extraction across CRLF/LF and Unicode values (e.g., `"Café — José"`). | 
| `UI-06` | `ShareCodec` round-trips arbitrary UTF-8 code including emojis and backticks | Assert `decode(encode(s)) == Ok(s)` and malformed/oversized payloads return `ShareDecodeError`. | 

### 5.2 Byte-Span Remapping & UTF-8 / UTF-16 Invariants

| Test ID | Contract / Invariant | Verification Assertion | 
| ----- | ----- | ----- | 
| `SPAN-01` | ASCII-only document: `byte_to_utf16(b) == Some(b)` for all `0..=len` | Assert 1:1 identity mapping across multi-line ASCII buffers. | 
| `SPAN-02` | Multi-byte UTF-8 (2-byte `é`, 3-byte `中`, 4-byte `🌀` = 2 UTF-16 code units) | Assert `"🌀 note(\"c3\")"` maps `note` start from byte `5` to UTF-16 offset `3`. | 
| `SPAN-03` | Mid-codepoint byte offset never panics | Assert `byte_to_utf16` returns `None` (or snaps safely per API contract) when given interior UTF-8 byte. | 
| `SPAN-04` | Insertion *before* a tracked `ByteSpan` shifts `start` and `end` by `+delta` | Assert `remap_span` returns `(Some(shifted), SpanValidity::Shifted)`. | 
| `SPAN-05` | Deletion *overlapping* the start or end of a tracked `ByteSpan` clips cleanly | Assert `remap_span` returns `(Some(clipped), SpanValidity::Clipped)` with `clipped.start <= clipped.end`. | 
| `SPAN-06` | Complete deletion of a tracked `ByteSpan` invalidates without panicking | Assert `remap_span` returns `(None, SpanValidity::Invalidated)`. | 

### 5.3 Inline Slider & Widget Mutation Contracts

| Test ID | Contract / Invariant | Verification Assertion | 
| ----- | ----- | ----- | 
| `SLIDER-01` | `slider(0.5, 0, 1, 0.01)` registers `WidgetDescriptor` with exact `literal_span` covering `"0.5"` | Assert `literal_span` slices to `"0.5"` and `RefValue` initializes to `0.5`. | 
| `SLIDER-02` | `DragSlider` clamps to `[min, max]` and snaps to `step` | Assert dragging `1.5` on `[0, 1]` yields `1.0`; dragging `0.333` with `step=0.1` yields `0.3`. | 
| `SLIDER-03` | `DragSlider` does **not** increment `eval_generation` | Assert `eval_generation` counter is unchanged while `RefValue` reflects new value immediately. | 
| `SLIDER-04` | `SliderPatch` formatting preserves valid numeric syntax and updates `SpanMap` | Assert applying `SliderPatch` to `"0.5"` -> `"0.75"` shifts downstream spans by `+1` byte. | 
| `SLIDER-05` | Multiple sliders on the same line remain aligned after dragging the first slider | Assert dragging slider #1 from `"0"` to `"0.25"` shifts slider #2's `literal_span` by `+3` bytes. | 

### 5.4 Deterministic Render & Golden Command Contracts

| Test ID | Contract / Invariant | Verification Assertion | 
| ----- | ----- | ----- | 
| `VIZ-01` | `pianoroll` vs `punchcard` transform-order parity | Given `note("c4")._<viz>().color("cyan")`, assert `punchcard` emits `cyan` commands while `pianoroll` emits default `active`/`inactive` colors. | 
| `VIZ-02` | `pianoroll` `autorange: true` tightens `[min_midi, max_midi]` to observed hap range | Assert all emitted note `y` coordinates fall strictly within `(0.0, viewport.height)`. | 
| `VIZ-03` | `spiral` `steady: 1.0` keeps spiral geometry invariant across cycle ticks | Assert spiral `Arc`/`Polyline` vertices are bitwise identical at `t = 0.0` and `t = 0.5`, while only playhead rotates. | 
| `VIZ-04` | `scope` zero-crossing alignment (`align: true, trigger: 0.0`) | Given a phase-shifted sine wave, assert first rendered `Polyline` point is near `y = pos * height` on rising edge. | 
| `VIZ-05` | `pitchwheel` `edo: 12` vs `edo: 19` angular placement | Assert `root` note maps to top (`-PI/2` rad) and octave intervals wrap modulo `edo`. | 
| `VIZ-06` | `spectrum` clamps out-of-range dBFS values to `[min, max]` | Assert `-140 dB` clamps to `min` (`-80`) and `+6 dB` clamps to `max` (`0`) without NaN/Inf. | 
| `VIZ-07` | Golden `DrawCmd` determinism | Assert identical `(haps, audio_slice, now, opts, viewport)` inputs produce identical `Vec<DrawCmd>` across `x86_64` and `wasm32`. | 

### 5.5 Editor Protocol & PWA Offline Cache Contracts

| Test ID | Contract / Invariant | Verification Assertion | 
| ----- | ----- | ----- | 
| `LSP-01` | JSON-RPC round-trip for `Eval` -> `EvalFlash` + `DocumentDecorations` + `ActiveHighlights` | Assert all responses validate against schema and respect negotiated UTF-8/UTF-16 `positionEncoding`. | 
| `LSP-02` | `Halt` cancels pending `ActiveHighlights` and emits empty `spans: []` | Assert no stale highlight frames are emitted after `Halt` is processed. | 
| `EXP-01` | Deterministic WAV/FLAC export produces valid RIFF/fLaC headers and exact frame count | Assert `frame_count == (cycles / cps * sample_rate).round()` and decoding FLAC matches PCM32f/24 source losslessly. | 
| `PWA-01` | Offline cache manifest contains all `wasm32` & shader assets; `IndexedDB` restores last sketch | Assert `evaluate()` succeeds in simulated offline mode (`navigator.onLine == false`). |

---

## 6. Category-King UI, Tracker Grid, Modulation Rings & VJ Extensions (`CK-02`, `CK-04`, `CK-07`, `CK-09`, `CK-10`)

1. **Bidirectional Tracker / Phrase Grid View (`TrackerPhraseView`)**: Toggleable vertical `NOTE | INST | VOL | FX1 | FX2 | FX3` spreadsheet view and 2D **Pattern Matrix** synchronized 1:1 with the pattern AST via `SourceSpan` edits, plus click-hold step parameter-lock (`plock`) editing on the 16/64-step LED trig bar.
2. **[Bitwig-Style](https://www.bitwig.com/userguide/latest/modulator/) Animated `ModulationRing` Overlays**: Renders real-time 60 FPS modulation arcs around inline `slider()` and knob widgets showing live `k-rate`/`a-rate` signal excursions.
3. **Docked 5-Stage `InteractiveTour`**: In-REPL guided onboarding tour ([Sonic Pi-style "First Beat in 30 Seconds"](https://sonic-pi.net/tutorial.html)) with live diff validation and inline `._punchcard()` feedback.
4. **Dual-Cursor Co-Performance `ActorBadge`**: Displays distinct cursor badges (`[HUMAN: Performer]` vs `[MCP AGENT]`) with surgical AST patch highlights during human + AI B2B sessions.
5. **Native Syphon (macOS), Spout (Windows) & NDI Output**: Zero-copy GPU texture sharing from `silverwave-viz` to external VJ hosts (Resolume, TouchDesigner, VDMX) with configurable audio-visual frame delay compensation (`-200 ms..+200 ms`).
