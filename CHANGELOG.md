# Changelog

All notable changes to **SilverWave** (`silverwave`) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-11-16

### Added
- **`silverwave-mcp` (`v0.7.0` 1:1 Human-Agent Surface Parity Model Context Protocol Server — 42 Tools, 10 Live Resources & 5 Prompts)**:
  - **MCP Transports, Co-Performance Hub & Surgical AST Slider Patching (`src/protocol.rs`)**: Implemented `McpTransportMode` (`stdio` & Streamable `http+sse`), `CoPerformanceSessionHub` (`CoPerformanceBridge`), optimistic concurrency control (`-32001 RevisionConflict`), and cursor-preserving surgical `slider()` literal patching (`SurgicalTextEdit`).
  - **Filesystem Sandbox, SSRF Guard, Token-Bucket Rate Limiter & Preallocated RT Command Ring (`src/security.rs`)**: Implemented `DefaultSandboxGuard` (`PathSandboxGuard`), `TokenBucketRateLimiter` (`RateLimiterPolicy`), and 64-slot `PreallocatedAudioCommandRing` (`RtAudioCommand`).
  - **Complete 42-Tool Surface (`src/tools.rs`, `src/resources.rs`)**: Implemented all 38 core tools (`SilverWaveMcpToolSurface`), 4 Category-King tools (`SilverWaveMcpCategoryKingExtension`: `set_crossfader_scene`, `trigger_looper_freeze`, `set_fill_latch`, `render_video`), `AcousticMixDiagnostic` (`CK-06`), 10 live `silverwave://` resources (`MCP_RESOURCE_URIS`), and 5 workflow prompts (`SilverWaveMcpPromptSurface`).
- **`silverwave-cli` (`v0.7.0` Unified 11-Subcommand CLI & 4-Stage Panic-Safe Live Performance Recovery Supervisor)**:
  - **11-Subcommand CLI Surface & BSD Sysexits (`src/cli.rs`)**: Implemented `repl`, `daemon`, `lsp`, `eval`, `render` (`--video`), `osc`, `sampler` (`index`, `verify`, `import`, `pack`, `list`), `plugin-bundler`, `doctor`, `completions` (across 6 shells), and `mcp`, with deterministic `SilverWaveExitCode` mapping (`0`, `1`, `2`, `64`, `65`, `66`, `69`, `70`, `74`, `78`, `130`).
  - **4-Stage Crash/Panic-Safe Live Performance Recovery Supervisor (`src/supervisor.rs`)**: Implemented Stage 1 lock-free `EmergencyMidiFlushRing` (`CC 120`, `CC 123`, `CC 64` + `ActiveNoteBitset` Note-Offs across 16 channels), Stage 2 64-sample raised-cosine `RtAudioSafetyInterlock`, Stage 3 atomic `.swave` + `.crash.json` `PreallocatedSessionAutosaver`, and Stage 4 `LivePerformanceSupervisor` (`max_restarts_per_minute = 10`).

## [0.6.0] - 2026-11-16

### Added
- **`silverwave-plugin` (`v0.6.0` Native 16-Stereo-Bus / 32-Channel CLAP & VST3 DAW Plugin, Host Transport Sync, 64 Macro Parameters with CLAP Polyphonic Modulation & Tonal.js Theory Bridge)**:
  - **16-Stereo-Bus (32-Channel) Orbit Layout & Bundle Manifests (`src/layout.rs`)**: Implemented `CLAP_PLUGIN_ID` (`org.silverwave.live`), `VST3_CLASS_UID`, `DawPluginConfig` (`PP-12`), `OrbitBusRole`, `StereoOrbitBusDescriptor`, `MultiBusAudioLayout`, `PluginFormat`, and `PluginBundleManifest`.
  - **64 DAW-Automatable Macro Parameters & CLAP Polyphonic Modulation (`src/params.rs`)**: Implemented `ParamId`, `PARAM_ID_XFADER`, `MacroParamSlot` (1-pole zipper-free smoothing), `DawAutomationEvent` (`ParamValue` & `PolyParamMod` per `note_id`), and `DawMacroParamBank`.
  - **Sample-Accurate Host Transport Synchronizer, Tonal.js Theory Bridge & Embedded Co-Performance Bridge (`src/transport.rs`)**: Implemented `DawTransportSynchronizer` (`AUD-03`, `PP-12`), `TonalTheoryDescriptor`, and `DawCoPerformanceBridge` (embedded `silverwave-gui` + `silverwave-lsp` inside the DAW).
  - **Zero-Allocation Real-Time Plugin Processor & `pluginval` / `clap-validator` Self-Test Harness (`src/processor.rs`)**: Implemented `ScheduledOrbitImpulse`, `PluginValidatorReport`, and `SilverWaveDawPlugin`.

