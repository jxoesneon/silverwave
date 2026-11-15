//! Tonal scales, Unified Superset Chord & Voicing Dictionary (`PP-11` supporting `ireal`,
//! standard, and TidalCycles chord symbols plus inline `@dict` voicing tags), and
//! Microtonal Scala (`.scl` / `.kbm`) tuning engine (`ScalaTuning`).

use crate::algebra::{lift_a2_left, PatternFunctor, PatternMonad};
use crate::pattern::Pattern;
use crate::signals::ln_f64;
use crate::value::{exp2_f64, EngineCompatibilityProfile, NoteValue, Value};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Returns the semitone intervals for a named scale (Tonal.js + TidalCycles dictionary parity).
pub fn scale_intervals(scale_type: &str) -> &'static [i32] {
    match scale_type.trim().to_ascii_lowercase().as_str() {
        "major" | "ionian" => &[0, 2, 4, 5, 7, 9, 11],
        "minor" | "aeolian" | "natural:minor" => &[0, 2, 3, 5, 7, 8, 10],
        "dorian" => &[0, 2, 3, 5, 7, 9, 10],
        "phrygian" => &[0, 1, 3, 5, 7, 8, 10],
        "lydian" => &[0, 2, 4, 6, 7, 9, 11],
        "mixolydian" => &[0, 2, 4, 5, 7, 9, 10],
        "locrian" => &[0, 1, 3, 5, 6, 8, 10],
        "harmonic:minor" | "harmonicminor" => &[0, 2, 3, 5, 7, 8, 11],
        "melodic:minor" | "melodicminor" => &[0, 2, 3, 5, 7, 9, 11],
        "major:pentatonic" | "pentatonic" | "majpent" => &[0, 2, 4, 7, 9],
        "minor:pentatonic" | "minpent" => &[0, 3, 5, 7, 10],
        "blues" => &[0, 3, 5, 6, 7, 10],
        "whole:tone" | "wholetone" => &[0, 2, 4, 6, 8, 10],
        "diminished" | "octatonic" => &[0, 2, 3, 5, 6, 8, 9, 11],
        "augmented" => &[0, 3, 4, 7, 8, 11],
        "bebop" => &[0, 2, 4, 5, 7, 9, 10, 11],
        "hirajoshi" => &[0, 2, 3, 7, 8],
        "iwato" => &[0, 1, 5, 6, 10],
        "kumoi" => &[0, 2, 3, 7, 9],
        "pelog" => &[0, 1, 3, 7, 8],
        "prometheus" => &[0, 2, 4, 6, 9, 10],
        "enigmatic" => &[0, 1, 4, 6, 8, 10, 11],
        "phrygian:dominant" | "spanish" => &[0, 1, 4, 5, 7, 8, 10],
        "chromatic" => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        _ => &[0, 2, 4, 5, 7, 9, 11],
    }
}

/// Parses a scale descriptor like `"C:major"`, `"A2:minor:pentatonic"`, or `"D:dorian"` into
/// `(root_midi, scale_intervals)`.
pub fn parse_scale_descriptor(desc: &str) -> (i32, &'static [i32]) {
    let trimmed = desc.trim();
    if let Some((root_part, mode_part)) = trimmed.split_once(':') {
        let (pc, oct) = NoteValue::parse_pitch_name(root_part).unwrap_or((0, 4));
        let root_midi = 12 * (oct + 1) + pc;
        (root_midi, scale_intervals(mode_part))
    } else {
        (60, scale_intervals(trimmed))
    }
}

/// Resolves an integer scale `degree` (including negative degrees and multi-octave wrapping)
/// to a MIDI note number.
pub fn resolve_scale_degree(root_midi: i32, intervals: &[i32], degree: i64) -> f64 {
    let len = intervals.len() as i64;
    let oct_shift = degree.div_euclid(len) as i32;
    let idx = degree.rem_euclid(len) as usize;
    (root_midi + oct_shift * 12 + intervals[idx]) as f64
}

