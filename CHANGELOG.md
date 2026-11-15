# Changelog

All notable changes to **SilverWave** (`silverwave`) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-11-15

### Added

* **`silverwave-mini` (`v0.2.0` Mini-Notation, Mondo & Dual-Layer Rust DSL)**:

  * **UTF-8 `SourceSpan`, `SourceOrigin` (`withLoc`), `DelimiterStack` & Damerau-Levenshtein Diagnostics (`src/span.rs`, `src/diagnostics.rs`)**: Implemented 0-indexed UTF-8 byte + 1-indexed line/column spans, escape-aware `with_loc` offset mapping, `DelimiterStack` opening-delimiter blame (`PP-06`), Damerau-Levenshtein typo suggestions (`".rom"` -> `".room"`, `".lfp"` -> `".lpf"`), and compile-time DoS quotas (`MAX_AST_NESTING_DEPTH = 64`, `MAX_EUCLID_STEPS = 1024`, `MAX_REPLICATE_OR_ELONGATE = 1024`, `MAX_LSYSTEM_EXPANDED_TOKENS = 4096`).

  * **Two-Stage `TPat<T>` AST & Zero-Panic Mini-Notation + Mondo + DSP Pipe Parser (`src/ast.rs`, `src/parser.rs`)**: Implemented all 20 Strudel Krill + Tidal Mini-Notation operator families (`~`, `-`, `[]`, `<>`, `{}`, `,`, `.`, `|`, `*`, `/`, `@w`, `_`, `!n`, `?p`, `:i`, `(k,n,o)`, `a..b`, `^var`, `^^var`, scientific pitch notes, chords, `w/h/q/e/s/t/f/x` ratio shorthands), Elektron conditional trigs (`@1:4`, `@1st`, `@!1st`, `@fill`, `@!fill`, `@pre`, `@!pre`, `@nei`, `@!nei` from `CK-01`), Mondo S-expressions `(func arg1 arg2 ...)`, and inline `>>` modular DSP pipes (`CK-05`).

  * **Stage 2 Compiler & Zero-Reparse 60 FPS Highlighting (`src/compiler.rs`)**: Implemented `compile_tpat`, `mini`, `mini_with_loc`, and `CompiledHighlightPattern<T>` guaranteeing 1 parse on buffer edit and $O(k)$ zero-allocation span lookup across 60 FPS frame queries.

  * **Dual-Layer Rust DSL, All 7 Alignment Suffixes & Fault-Tolerant Session Evaluator (`src/dsl.rs`, `src/transpiler.rs`)**: Implemented `PatternCombinator` (`.in`, `.out`, `.mix`, `.squeeze`, `.squeezeout`, `.reset`, `.restart`), `ur`, `seq_p_loop`, `s_mini`, `note_mini`, `n_mini`, and `MiniStreamSession` (`d1..d16`, `$:`, instant-muted `_$:`, `hush`, `solo`, `mute`, `all`, and `PP-07` fault-isolated stream evaluation).

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

## [0.0.0] - 2026-11-15

### Added

* **`00-MASTER-ROADMAP-v0.0.0-to-v1.0.0.md` through `09-SPEC-MARKET-RESEARCH-AND-COMPETITIVE-MOAT.md`**: Complete DocDD + TDD specification baseline.
