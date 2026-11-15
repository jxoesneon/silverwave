# `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md` — SilverWave Global Market Research, Competitive Intelligence & Category-King Moat Specification

| Field | Value |
| :--- | :--- |
| **Document ID** | `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT` |
| **Version** | `1.0.0` |
| **Status** | Normative Market & Product Strategy Specification (DocDD / TDD Baseline) |
| **Target Crates** | `silverwave-core`, `silverwave-mini`, `silverwave-dsp`, `silverwave-io`, `silverwave-viz`, `silverwave-gui`, `silverwave-wasm`, `silverwave-mcp`, `silverwave-plugin`, `silverwave-cli` |
| **Benchmark Date** | November 2026 |

---

## 1. Global Live-Coding & Algorithmic Music Competitive Matrix (Nov 2026)

To take the top position across live coding, algorithmic composition, and AI-assisted music production, **SilverWave** (`silverwave`) is benchmarked against every active platform in the global ecosystem.

### 1.1 Quantitative Repository & Architecture Matrix

| Platform & Primary Repo | GitHub Stars / Forks / Open Issues (Nov 2026) | Core Language & Audio Engine Stack | Primary Target Audience | Structural Ceiling Preventing #1 Market Dominance |
| :--- | :--- | :--- | :--- | :--- |
| **[Sonic Pi](https://github.com/sonic-pi-net/sonic-pi)** & **[Tau5](https://github.com/samaaron/tau5)** | `sonic-pi`: **12,273 ★** / 1,005 forks / 41 issues<br>`tau5`: **219 ★** / 11 forks / 3 issues | Ruby + [Erlang/Elixir BEAM router](https://podcast.thinkingelixir.com/106) + C++/Qt6 GUI + [SuperCollider](https://github.com/supercollider/supercollider) `scsynth` (`tau5`: Phoenix LiveView + Lua Tau5Lang) | K–12/university STEAM education ([Sonic Pi Education PDF](https://sonic-pi.net/files/articles/Live-Coding-Education.pdf)) & entry-level live coders | **Imperative `play`/`sleep` verbosity** makes complex polyrhythmic mutation slow on stage; cross-thread state races ([`#2934`](https://github.com/sonic-pi-net/sonic-pi/issues/2934), [`#3208`](https://github.com/sonic-pi-net/sonic-pi/issues/3208)); 4-daemon boot failures on Windows 11/macOS/Pi ([`#3565`](https://github.com/sonic-pi-net/sonic-pi/issues/3565), [`#3561`](https://github.com/sonic-pi-net/sonic-pi/issues/3561), [`#3552`](https://github.com/sonic-pi-net/sonic-pi/issues/3552)); no MIDI export ([`#854`](https://github.com/sonic-pi-net/sonic-pi/issues/854)). |
| **[Orca](https://github.com/hundredrabbits/Orca)** & **[Pilot](https://github.com/hundredrabbits/Pilot)** / **[Orca-c](https://github.com/hundredrabbits/Orca-c)** | `Orca`: **5,143 ★** / 255 forks / 40 issues<br>`Pilot`: **561 ★**<br>`Orca-c`: **531 ★** | 2D cellular ASCII grid (`A`–`Z` spatial operators) in Electron/JS or C/ncurses emitting MIDI/OSC/UDP ([Feeling of Computing #45](https://feelingof.com/episodes/045/)) | Modular synth / Monome Norns / tracker minimalists | **Zero built-in audio engine** (browser version strips UDP/OSC entirely); base-36 single-character cells hit a complexity wall on harmony/voicings ([`#278`](https://github.com/hundredrabbits/Orca/issues/278), [`#281`](https://github.com/hundredrabbits/Orca/issues/281)); Electron clock jitter ([`#257`](https://github.com/hundredrabbits/Orca/issues/257)) and single-MIDI-port limit ([`#267`](https://github.com/hundredrabbits/Orca/issues/267)). |
| **[Glicol](https://github.com/chaosprint/glicol)** | **3,021 ★** / 106 forks / 51 issues ([`glicol_synth` 0.13.5](https://crates.io/crates/glicol_synth/0.13.5)) | **100% Rust** (`pest` parser + `petgraph` 128-sample block AudioWorklet WASM + `rhai` scripting) ([Web Audio Conf 2021](https://webaudioconf.com/posts/2021_8/)) | Rust audio devs & modular graph live coders (`sin 440 >> mul ~env`) | **Direct Rust competitor**, but crippled by rudimentary `seq` pattern syntax (nested `seq` panics [`#105`](https://github.com/chaosprint/glicol/issues/105)), non-bandlimited oscillator aliasing ([`#127`](https://github.com/chaosprint/glicol/issues/127)), broken `adsr` sustain ([`#175`](https://github.com/chaosprint/glicol/issues/175)), slow `rhai` sample scripting ([HN Discussion](https://news.ycombinator.com/item?id=30942074)), no MIDI ([`#100`](https://github.com/chaosprint/glicol/issues/100)), and broken Safari 18.2 support ([`#185`](https://github.com/chaosprint/glicol/issues/185)). |
| **[TidalCycles](https://github.com/tidalcycles/Tidal)** & **[SuperDirt](https://github.com/musikinformatik/SuperDirt)** | `Tidal` + `SuperDirt` ([Codeberg](https://codeberg.org/musikinformatik/SuperDirt)) | Haskell GHCi pattern DSL $\rightarrow$ UDP OSC $\rightarrow$ SuperCollider `scsynth` + `sc3-plugins` | Algorave performers & functional programming musicians | **20–30 min 4-stack installation wall** ([Tidal Install Docs](https://tidalcycles.org/docs/getting-started/windows_install/)); cryptic GHC errors ([Uzu #2767](https://uzu.lurk.org/t/using-markovpat-for-chords/2767)); onset-only parameter freezing ([Tidal #806](https://github.com/tidalcycles/Tidal/issues/806), [SuperDirt #190](https://github.com/musikinformatik/SuperDirt/issues/190)); $O(N^2)$ randomness lag ([Tidal #636](https://github.com/tidalcycles/Tidal/issues/636)); 2 GB `scsynth` memory cap ([SuperDirt #255](https://github.com/musikinformatik/SuperDirt/issues/255)); no native browser or DAW plugin. |
| **[Strudel](https://codeberg.org/uzu/strudel)** ([GitHub Mirror](https://github.com/tidalcycles/strudel)) | Flagship web port ([strudel.cc](https://strudel.cc/), [Zenodo](https://zenodo.org/records/7842142)) | JavaScript/PeggyJS (`krill`) + WebAudio (`superdough` AudioWorklets) | Web-first algorave artists, TikTok/YouTube creators, educators | **Onset-only parameter sampling** ([Strudel #561](https://github.com/tidalcycles/strudel/issues/561)); `composify` memory leaks (`100–200 kB/s`, [Strudel #487](https://github.com/tidalcycles/strudel/issues/487), [`#1266`](https://github.com/tidalcycles/strudel/issues/1266)); whole-buffer eval fragility ([`#34`](https://github.com/tidalcycles/strudel/issues/34)); main-thread `setInterval` & reverb IR stalls ([Stave #1665](https://github.com/MrityunjayBhardwaj/Stave/issues/1665)); MIDI clock drift ([`#119`](https://github.com/tidalcycles/strudel/issues/119)); no native desktop/CLAP/VST3/stem bounce ([`#1353`](https://github.com/tidalcycles/strudel/discussions/1353)). |
| **[Extempore](https://github.com/digego/extempore)** | **1,468 ★** / 128 forks / 18 issues (`v0.11.0` Nov 2026) | C/C++20 host + `s7` Scheme + `xtlang` compiled to native code via **LLVM 22.1.6 ORC JIT** + `wgpu-native` | Academic cyber-physical coders & bare-metal DSP researchers | **Steepest cognitive barrier in live coding** (dual Lisp + manual region/zone memory pointers); 30-min LLVM source build; unsigned macOS binaries blocked by Gatekeeper ([`#417`](https://github.com/digego/extempore/issues/417)); zero browser/WASM target ([`#413`](https://github.com/digego/extempore/issues/413)). |
| **[FoxDot](https://github.com/Qirky/FoxDot)** & **[Renardo](https://github.com/e-lie/renardo)** | `FoxDot`: **1,165 ★** / 146 forks / 52 issues<br>`renardo`: **97 ★** / 13 forks / 27 issues | Python 3 OOP players (`p1 >> pluck()`) + `TimeVar` $\rightarrow$ OSC $\rightarrow$ SuperCollider Quark | Python-oriented algorave performers | **Upstream FoxDot abandoned** ([`#255`](https://github.com/Qirky/FoxDot/issues/255)) with Python 3.11+ `getargspec` breakage ([`#267`](https://github.com/Qirky/FoxDot/issues/267)); Renardo fork has tiny 97-star reach, `uv`/`GLIBC_2.38` install errors ([`#49`](https://github.com/e-lie/renardo/issues/49), [`#3`](https://github.com/e-lie/renardo/issues/3)), sample server outages ([`#18`](https://github.com/e-lie/renardo/issues/18)), and ["(very unusable) Reaper integration"](https://renardo.org/install/reaper-expe/). |
| **[ChucK](https://github.com/ccrma/chuck)** & **[WebChucK](https://github.com/ccrma/webchuck)** | `chuck`: **1,056 ★** / 147 forks / 156 issues<br>`webchuck`: **62 ★** / 11 forks / 7 issues | Strongly-timed C++ VM (`1::samp => now`) & Emscripten WASM `AudioWorklet` ([NIME 2023](https://nime.org/proc/nime2023_28/)) | University DSP courses (Stanford CCRMA) & game audio ([Chunity](https://github.com/ccrma/chunity)) | **Verbose imperative syntax** (`while(true)` loops) unsuitable for rapid beat improvisation; 156 open C++ VM issues including memory hazards/segfaults ([`#283`](https://github.com/ccrma/chuck/issues/283), [`#162`](https://github.com/ccrma/chuck/issues/162), [`#93`](https://github.com/ccrma/chuck/issues/93)); WebChucK lacks WebMIDI ([`#38`](https://github.com/ccrma/webchuck/issues/38)) and WebChuGL ([`#39`](https://github.com/ccrma/webchuck/issues/39)). |
| **[Mercury](https://github.com/tmhglnd/mercury)** & **[Playground](https://github.com/tmhglnd/mercury-playground)** | `mercury`: **383 ★** / 15 forks / 36 issues<br>`playground`: **80 ★** | Cycling '74 Max 8 + Node4Max (`total-serialism`) + Jitter OpenGL (Web port on WebAudio) | Algorave performers & serialist workshop students | **Proprietary Max 8 host dependency** on desktop; hard **30-line whole-buffer ceiling** with no per-line/block evaluation ([`#82`](https://github.com/tmhglnd/mercury/issues/82)) or multi-tempo support ([`#94`](https://github.com/tmhglnd/mercury/issues/94)); inconsistent time units ([`#71`](https://github.com/tmhglnd/mercury/issues/71)). |
| **[Sardine](https://github.com/Bubobubobubobubo/sardine)** | **328 ★** / 43 forks / 19 issues ([Zenodo](https://zenodo.org/records/7843817)) | Python 3.10+ `asyncio` `@swim` temporal recursion + Amphibian/Ziffers/Vortex $\rightarrow$ SuperDirt | Hackable Python live coders & musicologists | Mutating `@swim` arguments live crashes swimmer loops ([`#260`](https://github.com/Bubobubobubobubo/sardine/issues/260)); Vortex Tidal port examples throw runtime errors ([`#269`](https://github.com/Bubobubobubobubo/sardine/issues/269)); requires full Python + SuperCollider + SuperDirt setup ([`#188`](https://github.com/Bubobubobubobubo/sardine/issues/188)). |

---

## 2. The 2025–2026 AI Music & MCP Server Market Audit

### 2.1 Why 82% of Producers Reject Black-Box Audio AI in Favor of Symbolic Co-Performance
1. **Verified Producer Survey Data**: According to the global Tracklib producer survey cited in the [2026 Zekai Music Producer Report](https://zekaiwork.com/ai-for-music-producers-guide/) (1,700+ music producers) and the [Sonarworks 2026 Producer Survey](https://www.sonarworks.com/blog/research/future-music-production-human-producer-survey-2026):
   - **82% of music producers oppose full song generation from text prompts**, and only **6%** use full-track prompt generators regularly.
   - Among the **32%** of producers who actively use AI, **74%** use it strictly for **modular, controllable studio tasks** where human authorship is preserved.
2. **Copyright, Walled Gardens & Platform Backlash (2025–2026)**:
   - Major-label litigation forced Udio to disable downloads during its October 2025 licensing transition ([MakeBestMusic Jan 2026](https://makebestmusic.com/blog/best-ai-music-generation-tools-january-2026), [CreateYourMusic 2026 Copyright Audit](https://createyourmusic.ai/blog/ai-music-copyright)), while Suno's Q1 2026 "Licensed Mode" and September 2026 download limits sparked widespread community revolt ([TLDL 2026 Comparison](https://www.tldl.io/blog/ai-music-generation-suno-udio-lyria), [MusicRadar Sept 2026](https://www.musicradar.com/music-tech/guilt-free-ai-what-so-called-ethical-ai-tools-mean-for-musicians-and-producers), [r/SunoAI Analysis](https://www.reddit.com/r/SunoAI/comments/1vl1aa4/the_real_story_behind_sunos_new_download_limits/)).
   - Purely prompt-generated audio receives **zero U.S. copyright protection**, and Beatport banned purely AI-generated tracks ([Zinstrel Suno Hub](https://zinstrel.com/tools/suno), [MusicRadar Sept 2026](https://www.musicradar.com/music-tech/guilt-free-ai-what-so-called-ethical-ai-tools-mean-for-musicians-and-producers)).
   - Peer-reviewed studies ([Applied Sciences 2025/2026 Controllable Music Study](https://asciences.org/index.php/ojs/article/view/118/117), [Frontiers in Psychology 2026 Jazz GAI Study](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2026.1895145/full)) confirm that working musicians require **event-level symbolic editability**, deterministic synthesis, and phase-coherent stems rather than black-box waveform regeneration.

### 2.2 Competitive Audit of Existing Music MCP Servers (2025–2026)

| MCP Segment | Existing Open-Source Repositories | Transport & Architecture | Critical Engineering Deficiencies Solved by `silverwave-mcp` |
| :--- | :--- | :--- | :--- |
| **Strudel MCP Wrappers** | [`williamzujkowski/live-coding-music-mcp`](https://github.com/williamzujkowski/live-coding-music-mcp), [`phildougherty/strudel-mcp-bridge`](https://github.com/phildougherty/strudel-mcp-bridge), [`takeachangs/strudel-mcp`](https://github.com/takeachangs/strudel-mcp), [`TheSmallPixel/strudel-ai-dj`](https://github.com/TheSmallPixel/strudel-ai-dj) | Node.js spawning headless Chromium **Playwright** ([`live-coding-music-mcp`](https://github.com/williamzujkowski/strudel-mcp-server?ref=explainx)) or Chrome WebSocket extension ([`strudel-mcp-bridge`](https://github.com/phildougherty/strudel-mcp-bridge)) | 500+ MB Chromium overhead; AGPLv3 license contamination ([`live-coding-music-mcp` Releases](https://github.com/williamzujkowski/strudel-mcp-server/releases)); **no audio telemetry**, no rational `Hap` arc querying, no Tidal transitions, and full-buffer overwrites instead of surgical AST `slider()` patching. |
| **TidalCycles & Sonic Pi MCPs** | [`Benedict/tidal-cycles-mcp-server`](https://github.com/Benedict/tidal-cycles-mcp-server), [`abhishekjairath/sonic-pi-mcp`](https://github.com/abhishekjairath/sonic-pi-mcp) | Node.js spawning `ghci` + `scsynth` + `SuperDirt` ([LobeHub](https://lobehub.com/it/mcp/benedict-tidal-cycles-mcp-server)) or one-way UDP OSC | Requires installing Node 22 + Haskell + SuperCollider + SuperDirt; blind one-way string injection with zero syntax pre-validation, zero audio listening, and zero visual inspection. |
| **Ableton Live MCPs** | [`ahujasid/ableton-mcp`](https://github.com/ahujasid/ableton-mcp) (3.4k+ ★), [`dreamrec/LivePilot`](https://github.com/dreamrec/LivePilot) (474 tools), [`uisato/ableton-mcp-extended`](https://github.com/uisato/ableton-mcp-extended), [`nicholasbien/ableton-mcp-pro`](https://github.com/nicholasbien/ableton-mcp-pro) | Python FastMCP $\rightarrow$ TCP socket $\rightarrow$ Ableton Python MIDI Remote Script on UI thread | Blocks on Live's main UI thread; [`ableton-mcp`](https://github.com/ahujasid/ableton-mcp) is **100% acoustically deaf** ([Chatmu DAW MCP Audit](https://chatmu.io/blog/daw-mcp-servers-hear-your-mix)), while [`LivePilot`](https://github.com/dreamrec/LivePilot) requires a manual Max for Live UDP/OSC bridge. |
| **REAPER & Bitwig MCPs** | [`xDarkzx/Reaper-MCP`](https://github.com/xDarkzx/Reaper-MCP), [`TwelveTake-Studios/reaper-mcp`](https://github.com/TwelveTake-Studios/reaper-mcp), [`bonfire-systems/reaper-mcp`](https://github.com/bonfire-systems/reaper-mcp), [`shiehn/total-reaper-mcp`](https://github.com/shiehn/total-reaper-mcp), [`WeModulate/bitwig-mcp-server`](https://github.com/WeModulate/bitwig-mcp-server) | File-based JSON polling on REAPER's Lua `defer` timer ([`reaper-mcp-mac`](https://glama.ai/mcp/servers/Spartan-Game-Studios/reaper-mcp-mac)) or DrivenByMoss UDP OSC | High file-polling latency; requires bouncing temporary files to disk to analyze audio ([`itsuzef/reaper-mcp`](https://pypi.org/project/iflow-mcp_itsuzef-reaper_mcp/), [Chatmu Audit](https://chatmu.io/blog/daw-mcp-servers-hear-your-mix)). |
| **SuperCollider & MIDI MCPs** | [`Synohara/supercollider-mcp`](https://github.com/Synohara/supercollider-mcp), [`agrathwohl/supercollider-mcp`](https://github.com/agrathwohl/supercollider-mcp), [`Tok/SuperColliderMCP`](https://github.com/Tok/SuperColliderMCP), [`tubone24/midi-mcp-server`](https://github.com/tubone24/midi-mcp-server), [`sandst1/mcp-server-midi`](https://github.com/sandst1/mcp-server-midi), [`feamster/digitakt-midi-mcp`](https://github.com/feamster/digitakt-midi-mcp) | UDP OSC to `scsynth` or Python `asyncio` virtual MIDI port ([Skywork MIDI MCP Analysis](https://skywork.ai/skypage/en/ai-music-midi-output/1981902365165416448)) | No rational-time pattern combinators, no mini-notation, Python `asyncio` MIDI timing jitter, and no closed-loop audio/visual state. |

---

## 3. Category-King Moat: 10 Undisputed Differentiators Added to SilverWave

To guarantee **SilverWave** stands unchallenged across live coders, hardware/tracker producers, DAW engineers, educators, content creators, and AI agents, the following **10 Category-King Capabilities (`CK-01` through `CK-10`)** are normatively integrated into the SilverWave architecture.

### 3.1 `CK-01`: Elektron-Grade Conditional Trigs (`@1:4`, `@fill`, `@pre`, `@nei`, `@1st`)
- **Market Origin**: [Elektron Digitakt II](https://www.elektron.se/wp-content/uploads/2026/10/Digitakt-2-User-Manual_ENG_OS1.17_260930.pdf) and [Syntakt](https://www.elektron.se/wp-content/uploads/2026/03/Syntakt-User-Manual_ENG_OS1.40_260304.pdf) conditional trigs ([Elektronauts NEI/PRE Analysis](https://www.elektronauts.com/t/nei-and-pre-trig-conditions/51959)).
- **Architectural Design**: Extends `silverwave-mini` and `silverwave-core`'s `StatefulPattern` checkpoint ledger with causal inter-step and inter-stream predicates:
  - `@A:B` (e.g., `sd@1:4`, `cp@3:4`): Fires only when `(cycle_index % B) + 1 == A`.
  - `@1st` / `@!1st`: Fires only on (or after) the very first cycle after stream activation.
  - `@fill` / `@!fill`: Fires only while the performance `FILL` latch (`set_fill(true)` / MIDI sustain pedal / GUI Fill button) is active (or inactive).
  - `@pre` / `@!pre`: Fires only if the most recent conditional trig on the **same stream** evaluated `true` (or `false`), ignoring unconditional steps.
  - `@nei` / `@!nei`: Fires only if the most recent conditional trig on the **neighboring stream** ($d_{k-1}$) evaluated `true` (or `false`), enabling zero-boilerplate causal drum/bass dialogue across streams.

### 3.2 `CK-02`: Step-Indexed Parameter Locks (`plock`) & Click-Hold Step Inspector
- **Market Origin**: [Elektron Parameter Locks](https://www.elektron.se/wp-content/uploads/2026/10/Digitakt-User-Manual_ENG_OS1.54_260930.pdf).
- **Architectural Design**:
  - In DSL/Mini-Notation: `.plock(step_index, total_steps, control_patch)` (e.g., `s("bd*16").plock(11, 16, lpf(350).pan(0.8).crush(5))`) overrides any subset of parameters on a specific step index without splitting or rewriting the carrier string.
  - In `silverwave-gui`: Holding a step cell on the 16/64-step trig bar and dragging any parameter slider/knob surgically inserts or updates the corresponding `.plock(...)` AST call via `SourceSpan` patching.

### 3.3 `CK-03`: Octatrack Crossfader Scenes (`scene_a` / `scene_b` / `xfader`) & Live Looper Transition Freeze (`.ot_freeze()`)
- **Market Origin**: [Elektron Octatrack MKII](https://www.elektron.se/explore/octatrack-mkii) Scene A/B Crossfader ([Octatrack Coursebook](https://www.scribd.com/document/916249919/octratrack-coursebook), [Elektronauts Transition Thread](https://www.elektronauts.com/t/the-crossfader-transition-thread-tips-tricks-etc/4073)).
- **Architectural Design**:
  - Performers define up to 16 named parameter-lock scenes (`scene(0, ...)`, `scene(1, ...)`), bind any two scenes to **Slot A** and **Slot B**, and morph between them continuously at 64-sample `k-rate` via `xfader(0.0..1.0)` (or a physical MIDI crossfader / gamepad trigger).
  - `.ot_freeze(cycles)`: One-call master/orbit circular transition buffer that captures the last `N` cycles of live audio, crossfades playback to the frozen loop (with optional filter/stutter FX), lets the performer rewrite all code buffers silently underneath, and crossfades back on release.

### 3.4 `CK-04`: Dirtywave M8 Multi-Rate Sub-Tables (`.table()`), Hex Groove Swing (`.groove()`) & Bidirectional Tracker View
- **Market Origin**: [Dirtywave M8 Model:02](https://dirtywave.com/products/m8-tracker-model-02) ([M8 Manual](https://images.equipboard.com/uploads/item/manual/136211/dirtywave-m8-tracker-model-02-manual.pdf), [DirtyWave M8 Tips](https://github.com/pauley-unsaturated/DirtyWave-M8-Tips)), [Renoise](https://tutorials.renoise.com/wiki/Phrase_Editor), and [Polyend Tracker+](https://backstage.polyend.com/t/announcing-tracker/11031).
- **Architectural Design**:
  - `.table(tick_rate, param_pattern)`: Attaches an independent high-speed modulation/arpeggio sub-pattern that restarts at `t = 0` on every note onset and steps at `tick_rate` Hz or sub-cycle divisions (turning a single bass note into an M8-style wavetable/pitch/filter transient sequence).
  - `.groove(&[u8])`: Applies integer tick-ratio swing tables (e.g., `.groove([7, 5, 7, 5])`) across cycle subdivisions.
  - **Bidirectional Tracker / Phrase Grid View (`silverwave-gui`)**: Toggleable vertical `NOTE | INST | VOL | FX1 | FX2 | FX3` spreadsheet view synchronized 1:1 with the code AST via `SourceSpan`.

### 3.5 `CK-05`: Glicol / FunDSP Inline Modular Pipe Macro (`synth_pipe!` / `.dsp_pipe()`)
- **Market Origin**: [Glicol (`chaosprint/glicol`)](https://github.com/chaosprint/glicol), [FunDSP (`SamiPerttu/fundsp`)](https://github.com/SamiPerttu/fundsp), [Firewheel (`BillyDM/firewheel`)](https://github.com/BillyDM/firewheel), and [Knyst (`ErikNatanael/knyst`)](https://github.com/ErikNatanael/knyst).
- **Architectural Design**: Unifies Tidal/Strudel pattern algebra with Glicol/FunDSP signal-graph live coding. Users can define custom zero-allocation voice or insert DSP graphs inline using `>>` pipe combinators (`s("pipe: saw(freq) >> lpf(env(2000, 120, 0.15), 0.8) >> tanh(2.5)")`) with PolyBLEP bandlimited oscillators (fixing [Glicol #127](https://github.com/chaosprint/glicol/issues/127)) and `5 ms` equal-power hot-swap crossfading when the graph topology changes.

### 3.6 `CK-06`: Closed-Loop Acoustic Intelligence for MCP Agents (`silverwave-mcp`)
- **Market Origin**: Solves the "acoustically deaf DAW MCP" bottleneck identified in [Chatmu's October 2026 DAW MCP Audit](https://chatmu.io/blog/daw-mcp-servers-hear-your-mix).
- **Architectural Design**: Extends `read_audio_telemetry` and `silverwave://dsp/orbits` with real-time **Mix & Spectral Diagnostics** computed in-process without disk bounces:
  - Per-orbit and master EBU R128 momentary/short-term/integrated `LUFS`, true peak (`dBTP`), crest factor, stereo phase correlation (`[-1.0, +1.0]`), 1/3-octave 31-band spectrum, and **Inter-Orbit Frequency Masking / Sub-Bass Phase Cancellation Alerts** (e.g., detecting `d1` kick vs `d2` bass destructive interference at `40–100 Hz`).

### 3.7 `CK-07`: 1-Click Deterministic 60 FPS Video, Vertical `9:16` & Alpha-Overlay Renderer (`silverwave render --video`)
- **Market Origin**: Viral short-form algorave workflows of [Switch Angel](https://www.youtube.com/watch?v=HkgV_-nJOuE) (1.2M+ views), [DJ_Dave](https://www.interviewmagazine.com/music/live-coder-dj-dave-is-logged-in-to-the-algorave-afters), [Char Stiles](https://charstiles.com/cv/), and [Naoto Hieda](https://naoto-portfolio.glitches.me/), eliminating [BlackHole/OBS loopback](https://github.com/ExistentialAudio/BlackHole/wiki/How-to-Stream-Audio-from-a-DAW-into-OBS-using-BlackHole) and [Syphon/Spout frame lag](https://limeartgroup.com/the-mega-list-of-vj-software-and-tools/).
- **Architectural Design**:
  - Offline frame-stepped deterministic muxer (`dt = 1/60s` = `800` stereo frames at `48 kHz`) rendering synchronized `wgpu` code + active `SourceSpan` glow + inline `._pianoroll()`/`._scope()` + WGSL/Hydra shaders + master audio directly to:
    - **`9:16` Vertical (`1080×1920`, 60 FPS MP4)** with mobile-reflowed 32-column typography for TikTok, YouTube Shorts, and Reels.
    - **`16:9` Horizontal (`1920×1080` / `3840×2160`, 60 FPS MP4)**.
    - **Transparent Alpha Overlay (`WebM VP9 Alpha` / `ProRes 4444 .mov`)** for compositing glowing code/pianorolls over live camera footage.
  - **Live Take Keystroke & Eval Replay Log (`.swtake`)**: Captures timestamped buffer edits, cursor moves, and `slider()` drags during a live jam (`< 50 KB`) so a live performance can be re-rendered offline at 4K 60 FPS with zero frame drops, plus native real-time **Syphon (macOS) / Spout (Windows) / NDI** output with adjustable audio-visual delay compensation.

### 3.8 `CK-08`: Stateless LZ4 Permalinks (`#sw=`), Scannable Video QR Watermarks & Community Package Registry
- **Market Origin**: Solves [Strudel's backend-dependent Share URL expiration warning](https://strudel.cc/technical-manual/project-start/) and standardizes [Switch Angel-style reusable synth/pattern packs](https://hackaday.com/2025/10/16/live-coding-techno-with-strudel/).
- **Architectural Design**:
  - Compresses the full buffer, custom DSP graphs, shader code, and widget states using pure-Rust LZ4 + Base64URL into a stateless `#sw=<payload>` URI fragment that requires zero database and never expires.
  - Optional scannable QR watermark on exported `9:16` videos so viewers can scan any TikTok/Reel and open the exact playable patch in `< 2 seconds`.
  - Decentralized `use("sw:@author/pack@v1.0")` registry with SHA-256 content addressing.

### 3.9 `CK-09`: Cycle-Quantized CRDT Ensemble Sync & Dual-Cursor `[HUMAN]` + `[MCP AGENT]` B2B Mode
- **Market Origin**: Solves unquantized evaluation jitter and DAC phase drift in [Flok](https://github.com/munshkr/flok), [Estuary](https://github.com/dktr0/estuary), and [Troop](https://github.com/qirky/troop) ([Multi-User Tidal](https://tidalcycles.org/docs/configuration/multiuser-tidal/)).
- **Architectural Design**:
  - Pairs CRDT text synchronization with **Cycle-Quantized AST Commit Epochs** (`target_cycle: Rational64`) and per-peer DAC buffer latency compensation so every peer compiles and activates pattern changes on the exact same acoustic downbeat.
  - Renders color-coded actor cursors and evaluation badges (`[HUMAN: Performer]` vs `[MCP AGENT]`) in the editor and exported videos for "Human + AI B2B Live Coding" performances.

### 3.10 `CK-10`: "First Beat in 30 Seconds" Docked Interactive Tour, Offline Starter Pack & Handheld/Embedded Targets
- **Market Origin**: Combines [Sonic Pi's "10-Year-Old Rule" & docked tutorial](https://sonic-pi.net/tutorial.html) with [Steam Deck](https://github.com/Necrosiak/Steamcord), [Raspberry Pi 5 Patchbox OS](https://blokas.io/patchbox-os/), [Monome Norns](https://github.com/monome/norns), and [Electro-Smith Daisy](https://docs.rs/crate/daisy-embassy/latest) portability.
- **Architectural Design**:
  - Ships with a docked interactive 5-stage tutorial inside the REPL and a two-tier progressive starter pack (core WASM + procedural synth/drum micro-bank `< 1.8 MB` gzipped for instant load, backed by a content-addressed `15 MB` IndexedDB/disk starter pack) (303 Acid, 808/909, Amen breaks, FM/Wavetable pads) so beginners make studio-grade sound in `< 30 seconds` offline.
  - Supports native **Steam Deck / Linux Handheld Gamepad Mode** (`gilrs` raw HID + dual-thumbstick radial chord/scene wheels) and `no_std` core compilation for `STM32H750` ([Electro-Smith Daisy](https://github.com/electro-smith/libDaisy/releases)).

---

## 4. Normative Rust Trait & Type Contracts (`CK-01` – `CK-10`)

```rust
use crate::rational::Rational64;

/// [CK-01] Elektron-grade conditional trig predicates evaluated per step via `StatefulPattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrigCondition {
    /// Fires on cycle `a` of every `b` cycles (`@a:b`, e.g. `@1:4`).
    Ratio { a: u16, b: u16 },
    /// Fires only on (`true`) or after (`false`) the first cycle of stream activation (`@1st` / `@!1st`).
    FirstCycle { expected: bool },
    /// Fires only when performance FILL mode is active (`true`) or inactive (`false`) (`@fill` / `@!fill`).
    FillLatch { active: bool },
    /// Fires only if the most recent conditional trig on the SAME stream evaluated to `expected` (`@pre` / `@!pre`).
    PreviousConditionalOnSameStream { expected: bool },
    /// Fires only if the most recent conditional trig on the NEIGHBOR stream (`d_{k-1}`) evaluated to `expected` (`@nei` / `@!nei`).
    PreviousConditionalOnNeighborStream { expected: bool },
}

/// [CK-03] Octatrack-style Crossfader Scene Lock and Live Loop Transition Freeze contract.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderSceneBank {
    pub scene_a_id: u8,
    pub scene_b_id: u8,
    /// Continuous crossfader position `0.0` (Scene A) .. `1.0` (Scene B), slewed at 64-sample k-rate.
    pub xfader_position: f32,
    pub looper_freeze_active: bool,
    pub looper_freeze_cycles: Rational64,
}

/// [CK-06] Closed-Loop Acoustic Intelligence & Inter-Orbit Masking Telemetry for `silverwave-mcp`.
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

/// [CK-07 / CK-08] Deterministic 60 FPS Video Renderer & Stateless LZ4 QR Permalink contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoExportPreset {
    /// 1080x1920 9:16 60fps MP4 (TikTok / YouTube Shorts / Instagram Reels) with 32-col mobile code reflow.
    Vertical9x16ShortsMp4,
    /// 1920x1080 or 3840x2160 16:9 60fps MP4 (YouTube / Twitch / Desktop).
    Horizontal16x9UhdMp4 { width: u32, height: u32 },
    /// Transparent background RGBA overlay (WebM VP9 Alpha or ProRes 4444 .mov) for camera compositing.
    TransparentAlphaOverlayWebm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeterministicVideoRenderRequest {
    pub preset: VideoExportPreset,
    pub fps: u16,
    pub duration_cycles: Rational64,
    pub embed_qr_permalink_watermark: bool,
    pub replay_take_log: Option<std::path::PathBuf>,
}
```

---

## 5. TDD Verification Matrix — Category-King Moat Contracts

```rust
#[cfg(test)]
pub mod category_king_moat_tdd_contracts {
    use super::*;

    /// [CK-01] Verifies Elektron `@1:4`, `@fill`/`@!fill`, `@pre`/`@!pre` (ignoring unconditional steps),
    /// and `@nei`/`@!nei` (reading stream `d_{k-1}`) match Digitakt II / Syntakt truth tables across 16 cycles.
    pub fn test_elektron_trig_conditions_pre_nei_fill_and_ratio_truth_tables();

    /// [CK-02 / CK-03] Verifies step-indexed `.plock(11, 16, ...)` overrides only step 11 without splitting
    /// adjacent steps, and verifies `xfader(0.0..1.0)` interpolates Scene A and Scene B at 64-sample k-rate.
    pub fn test_step_plock_isolation_and_octatrack_scene_crossfader_interpolation();

    /// [CK-04] Verifies Dirtywave M8 `.table(rate, ...)` sub-step arpeggio/modulation restarts on each note
    /// onset, verifies `.groove([7, 5])` swing ratios, and verifies bidirectional Tracker Grid <-> AST round-trip.
    pub fn test_m8_subtable_groove_swing_and_bidirectional_tracker_ast_sync();

    /// [CK-05] Verifies Glicol/FunDSP inline `.dsp_pipe("saw(freq) >> lpf(1200, 0.8) >> tanh(2.0)")`
    /// compiles to a zero-allocation voice graph with PolyBLEP anti-aliasing and 5ms click-free hot-swap.
    pub fn test_inline_modular_dsp_pipe_bandlimiting_and_click_free_hotswap();

    /// [CK-06] Verifies `AcousticMixDiagnostic` detects 180-degree out-of-phase 60 Hz sub-bass between
    /// Orbit 0 and Orbit 1 (`SubBassPhaseCancellation`) and reports accurate EBU R128 LUFS / dBTP to MCP.
    pub fn test_mcp_acoustic_mix_diagnostics_phase_cancellation_and_lufs();

    /// [CK-07 / CK-08] Verifies offline deterministic 60 FPS `Vertical9x16ShortsMp4` and
    /// `TransparentAlphaOverlayWebm` rendering steps audio (`800` frames/frame at 48 kHz) and GPU frames
    /// with 0 dropped frames, and verifies `#sw=` LZ4 + Base64URL QR permalink round-trip decoding.
    pub fn test_deterministic_60fps_video_alpha_export_and_lz4_qr_permalink();
}
```
