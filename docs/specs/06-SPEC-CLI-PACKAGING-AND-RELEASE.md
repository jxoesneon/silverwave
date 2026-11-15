# 06 — Specification: `silverwave-cli`, Multi-Platform Packaging, and Production Release Engineering (`v0.7.0` → `v1.0.0` GA)

| Metadata Field | Value | 
| ----- | ----- | 
| **Document ID** | `06-SPEC-CLI-PACKAGING-AND-RELEASE` | 
| **Target Crates** | `silverwave-cli`, `silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io`, `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-mcp`, `silverwave-plugin` | 
| **Release Horizon** | `v0.7.0` (CLI & Daemon Alpha) → `v0.8.0` (Crash-Safe Live Recovery & Plugin Bundles) → `v0.9.0` (Multi-OS Installers & `wasm32` PWA) → `v1.0.0` GA | 
| **Normative Standards** | [Semantic Versioning 2.0.0](https://semver.org/), [MIDI 1.0 Control Change Specification](https://midi.org/midi-1-0-control-change-messages), [VST 3 Plug-In Format](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html), [CLAP Audio Plugin API](https://github.com/free-audio/clap), [W3C Web Audio API 1.1](https://www.w3.org/TR/webaudio-1.1/), [SLSA v1.0 Level 3](https://slsa.dev/), [OWASP CycloneDX](https://github.com/cyclonedx/cyclonedx-rust-cargo) | 
| **License Expression** | [`MIT OR Apache-2.0`](https://spdx.org/licenses/) | 

## Architectural Scope & Release Milestone Progression

SilverWave provides a pure-Rust live-coding pattern and synthesis environment combining 1:1 behavioral parity with [Strudel](https://codeberg.org/uzu/strudel) and [TidalCycles](https://tidalcycles.org/). This specification defines the normative contracts for the `silverwave-cli` binary suite, the panic-safe live performance supervisor, native desktop and plugin distribution formats across Linux, macOS, Windows, and WebAssembly, supply-chain security attestations, and the automated verification gates required to advance from `v0.7.0` to `v1.0.0` General Availability (GA).

| Milestone Version | Engineering Scope | Mandatory Release Gate | 
| ----- | ----- | ----- | 
| **`v0.7.0`** | Complete `silverwave-cli` 14-subcommand surface (`repl`, `play`, `eval`, `render`, `osc`, `midi`, `samples`, `lsp`, `mcp`, `share`, `fmt`, `check`, `doctor`, `completions`) and deterministic exit/diagnostic contracts. | 100% CLI flag/subcommand schema compliance, golden stdout/stderr snapshot tests, and shell completion generation across 6 shells. | 
| **`v0.8.0`** | Crash/Panic-Safe Live Performance Recovery supervisor, lock-free [MIDI CC 120/123](https://midi.org/midi-1-0-control-change-messages) hardware flush, audio buffer zeroing, atomic unsaved buffer autosave, and [VST3](https://github.com/steinbergmedia/vst3sdk) / [CLAP](https://cleveraudio.org/) bundle generation. | Fault-injected panic recovery verified under $\le 2\text{ ms}$ MIDI flush latency; [`pluginval`](https://github.com/Tracktion/pluginval) strictness 10 and [`clap-validator`](https://github.com/free-audio/clap-validator) pass with zero errors. | 
| **`v0.9.0`** | Multi-platform packaging: Linux (`.deb`, `.rpm`, `.flatpak`, `AppImage`, [Arch Linux `PKGBUILD`](https://wiki.archlinux.org/title/PKGBUILD)), macOS Universal2 (`.app`, `.dmg`, `.pkg`, [Hardened Runtime](https://www.electron.build/docs/notarization/), `notarytool`, [Homebrew](https://docs.brew.sh/)), Windows ([WiX v4 `.msi`](https://www.firegiant.com/wixtoolset/), NSIS, [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/), Scoop), and `wasm32` PWA with [COOP/COEP `SharedArrayBuffer`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy) isolation. | Clean-VM installer smoke tests across all target OS architectures and headless browser [`AudioWorkletProcessor`](https://developer.mozilla.org/en-US/docs/Web/API/AudioWorkletProcessor) ring-buffer verification. | 
| **`v1.0.0` GA** | Topological `crates.io` publication, [SLSA Level 3](https://slsa.dev/) provenance, [CycloneDX SBOM](https://github.com/cyclonedx/cyclonedx-rust-cargo), [SPDX `MIT OR Apache-2.0`](https://spdx.org/licenses/) + [AKWF](https://www.adventurekid.se/akrt/waveforms/adventure-kid-waveforms/) legal audit, and frozen [SemVer 2.0.0](https://semver.org/) public API/wire contracts. | Zero open security advisories, verified Sigstore provenance bundles, `cargo-semver-checks` pass, and $\ge 99\%$ line ($\ge 95\%$ branch) workspace test coverage ($100\%$ on safety/recovery paths). | 

## 1. `silverwave-cli` Subcommand & Flag Contracts

### 1.1 Global Invocation, Environment Variables, and Exit Codes

```text
silverwave [GLOBAL_OPTIONS] <SUBCOMMAND> [SUBCOMMAND_OPTIONS] [ARGS...]
```

| Global Flag | Short | Type / Value | Default | Environment Variable | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | ----- | 
| `--config <PATH>` | `-c` | `PathBuf` | `$XDG_CONFIG_HOME/silverwave/config.toml` | `SILVERWAVE_CONFIG` | Overrides the path to the workspace/user TOML configuration file. | 
| `--log-level <LEVEL>` | `-l` | `trace \| debug \| info \| warn \| error` | `info` | `SILVERWAVE_LOG` | Sets structured diagnostic verbosity. | 
| `--log-format <FMT>` |  | `pretty \| compact \| json` | `pretty` | `SILVERWAVE_LOG_FMT` | Selects terminal or JSON log serialization. | 
| `--color <WHEN>` |  | `auto \| always \| never` | `auto` | `NO_COLOR` | Controls ANSI color emission. | 
| `--runtime-dir <DIR>` |  | `PathBuf` | `$XDG_RUNTIME_DIR/silverwave` | `SILVERWAVE_RUNTIME_DIR` | Directory for IPC sockets, PID files, and lock-free shared-memory recovery segments. | 
| `--sample-dir <DIR>` | `-s` | `PathBuf` | `$XDG_DATA_HOME/silverwave/samples` | `SILVERWAVE_SAMPLE_DIR` | Root directory for local sample banks, `.sf2` files, and [AKWF](https://github.com/KristofferKarlAxelEkstrand/AKWF-FREE) wavetables. | 
| `--no-recovery-hook` |  | `bool` | `false` | `SILVERWAVE_NO_RECOVERY` | Disables the live-performance crash/panic supervisor (intended only for core-dump debugging). | 
| `--version` | `-V` | `flag` |  |  | Prints `silverwave <SEMVER> (<GIT_SHA> <TARGET_TRIPLE> <BUILD_DATE>)` to `stdout` and exits `0`. | 
| `--help` | `-h` | `flag` |  |  | Prints help synopsis for the root command or active subcommand to `stdout` and exits `0`. | 

| Exit Code | Symbolic Constant | Meaning & Trigger Condition | 
| ----- | ----- | ----- | 
| `0` | `EXIT_SUCCESS` | Clean execution and termination. | 
| `1` | `EXIT_EVAL_ERROR` | Pattern parse error, type mismatch, or evaluation failure in `eval` or `repl`. | 
| `2` | `EXIT_CLI_USAGE` | Invalid CLI flag, unknown subcommand, or missing required positional argument (`clap` usage error). | 
| `64` | `EX_USAGE` | Malformed mini-notation or incompatible flag combination (`--stem-split` with single-stream stdout). | 
| `65` | `EX_DATAERR` | Corrupted sample manifest, invalid `.sf2` header, or hash mismatch in `samples verify`. | 
| `66` | `EX_NOINPUT` | Specified input pattern file, configuration file, or sample archive does not exist. | 
| `69` | `EX_UNAVAILABLE` | Requested audio backend ([PipeWire](https://pipewire.org/), [JACK](https://jackaudio.org/), ALSA, CoreAudio, WASAPI, ASIO) or MIDI port could not be opened. | 
| `70` | `EX_SOFTWARE` | Unrecoverable supervisor failure after exhausting worker restart retries. | 
| `74` | `EX_IOERR` | Disk write failure during offline `render`, `plugin-bundler` output, or crash autosave. | 
| `78` | `EX_CONFIG` | Invalid `config.toml` syntax or failed system readiness check in `silverwave doctor --strict`. | 
| `130` | `EXIT_SIGINT` | Graceful termination triggered by `SIGINT` (`Ctrl+C`) after completing emergency MIDI note-off flush. | 

### 1.2 Subcommand Contracts

#### 1.2.1 `silverwave repl`

Starts the interactive live-coding terminal interface and Read-Eval-Print Loop with cycle-quantized hot-swapping, pattern timeline visualization, and panic-safe worker supervision.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--file <PATH>` | `-f` | `Option<PathBuf>` | `None` | Preloads and optionally evaluates a `.swave` or `.strudel` script on startup. | 
| `--connect <ADDR_OR_SOCK>` |  | `Option<Endpoint>` | Local spawn | Connects to an existing `silverwave daemon` via UNIX socket or TCP instead of spawning an in-process engine. | 
| `--audio-backend <BACKEND>` | `-a` | `auto \| pipewire \| jack \| alsa \| coreaudio \| wasapi \| asio` | `auto` | Selects real-time audio driver backend. | 
| `--audio-device <NAME>` | `-d` | `Option<String>` | System default | Binds to a named hardware output/input device. | 
| `--sample-rate <HZ>` | `-r` | `u32` (`44100 \| 48000 \| 88200 \| 96000`) | `48000` | Sets hardware sample rate in Hz. | 
| `--buffer-size <FRAMES>` | `-b` | `u32` (`32 \| 64 \| 128 \| 256 \| 512`) | `128` | Sets audio callback buffer size in frames. | 
| `--cps <FLOAT>` |  | `f64` (`> 0.0`) | `0.5` | Initial cycles-per-second tempo ($0.5\text{ cps} = 120\text{ BPM}$ in $4/4$). | 
| `--midi-out <PORT_SPEC>` |  | `Vec<String>` | `[]` | Opens one or more hardware/virtual MIDI output ports for pattern routing and emergency CC 120/123 flush. | 
| `--midi-clock-source <SRC>` |  | `internal \| midi-in \| ableton-link` | `internal` | Selects master timing reference. | 
| `--keymap <MODE>` |  | `emacs \| vim \| helix` | `vim` | Selects modal or chordal editor keybindings. | 
| `--headless-repl` |  | `bool` | `false` | Disables full-screen TUI chrome and runs a line-oriented readline stream over `stdin`/`stdout`. | 
| `--recovery-mode <MODE>` |  | `resume-last-good \| mute-standby` | `resume-last-good` | Configures post-panic worker behavior. | 

#### 1.2.2 `silverwave play` / `silverwave daemon`

Runs the headless real-time audio synthesis, sample engine, and cycle scheduler as a background service or continuous player controlled via IPC, [LSP](https://microsoft.github.io/language-server-protocol/), [MCP](https://modelcontextprotocol.io/specification/2026-07-28), or OSC.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--socket <PATH>` |  | `PathBuf` | `$SILVERWAVE_RUNTIME_DIR/daemon.sock` | UNIX domain socket path (or Named Pipe `\\.\pipe\silverwave-daemon` on Windows). | 
| `--listen-tcp <IP:PORT>` |  | `Option<SocketAddr>` | `None` | Optional loopback TCP control endpoint (`127.0.0.1:57140`). | 
| `--pid-file <PATH>` |  | `PathBuf` | `$SILVERWAVE_RUNTIME_DIR/daemon.pid` | Writes exclusive locked process ID file on startup. | 
| `--foreground` | `-F` | `bool` | `false` | Keeps the daemon attached to the controlling terminal for `systemd` / `launchd` supervision. | 
| `--realtime-priority <PRIO>` |  | `u8` (`1..=99`) | `85` | Requests POSIX `SCHED_FIFO` or RealtimeKit (`rtkit`) priority on Linux, `THREAD_TIME_CRITICAL` / MMCSS on Windows, and Mach time-constraint threads on macOS. | 
| `--lock-memory` |  | `bool` | `true` | Invokes `mlockall(MCL_CURRENT \| MCL_FUTURE)` to prevent page faults on the audio thread. | 
| `--watchdog-timeout-ms <MS>` |  | `u64` | `50` | Maximum heartbeat stall before the supervisor triggers emergency audio mute, MIDI flush, and worker respawn. | 
| `--stop` |  | `bool` | `false` | Signals a running daemon instance to flush MIDI notes, close audio devices cleanly, and exit `0`. | 
| `--status` |  | `bool` | `false` | Queries the running daemon's health, active cycle, DSP CPU load, and xrun count as JSON or table. | 

#### 1.2.3 `silverwave lsp`

Starts the [Language Server Protocol 3.17](https://microsoft.github.io/language-server-protocol/) server for IDE/editor integrations (VS Code, Neovim, Helix, Zed, Emacs), providing real-time mini-notation AST diagnostics, inline cycle event visualizations, sample bank completions, and direct evaluation commands (`silverwave.evalBlock`, `silverwave.hush`, `silverwave.panic`).

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--stdio` |  | `bool` | `true` | Communicates JSON-RPC 2.0 messages over standard input and standard output. | 
| `--socket <PORT>` |  | `Option<u16>` | `None` | Accepts LSP JSON-RPC connections over loopback TCP on `<PORT>`. | 
| `--pipe <NAME>` |  | `Option<PathBuf>` | `None` | Communicates over a UNIX domain socket or Windows Named Pipe. | 
| `--daemon-socket <PATH>` |  | `Option<PathBuf>` | Auto-discover | Attaches the LSP server to a live `silverwave daemon` to stream live playhead highlights and active cycle telemetry into the editor. | 
| `--client-process-id <PID>` |  | `Option<u32>` | `None` | Monitors the parent editor PID and terminates cleanly if the host editor exits abnormally. | 

#### 1.2.4 `silverwave eval`

Evaluates a SilverWave / [Strudel](https://codeberg.org/uzu/strudel) / [TidalCycles](https://tidalcycles.org/) expression or script file deterministically, either querying discrete cycle events to `stdout` or dispatching live updates to a running `silverwave daemon`.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--expr <CODE>` | `-e` | `Option<String>` | `None` | Evaluates an inline pattern expression (mutually exclusive with `--file`; reads `stdin` if both omitted). | 
| `--file <PATH>` | `-f` | `Option<PathBuf>` | `None` | Evaluates the pattern script at `<PATH>`. | 
| `--cycles <SPAN>` | `-n` | `CycleSpan` (`start..end` or `count`) | `0..1` | Queries events over the rational cycle interval $[start, end)$ in offline query mode. | 
| `--cps <FLOAT>` |  | `f64` | `0.5` | Sets cycles-per-second for absolute timestamp calculation. | 
| `--format <FMT>` |  | `table \| json \| ndjson` | `table` | Selects output serialization format. | 
| `--send-daemon` | `-D` | `bool` | `false` | Hot-swaps the evaluated pattern into the active `silverwave daemon` on the next quantization boundary. | 
| `--slot <SLOT_ID>` |  | `String` (`d1`..`d16` or custom) | `d1` | Target pattern orbit/slot when dispatching to the daemon. | 
| `--quantize <CYCLES>` | `-q` | `Rational` | `1/1` | Cycle boundary alignment (`1/1`, `1/4`, `0` for immediate) for hot-swapping. | 
| `--hush` |  | `bool` | `false` | Immediately silences all active pattern slots on the target daemon. | 

#### 1.2.5 `silverwave render`

Executes deterministic, reproducible faster-than-realtime offline DSP and 60 FPS GPU video rendering of a pattern script to lossless/compressed audio or video files.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--input <PATH>` | `-i` | `PathBuf` | Required (`-` for `stdin`) | Input `.swave` script file to render. | 
| `--output <PATH>` | `-o` | `PathBuf` | Required | Output file path (or directory when `--stem-split` is enabled). | 
| `--format <AUDIO_FMT>` |  | `wav \| flac \| ogg \| mp4 \| webm` | `wav` | Output container/codec. | 
| `--cycles <F64>` | `-c` | `Option<f64>` | `Some(16.0)` | Total number of cycles to render (mutually exclusive with `--duration-secs`). | 
| `--duration-secs <F64>` | `-t` | `Option<f64>` | `None` | Exact wall-clock duration in seconds to render. | 
| `--tail-secs <F64>` |  | `f64` | `2.0` | Extra decay window appended after pattern end to capture reverb/delay tails. | 
| `--sample-rate <HZ>` | `-r` | `u32` | `48000` | Offline render sample rate (`44100`, `48000`, `88200`, `96000`, `192000`). | 
| `--bit-depth <BITS>` |  | `16 \| 24 \| 32f` | `24` | PCM quantization depth. | 
| `--dither <MODE>` |  | `none \| rect \| tpdf` | `tpdf` | Dithering probability density function for integer bit depths. | 
| `--stem-split` |  | `bool` | `false` | Renders each orbit (`d1..d16` / named orbits) plus master bus into separate phase-aligned stem files. | 
| `--seed <U64>` |  | `u64` | `0x5117_E77A_7E00_0001` | Deterministic PRNG seed ensuring bit-identical output across architectures for stochastic combinators (`rand`, `perlin`, `degradeBy`). | 
| `--normalize-lufs <LUFS>` |  | `Option<f64>` | `None` | Optional EBU R128 integrated loudness normalization target (e.g., `-14.0`). | 

#### 1.2.6 `silverwave osc` & `silverwave midi`

Provides a high-precision, zero-allocation Open Sound Control (OSC 1.0 / 1.1) and MIDI bridge translating between SilverWave's internal pattern engine, SuperDirt/SuperCollider (`/dirt/play`), [TidalCycles](https://tidalcycles.org/) (`/_`), MIDI clock, and external shader/DMX/TouchOSC visualizers.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--listen <ADDR:PORT>` | `-L` | `SocketAddr` | `127.0.0.1:57121` | Local UDP/TCP socket for incoming OSC bundles and clock messages. | 
| `--target <ADDR:PORT>` | `-T` | `Vec<SocketAddr>` | `["127.0.0.1:57120"]` | One or more destination OSC endpoints (e.g., SuperDirt default port `57120`). | 
| `--transport <PROTO>` |  | `udp \| tcp \| unix` | `udp` | Network transport protocol. | 
| `--dialect <DIALECT>` |  | `superdirt \| tidal \| strudel` | `superdirt` | Wire payload dialect. | 
| `--latency-secs <F64>` |  | `f64` | `0.050` | NTP timetag lookahead offset added to outgoing OSC bundle headers. | 
| `--clock-broadcast-hz <HZ>` |  | `u32` | `20` | Frequency of `/cycle/tick` and `/cps` telemetry broadcasts to connected visualizers. | 

#### 1.2.7 `silverwave samples`

Manages sample packs, [SoundFont 2.04](https://www.synthfont.com/sfspec24.pdf) (`.sf2`) instruments, and [Adventure Kid Waveforms (AKWF)](https://www.adventurekid.se/akrt/waveforms/adventure-kid-waveforms/) single-cycle wavetables with cryptographic BLAKE3 integrity verification and license auditing.

| Nested Action | Synopsis & Flags | Contract Description | 
| ----- | ----- | ----- | 
| `samples index` | `silverwave samples index [--dir <PATH>] [--output <MANIFEST_PATH>]` | Scans sample directories, extracts sample rate/channels/loop points/root MIDI note, computes BLAKE3 digests, and writes `samples.lock.json`. | 
| `samples verify` | `silverwave samples verify [--manifest <PATH>] [--strict-licenses]` | Verifies every indexed audio asset against its BLAKE3 digest and validates that an SPDX license identifier is present. Exits `65` (`EX_DATAERR`) on mismatch. | 
| `samples import` | `silverwave samples import <SOURCE_URI_OR_PATH> [--bank-name <NAME>] [--convert-flac]` | Imports a local folder, `.sf2` SoundFont, or Dirt-Samples archive into `$SILVERWAVE_SAMPLE_DIR` and updates `samples.lock.json`. | 
| `samples pack` | `silverwave samples pack <DIR> --output <ARCHIVE.swpack>` | Creates a compressed, content-addressed `.swpack` bundle with embedded `THIRD_PARTY_AUDIO_LICENSES.json` metadata for offline distribution or `wasm32` PWA preloading. | 
| `samples list` | `silverwave samples list [--filter <PREFIX>] [--format <table\|json>]` | Lists all registered banks, variant counts, and memory footprint. | 

#### 1.2.8 `silverwave plugin-bundler` (`bundle-plugin`)

Compiles, packages, signs, and validates [Steinberg VST3](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html) (`.vst3`) and [CLAP](https://github.com/free-audio/clap) (`.clap`) audio plugin bundles from the `silverwave-plugin` shared library target.

| Flag / Option | Short | Type | Default | Contract Description | 
| ----- | ----- | ----- | ----- | ----- | 
| `--format <FMT>` | `-f` | `vst3 \| clap \| all` | `all` | Selects plugin bundle format(s). | 
| `--target <TRIPLE>` | `-t` | `TargetTriple` (`universal2-apple-darwin` supported) | Host target | Rust target triple or `universal2-apple-darwin` (invokes dual-target build + `lipo -create`). | 
| `--release` |  | `bool` | `true` | Builds with `--profile release-lto` (`lto = "fat"`, `codegen-units = 1`, `panic = "unwind"` at FFI boundary). | 
| `--out-dir <DIR>` | `-o` | `PathBuf` | `target/bundled-plugins` | Output directory for `SilverWave.vst3` and `SilverWave.clap`. | 
| `--install-user` |  | `bool` | `false` | Copies generated bundles into the OS-standard per-user VST3 and CLAP plugin directories. | 
| `--sign-identity <ID>` |  | `Option<String>` | `None` | Signs the bundle using Apple `codesign` (macOS) or Microsoft `signtool` (Windows). | 
| `--validate` |  | `bool` | `false` | Automatically runs [`pluginval`](https://github.com/Tracktion/pluginval) (`--strictness-level 10`) and [`clap-validator`](https://github.com/free-audio/clap-validator) on the emitted bundles. | 

#### 1.2.9 `silverwave doctor`, `fmt`, `check`, `share`, `mcp` & `completions`

Executes non-destructive system diagnostics (`doctor`), canonical AST code formatting (`fmt`), static syntax/bank validation (`check`), stateless `#sw=` LZ4 permalink encoding (`share`), MCP server launch (`mcp`), and shell completion generation (`completions`).

### 1.3 Rust CLI & Subcommand Type Signatures

```rust
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilverWaveExitCode {
    Success = 0,
    EvalError = 1,
    CliUsage = 2,
    UsageError = 64,
    DataError = 65,
    NoInput = 66,
    ServiceUnavailable = 69,
    InternalSoftwareError = 70,
    IoError = 74,
    ConfigError = 78,
    Interrupted = 130,
}

impl From<SilverWaveExitCode> for ExitCode {
    fn from(code: SilverWaveExitCode) -> Self;
}

#[derive(Debug, Clone)]
pub struct GlobalOptions {
    pub config: Option<PathBuf>,
    pub log_level: LogLevel,
    pub log_format: LogFormat,
    pub color: ColorChoice,
    pub runtime_dir: PathBuf,
    pub sample_dir: PathBuf,
    pub no_recovery_hook: bool,
}

#[derive(Debug, Clone)]
pub enum Subcommand {
    Repl(ReplArgs),
    Play(DaemonArgs),
    Lsp(LspArgs),
    Mcp(McpArgs),
    Eval(EvalArgs),
    Render(RenderArgs),
    Osc(OscBridgeArgs),
    Midi(MidiArgs),
    Samples(SamplerSubcommand),
    Share(ShareArgs),
    Fmt(FmtArgs),
    Check(CheckArgs),
    Doctor(DoctorArgs),
    Completions(CompletionsArgs),
}

pub trait SubcommandContract: Send + Sync {
    type Args: Send + Sync;
    type Output: serde::Serialize + Send;
    type Error: std::error::Error + Send + Sync + 'static;

    fn execute(
        &self,
        global: &GlobalOptions,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error>;

    fn map_error_to_exit_code(err: &Self::Error) -> SilverWaveExitCode;
}
```

## 2. Crash/Panic-Safe Live Performance Recovery Contract

During a live performance, an unexpected panic in pattern evaluation, a custom DSP node, or a hot-swapped script must never leave external synthesizers droning on stuck MIDI notes, freeze the hardware audio ring buffer into a high-amplitude buzz, lose the performer's unsaved live-coding buffer, or terminate the host process (`silverwave repl`, `silverwave play`, or a DAW hosting `SilverWave.vst3` / `SilverWave.clap`).

### 2.1 Four-Stage Real-Time Panic & Fault Recovery Sequence

```text
[Worker Thread Panic / Watchdog Heartbeat Timeout (>50 ms)]
        │
        ├──► Stage 1 (≤ 2 ms): Lock-Free Emergency MIDI Panic Flush
        │      • For each active MIDI Output Port & Channel 0..=15:
        │        - Emit CC 120 (0xB0 | ch, 0x78, 0x00) — All Sound Off
        │        - Emit CC 123 (0xB0 | ch, 0x7B, 0x00) — All Notes Off
        │        - Emit CC 64  (0xB0 | ch, 0x40, 0x00) — Sustain Pedal Off
        │        - Emit explicit Note-Off (0x80 | ch, note, 0x00) for all bitset-tracked active notes
        │
        ├──► Stage 2 (≤ 1 Audio Quantum): Audio Hardware Buffer Zeroing & De-Click
        │      • Flip AtomicU8 `AudioEngineState::EmergencyMute` (SeqCst)
        │      • Apply 64-sample raised-cosine fade-out ramp to current output block
        │      • Zero all subsequent hardware output slices (`0.0f32`) & clear feedback delay lines
        │
        ├──► Stage 3 (≤ 10 ms): Atomic Unsaved Buffer Autosave to Disk
        │      • Read lock-free `ActiveEditorSnapshot` seq-lock buffer
        │      • Write temp file + `fsync` + atomic rename to:
        │        `$XDG_STATE_HOME/silverwave/crash-recovery/session-<UTC_ISO8601>-<PID>.swave`
        │      • Write companion structured telemetry manifest `.crash.json`
        │
        └──► Stage 4 (≤ 25 ms): Worker Respawn & Last-Known-Good AST Fallback
               • Supervisor catches `catch_unwind` / watchdog signal without exiting OS process
               • Respawn fresh evaluation/DSP worker thread with clean stack
               • Restore `LastKnownGoodGraph` at next cycle boundary (or remain in `MuteStandby`)
               • Broadcast non-blocking recovery diagnostic to REPL HUD and connected LSP clients
```

### 2.2 Rust Panic Recovery & Supervisor Contract Signatures

```rust
use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicU8};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEngineSafetyState {
    Running = 0,
    DeClickFadeOut = 1,
    EmergencyMute = 2,
    RecoveringFadeIn = 3,
}

#[repr(C, align(64))]
pub struct ActiveNoteBitset {
    pub channels: [[AtomicU64; 2]; 16],
}

#[repr(C, align(64))]
pub struct LiveRecoveryControlBlock {
    pub engine_state: AtomicU8,
    pub worker_heartbeat_ns: AtomicU64,
    pub active_cycle_bits: AtomicU64,
    pub panic_epoch: AtomicU64,
    pub active_notes: ActiveNoteBitset,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrashRecoveryManifest {
    pub schema_version: u32,
    pub timestamp_utc: String,
    pub process_id: u32,
    pub version: String,
    pub target_triple: String,
    pub panic_message: String,
    pub panic_location: Option<String>,
    pub cycle_at_panic: f64,
    pub cps_at_panic: f64,
    pub unsaved_buffer_path: String,
    pub last_known_good_buffer_path: Option<String>,
    pub midi_ports_flushed: Vec<String>,
    pub recovery_action: RecoveryMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecoveryMode {
    ResumeLastGood,
    MuteStandby,
}

pub trait MidiEmergencyFlushPort: Send + Sync {
    /// Must be lock-free, allocation-free, and complete within <= 2 ms across all 16 channels.
    fn emergency_flush_all_channels(&self, tracked_notes: &ActiveNoteBitset) -> Result<(), MidiFlushError>;
}

pub trait AudioSafetyInterlock: Send + Sync {
    fn trigger_emergency_mute(&self);
    fn process_emergency_block(&self, output_channels: &mut [&mut [f32]]);
    fn clear_feedback_buffers(&mut self);
}

pub trait AtomicSessionAutosaver: Send + Sync {
    fn snapshot_unsaved_buffer(&self, content: &[u8], cursor_offset: usize);
    fn flush_crash_snapshot_to_disk(
        &self,
        panic_info: &PanicHookInfo<'_>,
        recovery_dir: &Path,
    ) -> Result<CrashRecoveryManifest, std::io::Error>;
}

pub trait WorkerSupervisorContract: Send + Sync {
    fn install_live_performance_panic_hook(&self) -> Result<(), SupervisorError>;
    fn poll_watchdog(&self, now_monotonic_ns: u64) -> SupervisorAction;
    fn respawn_worker(&mut self, mode: RecoveryMode) -> Result<WorkerHandle, SupervisorError>;
}
```

## 3. Multi-Platform Packaging & Distribution Specifications

### 3.1 Linux, macOS, Windows, VST3/CLAP & `wasm32` PWA Contracts

- **Linux Packaging**: `.deb` (Debian 12+/Ubuntu 22.04+), `.rpm` (Fedora 40+/RHEL 9+), `.flatpak` (`org.silverwave.SilverWave`), `AppImage`, and [Arch Linux `PKGBUILD`](https://wiki.archlinux.org/title/PKGBUILD) (`groups=('pro-audio' 'vst3-plugins' 'clap-plugins')`) with `/etc/security/limits.d/99-silverwave-realtime.conf`, `rtkit` D-Bus fallback, and `/usr/lib/udev/rules.d/70-silverwave-midi.rules`.
- **macOS Universal2 Packaging**: Fat binaries (`x86_64-apple-darwin` + `aarch64-apple-darwin` via `lipo -create`), `SilverWave.app`, `.dmg`, `.pkg`, Developer ID codesigning with Hardened Runtime (`com.apple.security.cs.allow-jit`, `com.apple.security.device.audio-input`), `xcrun notarytool` stapling, and Homebrew Formula/Cask (`https://github.com/jxoesneon/silverwave`).
- **Windows Packaging**: [WiX v4 `.msi`](https://www.firegiant.com/wixtoolset/) and NSIS `.exe` signed via Azure Trusted Signing (`SHA-256` RFC 3161 timestamped), WASAPI Shared/Exclusive + ASIO + MMCSS `"Pro Audio"` thread elevation, and WinGet/Scoop manifests (`https://github.com/jxoesneon/silverwave`).
- **VST3 & CLAP Bundles**: Unified `SilverWave.vst3` ([Steinberg VST3 Format](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html)) and `SilverWave.clap` ([CLAP API](https://github.com/free-audio/clap)).
- **`wasm32` PWA Hosting**: Requires `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` ([MDN COEP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy)) to unlock `SharedArrayBuffer` for the lock-free `AudioWorkletProcessor`.

## 4. Supply-Chain, Security, Legal & Release Automation

- **Topological `crates.io` Publish Order**: Tier 0 (`silverwave-core`) → Tier 1 (`silverwave-mini`, `silverwave-dsp`) → Tier 2 (`silverwave-io`, `silverwave-viz`) → Tier 3 (`silverwave-gui`, `silverwave-wasm`, `silverwave-lsp`, `silverwave-mcp`, `silverwave-plugin`) → Tier 4 (`silverwave-cli`).
- **SLSA Level 3 & CycloneDX 1.5 SBOM**: Verified via `slsa-verifier` (`--source-uri github.com/jxoesneon/silverwave`) and [`cargo-cyclonedx`](https://crates.io/crates/cargo-cyclonedx/0.3.6).
- **Dual Licensing**: `MIT OR Apache-2.0` enforced via `cargo-deny`, with CC0-1.0 [AKWF](https://github.com/KristofferKarlAxelEkstrand/AKWF-FREE) attribution in `THIRD_PARTY_AUDIO_LICENSES.md`.

## 5. TDD CLI, Installer Smoke-Test, Plugin Validation & Release Verification Contracts

| Verification Gate ID | Requirement | Blocking Threshold for `v1.0.0` GA | 
| ----- | ----- | ----- | 
| **`GATE-01`** | Workspace Line & Branch Test Coverage (`cargo llvm-cov`) | $\ge 99.0\%$ line ($\ge 95.0\%$ branch) workspace-wide; $100.0\%$ across `silverwave-io::emergency_flush` and `silverwave-io::recovery`. | 
| **`GATE-02`** | Static Analysis & Compiler Warnings | `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits `0`. | 
| **`GATE-03`** | Real-Time Safety Audit (`rtsan` / no-alloc guard) | Zero allocations, mutex locks, or syscalls detected in `AudioSafetyInterlock`, `EmergencyMidiFlushRing`, and DSP `process()` callbacks. | 
| **`GATE-04`** | VST3 & CLAP Validation | [`pluginval`](https://github.com/Tracktion/pluginval) (`--strictness-level 10`) and [`clap-validator`](https://github.com/free-audio/clap-validator) pass 100% of test cases on Linux, macOS, and Windows. | 
| **`GATE-05`** | Supply-Chain Provenance & SBOM | [SLSA v1.0 Level 3](https://slsa.dev/) provenance verified via `slsa-verifier` and [CycloneDX 1.5](https://github.com/cyclonedx/cyclonedx-rust-cargo) JSON schema validated for all release targets. | 
| **`GATE-06`** | Legal & Audio Asset Audit | `cargo deny check` and `silverwave samples verify --strict-licenses` exit `0`. | 

## 6. Category-King CLI & Packaging Extensions (`silverwave mcp`, `render --video`, `swtake` & `bundle-plugin`)

1. **`silverwave mcp` Subcommand**: Launches the `silverwave-mcp` server over `--transport stdio` or `--transport http --bind 127.0.0.1:8765`, with `--attach-session`, `--sandbox-root`, `--rate-limit-rps`, and `--read-only` flags (specified in `07-SPEC-MCP-SERVER-AND-AGENT-SURFACE.md`).
2. **Deterministic 60 FPS Video & Alpha Overlay Exporter (`silverwave render --video`)**:
   - Flags: `--video`, `--preset <vertical-9x16-mp4|horizontal-16x9-mp4|alpha-overlay-webm>`, `--fps 60`, `--replay-take <file.swtake>`, `--qr-watermark`.
   - Steps `silverwave-dsp` (`800` stereo frames per video frame at `48 kHz`) and `silverwave-viz` (`wgpu` headless render target) in lockstep with zero frame drops, encoding via FFmpeg / WebCodecs.
3. **Stateless LZ4 Share Links (`silverwave share`)**: Encodes and decodes `#sw=<base64url_lz4_payload>` permalinks and generates scannable SVG/PNG QR watermarks with zero external database dependency.
4. **CLAP & VST3 Plugin Bundling (`cargo xtask bundle-plugin`)**: CI/CD release matrix compiles, code-signs, notarizes, and packages `SilverWave.clap` and `SilverWave.vst3` across Linux (`x86_64`/`aarch64`), macOS Universal2, and Windows (`x86_64`/`ARM64`).
