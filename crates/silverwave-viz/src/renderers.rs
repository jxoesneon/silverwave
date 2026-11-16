//! Deterministic 1:1 Renderers for all 6 Strudel Visualizers (`pianoroll`, `punchcard`,
//! `spiral`, `scope`, `pitchwheel`, `spectrum` — `05-SPEC` §3.3–3.7, `VIZ-01`..`VIZ-07`).

#![forbid(unsafe_code)]

use crate::types::{CycleTime, DrawCmd, LineCap, RgbaColor, VizHap, VizViewport};
use core::f32::consts::{PI, TAU};

/// Options for `pianoroll` / `_pianoroll` and `punchcard` / `_punchcard` (`05-SPEC` §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct PianorollOptions {
    pub cycles: u32,
    pub playhead: f32,
    pub vertical: bool,
    pub labels: bool,
    pub flip_time: bool,
    pub flip_values: bool,
    pub overscan: f32,
    pub hide_negative: bool,
    pub smear: bool,
    pub fold: bool,
    pub active: RgbaColor,
    pub inactive: RgbaColor,
    pub background: RgbaColor,
    pub playhead_color: RgbaColor,
    pub fill: bool,
    pub fill_active: bool,
    pub stroke: bool,
    pub stroke_active: bool,
    pub hide_inactive: bool,
    pub colorize_inactive: bool,
    pub font_family: String,
    pub min_midi: i32,
    pub max_midi: i32,
    pub autorange: bool,
}

impl Default for PianorollOptions {
    fn default() -> Self {
        Self {
            cycles: 4,
            playhead: 0.5,
            vertical: false,
            labels: false,
            flip_time: false,
            flip_values: false,
            overscan: 1.0,
            hide_negative: false,
            smear: false,
            fold: false,
            active: RgbaColor::new(255, 202, 40, 255),
            inactive: RgbaColor::new(116, 145, 210, 255),
            background: RgbaColor::new(0, 0, 0, 0),
            playhead_color: RgbaColor::new(255, 255, 255, 255),
            fill: true,
            fill_active: true,
            stroke: false,
            stroke_active: false,
            hide_inactive: false,
            colorize_inactive: true,
            font_family: "monospace".to_string(),
            min_midi: 10,
            max_midi: 90,
            autorange: false,
        }
    }
}

