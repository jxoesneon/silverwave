# `03-SPEC-AUDIO-DSP-AND-SAMPLES.md`: `silverwave-dsp` Audio Engine, DSP Graph, and Sample Subsystem

**Status:** Normative Specification  
**Crate:** `silverwave-dsp`  
**Target Parity:** Union of Strudel [`superdough`](https://www.npmjs.com/package/superdough) and TidalCycles [`SuperDirt`](https://github.com/musikinformatik/SuperDirt/blob/develop/synths/core-synths-global.scd)  

---

## 1. Scope & Architectural Overview

`silverwave-dsp` is the real-time audio synthesis, sample playback, effects processing, and bus-routing engine for Silverwave. It achieves 1:1 behavioral and parameter parity with the union of [Strudel's `superdough`](https://strudel.cc/learn/effects/) and [TidalCycles' `SuperDirt`](https://github.com/musikinformatik/SuperDirt) while enforcing hard real-time safety in Rust.

### 1.1 Non-Negotiable Real-Time Invariants

1. **Zero Heap Allocations on the Audio Thread:** No `malloc`, `free`, `Box::new`, `Vec::push`, `String::from`, or `Arc::drop` (when refcount reaches zero) may execute inside the audio callback.
2. **Wait-Free Cross-Thread Communication:** Control events and decoded asset handles cross thread boundaries exclusively through lock-free Single-Producer Single-Consumer (SPSC) ring buffers ([`rtrb`](https://docs.rs/rtrb) / [`rtrb-basedrop`](https://docs.rs/rtrb-basedrop)).
3. **Deferred Reclamation:** All reference-counted heap resources (`SampleBuffer`, `WavetableBuffer`, `ImpulseResponseBuffer`) are wrapped in `basedrop::Shared<T>` and reclaimed asynchronously by a non-real-time `basedrop::Collector` thread.
4. **Hardware Latency Budget:** End-to-end audio path latency (from ring-buffer ingestion to hardware DAC output) MUST remain **< 10 ms** at standard sample rates (`44.1 kHz` / `48 kHz`), targeting `64`–`256` frame hardware buffers (`1.33 ms`–`5.33 ms` at `48 kHz`) with sample-accurate sub-block event scheduling.
5. **NaN / Inf Immunity:** Every orbit monitor stage MUST sanitize non-finite floating-point samples (`NaN`, `±Inf`) to `0.0` before hardware output, matching SuperDirt's `CheckBadValues` contract.

---

## 2. Lock-Free `rtrb` + `rtrb-basedrop` Architecture

### 2.1 Threading Topology

```text
+--------------------------+       rtrb::Producer<TriggerEvent>       +---------------------------+
|  Pattern Scheduler /     | ---------------------------------------> |  Audio Callback Thread    |
|  Control Thread          |       (POD, zero-alloc, wait-free)       |  (cpal / JACK / CoreAudio)|
+--------------------------+                                          |                           |
                                                                      |  - Pre-allocated VoicePool|
+--------------------------+   rtrb_basedrop::Producer<AssetHandle>   |  - OrbitBus[MAX_ORBITS]   |
|  Async Sample Loader &   | ---------------------------------------> |  - Zero alloc / Zero lock |
|  Decoder Worker Pool     |       (basedrop::Shared<T>)              |  - < 10 ms latency        |
+--------------------------+                                          +---------------------------+
                                                                                    |
+--------------------------+       basedrop::Collector::collect()                   |
|  Background GC Thread    | <------------------------------------------------------+
|  (Periodic Reclamation)  |       (Deallocates dropped Shared<T> off-thread)
+--------------------------+
```

### 2.2 `TriggerEvent` POD Contract

`TriggerEvent` MUST be a Plain Old Data (POD) `#[repr(C)]` struct implementing `Copy + Clone`, containing zero heap pointers, `Vec`s, or `String`s. Variable-length parameters (`partials`, `phases`, `duckorbit` targets, `vowel` formants, `wt_*` wavetable IDs, sample IDs) are represented via fixed-capacity inline arrays or interned numeric handles.

```rust
pub const MAX_PARTIALS: usize = 64;
pub const MAX_DUCK_TARGETS: usize = 8;
pub const MAX_FM_OPERATORS: usize = 8;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TriggerEvent {
    // --- Scheduling & Identity ---
    pub trigger_sample: u64,        // Absolute sample frame timestamp for sample-accurate onset
    pub duration_samples: u64,      // Event duration in frames (derived from cps / clip)
    pub orbit: u8,                  // Target orbit index (0..MAX_ORBITS)
    pub cut_group: i32,             // Choke group (0 = disabled; != 0 chokes matching group)
    pub voice_kind: VoiceKind,      // Synth | Sample | Wavetable | Zzfx | Noise

    // --- Source & Sample Addressing ---
    pub sample_handle: SampleId,    // Interned handle to pre-resolved SampleKey
    pub bank_handle: BankId,        // Interned bank identifier (e.g., RolandTR909)
    pub n: u32,                     // Sample index within sound bank
    pub begin: f32,                 // Normalized start offset [0.0, 1.0]
    pub end: f32,                   // Normalized end offset [0.0, 1.0]
    pub speed: f32,                 // Playback rate multiplier (negative = reverse)
    pub loop_mode: LoopMode,        // Off | On | WavetableDefault
    pub loop_begin: f32,            // Normalized loop start [0.0, 1.0]
    pub loop_end: f32,              // Normalized loop end [0.0, 1.0]
    pub fit: f32,                   // Time-fit factor (0.0 = disabled, >0.0 = fit to duration)
    pub chop: u16,                  // Granular chop slices (0 = disabled)
    pub striate: u16,               // Interleaved striation slices (0 = disabled)
    pub slice_idx: u16,             // Active slice index
    pub slice_count: u16,           // Total slice count for slice/splice
    pub splice_mode: bool,          // true = pitch-match slice to event duration
    pub scrub: f32,                 // Scrubbing playhead position [0.0, 1.0]

    // --- Pitch & Vibrato ---
    pub freq_hz: f32,               // Fundamental frequency in Hz
    pub penv: f32,                  // Pitch envelope depth in semitones
    pub pattack: f32,               // Pitch envelope attack (s)
    pub pdecay: f32,                // Pitch envelope decay (s)
    pub psustain: f32,              // Pitch envelope sustain [0.0, 1.0]
    pub prelease: f32,              // Pitch envelope release (s)
    pub pcurve: f32,                // Pitch envelope curve shape
    pub panchor: f32,               // Pitch envelope anchor [0.0, 1.0]
    pub vib: f32,                   // Vibrato LFO rate (Hz)
    pub vibmod: f32,                // Vibrato depth (semitones)

    // --- Main Amplitude & Envelope ---
    pub gain: f32,                  // Pre-filter gain (default 0.8)
    pub postgain: f32,              // Post-chain gain (`post`, default 1.0)
    pub attack: f32,                // Main ADSR attack (s)
    pub decay: f32,                 // Main ADSR decay (s)
    pub sustain: f32,               // Main ADSR sustain [0.0, 1.0]
    pub release: f32,               // Main ADSR release (s)
    pub clip: f32,                  // Duration multiplier

    // --- Supersaw & Additive ---
    pub unison: u8,                 // Supersaw voice count (1..=16)
    pub detune: f32,                // Supersaw detune amount
    pub spread: f32,                // Supersaw stereo spread [0.0, 1.0]
    pub partial_count: u8,          // Active harmonics in partials/phases
    pub partials: [f32; MAX_PARTIALS],
    pub phases: [f32; MAX_PARTIALS],

    // --- 8-Operator FM Matrix ---
    pub fm_index: f32,              // `fm` global modulation index
    pub fmh: f32,                   // `fmh` harmonicity ratio
    pub fmattack: f32,              // FM ADSR attack (s)
    pub fmdecay: f32,               // FM ADSR decay (s)
    pub fmsustain: f32,             // FM ADSR sustain [0.0, 1.0]
    pub fmenv: f32,                 // FM envelope depth
    pub fm_ops: [FmOpParams; MAX_FM_OPERATORS], // `fm1..fm8` operator params
    pub fm_matrix: [[f32; MAX_FM_OPERATORS]; MAX_FM_OPERATORS], // e.g., `fm13` -> [0][2]

    // --- 20-Parameter ZZFX Engine ---
    pub zzfx: ZzfxParams,

    // --- Series Filters & Filter Envelopes ---
    pub ftype: FilterTopology,      // 12db | 24db | Ladder
    pub lpf: f32,                   // Low-pass cutoff (Hz)
    pub lpq: f32,                   // Low-pass resonance (Q)
    pub lpenv: f32,                 // LP envelope depth (octaves/semitones)
    pub lpa: f32, pub lpd: f32, pub lps: f32, pub lpr: f32,
    pub hpf: f32,                   // High-pass cutoff (Hz)
    pub hpq: f32,                   // High-pass resonance (Q)
    pub hpenv: f32,                 // HP envelope depth
    pub hpa: f32, pub hpd: f32, pub hps: f32, pub hpr: f32,
    pub bpf: f32,                   // Band-pass center frequency (`bandf`, Hz)
    pub bpq: f32,                   // Band-pass resonance (`bandq`, Q)
    pub bpenv: f32,                 // BP envelope depth
    pub bpa: f32, pub bpd: f32, pub bps: f32, pub bpr: f32,
    pub vowel: VowelFormant,        // None | A | E | I | O | U | Ae | Aa | Oe | Ue | Y | Uh | Un | En | An | On

    // --- Time-Stretch & Degradation / Saturation ---
    pub stretch: f32,               // OLA phase vocoder stretch factor
    pub coarse: u32,                // Sample-rate reduction factor (1 = off)
    pub crush: f32,                 // Bit-depth reduction (0.0 = off, 1.0..16.0 bits)
    pub shape: f32,                 // Waveshaper distortion [0.0, 1.0)
    pub distort: f32,               // Stereo overdrive (`distort` / `dist`)

    // --- Modulation, Dynamics, Spatial ---
    pub tremolo_rate: f32,          // `tremolosync` / `tremolorate`
    pub tremolo_depth: f32,         // `tremolodepth` [0.0, 1.0]
    pub tremolo_shape: LfoShape,    // Sine | Tri | Saw | Square
    pub tremolo_phase: f32,         // Initial phase [0.0, 1.0]
    pub compressor: CompressorParams,
    pub pan: f32,                   // Stereo pan [0.0 (L), 1.0 (R)], default 0.5
    pub phaser: f32,                // Phaser LFO rate (Hz)
    pub phaser_depth: f32,          // Phaser wet/depth [0.0, 1.0]
    pub phaser_sweep: f32,          // Phaser sweep range (Hz)
    pub phaser_center: f32,         // Phaser center frequency (Hz)

    // --- Global Orbit Sends & Sidechain ---
    pub delay: f32,                 // Delay send level [0.0, 1.0]
    pub delay_time: f32,            // Delay time (s or cycles if `lock == true`)
    pub delay_feedback: f32,        // Delay feedback [0.0, 0.99]
    pub delay_lock: bool,           // Lock delay time to `1.0 / cps`
    pub cps: f32,                   // Current cycles per second for tempo-locked FX
    pub room: f32,                  // Reverb send level [0.0, 1.0]
    pub size: f32,                  // Reverb feedback/size [0.0, 1.0]
    pub dry: f32,                   // Dry signal attenuation [0.0, 1.0]
    pub leslie: f32,                // Leslie dry/wet [0.0, 1.0]
    pub lrate: f32,                 // Leslie rotor rate (Hz)
    pub lsize: f32,                 // Leslie cabinet size (m)
    pub duck_targets: [u8; MAX_DUCK_TARGETS], // Target orbit indices (`duckorbit`)
    pub duck_count: u8,
    pub duck_attack: f32,           // Sidechain recovery time (`duckattack`, s)
    pub duck_depth: f32,            // Sidechain attenuation (`duckdepth`, [0.0, 1.0])
}
```

### 2.3 Ring-Buffer & Pre-Allocated Pool Contracts

```rust
pub struct EngineChannels {
    pub event_rx: rtrb::Consumer<TriggerEvent>,
    pub asset_rx: rtrb_basedrop::Consumer<AssetMessage>,
    pub rms_tx: rtrb::Producer<OrbitRmsReport>,
}

pub enum AssetMessage {
    SampleLoaded { id: SampleId, buffer: basedrop::Shared<SampleBuffer> },
    WavetableLoaded { id: SampleId, buffer: basedrop::Shared<WavetableBuffer> },
    ImpulseResponseLoaded { orbit: u8, buffer: basedrop::Shared<ImpulseResponseBuffer> },
    EvictSample { id: SampleId },
}

pub trait AudioEngine {
    /// Pre-allocates `VoicePool` (capacity >= `max_voices`), `OrbitBus` array,
    /// delay lines, reverb FDN buffers, and OLA scratch buffers.
    /// MUST NOT allocate after `new()` returns.
    fn new(config: EngineConfig, channels: EngineChannels) -> Self;

    /// Wait-free audio callback executed on the hardware audio thread.
    /// Contract: Zero heap allocations, zero locks, zero panics, zero syscalls.
    fn process_block(&mut self, output: &mut [f32], num_channels: usize);
}
```

**Latency & Sub-Block Accuracy Contract:**
- On each `process_block` invocation of `N` frames covering `[block_start_frame, block_start_frame + N)`:
  - Events with `trigger_sample < block_start_frame + N` are popped from `event_rx`.
  - Events targeting `frame_offset = (trigger_sample - block_start_frame).clamp(0, N - 1)` begin synthesis at exact `frame_offset` within the block, eliminating jitter from buffer quantization.
  - Events whose `trigger_sample >= block_start_frame + N` are held in a fixed-capacity pre-allocated lookahead ring (`LOOKAHEAD_CAPACITY`) without dropping or allocating.

---

## 3. `Orbit` Bus Routing, `cut` Groups, & Polyphony Policy

### 3.1 Orbit Signal Topology

Each `Orbit` (`0..MAX_ORBITS`, default `16`) models SuperDirt's four-bus architecture and Strudel's orbit routing:

```text
  [Active Voices for Orbit i]
               |
               v
          +---------+
          |synthBus |  (Sum of all voices assigned to Orbit i)
          +----+----+
               |
               v
          +---------+
          |  gate   |  (Orbit-level ducking envelope driven by `duckorbit` triggers)
          +----+----+
               |
               v
          +---------+
          | dryBus  |  (Attenuated by `(1.0 - leslie)` and `dry` where applicable)
          +----+----+------------------+------------------+------------------+
               |                       |                  |                  |
               | (dry)                 v                  v                  v
               |                 +-----------+      +-----------+      +-----------+
               |                 |dirt_delay |      |dirt_reverb|      |dirt_leslie|
               |                 +-----+-----+      +-----+-----+      +-----+-----+
               |                       |                  |                  |
               |                       +------------------+------------------+
               |                                          |
               |                                          v
               |                                 +-----------------+
               |                                 | globalEffectBus |
               |                                 +--------+--------+
               |                                          |
               +----------------------+-------------------+
                                      v
                               +------------+
                               | monitorOut | -> CheckBadValues (NaN/Inf -> 0.0)
                               +------+-----+    Limiter / Softclip (`limitertype`)
                                      |          RMS Telemetry (`dirt_rms`)
                                      v
                      [Stereo Mixdown OR Discrete Channels (2i, 2i+1)]
```

```rust
pub struct OrbitBus {
    pub index: u8,
    pub synth_bus: AudioBlockBuffer,       // Pre-allocated stereo/multichannel scratch
    pub dry_bus: AudioBlockBuffer,
    pub global_effect_bus: AudioBlockBuffer,
    pub duck_envelope: DuckSidechainState, // Driven by incoming duck triggers
    pub delay: OrbitDelay,                 // Up to 4.0s pre-allocated delay buffer
    pub reverb: OrbitReverb,               // 4x Allpass + FDN loop + OnePole + LeakDC
    pub leslie: OrbitLeslie,               // Crossover HPF 800Hz + Doppler DelayC + Balance2
    pub monitor: OrbitMonitor,             // CheckBadValues + Limiter/Softclip + RMS meter
}

pub enum OutputRoutingMode {
    /// All orbits summed to stereo channels [0, 1] (Strudel default).
    StereoMixdown,
    /// Orbit `i` routed to discrete stereo pair `[2 * i, 2 * i + 1]` (Multi-Channel Orbits).
    MultichannelOrbits,
}
```

### 3.2 `duckorbit` Sidechain Contract

- Any `TriggerEvent` with `duck_count > 0` injects a sidechain ducking impulse into each target `OrbitBus` listed in `duck_targets[0..duck_count]`.
- On target orbit `j`, the `gate` stage attenuates `synthBus` by gain `g(t)`:
  - Instantaneous drop to `1.0 - duck_depth` at the event's exact `frame_offset`.
  - Exponential or linear recovery back to `1.0` over `duck_attack` seconds.
- Multiple simultaneous ducking triggers on the same target orbit take `min(current_gain, new_duck_target)` so deeper ducks are never overridden by shallower ones.

### 3.3 `cut` Choke Groups

- `cut_group == 0`: Choking disabled; voice plays independently.
- `cut_group > 0`: When a new `TriggerEvent` with `cut_group = g` starts, all currently active voices with `cut_group == g` (across the same orbit, or globally when `cut_group < 0` per SuperDirt convention) immediately transition to a fast anti-click `CutFadeOut` state (`2 ms`–`5 ms` linear/cosine ramp to `0.0`), after which the voice slot is reclaimed.

### 3.4 Unbounded Polyphony & Voice-Stealing Policy

- Logical polyphony is unbounded from the pattern scheduler's perspective; `VoicePool` is pre-allocated with a high fixed capacity (e.g., `MAX_VOICES = 256` or `512`).
- If all slots are occupied when a new `TriggerEvent` arrives, `VoicePool::allocate_or_steal` MUST deterministically select a victim in strict priority order:
  1. **Tier 1 (Releasing / Choked):** Voice in `CutFadeOut` or `Release` phase with the lowest current envelope amplitude.
  2. **Tier 2 (Same Cut Group / Same Orbit Oldest):** Oldest active voice sharing the incoming `cut_group` or `orbit`.
  3. **Tier 3 (Global Quietest / Oldest):** Active voice with the lowest instantaneous RMS/envelope level, breaking ties by earliest `trigger_sample`.
- A stolen voice's previous sample state is crossfaded out over `64` frames (`~1.3 ms` at `48 kHz`) in a dedicated `StealFadeBuffer` to prevent clicks.

---

## 4. 1:1 Synth Engine Parity

All synth generators produce mono or stereo audio into the voice's pre-effects buffer, supporting continuous sample-accurate modulation from pitch, vibrato, and FM envelopes.

### 4.1 Basic Band-Limited Waveforms & Noise

```rust
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Waveform {
    Sine,
    Sawtooth,   // Alias: `saw`; PolyBLEP anti-aliased
    Square,     // PolyBLEP anti-aliased
    Triangle,   // Alias: `tri`; integrated PolyBLEP / DPW
    Supersaw,   // Unison detuned stereo sawtooths
    WhiteNoise,
    PinkNoise,  // Paul Kellet / Voss-McCartney -3 dB/oct filter
    BrownNoise, // Leaky integrator -6 dB/oct
    Crackle,    // Chaotic impulse/dust generator (`y[n] = |y[n-1] * chaos - offset|`)
}
```

- **Supersaw (`supersaw`):** Spawns `unison` (`1..=16`, default `5`) phase-randomized sawtooth oscillators. Frequencies are distributed symmetrically around `freq_hz` scaled by `detune`; stereo panning of odd/even detune pairs is scaled by `spread` (`0.0` = mono center, `1.0` = full L/R width). Gain is normalized by `1.0 / sqrt(unison)`.

### 4.2 Additive Synthesis (`partials` & `phases`)

- When `partial_count > 0`, the oscillator synthesizes a periodic waveform defined by `sum_{k=1..N} partials[k-1] * sin(2 * pi * k * phi + phases[k-1])`.
- Harmonic amplitudes above Nyquist (`k * f_0 >= f_s / 2`) are zeroed per-voice to guarantee alias-free playback.

### 4.3 8-Operator FM Matrix (`fm1..fm8`, `fm13`, `fm`, `fmh`, FM ADSR)

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct FmOpParams {
    pub ratio: f32,     // Frequency multiplier relative to carrier
    pub level: f32,     // Output / modulation amplitude
    pub feedback: f32,  // Self-feedback (`fm_matrix[i][i]`)
    pub waveform: Waveform,
}
```

- **2-Op Compatibility (`fm`, `fmh`, `fmattack`, `fmdecay`, `fmsustain`, `fmenv`):** Operator 1 acts as the primary modulator for the carrier at frequency `f_mod = f_0 * fmh`, with modulation index scaled by `fm * fm_adsr(t)` (where `fmenv` scales the envelope depth).
- **8-Op Matrix (`fm1..fm8`, `fm<src><dst>`):** Supports arbitrary routing where `fm_matrix[src - 1][dst - 1]` specifies the phase-modulation index from operator `src` into operator `dst` (e.g., `fm13` sets `fm_matrix[0][2]`). Self-modulation (`src == dst`) uses 1-sample delayed phase feedback stabilized against limit-cycle oscillation.

### 4.4 20-Parameter ZZFX Engine

Full 1:1 parity with Frank Force's ZZFX (`z_sine`, `z_sawtooth`, `z_triangle`, `z_square`, `z_tan`, `z_noise`):

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ZzfxParams {
    pub enabled: bool,
    pub shape: ZzfxShape,       // Sine | Triangle | Sawtooth | Tan | Noise | Square
    pub gain: f32,              // [0] Master volume
    pub zrand: f32,             // [1] Randomness factor
    pub freq: f32,              // [2] Base frequency (Hz)
    pub attack: f32,            // [3] Attack time (s)
    pub sustain_time: f32,      // [4] Sustain hold time (s)
    pub release: f32,           // [5] Release time (s)
    pub curve: f32,             // [7] Waveshape curve exponent
    pub slide: f32,             // [8] Linear frequency slide
    pub delta_slide: f32,       // [9] Acceleration of frequency slide
    pub pitch_jump: f32,        // [10] Step pitch jump (Hz)
    pub pitch_jump_time: f32,   // [11] Time before pitch jump (s)
    pub lfo: f32,               // [12] Repeat / LFO period (s)
    pub znoise: f32,            // [13] Phase noise / dirt amount
    pub zmod: f32,              // [14] FM modulation speed
    pub zcrush: f32,            // [15] Bit/sample crush [0.0, 1.0]
    pub zdelay: f32,            // [16] Internal comb delay time (s)
    pub sustain_vol: f32,       // [17] Sustain volume [0.0, 1.0]
    pub decay: f32,             // [18] Decay time (s)
    pub tremolo: f32,           // [19] LFO volume tremolo [0.0, 1.0]
}
```

### 4.5 AKWF `wt_*` Wavetables

- Any sound identifier prefixed with `wt_` resolves to a single-cycle wavetable buffer (supporting the 1000+ Adventure Kid Waveforms `AKWF` set and user-supplied wavetables).
- **Default Behavior:** `loop_mode` defaults to `LoopMode::WavetableDefault` (`loop = 1`), scanning between normalized `[loop_begin, loop_end]` (default `[0.0, 1.0]`) at phase increment `d_phi = f_0 / f_s` with cubic Hermite interpolation and mip-mapped anti-aliasing.

### 4.6 Pitch & Vibrato Modulators

At each sample `n`, instantaneous frequency is `f_inst[n] = f_0 * 2^((delta_penv[n] + delta_vib[n]) / 12)`, where `delta_penv[n]` is the pitch ADSR envelope scaled by `penv` (in semitones) and shaped by `pcurve`, and `delta_vib[n] = vibmod * sin(2 * pi * vib * t)`.


### 4.7 SoundFont (`.sf2` `gm_*`), Karplus-Strong Physical Modeling & Ring Modulator

```rust
/// General MIDI SoundFont (`.sf2`) preset playback (`gm_piano`, `gm_epiano1`, `gm_slap_bass_1`, etc.).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SoundFontVoiceParams {
    pub sf2_bank_id: u16,
    pub preset_num: u8,         // 0..=127 General MIDI program number
    pub bank_num: u8,           // 0 (melodic) or 128 (percussion)
    pub key: u8,                // MIDI note 0..=127
    pub velocity: u8,           // MIDI velocity 1..=127
}

/// Karplus-Strong plucked-string physical model (`s("pluck")` / `s("ks")`).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct KarplusStrongParams {
    pub freq_hz: f32,           // Delay-line length `L = sample_rate / freq_hz`
    pub damping: f32,           // Loop low-pass damping coefficient [0.0, 1.0]
    pub decay: f32,             // Loop feedback decay [0.9, 0.9999]
    pub pluck_pos: f32,         // Comb excitation position [0.0, 1.0]
}

/// Per-voice Ring Modulator (`ring`, `ringf`, `ringdf` — SuperDirt parity).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RingModParams {
    pub ring_wet: f32,          // Modulation depth (`ring`, [0.0, 1.0])
    pub ring_freq_hz: f32,      // Carrier frequency (`ringf`, Hz)
    pub ring_slide_hz: f32,     // Carrier glissando/slide (`ringdf`, Hz)
}
```

---

## 5. 1:1 Effects Signal Chain Parity

### 5.1 Per-Voice Serial Signal Chain

Every voice processes audio through the exact serial pipeline defined by `superdough`:

1. **`stretch` (OLA Phase Vocoder):** Time-stretches sample playback by `stretch` factor using Overlap-Add (Hann window, 75% overlap, pre-allocated ring/FFT scratch buffers) without altering pitch. Bypassed when `stretch == 0.0` or `1.0`.
2. **Main `gain` & Main ADSR:** Applies `gain` and the main amplitude envelope (`attack`, `decay`, `sustain`, `release`, triggered off at `duration_samples * clip`).
3. **Series Filters (`lpf` -> `hpf` -> `bpf` -> `vowel`):**
   - `ftype` selects the filter core for `lpf`, `hpf`, and `bpf`:
     - `12db`: 2-pole State Variable Filter (SVF, zero-delay feedback).
     - `24db`: Cascaded 4-pole SVF (`24 dB/oct`).
     - `ladder`: 4-pole nonlinear Huovilainen/Stilson Moog ladder filter with `tanh` saturation in the feedback path.
   - Each filter modulates its cutoff in octaves/semitones via its dedicated ADSR (`lpa`/`lpd`/`lps`/`lpr`/`lpenv`, `hpa`/`hpd`/`hps`/`hpr`/`hpenv`, `bpa`/`bpd`/`bps`/`bpr`/`bpenv`).
   - `vowel`: Parallel 3-band or 5-band formant filter bank supporting all 15 Strudel/SuperDirt vowel qualities (`a`, `e`, `i`, `o`, `u`, `ae`, `aa`, `oe`, `ue`, `y`, `uh`, `un`, `en`, `an`, `on`) and their Unicode/ASCII aliases (`å`/`ɑ` -> `aa`, `ø`/`ö` -> `oe`, `ı` -> `y`, `æ` -> `ae`).
4. **`coarse` (Sample-Rate Reduction):** Zero-order hold decimation holding every `coarse`-th sample (`coarse <= 1` bypassed).
5. **`crush` (Bit Crusher):** Quantizes amplitude to `2^crush` discrete levels (`crush <= 0.0` bypassed).
6. **`shape` (Waveshaper):** Soft-clipping transfer function `y = ((1 + k) * x) / (1 + k * |x|)` where `k = (2 * shape) / (1 - shape)` (clamped for `shape < 1.0`).
7. **`distort` (`dist`):** Stereo overdrive/saturation stage with pre-gain drive and post-compensation.
8. **`tremolo`:** Amplitude modulation LFO (`tremolo_rate`, `tremolo_depth`, `tremolo_shape`, `tremolo_phase`).
9. **`compressor`:** Per-voice dynamics compressor (`threshold_db`, `knee_db`, `ratio`, `attack_s`, `release_s`).
10. **`pan`:** Equal-power stereo panning for `pan` in `[0.0, 1.0]` (`L = cos(pi/2 * p)`, `R = sin(pi/2 * p)`), or `PanAz` for `>2` channels.
11. **`phaser`:** 4-to-6 stage all-pass filter network swept by a sine/triangle LFO (`phaser` rate, `phaser_depth`, `phaser_sweep`, `phaser_center`).
12. **`postgain` (`post`):** Final per-voice output trim before summing into `OrbitBus::synth_bus`.

### 5.2 Per-Orbit Global Effects (SuperDirt Parity)

| Effect | Parameters | DSP Contract |
| :--- | :--- | :--- |
| **`dirt_delay`** | `delay`, `delaytime`, `delayfeedback`, `delaySend`, `delayAmp`, `lock`, `cps` | Pre-allocated stereo delay line (`maxDelayTime = 4.0 s`). When `lock == true`, effective delay time is `delaytime / cps`. Includes `LeakDC` high-pass in feedback loop and smoothed parameter lag. |
| **`dirt_reverb`** | `room`, `size`, `dry`, optional IR `SampleId` | When no custom IR is bound: 4-stage input all-pass diffusion -> stereo FDN feedback loop (`OnePole` damping, `AllpassN`, `DelayN`, `LeakDC`) scaled by `size` (`linexp(0, 1, 0.01, 0.98)`). When a custom IR sample is bound: partitioned zero-latency convolution. |
| **`dirt_leslie`** | `leslie`, `lrate`, `lsize` | 800 Hz crossover splitting low/high bands; dual LFO-modulated Doppler delay (`distance / 343.0`) and amplitude throb with `Balance2` stereo rotation. |
| **`dirt_rms`** | `rmsReplyRate`, `rmsPeakLag` | Lock-free peak & RMS metering pushed to `rms_tx` for UI scope/meter telemetry. |
| **`dirt_monitor`** | `limitertype` (`0`=bypass, `1`=Limiter, `2`=Softclip) | Sums `dryBus + globalEffectBus`, sanitizes `NaN`/`Inf` to `0.0`, applies selected limiter (`Limiter` lookahead or `softclip(x * 0.5) * 2.0`), and writes to hardware output. |

---

## 6. Lazy Sample Loader (`strudel.json`, `github:user/repo`, Local)

### 6.1 Resolution & Lazy Loading Contract

Sample loading NEVER blocks the audio thread. All network I/O, filesystem I/O, and audio decoding (`symphonia` / `hound`) execute on an async background worker pool.

```rust
pub enum SampleSource {
    /// Parses `strudel.json` manifest using optional `_base` URL prefix.
    StrudelJsonUrl { url: String },
    /// Resolves `github:<user>/<repo>/<branch>` ->
    /// `https://raw.githubusercontent.com/<user>/<repo>/<branch>/strudel.json`
    /// Defaults: `<repo> = "samples"`, `<branch> = "main"`.
    GithubRepo { user: String, repo: String, branch: String },
    /// Scans local directory `<root>/<bank_or_sound>/<n>.{wav,flac,ogg,mp3}`
    /// or loads `<root>/strudel.json` if present.
    LocalPath { path: std::path::PathBuf },
}

pub trait SampleRegistry {
    /// Registers a manifest or directory index without decoding audio frames.
    fn register_source(&mut self, source: SampleSource) -> Result<ManifestHandle, LoaderError>;

    /// Interns `(bank, sound, n)` to a `SampleId` for POD `TriggerEvent` use.
    /// Triggers async background fetch + decode on first cache miss.
    fn resolve_or_enqueue(&mut self, bank: &str, sound: &str, n: u32) -> SampleId;
}
```

- **Cache-Miss Behavior:** If a `TriggerEvent` references a `SampleId` whose `basedrop::Shared<SampleBuffer>` has not yet arrived on `asset_rx`, the voice outputs silence (`0.0`) for that event without blocking or glitching the audio thread.

### 6.2 Sample Playback & Manipulation Parameters

| Parameter | Domain / Default | DSP Contract |
| :--- | :--- | :--- |
| `bank` | `Option<BankId>` | Prefixes sound lookup (e.g., `bank("RolandTR909")` + `s("bd")` -> `RolandTR909_bd`). |
| `n` | `u32` (default `0`) | Indexes into the sound's sample array modulo array length (`n % len`). |
| `begin` / `end` | `f32` `[0.0, 1.0]` (`0.0`, `1.0`) | Defines sub-region `[begin * len, end * len]` in frames. If `begin > end`, plays in reverse over `[end, begin]`. |
| `loop` | `bool` (default `false`; `true` for `wt_*`) | Loops playback between `[loop_begin, loop_end]` (falling back to `[begin, end]`). |
| `speed` | `f32` (default `1.0`) | Cubic-interpolated resampling ratio. Negative values reverse playback direction starting from `end`. |
| `fit` | `f32` (default `0.0`) | When `> 0.0`, overrides `speed` to `(region_frames / sample_rate) / (event_duration_s / fit)`. |
| `chop` | `u16` (default `0`) | Subdivides `[begin, end]` into `chop` equal contiguous grains triggered sequentially across the pattern. |
| `striate` | `u16` (default `0`) | Interleaves `striate` slices across consecutive cycles/events with `begin`/`end` windowing. |
| `slice` | `(u16, u16)` (`slice_idx`, `slice_count`) | Selects slice `slice_idx % slice_count` of `[begin, end]` at native `speed`. |
| `splice` | `(u16, u16)` (`slice_idx`, `slice_count`) | Same as `slice`, with `splice_mode = true` automatically time-fitting the slice's `speed` to `duration_samples`. |
| `scrub` | `f32` `[0.0, 1.0]` | Positions the playhead directly at `scrub * (end - begin) + begin` for continuous scrubbing/granular scanning. |

---

## 7. TDD Verification & Test Contracts

All test suites MUST pass in CI before merging any DSP or loader changes.

### 7.1 Real-Time Safety & Memory Contract Tests

| Test ID | Assertion |
| :--- | :--- |
| `RT-01` | **Zero-Allocation Callback:** Wrap the test harness in a thread-local counting allocator (`assert_no_alloc`). Run `process_block` for 10,000 blocks under full 256-voice polyphony + all orbit FX; assert `alloc_count == 0` and `dealloc_count == 0`. |
| `RT-02` | **Deferred Reclamation (`rtrb-basedrop`):** Drop `SampleBuffer` and `WavetableBuffer` handles on the audio thread; assert deallocation occurs exclusively on the `basedrop::Collector` thread. |
| `RT-03` | **SPSC Backpressure:** Saturate `event_rx` beyond `RING_CAPACITY`; assert `Producer::push` returns `Err(PushError::Full)` without blocking or panicking, and `Consumer` recovers cleanly. |
| `RT-04` | **Sub-Block Sample Accuracy:** Schedule a `TriggerEvent` with `trigger_sample = block_start + 37`; assert output frames `0..37` are `0.0` and frame `37` contains the exact onset sample. |
| `RT-05` | **Latency Budget:** Measure `process_block` worst-case execution time for 128 frames at `48 kHz` (`2.67 ms` budget) with 64 active voices and 4 active orbits; assert `p99_exec_time < 1.33 ms` (<50% CPU headroom, total hardware I/O + DSP < `10 ms`). |

### 7.2 DSP Accuracy & 1:1 Parity Tests

| Test ID | Assertion |
| :--- | :--- |
| `DSP-01` | **Band-Limited Oscillators:** Render `sawtooth` and `square` at `4000 Hz` (`48 kHz` sample rate); assert alias energy above Nyquist foldback is suppressed by `>= 50 dB` relative to fundamental. |
| `DSP-02` | **Supersaw:** Render `supersaw` with `unison = 7`, `detune = 0.5`, `spread = 1.0`; assert stereo decorrelation (`|L - R|` RMS > 0) and normalized RMS within `±1 dB` of `unison = 1`. |
| `DSP-03` | **Noise Spectra:** FFT spectral slope of `white` (`0 dB/oct`), `pink` (`-3 dB/oct ± 0.5 dB`), and `brown` (`-6 dB/oct ± 0.5 dB`) over `100 Hz..10 kHz`; `crackle` produces non-periodic impulses. |
| `DSP-04` | **8-Op FM & `fm13`:** Configure `fm13 = 2.0` with `fm1` and `fm3` active; assert sideband peaks at `f_c ± k * f_m` match analytical Bessel `J_k(beta)` magnitudes within `0.5 dB`. |
| `DSP-05` | **ZZFX Determinism:** Render all 20-parameter `ZzfxParams` presets with `zrand = 0.0`; assert bit-exact deterministic output across runs. |
| `DSP-06` | **Filter Topologies & Envelopes:** Sweep `lpf`, `hpf`, `bpf` across `12db`, `24db`, and `ladder` with `lpenv > 0`; assert `-12 dB/oct` vs `-24 dB/oct` stopband slopes and `vowel` formant peaks match reference formant frequencies. |
| `DSP-07` | **Serial Chain Ordering:** Verify `crush` -> `shape` -> `pan` -> `phaser` -> `postgain` order by feeding a known ramp and asserting quantization precedes waveshaping and stereo panning precedes phaser modulation. |
| `DSP-08` | **NaN/Inf Monitor Guard:** Inject `f32::NAN` and `f32::INFINITY` into `synth_bus`; assert `monitorOut` outputs `0.0` for all affected frames with no filter state corruption. |

### 7.3 Voice Lifecycle, Choking, & Orbit Routing Tests

| Test ID | Assertion |
| :--- | :--- |
| `LIFE-01` | **`cut` Choke Group:** Trigger Voice A (`cut = 1`), then Voice B (`cut = 1`) 100 frames later; assert Voice A decays to `0.0` within the anti-click ramp (`<= 240` frames at `48 kHz`) without discontinuity clicks (`|x[n] - x[n-1]| < threshold`). |
| `LIFE-02` | **Voice Stealing:** Fill `VoicePool` to `MAX_VOICES`, trigger one more event; assert a releasing/quietest voice is stolen cleanly with a 64-frame crossfade and zero panics. |
| `LIFE-03` | **`duckorbit` Sidechain:** Trigger kick on Orbit `0` with `duckorbit = [1, 2]`, `duckdepth = 0.8`, `duckattack = 0.2`; assert Orbits `1` and `2` attenuate by `-14 dB` (`0.2x` linear) at onset and recover to `1.0x` at `t = 0.2 s`. |
| `LIFE-04` | **Multichannel Orbits:** Switch `OutputRoutingMode::MultichannelOrbits`; assert Orbit `3` writes exclusively to channels `[6, 7]` while all other channels remain `0.0`. |

### 7.4 Sample Loader & Manipulation Tests

| Test ID | Assertion |
| :--- | :--- |
| `SAMP-01` | **`strudel.json` & `github:` Resolution:** Resolve `github:tidalcycles/dirt-samples` (defaults to `main`) and custom `_base` manifests; assert lazy registration does not fetch audio until `resolve_or_enqueue` is called. |
| `SAMP-02` | **AKWF `wt_*` Auto-Loop:** Load `wt_001` with default parameters; assert `loop_mode` resolves to looping and sustains indefinitely for `duration_samples`. |
| `SAMP-03` | **`begin`/`end`/`speed`/`splice`:** Play a 48,000-frame sample with `begin = 0.25`, `end = 0.75`, `speed = -1.0`; assert playback starts at frame `36,000` and steps backward to `12,000`. Assert `splice` scales playback rate to match `duration_samples`. |

---

## 8. 1:1+ & Category-King DSP Extensions (`PP-01`, `PP-02`, `PP-09`–`PP-11`, `CK-03`, `CK-05`)

1. **64-Sample Sub-Block Continuous Modulation Buses (`ControlSubBlockFrame`)**: Evaluates all 39 `ModulatableParam` targets at `k-rate` (every 64 samples with per-sample SIMD linear ramps) or `a-rate` using a `u64` dirty-bit bitmask so idle modulation lanes cost zero CPU cycles and never retrigger voice envelopes.
2. **Ordered Multi-Stage Per-Voice Insert FX (`OrderedVoiceFxChain`)**: Preserves up to 8 inline insert stages (`[Option<InsertFxStage>; 8]`) per voice in exact call order (`.lpf(200).distort(3.0).lpf(1200)`) with $1/\sqrt{N}$ equal-power multi-orbit send compensation.
3. **Seeded 8x8 Hadamard FDN Reverb + Dual-Buffer Crossfaded Convolution**: Zero main-thread buffer regeneration; custom IR changes crossfade over `50 ms` using `session_seed` for bit-exact offline stem bounces.
4. **Dedicated `GranularSamplerVoice` & PolyBLEP (`PolyBlepOscillator`) Inline Pipe Oscillators**: Executes `.chop(n)`, `.striate(n)`, `.scrub(pos)`, and `.grain()` inside a single parent voice with up to 512 windowed grains and PolyBLEP anti-aliased oscillators.
5. **Octatrack Live Looper Freeze (`.ot_freeze(cycles)`)**: Captures a rolling master/orbit ring buffer for seamless live code hot-swapping.
6. **AST Pre-Roll Sample Pre-Fetcher, Two-Tier Progressive Starter Cache & Mmap Streaming**:
   - Pre-fetches AST-referenced samples prior to cycle 0 (`StaleWhileRevalidate` manifest cache).
   - Keeps core WASM + procedural micro-bank `< 1.8 MB` gzipped while caching the `15 MB` royalty-free starter pack in IndexedDB/disk; streams stems `> 10 seconds` via memory-mapped (`mmap`) ring buffers.
   - Resolves simultaneous same-frame `cut` groups deterministically by `(stream_priority, hap_ordinal)` with a `2.5 ms` de-click ramp.
7. **Selectable `EngineCompatibilityProfile` (`SuperDoughC4` vs `SuperDirtC5`)**: Configurable middle-C (`C4` vs `C5`), filter ADSR inheritance, sustained synth headroom (`0.5x`), and `z1..z4` oscillator macro parameters.
8. **Hardware `FTZ`/`DAZ` & Non-Finite Feedback Sanitization**: Enables CPU Flush-to-Zero (`FTZ`) and Denormals-Are-Zero (`DAZ`) flags on the audio callback thread and applies branchless `sanitize_f32_finite` (`NaN`/`Inf` $\rightarrow 0.0$, soft-limiter at $\pm 4.0$) in all filter, waveguide, and reverb feedback loops.
