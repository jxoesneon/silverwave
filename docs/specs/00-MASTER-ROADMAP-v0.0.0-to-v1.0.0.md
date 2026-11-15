# SilverWave Master Roadmap & Architecture Specification (`v0.0.0` → `v1.0.0`)

**Document ID:** `00-MASTER-ROADMAP-v0.0.0-to-v1.0.0`  
**Status:** Canonical Specification (DocDD / TDD Contract)  
**Target Parity:** 1:1 Union of [Strudel](https://codeberg.org/uzu/strudel) and [TidalCycles](https://github.com/tidalcycles/tidal)  
**Language & Runtime:** Pure Rust (`edition = "2024"`, `#![forbid(unsafe_op_in_unsafe_fn)]`, `no_std`-compatible core)

---

## 1. Project Vision & Workspace Crate Layout

### 1.1 Vision
SilverWave is a deterministic, zero-allocation-audio, Pure-Rust live-coding pattern and synthesis engine that unifies the browser-first ergonomics, visual feedback, and tonal/visual bridges of [Strudel](https://strudel.cc/technical-manual/repl/) with the algebraic pattern expressiveness, SuperDirt DSP semantics, multi-stream transitions, and hardware/network synchronization of [TidalCycles](https://tidalcycles.org/docs/reference/pattern_structure/).

Core architectural principles:
1. **Exact Rational Time:** All pattern queries operate over exact `Rational` (`num/den` `i128`/`i64`) time spans (`TimeSpan`), eliminating floating-point drift across arbitrarily long live sets.
2. **Pure Functional Pattern Algebra:** `Pattern<T>` is a pure function `TimeSpan -> Vec<Hap<T>>` supporting Functor, Applicative (`appLeft`, `appRight`, `appBoth`), and Monad (`innerJoin`, `outerJoin`, `squeezeJoin`) semantics.
3. **Hard Real-Time Audio Separation:** The audio callback thread never allocates, never locks a mutex, and never performs I/O; pattern evaluation and sample decoding occur on dedicated non-RT threads communicating via lock-free ring buffers.
4. **Universal Compilation Targets:** Native desktop/headless (`Linux`, `macOS`, `Windows`), browser WASM + AudioWorklet PWA, and DAW plugin formats (`VST3`, `CLAP`) from a single workspace.

### 1.2 Workspace Crate Dependency Graph

```text
                                      +--------------------+
                                      |   silverwave-cli   |
                                      +---------+----------+
                                                |
    +-------------------+-------------------+---+---------------+-------------------+
    |                   |                   |                   |                   |
    v                   v                   v                   v                   v
+---+--------------+ +--+---------------+ +-+----------------+ ++-----------------+ +-------------------+
|  silverwave-gui  | | silverwave-wasm  | |  silverwave-lsp  | |  silverwave-mcp  | | silverwave-plugin |
+---+--------------+ +--+---------------+ +-+----------------+ ++-----------------+ +-------------------+
    |                   |                   |                   |                   |
    +-------------------+-------------------+---+---------------+-------------------+
                                                |
                        +-----------------------+-----------------------+
                        |                                               |
                        v                                               v
                +-------+-----------+                           +-------+-----------+
                |   silverwave-io   |                           |  silverwave-viz   |
                +-------+-----------+                           +-------+-----------+
                        |                                               |
                        +-----------+-----------------------+-----------+
                                    |                       |
                                    v                       v
                            +-------+-----------+   +-------+-----------+
                            |  silverwave-mini  |   |  silverwave-dsp   |
                            +-------+-----------+   +-------+-----------+
                                    |                       |
                                    +-----------+-----------+
                                                |
                                                v
                                        +-------+-----------+
                                        |  silverwave-core  |
                                        +-------------------+
```

### 1.3 Workspace Crate Schemas

| Crate | `no_std` | Primary Responsibility | Key Public Types / Modules (Schema Only) | Forbidden Dependencies |
| :--- | :--- | :--- | :--- | :--- |
| **`silverwave-core`** | Yes (`alloc`) | Exact rational time, `TimeSpan`, `Hap<T>`, `Pattern<T>`, algebraic combinators, continuous signals, PRNG, tonal & microtonal theory. | `Rational`, `TimeSpan`, `Context`, `SourceLoc`, `Hap<T>`, `Pattern<T>`, `ControlMap`, `Value`, `Signal`, `Scale`, `Voicing`, `ScalaTuning` | `std::net`, `std::fs`, audio drivers, UI frameworks |
| **`silverwave-mini`** | Yes (`alloc`) | Tidal/Strudel Mini-Notation parser, Mondo notation parser, AST, and source-span (`withLoc`) preservation for live token highlighting. | `MiniParser`, `MondoParser`, `MiniAst`, `SpanMap`, `ParseError`, `EvalContext` | Audio drivers, networking, `silverwave-dsp` |
| **`silverwave-dsp`** | Core: Yes; Loaders: `std` | Lock-free synthesis graph, SuperDirt-parity synths & effects, orbit buses, sidechain/ducking, sample bank & SF2 SoundFont decoding, `cpal` / AudioWorklet backends. | `AudioEngine`, `DspGraph`, `OrbitBus`, `VoiceAllocator`, `SynthNode`, `FxChain`, `SampleBank`, `SoundFontBank`, `RtRingBuffer` | Pattern parser, REPL, UI frameworks |
| **`silverwave-io`** | `std` | Multi-stream scheduler (`d1`–`d16`), clock, Ableton Link v3, OSC (SuperDirt & generic), MIDI I/O + MPE + Clock, transitions, HID/Gamepad/Motion. | `Scheduler`, `StreamRegistry`, `Transition`, `ClockSource`, `LinkSession`, `OscBridge`, `MidiRouter`, `InputMapper` | UI rendering, WASM DOM |
| **`silverwave-viz`** | `std` / `wasm32` | Real-time `wgpu` / Canvas2D visualization engines (`_pianoroll`, `_scope`, `_spectrum`, `_punchcard`, `_spiral`, `_pitchwheel`), Hydra bridge, and Syphon/Spout/NDI output. | `VizPipeline`, `Pianoroll`, `Oscilloscope`, `SpectrumAnalyzer`, `Punchcard`, `Spiral`, `HydraBridge`, `RenderFrame` | Pattern parsing, file system I/O |
| **`silverwave-gui`** | `std` | Native desktop GPU code editor, bidirectional Tracker/Phrase grid, 16/64-step `plock` bar, inline `ModulationRing` sliders, and docked interactive tour. | `ReplSession`, `Evaluator`, `TrackerPhraseView`, `ModulationRing`, `InteractiveTour` | Direct OS audio hardware calls |
| **`silverwave-wasm`** | `wasm32` | Browser WebAssembly `AudioWorklet` + WebGPU/Canvas2D PWA target (`< 1.8 MB` gzipped core + content-addressed `15 MB` IndexedDB starter pack). | `WasmAudioWorklet`, `WasmReplSession`, `ProgressiveSampleCache` | Native OS windowing |
| **`silverwave-lsp`** | `std` | Language Server Protocol ([LSP 3.17](https://microsoft.github.io/language-server-protocol/)) daemon for VS Code, Neovim, Helix, Emacs, and Zed with real-time active `SourceSpan` decorations. | `SilverWaveLspServer`, `ActiveSpanDecoration`, `DiagnosticEngine` | Direct OS audio hardware calls |
| **`silverwave-mcp`** | `std` | [Model Context Protocol (MCP)](https://modelcontextprotocol.io/specification/2026-07-28) server (`stdio` & Streamable HTTP+SSE) exposing 42 tools, 10 live resources, 5 prompts, and `AcousticMixDiagnostic` telemetry. | `SilverWaveMcpToolSurface`, `CoPerformanceBridge`, `AcousticMixDiagnostic` | Direct audio-thread locks |
| **`silverwave-plugin`** | `std` | Native 16-stereo-bus (32-channel) **CLAP** and **VST3** DAW instrument/FX plugin via [`nih-plug`](https://github.com/robbert-vdh/nih-plug) with sample-accurate host transport sync. | `DawPluginConfig`, `SilverWaveClapPlugin`, `SilverWaveVst3Plugin` | Blocking I/O in `process()` |
| **`silverwave-cli`** | `std` | Unified 14-subcommand binary entrypoint, headless daemon, TUI REPL, offline WAV/FLAC/MIDI & 60 FPS video renderer (`--video`), and plugin bundler. | `CliArgs`, `DaemonRunner`, `TuiApp`, `OfflineRenderer`, `VideoMuxer` | N/A (top-level binary crate) |

---

## 2. Complete Milestone Roadmap (`v0.0.0` → `v1.0.0`)

### `v0.0.0` — Finalized Specs & TDD Contracts
* **Scope:** Complete architectural specifications, algebraic laws, mini-notation grammar specification, SuperDirt OSC/DSP parameter contracts, and failing test suites (Red phase) across all 11 crates.
* **Crate Targets:** Workspace root + all `docs/specs/` and `tests/` contract harnesses.
* **Key Deliverables:**
  * Formal `Rational` arithmetic & `Pattern<T>` Functor/Applicative/Monad property-test specifications.
  * Complete Mini-Notation & Mondo EBNF/PEG grammar specification with `SourceLoc` offset rules.
  * SuperDirt control parameter schema (`ControlMap` canonical keys, units, ranges, defaults).
  * CI pipeline enforcing DocDD, `cargo-llvm-cov`, `cargo-mutants`, and `assert_no_alloc` tripwires.
* **Exit Gate:** 100% of specification documents signed off; all contract test suites compile and fail deterministically (0 passing stubs, 0 skipped tests).

### `v0.1.0` — Core Pattern Engine
* **Scope:** Pure-Rust implementation of `silverwave-core` passing all algebraic, temporal, randomness, and tonal property tests.
* **Crate Targets:** `silverwave-core`
* **Key Deliverables:**
  * Exact `Rational` and `TimeSpan` splitting (`splitQueries`, `whole`/`part` hap semantics).
  * Core constructors (`pure`, `silence`, `stack`, `cat`, `fastcat`, `timeCat`, `stepcat`, `run`, `scan`).
  * Time modifiers (`fast`, `slow`, `early`, `late`, `rev`, `iter`, `ply`, `compress`, `zoom`, `linger`, `swingBy`, `euclid`, `step`, `expand`, `pace`).
  * Higher-order combinators (`every`, `whenmod`, `sometimes`, `degradeBy`, `chunk`, `inside`, `outside`, `within`, `off`, `jux`, `superimpose`, `stut`, `echo`, `striate`, `chop`, `slice`, `splice`).
  * Continuous signals (`sine`, `cosine`, `saw`, `tri`, `square`, `rand`, `irand`, `perlin`) and deterministic hash-based PRNG keyed by cycle/hap position.
  * Tonal & microtonal engine (`scale`, `chord`, `voicing`, `transpose`, `scaleTranspose`, Scala `.scl`/`.kbm` parser).
* **Exit Gate:** 100% of `silverwave-core` unit & `proptest` algebraic law tests passing; `>= 99.5%` line coverage; zero `unsafe` blocks.

### `v0.2.0` — Mini-Notation & DSL
* **Scope:** Full TidalCycles + Strudel Mini-Notation and Mondo Notation parser with exact source-location tracking (`withLoc`) and DSL bindings.
* **Crate Targets:** `silverwave-mini`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` (parser/AST layer)
* **Key Deliverables:**
  * Zero-panic PEG/combinator parser supporting all mini-notation operators (`~`, `[]`, `<>`, `,`, `*`, `/`, `@`, `_`, `!`, `?`, `|`, `(k,n,o)`, `:n`, `%`, `^`, `..`, `{}`).
  * Mondo notation parser for function-call shorthand inside pattern strings.
  * `SpanMap` propagation attaching byte/UTF-16 column-line spans to every `Hap<T>` context for live editor highlighting.
  * Pure-Rust DSL surface supporting both Strudel-style method chaining (`s("bd sd").fast(2).lpf(800)`) and Tidal-style operator/function composition.
* **Exit Gate:** 100% of parser corpus & fuzz tests (`cargo-fuzz` 10M+ iterations with 0 panics) passing; exact `SourceLoc` round-trip parity with Strudel's `withLoc`.

### `v0.3.0` — Lock-Free DSP & Samples
* **Scope:** Hard real-time, zero-allocation synthesis engine, SuperDirt-parity effects, orbit mixing, and asynchronous sample/SoundFont management.
* **Crate Targets:** `silverwave-dsp`
* **Key Deliverables:**
  * Lock-free `rtrb` command queue and pre-allocated voice pool (polyphonic voice stealing, `cut` groups, `clip` duration gating).
  * Built-in synthesizers (`sine`, `sawtooth`, `square`, `triangle`, `supersaw`, `zzfx`, FM, Karplus-Strong, additive) and SoundFont (`.sf2`) playback.
  * Full SuperDirt per-voice and per-orbit FX chain (`lpf`/`hpf`/`bpf`, `vowel`, `shape`, `crush`, `coarse`, `delay`, `room` reverb, `phaser`, `tremolo`, `leslie`, `ring`, `distort`, `adsr`, `duck` sidechain).
  * Asynchronous background loader for `dirt-samples` and `strudel.json` sample manifests (`WAV`, `FLAC`, `OGG`, `MP3`) with zero-copy `Arc<[f32]>` sharing into the RT pool.
  * Native `cpal` audio backend (`WASAPI`, `CoreAudio`, `ALSA`/`JACK`/`PipeWire`).
* **Exit Gate:** Zero-alloc audio thread gate verified under `assert_no_alloc`; `< 2.5 ms` callback processing budget at 48 kHz / 128 frames; 100% DSP unit & impulse-response regression tests passing.

### `v0.4.0` — I/O, Link, Streams & Transitions
* **Scope:** Multi-stream live scheduler, TidalCycles transitions, Ableton Link v3, bidirectional OSC/MIDI, and external controller input.
* **Crate Targets:** `silverwave-io`, `silverwave-cli` (headless daemon)
* **Key Deliverables:**
  * Multi-orbit stream registry (`d1`–`d16`, `p(id, pat)`, `all`, `hush`, `panic`, `solo`, `mute`).
  * Full TidalCycles transition suite (`xfade`, `xfadeIn`, `clutch`, `clutchIn`, `anticipate`, `anticipateIn`, `histpan`, `jump`, `jumpIn`, `jumpMod`, `wait`, `mortal`, `wash`, `interpolate`).
  * Ableton Link v3 peer-to-peer tempo/phase/start-stop synchronization.
  * SuperDirt OSC client/server (send & receive `/dirt/play`, `/ctrl`, custom OSC paths) and full MIDI (`midinote`, `cc`, `progNum`, `pitchbend`, `MPE`, MIDI Clock In/Out).
  * Hardware input mapping (HID/Gamepad, `DeviceMotion` bridge).
* **Exit Gate:** Clock jitter `< 0.1 ms` RMS under Link/MIDI sync tests; 100% deterministic transition boundary tests passing.

### `v0.5.0` — REPL, Visualizers & WASM PWA
* **Scope:** Interactive live-coding environments (native TUI + browser WASM PWA with AudioWorklet) and real-time pattern/audio visualizers.
* **Crate Targets:** `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-viz`, `silverwave-cli`
* **Key Deliverables:**
  * Hot-swapping REPL with cycle-quantized pattern updates, inline `withLoc` token highlighting, and non-blocking diagnostic overlays.
  * Native visualizers (`_pianoroll`, `_scope`, `_fft`, `_punchcard`, `_spiral`, `_pitchwheel`) rendering at 60/120 FPS via WebGPU/Canvas2D and terminal braille/Sixel.
  * `wasm32-unknown-unknown` build with `AudioWorklet` + `SharedArrayBuffer` lock-free ring buffer and offline-first Service Worker PWA.
  * Offline audio/MIDI stem renderer (`silverwave render`) for deterministic export.
* **Exit Gate:** WASM PWA cold-start `< 1.5 s`; zero audio dropouts during live code re-evaluation at 64-sample buffer; 100% visualizer frame-determinism tests passing.

### `v0.6.0` — VST3/CLAP via `nih-plug`, MCP Co-Performance Server & Hydra/Tonal Bridges
* **Scope:** DAW plugin integration (`VST3` & `CLAP`), `silverwave-mcp` 42-tool MCP server, and external creative-coding/theory ecosystem bridges.
* **Crate Targets:** `silverwave-plugin`, `silverwave-mcp`, `silverwave-cli`, `silverwave-viz`, `silverwave-core`
* **Key Deliverables:**
  * `nih-plug` wrapper exposing SilverWave as a `VST3` and `CLAP` instrument/MIDI-generator plugin with DAW host transport/tempo sync, multi-out orbit buses, and automatable `ctrl` parameters.
  * `silverwave-mcp` server (`stdio` and Streamable HTTP+SSE) with 42 tools, 10 live resources, 5 prompts, and `AcousticMixDiagnostic` telemetry.
  * Hydra visual synthesizer bridge: bidirectional pattern-to-Hydra uniform/OSC/WebGL modulation stream (`H(pat)`).
  * Tonal.js-compatible theory JSON/API bridge and Csound/Faust external synth node bridge.
* **Exit Gate:** `pluginval` (VST3) and `clap-validator` (CLAP) strict validation suites passing with zero errors or audio-thread allocations.

### `v0.7.0` — CLI Suite, Editor Extensions & Crash-Safe Live Supervisor
* **Scope:** Full `silverwave-cli` 14-subcommand binary (`repl`, `play`, `eval`, `render`, `osc`, `midi`, `samples`, `lsp`, `mcp`, `share`, `fmt`, `check`, `doctor`, `completions`), 4-stage panic-safe live performance supervisor, and LSP extensions for Neovim, VSCode, Emacs, and Pulsar (`06-SPEC-CLI-PACKAGING-AND-RELEASE.md`).
* **Crate Targets:** `silverwave-cli`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-io`
* **Exit Gate:** 100% of `CLI-01`..`CLI-06` and `PANIC-01`..`PANIC-05` tests green; simulated panic flushes MIDI `CC 120`/`123`, zeroes hardware audio buffers, autosaves unsaved live-coding buffer, and respawns worker in `< 5 ms`.

### `v0.8.0` — Multi-Platform Packaging, Installers & Distribution
* **Scope:** Automated cross-OS packaging and signing pipelines across Linux (`.deb`, `.rpm`, `.flatpak`, `AppImage`, Arch `PKGBUILD`, `rtkit`/`udev` real-time rules), macOS (Universal2 `.app`/`.dmg`/`.pkg`, Developer ID codesigning, `notarytool` notarization, Homebrew Formula & Cask), Windows (WiX v4 `.msi`, NSIS `.exe`, EV Authenticode, WinGet & Scoop manifests), merged `SilverWave.vst3` / `SilverWave.clap` bundles, and `wasm32` COOP/COEP PWA production bundle (`06-SPEC-CLI-PACKAGING-AND-RELEASE.md`).
* **Crate Targets:** `silverwave-cli`, `packaging/*`
* **Exit Gate:** 100% of `PKG-LIN-*`, `PKG-MAC-*`, `PKG-WIN-*`, and `PWA-HOST-*` installer smoke tests green in clean container/VM matrices.

### `v0.9.0` — Release Candidate, 24h Soak/Chaos Testing & Supply-Chain Hardening
* **Scope:** Pre-production Release Candidate hardening: 24-hour continuous multi-orbit audio/Link soak testing (`0` xruns, `0` memory growth), `100M+` libFuzzer corpus iterations across all parsers, SLSA Level 3 build provenance attestation, CycloneDX 1.5 SBOM generation, dual `MIT OR Apache-2.0` + audio asset attribution verification, and `crates.io --dry-run` topological publish verification.
* **Crate Targets:** Entire workspace (`silverwave-*`)
* **Exit Gate:** Zero memory leaks over 24h soak; SLSA Level 3 & CycloneDX SBOM verified; `cargo publish --dry-run` succeeds in strict topological order (`core` → `mini` → `dsp` → `io` → `viz` → `gui`/`wasm`/`lsp`/`mcp`/`plugin` → `cli`).

### `v1.0.0` — 1:1+ Parity Gate, ~100% TDD Coverage & Multi-Platform Release
* **Scope:** Final parity audit against the union of Strudel and TidalCycles, full coverage/mutation verification, and signed multi-platform binary/WASM/plugin releases.
* **Crate Targets:** Entire workspace (`silverwave-*`)
* **Key Deliverables:**
  * 1:1+ Parity Matrix audit: every row in Section 3 and Section 6 verified with automated cross-reference tests.
  * Release artifacts for `x86_64` / `aarch64` (`Linux`, `macOS`, `Windows`), `wasm32` PWA bundle, and `VST3`/`CLAP` plugin bundles.
* **Exit Gate:** 100% of Parity Matrix tests green; `>= 99%` line coverage across workspace; `>= 95%` mutation score; zero `assert_no_alloc` violations; clean `cargo audit` & `cargo deny`.

---

## 3. Comprehensive 1:1 Feature Parity Matrix

### 3.1 Core Pattern Primitives & Algebraic Combinators

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Time & Spans** | `Fraction`, `TimeSpan`, `Arc`, `whole`, `part` | Exact `Rational` intervals `[begin, end)` with `splitQueries` cycle slicing | `silverwave-core` |
| **Event / Hap** | `Event` (Tidal) / `Hap` (Strudel) | `Hap { whole: Option<TimeSpan>, part: TimeSpan, value: T, context: Context }` | `silverwave-core` |
| **Pattern Monad** | `pure`, `silence`, `fmap`, `appLeft`, `appRight`, `appBoth`, `innerJoin`, `outerJoin`, `squeezeJoin` | Full Functor / Applicative / Monad laws over `Pattern<T>` | `silverwave-core` |
| **Constructors** | `stack`, `cat`/`slowcat`, `fastcat`, `timeCat`, `stepcat`, `run`, `scan`, `sequence`, `fromList` | Cycle-aligned, proportional, and stepwise pattern concatenation & layering | `silverwave-core` |
| **Time Modifiers** | `fast`, `slow`, `early` (`<~`), `late` (`~>`), `rev`, `palindrome`, `iter`, `iterBack`, `ply`, `compress`, `zoom`, `linger`, `swing`, `swingBy` | Exact rational time-warping on both queries and hap spans | `silverwave-core` |
| **Euclidean & Poly** | `euclid`, `euclidInv`, `euclidFull`, `polymeter`, `polyrhythm` | Bjorklund algorithm with rotation offset and nested polymetric steps | `silverwave-core` |
| **Stepwise Functions** | `step`, `expand`, `pace`, `drop`, `take`, `extend`, `stepcat` ([Strudel Stepwise](https://strudel.cc/learn/stepwise/)) | Metrical step-counting relative to `^` markers or top-level steps | `silverwave-core` |
| **Conditionals & Combinators** | `every`, `whenmod`, `sometimes`, `often`, `rarely`, `almostAlways`, `almostNever`, `always`, `never`, `chunk`, `chunkBack`, `inside`, `outside`, `within`, `off`, `jux`, `juxBy`, `superimpose`, `layer`, `stut`, `echo`, `echoWith` | Higher-order pattern transformations with cycle predicates and stereo/value offsets | `silverwave-core` |
| **Sample Slicing Combinators** | `striate`, `chop`, `slice`, `splice`, `randslice`, `scramble`, `shuffle`, `rot` | Sub-span `begin`/`end` slicing and permutation | `silverwave-core` |
| **Stochasticity** | `degrade`, `degradeBy`, `undegrade`, `choose`, `wchoose`, `chooseCycles`, `rand`, `irand`, `perlin` | Deterministic cycle/hap-seeded PRNG (reproducible across re-queries) | `silverwave-core` |
| **Continuous Signals** | `sine`, `cosine`, `saw`, `isaw`, `tri`, `square`, `segment`, `range`, `rangex`, `quantize`, `smooth` | Continuous `whole: None` sampling & discrete segmentation | `silverwave-core` |

### 3.2 Mini-Notation, Mondo Notation & DSL

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Sequence & Rests** | `"a b ~ c"` | Space-delimited equal subdivision; `~` produces silence | `silverwave-mini` |
| **Subdivision & Polyphony** | `[a b]`, `{a b, c d e}`, `a, b` | Nested bracket subdivision, `{}` polymeter, `,` parallel stack | `silverwave-mini` |
| **Alternation & Choice** | `<a b c>`, `a\|b\|c` | Cycle-by-cycle alternation (`<>`) and per-event random choice (`\|`) | `silverwave-mini` |
| **Speed & Elongation** | `*n`, `/n`, `@n`, `_`, `!n`, `?p` | Multiplication, division, weight elongation (`@`/`_`), replication (`!`), degrade (`?`) | `silverwave-mini` |
| **Euclidean & Sample Index** | `(k,n,o)`, `:n`, `%`, `^`, `..` | Bjorklund tuple, sample/scale index `:n`, ratio `%`, stepwise `^`, integer range `..` | `silverwave-mini` |
| **Mondo Notation** | `(fn arg1 arg2)` ([Strudel Mondo](https://strudel.cc/learn/mondo-notation/)) | Inline function application syntax inside pattern strings | `silverwave-mini` |
| **Source Locations** | `withLoc` AST offsets | Byte/UTF-16 span mapping from mini-notation tokens into `Hap.context` | `silverwave-mini`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` |
| **Live-Coding DSL** | Strudel JS chaining + Tidal Haskell combinators | Dual-ergonomic Rust DSL (method-chaining & prefix combinators) with hot-reload | `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` |

### 3.3 Tonal, Harmony & Microtonality

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Pitch & Note Representation** | `note`, `n`, `freq`, `midinote` | Scientific pitch (`c4`, `f#3`, `bb2`), MIDI note numbers, Hz frequencies | `silverwave-core` |
| **Scales & Quantization** | `scale("c:minor")`, `scaleTranspose`, `transpose` ([Strudel Tonal](https://strudel.cc/learn/tonal/)) | Full Tonal.js scale dictionary, negative/chromatic step wrapping | `silverwave-core` |
| **Chords & Voicings** | `chord`, `voicing`, `dict`, `anchor`, `mode` (`below`/`duck`/`above`/`root`), `offset`, `rootNotes` | Voice-leading dictionaries, anchor alignment, scale-like `n` indexing | `silverwave-core` |
| **Microtonality** | Scala `.scl` & `.kbm` tuning tables, `xen` | Arbitrary N-EDO, Bohlen-Pierce, Just Intonation, and custom `.scl`/`.kbm` mappings | `silverwave-core` |

### 3.4 Synthesis, SuperDirt DSP & Sample Engine

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Built-in Synths** | `sine`, `sawtooth`, `square`, `triangle`, `supersaw`, `zzfx`, FM, Karplus-Strong, additive | PolyBLEP band-limited oscillators, detuned unison, ZzFX retro SFX, physical modeling | `silverwave-dsp` |
| **SoundFont & Samples** | `s`, `n`, `dirt-samples`, `strudel.json`, `gm_*` (`.sf2`), `begin`, `end`, `speed`, `unit`, `loopAt` | Async sample & SF2 bank loading, pitch-shifted/timestretched playback, zero-alloc RT pool | `silverwave-dsp` |
| **Envelopes & Dynamics** | `gain`, `velocity`, `adsr` (`attack`, `decay`, `sustain`, `release`), `clip`, `cut`, `duck`/`sidechain` | Exponential/linear ADSR, choke groups (`cut`), orbit-to-orbit sidechain ducking | `silverwave-dsp` |
| **Filters & Tone Shaping** | `lpf`/`cutoff`, `hpf`/`hcutoff`, `bpf`/`bandf`, `lpq`/`resonance`, `hpq`, `bpq`, `vowel`, `shape`, `distort`, `crush`, `coarse` | Zero-delay feedback SVF filters, formant vowel filter bank, waveshaping, bit/rate reduction | `silverwave-dsp` |
| **Time-Based & Spatial FX** | `pan`, `delay`, `delaytime`, `delayfeedback`, `room`, `size`, `dry`, `phaser`, `tremolo`, `leslie`, `ring` | Stereo equal-power panning, tempo-synced delay, Schroeder/FDN reverb, modulation FX | `silverwave-dsp` |
| **Orbit Bus Architecture** | `orbit(0..N)` | Isolated per-orbit global FX sends (delay/reverb) + master limiter | `silverwave-dsp` |
| **Audio Backends** | WebAudio AudioWorklet, native `cpal` | `WASAPI`, `CoreAudio`, `ALSA`/`JACK`/`PipeWire`, browser `AudioWorklet` | `silverwave-dsp` |

### 3.5 I/O, Synchronization, Streams & Transitions

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Multi-Stream Scheduler** | `d1`–`d16`, `p`, `all`, `hush`, `panic`, `solo`, `mute`, `cps`, `bpm` | Lookahead arc scheduler (`50 ms` tick / `100 ms` min latency) with lock-free hot-swap | `silverwave-io` |
| **Transitions** | `xfade`, `xfadeIn`, `clutch`, `clutchIn`, `anticipate`, `anticipateIn`, `histpan`, `jump`, `jumpIn`, `jumpMod`, `wait`, `mortal`, `wash`, `interpolate` | Cycle-quantized pattern crossfading, state interpolation, and delayed cutover | `silverwave-io` |
| **Clock & Link** | Ableton Link v3, internal high-precision clock | Peer-to-peer Link session tempo/phase/start-stop sync | `silverwave-io` |
| **OSC Protocol** | SuperDirt `/dirt/play`, `/ctrl`, generic OSC In/Out | Bidirectional UDP/TCP OSC encoding & pattern-value mapping | `silverwave-io` |
| **MIDI & Hardware** | `midi`, `midichan`, `cc`, `progNum`, `pitchbend`, MPE, MIDI Clock, HID/Gamepad/`DeviceMotion` | Cross-platform MIDI (`midir`/WebMIDI), MPE per-note expression, controller streams | `silverwave-io` |

### 3.6 REPL, Visualizers, Plugins & Ecosystem Bridges

| Feature Category | Strudel / TidalCycles Surface | Parity Semantics | Target Crate(s) |
| :--- | :--- | :--- | :--- |
| **Live Code Editor** | Strudel REPL / Tidal editor modes | Syntax highlighting, inline `withLoc` active-event flash, non-blocking error gutter | `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-cli` |
| **Inline Visualizers** | `_pianoroll`, `_scope`, `_fft`/`_spectrum`, `_punchcard`, `_spiral`, `_pitchwheel` | Real-time pattern & audio telemetry rendered in WebGPU/Canvas2D and TUI | `silverwave-viz` |
| **Visual Synth Bridge** | Hydra (`H(pat)`, `/hydra`) | Pattern-driven uniform & modulation streaming to Hydra WebGL/GLSL pipelines | `silverwave-viz` |
| **External Synth Bridges** | Csound / Faust / `nih-plug` VST3 & CLAP | Native DAW plugin export (`VST3`, `CLAP`) with host transport sync & multi-out orbits | `silverwave-plugin`, `silverwave-cli`, `silverwave-dsp` |
| **Offline & PWA** | Offline-first PWA, stem export | Service-worker cached `wasm32` PWA; deterministic `WAV`/`FLAC`/`MIDI` CLI renderer | `silverwave-cli`, `silverwave-wasm` |

---

## 4. DocDD + TDD Governance Gates

Every pull request and milestone transition in SilverWave is strictly governed by automated Document-Driven Development (DocDD) and Test-Driven Development (TDD) gates. **No implementation code may be merged without a finalized specification and failing tests committed first.**

### 4.1 Mandatory Development Lifecycle (Red → Green → Refactor)

1. **Specification Gate (DocDD):** Every new feature or module requires an approved spec in `docs/specs/` defining public types, mathematical/DSP invariants, error variants, and parity references to Strudel/TidalCycles.
2. **Red Gate (Failing Tests First):** A PR introducing the specification must include failing unit, property (`proptest`), or integration tests covering all specified invariants before any implementation code is written.
3. **Green Gate (Minimal Implementation):** Implementation code is added to satisfy the failing tests without modifying the test assertions to fit the code.
4. **Refactor & Hardening Gate:** Code is refactored for clarity, zero-alloc guarantees, and lint compliance while keeping all tests green.

### 4.2 Automated CI Enforcement Gates

| Gate Name | Tooling / Mechanism | Pass Threshold | Scope |
| :--- | :--- | :--- | :--- |
| **Spec-Before-Code Gate** | CI diff checker (`docs/specs/` + `tests/` timestamp/commit check) | 100% of new `pub` items linked to spec ID | Workspace-wide |
| **Unit & Property Gate** | `cargo test --workspace` + `proptest` (10,000 cases/property) | 100% pass, 0 ignored/skipped tests | Workspace-wide |
| **Line & Branch Coverage Gate** | `cargo llvm-cov --workspace --lcov` | **~100% (`>= 99.0%` line, `>= 95.0%` branch)** | `silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io` |
| **Mutation Testing Gate** | `cargo mutants` | `>= 95%` mutant kill rate | `silverwave-core`, `silverwave-mini` |
| **Zero-Alloc Audio Thread Gate** | `assert_no_alloc` global allocator tripwire in RT callback tests | **0 heap allocations, 0 deallocations, 0 mutex locks** | `silverwave-dsp` RT path |
| **Real-Time WCET Budget Gate** | Criterion + audio callback harness (48 kHz / 128 frames = `2.67 ms`) | `p99 < 1.5 ms` at 64-voice polyphony + full orbit FX | `silverwave-dsp` |
| **Fuzzing & Panic-Freedom Gate** | `cargo fuzz` (libFuzzer) on mini-notation, OSC, `.scl`, `.sf2` | 0 panics, 0 OOMs across 10M+ corpus mutations | `silverwave-mini`, `silverwave-io`, `silverwave-dsp` |
| **Safety & Lint Gate** | `cargo clippy --all-targets -- -D warnings` (`pedantic` + `nursery`) | 0 warnings; `#![forbid(unsafe_code)]` in `core`/`mini` | Workspace-wide |
| **Plugin & WASM Validation Gate** | `pluginval` (strictness 10), `clap-validator`, `wasm-pack test` | 0 validation errors | `silverwave-plugin`, `silverwave-wasm`, `silverwave-viz` |
| **Security & Supply Chain Gate** | `cargo audit` + `cargo deny check` | 0 known CVEs, 0 unapproved licenses | Workspace-wide |

### 4.3 Milestone Sign-Off Checklist

A milestone (`v0.1.0` through `v1.0.0`) is considered complete **only** when:
1. All target crates pass every CI gate in Section 4.2.
2. All corresponding rows in the Section 3 Parity Matrix have passing cross-reference tests.
3. No `TODO`, `unimplemented!()`, or `todo!()` macros exist in non-test code for the milestone's scope.
4. Release benchmarks and memory profiles are archived in `docs/benchmarks/<version>.md`.

---

## 5. Specification Suite Index (`00`–`09`) & 1:1+ / MCP / Market Moat Cross-Reference

| Spec File | Primary Crate(s) | Scope & Normative Coverage |
| :--- | :--- | :--- |
| `00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md` | Workspace-wide (`silverwave`) | Cargo workspace topology (`silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io`, `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-mcp`, `silverwave-plugin`, `silverwave-cli`), DocDD + TDD gates, and semantic release milestones (`v0.0.0` → `v1.0.0`). |
| `01-SPEC-CORE-PATTERN-ENGINE.md` | `silverwave-core` | Exact rational `Time`/`TimeSpan`/`Hap<T>`, `Pattern<T>` functor/applicative/monad laws, 90+ combinators, Bjorklund Euclidean rhythms, stepwise patterns, sample slicing, and Scala (`.scl`/`.kbm`) microtonality. |
| `02-SPEC-MINI-NOTATION-AND-DSL.md` | `silverwave-mini` | Unified Strudel Krill + TidalCycles Mini-Notation + Mondo `(fn arg1 arg2)` EBNF/PEG grammar, exact UTF-8 byte/line/col `SourceSpan` tracking, and Rust macro/fluent/chained builder DSL. |
| `03-SPEC-AUDIO-DSP-AND-SAMPLES.md` | `silverwave-dsp` | Zero-allocation `cpal` audio graph, `superdough` + `SuperDirt` synths, SoundFont (`.sf2`) General MIDI `gm_*`, Karplus-Strong, Ring Modulator, 16 orbits, and `strudel.json`/`github:` sample engine. |
| `04-SPEC-IO-STREAMS-AND-TRANSITIONS.md` | `silverwave-io` | Streams (`d1..d16`, `$:`), all 11 Tidal transitions, Ableton Link, SuperDirt OSC (`57120`), MIDI 1.0/2.0 + MPE, Csound `.orc`, and Gamepad/HID/`DeviceMotion` inputs. |
| `05-SPEC-REPL-UI-AND-VISUALS.md` | `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` | 60 FPS span highlighting, inline widgets (`slider`, `ribbon`), 6 visualizers (`pianoroll`, `punchcard`, `spiral`, `scope`, `pitchwheel`, `spectrum`), and Hydra/WebGL2 shader bridge. |
| `06-SPEC-CLI-PACKAGING-AND-RELEASE.md` | `silverwave-cli` | Complete CLI subcommands (`repl`, `play`, `eval`, `render`, `osc`, `midi`, `samples`, `lsp`, `mcp`, `share`, `fmt`, `check`, `doctor`, `completions`), offline WAV/FLAC stem exporter, and release engineering. |
| `07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md` | `silverwave-mcp` | 1:1 human-to-MCP surface parity over `stdio` and Streamable HTTP+SSE: 42 tools, 10 live-subscribed `silverwave://` resources, 5 prompts, surgical AST slider patching, and zero-allocation audio safety. |
| `08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md` | Workspace-wide + `silverwave-plugin` | 28 verified Strudel & TidalCycles/SuperDirt pain-point resolutions: `k-rate`/`a-rate` continuous modulation buses (`*~`/`.bus()`), `StatefulPattern` checkpoints (`accum`/`markov`/`lsystem`/`glide`/`_`), global vs. local controller time, $O(N \log N)$ sweep-line merge, opening-bracket blame, fault-isolated per-stream eval, multi-stage voice FX, seeded FDN reverb, granular DSP, unified chord dictionary, `SuperDoughC4`/`SuperDirtC5` profiles, sample-accurate MIDI ordering, and native CLAP/VST3 DAW plugin (`silverwave-plugin`). |
| `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md` | Workspace-wide | Global competitive telemetry (Sonic Pi, Tau5, Orca, Glicol, Extempore, FoxDot/Renardo, ChucK/WebChucK, Mercury, Sardine), 2025–2026 AI & MCP server audit, and 10 Category-King features (`CK-01`–`CK-10`: Elektron `@pre`/`@nei`/`@fill` trig conditions, `plock()`, Octatrack `scene`/`xfader`/`.ot_freeze()`, M8 `.table()`/`.groove()` & bidirectional Tracker view, inline `>>` modular DSP pipes, closed-loop MCP mix/phase/LUFS diagnostics, 1-click 60 FPS `9:16`/Alpha video exporter, stateless `#sw=` LZ4 QR permalinks, quantized CRDT ensemble sync, and Steam Deck/Daisy targets). |

---

## 6. Clean-Room IP Covenant & Complete `v0.1.0`–`v1.0.0` Feature-to-Milestone Traceability Matrix

### 6.1 Clean-Room Permissive Licensing Covenant (`MIT OR Apache-2.0`)
- All 11 crates in the `silverwave` workspace are licensed under `MIT OR Apache-2.0`.
- All specifications and implementations are clean-room Rust designs derived from public mathematical pattern theory and protocol specifications. Zero AGPLv3 or GPLv3 source code from Strudel, TidalCycles, SuperDirt, or Csound is linked or copied into the workspace; SuperDirt compatibility operates strictly over UDP OSC network packets, and Csound `.orc` support operates via optional dynamic plugin loading / IPC.

### 6.2 Complete Traceability of `PP-01`–`PP-12` (`08-SPEC`) & `CK-01`–`CK-10` (`09-SPEC`) Across Release Milestones

| Milestone | Core Scope | Mapped Pain-Point Resolutions (`08-SPEC`) & Category-King Capabilities (`09-SPEC`) | Mandatory Gate |
| :--- | :--- | :--- | :--- |
| **`v0.1.0`** | `silverwave-core` | `PP-03` (`StatefulPattern`, `_` ties, `glide`), `PP-04` (dual-timeframe `QueryContext`), `PP-05` ($O(N \log N)$ sweep-line join), `PP-08` (zero-safe `arrange`, `once`, `asap`, `fade_out`), `PP-11` (Unified Chord Dictionary), `CK-01` (Elektron `TrigCondition` `@1:4`, `@fill`, `@pre`, `@nei`), `CK-02` (`plock`), `CK-04` (`.table()`, `.groove()`) | `>= 99%` coverage, `i128` Stern-Brocot overflow fuzzing |
| **`v0.2.0`** | `silverwave-mini` | `PP-06` (`DelimiterStack` opening-bracket blame, Damerau-Levenshtein hints, `IntoPattern<T>` lifting, DoS quotas), `CK-01` (`@` trig grammar), `CK-05` (`synth_pipe!` / `.dsp_pipe()` `>>` grammar) | `>= 99%` coverage, 0 panics on `cargo fuzz` |
| **`v0.3.0`** | `silverwave-dsp` | `PP-01` & `PP-02` (64-sample `k-rate`/`a-rate` `ControlSubBlockFrame` across all 39 params), `PP-09` (`OrderedVoiceFxChain`, seeded 8x8 FDN + crossfaded IR reverb), `PP-10` (`GranularSamplerVoice`, AST pre-fetcher, mmap pool, deterministic `cut`), `PP-11` (`SuperDoughC4`/`SuperDirtC5` profiles), `CK-03` (`.ot_freeze()`), `CK-05` (PolyBLEP inline pipe DSP) | `>= 99%` coverage, 0 audio-thread allocations (`assert_no_alloc`), `FTZ`/`DAZ` verified |
| **`v0.4.0`** | `silverwave-io` | `PP-07` (fault-isolated per-stream eval), `PP-08` (pattern-level transitions), `PP-12` (`MidiEventOrderingPriority`, 24 PPQN slave PLL, `.keepAudio(true)`), `CK-03` (`CrossfaderSceneBank`), `CK-09` (cycle-quantized CRDT `CommitEpoch`), `CK-10` (Steam Deck `gilrs` raw HID & Daisy `no_std` core) | `>= 99%` coverage, 0.0 ms 60-min MIDI clock drift |
| **`v0.5.0`** | `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` | `CK-02` (16/64-step LED `plock` bar), `CK-04` (Bidirectional Tracker Grid & Pattern Matrix), `CK-05` (Bitwig-style `ModulationRing` widgets), `CK-07` (Syphon/Spout/NDI output), `CK-09` (Dual-cursor `[HUMAN: Performer]` vs `[MCP AGENT]` badges), `CK-10` (`< 1.8 MB` core WASM + `15 MB` progressive starter pack + Docked `InteractiveTour`) | `>= 99%` coverage, 60 FPS frame budget `< 4 ms` |
| **`v0.6.0`** | `silverwave-mcp`, `silverwave-plugin`, `silverwave-cli` | `07-SPEC` (42 MCP tools, 10 live resources, 5 prompts, surgical AST `slider()` patching), `PP-12` (`silverwave-plugin` 16-bus CLAP/VST3), `CK-06` (`AcousticMixDiagnostic` LUFS/dBTP/phase cancellation alerts), `CK-07` (`silverwave render --video` 60 FPS `9:16`/Alpha WebM + `.swtake` replay), `CK-08` (`#sw=` LZ4 QR permalinks & registry) | `>= 99%` coverage, CLAP/VST3 validator pass |
| **`v0.7.0`–`v1.0.0`** | Full Workspace RC & GA | Soak testing (72h zero-leak/zero-XRUN), WCAG 2.2 AA audit, multi-arch release packaging (`.deb`, `.rpm`, `.flatpak`, `.dmg`, `.msi`, `.clap`, `.vst3`, npm WASM, `crates.io`) | Release Gate Sign-Off |