/// Trait for `pianoroll` and `punchcard` rendering (`05-SPEC` §3.3, `VIZ-01`, `VIZ-02`).
pub trait PianorollRenderer {
    fn render_pianoroll(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;

    fn render_punchcard(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}

/// Deterministic `PianorollRenderer` implementation (`VIZ-01`, `VIZ-02`).
pub struct SilverWavePianoroll;

impl SilverWavePianoroll {
    fn render_internal(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
        use_post_transform_color: bool,
    ) -> Vec<DrawCmd> {
        let mut cmds = Vec::new();
        if opts.background.a > 0 {
            cmds.push(DrawCmd::Clear {
                color: opts.background,
            });
        }
        let span_cycles = (opts.cycles.max(1) as f64).max(0.25);
        let win_start = now - (opts.playhead as f64) * span_cycles;
        let win_end = win_start + span_cycles;

        let (min_m, max_m) = if opts.autorange && !haps.is_empty() {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for h in haps {
                lo = lo.min(h.midi_pitch);
                hi = hi.max(h.midi_pitch);
            }
            ((lo - 2.0).floor(), (hi + 2.0).ceil().max(lo + 4.0))
        } else {
            (
                opts.min_midi as f32,
                (opts.max_midi.max(opts.min_midi + 1)) as f32,
            )
        };
        let pitch_span = (max_m - min_m).max(1.0);
        let note_h = (viewport.height / pitch_span).clamp(2.0, viewport.height * 0.25);

        for h in haps {
            if opts.hide_negative && h.end_cycle < 0.0 {
                continue;
            }
            if h.end_cycle < win_start || h.start_cycle > win_end {
                continue;
            }
            let active = h.is_active_at(now);
            if !active && opts.hide_inactive {
                continue;
            }
            let hap_custom_color = if use_post_transform_color {
                h.post_transform_color.or(h.pre_transform_color)
            } else {
                h.pre_transform_color
            };
            let color = if active {
                hap_custom_color.unwrap_or(opts.active)
            } else if opts.colorize_inactive {
                hap_custom_color.unwrap_or(opts.inactive)
            } else {
                opts.inactive
            };

            let t0 = ((h.start_cycle - win_start) / span_cycles).clamp(0.0, 1.0) as f32;
            let t1 = ((h.end_cycle - win_start) / span_cycles).clamp(0.0, 1.0) as f32;
            let mut x_norm = if opts.flip_time { 1.0 - t1 } else { t0 };
            let w_norm = (t1 - t0).max(0.005);
            x_norm = x_norm.clamp(0.0, 1.0 - w_norm);

            let mut p_norm = ((h.midi_pitch - min_m) / pitch_span).clamp(0.02, 0.96);
            if !opts.flip_values {
                p_norm = 1.0 - p_norm;
            }

            let (rx, ry, rw, rh) = if opts.vertical {
                (
                    p_norm * (viewport.width - note_h),
                    x_norm * viewport.height,
                    note_h,
                    w_norm * viewport.height,
                )
            } else {
                (
                    x_norm * viewport.width,
                    (p_norm * (viewport.height - note_h)).clamp(1.0, (viewport.height - note_h - 1.0).max(1.0)),
                    w_norm * viewport.width,
                    note_h,
                )
            };

            if opts.stroke || (active && opts.stroke_active) {
                cmds.push(DrawCmd::StrokeRect {
                    x: rx,
                    y: ry,
                    w: rw,
                    h: rh,
                    thickness: 1.5,
                    color,
                });
            } else {
                cmds.push(DrawCmd::FillRect {
                    x: rx,
                    y: ry,
                    w: rw,
                    h: rh,
                    color,
                });
            }

            if opts.labels {
                if let Some(ref lbl) = h.label {
                    cmds.push(DrawCmd::Text {
                        x: rx + 2.0,
                        y: ry + rh * 0.75,
                        content: lbl.clone(),
                        font_family: opts.font_family.clone(),
                        size_px: 10.0,
                        color: opts.playhead_color,
                    });
                }
            }
        }

        // Playhead line
        let ph_x = opts.playhead.clamp(0.0, 1.0) * viewport.width;
        cmds.push(DrawCmd::Line {
            x1: ph_x,
            y1: 0.0,
            x2: ph_x,
            y2: viewport.height,
            thickness: 2.0,
            cap: LineCap::Butt,
            color: opts.playhead_color,
        });
        cmds
    }
}

impl PianorollRenderer for SilverWavePianoroll {
    fn render_pianoroll(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        Self::render_internal(haps, now, opts, viewport, false)
    }

    fn render_punchcard(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PianorollOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        Self::render_internal(haps, now, opts, viewport, true)
    }
}

/// Options for `spiral` / `_spiral` (`05-SPEC` §3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct SpiralOptions {
    pub stretch: f32,
    pub size: Option<f32>,
    pub thickness: f32,
    pub cap: LineCap,
    pub inset: f32,
    pub playhead_color: RgbaColor,
    pub playhead_length: f32,
    pub playhead_thickness: Option<f32>,
    pub padding: f32,
    pub steady: f32,
    pub active_color: Option<RgbaColor>,
    pub inactive_color: Option<RgbaColor>,
    pub colorize_inactive: bool,
    pub fade: bool,
    pub log_spiral: bool,
}

impl Default for SpiralOptions {
    fn default() -> Self {
        Self {
            stretch: 1.0,
            size: None,
            thickness: 3.0,
            cap: LineCap::Butt,
            inset: 3.0,
            playhead_color: RgbaColor::new(255, 255, 255, 255),
            playhead_length: 0.02,
            playhead_thickness: None,
            padding: 0.0,
            steady: 0.0,
            active_color: None,
            inactive_color: None,
            colorize_inactive: false,
            fade: true,
            log_spiral: false,
        }
    }
}

/// Trait for `spiral` rendering (`05-SPEC` §3.4, `VIZ-03`).
pub trait SpiralRenderer {
    fn render_spiral(
        haps: &[VizHap],
        now: CycleTime,
        opts: &SpiralOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}

/// Deterministic `SpiralRenderer` implementation (`VIZ-03`).
pub struct SilverWaveSpiral;

impl SpiralRenderer for SilverWaveSpiral {
    fn render_spiral(
        haps: &[VizHap],
        now: CycleTime,
        opts: &SpiralOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        let mut cmds = Vec::new();
        let cx = viewport.width * 0.5;
        let cy = viewport.height * 0.5;
        let diam = opts
            .size
            .unwrap_or_else(|| viewport.width.min(viewport.height) - opts.padding * 2.0)
            .max(16.0);
        let max_r = diam * 0.5;
        let active_col = opts.active_color.unwrap_or(viewport.theme_fg);
        let inactive_col = opts.inactive_color.unwrap_or(viewport.theme_gutter_fg);

        // When `steady == 1.0`, spiral geometry is stationary (`rot_offset == 0.0`), while playhead rotates (`VIZ-03`).
        let rot_offset = ((1.0 - opts.steady.clamp(0.0, 1.0)) as f64) * now;

        for h in haps {
            let rel_start = ((h.start_cycle - rot_offset) as f32) * opts.stretch + opts.inset;
            let rel_end = ((h.end_cycle - rot_offset) as f32) * opts.stretch + opts.inset;
            if rel_end <= 0.0 {
                continue;
            }
            let r_mid = if opts.log_spiral {
                (rel_start.max(0.1).ln() * 12.0).clamp(4.0, max_r)
            } else {
                (rel_start * 8.0).clamp(4.0, max_r)
            };
            let start_rad = rel_start * TAU - PI * 0.5;
            let end_rad = rel_end * TAU - PI * 0.5;
            // In `steady: 1.0` mode, keep geometry and base color invariant across ticks (`VIZ-03`)
            let col = if opts.steady >= 1.0 {
                inactive_col
            } else if h.is_active_at(now) {
                h.post_transform_color.unwrap_or(active_col)
            } else if opts.colorize_inactive {
                h.post_transform_color.unwrap_or(inactive_col)
            } else {
                inactive_col
            };
            cmds.push(DrawCmd::Arc {
                cx,
                cy,
                radius: r_mid,
                start_rad,
                end_rad,
                thickness: opts.thickness,
                cap: opts.cap,
                color: col,
            });
        }

        // Playhead line rotates when `steady > 0.0`
        let ph_angle = ((now as f32) * opts.steady.clamp(0.0, 1.0) * opts.stretch) * TAU - PI * 0.5;
        let ph_r = max_r * 0.9;
        cmds.push(DrawCmd::Line {
            x1: cx,
            y1: cy,
            x2: cx + ph_r * ph_angle.cos(),
            y2: cy + ph_r * ph_angle.sin(),
            thickness: opts.playhead_thickness.unwrap_or(opts.thickness),
            cap: opts.cap,
            color: opts.playhead_color,
        });
        cmds
    }
}

/// Options for `scope` / `_scope` (`tscope` — `05-SPEC` §3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeOptions {
    pub align: bool,
    pub color: RgbaColor,
    pub thickness: f32,
    pub scale: f32,
    pub pos: f32,
    pub trigger: f32,
}

impl Default for ScopeOptions {
    fn default() -> Self {
        Self {
            align: true,
            color: RgbaColor::new(255, 255, 255, 255),
            thickness: 3.0,
            scale: 0.25,
            pos: 0.5,
            trigger: 0.0,
        }
    }
}

/// Trait for time-domain oscilloscope rendering (`05-SPEC` §3.5, `VIZ-04`).
pub trait ScopeRenderer {
    fn render_scope(
        samples: &[f32],
        opts: &ScopeOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}

/// Deterministic `ScopeRenderer` with rising-edge zero/trigger crossing alignment (`VIZ-04`).
pub struct SilverWaveScope;

impl ScopeRenderer for SilverWaveScope {
    fn render_scope(
        samples: &[f32],
        opts: &ScopeOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        if samples.is_empty() {
            return Vec::new();
        }
        let start_idx = if opts.align && samples.len() >= 2 {
            let mut found = 0usize;
            for i in 1..(samples.len() / 2).max(2) {
                if samples[i - 1] <= opts.trigger && samples[i] > opts.trigger {
                    found = i;
                    break;
                }
            }
            found
        } else {
            0
        };

        let slice = &samples[start_idx..];
        let n = slice.len().max(1);
        let center_y = opts.pos.clamp(0.0, 1.0) * viewport.height;
        let amp_scale = opts.scale * viewport.height;
        let mut pts = Vec::with_capacity(n);
        for (i, &s) in slice.iter().enumerate() {
            let x = if n <= 1 {
                0.0
            } else {
                (i as f32 / (n - 1) as f32) * viewport.width
            };
            let finite_s = if s.is_finite() { s.clamp(-2.0, 2.0) } else { 0.0 };
            let y = center_y - finite_s * amp_scale;
            pts.push((x, y));
        }

        vec![DrawCmd::Polyline {
            points: pts,
            thickness: opts.thickness,
            cap: LineCap::Round,
            color: opts.color,
            closed: false,
        }]
    }
}

/// Pitchwheel angular display mode (`05-SPEC` §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PitchwheelMode {
    Radians,
    Degrees,
    Steps,
}

/// Options for `pitchwheel` / `_pitchwheel` (`05-SPEC` §3.6).
#[derive(Debug, Clone, PartialEq)]
pub struct PitchwheelOptions {
    pub hapcircles: u32,
    pub circle: u32,
    pub edo: u32,
    pub root: String,
    pub thickness: f32,
    pub hap_radius: f32,
    pub mode: PitchwheelMode,
    pub margin: f32,
}

impl Default for PitchwheelOptions {
    fn default() -> Self {
        Self {
            hapcircles: 1,
            circle: 1,
            edo: 12,
            root: "C".to_string(),
            thickness: 2.0,
            hap_radius: 5.0,
            mode: PitchwheelMode::Steps,
            margin: 10.0,
        }
    }
}

/// Trait for `pitchwheel` rendering (`05-SPEC` §3.6, `VIZ-05`).
pub trait PitchwheelRenderer {
    fn render_pitchwheel(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PitchwheelOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}

/// Deterministic `PitchwheelRenderer` supporting arbitrary `edo` divisions (`VIZ-05`).
pub struct SilverWavePitchwheel;

impl SilverWavePitchwheel {
    /// Computes the angle in radians for `midi_pitch` given `edo` and `root_pc`, placing `root` at `-PI / 2` (top — `VIZ-05`).
    pub fn pitch_to_angle_rad(midi_pitch: f32, edo: u32, root_pc: f32) -> f32 {
        let e = (edo.max(1)) as f32;
        let step_in_edo = (midi_pitch - root_pc).rem_euclid(e);
        -PI * 0.5 + (step_in_edo / e) * TAU
    }