/// Parsed chord representation from the Unified Superset Chord & Voicing Parser (`PP-11`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedChord {
    pub root_semitone: i32,
    pub root_octave: i32,
    pub quality: String,
    pub intervals: Vec<i32>,
    pub inline_dict: Option<String>,
}

/// Unified Superset Chord & Voicing Parser (`PP-11`):
/// Simultaneously parses `ireal` (`C^7`, `Co7`, `C-7`, `C7b9`, `Ch7`),
/// standard music theory (`Cmaj7`, `Cdim`, `Cdim7`, `Cm7b5`, `Caug`, `Csus4`),
/// and TidalCycles symbols (`c'maj7`, `gm13`, `a7s5`, `dm9`), plus optional inline `@dict` suffix tags
/// (e.g. `"Cmaj7@lefthand"`, `"Dm9@ireal"`, `"G13@drop2"`, `"C6@triad"`).
pub fn parse_chord_symbol(symbol: &str) -> Option<ParsedChord> {
    let s = symbol.trim();
    if s.is_empty() {
        return None;
    }
    let (chord_body, inline_dict) = if let Some((b, d)) = s.split_once('@') {
        (b.trim(), Some(d.trim().to_ascii_lowercase()))
    } else {
        (s, None)
    };

    let mut chars = chord_body.chars().peekable();
    let root_base = match chars.next()?.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };

    let mut acc = 0i32;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | 's' => {
                // Only treat 's' as sharp if not the start of "sus"
                let rem: String = chars.clone().collect();
                if rem.to_ascii_lowercase().starts_with("sus") {
                    break;
                }
                acc += 1;
                chars.next();
            }
            'b' | 'f' => {
                acc -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    let raw_qual: String = chars.collect();
    // Strip Tidal single-quote separator (`c'maj7` -> `maj7`)
    let qual = raw_qual.trim_start_matches('\'').trim();

    let intervals = match qual {
        "" | "M" | "maj" | "^" => vec![0, 4, 7],
        "m" | "min" | "-" => vec![0, 3, 7],
        "dim" | "o" | "0" => vec![0, 3, 6],
        "dim7" | "o7" => vec![0, 3, 6, 9],
        "m7b5" | "h7" | "h" | "-7b5" => vec![0, 3, 6, 10],
        "aug" | "+" | "s5" | "#5" => vec![0, 4, 8],
        "sus2" => vec![0, 2, 7],
        "sus4" | "sus" => vec![0, 5, 7],
        "6" | "M6" | "maj6" => vec![0, 4, 7, 9],
        "m6" | "min6" | "-6" => vec![0, 3, 7, 9],
        "7" | "dom7" => vec![0, 4, 7, 10],
        "maj7" | "^7" | "M7" => vec![0, 4, 7, 11],
        "m7" | "min7" | "-7" => vec![0, 3, 7, 10],
        "mMaj7" | "-^7" | "minMaj7" => vec![0, 3, 7, 11],
        "7s5" | "7#5" | "+7" | "aug7" => vec![0, 4, 8, 10],
        "7b5" => vec![0, 4, 6, 10],
        "7b9" => vec![0, 4, 7, 10, 13],
        "7#9" | "7s9" => vec![0, 4, 7, 10, 15],
        "9" | "dom9" => vec![0, 4, 7, 10, 14],
        "maj9" | "^9" | "M9" => vec![0, 4, 7, 11, 14],
        "m9" | "min9" | "-9" => vec![0, 3, 7, 10, 14],
        "11" => vec![0, 4, 7, 10, 14, 17],
        "m11" | "min11" | "-11" => vec![0, 3, 7, 10, 14, 17],
        "13" => vec![0, 4, 7, 10, 14, 21],
        "m13" | "min13" | "-13" => vec![0, 3, 7, 10, 14, 21],
        "maj13" | "^13" => vec![0, 4, 7, 11, 14, 21],
        _ => vec![0, 4, 7],
    };

    let final_intervals = match inline_dict.as_deref() {
        Some("triad" | "triads") => intervals.into_iter().take(3).collect(),
        Some("guidetones") => {
            if intervals.len() >= 4 {
                vec![intervals[1], intervals[3]]
            } else {
                intervals
            }
        }
        Some("lefthand") => {
            if intervals.len() >= 4 {
                vec![intervals[1], intervals[2], intervals[3], 14]
            } else {
                intervals
            }
        }
        Some("drop2") => {
            if intervals.len() >= 4 {
                let mut d = intervals.clone();
                let idx = d.len() - 2;
                d[idx] -= 12;
                d.sort();
                d
            } else {
                intervals
            }
        }
        _ => intervals,
    };

    Some(ParsedChord {
        root_semitone: (root_base + acc).rem_euclid(12),
        root_octave: 4,
        quality: qual.to_string(),
        intervals: final_intervals,
        inline_dict,
    })
}

