# `08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS.md` — SilverWave 1:1+ Pain-Point Resolution & Architectural Enhancements Specification

| Field | Value |
| :--- | :--- |
| **Document ID** | `08-SPEC-1-TO-1-PLUS-PAIN-POINT-SOLUTIONS` |
| **Version** | `1.0.0` |
| **Status** | Normative Specification (DocDD / TDD Baseline) |
| **Target Crates** | `silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io`, `silverwave-plugin`, `silverwave-lsp`, `silverwave-gui`, `silverwave-mcp`, `silverwave-cli` |
| **Upstream Baselines** | [Strudel](https://codeberg.org/uzu/strudel) ([GitHub Mirror](https://github.com/tidalcycles/strudel), [Uzu Forum](https://uzu.lurk.org/)) & [TidalCycles](https://github.com/tidalcycles/Tidal) ([SuperDirt](https://github.com/musikinformatik/SuperDirt), [Codeberg SuperDirt](https://codeberg.org/musikinformatik/SuperDirt)) |

---

## 1. Executive Summary & 1:1+ Pain-Point Taxonomy

While achieving 1:1 functional parity with the union of [Strudel](https://codeberg.org/uzu/strudel) and [TidalCycles](https://github.com/tidalcycles/Tidal) establishes a complete live-coding foundation, real-world performers and composers across both ecosystems encounter recurring structural bottlenecks rooted in JavaScript/WebAudio (`superdough`), Haskell/GHCi typeclass resolution, and SuperCollider (`scsynth`/`SuperDirt`) IPC constraints.

This specification defines **SilverWave's 1:1+ architectural solutions** for **28 verified community pain points** across 8 engineering domains, complete with Rust trait contracts, grammar extensions, DSP topologies, and TDD verification matrices.

### 1.1 Master Pain-Point & 1:1+ Solution Matrix

| ID | Upstream Ecosystem | Verified User Pain Point & Primary Sources | Root Cause in Strudel / TidalCycles | SilverWave 1:1+ Architectural Solution | Target Crate(s) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **PP-01** | Strudel & Tidal/SuperDirt | **Onset-Only ("Set-and-Forget") Parameter Sampling**: Continuous patterns (`sine`, `saw`, `slider()`, `cc()`) freeze at note-on; sweeping a filter across a long pad or breakbeat requires `.seg(16)`, which destructively retriggers sample/synth onsets ([Strudel #561](https://github.com/tidalcycles/strudel/issues/561), [Strudel Effects Docs](https://strudel.cc/learn/effects/), [Tidal #806](https://github.com/tidalcycles/Tidal/issues/806)). | `Cyclist` ([`cyclist.mjs`](https://github.com/calvinw/strudel-llm-mirror/blob/main/packages/core/cyclist.mjs)) and Tidal only dispatch parameters when `hap.hasOnset()` is true. | **Lock-Free Control-Rate (`k-rate`) & Audio-Rate (`a-rate`) Modulation Buses (`*~`, `.bus()`, `.mod()`)** updating active voices every 64-sample sub-block without retriggering envelopes. | `silverwave-core`, `silverwave-dsp` |
| **PP-02** | Tidal/SuperDirt | **41 Non-Modulatable SuperDirt Parameters & 20 Hz Bus Flood**: SuperDirt `^bus` cannot modulate 41 core parameters (`crush`, `coarse`, `shape`, `vowel`, `room`, `size`, `Leslie`) and floods OSC with redundant 20 Hz messages ([SuperDirt #190](https://github.com/musikinformatik/SuperDirt/issues/190), [Tidal #806](https://github.com/tidalcycles/Tidal/issues/806)). | Hardcoded `SynthDef` UGen wiring in SuperCollider and discretized 20 Hz OSC polling in Haskell. | **100% Modulatable Parameter Matrix** with SIMD per-block parameter smoothing and dirty-bit delta suppression. | `silverwave-dsp`, `silverwave-io` |
| **PP-03** | Strudel & TidalCycles | **Stateless Cycle Horizon Blocks Accumulators, Markov Chains, L-Systems, Glides & Ties**: Pure `TimeSpan -> [Hap]` prevents step counters across cycles ([Strudel Disc #1201](https://github.com/tidalcycles/strudel/discussions/1201), [Strudel Disc #646](https://github.com/tidalcycles/strudel/discussions/646), [Uzu #3353](https://uzu.lurk.org/t/generative-strategies-and-workflows/3353), [Uzu #4247](https://uzu.lurk.org/t/tidal-2-0/4247?page=2)), cross-event pitch glides ([Strudel #561](https://github.com/tidalcycles/strudel/issues/561)), and cycle-spanning ties ([Strudel Disc #1254](https://github.com/tidalcycles/strudel/discussions/1254)). | Purely functional query function has no cycle-indexed state checkpointing or voice-continuation metadata. | **Deterministic Epoch-Checkpointed `StatefulPattern<S, T>`** (`accum`, `scan`, `markov`, `lsystem`), cross-event `glide(cycles)` portamento, and cycle-spanning musical ties (`_`). | `silverwave-core`, `silverwave-dsp` |
| **PP-04** | TidalCycles | **Time-Transformer Controller Desynchronization**: Applying `slow 2` or `fast 4` scales the query arc used to read external controller state (`^var` / `cF`), causing two patterns reading the same knob to see values from different cycles ([Tidal #1029](https://github.com/tidalcycles/Tidal/issues/1029)). | `cF` samples state at local transformed pattern time rather than wall-clock/global cycle time. | **Dual-Timeframe Controller Lookup**: `cF` / `^var` binds to unscaled **Global Cycle Time** by default, while `cF_local` / `^^var` explicitly opts into pattern-warped local time. | `silverwave-core` |
| **PP-05** | TidalCycles | **Exponential $O(N^2)$ Event-Merge Lag with Stacked Randomness**: Stacking `sometimesBy` + `#` control merges causes severe CPU spikes and audio dropouts ([Tidal #636](https://github.com/tidalcycles/Tidal/issues/636), [Tidal #641](https://github.com/tidalcycles/Tidal/issues/641)). | Naive Cartesian list filtering and duplicate branches in `filterValues` / `applyPatToPat`. | **Interval-Tree Sweep-Line `Hap` Join ($O(N \log N)$)** and branch-fused probabilistic masks (`SmallVec<[Hap; 16]>`). | `silverwave-core` |
| **PP-06** | Strudel & TidalCycles | **Cryptic Parser/GHC Errors & End-of-String Bracket Blame**: PeggyJS/Acorn throws raw `peg$SyntaxError` / `Unexpected token` blaming end-of-string on unclosed `[` ([Uzu #6135](https://uzu.lurk.org/t/contribution-error-handling/6135), [Strudel PR #1315](https://github.com/tidalcycles/strudel/pull/1315), [Codeberg PR #936](https://codeberg.org/uzu/strudel/pull/936), [Strudel #30](https://github.com/tidalcycles/strudel/issues/30)); GHCi emits multi-page `No instance for (IsString Double)` / `(Num Bool)` ([Uzu #2767](https://uzu.lurk.org/t/using-markovpat-for-chords/2767), [Uzu #4702](https://uzu.lurk.org/t/boolean-midi-controls-for-more-efficient-code/4702)) and `$` vs `#` vs `.` fixity traps ([Uzu #2728](https://uzu.lurk.org/t/syntax-tips-for-stitching-tidal-functions-together/2728)). | Lack of delimiter-stack recovery, dynamic JS prototype untyped chains, and Haskell leaky typeclasses. | **Opening-Delimiter Span Blame, Automatic `Pattern<T>` Argument Lifting** (making `whenmod`, `arpWith`, `markov`, `while` accept patterns in all positions, fixing [Uzu #2722](https://uzu.lurk.org/t/whenmod-8-5-6-5-possible/2722), [Uzu #5104](https://uzu.lurk.org/t/patterning-function-parameters/5104), [Tidal #620](https://github.com/tidalcycles/Tidal/issues/620)), and **Damerau-Levenshtein Typo Diagnostics**. | `silverwave-mini`, `silverwave-lsp` |
| **PP-07** | Strudel | **`composify` Memory Leak & Whole-Buffer Evaluation Fragility**: `composifiedRegistry` leaks 100–200 kB/s until browser crash ([Strudel #487](https://github.com/tidalcycles/strudel/issues/487), [Codeberg PR #484](https://codeberg.org/uzu/strudel/pulls/484), [Strudel #1266](https://github.com/tidalcycles/strudel/issues/1266)); syntax error on one line blocks the entire buffer and Flok peers, or resets `cps`/CC state ([Strudel #34](https://github.com/tidalcycles/strudel/issues/34), [Codeberg PR #539](https://codeberg.org/uzu/strudel/pulls/539), [Codeberg PR #1054](https://codeberg.org/uzu/strudel/pull/1054)). | Dynamic JS closure wrapping per evaluation and monolithic whole-file AST evaluation. | **Zero-Leak Arena/Arc Pattern Graph & Fault-Isolated Per-Stream Evaluation**: broken `d1:` keeps previous `d1:` pattern playing while `d2:` updates cleanly; session `cps`/CC/orbit state is never wiped on re-eval. | `silverwave-core`, `silverwave-lsp` |
| **PP-08** | Strudel & TidalCycles | **Linear Song Arrangement, `arrange(0, pat)` Crash, Missing `once`/`asap` & Pattern-Triggered Transitions**: `arrange([0, pat])` throws divide-by-zero and rejects `slider()` ([Codeberg PR #508](https://codeberg.org/uzu/strudel/pulls/508), [Strudel Disc #629](https://github.com/tidalcycles/strudel/discussions/629)); Strudel lacks quantized `once`/`asap` ([Uzu #5864](https://uzu.lurk.org/t/once-or-asap-in-strudel/5864)) and timeline fade-outs ([Uzu #6215](https://uzu.lurk.org/t/strudel-how-to-do-a-progressive-fade-out/6215)); Tidal `ur`/`seqP`/`qtrigger` are verbose and transitions cannot be triggered from patterns/MIDI ([Uzu #5461](https://uzu.lurk.org/t/i-want-tips-for-structuring-my-first-tidalcycles-piece/5461), [Tidal #716](https://github.com/tidalcycles/Tidal/issues/716)). | `arrange` divides by total cycles without filtering zero-length sections; transitions live in `IO` outside the pattern algebra. | **Zero-Safe Dynamic `arrange` (supporting `0` mute/skip & `slider()`), First-Class `once` / `asap` / `qtrigger` / `fade_out`, and Pattern-Combinator Transitions** callable inside patterns and MIDI mappings. | `silverwave-core`, `silverwave-io` |
| **PP-09** | Strudel & SuperDirt | **Single-Use Effect Slots, Multi-Orbit Gain Multiplication & Main-Thread Reverb IR Stalls**: `.lpf(100).distort(2).lpf(800)` overwrites the first filter; `.orbit("2,3,4")` triples dry volume ([Strudel Effects Docs](https://strudel.cc/learn/effects/)); changing `roomsize`/`rlp` regenerates multi-second white-noise IRs on the main thread with unseeded `Math.random()`, causing clicks and non-deterministic bounces ([Stave #1665](https://github.com/MrityunjayBhardwaj/Stave/issues/1665)); `panic()` nulls master gain ([Medium Analysis](https://medium.com/@jacksonfdam/hook-the-beat-before-it-sounds-e1e5f14acc83)). | Flat object property map per `Hap` (`hap.value.lpf`) and synchronous `reverbGen.mjs` buffer allocation. | **Ordered Multi-Stage Per-Voice Insert FX Chain** (allowing arbitrary repeated filter/drive stages), energy-normalized multi-orbit routing, and **Deterministic Seeded FDN + Dual-Buffer Crossfaded Convolution Reverb**. | `silverwave-dsp` |
| **PP-10** | Strudel & SuperDirt | **Sample First-Hit Silence, `chop(64)` Scheduler Flood, Cut-Group Race & 2 GB `scsynth` RAM Wall**: Lazy HTTP sample fetch exceeds 100 ms lookahead ([Strudel #187](https://github.com/tidalcycles/strudel/issues/187)); Service Worker caches stale `strudel.json` for 30 days ([Strudel #1218](https://github.com/tidalcycles/strudel/issues/1218)); `.chop(64)` floods scheduler with discrete Haps ([Reddit Granular](https://www.reddit.com/r/livecoding/comments/1nzeq8u/granular_synthesis_in_strudel/)); simultaneous cut-groups fail ([SuperDirt #282](https://github.com/musikinformatik/SuperDirt/issues/282)); `scsynth` crashes at `maxNodes`/`numBuffers`/2 GB `memSize` ([SuperDirt #156](https://github.com/musikinformatik/SuperDirt/issues/156), [SuperDirt #255](https://github.com/musikinformatik/SuperDirt/issues/255), [Uzu #3148](https://uzu.lurk.org/t/superdirt-lazy-samples-loading/3148?page=2)). | Reactive on-hit sample loading, event-level granular slicing, non-atomic voice stealing, and fixed `scsynth` real-time pool limits. | **AST Pre-Roll Sample Pre-Fetcher** with `StaleWhileRevalidate` manifest cache, **Dedicated DSP Granular Engine** (zero `Hap` explosion), **Deterministic Same-Sample Cut-Group Resolution**, and **Streaming Disk/Mmap Sample Pool**. | `silverwave-dsp`, `silverwave-io` |
| **PP-11** | Strudel, Tidal & SuperDirt | **Harmony Dictionary Fragmentation & `StrudelDirt` Pitch/Envelope Divergence**: `ireal` dictionary broke `dim`/`dim7` and Tidal `Gm13`/`A7s5` ([Codeberg PR #967](https://codeberg.org/uzu/strudel/pull/967), [Strudel #1265](https://github.com/tidalcycles/strudel/issues/1265), [Uzu #5748](https://uzu.lurk.org/t/a-full-list-of-supported-chords-or-a-way-to-import-custom-chord-definitions-in-strudel/5748), [Strudel #316](https://github.com/tidalcycles/strudel/issues/316)); SuperDirt and Strudel disagree on middle-C (`C5` vs `C4`), synth gain calibration, and filter ADSR defaults ([SuperDirt #302](https://github.com/musikinformatik/SuperDirt/issues/302)). | Incompatible chord lookup tables and divergent `superdough` vs `SuperDirt` synth definitions. | **Unified Superset Chord/Voicing Parser** (supporting `dim7`, `o7`, `Gm13`, `A7s5`, and per-chord dictionary tags) and **Selectable `PitchAndEnvelopeProfile` (`SuperDoughC4` vs `SuperDirtC5`)**. | `silverwave-core`, `silverwave-dsp` |
| **PP-12** | Strudel & TidalCycles | **MIDI Clock Drift, `noteOffsetMs: 10` Retrigger Drop, Bank/PC Ordering & DAW Loopback Friction**: `AudioContext` vs `performance.now()` causes 15 ms/min MIDI drift ([Strudel #119](https://github.com/tidalcycles/strudel/issues/119)); `noteOffsetMs: 10` hack drops fast notes ([`midi.mjs`](https://github.com/calvinw/strudel-llm-mirror/blob/main/packages/midi/midi.mjs)); `sendNote` fires before `sendProgramChange` ([Strudel #192](https://github.com/tidalcycles/strudel/issues/192), [Strudel #1175](https://github.com/tidalcycles/strudel/issues/1175)); `.midi()` mutes WebAudio ([Uzu #5718](https://uzu.lurk.org/t/webaudio-playing-while-midi-is-also-being-sent/5718)); DAW integration requires loopback cables ([Uzu #6015](https://uzu.lurk.org/t/how-to-get-multichannel-audio-into-daw/6015), [Uzu #4861](https://uzu.lurk.org/t/the-supedirtmixer-v1-0-is-out-now/4861)). | Dual unsynchronized clocks, timer-based MIDI note-offs, unordered event hooks, and standalone-only process model. | **Hardware-Sample-Accurate MIDI Scheduler + External MIDI Clock PLL**, strict `BankMSB -> BankLSB -> ProgramChange -> CC -> NoteOn` state machine, simultaneous `.midi().audio()` dual routing, and **Native CLAP / VST3 Plugin Target (`silverwave-plugin`)** with 16 stereo DAW output buses. | `silverwave-io`, `silverwave-plugin` |

---

## 2. Continuous Control-Rate (`k-rate`) & Audio-Rate (`a-rate`) Modulation Bus Architecture (Solves PP-01, PP-02)

### 2.1 Architectural Problem Statement
In both [Strudel (`#561`)](https://github.com/tidalcycles/strudel/issues/561) and [TidalCycles (`#806`)](https://github.com/tidalcycles/Tidal/issues/806), pattern parameters are evaluated only at discrete event onsets (`hap.hasOnset()`). If a user writes `s("pad").note("c3").lpf(sine.range(200, 4000))` over a 4-cycle sustaining pad or breakbeat, the low-pass filter cutoff is sampled once at `t = 0` and stays frozen for all 4 cycles. Attempting to work around this via `.seg(64)` slices the pattern into 64 discrete events per cycle, retriggering the sample/oscillator 64 times instead of sweeping the filter smoothly. Furthermore, in [SuperDirt (`#190`)](https://github.com/musikinformatik/SuperDirt/issues/190), 41 parameters cannot be modulated via control buses at all.

### 2.2 SilverWave Solution: Sub-Block Continuous Modulation Buses
SilverWave decouples **Onset Trigger Scheduling** from **Continuous Parameter Evaluation**:
1. **Implicit Continuous Signal Promotion**: Whenever a control method (`.lpf()`, `.pan()`, `.detune()`, `.crush()`, `.shape()`, `.room()`, `.delaytime()`, `.pitch()`, etc.) receives a continuous signal pattern (`sine`, `cosine`, `saw`, `tri`, `perlin`, `slider()`, `cc()`, `pitchbend()`, or any expression marked with `.continuous()` / `*~`), the compiler binds that parameter to a **Per-Stream / Per-Voice Continuous Modulation Lane** without slicing the carrier pattern's `whole` onset span.
2. **Explicit Shared Control Buses (`.bus(id, pat)` / `^bus`)**: Compatible with Tidal's `lpfbus` / `^name` syntax, allowing multiple streams and orbits to share a single named modulation bus.
3. **64-Sample Control-Rate (`k-rate`) & Sample-Accurate (`a-rate`) Evaluation**: Inside `silverwave-dsp`, every sustaining voice evaluates its attached modulation lanes at every 64-sample sub-block (`1.33 ms` at `48 kHz`) with linear SIMD ramp interpolation down to individual samples (`a-rate`), and zero-allocation dirty-bit suppression so unchanged parameters incur zero overhead.
4. **100% Modulatable DSP Parameter Matrix**: Every synth, sampler, filter, distortion, delay, reverb, and granular parameter in SilverWave accepts continuous modulation — eliminating SuperDirt's 41 non-modulatable parameter restriction ([SuperDirt #190](https://github.com/musikinformatik/SuperDirt/issues/190)).

### 2.3 Rust Trait & Type Contracts (`silverwave-core` & `silverwave-dsp`)

```rust
use crate::rational::Rational64;

/// Rate at which a continuous control signal is sampled and ramped on active voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulationRate {
    /// Sampled only at `hap.has_onset()` (legacy Strudel/Tidal discrete behavior).
    OnsetOnly,
    /// Evaluated every 64-sample sub-block (`~750 Hz` at 48 kHz) with linear per-sample SIMD ramp.
    ControlRateSubBlock,
    /// Evaluated at full audio sample rate (`48 kHz` / `96 kHz`) for FM/AM/ring-mod/waveshaping.
    AudioRateSampleExact,
}

/// Identifies either a voice-local modulation lane or a globally shared named bus (`^bus`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlBusId {
    VoiceLocal { stream_id: u16, param: ModulatableParam },
    NamedSharedBus { bus_index: u16 },
}

/// Complete enumeration of all modulatable DSP parameters (zero non-modulatable parameters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModulatableParam {
    Gain,
    Pan,
    NoteSemitones,
    SpeedRatio,
    PitchShiftSemitones,
    DetuneCents,
    LpfCutoffHz,
    LpfResonanceQ,
    HpfCutoffHz,
    HpfResonanceQ,
    BpfCutoffHz,
    BpfResonanceQ,
    VowelFormantMorph,
    CrushBits,
    CoarseDownsample,
    ShapeDrive,
    DistortDrive,
    RingModFreqHz,
    RingModDepth,
    FmIndex,
    FmRatio,
    WavetablePosition,
    UnisonSpread,
    DelaySend,
    DelayTimeSec,
    DelayFeedback,
    ReverbRoomSend,
    ReverbSize,
    ReverbDamping,
    PhaserRateHz,
    PhaserDepth,
    LeslieSpeed,
    GrainDensityHz,
    GrainDurationSec,
    GrainPositionNormalized,
    OscillatorMetaZ1,
    OscillatorMetaZ2,
    OscillatorMetaZ3,
    OscillatorMetaZ4,
}

/// Pre-evaluated 64-sample control frame pushed from the scheduler into the lock-free voice bus table.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlSubBlockFrame {
    pub bus_id: ControlBusId,
    pub start_sample_frame: u64,
    pub sub_block_len: u16,
    pub start_value: f32,
    pub end_value: f32,
    pub rate: ModulationRate,
}

pub trait ContinuousModulationEngine: Send + Sync + 'static {
    /// Evaluates all active continuous modulation lanes over `[block_start_cycle, block_end_cycle)`
    /// into pre-allocated `ControlSubBlockFrame` slices without triggering new voice onsets.
    fn evaluate_modulation_sub_blocks(
        &self,
        block_start_cycle: Rational64,
        block_end_cycle: Rational64,
        sample_frame_start: u64,
        frames_in_block: usize,
        out_frames: &mut [ControlSubBlockFrame],
    ) -> usize;
}
```

---

## 3. Deterministic Cycle-Boundary State, Glides, Ties & Dual-Timeframe Controllers (Solves PP-03, PP-04, PP-05)

### 3.1 Architectural Problem Statement
1. **Stateless Horizon (`PP-03`)**: Because `Pattern::query_arc` is a pure function of `TimeSpan`, users cannot write accumulators (`x = x + 1` per event), stateful Markov chains, L-systems, or cellular automata that evolve across cycle boundaries without external hacks ([Strudel Disc #1201](https://github.com/tidalcycles/strudel/discussions/1201), [Strudel Disc #646](https://github.com/tidalcycles/strudel/discussions/646), [Uzu #3353](https://uzu.lurk.org/t/generative-strategies-and-workflows/3353)). Similarly, cross-event pitch portamento (`.glide()`) and cycle-spanning musical ties (`_`) fail because events have no awareness of predecessor/successor voice identity across cycle boundaries ([Strudel #561](https://github.com/tidalcycles/strudel/issues/561), [Strudel Disc #1254](https://github.com/tidalcycles/strudel/discussions/1254)).
2. **Time-Warped Controller Desync (`PP-04`)**: In [Tidal Issue #1029](https://github.com/tidalcycles/Tidal/issues/1029), applying `slow 2` to a pattern scales the query `TimeSpan` by `1/2` before querying `cF "knob"`, so a slowed pattern reads historical knob values rather than the live hardware knob state.
3. **Exponential $O(N^2)$ Merge Lag (`PP-05`)**: In [Tidal Issues #636](https://github.com/tidalcycles/Tidal/issues/636) and [#641](https://github.com/tidalcycles/Tidal/issues/641), chaining multiple `sometimesBy` / `degradeBy` modifiers with `#` control merges duplicates event lists and performs $O(N^2)$ pairwise intersection checks.

### 3.2 SilverWave Solutions
1. **Epoch-Checkpointed `StatefulPattern<S, T>`**:
   - Maintains a lock-free ring of cycle-boundary state snapshots `S` indexed by integer cycle number `c`.
   - Querying any arc `[begin, end)` looks up the nearest checkpoint $c_0 \le \lfloor \text{begin} \rfloor$ in $O(1)$ time and evolves state deterministically forward, preserving referential transparency for identical `(cycle, seed)` pairs while enabling `.accum(init, fn)`, `.scan(init, fn)`, `.markov(matrix)`, and `.lsystem(axiom, rules)`.
2. **Cross-Cycle Musical Ties (`_`) & Pitch Glide (`.glide(cycles, curve)`)**:
   - Mini-notation `_` extends the `whole.end` of the preceding `Hap` across step and cycle boundaries (`"c4 _ _ d4"` produces a single `Hap` with `whole = [0, 3/4)`).
   - `.glide(time_cycles)` tags consecutive `Hap` events on the same stream/voice lane with `GlideTarget { prev_note, target_note, glide_duration, curve }`, smoothly slewing oscillator/sample pitch without retriggering the amplitude envelope in legato mode.
3. **Global vs. Local Controller Timeframes**:
   - `QueryContext` carries both `local_span: TimeSpan` (transformed by `fast`/`slow`/`early`/`late`) and `global_span: TimeSpan` (unscaled wall-clock cycle time).
   - Hardware/UI controllers (`cc()`, `slider()`, `cF()`, `^var`) read from `global_span` by default (resolving [Tidal #1029](https://github.com/tidalcycles/Tidal/issues/1029)), while `.localTime()` / `^^var` explicitly opts into pattern-warped time.
4. **$O(N \log N)$ Sweep-Line `Hap` Intersection**:
   - Control pattern merging (`#`, `|>`, `|<`, `struct`) sorts candidate arcs by `part.begin` inside stack-allocated `SmallVec<[Hap<T>; 16]>` buffers and performs a single monotonic sweep-line join in $O((N + M) \log (N + M))$ worst-case time.

### 3.3 Rust Trait & Type Contracts (`silverwave-core`)

```rust
use crate::rational::{Rational64, TimeSpan};

/// Execution context passed through `Pattern::query_arc_with_context`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryContext {
    /// TimeSpan after pattern-local time transformations (`fast`, `slow`, `rev`, `late`).
    pub local_span: TimeSpan,
    /// Unwarped global scheduler cycle TimeSpan for hardware CC / slider / Link synchronization.
    pub global_span: TimeSpan,
    /// Deterministic PRNG seed for reproducible randomness and offline bounces.
    pub session_seed: u64,
    /// Stream slot index (`0..64`) for voice-continuation & state checkpoint lookup.
    pub stream_slot: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlideCurve {
    Linear,
    Exponential,
    EqualPowerScurve,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegatoGlideSpec {
    pub duration_cycles: Rational64,
    pub curve: GlideCurve,
    pub legato_tie: bool,
}

/// Deterministic cycle-checkpointed state store for `accum`, `scan`, `markov`, and `lsystem`.
pub trait CycleStateCheckpointer<S: Clone + Send + Sync + 'static>: Send + Sync + 'static {
    /// Retrieves state at the start of `cycle_index`, computing and caching intermediate
    /// checkpoints from the nearest prior snapshot if needed.
    fn state_at_cycle(&self, cycle_index: i64, step_fn: &dyn Fn(i64, &S) -> S) -> S;

    /// Resets state checkpoints to `initial_state` at `at_cycle`.
    fn reset_state(&self, initial_state: S, at_cycle: i64);
}
```

---

## 4. Compiler Diagnostics, Universal Pattern Lifting & Fault-Tolerant Evaluation (Solves PP-06, PP-07)

### 4.1 Architectural Problem Statement
1. **Unclosed Delimiter End-of-String Blame (`PP-06`)**: In Strudel ([Uzu #6135](https://uzu.lurk.org/t/contribution-error-handling/6135), [Strudel PR #1315](https://github.com/tidalcycles/strudel/pull/1315)), forgetting a closing bracket in `s("bd [sd hh")` points the syntax error at the end of the file instead of the opening `[`.
2. **Untyped Chaining & Silent Typos (`PP-06`, `PP-07`)**: Strudel's `composify` dynamically patches `Pattern.prototype`, leaking memory (`composifiedRegistry`, [Strudel #487](https://github.com/tidalcycles/strudel/issues/487), [Strudel #1266](https://github.com/tidalcycles/strudel/issues/1266)) and allowing typos like `.rom(0.8)` to fail at runtime or silently do nothing.
3. **Unpatterned Arguments & GHC Typeclass Errors (`PP-06`)**: In TidalCycles, users hit `No instance for (IsString Double)` ([Uzu #2767](https://uzu.lurk.org/t/using-markovpat-for-chords/2767)), `No instance for (Num Bool)` ([Uzu #4702](https://uzu.lurk.org/t/boolean-midi-controls-for-more-efficient-code/4702)), and cannot pass patterns as the cycle arguments of `whenmod` ([Uzu #2722](https://uzu.lurk.org/t/whenmod-8-5-6-5-possible/2722)) or function patterns to `arpWith` ([Uzu #5104](https://uzu.lurk.org/t/patterning-function-parameters/5104), [Tidal #620](https://github.com/tidalcycles/Tidal/issues/620)).
4. **Whole-Buffer Evaluation Fragility (`PP-07`)**: In Strudel ([Strudel #34](https://github.com/tidalcycles/strudel/issues/34), [Codeberg PR #1054](https://codeberg.org/uzu/strudel/pull/1054)), a single typo on line 40 prevents `Ctrl+Enter` from updating lines 1–39 and disrupts collaborative sessions.

### 4.2 SilverWave Solutions
1. **Delimiter-Stack Span Tracking**: The recursive-descent/Pratt parser pushes every opening delimiter (`[`, `<`, `{`, `(`, `"`, `` ` ``) onto an explicit `DelimiterStack` with its exact `SourceSpan`. On premature EOF or mismatched closing delimiter, the diagnostic primary span highlights the **unclosed opening delimiter** and emits a secondary span at the expected closure site.
2. **Damerau-Levenshtein Method & Control Suggestions**: Every method/control name is checked against the static compile-time symbol table; unknown names within edit distance $\le 2$ (e.g., `.rom(0.5)` $\rightarrow$ `.room(0.5)`, `.lfp(400)` $\rightarrow$ `.lpf(400)`) produce a structured diagnostic with a one-click LSP/GUI/MCP code-action replacement.
3. **Universal `IntoPattern<T>` Argument Lifting**: Every combinator in `silverwave-core` (`every`, `whenmod`, `euclid`, `ply`, `chop`, `striate`, `arp_with`, `markov`, `while_pat`) accepts `impl IntoPattern<T>` in **every** parameter position via automatic `inner_join` lifting — scalars (`4`, `0.5`, `true`), mini-notation strings (`"<4 8>"`), and dynamic patterns (`slider()`, `cI()`) work uniformly without typeclass boilerplate.
4. **Fault-Isolated Per-Stream AST Partitioning**: Full-buffer evaluation partitions the source file into independent top-level stream declarations (`d1..d16`, `$name`, `$:`). If stream `d2` contains a syntax or type error:
   - All valid streams (`d1`, `d3`, etc.) compile and swap into the scheduler on the next quantized grid boundary.
   - Stream `d2` continues playing its **last known valid pattern** uninterrupted while displaying an inline error badge in the editor gutter and `silverwave://session/streams`.

### 4.3 Rust Trait & Type Contracts (`silverwave-mini` & `silverwave-lsp`)

```rust
use crate::span::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichCompilerDiagnostic {
    pub error_code: &'static str,
    pub message: String,
    /// Primary span (e.g. the exact opening `[` for an unclosed sub-cycle group).
    pub primary_span: SourceSpan,
    /// Optional secondary span (e.g. where a mismatched `}` was encountered).
    pub secondary_span: Option<(SourceSpan, String)>,
    /// Damerau-Levenshtein symbol suggestion (e.g. `Some(".room")`).
    pub did_you_mean: Option<String>,
    /// Affected stream ID if isolated to a single stream block (`Some("d2")`).
    pub isolated_stream_id: Option<String>,
}

/// Fault-isolated compilation output allowing valid streams to update even when sibling blocks fail.
#[derive(Debug, Clone)]
pub struct FaultTolerantCompileResult {
    pub updated_streams: Vec<String>,
    pub retained_previous_streams: Vec<String>,
    pub diagnostics: Vec<RichCompilerDiagnostic>,
}
```

---

## 5. Linear Song Arrangement, Quantized One-Shots & Pattern-Triggered Transitions (Solves PP-08)

### 5.1 Architectural Problem Statement
1. **`arrange([0, pat])` Divide-by-Zero & Static Cycle Counts**: In Strudel ([Codeberg PR #508](https://codeberg.org/uzu/strudel/pulls/508), [Strudel Disc #629](https://github.com/tidalcycles/strudel/discussions/629)), setting a section's cycle count to `0` in `arrange([0, intro], [8, verse])` to skip that section during rehearsal causes a divide-by-zero crash if all active sections sum to `0` or misbehaves when modulated by a `slider()`.
2. **Missing Quantized One-Shots (`once` / `asap`) & Progressive Fade-Outs**: Users frequently request Tidal's `once` / `asap` one-shot evaluation and linear timeline fade-outs (`fadeOut`, `fadeIn`) in Strudel ([Uzu #5864](https://uzu.lurk.org/t/once-or-asap-in-strudel/5864), [Uzu #6215](https://uzu.lurk.org/t/strudel-how-to-do-a-progressive-fade-out/6215), [Uzu #5777](https://uzu.lurk.org/t/pattern-sequencing-in-strudel-how-to-play-full-cycles-sequentially/5777)).
3. **Transitions Trapped in `IO` ([Tidal #716](https://github.com/tidalcycles/Tidal/issues/716))**: In TidalCycles, transitions (`xfadeIn`, `clutchIn`, `anticipate`) are top-level GHCi `IO` actions that cannot be composed inside `ur`, `seqP`, or triggered via pattern events / MIDI pads ([Uzu #5461](https://uzu.lurk.org/t/i-want-tips-for-structuring-my-first-tidalcycles-piece/5461)).

### 5.2 SilverWave Solutions
1. **Zero-Safe Dynamic `arrange` & `seq_p`**:
   - Sections with `cycles <= 0` are cleanly skipped (treated as zero-duration bypassed sections so users can toggle `[0, intro]` or `[slider(8, 0, 16, 1), verse]` live without divide-by-zero). If all sections have `cycles <= 0`, `arrange` returns `Pattern::silence()`.
2. **First-Class `once(pat)`, `asap(pat)`, `qtrigger(pat)`, `fade_in(cycles)` & `fade_out(cycles)`**:
   - `asap(pat)` plays `pat` for exactly 1 cycle starting immediately at the current fractional cycle offset `[t_now, t_now + 1)`.
   - `once(pat)` plays `pat` for exactly 1 cycle quantized to the next cycle boundary `[\lceil t_{now} \rceil, \lceil t_{now} \rceil + 1)`.
   - `qtrigger(stream_id, pat)` aligns pattern local cycle `0` to the next integer cycle boundary without resetting the global session clock.
   - `.fade_out(cycles)` and `.fade_in(cycles)` apply a one-shot equal-power cosine gain envelope anchored to the evaluation start cycle, automatically silencing the stream upon completion of `.fade_out`.
3. **Pure Pattern-Level Transition Combinators**:
   - All 11 Tidal transitions (`xfade`, `clutch`, `anticipate`, `interpolate`, `jump_in`, `jump_mod`, `histpan`, `wait`, `wash`, `mortal`) are implemented as pure `Pattern<ControlMap>` combinators (`Pattern::transition_to(old, new, start_cycle, spec)`), allowing them to be nested inside `arrange`, `ur`, `seq_p`, or triggered by incoming MIDI/OSC events.

---

## 6. Multi-Stage Voice FX Chains, Seeded FDN Reverb & True Granular DSP (Solves PP-09, PP-10)

### 6.1 Architectural Problem Statement
1. **Single-Slot FX Overwrite & Multi-Orbit Volume Multiplication (`PP-09`)**: In Strudel ([Strudel Effects Docs](https://strudel.cc/learn/effects/)), parameters are stored in a flat dictionary per `Hap`, so `.lpf(200).distort(3).lpf(1200)` overwrites `lpf: 200` with `1200` instead of placing a pre-distortion filter and a post-distortion filter in series. Passing `.orbit("1,2,3")` duplicates the full dry voice into 3 orbits, tripling output amplitude.
2. **Main-Thread Unseeded Reverb IR Stalls (`PP-09`)**: `reverbGen.mjs` regenerates multi-second convolution buffers on the main thread using `Math.random()` whenever `roomsize` changes ([Strudel Effects Docs](https://strudel.cc/learn/effects/), [Stave #1665](https://github.com/MrityunjayBhardwaj/Stave/issues/1665)), causing audible glitches and breaking deterministic offline rendering.
3. **First-Hit Sample Silence, Stale Cache, `chop(64)` Flood & Cut-Group Race (`PP-10`)**:
   - Lazy HTTP sample fetching causes the first hit of a new sample bank to play silent ([Strudel #187](https://github.com/tidalcycles/strudel/issues/187)), while a 30-day `CacheFirst` Service Worker serves stale `strudel.json` manifests ([Strudel #1218](https://github.com/tidalcycles/strudel/issues/1218)).
   - `.chop(64)` / `.striate(128)` generates hundreds of scheduler `Hap` nodes per cycle ([Reddit Granular Thread](https://www.reddit.com/r/livecoding/comments/1nzeq8u/granular_synthesis_in_strudel/)).
   - Simultaneous events in the same `cut` group race and fail to choke one another deterministically ([SuperDirt #282](https://github.com/musikinformatik/SuperDirt/issues/282)).
   - Loading large sample folders exhausts SuperCollider's `numBuffers` / 2 GB real-time `memSize` pool ([SuperDirt #156](https://github.com/musikinformatik/SuperDirt/issues/156), [SuperDirt #255](https://github.com/musikinformatik/SuperDirt/issues/255), [Uzu #3148](https://uzu.lurk.org/t/superdirt-lazy-samples-loading/3148?page=2)).

### 6.2 SilverWave Solutions
1. **Ordered Multi-Stage Per-Voice Insert FX Pipeline (`SmallVec<[InsertFxStage; 8]>`)**:
   - Calling `.lpf(200).distort(3.0).lpf(1200)` appends three ordered `InsertFxStage` nodes (`SvfLowPass(200 Hz) -> Waveshaper(3.0) -> SvfLowPass(1200 Hz)`) to the voice's inline DSP chain, executing in exact user-written call order.
   - Multi-orbit routing (`.orbit("1, 2, 3")` or `.orbitSend([(1, 0.8), (2, 0.4)])`) separates dry voice summation from per-orbit auxiliary send levels with automatic $1/\sqrt{N}$ equal-power gain compensation.
2. **Deterministic Seeded 8x8 Hadamard FDN Reverb + Dual-Buffer Crossfaded IR Convolution**:
   - Default orbit reverb uses an algorithmic 8-line Feedback Delay Network (FDN) whose `size`, `damping`, `predelay`, and `modulation` parameters modulate continuously at `k-rate` with zero buffer regeneration and zero clicks.
   - When custom IR convolution is selected, background workers pre-compute partitioned FFT IRs seeded deterministically by `session_seed`, swapping IRs via a `50 ms` equal-power dual-convolver crossfade.
3. **Dedicated Real-Time Granular Voice Engine**:
   - `.chop(n)`, `.striate(n)`, `.scrub(pos)`, and `.grain(density, size, scatter, pitch)` execute inside a dedicated `GranularSamplerVoice` in `silverwave-dsp` using a single parent `Hap` plus continuous grain-window scheduling (supporting up to 512 simultaneous Hann/Tukey-windowed grains per stream with zero scheduler `Hap` bloat).
4. **AST Pre-Roll Sample Pre-Fetcher, Disk/Mmap Streaming & Deterministic Cut Groups**:
   - Before quantized pattern activation (`next_cycle`), the compiler extracts all static sample bank literals referenced in the AST and dispatches high-priority pre-fetch/decode tasks during the lookahead window (eliminating first-hit silence, [Strudel #187](https://github.com/tidalcycles/strudel/issues/187)).
   - Remote `strudel.json` manifests use `StaleWhileRevalidate` with SHA-256 ETag validation (fixing [Strudel #1218](https://github.com/tidalcycles/strudel/issues/1218)).
   - Samples shorter than `10 seconds` are cached in lock-free RAM slabs; long stems/breaks (`> 10 seconds`) use memory-mapped (`mmap`) ring-buffered streaming so 50+ GB sample libraries never hit SuperCollider's 2 GB `memSize` ceiling ([SuperDirt #255](https://github.com/musikinformatik/SuperDirt/issues/255)).
   - Simultaneous events on the same `(cut_group, sample_frame)` resolve deterministically by `(stream_priority, hap_ordinal)` with a `2.5 ms` linear de-click fade-out on choked voices (fixing [SuperDirt #282](https://github.com/musikinformatik/SuperDirt/issues/282)).

### 6.3 Rust Trait & Type Contracts (`silverwave-dsp`)

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InsertFxStage {
    LowPass { cutoff_hz: f32, q: f32 },
    HighPass { cutoff_hz: f32, q: f32 },
    BandPass { cutoff_hz: f32, q: f32 },
    Notch { cutoff_hz: f32, q: f32 },
    VowelFormant { morph: f32 },
    Distort { drive: f32, mode: DistortionMode },
    Bitcrush { bits: f32, downsample: f32 },
    RingMod { freq_hz: f32, depth: f32 },
    Compressor { threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistortionMode {
    SoftClipTanh,
    HardClip,
    WavefoldSaturate,
    AsymmetricTube,
}

/// Ordered per-voice insert FX chain preserving repeated filter/distortion stages in exact call order.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedVoiceFxChain {
    pub stages: [Option<InsertFxStage>; 8],
    pub stage_count: u8,
}
```

---

## 7. Unified Harmony Dictionary & `StrudelDirt` / `SuperDirt` Profile Bridge (Solves PP-11)

### 7.1 Architectural Problem Statement
1. **Chord Symbol Breakage (`PP-11`)**: Switching Strudel's default chord dictionary to `ireal` ([Codeberg PR #967](https://codeberg.org/uzu/strudel/pull/967)) broke standard `dim`/`dim7` chords ([Strudel #1265](https://github.com/tidalcycles/strudel/issues/1265)), rejected Tidal chord symbols like `Gm13` or `A7s5` ([Uzu #5748](https://uzu.lurk.org/t/a-full-list-of-supported-chords-or-a-way-to-import-custom-chord-definitions-in-strudel/5748)), and prevented mixing simple triads with jazz voicings inside a single pattern ([Strudel #316](https://github.com/tidalcycles/strudel/issues/316)).
2. **`StrudelDirt` vs `SuperDirt` Semantic Divergence (`PP-11`)**: As documented in [SuperDirt Issue #302](https://github.com/musikinformatik/SuperDirt/issues/302), Strudel (`superdough`) and Tidal (`SuperDirt`) disagree on whether middle C (`note 0`) is `C4` (`261.63 Hz`) or `C5` (`523.25 Hz`), whether filter envelopes default to the amplitude ADSR envelope, how sustained synth gain is calibrated relative to percussion (`0.5x`), and how oscillator macro parameters (`z1..z4`) are mapped.

### 7.2 SilverWave Solutions
1. **Unified Superset Chord & Voicing Parser**:
   - SilverWave's chord parser natively accepts **all three notation dialects simultaneously**: `ireal` (`C^7`, `Co7`, `C-7`, `C7b9`), standard music theory (`Cmaj7`, `Cdim`, `Cdim7`, `Cm7b5`, `Caug`), and TidalCycles symbols (`c'maj7`, `gm13`, `a7s5`, `dm9`).
   - Individual chord tokens inside a single mini-notation pattern can specify an inline dictionary or voicing style suffix (`"<Cmaj7@lefthand Dm9@ireal G13@drop2 C6@triad>"`), resolving [Strudel #316](https://github.com/tidalcycles/strudel/issues/316).
2. **Configurable `EngineCompatibilityProfile` (`SuperDoughC4` vs `SuperDirtC5`)**:
   - Selectable per-session or per-stream via `.profile("strudel")` / `.profile("superdirt")`, reconciling every divergence tracked in [SuperDirt #302](https://github.com/musikinformatik/SuperDirt/issues/302):
     - `middle_c_octave`: `4` (`261.63 Hz` for `note 0` in Strudel mode) or `5` (`523.25 Hz` in SuperDirt mode).
     - `filter_env_inheritance`: `InheritAmpAdsrWhenUnset` (Strudel) or `IndependentOnly` (SuperDirt).
     - `sustained_synth_gain_scale`: `0.5` (`-6.02 dB` auto-headroom for sustained synths) or `1.0`.
     - `oscillator_meta_params`: Exposes `z1..z4` Braids/Elektron-style timbral macro controls across all oscillators.

---

## 8. Sample-Accurate MIDI Engine, Dual Sink Routing & Native CLAP/VST3 DAW Plugin (Solves PP-12)

### 8.1 Architectural Problem Statement
1. **MIDI Clock Drift & Ordering Bugs**: WebAudio vs `performance.now()` clock skew causes 15 ms/min MIDI drift ([Strudel #119](https://github.com/tidalcycles/strudel/issues/119)); `noteOffsetMs: 10` in [`midi.mjs`](https://github.com/calvinw/strudel-llm-mirror/blob/main/packages/midi/midi.mjs) drops rapid notes; `sendNote` executes before `sendProgramChange` ([Strudel #192](https://github.com/tidalcycles/strudel/issues/192), [Strudel #1175](https://github.com/tidalcycles/strudel/issues/1175)); and `.midi()` silences internal synthesis unless duplicated with `.superimpose()` ([Uzu #5718](https://uzu.lurk.org/t/webaudio-playing-while-midi-is-also-being-sent/5718)).
2. **DAW Integration & Multichannel Routing Friction**: Routing Strudel or TidalCycles into Ableton Live, Bitwig, Reaper, or Logic requires virtual audio cables (BlackHole, JACK, LoopBe) and manual 32-channel `startup.scd` tweaks ([Uzu #6015](https://uzu.lurk.org/t/how-to-get-multichannel-audio-into-daw/6015), [SuperDirt #156](https://github.com/musikinformatik/SuperDirt/issues/156), [Uzu #4861](https://uzu.lurk.org/t/the-supedirtmixer-v1-0-is-out-now/4861)).

### 8.2 SilverWave Solutions
1. **Hardware-Sample-Clocked MIDI Scheduler & External MIDI Clock Slave PLL**:
   - Outbound MIDI events and 24 PPQN `0xF8` Timing Clock ticks are scheduled against the exact hardware audio sample frame counter (zero drift relative to internal DSP).
   - Inbound 24 PPQN MIDI clock drives a 2nd-order Phase-Locked Loop (PLL) filter so SilverWave can lock to external hardware sequencers (Elektron, MPC, modular clocks) with $< 0.2\text{ ms}$ jitter.
2. **Deterministic MIDI Message Priority & Zero-Hack Retrigger State Machine**:
   - Within the same timestamp, outbound MIDI messages are strictly ordered:
     $$\text{Active Voice Retrigger } \texttt{NoteOff} \rightarrow \texttt{CC 0 (Bank MSB)} \rightarrow \texttt{CC 32 (Bank LSB)} \rightarrow \texttt{ProgramChange} \rightarrow \texttt{Parameter CCs / PitchBend} \rightarrow \texttt{NoteOn}$$
   - Eliminates the `10 ms` note-duration subtraction hack: if a note is retriggered on the same `(port, channel, note)` before its prior `NoteOff`, the state machine emits a sample-accurate `NoteOff` immediately prior to the new `NoteOn`.
   - `.midi("port")` supports `.keepAudio(true)` (or `.midiAndAudio("port")`) so performers can layer hardware synths and internal DSP without `.superimpose()` boilerplate ([Uzu #5718](https://uzu.lurk.org/t/webaudio-playing-while-midi-is-also-being-sent/5718)).
3. **Native CLAP & VST3 DAW Plugin (`silverwave-plugin`)**:
   - Compiles SilverWave as a native **CLAP** (`org.silverwave.live`) and **VST3** instrument/effect plugin via `nih-plug` architecture:
     - **Host Transport Sync**: Locks cycle phase (`cycle_rational`), `cps`/`bpm`, bar position, and play/stop state directly to the DAW's `ProcessContext::transport()` sample position.
     - **16 Stereo Output Buses (32 Channels)**: Exposes Orbit `0` (Main Stereo) plus Orbits `1..15` as 15 auxiliary stereo output buses directly into the DAW mixer — zero BlackHole/JACK virtual cables required.
     - **Sample-Accurate DAW Automation & Note Output**: Exposes 64 automatable macro parameters (`slider(0..63)`) to DAW automation lanes with CLAP polyphonic parameter modulation support, plus sample-accurate MIDI Note/CC output to downstream DAW tracks.
     - **Co-Performance Bridge Inside the DAW**: The plugin embeds the `silverwave-gui` editor view AND runs the `silverwave-mcp` / `silverwave-lsp` co-performance socket so external editors (VS Code, Neovim) and MCP agents can live-code directly inside an open DAW project.

### 8.3 Rust Trait & Type Contracts (`silverwave-io` & `silverwave-plugin`)

```rust
/// Strict intra-timestamp ordering priority for outbound MIDI messages (fixes Strudel #192 & #1175).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum MidiEventOrderingPriority {
    RetriggerNoteOff = 0,
    BankSelectMsbCc0 = 1,
    BankSelectLsbCc32 = 2,
    ProgramChange = 3,
    ControlChangeAndPitchBend = 4,
    NoteOn = 5,
    ScheduledNoteOff = 6,
}

/// Native CLAP / VST3 DAW plugin descriptor contract (`silverwave-plugin`).
#[derive(Debug, Clone, PartialEq)]
pub struct DawPluginConfig {
    pub plugin_id: &'static str,
    pub stereo_orbit_buses: u8,          // 16 stereo buses (32 audio output channels)
    pub exposed_macro_params: u8,        // 64 DAW-automatable slider/control parameters
    pub sync_to_host_transport: bool,    // Locks rational cycle phase to DAW bar/beat PPQ
    pub emit_sample_accurate_midi: bool, // Outputs pattern MIDI events to host instrument tracks
    pub enable_mcp_and_lsp_bridge: bool, // Allows MCP agents & external editors to attach to DAW instance
}
```

---

## 9. TDD Verification Matrix — 1:1+ Pain-Point Regression & Conformance Contracts

Every pain-point resolution specified in this document is gated by deterministic unit, property-based (`proptest`), and real-time audio callback tests.

```rust
#[cfg(test)]
pub mod pain_point_resolution_tdd_contracts {
    use super::*;

    /// [PP-01 / PP-02] Verifies that `s("pad").note("c3").lpf(sine.range(200, 4000))`
    /// triggers exactly 1 voice onset over a 4-cycle sustain while updating the SVF cutoff
    /// every 64 samples along the sine trajectory, and verifies all 39 ModulatableParam variants.
    pub fn test_continuous_krate_filter_sweep_without_onset_retrigger();

    /// [PP-03] Verifies `StatefulPattern` (`accum`, `markov`, `lsystem`) produces identical
    /// values whether queried sequentially `[0,1), [1,2), [2,3)` or out-of-order `[2,3)` via
    /// cycle checkpoint lookup, and verifies cross-cycle `_` ties and `.glide(0.25)` portamento.
    pub fn test_stateful_pattern_checkpoint_determinism_and_cross_cycle_ties();

    /// [PP-04] Regression test for Tidal #1029: verifies `d1: s("bd*4").lpf(cF("knob"))` and
    /// `d2: s("sd*4").slow(4).lpf(cF("knob"))` read the exact same hardware knob value at wall-clock cycle T.
    pub fn test_global_timeframe_controller_sync_under_slow_and_fast();

    /// [PP-05] Regression test for Tidal #636/#641: stacks 12 layers of `sometimesBy` + `#`
    /// control merges over 64 events/cycle and asserts query execution completes in < 250 microseconds.
    pub fn test_sweep_line_hap_merge_prevents_exponential_randomness_lag();

    /// [PP-06 / PP-07] Regression test for Strudel #1315/#487/#34: verifies unclosed `[` blames the
    /// opening bracket span, `.rom(0.5)` suggests `.room(0.5)`, `whenmod` accepts patterned cycle counts,
    /// and a syntax error in `d2:` does not interrupt playback or updates of `d1:`.
    pub fn test_opening_bracket_blame_typo_hint_and_fault_isolated_stream_eval();

    /// [PP-08] Regression test for Strudel PR #508 & Disc #629: verifies `arrange([0, patA], [4, patB])`
    /// and `arrange([0, patA], [0, patB])` never divide by zero, supports `slider()` durations,
    /// and verifies quantized `once()`, `asap()`, `fade_out(4)`, and pattern-triggered transitions.
    pub fn test_zero_safe_arrange_once_asap_and_pattern_transitions();

    /// [PP-09] Verifies `.lpf(200).distort(3.0).lpf(1200)` executes two distinct SVF stages bracketing
    /// the waveshaper, verifies `.orbit("1,2,3")` equal-power normalization, and verifies zero-click
    /// deterministic seeded FDN/convolution reverb under live `roomsize` modulation.
    pub fn test_multi_stage_insert_fx_order_and_deterministic_seeded_reverb();

    /// [PP-10] Regression test for Strudel #187/#1218 & SuperDirt #282/#255: verifies AST pre-roll
    /// sample pre-fetching eliminates cycle-0 silence, verifies granular `.chop(128)` emits 1 scheduler Hap,
    /// and verifies simultaneous same-sample `cut(1)` events deterministically choke without race conditions.
    pub fn test_ast_sample_prefetch_granular_voice_and_deterministic_cut_groups();

    /// [PP-11] Regression test for Strudel #1265/#316 & SuperDirt #302: verifies `dim7`, `o7`, `Gm13`,
    /// and `A7s5` parse in the unified chord table, mixed per-chord `@dict` tags resolve, and
    /// `SuperDoughC4` vs `SuperDirtC5` profiles switch middle-C and filter ADSR inheritance cleanly.
    pub fn test_unified_chord_dictionary_and_superdirt_strudel_profiles();

    /// [PP-12] Regression test for Strudel #119/#192/#1175 & Uzu #5718: verifies sample-accurate
    /// MIDI clock (0.0 ms drift over 60 minutes), strict `Bank -> PC -> CC -> NoteOn` ordering,
    /// zero-drop rapid note retriggers without 10ms offset hacks, and 16-bus CLAP/VST3 DAW transport lock.
    pub fn test_sample_accurate_midi_ordering_and_clap_vst3_daw_transport_sync();
}
```