## [0.5.0] - 2026-11-16

### Added
- **`silverwave-viz` (`v0.5.0` 1:1 Global & Inline Visualizers, Hydra/WGSL Bridge, Syphon/Spout/NDI VJ Output, Unicode Braille TUI & Deterministic WAV/FLAC Exporter)**:
  - **Draw Command AST & AudioTap (`src/types.rs`)**: Implemented `CycleTime`, `RgbaColor` (`parse_css`, `to_css_hex`), `LineCap`, `DrawCmd`, `VizKind`, `VizTarget` (`Global` & `Inline`), `VizViewport`, `VizFrame`, `VizHap`, `AudioTap`, and `RingBufferAudioTap`.
  - **All 6 Strudel Visualizers (`src/renderers.rs`)**: Implemented `PianorollOptions`, `PianorollRenderer`, `SilverWavePianoroll` (`pianoroll` pre-transform vs `punchcard` post-transform parity & `autorange` — `VIZ-01`, `VIZ-02`), `SpiralOptions`, `SpiralRenderer`, `SilverWaveSpiral` (`steady: 1.0` invariant geometry — `VIZ-03`), `ScopeOptions`, `ScopeRenderer`, `SilverWaveScope` (zero-crossing trigger alignment — `VIZ-04`), `PitchwheelMode`, `PitchwheelOptions`, `PitchwheelRenderer`, `SilverWavePitchwheel` (`edo` divisions with root at `-PI/2` — `VIZ-05`), `SpectrumOptions`, `SpectrumRenderer`, and `SilverWaveSpectrum` (`VIZ-06`, `VIZ-07`).
  - **Hydra/WGSL Shader Bridge, VJ Texture Sharing, Braille TUI & WAV/FLAC Exporter (`src/bridges.rs`)**: Implemented `HydraInitOptions`, `HydraUniformFrame`, `VisualBridge`, `SilverWaveHydraBridge`, `VjOutputProtocol` (`SyphonMacOs`, `SpoutWindows`, `NdiNetwork`), `VjFrameBroadcaster`, `render_draw_cmds_to_braille`, `AudioExportFormat`, `ExportConfig`, and `DeterministicAudioExporter` (`EXP-01`).
- **`silverwave-gui` (`v0.5.0` REPL Engine, Adaptive UTF-8/UTF-16 `SpanMap`, Inline `slider()` Widgets, `.markcss()` Sanitizer, `DocMetadata`, `ShareCodec`, Tracker Grid & Interactive Tour)**:
  - **Adaptive UTF-8 <-> UTF-16 `SpanMap` (`src/span_map.rs`)**: Implemented `ByteOffset`, `Utf16Offset`, `ByteSpan`, `Utf16Span`, `LspPosition`, `LspRange`, `TextEdit`, `SpanValidity`, `SpanMapper`, and `SpanMap` (`SPAN-01`..`SPAN-06`).
  - **Zero-Re-Eval `slider()` `WidgetRegistry`, `ModulationRingOverlay` & CSS Sanitizer (`src/widgets.rs`)**: Implemented `WidgetId`, `WidgetDescriptor`, `SliderPatch`, `ModulationRingOverlay` (`CK-02`), `WidgetRegistry` (`SLIDER-01`..`SLIDER-05`), `NativeHighlightStyle`, `HighlightStyle`, `sanitize_mark_css`, and `ActiveHighlight`.
  - **Metadata & URL Share Codec (`src/metadata.rs`)**: Implemented `ReplError`, `DocMetadata`, `MetadataParser`, `SilverWaveMetadataParser` (`UI-05`), `ShareCodec`, and `SilverWaveShareCodec` (`UI-06` with `MAX_SHARE_PAYLOAD_BYTES = 1_048_576`).
  - **REPL Lifecycle State Machine, Tracker Phrase Grid & 5-Stage Tour (`src/repl.rs`, `src/tracker.rs`)**: Implemented `ClockSource`, `InlineVizAnchor`, `ReplCommand`, `EvalFlashEvent`, `EvalOutcome`, `ReplViewState`, `FrontendCapabilities`, `NativeFrontend`, `ReplEngine` (`UI-01`..`UI-04`), `TrackerFxCommand`, `TrackerRow`, `TrackerPhraseView` (`CK-04`), `TourStage`, and `InteractiveTour` (`CK-07`).