/// Tonal scale/chord/voicing operations on note/degree/symbol patterns.
pub trait TonalExt {
    /// Maps integer scale degrees (or note offsets) through a named scale.
    fn scale(&self, scale_name: &Pattern<String>) -> Pattern<Value>;

    /// Transposes note values by `steps` semitones or scale steps.
    fn scale_transpose(&self, steps: &Pattern<i64>) -> Pattern<Value>;

    /// Chromatic transposition by `semitones`.
    fn transpose(&self, semitones: &Pattern<f64>) -> Pattern<Value>;

    /// Parses chord symbols into chordal `Value::List` pitch sets (`chord`).
    fn chord(&self, chord_pat: &Pattern<String>) -> Pattern<Value>;

    /// Expands chord symbols into voice-led voicings (`voicing`).
    fn voicing(&self) -> Pattern<Value>;

    /// Sets the voicing dictionary used by `voicing()`.
    fn dict(&self, dictionary: &Pattern<String>) -> Pattern<Value>;

    /// Extracts the root note of each chord in a chord pattern (`rootNotes`).
    fn root_notes(&self, octave: i32) -> Pattern<Value>;

    /// Anchors voicing register around a target pitch (`anchor`).
    fn anchor(&self, pitch: &Pattern<String>) -> Pattern<Value>;
}

impl TonalExt for Pattern<Value> {
    fn scale(&self, scale_name: &Pattern<String>) -> Pattern<Value> {
        lift_a2_left(self, scale_name, |val, sname| {
            let (root_midi, intervals) = parse_scale_descriptor(sname);
            let deg = val.as_i64().unwrap_or(0);
            let midi = resolve_scale_degree(root_midi, intervals, deg);
            Value::Note(NoteValue::Midi(midi))
        })
    }

    fn scale_transpose(&self, steps: &Pattern<i64>) -> Pattern<Value> {
        lift_a2_left(self, steps, |val, &st| {
            let base = val.as_f64().unwrap_or(60.0);
            Value::Note(NoteValue::Midi(base + (st as f64)))
        })
    }

    fn transpose(&self, semitones: &Pattern<f64>) -> Pattern<Value> {
        lift_a2_left(self, semitones, |val, &semi| {
            let base = val.as_f64().unwrap_or(60.0);
            Value::Note(NoteValue::Midi(base + semi))
        })
    }

    fn chord(&self, chord_pat: &Pattern<String>) -> Pattern<Value> {
        lift_a2_left(self, chord_pat, |_val, sym| {
            if let Some(parsed) = parse_chord_symbol(sym) {
                let root_midi = 12 * (parsed.root_octave + 1) + parsed.root_semitone;
                let notes = parsed
                    .intervals
                    .into_iter()
                    .map(|iv| Value::Note(NoteValue::Midi((root_midi + iv) as f64)))
                    .collect();
                Value::List(notes)
            } else {
                Value::Null
            }
        })
    }

    fn voicing(&self) -> Pattern<Value> {
        self.fmap(|val| match val {
            Value::String(sym) => {
                if let Some(parsed) = parse_chord_symbol(sym) {
                    let root_midi = 12 * (parsed.root_octave + 1) + parsed.root_semitone;
                    let notes = parsed
                        .intervals
                        .into_iter()
                        .map(|iv| Value::Note(NoteValue::Midi((root_midi + iv) as f64)))
                        .collect();
                    Value::List(notes)
                } else {
                    val.clone()
                }
            }
            _ => val.clone(),
        })
    }