    fn parse_root_pc(root: &str) -> f32 {
        match root.trim().to_ascii_uppercase().as_str() {
            "C" => 0.0,
            "C#" | "DB" => 1.0,
            "D" => 2.0,
            "D#" | "EB" => 3.0,
            "E" => 4.0,
            "F" => 5.0,
            "F#" | "GB" => 6.0,
            "G" => 7.0,
            "G#" | "AB" => 8.0,
            "A" => 9.0,
            "A#" | "BB" => 10.0,
            "B" => 11.0,
            _ => 0.0,
        }
    }
}

impl PitchwheelRenderer for SilverWavePitchwheel {
    fn render_pitchwheel(
        haps: &[VizHap],
        now: CycleTime,
        opts: &PitchwheelOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        let mut cmds = Vec::new();
        let cx = viewport.width * 0.5;
        let cy = viewport.height * 0.5;
        let r = (viewport.width.min(viewport.height) * 0.5 - opts.margin).max(10.0);
        let root_pc = Self::parse_root_pc(&opts.root);

        if opts.circle > 0 {
            cmds.push(DrawCmd::Circle {
                cx,
                cy,
                radius: r,
                fill: None,
                stroke: Some((opts.thickness, viewport.theme_gutter_fg)),
            });
        }

        for h in haps {
            if !h.is_active_at(now) && opts.hapcircles == 0 {
                continue;
            }
            let ang = Self::pitch_to_angle_rad(h.midi_pitch, opts.edo, root_pc);
            let hx = cx + r * ang.cos();
            let hy = cy + r * ang.sin();
            let col = if h.is_active_at(now) {
                h.post_transform_color.unwrap_or(viewport.theme_fg)
            } else {
                viewport.theme_gutter_fg
            };
            cmds.push(DrawCmd::Circle {
                cx: hx,
                cy: hy,
                radius: opts.hap_radius,
                fill: Some(col),
                stroke: None,
            });
        }
        cmds
    }
}

/// Options for `spectrum` / `_spectrum` (`05-SPEC` §3.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpectrumOptions {
    pub thickness: u32,
    pub speed: u32,
    pub min: i32,
    pub max: i32,
}

impl Default for SpectrumOptions {
    fn default() -> Self {
        Self {
            thickness: 3,
            speed: 1,
            min: -80,
            max: 0,
        }
    }
}

/// Trait for frequency-domain spectrum rendering (`05-SPEC` §3.7, `VIZ-06`).
pub trait SpectrumRenderer {
    fn render_spectrum(
        bins_db: &[f32],
        sample_rate: u32,
        opts: &SpectrumOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd>;
}

/// Deterministic `SpectrumRenderer` clamping out-of-range dBFS values to `[min, max]` without NaN/Inf (`VIZ-06`).
pub struct SilverWaveSpectrum;

impl SpectrumRenderer for SilverWaveSpectrum {
    fn render_spectrum(
        bins_db: &[f32],
        _sample_rate: u32,
        opts: &SpectrumOptions,
        viewport: &VizViewport,
    ) -> Vec<DrawCmd> {
        if bins_db.is_empty() {
            return Vec::new();
        }
        let min_db = opts.min as f32;
        let max_db = (opts.max.max(opts.min + 1)) as f32;
        let span_db = (max_db - min_db).max(1.0);
        let n = bins_db.len();
        let mut pts = Vec::with_capacity(n);

        for (i, &raw_db) in bins_db.iter().enumerate() {
            let clamped_db = if raw_db.is_finite() {
                raw_db.clamp(min_db, max_db)
            } else if raw_db.is_sign_positive() {
                max_db
            } else {
                min_db
            };
            let norm = (clamped_db - min_db) / span_db; // 0.0 at min_db, 1.0 at max_db
            let x = if n <= 1 {
                0.0
            } else {
                (i as f32 / (n - 1) as f32) * viewport.width
            };
            let y = (1.0 - norm) * viewport.height;
            pts.push((x, y));
        }

        vec![DrawCmd::Polyline {
            points: pts,
            thickness: opts.thickness.max(1) as f32,
            cap: LineCap::Round,
            color: viewport.theme_fg,
            closed: false,
        }]
    }
}
