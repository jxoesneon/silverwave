# `04-SPEC-IO-STREAMS-AND-TRANSITIONS.md` — `silverwave-io` Specification

**Crate:** `silverwave-io`  
**Status:** Normative Specification (Contract-Only)  
**Target Parity:** [Strudel](https://codeberg.org/uzu/strudel) ([`@strudel/midi` / Input Devices](https://strudel.cc/learn/input-devices/), [`@strudel/csound`](https://strudel.cc/learn/csound/)) & [TidalCycles](https://github.com/tidalcycles/Tidal) ([`Sound.Tidal.Transition`](https://tidalcycles.org/docs/reference/transitions/), [SuperDirt OSC](https://github.com/musikinformatik/SuperDirt), [Ableton Link](https://ableton.github.io/link/))  
**Scope:** Audio/Plugin/Csound backends, `rosc` OSC engine, `midir` MIDI engine, `ableton-link-rs` clock & multi-user sync, stream lifecycle (`d1..d16`, `$:`), all TidalCycles transitions, and TDD verification contracts.

---

## Table of Contents

1. [Architecture & Crate Boundaries](#1-architecture--crate-boundaries)
2. [Audio, Plugin & Csound Backends](#2-audio-plugin--csound-backends)
3. [OSC Engine (`rosc`) — SuperDirt & Custom I/O](#3-osc-engine-rosc--superdirt--custom-io)
4. [MIDI Engine (`midir`) — `@strudel/midi` Parity & Note-Off Safety](#4-midi-engine-midir--strudelmidi-parity--note-off-safety)
5. [Clock, Tempo & Ableton Link Sync (`ableton-link-rs`)](#5-clock-tempo--ableton-link-sync-ableton-link-rs)
6. [Stream Management & Transitions](#6-stream-management--transitions)
7. [TDD Verification & Test Contracts](#7-tdd-verification--test-contracts)
8. [1:1+ & Category-King I/O, MIDI, DAW Plugin & Multiplayer Extensions](#8-11--category-king-io-midi-daw-plugin--multiplayer-extensions-pp-07-pp-08-pp-12-ck-03-ck-09)

---

## 1. Architecture & Crate Boundaries

`silverwave-io` bridges the pure-functional pattern engine (`silverwave-core`) with real-time audio hardware, DAW hosts, Csound synthesis, UDP/OSC networks, MIDI ports, and Ableton Link peers.

```text
+-------------------------------------------------------------------------+
|                         silverwave-core (Pure)                          |
|   Pattern<ControlMap>  |  Arc<Rational>  |  Happening<T>  |  StateMap   |
+-----------------------------------+-------------------------------------+
                                    | query(Arc, StateMap) -> Vec<Hap>
                                    v
+-------------------------------------------------------------------------+
|                       silverwave-io (Real-Time I/O)                     |
|                                                                         |
|  +-----------------------+    +--------------------------------------+  |
|  |   StreamRegistry      |    |   ClockEngine (ableton-link-rs)      |  |
|  |   d1..d16, $:, mute,  |<-->|   CPS / BPM / Quantum / Phase        |  |
|  |   solo, hush, panic   |    |   Multi-peer Link + Host Transport   |  |
|  +-----------+-----------+    +--------------------------------------+  |
|              |                                                          |
|              | TransitionEngine (xfade, clutch, anticipate, ...)        |
|              v                                                          |
|  +-------------------------------------------------------------------+  |
|  |                        EventDispatcher                            |  |
|  +---------+-----------------+------------------+--------------------+  |
|            |                 |                  |                       |
|            v                 v                  v                       |
|  +-------------------+ +------------+ +------------------+              |
|  | Audio/Plugin/     | | rosc OSC   | | midir MIDI       |              |
|  | Csound Backends   | | Engine     | | Engine           |              |
|  | (cpal, wasm32,    | | /dirt/play | | Out: CC/NRPN/    |              |
|  |  nih-plug,csound) | | /ctrl, etc | | SysEx/Clock/Note |              |
|  +-------------------+ +------------+ | In: midin/keys   |              |
|                                       +------------------+              |
+-------------------------------------------------------------------------+
```

### 1.1 Core Shared Types & Error Contracts

```rust
use std::collections::BTreeMap;
use std::time::Duration;

/// Rational cycle time (inherited from `silverwave-core`).
pub type CycleTime = num_rational::Rational64;

/// Cycle-domain span `[start, stop)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CycleArc {
    pub start: CycleTime,
    pub stop: CycleTime,
}

/// Typed control value carried in `ControlMap`.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Float(f64),
    Int(i64),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Note(MidiNoteValue),
}

pub type ControlMap = BTreeMap<String, ControlValue>;
pub type StateMap = BTreeMap<String, ControlValue>;

/// Triggered pattern event ready for I/O dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct DispatchEvent {
    pub whole: Option<CycleArc>,
    pub part: CycleArc,
    pub value: ControlMap,
    /// Scheduled wall-clock or sample-frame onset in seconds relative to epoch.
    pub onset_secs: f64,
    /// Event duration in seconds (`delta` in SuperDirt).
    pub duration_secs: f64,
    /// Active CPS at event onset.
    pub cps: f64,
    /// Stream identifier (e.g., `"1"` for `d1`, `"drums"` for `p("drums")`).
    pub stream_id: StreamId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(pub String);

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("audio backend error: {0}")]
    Audio(String),
    #[error("plugin host error: {0}")]
    Plugin(String),
    #[error("csound engine error: {0}")]
    Csound(String),
    #[error("osc transport error: {0}")]
    Osc(String),
    #[error("midi port error: {0}")]
    Midi(String),
    #[error("ableton link error: {0}")]
    Link(String),
    #[error("invalid stream id: {0}")]
    StreamNotFound(String),
    #[error("configuration error: {0}")]
    Config(String),
}
```

### 1.2 Backend Dispatch Trait

All output sinks implement a unified, non-blocking `OutputSink` contract so the scheduler tick never blocks on socket, driver, or audio-thread locks.

```rust
pub trait OutputSink: Send + Sync {
    /// Human-readable sink identifier (e.g., `"superdirt"`, `"midi:IAC Bus 1"`).
    fn name(&self) -> &str;

    /// Enqueue a batch of scheduled events for the current scheduler tick.
    /// Must be lock-free or bounded-wait; never blocks the tick loop.
    fn send_batch(&self, events: &[DispatchEvent]) -> Result<(), IoError>;

    /// Immediately silence all active voices/notes on this sink.
    fn panic(&self) -> Result<(), IoError>;

    /// Graceful shutdown and resource release.
    fn close(&self) -> Result<(), IoError>;
}
```

---

## 2. Audio, Plugin & Csound Backends

### 2.1 Common Audio Render Contract

```rust
/// Audio sample format and stream geometry negotiated by any audio backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
}

/// Sample-accurate event handed to the real-time audio callback via SPSC ring buffer.
#[derive(Clone, Debug)]
pub struct AudioThreadEvent {
    /// Sample offset within the target audio buffer `[0, buffer_frames)`.
    pub frame_offset: u32,
    pub duration_frames: u64,
    pub orbit: u16,
    pub controls: ControlMap,
}

/// Real-time synthesis/mixing callback contract (lock-free, zero-allocation on audio thread).
pub trait AudioRenderEngine: Send {
    fn prepare(&mut self, format: AudioFormat);
    fn push_event(&mut self, event: AudioThreadEvent);
    fn process_block(&mut self, output: &mut [f32], frames: usize, channels: usize);
    fn panic_all_voices(&mut self);
}
```

### 2.2 `cpal` Native Backend (`target_arch != "wasm32"`)

| Property | Specification |
| :--- | :--- |
| **Crate** | `cpal` |
| **Thread Safety** | Lock-free SPSC ring buffer (`rtrb` or `ringbuf`) between scheduler thread and `cpal` audio callback. Zero heap allocation, zero mutexes on audio thread. |
| **Clock Source** | Monotonic hardware sample counter (`total_frames / sample_rate`) exposed as an atomic `f64` for sample-accurate `AudioSampleClock` synchronization. |
| **Device Hot-Plug** | Stream error callback triggers non-blocking device re-enumeration and seamless stream rebuild without dropping `StreamRegistry` state. |

```rust
#[derive(Clone, Debug)]
pub struct CpalConfig {
    pub device_name: Option<String>,
    pub preferred_sample_rate: Option<u32>,
    pub preferred_buffer_size: Option<u32>,
    pub channels: u16,
    pub ring_capacity_events: usize,
}

pub trait CpalBackend {
    fn list_output_devices() -> Result<Vec<String>, IoError>;
    fn start(
        config: CpalConfig,
        engine: Box<dyn AudioRenderEngine>,
    ) -> Result<Self, IoError>
    where
        Self: Sized;
    fn format(&self) -> AudioFormat;
    fn sample_clock_secs(&self) -> f64;
    fn event_sender(&self) -> AudioEventProducer;
    fn stop(self) -> Result<(), IoError>;
}

pub trait AudioEventProducer: Send + Sync {
    fn try_push(&self, event: AudioThreadEvent) -> Result<(), IoError>;
}
```

### 2.3 `wasm32` WebAudio / `AudioWorklet` Backend (`target_arch = "wasm32"`)

| Property | Specification |
| :--- | :--- |
| **Runtime** | `web-sys` `AudioContext` + `AudioWorkletNode` (`silverwave-wasm` AudioWorklet processor). |
| **Memory Transport** | Dual-mode: (1) `SharedArrayBuffer` lock-free ring when `crossOriginIsolated == true`; (2) `MessagePort` structured batch fallback when `SharedArrayBuffer` is unavailable. |
| **Clock Bridge** | `AudioContext.currentTime` + `outputLatency` mapped to `CycleTime` with lookahead scheduling (`schedule_ahead_secs`, default `0.1`). |
| **Offline Render** | `OfflineAudioContext` target for deterministic stem/cycle export. |

```rust
#[derive(Clone, Debug)]
pub struct WebAudioConfig {
    pub worklet_url: String,
    pub processor_name: String,
    pub latency_hint_secs: f64,
    pub schedule_ahead_secs: f64,
    pub use_shared_array_buffer: bool,
}

pub trait WebAudioBackend {
    fn initialize(config: WebAudioConfig) -> Result<Self, IoError>
    where
        Self: Sized;
    fn resume(&self) -> Result<(), IoError>;
    fn suspend(&self) -> Result<(), IoError>;
    fn context_time_secs(&self) -> f64;
    fn output_latency_secs(&self) -> f64;
    fn post_event_batch(&self, events: &[AudioThreadEvent]) -> Result<(), IoError>;
    fn render_offline(
        format: AudioFormat,
        duration_secs: f64,
        events: &[AudioThreadEvent],
    ) -> Result<Vec<f32>, IoError>;
}
```

### 2.4 `nih-plug` VST3 / CLAP Plugin Target

| Property | Specification |
| :--- | :--- |
| **Framework** | [`nih_plug`](https://github.com/robbert-vdh/nih-plug) (`Plugin`, `Vst3Plugin`, `ClapPlugin`). |
| **Transport Sync** | Reads `ProcessContext::transport()` per audio buffer (`tempo`, `playing`, `pos_beats`, `pos_samples`, `time_sig_numerator/denominator`). |
| **Sample Accuracy** | Evaluates `Pattern` over the exact `[buffer_start_cycle, buffer_end_cycle)` arc derived from host `pos_beats / bpc` or internal `LinkClock` when host transport is free-running. |
| **MIDI & Params** | Emits `NoteEvent` (`NoteOn`, `NoteOff`, `MidiCC`, `MidiPitchBend`, `PolyModulation`) directly into `nih_plug::context::process::ProcessContext` and reads host automation parameters into `StateMap`. |

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginSyncMode {
    /// Lock cycle phase and CPS strictly to DAW host transport (`pos_beats`, `tempo`).
    HostTransport,
    /// Free-run using internal/Ableton Link clock while rendering into host audio buffers.
    InternalOrLink,
}

#[derive(Clone, Debug)]
pub struct HostTransportSnapshot {
    pub playing: bool,
    pub tempo_bpm: f64,
    pub pos_beats: Option<f64>,
    pub pos_samples: Option<i64>,
    pub time_sig_num: i32,
    pub time_sig_den: i32,
}

pub trait NihPlugBridge {
    fn sync_mode(&self) -> PluginSyncMode;
    fn set_sync_mode(&mut self, mode: PluginSyncMode);
    fn on_transport_update(&mut self, snapshot: &HostTransportSnapshot, sample_rate: f32);
    fn render_plugin_buffer(
        &mut self,
        buffer_frames: usize,
        channels: usize,
        out_audio: &mut [f32],
        out_midi: &mut Vec<PluginMidiEvent>,
    );
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginMidiEvent {
    NoteOn { timing: u32, channel: u8, note: u8, velocity: f32 },
    NoteOff { timing: u32, channel: u8, note: u8, velocity: f32 },
    MidiCC { timing: u32, channel: u8, cc: u8, value: f32 },
    PitchBend { timing: u32, channel: u8, value: f32 },
    PolyAftertouch { timing: u32, channel: u8, note: u8, pressure: f32 },
}
```

### 2.5 Csound Backend (`@strudel/csound` Parity)

Provides 1:1 parity with Strudel's [`@strudel/csound`](https://strudel.cc/learn/csound/) (`loadCsound`, `loadOrc`, `start`, `csound`, `csoundm`, `Mono`/`MonoNote`/`NoiSaw` instrument conventions).

| Strudel `@strudel/csound` Feature | `silverwave-io` Contract |
| :--- | :--- |
| `loadOrc(url \| text)` | `CsoundEngine::load_orc(source)` with `github:` shortcut URL resolution & ORC cache. |
| `start("Instr")` / `stop("Instr")` | Continuous/monophonic instrument lifecycle (`i "Instr" 0 -1` / negative p1 turn-off). |
| `s("Instr").csound()` | Maps `ControlMap` fields (`note`/`freq`, `gain`/`amp`, `pan`, `dur`, `p4..pN`) to Csound score `i` events. |
| `csoundm("Instr")` | Sends MIDI-style note triggers + `chnset` control channel updates (`Instr.param`). |
| Native vs. `wasm32` | Native links `libcsound` C-API; `wasm32` delegates to `@csound/browser` JS bridge. |

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrcSource {
    Inline(String),
    Url(String),
    /// Supports Strudel's `github:user/repo/branch/path.orc` shorthand.
    GithubShorthand(String),
    FilePath(String),
}

pub trait CsoundEngine: OutputSink {
    fn init_csound(&mut self, sample_rate: u32) -> Result<(), IoError>;
    fn load_orc(&mut self, source: OrcSource) -> Result<(), IoError>;
    /// Starts a long-running instrument (e.g., `start("Mono")` -> `i "Mono" 0 -1`).
    fn start_instrument(&self, instrument: &str) -> Result<(), IoError>;
    /// Stops a long-running instrument (`i -p1 0 0`).
    fn stop_instrument(&self, instrument: &str) -> Result<(), IoError>;
    /// Dispatches a Csound score line (`i "Instr" onset_secs dur_secs p4 ...`).
    fn send_score_event(&self, score_line: &str) -> Result<(), IoError>;
    /// Updates a named Csound control channel (`chnset`), e.g. `NoiSaw.cut`.
    fn set_control_channel(&self, channel: &str, value: f64) -> Result<(), IoError>;
}
```

---

## 3. OSC Engine (`rosc`) — SuperDirt & Custom I/O

### 3.1 Target & Scheduling Modes

Mirrors TidalCycles' `Target` and `oSchedule` semantics using `rosc::OscPacket`, `rosc::OscMessage`, and `rosc::OscBundle`.

```rust
use std::net::SocketAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OscScheduleMode {
    /// Send individual `OscMessage`s at wall-clock `onset - latency`.
    Live,
    /// Wrap each `OscMessage` in an `OscBundle` with NTP timetag `onset + latency`.
    /// Sent ahead of time in scheduler bursts (SuperDirt default).
    PreBundleStamp,
    /// Inject integer `sec` and `usec` fields into the `OscMessage` payload.
    PreMessageStamp,
}

#[derive(Clone, Debug)]
pub struct OscTarget {
    pub name: String,
    pub remote_addr: SocketAddr,
    pub local_bind_addr: Option<SocketAddr>,
    pub latency_secs: f64,
    pub schedule: OscScheduleMode,
    pub handshake: bool,
    pub bus_port: Option<u16>,
}
```

### 3.2 SuperDirt `/dirt/play` Wire Compatibility

When targeting [SuperDirt](https://github.com/musikinformatik/SuperDirt) (`OscShape::superdirt()`), every `DispatchEvent` encodes an `OscBundle` (under `PreBundleStamp`) containing an `OscMessage` with address `"/dirt/play"`.

#### Wire Format Contract (`/dirt/play`)

- **Packet:** `OscBundle { timetag: OscTime { seconds, fractional }, content: vec![OscPacket::Message(msg)] }`
- **NTP Epoch:** `seconds` = Unix epoch seconds + `2_208_988_800` (`NTP_UNIX_OFFSET_SECS`); `fractional` = `((frac_secs) * 2^32) as u32`.
- **Arguments:** Alternating `(OscType::String(key), OscType::<Val>)` pairs (SuperDirt named-argument convention).
- **Mandatory Core Fields (always emitted):**

| Key | OSC Type | Value Source |
| :--- | :--- | :--- |
| `"cps"` | `OscType::Float(f32)` | `event.cps as f32` |
| `"cycle"` | `OscType::Float(f32)` | `event.whole.unwrap_or(event.part).start.to_f64() as f32` |
| `"delta"` | `OscType::Float(f32)` | `event.duration_secs as f32` |
| `"orbit"` | `OscType::Int(i32)` | `controls.get("orbit")` or `0` |

- **Pattern Control Fields:** All remaining `ControlMap` entries (`s`, `n`, `gain`, `pan`, `speed`, `cut`, `note`, `midinote`, `freq`, `vowel`, `room`, `size`, `delay`, `cutoff`, `resonance`, etc.) are serialized in deterministic key order as `(String, Float|Int|String|Blob)`. Booleans serialize as `OscType::Int(1 | 0)`.
- **SuperDirt Handshake (`oHandshake == true`):** Sends `/dirt/handshake` on startup and listens for `/dirt/handshake/reply` to discover SuperDirt control bus indices (`bus_port`).

### 3.3 Incoming `/ctrl` & Stream Control Wire Compatibility

The `OscReceiver` binds a UDP socket (default `0.0.0.0:6010` for Tidal `/ctrl` parity, configurable) and routes incoming OSC messages:

| OSC Address | Expected Arg Types | Action |
| :--- | :--- | :--- |
| `/ctrl` | `[String \| Int, Float \| Int \| String]` | Upserts `(key, ControlValue)` into global `StateMap` (`cF`, `cI`, `cS`, `cN`). Integer keys convert to decimal string. |
| `/mute` | `[String \| Int]` | Mutes target `StreamId` (`d1..d16` or named). |
| `/unmute` | `[String \| Int]` | Unmutes target `StreamId`. |
| `/muteAll` | `[]` | Mutes all active streams. |
| `/unmuteAll` | `[]` | Unmutes all streams. |
| `/solo` | `[String \| Int]` | Solos target `StreamId`. |
| `/unsolo` | `[String \| Int]` | Unsolos target `StreamId`. |
| `/hush` | `[]` | Silences all streams and triggers `panic()` on all sinks. |
| `<custom>` | Arbitrary | Dispatched to registered `OscInputHandler` callbacks. |

### 3.4 Custom OSC Shapes & Address Interpolation

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum OscArgDefault {
    Required,
    Str(String),
    Float(f32),
    Int(i32),
    Bool(bool),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OscArgSpec {
    pub param_name: String,
    pub default: OscArgDefault,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OscPayloadMode {
    /// Positional argument list matching `OscArgSpec` order; skips event if any `Required` param is absent.
    ArgList(Vec<OscArgSpec>),
    /// Alternating `key, value` pairs (SuperDirt style) with optional required-key gate.
    NamedPairs { required_keys: Vec<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OscShape {
    /// Supports `{paramName}` template interpolation (e.g. `"/fixture/{color}/set"`).
    pub address_pattern: String,
    pub payload_mode: OscPayloadMode,
}

pub trait OscEngine: OutputSink {
    fn add_target(&mut self, target: OscTarget, shapes: Vec<OscShape>) -> Result<(), IoError>;
    fn encode_event(
        target: &OscTarget,
        shape: &OscShape,
        event: &DispatchEvent,
    ) -> Result<Option<rosc::OscPacket>, IoError>;
    fn start_ctrl_listener(
        &mut self,
        bind_addr: SocketAddr,
        state_sink: StateUpdateSender,
        stream_cmd_sink: StreamCommandSender,
    ) -> Result<(), IoError>;
    fn register_custom_listener(
        &mut self,
        address_prefix: &str,
        handler: Box<dyn Fn(&rosc::OscMessage) + Send + Sync>,
    ) -> Result<(), IoError>;
}
```

---

## 4. MIDI Engine (`midir`) — `@strudel/midi` Parity & Note-Off Safety

### 4.1 Port Management & Cross-Platform Transport

- **Native:** `midir::MidiOutput` / `midir::MidiInput` (supports virtual ports on macOS/Linux ALSA/JACK and hardware ports on Windows/macOS/Linux).
- **`wasm32`:** `web-sys` WebMIDI API (`MidiAccess`, `MidiOutput`, `MidiInput`) behind the identical `MidiOutputPort` / `MidiInputPort` traits.

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
    pub is_virtual: bool,
}

pub trait MidiBackend: Send + Sync {
    fn list_outputs(&self) -> Result<Vec<MidiPortInfo>, IoError>;
    fn list_inputs(&self) -> Result<Vec<MidiPortInfo>, IoError>;
    fn open_output(&mut self, port_selector: Option<&str>) -> Result<MidiPortHandle, IoError>;
    fn open_input(
        &mut self,
        port_selector: Option<&str>,
        callback: Box<dyn FnMut(u64, &[u8]) + Send>,
    ) -> Result<MidiPortHandle, IoError>;
}
```

### 4.2 `@strudel/midi` Control Surface & Wire Encoding

Every `@strudel/midi` control parameter maps deterministically to raw MIDI bytes:

| Control | Domain / Default | MIDI Wire Encoding |
| :--- | :--- | :--- |
| `midichan` | `1..=16` (default `1`) | Low nibble `ch = (midichan - 1).clamp(0, 15)` |
| `note` | Note name (`"c4"`) or `0..=127` | NoteOn `[0x90 \| ch, note, vel]`; NoteOff `[0x80 \| ch, note, 0]` at `onset + duration` |
| `velocity` | `0.0..=1.0` (default `0.9`) | Combined with `gain`: `vel_u7 = ((velocity * gain).clamp(0.0, 1.0) * 127.0).round() as u8` |
| `gain` | `0.0..=1.0` (default `1.0`) | Multiplier on `velocity` |
| `ccn` / `ccv` | `ccn`: `0..=127`, `ccv`: `0.0..=1.0` | `[0xB0 \| ch, ccn, (ccv.clamp(0.0, 1.0) * 127.0).round() as u8]` |
| `control` | Named `midimap` parameter | Looked up in `MidiMapRegistry`; scaled via `min`/`max`/`exp` then emitted as `ccn`/`ccv` |
| `progNum` | `0..=127` | `[0xC0 \| ch, progNum.clamp(0, 127)]` |
| `sysex` / `sysexid` / `sysexdata` | `id`: `u8` or `Vec<u8>`, `data`: `Vec<u8>` (each `0..=127`) | `[0xF0, ...id, ...data, 0xF7]` (all payload bytes masked `& 0x7F`) |
| `nrpnn` / `nrpv` | `nrpnn`: `[msb, lsb]` (`0..=127`), `nrpv`: `0..=127` or `0..=16383` | 4-CC sequence on `ch`: `CC 99 = msb`, `CC 98 = lsb`, `CC 6 = data_msb`, `CC 38 = data_lsb` |
| `midibend` | `-1.0..=1.0` | 14-bit `val = ((midibend.clamp(-1.0, 1.0) + 1.0) * 8191.5).round() as u16`; `[0xE0 \| ch, (val & 0x7F) as u8, ((val >> 7) & 0x7F) as u8]` |
| `miditouch` | `0.0..=1.0` | Channel aftertouch `[0xD0 \| ch, val_u7]` (or Poly aftertouch `[0xA0 \| ch, note, val_u7]` when `note` is present) |
| `midicmd` | `"clock"` \| `"midiClock"` \| `"start"` \| `"stop"` \| `"continue"` | Real-time single byte: `0xF8` (24 PPQN), `0xFA`, `0xFC`, `0xFB` |

### 4.3 `midimaps` Registry (`min`, `max`, `exp`)

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiScaleCurve {
    Linear,
    /// Exponential curve: `min + (max - min) * norm.powf(exp)` (or `0..1` -> `exp` shaping).
    Exponential(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiCcMapping {
    pub cc: u8,
    pub min: f64,
    pub max: f64,
    pub curve: MidiScaleCurve,
}

#[derive(Clone, Debug, Default)]
pub struct MidiMap {
    pub name: String,
    pub default_channel: Option<u8>,
    pub controls: BTreeMap<String, MidiCcMapping>,
}

pub trait MidiMapRegistry: Send + Sync {
    fn register_map(&mut self, map: MidiMap);
    fn get_map(&self, name: &str) -> Option<&MidiMap>;
    /// Converts a normalized `0.0..=1.0` or physical `[min, max]` control value to 7-bit MIDI `0..=127`.
    fn scale_to_u7(mapping: &MidiCcMapping, value: f64) -> u8;
}
```

### 4.4 MIDI Input (`midin` & `midikeys`)

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct MidiInputNote {
    pub channel: u8,
    pub note: u8,
    pub velocity: f64,
    pub active: bool,
}

pub trait MidiInputState: Send + Sync {
    /// Strudel `midin(port?)`: returns normalized `0.0..=1.0` CC value for `(channel, cc)`.
    fn get_cc(&self, channel: u8, cc: u8) -> f64;
    /// Strudel `midikeys(port?)`: returns all currently held notes on `channel` (or all channels if `None`).
    fn active_keys(&self, channel: Option<u8>) -> Vec<MidiInputNote>;
    /// Ingests raw MIDI bytes from `midir` callback (updates CC table & active key map).
    fn ingest_raw_midi(&self, bytes: &[u8]);
}
```

### 4.5 Note-Off Safety & Active-Note Tracker

To guarantee zero stuck notes across live-coding edits, `hush`, `panic`, pattern replacement, or process shutdown:

1. Every emitted `NoteOn(port, ch, note)` registers an `ActiveNoteEntry { port, channel, note, off_time_secs, stream_id }` in `ActiveNoteTracker`.
2. Scheduler ticks flush all `ActiveNoteEntry` items where `now_secs >= off_time_secs` **before** dispatching new `NoteOn` events for the same `(port, ch, note)` (re-trigger safety).
3. Calling `panic()` on `MidiEngine` or dropping `MidiEngine`:
   - Immediately sends explicit `NoteOff` (`0x80 | ch, note, 0`) for every tracked active note.
   - Sends `CC 123` (All Notes Off) and `CC 120` (All Sound Off) on all 16 channels (`0..16`).
   - Clears `ActiveNoteTracker`.

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActiveNoteKey {
    pub port_id: String,
    pub channel: u8,
    pub note: u8,
}

pub trait ActiveNoteTracker: Send + Sync {
    fn track_note_on(&self, key: ActiveNoteKey, off_time_secs: f64, stream_id: &StreamId);
    fn drain_due_note_offs(&self, now_secs: f64) -> Vec<ActiveNoteKey>;
    fn drain_stream_note_offs(&self, stream_id: &StreamId) -> Vec<ActiveNoteKey>;
    fn drain_all_note_offs(&self) -> Vec<ActiveNoteKey>;
}
```


### 4.6 MIDI Polyphonic Expression (MPE) & Hardware Controller Bridges (HID / Gamepad / `DeviceMotion`)

```rust
/// MPE Zone Configuration (Lower Zone: Master Ch 1, Member Ch 2..=N; Upper Zone: Master Ch 16, Member Ch 15..=N).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpeZoneConfig {
    pub master_channel: u8,         // 1 (Lower) or 16 (Upper)
    pub member_channels: u8,        // 1..=15
    pub pitch_bend_range_semitones: u8, // Default 48 semitones
}

/// Per-note MPE expression state allocated dynamically across member channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MpeNoteExpression {
    pub pitch_bend_semitones: f32,  // Sent as 14-bit Pitch Bend on assigned member channel
    pub timbre_cc74: f32,           // Sent as CC 74 (Slide / Brightness) [0.0, 1.0]
    pub pressure: f32,              // Sent as Channel Aftertouch (0xD0) on assigned member channel [0.0, 1.0]
}

/// Cross-platform Gamepad / HID (`gilrs` on native, Web Gamepad API on `wasm32`) and `DeviceMotion` bridge.
pub trait ControllerInputBridge: Send + Sync {
    /// Reads normalized axis `[-1.0, 1.0]` or button `[0.0, 1.0]` for gamepad index `pad_id` into `StateMap`.
    fn gamepad_axis(&self, pad_id: usize, axis_name: &str) -> f64;
    fn gamepad_button(&self, pad_id: usize, button_name: &str) -> f64;
    /// Reads device accelerometer/gyroscope (`gx`, `gy`, `gz`, `ax`, `ay`, `az`) normalized to `[0.0, 1.0]`.
    fn device_motion(&self, channel: &str) -> f64;
}
```

---

## 5. Clock, Tempo & Ableton Link Sync (`ableton-link-rs`)

### 5.1 Tempo & Cycle Mathematics

- **CPS vs. BPM:** `cps = bpm / 60.0 / beats_per_cycle` (default `beats_per_cycle = 4.0`, so `120 BPM = 0.5 CPS`).
- **Phase-Continuous Tempo Changes:** Changing `setcps` or `setbpm` updates the epoch `(origin_secs, origin_cycle, cps)` atomically so the current cycle position never jumps backward or forward discontinuously.

### 5.2 `ableton-link-rs` Integration & Multi-User Shared Clock

| Feature | Specification |
| :--- | :--- |
| **Library** | `ableton-link-rs` (Rust bindings / pure-Rust [Ableton Link](https://ableton.github.io/link/) protocol) |
| **Session State** | Captures `SessionState` per scheduler tick (`capture_app_session_state` / `commit_app_session_state`). |
| **Quantum** | `quantum = beats_per_cycle` (default `4.0`). `phase_at_time(time, quantum)` maps directly to fractional cycle position `[0.0, 1.0)`. |
| **Multi-User Sync** | Automatic peer discovery on LAN; `num_peers()` exposed to UI/status; `start_stop_sync` propagates transport play/pause across all connected Link peers. |
| **`wasm32` Fallback** | When compiled for `wasm32`, `LinkClock` degrades cleanly to `ClockSource::AudioSampleClock` or `ClockSource::Monotonic` with optional WebSocket clock-sync bridge. |

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSource {
    Monotonic,
    AbletonLink,
    AudioSampleClock,
    DawHostClock,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClockSnapshot {
    pub wall_secs: f64,
    pub cycle: f64,
    pub cps: f64,
    pub bpm: f64,
    pub beats_per_cycle: f64,
    pub quantum: f64,
    pub link_peers: usize,
    pub is_playing: bool,
}

pub trait ClockEngine: Send + Sync {
    fn source(&self) -> ClockSource;
    fn snapshot(&self) -> ClockSnapshot;

    /// Converts a `CycleTime` to wall-clock seconds using current tempo/Link session state.
    fn cycle_to_wall_secs(&self, cycle: CycleTime) -> f64;
    /// Converts wall-clock seconds to continuous cycle position.
    fn wall_secs_to_cycle(&self, wall_secs: f64) -> f64;

    /// TidalCycles / Strudel tempo API:
    fn set_cps(&self, cps: f64) -> Result<(), IoError>;
    fn set_bpm(&self, bpm: f64) -> Result<(), IoError>;
    fn set_beats_per_cycle(&self, bpc: f64) -> Result<(), IoError>;
    /// Phase nudge in seconds (positive = delay, negative = advance).
    fn nudge_secs(&self, delta_secs: f64);

    /// Ableton Link controls:
    fn enable_link(&self, enabled: bool) -> Result<(), IoError>;
    fn enable_start_stop_sync(&self, enabled: bool) -> Result<(), IoError>;
    fn link_num_peers(&self) -> usize;
    fn set_playing(&self, playing: bool);
}
```

---

## 6. Stream Management & Transitions

### 6.1 Stream Slots (`d1..d16`, `$:`, `p`) & Mute/Solo/Hush/Panic

- **Named & Numbered Streams:** `d1`..`d16` map to `StreamId("1")`..`StreamId("16")`. Arbitrary string IDs are supported via `p(id, pat)`.
- **Strudel `$:` Auto-Slot Registration:** Evaluating `$:` stacks allocates deterministic sequential `StreamId`s (`"$:0"`, `"$:1"`, ...) per evaluation block and silences any orphaned `$:` slots removed in the latest evaluation.
- **Mute / Solo Semantics:**
  - `mute(id)` / `unmute(id)` toggles per-stream `muted` flag.
  - `solo(id)` / `unsolo(id)` sets `soloed` flag; when **any** stream has `soloed == true`, only streams with `soloed == true && !muted` emit events.
  - `hush()` clears all stream patterns to `Silence` and calls `panic()` on all registered `OutputSink`s.
  - `panic()` immediately flushes all active voices/notes on all `OutputSink`s without clearing pattern definitions.

```rust
#[derive(Clone, Debug)]
pub struct StreamSlotState<P> {
    pub id: StreamId,
    /// History stack: index 0 is the most recent pattern; older patterns retained for `histpan` & transitions.
    pub history: Vec<P>,
    pub muted: bool,
    pub soloed: bool,
    pub orbit: u16,
}

pub trait StreamManager<P>: Send + Sync {
    fn set_pattern(&self, id: &StreamId, pattern: P);
    fn apply_dollar_colon_block(&self, patterns: Vec<P>);
    fn silence(&self, id: &StreamId);
    fn mute(&self, id: &StreamId);
    fn unmute(&self, id: &StreamId);
    fn mute_all(&self);
    fn unmute_all(&self);
    fn solo(&self, id: &StreamId);
    fn unsolo(&self, id: &StreamId);
    fn hush(&self) -> Result<(), IoError>;
    fn panic(&self) -> Result<(), IoError>;
    fn active_streams(&self) -> Vec<StreamId>;
}
```

### 6.2 Complete TidalCycles Transition Parity

All transitions operate on the stream's `history` (`prev`, `next`, or `[p_0, p_1, ...]` at evaluation cycle `t_eval`).

| Transition | Default Cycles | Exact Semantics & Contract |
| :--- | :--- | :--- |
| `xfade(id, pat)` | `4` | Alias for `xfadeIn(id, 4, pat)`. |
| `xfadeIn(id, t, pat)` | `t` | Linearly ramps `prev` gain `1.0 -> 0.0` and `next` gain `0.0 -> 1.0` over `[t_eval, t_eval + t)`; drops `prev` after `t_eval + t`. |
| `clutch(id, pat)` | `2` | Alias for `clutchIn(id, 2, pat)`. |
| `clutchIn(id, t, pat)` | `t` | Stochastic event-level crossfade: degrades `prev` (`fadeOut` probability `0.0 -> 1.0`) while undegrading `next` (`fadeIn` probability `1.0 -> 0.0`) over `t` cycles. |
| `anticipate(id, pat)` | `8` | Alias for `anticipateIn(id, 8, pat)`. |
| `anticipateIn(id, t, pat)` | `t` | Builds anticipation (e.g., `wash` / filter-or-stutter wash on `prev`) for `t` cycles, then drops `next` cleanly at `t_eval + t`. |
| `interpolate(id, pat)` | `4` | Alias for `interpolateIn(id, 4, pat)`. |
| `interpolateIn(id, t, pat)` | `t` | Morphs numeric `ControlMap` parameters (`cutoff`, `pan`, `gain`, etc.) linearly from `prev` values to `next` values over `[t_eval, t_eval + t)`. |
| `jump(id, pat)` | `0` | Immediate replacement (no transition). |
| `jumpIn(id, n, pat)` | `n` (int) | Replaces `prev` with `next` after `n` cycles have elapsed from `t_eval` (unaligned to cycle boundary). |
| `jumpIn'(id, n, pat)` (`jump_in_prime`) | `n` (int) | Replaces `prev` with `next` at the exact cycle boundary `ceil(t_eval) + n`. |
| `jumpMod(id, m, pat)` | `m` (int) | Replaces `prev` with `next` at the next cycle boundary `c` where `c % m == 0`. |
| `histpan(id, n, pat)` | `n` (int) | Retains up to `n` most recent patterns in `history`, panning them evenly across stereo `[-1.0, 1.0]` (or `[0.0, 1.0]` SuperDirt pan) with most recent on the left. |
| `wait(id, t, pat)` | `t` | Pauses/silences stream `id` for `t` cycles (`[t_eval, t_eval + t)`), then starts `pat`. |
| `waitT(id, trans, t, pat)` | `t` | Pauses stream `id` for `t` cycles, then executes transition `trans` into `pat`. |
| `wash(out_fn, in_fn, delay, dur, ...)` | — | Combinator applying `out_fn` to `prev` and `in_fn` to `next` over configurable delay/duration windows (building block for `anticipate` / `jump`). |
| `washIn(fn, delay, dur)` | — | Applies `fn` to the incoming pattern over `[t_eval + delay, t_eval + delay + dur)`. |
| `mortal(id, lifespan, release, pat)` | — | Plays `pat` for `lifespan` cycles, fades out (`fadeOut`) over `release` cycles, then transitions to `Silence`. |

```rust
/// Fade building-block primitives (TidalCycles `Sound.Tidal.Transition` parity).
pub trait FadePrimitives<P> {
    fn fade_in(cycles: CycleTime, pat: P) -> P;
    fn fade_in_from(start: CycleTime, cycles: CycleTime, pat: P) -> P;
    fn fade_out(cycles: CycleTime, pat: P) -> P;
    fn fade_out_from(start: CycleTime, cycles: CycleTime, pat: P) -> P;
}

#[derive(Clone, Debug)]
pub enum TransitionSpec<P> {
    XfadeIn { cycles: CycleTime },
    ClutchIn { cycles: CycleTime },
    AnticipateIn { cycles: CycleTime },
    InterpolateIn { cycles: CycleTime },
    Jump,
    JumpIn { cycles: i64 },
    JumpInPrime { cycles: i64 },
    JumpMod { cycle_mod: i64 },
    Histpan { count: usize },
    Wait { cycles: CycleTime },
    WaitT { cycles: CycleTime, inner: Box<TransitionSpec<P>> },
    Wash {
        delay: CycleTime,
        duration: CycleTime,
        out_transform: fn(P) -> P,
        in_transform: fn(P) -> P,
    },
    WashIn {
        delay: CycleTime,
        duration: CycleTime,
        transform: fn(P) -> P,
    },
    Mortal { lifespan: CycleTime, release: CycleTime },
}

pub trait TransitionEngine<P> {
    fn apply_transition(
        &self,
        id: &StreamId,
        spec: TransitionSpec<P>,
        next: P,
        eval_cycle: CycleTime,
    ) -> Result<(), IoError>;
}
```

---

## 7. TDD Verification & Test Contracts

All test contracts below are normative acceptance criteria. Every module in `silverwave-io` must pass its corresponding contract suite prior to release.

### 7.1 Audio & Plugin Backend Contracts

| Test ID | Target | Contract Assertion |
| :--- | :--- | :--- |
| `AUD-01` | `CpalBackend` | Ring buffer push/pop is lock-free; pushing `N` events produces exact `frame_offset` sample placement within `±1` sample at `48_000 Hz`. |
| `AUD-02` | `WebAudioBackend` | `OfflineAudioContext` render of a 2-cycle impulse pattern matches native `cpal` render bit-for-bit (peak delta `< 1e-6`). |
| `AUD-03` | `NihPlugBridge` | Under `PluginSyncMode::HostTransport`, changing host `tempo_bpm` from `120.0` to `140.0` and `pos_beats` across buffer boundaries produces zero skipped or duplicated events. |
| `AUD-04` | `CsoundEngine` | `load_orc` resolves `github:` shorthand, `start_instrument("Mono")` emits `i "Mono" 0 -1`, and `set_control_channel("NoiSaw.cut", 800.0)` updates the Csound channel. |

### 7.2 `rosc` OSC Engine Contracts

| Test ID | Target | Contract Assertion |
| :--- | :--- | :--- |
| `OSC-01` | `/dirt/play` Wire | `OscEngine::encode_event` with `PreBundleStamp` produces a valid `OscBundle` whose NTP timetag equals `onset + latency + 2_208_988_800` and whose message starts with `/dirt/play` containing `cps`, `cycle`, `delta`, `orbit` + all `ControlMap` pairs. |
| `OSC-02` | `/ctrl` Receiver | Receiving `/ctrl sf cutoff 1200.0` and `/ctrl if 4 0.75` updates `StateMap["cutoff"] = Float(1200.0)` and `StateMap["4"] = Float(0.75)`. |
| `OSC-03` | Remote Stream Control | Receiving `/mute 1`, `/solo "drums"`, `/hush` invokes the corresponding `StreamManager` methods on the next tick. |
| `OSC-04` | Custom `OscShape` | `OscShape` with `Required` parameter suppresses packet emission when parameter is missing; `{color}` in address template interpolates `"red"` -> `"/fixture/red/set"`. |

### 7.3 `midir` MIDI Engine Contracts

| Test ID | Target | Contract Assertion |
| :--- | :--- | :--- |
| `MID-01` | Note & Velocity | `note=60, midichan=2, velocity=0.8, gain=0.5` emits `[0x91, 60, 51]` at `onset` and `[0x81, 60, 0]` at `onset + duration`. |
| `MID-02` | `midimaps` Scaling | `MidiMap` with `min=20.0, max=120.0, curve=Exponential(2.0)` scales `0.5` to expected non-linear 7-bit CC value; `Linear` scales `0.5` to `64`. |
| `MID-03` | `nrpn` Sequence | `nrpnn=[0x01, 0x20], nrpv=64` emits exact 4-message sequence: `CC 99=1`, `CC 98=32`, `CC 6=64`, `CC 38=0` in order. |
| `MID-04` | `sysex` Framing | `sysexid=[0x7E, 0x7F], sysexdata=[0x06, 0x01]` emits `[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]`. |
| `MID-05` | `midibend` & `miditouch` | `midibend=0.0` emits `[0xE0, 0x00, 0x40]` (center `8192`); `midibend=-1.0` emits `[0xE0, 0x00, 0x00]`; `miditouch=1.0` emits `[0xD0, 127]`. |
| `MID-06` | `midicmd` Clock | At `0.5 CPS` (`120 BPM`, `4/4`), `midicmd="clock"` emits `0xF8` at `48 Hz` (`24 PPQN`); `"start"`/`"stop"`/`"continue"` emit `0xFA`/`0xFC`/`0xFB`. |
| `MID-07` | `midin` & `midikeys` | Ingesting `[0xB0, 74, 127]` sets `get_cc(1, 74) == 1.0`; ingesting `[0x90, 60, 100]` adds note `60` to `active_keys(Some(1))`, and `[0x80, 60, 0]` removes it. |
| `MID-08` | Note-Off Safety & `panic` | Calling `panic()` or dropping `MidiEngine` while 4 notes are held emits 4 explicit `NoteOff`s + `CC 123` and `CC 120` on all 16 channels, leaving `ActiveNoteTracker` empty. |

### 7.4 Ableton Link & Clock Phase-Lock Contracts

| Test ID | Target | Contract Assertion |
| :--- | :--- | :--- |
| `LNK-01` | Tempo Continuity | Calling `set_cps(0.75)` mid-cycle at `cycle = 10.35` preserves `wall_secs_to_cycle(now) == 10.35 ± 1e-9` immediately after the call. |
| `LNK-02` | Phase-Lock | Two `LinkClock` instances in the same Link session converge within `250 ms` to `|phase_a - phase_b| < 0.002` cycles and identical `bpm`. |
| `LNK-03` | `start_stop_sync` | Calling `set_playing(false)` on Peer A propagates `is_playing == false` to Peer B when `enable_start_stop_sync(true)` is active. |

### 7.5 Stream & Transition Continuity Contracts

| Test ID | Target | Contract Assertion |
| :--- | :--- | :--- |
| `TRN-01` | `xfadeIn` Gain Curve | Over `[t_eval, t_eval + 4)`, `prev` gain decreases monotonically `1.0 -> 0.0`, `next` gain increases `0.0 -> 1.0`, and `prev` produces 0 events for `t >= t_eval + 4`. |
| `TRN-02` | `clutchIn` Continuity | Over `[t_eval, t_eval + 2)`, `prev` event density approaches `0` while `next` event density approaches `1.0` with no duplicate `whole` arcs. |
| `TRN-03` | `interpolateIn` | Numeric control `cutoff` morphs linearly from `prev` (`100.0`) to `next` (`16000.0`) across `t` cycles (`cutoff(t_eval + t/2) ≈ 8050.0`). |
| `TRN-04` | `jumpIn` vs `jumpIn'` | `jumpIn(1, 2)` evaluated at `c = 3.4` switches at `5.4`; `jumpIn'(1, 2)` evaluated at `c = 3.4` switches at exact cycle boundary `6.0`. |
| `TRN-05` | `jumpMod` | `jumpMod(1, 4)` evaluated at `c = 5.2` switches at `c = 8.0` (`8 % 4 == 0`). |
| `TRN-06` | `histpan` | `histpan(1, 4)` with 4 historical patterns assigns distinct stereo pan values ordered left-to-right from newest to oldest. |
| `TRN-07` | `wait` & `waitT` | `wait(1, 4)` emits 0 events during `[t_eval, t_eval + 4)` and resumes `pat` at `t_eval + 4`; `waitT` invokes inner transition at `t_eval + 4`. |
| `TRN-08` | `mortal` | `mortal(1, 4, 2)` plays full events on `[0, 4)`, degrades on `[4, 6)`, and emits 0 events for `t >= 6`. |
| `TRN-09` | `solo` / `mute` / `$:` | Soloing `d2` silences `d1` and `d3`; re-evaluating `$:` with 2 patterns instead of 3 silences `"$:2"` automatically. |

---

## 8. 1:1+ & Category-King I/O, MIDI, DAW Plugin & Multiplayer Extensions (`PP-07`, `PP-08`, `PP-12`, `CK-03`, `CK-09`)

1. **Fault-Isolated Per-Stream Evaluation**: Syntax or type errors in one stream block (`d2:`) retain that stream's last valid pattern while allowing sibling streams (`d1:`, `d3:`) to update on the next quantized boundary without resetting `cps`, MIDI CCs, or orbit states.
2. **Pure Pattern-Level Transition Combinators**: All 11 [Tidal transitions](https://tidalcycles.org/docs/reference/transitions/) are callable both as stream commands and as pure `Pattern<ControlMap>` combinators inside `arrange`, `seq_p`, `ur`, or MIDI triggers.
3. **Sample-Accurate MIDI State Machine (`MidiEventOrderingPriority`) & External Clock PLL**:
   - Enforces strict intra-timestamp ordering: `RetriggerNoteOff (0) -> BankSelectMsbCc0 (1) -> BankSelectLsbCc32 (2) -> ProgramChange (3) -> ControlChangeAndPitchBend (4) -> NoteOn (5) -> ScheduledNoteOff (6)`.
   - Eliminates `10 ms` note-duration subtraction hacks and supports `.keepAudio(true)` simultaneous MIDI + internal DSP routing, plus a 2nd-order PLL for external 24 PPQN MIDI clock slave sync (`< 0.2 ms` jitter).
4. **Octatrack Scene Bank (`CrossfaderSceneBank`)**: Interpolates between `scene_a` and `scene_b` parameter locks via `xfader(0.0..1.0)` at 64-sample `k-rate`.
5. **Native CLAP & VST3 DAW Plugin (`DawPluginConfig` in `silverwave-plugin`)**: Exposes 16 stereo orbit output buses (32 channels), 64 DAW-automatable macro parameters with CLAP polyphonic modulation, sample-accurate host transport lock, and embedded GUI/MCP/LSP co-performance bridges.
6. **Cycle-Quantized CRDT Ensemble Sync (`CommitEpoch`) & Steam Deck Handheld Mode**: Synchronizes collaborative Yjs/CRDT edits with deterministic `target_cycle: Rational64` activation epochs and per-peer DAC latency offsets, plus direct [`gilrs`](https://docs.rs/gilrs/)/HID gamepad polling for Steam Deck and Linux handhelds.