- **`silverwave-wasm` (`v0.5.0` Browser PWA, CodeMirror 6 Handle & Offline `IndexedDB` Sketch Store)**:
  - Implemented `WasmHighlightEntry`, `WasmInlineVizEntry`, `FrameUpdate`, `PwaCacheStrategy`, `PwaOfflineStore`, and `SilverwaveWasmHandle` (`PWA-01`).
- **`silverwave-lsp` (`v0.5.0` Unified Editor Protocol & JSON-RPC 2.0 Daemon for Neovim, VSCode, Emacs & Pulsar)**:
  - Implemented `PositionEncoding`, `EditorRpcRequest`, `EditorRpcNotification`, and `SilverWaveLspServer` (`LSP-01`, `LSP-02`).

## [0.4.0] - 2026-11-16

### Added
- **`silverwave-io` (`v0.4.0` Real-Time Stream Manager, 11 TidalCycles Transitions, Ableton Link Clock, SuperDirt OSC 1.1, `@strudel/midi` + MPE + PLL, Csound & CRDT Ensemble Sync)**:
  - **Shared I/O Primitives & `OutputSink` (`src/types.rs`)**: Implemented `CycleTime`, `CycleArc`, `MidiNoteValue`, `ControlValue`, `ControlMap`, `StateMap`, `StreamId`, `DispatchEvent`, `IoError`, and non-blocking `OutputSink`.
  - **Audio (`cpal` & `wasm32` WebAudio) & `nih-plug` Bridges (`src/backends.rs`, `src/csound.rs`)**: Implemented `AudioFormat`, `AudioThreadEvent`, `AudioRenderEngine`, `ImpulseAudioRenderEngine`, `CpalConfig`, `AudioEventProducer`, `SpscAudioEventProducer`, `CpalBackend`, `MemoryCpalBackend` (`AUD-01`), `WebAudioConfig`, `WebAudioBackend`, `MemoryWebAudioBackend` (`AUD-02` bit-exact parity), `PluginSyncMode`, `HostTransportSnapshot`, `PluginMidiEvent`, `NihPlugBridge`, `SilverWaveNihPlugBridge` (`AUD-03`), `OrcSource` (`github:` shorthand), `CsoundEngine`, and `InProcessCsoundEngine` (`AUD-04`).
  - **SuperDirt OSC 1.0/1.1 Engine (`src/osc.rs`)**: Implemented 4-byte-aligned `encode_osc_packet`, `OscTime` (`NTP_UNIX_OFFSET_SECS = 2_208_988_800`), `OscType`, `OscMessage`, `OscBundle`, `OscPacket`, `OscScheduleMode`, `OscTarget`, `OscArgDefault`, `OscArgSpec`, `OscPayloadMode`, `OscShape`, `RemoteStreamCommand`, `OscEngine`, and `SilverWaveOscEngine` (`OSC-01`..`OSC-04`).
  - **Sample-Accurate `@strudel/midi`, MPE, 24 PPQN Clock PLL & Steam Deck Bridge (`src/midi.rs`)**: Implemented `MidiPortInfo`, `MidiPortHandle`, `MidiBackend`, `MidiEventOrderingPriority` (`PP-12`), `ScheduledMidiPacket`, `MidiScaleCurve`, `MidiCcMapping`, `MidiMap`, `MidiMapRegistry`, `InMemoryMidiMapRegistry` (`MID-02`), `MidiInputNote`, `MidiInputState`, `InMemoryMidiInputState` (`MID-07`), `ActiveNoteKey`, `ActiveNoteEntry`, `ActiveNoteTracker`, `InMemoryActiveNoteTracker` (`MID-08`), `MpeZoneConfig`, `MpeNoteExpression`, `MpeChannelAllocator`, `ExternalMidiClockPll` (`PP-12`), `ControllerInputBridge`, `SteamDeckControllerBridge` (`CK-10`), and `SilverWaveMidiEngine` (`MID-01`..`MID-08`).
  - **Phase-Continuous Clock & Ableton Link Mesh (`src/clock.rs`)**: Implemented `ClockSource`, `ClockSnapshot`, `ClockEngine`, `LinkSessionMesh`, and `LinkClock` (`LNK-01`..`LNK-03`).
  - **All 11 TidalCycles Transitions & Stream Slot Registry (`src/transitions.rs`, `src/streams.rs`)**: Implemented `FadePrimitives`, `TransitionSpec` (`XfadeIn`, `ClutchIn`, `AnticipateIn`, `InterpolateIn`, `Jump`, `JumpIn`, `JumpInPrime`, `JumpMod`, `Histpan`, `Wait`, `WaitT`, `Wash`, `WashIn`, `Mortal`), `TransitionEngine`, `compile_transition_pattern`, `query_transition_arc`, `StreamSlotState`, `StreamManager`, and `SilverWaveStreamRegistry` (`TRN-01`..`TRN-09`, `PP-07`, `PP-08`).
  - **Cycle-Quantized CRDT Ensemble Sync (`src/crdt.rs`)**: Implemented `ActorKind` (`HumanPerformer`, `McpAgent`), `ActorCursorBadge`, `CommitEpoch`, and `CrdtEnsembleSession` (`CK-09`).

