# Changelog

All notable changes to **SilverWave** (`silverwave`) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.0] - 2026-11-15

### Added
- **`00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md`**: Established the 11-crate pure-Rust workspace architecture (`silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io`, `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-mcp`, `silverwave-plugin`, `silverwave-cli`), DocDD + TDD governance gates (`>= 99.0%` line coverage, zero audio-thread allocations), and full `v0.0.0` → `v1.0.0` release roadmap.
- **`01-SPEC-CORE-PATTERN-ENGINE.md`**: Normative specification for exact rational `Time`/`TimeSpan`/`Hap<T>`, `Pattern<T>` laws, 90+ Strudel/Tidal combinators, Bjorklund Euclidean rhythms, stepwise operators, sample slicing, `StatefulPattern`, and Scala (`.scl`/`.kbm`) microtonality.
- **`02-SPEC-MINI-NOTATION-AND-DSL.md`**: Normative EBNF/PEG grammar for Strudel Krill + Tidal Mini-Notation + Mondo notation, Elektron conditional trigs (`@1:4`, `@fill`, `@pre`, `@nei`), `DelimiterStack` opening-delimiter diagnostics, and strongly-typed Rust DSL.
- **`03-SPEC-AUDIO-DSP-AND-SAMPLES.md`**: Normative specification for the lock-free `cpal`/WASM synthesis graph, 64-sample sub-block continuous modulation buses (`ControlSubBlockFrame`), `OrderedVoiceFxChain`, seeded 8x8 Hadamard FDN + dual-buffer crossfaded convolution reverb, `GranularSamplerVoice`, `.sf2` SoundFont (`gm_*`), and `strudel.json`/`github:` sample loader.
- **`04-SPEC-IO-STREAMS-AND-TRANSITIONS.md`**: Normative specification for `d1..d16`/`$:` streams, all 11 Tidal transitions, Ableton Link, SuperDirt OSC (`57120`), sample-accurate MIDI 1.0/2.0 + MPE, Csound `.orc`, native 16-stereo-bus CLAP/VST3 DAW plugin (`silverwave-plugin`), and cycle-quantized CRDT ensemble sync.
- **`05-SPEC-REPL-UI-AND-VISUALS.md`**: Normative specification for 60 FPS active-span highlighting, inline widgets (`slider`, `ribbon`, Bitwig-style `ModulationRing`), 6 visualizers (`pianoroll`, `punchcard`, `spiral`, `scope`, `pitchwheel`, `spectrum`), bidirectional Tracker/Phrase Grid view, Hydra WebGL bridge, and Syphon/Spout/NDI output.
- **`06-SPEC-CLI-PACKAGING-AND-RELEASE.md`**: Normative specification for the 14-subcommand `silverwave` CLI, offline WAV/FLAC stem exporter, deterministic 60 FPS `9:16`/Alpha video renderer (`silverwave render --video`), stateless `#sw=` LZ4 permalinks, and multi-platform release engineering.
- **`07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md`**: Normative specification for `silverwave-mcp` (`stdio` and Streamable HTTP+SSE) providing 1:1 human-to-MCP parity across 42 tools, 10 live-subscribed `silverwave://` resources, 5 workflow prompts, surgical AST `slider()` patching, and `AcousticMixDiagnostic` telemetry.
- **`08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md`**: Architectural solutions and TDD regression contracts for 28 verified Strudel, TidalCycles, and SuperDirt user pain points (`PP-01`–`PP-12`).
- **`09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md`**: Global competitive intelligence across 10 live-coding platforms, 2025–2026 AI/MCP music server audit, and 10 Category-King moat specifications (`CK-01`–`CK-10`).
