# `02-SPEC-MINI-NOTATION-AND-DSL.md`

**Crate:** `silverwave-mini` & `silverwave-core::dsl` (re-exported as `silverwave::dsl`)  
**Status:** Normative Specification  
**Parity Target:** 1:1 union of [TidalCycles `ParseBP`](https://tidalcycles.org/docs/reference/time/) and Strudel [`@strudel/mini`](https://strudel.cc/technical-manual/repl/) + [`@strudel/transpiler`](https://strudel.cc/technical-manual/repl/) + [Pattern Alignment & Chaining API](https://strudel.cc/technical-manual/alignment/)

---

## 1. Mini-Notation Grammar & Operator Specification

### 1.1 Lexical & Structural Rules

1. **Encoding:** All mini-notation inputs are valid UTF-8 `&str` slices. All source locations are 0-indexed UTF-8 byte offsets (`Range<usize>`).
2. **Whitespace:** ASCII whitespace (`' '`, `'\t'`, `'\n'`, `'\r'`) separates sibling sequential tokens unless bound tighter by an infix/postfix operator or grouping delimiter.
3. **Comments (Multi-line / Backtick Literals):** Inside backtick-delimited multi-line mini-notation strings, `// ...` (to EOL) and `/* ... */` (non-nesting) are skipped as trivia without altering byte-offset accounting.
4. **Rational Time:** All durations, speeds, weights, and cycle spans are exact `Rational` (`i64 / i64` reduced) values. Floating-point literals in mini-notation (e.g., `0.25`, `.5`) are parsed into exact decimal rationals (`1/4`, `1/2`).

### 1.2 Formal PEG / EBNF Grammar

Precedence is ordered from lowest (top) to highest (bottom):

```ebnf
Pattern        := Stack

Stack          := Sequence ( ',' Sequence )*
Sequence       := ( Element | DotGroup )+
DotGroup       := SequenceItem+ ( '.' SequenceItem+ )+   (* '.' binds tighter than ',' and looser than postfix ops *)

SequenceItem   := Choice
Choice         := PolyrhythmItem ( '|' PolyrhythmItem )*

PolyrhythmItem := Modulated
Modulated      := Primary ( PostfixOp )*

PostfixOp      := '*' Factor                   (* fast / speed multiply *)
                | '/' Factor                   (* slow / speed divide *)
                | '@' Weight                   (* elongate / relative duration *)
                | '_'                          (* elongate by +1 unit weight *)
                | '!' RepeatCount?             (* replicate discrete steps; bare '!' = 2 *)
                | '?' Prob?                    (* degrade / random drop; bare '?' = 1/2 *)
                | ':' Index                    (* sample / variant index *)
                | '(' EuclidArgs ')'           (* Euclidean Bjorklund rhythm *)

Factor         := Number | Rational | RatioShorthand | Group
Weight         := Number | Rational | RatioShorthand
RepeatCount    := Integer | Group
Prob           := Number | Rational | Group
Index          := Integer | Group
EuclidArgs     := EuclidArg ',' EuclidArg ( ',' EuclidArg )?
EuclidArg      := Integer | Group

Primary        := Group
                | Slowcat
                | Polymeter
                | Rest
                | StateVar
                | EnumRange
                | ChordLiteral
                | NoteLiteral
                | NumberOrRatio
                | SymbolAtom

Group          := '[' Stack ']'
Slowcat        := '<' Stack '>'
Polymeter      := '{' Stack '}' ( '%' PolymeterSteps )?
PolymeterSteps := Number | Rational | Group

Rest           := '~' | '-'
StateVar       := '^' Ident
EnumRange      := RangeBound '..' RangeBound ( '..' RangeBound )?
RangeBound     := SignedNumber | NoteLiteral

ChordLiteral   := NoteClass Accidental? ChordQuality ( '^' | '/' NoteClass Accidental? )?
NoteLiteral    := NoteClass Accidental? Octave?
NoteClass      := [a-gA-G]
Accidental     := 's' | 'f' | 'n' | '#' | 'b'
Octave         := '-'? [0-9]

RatioShorthand := 'w' | 'h' | 'q' | 'e' | 's' | 't' | 'f' | 'x'
NumberOrRatio  := Rational | Float | SignedInteger
Rational       := SignedInteger '/' UnsignedInteger
SymbolAtom     := [a-zA-Z0-9_]+
Ident          := [a-zA-Z_][a-zA-Z0-9_]*
```

### 1.3 Complete Operator & Token Parity Table

| Syntax | Name | Arity / Fixity | Precedence | Exact Semantics & Tidal/Strudel Equivalence |
| :--- | :--- | :--- | :--- | :--- |
| `[ a b ]` | Subdivision / Fast Sequence | Circumfix | Primary | Compresses `Stack` (`a b`) into a single parent slot (`fastcat`). |
| `< a b >` | Slow Alternation | Circumfix | Primary | Plays one element per cycle in rotation (`slowcat`). |
| `a , b` | Stack / Polyrhythm | Infix (Assoc-L) | Lowest (`1`) | Superimposes `a` and `b` simultaneously (`stack`). Valid at top-level and inside `[ ]`, `< >`, `{ }`. |
| `a . b` | Feet / Beat Grouping | Infix (Assoc-L) | `2` | Groups whitespace-separated runs on each side into implicit `[a] [b]` feet (e.g., `a b . c d e` == `[a b] [c d e]`). |
| `a \| b` | Random Choice | Infix (Assoc-L) | `3` | Selects uniformly at random per cycle/event between `a` and `b` (`choose`). |
| `x*n` | Speed Multiply | Postfix | `4` | Repeats `x` `n` times within its slot (`fast n`). `n` may be a sub-pattern (e.g., `bd*<2 4>`). |
| `x/n` | Speed Divide | Postfix | `4` | Stretches `x` across `n` parent slots/cycles (`slow n`). `n` may be a sub-pattern. |
| `x@w` | Elongate / Weight | Postfix | `4` | Assigns relative duration weight `w` to `x` inside `timeCat` (default weight = `1`). |
| `x _` | Hold-Extend Step | Postfix / Sibling | `4` | Adds `+1` unit weight to the immediately preceding sibling in a sequence (`a _ b` == `a@2 b`). |
| `x!n` | Replicate | Postfix | `4` | Emits `n` discrete copies of `x` as separate sequence steps (`a!3` == `a a a`; bare `a!` == `a!2`). |
| `x?p` | Degrade / Drop | Postfix | `4` | Drops `x` with probability `p` in `[0, 1]` (`degradeBy p`; bare `x?` defaults to `p = 0.5`). |
| `x:i` | Sample / Variant Index | Postfix | `5` | Attaches sample/variant index `i` (e.g., `bd:2`, `casio:0`). `i` may be a pattern (`bd:<0 1 2>`). |
| `x(k,n,o)` | Euclidean Rhythm | Postfix | `5` | Distributes `k` onsets of `x` across `n` steps via Bjorklund's algorithm, rotated by optional offset `o` (default `0`). `k`, `n`, `o` may be patterns. |
| `{a b, x y}%r` | Polymeter | Circumfix | Primary | Aligns step-by-step across sequences; `r` sets steps per cycle (default = step count of the first sequence). |
| `a..b` | Enum Range | Infix | Primary | Generates inclusive integer/numeric/note sequence from `a` to `b` (ascending or descending; optional step `a..s..b`). |
| `~` / `-` | Rest / Silence | Atom | Primary | Emits no hap (`silence`), occupying 1 unit of sequence weight. |
| `^var` | State Variable | Prefix Atom | Primary | Resolves dynamic pattern binding `var` from runtime `ControlMap` / `setF` state at query time. |
| `c4`, `eb3`, `f#5` | Note Name | Atom | Primary | Scientific pitch notation (`s`/`#` sharp, `f`/`b` flat, `n` natural; default octave `3` or `4` per context, configurable, default `3` in Tidal/Strudel `note`). |
| `C^7`, `Am7`, `G7b9` | Chord Symbol | Atom | Primary | Expands to chord voicing intervals/pitches compatible with `.voicing()` modes (`below`, `above`, `duck`, `root`). |
| `w h q e s t f x` | Ratio Shorthands | Atom / Factor | Primary | Exact cycle fractions: `w = 1`, `h = 1/2`, `q = 1/4`, `e = 1/8`, `s = 1/16`, `t = 1/3`, `f = 1/5`, `x = 1/6`. |


### 1.4 Mondo Notation Grammar & AST (`@strudel/mondo` Parity)

Mondo notation embeds functional combinator calls directly inside pattern strings using S-expression-inspired parentheses `(func arg1 arg2 ...)`, distinguished from Euclidean `(k,n,o)` tuples by whitespace-delimited function identifiers rather than comma-delimited integers:

```ebnf
MondoExpr      := '(' FuncIdent ( Whitespace+ MondoArg )* ')'
FuncIdent      := [a-zA-Z_][a-zA-Z0-9_]*
MondoArg       := MondoExpr | Sequence | Modulated | QuotedMini
```

- **AST Representation:** `TPat::MondoCall { func: String, args: Vec<TPat<T>>, span: ByteSpan }`.
- **Semantics:** `(fast 2 [bd sd])` lowers to `compile_tpat("[bd sd]").fast(2)` while preserving exact `ByteSpan` offsets on both the function token and inner atoms.

---

## 2. Two-Stage `TPat` AST & Zero-Reparse 60 fps Highlighting

### 2.1 Stage 1: Location-Annotated `TPat<T>` AST

To preserve exact source mapping for live editor feedback without re-parsing on every animation frame, `silverwave-mini` separates parsing from pattern compilation:

1. **Byte-Span Type:** `pub type ByteSpan = Range<usize>;` (UTF-8 byte offsets `[start, end)` relative to the mini-notation string).
2. **Two-Level Location Composition (`with_loc`):**
   - Outer DSL parser records `outer_span: ByteSpan` for the string literal in the editor buffer.
   - Inner mini-notation parser records `inner_span: ByteSpan` for every `TPat` node.
   - Composition produces `abs_span = (outer_start + inner_start)..(outer_start + inner_end)` (accounting for quote/backtick delimiter width and escape sequences via a byte-offset map).

```rust
pub struct Spanned<T> {
    pub node: T,
    pub span: ByteSpan,
}

pub enum TPat<T> {
    Atom { value: T, span: ByteSpan },
    Var { name: String, span: ByteSpan },
    Silence { span: ByteSpan },
    Seq { items: Vec<TPat<T>>, span: ByteSpan },
    Stack { layers: Vec<TPat<T>>, span: ByteSpan },
    Slowcat { items: Vec<TPat<T>>, span: ByteSpan },
    Polymeter { layers: Vec<TPat<T>>, steps: Option<Box<TPat<Rational>>>, span: ByteSpan },
    Fast { factor: Box<TPat<Rational>>, inner: Box<TPat<T>>, span: ByteSpan },
    Slow { factor: Box<TPat<Rational>>, inner: Box<TPat<T>>, span: ByteSpan },
    Elongate { weight: Rational, inner: Box<TPat<T>>, span: ByteSpan },
    Repeat { count: Box<TPat<usize>>, inner: Box<TPat<T>>, span: ByteSpan },
    DegradeBy { prob: Box<TPat<f64>>, seed_salt: u64, inner: Box<TPat<T>>, span: ByteSpan },
    EnumFromTo { start: Box<TPat<T>>, step: Option<Box<TPat<T>>>, end: Box<TPat<T>>, span: ByteSpan },
    Euclid { pulses: Box<TPat<isize>>, steps: Box<TPat<isize>>, offset: Option<Box<TPat<isize>>>, inner: Box<TPat<T>>, span: ByteSpan },
    Choose { options: Vec<TPat<T>>, span: ByteSpan },
    SampleIndex { target: Box<TPat<T>>, index: Box<TPat<usize>>, span: ByteSpan },
    Chord { root: NoteSpec, quality: String, inversion: Option<NoteSpec>, span: ByteSpan },
}
```

### 2.2 Stage 2: Lowering (`TPat<T>` -> `Pattern<T>`) & `Context` Attachment

- **Contract:** `fn compile_tpat<T: Clone + Send + Sync + 'static>(ast: &TPat<T>, origin: SourceOrigin) -> Pattern<T>`
- Every leaf `TPat::Atom`, `TPat::Var`, and `TPat::Chord` wraps its emitted `Hap<T>` with `HapContext { locations: SmallVec<[ByteSpan; 4]>, .. }`.
- Combinator nodes (`Fast`, `Slow`, `Euclid`, `SampleIndex`, etc.) propagate and merge `HapContext.locations` so compound expressions (e.g., `bd*4` or `sd(3,8)`) highlight both the atom and its active operator span.

### 2.3 Zero-Reparse 60 fps Editor Highlighting Architecture

1. **Parse Once on Edit:** `TPat<T>` is parsed **only** when the editor text buffer mutates (debounced or on `eval`), producing an immutable `Arc<CompiledPattern>` and a sorted `Vec<ByteSpan>` highlight table.
2. **60 Hz Frame Query Contract:**
   - At each vsync tick (`16.67 ms` budget), the UI/editor queries active haps for the current audio-clock window `[t_now, t_now + frame_dt)` using `Pattern::query_arc`.
   - Active `ByteSpan`s are read directly from `hap.context.locations` in `O(k)` where `k` is the number of active haps in the frame—**zero string scanning, zero PEG re-parsing, and zero AST allocations** on the render thread.
3. **Invariant:** `abs_span.start <= abs_span.end <= source.len()` and both endpoints are guaranteed valid UTF-8 `char` boundaries (`source.is_char_boundary`).

---

## 3. Dual-Layer Rust DSL & Live-Eval Scripting Ergonomics

### 3.1 Layer 1: Strongly-Typed Rust DSL (`silverwave-mini::dsl`)

- Core types: `Pattern<T>`, `ControlPattern` (`Pattern<ControlMap>`), `Hap<T> { whole: Option<ArcSpan>, part: ArcSpan, value: T, context: HapContext }`.
- Mini-notation entry points:
  - `mini::<T>(src: &str) -> Result<Pattern<T>, ParseError>`
  - `mini_with_loc::<T>(src: &str, origin: SourceOrigin) -> Result<Pattern<T>, ParseError>`
  - `s(pat: impl IntoPattern<String>) -> ControlPattern`
  - `note(pat: impl IntoPattern<NoteOrNumber>) -> ControlPattern`
  - `n(pat: impl IntoPattern<f64>) -> ControlPattern`

### 3.2 Layer 2: Live-Eval Scripting & Transpiler (`@strudel/transpiler` Parity)

The live-eval pre-processor transforms user scripts before evaluation:
1. **Automatic Mini-Notation Lifting:** Double-quoted (`"..."`) and backtick-quoted (`` `...` ``) literals passed to pattern-accepting functions or operators are rewritten to `m("<literal>", outer_byte_span)` with exact `SourceOrigin` byte offsets. Single-quoted (`'...'`) literals remain raw strings.
2. **Stream Prefix Desugaring:**
   - `$: <expr>` -> registers an anonymous active stream in the session's `stack`.
   - `_$: <expr>` -> parses and type-checks `<expr>` for editor diagnostics/highlighting but excludes it from audio output (instant mute).
   - `d1..d16` (or `d1: <expr>`) -> binds named/numbered slots `1..=16` in the `StreamRegistry` with hot-swap and transition support.

### 3.3 Fluent Chaining Methods & Alignment Suffixes

Every binary pattern combinator (`add`, `sub`, `mul`, `div`, `mod_`, `pow`, `set`, `keep`, `struct_`, `mask`, `lpf`, `gain`, etc.) exposes a default alignment (`.in()`) and all **7** explicit alignment suffixes matching Strudel's `Pattern Alignment` semantics:

| Alignment Suffix | Underlying Join / Bind | Structure Source (`whole`) | Time-Mapping Semantics |
| :--- | :--- | :--- | :--- |
| `.in()` *(default)* | `appLeft` / `innerBind` | Left pattern (`self`) | Left pattern dictates event onsets and `whole` spans; right values are sampled at left intersections. |
| `.out()` | `appRight` / `outerBind` | Right pattern (`rhs`) | Right pattern dictates event onsets and `whole` spans; left values are sampled at right intersections. |
| `.mix()` | `appBoth` / `bind` | Intersection (`self ∩ rhs`) | Events are created at exact intersections of `self` and `rhs` with `whole = self.whole ∩ rhs.whole` (no fragments). |
| `.squeeze()` | `squeezeBind` | Left event + compressed right | Each cycle of `rhs` is time-compressed ("squeezed") into the `whole` timespan of each event in `self`. |
| `.squeezeout()` | `squeezeOutBind` | Right event + compressed left | Each cycle of `self` is time-compressed into the `whole` timespan of each event in `rhs`. |
| `.reset()` | `resetJoin` | Right event triggers left reset | Each event onset in `rhs` resets `self` to the start of its *current* cycle, truncating `self` to fit `rhs`'s event span. |
| `.restart()` | `restartJoin` | Right event triggers left restart | Identical to `.reset()`, except `self` restarts from **cycle 0** (`[0, 1)`) on every `rhs` onset. |

**Method Signature Contract (Rust Layer 1 & Live-Eval Layer 2):**
```rust
pub trait PatternCombinator<T> {
    fn add(self, rhs: impl IntoPattern<T>) -> Pattern<T>; // defaults to .in()
    fn add_in(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_out(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_mix(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_squeeze(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_squeezeout(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_reset(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
    fn add_restart(self, rhs: impl IntoPattern<T>) -> Pattern<T>;
}
// Live-eval DSL supports both `pat.add.squeeze(rhs)` and `pat.add_squeeze(rhs)`.
```

### 3.4 Macro-Composition & Stream Management

| Construct | Signature / Syntax | Semantics |
| :--- | :--- | :--- |
| `ur` | `ur(len: Rational, pat: Pattern<String>, pats: Map<String, Pattern<T>>, fx: Map<String, fn(Pattern<T>) -> Pattern<T>>) -> Pattern<T>` | Song-level pattern-of-patterns; stretches `pat` over `len` cycles, looks up named patterns and `:`-separated effects. |
| `seqP` / `seqPLoop` | `seqP(entries: Vec<(Rational, Rational, Pattern<T>)>) -> Pattern<T>` | Schedules sub-patterns across explicit `[start, end)` cycle windows; `seqPLoop` wraps modulo max end cycle. |
| `stack` | `stack(pats: Vec<Pattern<T>>) -> Pattern<T>` | Superimposes all patterns in parallel over identical query spans. |
| `all` | `all(f: impl Fn(ControlPattern) -> ControlPattern)` | Applies a global transformation across all active `d1..d16` and `$:` streams. |
| `d1..d16` | `d1(pat)` / `d1: pat` | Binds pattern to slot `1..=16`; supports `hush()`, `solo(id)`, `unsolo(id)`, `mute(id)`, `unmute(id)`, and Tidal transitions (`xfade`, `clutch`, `anticipate`, `jump`). |

---

## 4. TDD Parser, AST & Byte-Span Fidelity Test Suite Specification

Every requirement below MUST be verified by automated tests in `silverwave-mini` and `silverwave-mini::dsl` before implementation is considered complete.

### 4.1 Lexer & UTF-8 Byte-Span Fidelity Tests

1. **`test_span_ascii_atoms`:** Verify `"bd sd hh"` produces 3 `TPat::Atom` nodes with exact `ByteSpan`s `0..2`, `3..5`, `6..8`.
2. **`test_span_multibyte_utf8`:** Verify strings containing multi-byte UTF-8 characters (e.g., comments `// café 🎵\nbd sd`) compute byte offsets (not char indices), and every `ByteSpan` satisfies `src.is_char_boundary(span.start) && src.is_char_boundary(span.end)`.
3. **`test_with_loc_composition`:** Verify `mini_with_loc` with `outer_offset = 14` (e.g., `d1: s("bd [sd cp]")`) maps `"sd"` to `21..23` in the outer buffer, accounting for quote delimiter offset.
4. **`test_escaped_quotes_and_multiline_backticks`:** Verify `\n` and `\"` inside quoted literals preserve exact outer-buffer `ByteSpan` alignment.

### 4.2 Mini-Notation Grammar & Operator Parity Tests

1. **`test_subdivision_and_slowcat`:** `"[a b] <c d>"` over cycles `0..1` and `1..2` yields `[a, b, c]` in cycle 0 and `[a, b, d]` in cycle 1 with exact `Rational` spans (`0..1/4`, `1/4..1/2`, `1/2..1`).
2. **`test_stack_comma_all_contexts`:** Verify `,` at top-level, inside `[ ]`, `< >`, and `{ }` produces `TPat::Stack` and simultaneous haps.
3. **`test_feet_dot_precedence`:** `"a b . c d e"` parses identically to `"[a b] [c d e]"` and binds looser than `*`/`/` (`"a*2 b . c"` == `"[a*2 b] [c]"`).
4. **`test_speed_mul_div_patterned`:** `"bd*<2 4>"` and `"bd/<2 3>"` evaluate dynamically per cycle with exact rational `whole` and `part` spans.
5. **`test_elongate_at_and_underscore`:** `"a@2 b"` and `"a _ b"` both yield `a` with `2/3` duration and `b` with `1/3` duration.
6. **`test_replicate_bang`:** `"a!3 b"` yields 4 equal `1/4` steps (`a, a, a, b`); bare `"a! b"` defaults to `"a!2 b"`.
7. **`test_degrade_determinism`:** `"bd*8?"` and `"bd*8?0.25"` produce deterministic drop sequences for a fixed cycle and seed, and never drop when `p = 0`.
8. **`test_choice_pipe`:** `"bd|sd|cp"` selects one option per cycle/event deterministically under seed.
9. **`test_sample_index_colon`:** `"bd:2"`, `"casio:0"`, and `"bd:<0 1>"` attach `n = 2`, `n = 0`, and patterned `n` respectively.
10. **`test_euclidean_bjorklund`:** `"bd(3,8)"`, `"bd(5,8,2)"`, and `"bd(<3 5>,8)"` match exact Bjorklund onset matrices (e.g., `(3,8)` -> `[1,0,0,1,0,0,1,0]`).
11. **`test_polymeter_steps`:** `"{a b c, x y}%4"` advances 4 steps per cycle across both sequences, wrapping independently at lengths 3 and 2.
12. **`test_enum_range`:** `"0..3"`, `"3..0"`, and `"0..6..2"` expand to `[0,1,2,3]`, `[3,2,1,0]`, and `[0,2,4,6]`.
13. **`test_rests_tilde_dash`:** `"bd ~ sd -"` emits haps only at `[0, 1/4)` and `[1/2, 3/4)`.
14. **`test_state_var_caret`:** `"^cutoff"` resolves dynamically from `ControlMap` state on each `query_arc` without re-parsing.
15. **`test_notes_chords_and_ratio_shorthands`:** Verify `c4`, `eb3`, `f#5`, `C^7`, `Am7`, and `w/h/q/e/s/t/f/x` (`1`, `1/2`, `1/4`, `1/8`, `1/16`, `1/3`, `1/5`, `1/6`) resolve to exact pitches, voicings, and `Rational` values.

### 4.3 Two-Stage `TPat` & 60 fps Highlighting Tests

1. **`test_zero_reparse_playback_queries`:** Instrument parser call counter; assert `parse_count == 1` across 600 consecutive `query_arc` calls (simulating 10 seconds at 60 fps).
2. **`test_hap_context_span_roundtrip`:** For `"bd [sd cp]*2"`, every queried `Hap` carries non-empty `context.locations` pointing to the exact slice of `"bd"`, `"sd"`, or `"cp"` in the source string.
3. **`test_60fps_frame_budget`:** Benchmark querying 16 stacked complex patterns for a `1/60s` window; assert p99 latency `< 1.0 ms` (well within the `16.67 ms` frame budget).

### 4.4 Alignment Suffixes, Macro-Composition & Transpiler Tests

1. **`test_alignment_in_out_mix`:** Verify `"0 1 2".add.in("10 20")`, `.add.out(...)`, and `.add.mix(...)` match Strudel's exact `whole`/`part` fragment and intersection semantics.
2. **`test_alignment_squeeze_and_squeezeout`:** Verify `"0 1 2".add.squeeze("10 20")` == `"[10 20] [11 21] [12 22]"` and `.squeezeout(...)` == `"[10 11 12] [20 21 22]"`.
3. **`test_alignment_reset_and_restart`:** Verify `"0 1 2 3 4 5 6 7".add.reset("10 [20 30]")` == `"10 11 12 13 20 21 30 31"`, and `.restart()` diverges from `.reset()` when the left pattern varies across cycles (`"<0 100> 1"`).
4. **`test_ur_seqp_stack_all`:** Verify `ur` lookup + fx application, `seqP` / `seqPLoop` cycle windowing, `stack`, and `all` global transform.
5. **`test_transpiler_streams_and_mute`:** Verify `$:` registers active parallel streams, `_$:` mutes output while preserving parse/highlight diagnostics, and `d1..d16` / `hush()` manage slot lifecycles.

### 4.5 Property-Based & Fuzzing Invariants

1. **No-Panic Fuzzing:** Arbitrary UTF-8 byte sequences passed to `mini()` and the live-eval transpiler MUST return `Ok(...)` or `Err(ParseError)`—never panic or overflow the stack (depth-bounded recursion).
2. **Span Bounds Invariant:** For every `TPat` node and every `Hap.context.locations` entry, `span.start <= span.end <= input.len()` and both indices lie on UTF-8 char boundaries.
3. **Time Conservation Invariant:** For any deterministic non-degraded `Sequence`, the sum of child `whole` durations over `[0, 1)` equals `1`.

---

## 5. 1:1+ Grammar Extensions, Delimiter-Stack Blame, DoS Quotas & Inline DSP Pipes (`PP-06`, `CK-01`, `CK-05`)

### 5.1 EBNF Grammar Extensions for Ties (`_`), Conditional Trigs (`@`) & Voicing Tags
```ebnf
tie_continuation = "_" ;
trig_condition   = "@" , ( ratio_cond | "1st" | "!1st" | "fill" | "!fill" | "pre" | "!pre" | "nei" | "!nei" | voicing_dict_tag ) ; /* e.g. @fill, @!fill, @pre, @!pre, @nei, @!nei, @1:4 */
ratio_cond       = integer , ":" , integer ;
voicing_dict_tag = identifier ;
dsp_pipe_expr    = dsp_stage , { ">>" , dsp_stage } ;
```

### 5.2 Opening-Delimiter Stack Blame, Damerau-Levenshtein Hints & Compile-Time DoS Quotas
- **`DelimiterStack`**: Pushes every opening `[`, `<`, `{`, `(`, `"`, and `` ` `` with its `SourceSpan`. Premature EOF or mismatched closing tokens blame the opening token span directly (resolving [Strudel PR #1315](https://github.com/tidalcycles/strudel/pull/1315)).
- **Typo Suggestions**: Unknown method/control identifiers within Damerau-Levenshtein distance $\le 2$ emit structured `did_you_mean` diagnostics (`".rom"` $\rightarrow$ `".room"`).
- **Universal `IntoPattern<T>` Lifting**: Every combinator argument (`whenmod`, `euclid`, `arp_with`, `markov`, `while_pat`) accepts `impl IntoPattern<T>`.
- **Deterministic Resource & DoS Quotas**:
  - `MAX_AST_NESTING_DEPTH = 64`
  - `MAX_EUCLID_STEPS = 1024`
  - `MAX_REPLICATE_OR_ELONGATE = 1024`
  - `MAX_LSYSTEM_EXPANDED_TOKENS = 4096`
- **Inline Modular Pipe Macro (`synth_pipe!` / `.dsp_pipe()`)**: Compiles [Glicol](https://github.com/chaosprint/glicol) / [FunDSP](https://github.com/SamiPerttu/fundsp)-style `>>` signal chains (`"saw(freq) >> lpf(cutoff, 0.8) >> tanh(2.0)"`) directly into stack-allocated `silverwave-dsp` voice graphs.