## [0.3.0] - 2026-11-15

### Added

* **`silverwave-dsp` (`v0.3.0` Real-Time Zero-Allocation Audio Engine, Synths, FX, Orbits, Sample Loader & Acoustic Telemetry)**:

  * **POD `TriggerEvent` & Branchless Sanitization (`src/events.rs`)**: Implemented `#[repr(C)]` `Copy + Clone` `TriggerEvent`, `Waveform`, `VoiceKind`, `LoopMode`, `FmOpParams`, `ZzfxParams`, `SoundFontVoiceParams`, `KarplusStrongParams`, `RingModParams`, `FilterTopology`, 15-vowel `VowelFormant` (with Unicode/ASCII alias resolution), `InsertFxStage`, `OrderedVoiceFxChain` (`PP-09`), and branchless `sanitize_f32_finite` / `soft_limit_feedback`.

  * **64-Sample Sub-Block Continuous `k-rate`/`a-rate` Modulation Buses & Octatrack Crossfader Scenes (`src/modulation.rs`)**: Implemented `ModulationRate`, `ControlBusId`, all 39 `ModulatableParam` variants (`PP-01`, `PP-02`), `ControlSubBlockFrame`, `ContinuousModulationEngine`, `ModulationBusTable` (`u64` dirty-bit mask), `PatternModulationEvaluator`, and 16-scene `CrossfaderSceneBank` (`CK-03`).

  * **Band-Limited PolyBLEP Oscillators, Supersaw, Spectral Noise, 8-Op FM, 20-Param ZZFX, Karplus-Strong & Inline `>>` Pipe (`src/synths.rs`)**: Implemented `PolyBlepOscillator` (`DSP-01`), `SupersawGenerator` (`DSP-02`), `NoiseGenerator` (`DSP-03`), `AdditiveOscillator`, `FmMatrixSynthesizer` (`DSP-04`), `ZzfxSynthesizer` (`DSP-05`), `KarplusStrongPluck`, and `InlineDspPipeVoice` (`CK-05` with `5 ms` equal-power hot-swap crossfade).

  * **Cytomic ZDF SVF (`12db`/`24db`), Moog Ladder, 15-Vowel Formants & Serial/Ordered FX (`src/filters.rs`, `src/fx.rs`)**: Implemented `AdsrEnvelope`, `MultiModeFilter`, `VowelFormantFilter` (`DSP-06`), `BitCrusherAndCoarse`, `process_shape_and_distort`, `RingModulator`, `TremoloModulator`, `VoiceCompressor`, `equal_power_pan`, `AllpassPhaser`, and `OrderedVoiceFxExecutor` (`DSP-07`, `PP-09`).

  * **`OrbitBus` Routing, `duckorbit`, Seeded 8x8 Hadamard FDN Reverb, Lazy Sample Loader, Granular Engine & Acoustic Telemetry (`src/orbit.rs`, `src/samples.rs`, `src/telemetry.rs`, `src/engine.rs`)**: Implemented `OrbitBus`, `DuckSidechainState` (`LIFE-03`), `OrbitDelay`, `OrbitReverb` (`PP-09`), `OrbitLeslie`, `OrbitMonitor` (`DSP-08`), `DeferredCollector` (`RT-02`), `LazySampleLoader` (`SAMP-01`), `SampleVoicePlayer` (`SAMP-02`, `SAMP-03`), `GranularSamplerVoice` (`PP-10`), `AcousticTelemetryAnalyzer` (`CK-06`), `spsc_ring_buffer` (`RT-03`), and 256-voice `SilverWaveAudioEngine` (`RT-01`, `RT-04`, `RT-05`, `LIFE-01`, `LIFE-02`, `LIFE-04`).