    fn dict(&self, dictionary: &Pattern<String>) -> Pattern<Value> {
        lift_a2_left(self, dictionary, |val, dname| match val {
            Value::String(sym) => {
                let tagged = if sym.contains('@') {
                    sym.clone()
                } else {
                    alloc::format!("{}@{}", sym, dname)
                };
                if let Some(parsed) = parse_chord_symbol(&tagged) {
                    let root_midi = 12 * (parsed.root_octave + 1) + parsed.root_semitone;
                    let notes = parsed
                        .intervals
                        .into_iter()
                        .map(|iv| Value::Note(NoteValue::Midi((root_midi + iv) as f64)))
                        .collect();
                    Value::List(notes)
                } else {
                    Value::String(tagged)
                }
            }
            _ => val.clone(),
        })
    }

    fn root_notes(&self, octave: i32) -> Pattern<Value> {
        self.fmap(move |val| match val {
            Value::String(sym) => {
                if let Some(parsed) = parse_chord_symbol(sym) {
                    let midi = 12 * (octave + 1) + parsed.root_semitone;
                    Value::Note(NoteValue::Midi(midi as f64))
                } else {
                    val.clone()
                }
            }
            Value::List(items) => items.first().cloned().unwrap_or(Value::Null),
            _ => val.clone(),
        })
    }

    fn anchor(&self, pitch: &Pattern<String>) -> Pattern<Value> {
        lift_a2_left(self, pitch, |val, target_str| {
            let anchor_midi = NoteValue::Name(target_str.clone())
                .to_midi(EngineCompatibilityProfile::SuperDoughC4)
                .unwrap_or(60.0);
            match val {
                Value::List(notes) => {
                    let adjusted = notes
                        .iter()
                        .map(|n| {
                            let mut m = n.as_f64().unwrap_or(60.0);
                            while m > anchor_midi + 6.0 {
                                m -= 12.0;
                            }
                            while m < anchor_midi - 6.0 {
                                m += 12.0;
                            }
                            Value::Note(NoteValue::Midi(m))
                        })
                        .collect();
                    Value::List(adjusted)
                }
                _ => val.clone(),
            }
        })
    }
}

/// Patterned arpeggiator `arp_with` accepting a pattern of index-transform functions (`PP-06`, Tidal #620).
pub fn arp_with<F>(chord_list_pat: &Pattern<Value>, mode_pat: &Pattern<F>) -> Pattern<Value>
where
    F: Fn(&[Value]) -> Vec<Value> + Send + Sync + 'static,
{
    let combined = crate::algebra::lift_a2_both(chord_list_pat, mode_pat, |val, f| match val {
        Value::List(notes) => {
            let ordered = f(notes);
            crate::combinators::from_list(ordered)
        }
        other => crate::combinators::pure(other.clone()),
    });
    <Pattern<Value> as PatternMonad<Value>>::squeeze_join(&combined)
}

/// Microtonal Scala (`.scl` / `.kbm`) & N-EDO tuning contract.
#[derive(Clone, Debug, PartialEq)]
pub struct ScalaTuning {
    pub description: String,
    pub pitches_cents: Vec<f64>,
    pub reference_midi: i32,
    pub reference_freq_hz: f64,
}

impl ScalaTuning {
    /// Parses standard Scala `.scl` and optional `.kbm` keyboard mapping text into a tuning table.
    pub fn from_scl_kbm(scl_text: &str, kbm_text: Option<&str>) -> Result<Self, String> {
        let mut non_comment_lines = scl_text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('!'));

        let description = non_comment_lines
            .next()
            .ok_or_else(|| "missing Scala description line".to_string())?
            .to_string();

