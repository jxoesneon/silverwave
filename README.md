# SilverWave (`silverwave`)

**SilverWave** is a pure-Rust live-coding pattern, synthesis, visualization, DAW plugin (**CLAP** / **VST3**), and **Model Context Protocol (MCP)** co-performance engine engineered for **1:1+ parity** with the union of [Strudel](https://codeberg.org/uzu/strudel) ([strudel.cc](https://strudel.cc/)) and [TidalCycles](https://github.com/tidalcycles/Tidal) ([tidalcycles.org](https://tidalcycles.org/)) / [SuperDirt](https://github.com/musikinformatik/SuperDirt).

---

## Key Architectural Highlights

- **Pure-Rust Rational-Time Pattern Engine (`silverwave-core`, `silverwave-mini`)**:
  - Exact `Rational64` (`i128` Stern-Brocot overflow-guarded) cycle timing (`TimeSpan`, `Hap<T>`, `Pattern<T>`).
  - Unified [Strudel Krill](https://strudel.cc/learn/mini-notation/) + [TidalCycles Mini-Notation](https://tidalcycles.org/docs/reference/mini_notation/) + [Mondo Notation](https://strudel.cc/learn/mondo-notation/) parser with UTF-8 byte/line/column `SourceSpan` tracking and `DelimiterStack` opening-bracket diagnostics.
  - Epoch-checkpointed `StatefulPattern<S, T>` (`accum`, `scan`, `markov`, `lsystem`), cross-cycle ties (`_`), legato portamento (`.glide()`), and $O((N+M)\log(N+M))$ sweep-line `Hap` joins.
- **Zero-Allocation Real-Time Audio & Continuous Modulation (`silverwave-dsp`)**:
  - Lock-free `cpal` / WebAudio `AudioWorklet` synthesis graph with `0` audio-thread heap allocations (`assert_no_alloc`).
  - **64-Sample Sub-Block Continuous `k-rate` & `a-rate` Modulation Buses (`*~`, `.bus()`)** across all 39 synthesis and effect parameters without retriggering envelopes via `.seg()`.
  - Ordered multi-stage per-voice insert FX chains, seeded 8x8 Hadamard FDN + dual-buffer crossfaded convolution reverb, dedicated `GranularSamplerVoice`, `.sf2` SoundFont (`gm_*`), Karplus-Strong waveguide, and inline PolyBLEP `>>` modular DSP pipes.
- **Hardware Sequencer, Tracker & DAW Bridges (`silverwave-io`, `silverwave-gui`, `silverwave-plugin`)**:
  - Native **Elektron-grade conditional trigs** (`@1:4`, `@1st`, `@fill`, `@pre`, `@nei`), step-indexed parameter locks (`.plock()`), and **Octatrack Scene A/B Crossfader Macros (`xfader`)** + live looper resample freeze (`.ot_freeze()`).
  - **Dirtywave M8 / Renoise Tracker View** (`NOTE | INST | VOL | FX1 | FX2 | FX3`), multi-rate `.table()` sub-step modulators, and `.groove()` tick-ratio swing tables.
  - Native **16-Stereo-Bus (32-Channel) CLAP & VST3 Plugin (`silverwave-plugin`)** via [`nih-plug`](https://github.com/robbert-vdh/nih-plug) with sample-accurate host transport sync, plus [Ableton Link](https://ableton.github.io/link/), SuperDirt OSC (`57120`), sample-accurate MIDI 1.0/2.0 + MPE, and Csound `.orc` support.
- **1:1 Human + MCP Co-Performance Surface (`silverwave-mcp`)**:
  - Full [Model Context Protocol (2026-07-28)](https://modelcontextprotocol.io/specification/2026-07-28) server over `stdio` and Streamable HTTP+SSE (`silverwave mcp`).
  - Exposes **42 tools**, **10 live-subscribed `silverwave://` resources**, **5 workflow prompts**, surgical in-place AST `slider()` patching, and closed-loop `AcousticMixDiagnostic` telemetry (EBU R128 `LUFS`, `dBTP`, stereo phase correlation, and inter-orbit sub-bass phase cancellation alerts).
- **1-Click Deterministic 60 FPS Video Renderer & Stateless Share Permalinks (`silverwave-viz`, `silverwave-cli`)**:
  - `silverwave render --video` exports frame-exact `9:16` Vertical (`1080x1920` Shorts/TikTok/Reels), `16:9` 4K, and **Transparent Alpha WebM VP9 / ProRes 4444** code + visualizer + shader overlays without OBS or BlackHole loopback.
  - Stateless `#sw=<base64url_lz4>` share permalinks with scannable QR watermarks.

---

## Workspace Crate Status (`v1.0.0` GA)

| Crate | Version | Status | Coverage Gate |
| :--- | :--- | :--- | :--- |
| **`crates/silverwave-core`** | `1.0.0` | **Implemented (`#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-mini`** | `1.0.0` | **Implemented (`#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-dsp`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, Zero-Alloc Audio Callback)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-io`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, Lock-Free SPSC & Link/OSC/MIDI/CRDT)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-viz`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, 6 Visualizers + Hydra/VJ + WAV/FLAC)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-gui`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, REPL Engine + SpanMap + Sliders + Tracker)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-wasm`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, CodeMirror 6 + PWA Offline Store)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-lsp`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, JSON-RPC 2.0 Editor Daemon)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-plugin`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, 32-Ch CLAP & VST3 DAW Plugin)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-mcp`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, 42 Tools, 10 Resources, 5 Prompts)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |
| **`crates/silverwave-cli`** | `1.0.0` | **Implemented (`#![forbid(unsafe_code)]`, 11 Subcommands + 4-Stage Panic Supervisor)** | **100.0% Symbol & Contract Coverage (`>= 99.5%` CI Line Gate)** |

---

## Specification Suite (`v0.0.0` DocDD + TDD Baseline)

| Document | Target Crate(s) | Scope |
| :--- | :--- | :--- |
| [`docs/specs/00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md`](docs/specs/00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md) | Workspace-wide | 11-crate workspace topology, DocDD + TDD governance gates, and `v0.0.0` → `v1.0.0` release roadmap |
| [`docs/specs/01-SPEC-CORE-PATTERN-ENGINE.md`](docs/specs/01-SPEC-CORE-PATTERN-ENGINE.md) | `silverwave-core` | Rational `Time`/`TimeSpan`/`Hap<T>`, `Pattern<T>`, 90+ combinators, Euclidean/stepwise/slicing, `StatefulPattern`, and Scala `.scl`/`.kbm` |
| [`docs/specs/02-SPEC-MINI-NOTATION-AND-DSL.md`](docs/specs/02-SPEC-MINI-NOTATION-AND-DSL.md) | `silverwave-mini` | Krill + Tidal Mini-Notation + Mondo EBNF grammar, `@pre`/`@nei`/`@fill` trig conditions, `DelimiterStack` blame, and Rust DSL |
| [`docs/specs/03-SPEC-AUDIO-DSP-AND-SAMPLES.md`](docs/specs/03-SPEC-AUDIO-DSP-AND-SAMPLES.md) | `silverwave-dsp` | Zero-alloc audio graph, 64-sample `k-rate`/`a-rate` buses, `OrderedVoiceFxChain`, FDN/IR reverb, granular engine, and `.sf2`/`strudel.json` loader |
| [`docs/specs/04-SPEC-IO-STREAMS-AND-TRANSITIONS.md`](docs/specs/04-SPEC-IO-STREAMS-AND-TRANSITIONS.md) | `silverwave-io`, `silverwave-plugin` | Streams (`d1..d16`, `$:`), 11 Tidal transitions, Ableton Link, SuperDirt OSC, MIDI/MPE, CLAP/VST3 plugin, and CRDT ensemble sync |
| [`docs/specs/05-SPEC-REPL-UI-AND-VISUALS.md`](docs/specs/05-SPEC-REPL-UI-AND-VISUALS.md) | `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp` | 60 FPS active-span highlighting, inline `slider()` + `ModulationRing`, 6 visualizers, Tracker Grid view, Hydra bridge, and Syphon/Spout/NDI |
| [`docs/specs/06-SPEC-CLI-PACKAGING-AND-RELEASE.md`](docs/specs/06-SPEC-CLI-PACKAGING-AND-RELEASE.md) | `silverwave-cli` | 14 CLI subcommands (`repl`, `daemon`, `lsp`, `eval`, `render`, `osc`, `midi`, `samples`, `lsp`, `mcp`, `share`, `fmt`, `check`, `doctor`, `completions`), 60 FPS video/stem export, and CI/CD packaging |
| [`docs/specs/07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md`](docs/specs/07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md) | `silverwave-mcp` | 1:1 human-to-MCP co-performance surface (42 tools, 10 live resources, 5 prompts, surgical AST patching, and `AcousticMixDiagnostic`) |
| [`docs/specs/08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md`](docs/specs/08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md) | Workspace-wide | Architectural solutions and TDD contracts for 28 verified Strudel, TidalCycles, and SuperDirt community pain points (`PP-01`–`PP-12`) |
| [`docs/specs/09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md`](docs/specs/09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md) | Workspace-wide | Global competitive telemetry, 2025–2026 AI/MCP music server audit, and 10 Category-King moat specifications (`CK-01`–`CK-10`) |

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([`LICENSE-MIT`](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