## [0.2.0] - 2026-11-15

### Added
- **`silverwave-mini` (`v0.2.0` Mini-Notation, Mondo & Dual-Layer Rust DSL)**:
  - **UTF-8 `SourceSpan`, `SourceOrigin` (`withLoc`), `DelimiterStack` & Damerau-Levenshtein Diagnostics (`src/span.rs`, `src/diagnostics.rs`)**: Implemented 0-indexed UTF-8 byte + 1-indexed line/column spans, escape-aware `with_loc` offset mapping, `DelimiterStack` opening-delimiter blame (`PP-06`), Damerau-Levenshtein typo suggestions (`".rom"` -> `".room"`, `".lfp"` -> `".lpf"`), and compile-time DoS quotas (`MAX_AST_NESTING_DEPTH = 64`, `MAX_EUCLID_STEPS = 1024`, `MAX_REPLICATE_OR_ELONGATE = 1024`, `MAX_LSYSTEM_EXPANDED_TOKENS = 4096`).
  - **Two-Stage `TPat<T>` AST & Zero-Panic Mini-Notation + Mondo + DSP Pipe Parser (`src/ast.rs`, `src/parser.rs`)**: Implemented all 20 Strudel Krill + Tidal Mini-Notation operator families (`~`, `-`, `[]`, `<>`, `{}`, `,`, `.`, `|`, `*`, `/`, `@w`, `_`, `!n`, `?p`, `:i`, `(k,n,o)`, `a..b`, `^var`, `^^var`, scientific pitch notes, chords, `w/h/q/e/s/t/f/x` ratio shorthands), Elektron conditional trigs (`@1:4`, `@1st`, `@!1st`, `@fill`, `@!fill`, `@pre`, `@!pre`, `@nei`, `@!nei` from `CK-01`), Mondo S-expressions `(func arg1 arg2 ...)`, and inline `>>` modular DSP pipes (`CK-05`).
  - **Stage 2 Compiler & Zero-Reparse 60 FPS Highlighting (`src/compiler.rs`)**: Implemented `compile_tpat`, `mini`, `mini_with_loc`, and `CompiledHighlightPattern<T>` guaranteeing 1 parse on buffer edit and $O(k)$ zero-allocation span lookup across 60 FPS frame queries.
  - **Dual-Layer Rust DSL, All 7 Alignment Suffixes & Fault-Tolerant Session Evaluator (`src/dsl.rs`, `src/transpiler.rs`)**: Implemented `PatternCombinator` (`.in`, `.out`, `.mix`, `.squeeze`, `.squeezeout`, `.reset`, `.restart`), `ur`, `seq_p_loop`, `s_mini`, `note_mini`, `n_mini`, and `MiniStreamSession` (`d1..d16`, `$:`, instant-muted `_$:`, `hush`, `solo`, `mute`, `all`, and `PP-07` fault-isolated stream evaluation).

## [0.1.0] - 2026-11-15

### Added

