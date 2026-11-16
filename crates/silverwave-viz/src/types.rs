//! Core `silverwave-viz` Draw Command AST (`DrawCmd`, `RgbaColor`, `LineCap`),
//! Viewport (`VizViewport`), Targets (`VizKind`, `VizTarget`), and AudioTap (`05-SPEC` §2.1, §3.1–3.2).

#![forbid(unsafe_code)]

/// Cycle time in floating-point cycles (`05-SPEC` §2.1).
pub type CycleTime = f64;

/// 8-bit per-channel RGBA color (`05-SPEC` §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    /// Creates a new `RgbaColor`.
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses CSS named colors or `#RRGGBB` / `#RRGGBBAA` hex strings (`05-SPEC` §4.4).
    pub fn parse_css(input: &str) -> Option<Self> {
        let s = input.trim().to_ascii_lowercase();
        match s.as_str() {
            "cyan" => Some(Self::new(0, 255, 255, 255)),
            "magenta" => Some(Self::new(255, 0, 255, 255)),
            "yellow" => Some(Self::new(255, 255, 0, 255)),
            "red" => Some(Self::new(255, 0, 0, 255)),
            "green" => Some(Self::new(0, 255, 0, 255)),
            "blue" => Some(Self::new(0, 0, 255, 255)),
            "white" => Some(Self::new(255, 255, 255, 255)),
            "black" => Some(Self::new(0, 0, 0, 255)),
            "transparent" => Some(Self::new(0, 0, 0, 0)),
            "amber" | "gold" => Some(Self::new(255, 202, 40, 255)),
            _ if s.starts_with('#') => {
                let hex = &s[1..];
                if hex.len() == 6 {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    Some(Self::new(r, g, b, 255))
                } else if hex.len() == 8 {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                    Some(Self::new(r, g, b, a))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Formats `self` as `#rrggbb` or `#rrggbbaa`.
    pub fn to_css_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// Stroke line-cap style (`05-SPEC` §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// Platform-agnostic vector command emitted by all `silverwave-viz` visualizers (`05-SPEC` §3.2).
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Clear {
        color: RgbaColor,
    },
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: RgbaColor,
    },
    StrokeRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        thickness: f32,
        color: RgbaColor,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: f32,
        cap: LineCap,
        color: RgbaColor,
    },
    Polyline {
        points: Vec<(f32, f32)>,
        thickness: f32,
        cap: LineCap,
        color: RgbaColor,
        closed: bool,
    },
    Circle {
        cx: f32,
        cy: f32,
        radius: f32,
        fill: Option<RgbaColor>,
        stroke: Option<(f32, RgbaColor)>,
    },
    Arc {
        cx: f32,
        cy: f32,
        radius: f32,
        start_rad: f32,
        end_rad: f32,
        thickness: f32,
        cap: LineCap,
        color: RgbaColor,
    },
    Text {
        x: f32,
        y: f32,
        content: String,
        font_family: String,
        size_px: f32,
        color: RgbaColor,
    },
}

/// Visualizer family (`05-SPEC` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VizKind {
    Pianoroll,
    Punchcard,
    Spiral,
    Scope,
    Pitchwheel,
    Spectrum,
}

/// Global vs `_`-prefixed Inline visualizer target (`05-SPEC` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VizTarget {
    Global {
        kind: VizKind,
    },
    Inline {
        id: u32,
        kind: VizKind,
        anchor_line: u32,
    },
}

/// Viewport geometry and theme palette for visualizer rendering (`05-SPEC` §3.1).
#[derive(Debug, Clone, PartialEq)]
pub struct VizViewport {
    pub width: f32,
    pub height: f32,
    pub dpi_scale: f32,
    pub theme_fg: RgbaColor,
    pub theme_gutter_fg: RgbaColor,
    pub theme_bg: RgbaColor,
}

impl Default for VizViewport {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 200.0,
            dpi_scale: 1.0,
            theme_fg: RgbaColor::new(255, 202, 40, 255),
            theme_gutter_fg: RgbaColor::new(116, 145, 210, 255),
            theme_bg: RgbaColor::new(18, 18, 24, 255),
        }
    }
}

/// Complete rendered frame for a single `VizTarget` (`05-SPEC` §2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct VizFrame {
    pub target: VizTarget,
    pub commands: Vec<DrawCmd>,
}

/// Evaluated hap snapshot consumed by `pianoroll`, `punchcard`, `spiral`, and `pitchwheel` (`05-SPEC` §3.3–3.6).
#[derive(Debug, Clone, PartialEq)]
pub struct VizHap {
    pub start_cycle: CycleTime,
    pub end_cycle: CycleTime,
    pub midi_pitch: f32,
    pub velocity: f32,
    pub label: Option<String>,
    /// Pre-transform color captured at `.pianoroll()` call site (`VIZ-01`).
    pub pre_transform_color: Option<RgbaColor>,
    /// Post-transform color captured at final stream evaluation for `.punchcard()` (`VIZ-01`).
    pub post_transform_color: Option<RgbaColor>,
}

impl VizHap {
    /// Creates a new `VizHap`.
    pub fn new(start_cycle: CycleTime, end_cycle: CycleTime, midi_pitch: f32) -> Self {
        Self {
            start_cycle,
            end_cycle,
            midi_pitch,
            velocity: 0.8,
            label: None,
            pre_transform_color: None,
            post_transform_color: None,
        }
    }

    /// Returns `true` if `now` falls inside `[start_cycle, end_cycle)`.
    #[inline]
    pub fn is_active_at(&self, now: CycleTime) -> bool {
        now >= self.start_cycle && now < self.end_cycle
    }
}

/// Lock-free audio tap trait supplying time-domain and spectrum dBFS samples (`05-SPEC` §2.1).
pub trait AudioTap: Send + Sync {
    /// Copies up to `out.len()` time-domain samples into `out` and returns the count written.
    fn read_time_domain(&self, out: &mut [f32]) -> usize;

    /// Copies magnitude spectrum bins (in dBFS) into `out` and returns the stream sample rate.
    fn read_spectrum_db(&self, out: &mut [f32]) -> u32;
}

/// Deterministic in-memory `AudioTap` for testing and offline rendering.
#[derive(Debug, Clone)]
pub struct RingBufferAudioTap {
    pub time_samples: Vec<f32>,
    pub spectrum_bins_db: Vec<f32>,
    pub sample_rate: u32,
}

impl Default for RingBufferAudioTap {
    fn default() -> Self {
        Self {
            time_samples: vec![0.0; 512],
            spectrum_bins_db: vec![-60.0; 256],
            sample_rate: 48_000,
        }
    }
}

impl AudioTap for RingBufferAudioTap {
    fn read_time_domain(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(self.time_samples.len());
        out[..n].copy_from_slice(&self.time_samples[..n]);
        n
    }

    fn read_spectrum_db(&self, out: &mut [f32]) -> u32 {
        let n = out.len().min(self.spectrum_bins_db.len());
        out[..n].copy_from_slice(&self.spectrum_bins_db[..n]);
        self.sample_rate
    }
}