        let count_line = non_comment_lines
            .next()
            .filter(|l| !l.is_empty())
            .ok_or_else(|| "missing Scala note count line".to_string())?;
        let declared_count = count_line
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| "invalid Scala note count".to_string())?;

        let mut pitches_cents = Vec::with_capacity(declared_count);
        for line in non_comment_lines {
            if line.is_empty() {
                continue;
            }
            let token = line.split_whitespace().next().unwrap_or("");
            if token.contains('.') {
                let cents = token
                    .parse::<f64>()
                    .map_err(|_| "invalid cents pitch value".to_string())?;
                pitches_cents.push(cents);
            } else if let Some((n_str, d_str)) = token.split_once('/') {
                let n = n_str
                    .parse::<f64>()
                    .map_err(|_| "invalid ratio numerator".to_string())?;
                let d = d_str
                    .parse::<f64>()
                    .map_err(|_| "invalid ratio denominator".to_string())?;
                if n <= 0.0 || d <= 0.0 {
                    return Err("Scala ratio must be positive".to_string());
                }
                let cents = 1200.0 * (ln_f64(n / d) / core::f64::consts::LN_2);
                pitches_cents.push(cents);
            } else {
                let ratio = token
                    .parse::<f64>()
                    .map_err(|_| "invalid integer ratio".to_string())?;
                if ratio <= 0.0 {
                    return Err("Scala integer ratio must be positive".to_string());
                }
                let cents = 1200.0 * (ln_f64(ratio) / core::f64::consts::LN_2);
                pitches_cents.push(cents);
            }
        }

        if pitches_cents.len() != declared_count {
            return Err("Scala pitch count mismatch".to_string());
        }

        let mut reference_midi = 69;
        let mut reference_freq_hz = 440.0;
        if let Some(kbm) = kbm_text {
            let kbm_vals: Vec<&str> = kbm
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('!'))
                .collect();
            if kbm_vals.len() >= 5 {
                if let Ok(rm) = kbm_vals[3].split_whitespace().next().unwrap_or("69").parse::<i32>() {
                    reference_midi = rm;
                }
                if let Ok(rf) = kbm_vals[4]
                    .split_whitespace()
                    .next()
                    .unwrap_or("440.0")
                    .parse::<f64>()
                {
                    if rf > 0.0 {
                        reference_freq_hz = rf;
                    }
                }
            }
        }

        Ok(Self {
            description,
            pitches_cents,
            reference_midi,
            reference_freq_hz,
        })
    }

    /// Constructs an `n`-division equal temperament of the `octave_ratio` (default `2.0`).
    pub fn edo(divisions: u32, octave_ratio: f64) -> Self {
        let divs = divisions.max(1);
        let period_cents = 1200.0 * (ln_f64(octave_ratio.max(1.0001)) / core::f64::consts::LN_2);
        let pitches_cents = (1..=divs)
            .map(|k| period_cents * (k as f64) / (divs as f64))
            .collect();
        Self {
            description: alloc::format!("{}-EDO", divs),
            pitches_cents,
            reference_midi: 69,
            reference_freq_hz: 440.0,
        }
    }

    /// Resolves scale step `degree` (relative to `reference_midi` / degree 0) to exact frequency in Hz.
    pub fn degree_to_freq_hz(&self, degree: f64) -> f64 {
        if self.pitches_cents.is_empty() {
            return self.reference_freq_hz;
        }
        let n = self.pitches_cents.len() as i64;
        let period_cents = *self.pitches_cents.last().unwrap_or(&1200.0);
        let int_deg = if degree >= 0.0 {
            degree as i64
        } else {
            let f = degree as i64;
            if (f as f64) == degree { f } else { f - 1 }
        };
        let frac = degree - (int_deg as f64);
        let periods = int_deg.div_euclid(n);
        let step_idx = int_deg.rem_euclid(n) as usize;
        let step_cents = if step_idx == 0 {
            0.0
        } else {
            self.pitches_cents[step_idx - 1]
        };
        let next_cents = self.pitches_cents[step_idx];
        let total_cents =
            (periods as f64) * period_cents + step_cents + frac * (next_cents - step_cents);
        self.reference_freq_hz * exp2_f64(total_cents / 1200.0)
    }
}