* **`silverwave-core` (`v0.1.0` Core Pattern Engine)**:

  * **Exact Rational Time & Stern-Brocot Guard (`src/rational.rs`, `src/span.rs`)**: Implemented `#![no_std]` + `alloc` `Rational` / `Rational64` / `Time` backed by `i128` intermediate arithmetic and Stern-Brocot continued-fraction rationalization (`MAX_DENOMINATOR = 1_000_000_007`), `TimeExt` (`sam`, `next_sam`, `cycle_pos`, `to_seconds`), `TimeSpan` (`Arc`, `split_cycles`, `intersection`), `Context`, and `Hap<T>` (`Event<T>`, `has_onset`).

  * **Values, Profiles & Dual-Timeframe `QueryContext` (`src/value.rs`, `src/pattern.rs`)**: Implemented `Value`, `NoteValue`, `ControlMap`, `EngineCompatibilityProfile` (`SuperDoughC4` vs `SuperDirtC5` solving `PP-11`), Dual-Timeframe `QueryContext` (`local_span` vs `global_span` solving `PP-04`), `global_controller` vs `.local_time()`, and universal `IntoPattern<T>` argument lifting (`PP-06`).

  * **Pattern Algebra &** $O((N+M)\log(N+M))$ **Sweep-Line Joins (`src/algebra.rs`)**: Implemented `PatternFunctor` (`fmap`), `PatternApplicative` (`app_both`, `app_left`, `app_right`, `lift_a2_*`), `sweep_line_join` (`PP-05`), `PatternMonad` (`bind`, `unwrap`, `inner_join`, `outer_join`, `squeeze_join`), and `PatternTrigJoin` (`trig_join`, `trig_zero_join`, `reset`, `restart`).

  * **90+ Combinators, Euclidean, Stepwise & Sample Slicing (`src/combinators.rs`, `src/stepwise.rs`)**: Implemented `pure`, `silence`, `stack`, `slowcat`, `slowcat_prime`, `fastcat`, `timecat`, `from_list`, `run`, `scan`, `fast`, `slow`, `rev`, `palindrome`, `early`, `late`, `iter`, `iter_back`, `segment`, `compress`, `zoom`, `inside`, `outside`, `within`, `linger`, `swing_by`, `swing`, `ribbon`, `every`, `every_offset`, `each`, `when`, `whenmod`, `whenmod_pat`, `sometimes_by`, `chunk`, `ply`, `off`, `superimpose`, `layer`, `stut`, `echo`, `echo_with`, `jux`, `jux_by`, `bjorklund`, `euclid`, `euclid_rot`, `euclid_inv`, `euclid_full`, `step`, `expand`, `pace`, `take_steps`, `drop_steps`, `rot`, `stepcat`, `polymeter`, `polyrhythm`, `striate`, `chop`, `slice`, `splice`, `randslice`, `scramble`, `shuffle`, and `ControlPatternOps`.

  * **Deterministic Signals, Unified Chord/Voicing Engine & Scala Microtonality (`src/signals.rs`, `src/tonal.rs`)**: Implemented SplitMix64 `time_to_rand`, `rand`, `irand`, $C^2$-continuous quintic `perlin`, `berlin`, `sine`, `cosine`, `saw`, `isaw`, `tri`, `square`, `SignalRange`, 24-scale `TonalExt`, Unified Superset Chord & Voicing Parser (`ireal` `C^7`/`Co7`, standard `Cmaj7`/`Cdim7`, Tidal `c'maj7`/`gm13`/`a7s5`, plus `@lefthand`/`@ireal`/`@drop2`/`@triad` tags), `arp_with`, and `ScalaTuning` (`.scl`/`.kbm`, `edo`, `degree_to_freq_hz`).

  * **Stateful Patterns, Elektron Trigs, Plocks, M8 Sub-Tables & 11 Pure Transitions (`src/stateful.rs`, `src/extensions.rs`)**: Implemented `LruCycleStateCheckpointer` (`MAX_STATE_CHECKPOINTS_PER_STREAM = 256`), `StatefulPattern` (`.accum()`, `markov_pattern`, `lsystem_expand`), `tie_contiguous`, `.glide()`, Elektron `TrigCondition` (`@1:4`, `@1st`, `@fill`, `@pre`, `@nei`), `.plock()`, `.table()`, `.groove()`, zero-safe `arrange`, `seq_p`, `once`, `asap`, `qtrigger`, `.fade_in()`, `.fade_out()`, and all 11 pure pattern-level Tidal transitions.

* **11-Crate Cargo Workspace Scaffolding (`Cargo.toml`, `deny.toml`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`)**: Established the full workspace build and TDD coverage pipeline.

## \[0.0.0\] - 2026-11-15

### Added

* **`00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md` through `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md`**: Complete DocDD + TDD specification baseline.
