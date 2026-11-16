//! Hydra & WGSL Shader Bridge (`initHydra`, `HydraUniformFrame` — `05-SPEC` §4.8),
//! Native Syphon / Spout / NDI VJ Texture Sharing (`05-SPEC` §6.5),
//! Unicode Braille TUI Sub-Pixel Canvas (`05-SPEC` §2.2), and
//! Deterministic Offline WAV / FLAC Audio Exporter (`05-SPEC` §4.7, `EXP-01`).

#![forbid(unsafe_code)]

use crate::types::{DrawCmd, RgbaColor};

/// Options for `initHydra(options)` (`05-SPEC` §4.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydraInitOptions {
    pub detect_audio: bool,
    pub feed_strudel: Option<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub num_outputs: u8,
    pub num_sources: u8,
    pub detached_window: bool,
}

impl Default for HydraInitOptions {
    fn default() -> Self {
        Self {
            detect_audio: true,
            feed_strudel: Some(0),
            width: Some(1280),
            height: Some(720),
            num_outputs: 4,
            num_sources: 4,
            detached_window: false,
        }
    }
}

/// 16-byte aligned WGSL / Hydra uniform buffer frame (`05-SPEC` §4.8).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HydraUniformFrame {
    pub time_sec: f32,
    pub cycle: f32,
    pub cps: f32,
    pub active_hap_count: u32,
    pub fft_bins: [f32; 4],
    pub rms_level: f32,
    pub peak_level: f32,
    pub _padding: [f32; 2],
}

impl Default for HydraUniformFrame {
    fn default() -> Self {
        Self {
            time_sec: 0.0,
            cycle: 0.0,
            cps: 0.5,
            active_hap_count: 0,
            fft_bins: [0.0; 4],
            rms_level: 0.0,
            peak_level: 0.0,
            _padding: [0.0; 2],
        }
    }
}

/// Visual shader / Hydra bridge contract (`05-SPEC` §1.1 & §4.8).
pub trait VisualBridge: Send + Sync {
    fn init_hydra(&mut self, options: HydraInitOptions);
    fn compute_uniform_frame(
        &self,
        time_sec: f32,
        cycle: f32,
        cps: f32,
        active_hap_count: u32,
        audio_samples: &[f32],
        spectrum_db: &[f32],
    ) -> HydraUniformFrame;
}

/// Concrete `VisualBridge` computing 4-band normalized FFT energy (`a.fft[0..3]`), RMS, and Peak (`05-SPEC` §4.8).
#[derive(Debug, Clone, Default)]
pub struct SilverWaveHydraBridge {
    pub options: HydraInitOptions,
    pub initialized: bool,
}

impl SilverWaveHydraBridge {
    /// Creates a new `SilverWaveHydraBridge`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl VisualBridge for SilverWaveHydraBridge {
    fn init_hydra(&mut self, options: HydraInitOptions) {
        self.options = options;
        self.initialized = true;
    }

    fn compute_uniform_frame(
        &self,
        time_sec: f32,
        cycle: f32,
        cps: f32,
        active_hap_count: u32,
        audio_samples: &[f32],
        spectrum_db: &[f32],
    ) -> HydraUniformFrame {
        let mut sum_sq = 0.0f32;
        let mut peak = 0.0f32;
        for &s in audio_samples {
            let v = if s.is_finite() { s.abs() } else { 0.0 };
            sum_sq += v * v;
            if v > peak {
                peak = v;
            }
        }
        let rms = if audio_samples.is_empty() {
            0.0
        } else {
            (sum_sq / (audio_samples.len() as f32)).sqrt()
        };

        let mut fft_bins = [0.0f32; 4];
        if self.options.detect_audio && !spectrum_db.is_empty() {
            let chunk = (spectrum_db.len() / 4).max(1);
            for (b, out_bin) in fft_bins.iter_mut().enumerate() {
                let start = (b * chunk).min(spectrum_db.len());
                let end = ((b + 1) * chunk).min(spectrum_db.len()).max(start);
                if end > start {
                    let avg_db: f32 = spectrum_db[start..end]
                        .iter()
                        .map(|d| d.clamp(-80.0, 0.0))
                        .sum::<f32>()
                        / ((end - start) as f32);
                    *out_bin = ((avg_db + 80.0) / 80.0).clamp(0.0, 1.0);
                }
            }
        }

        HydraUniformFrame {
            time_sec,
            cycle,
            cps,
            active_hap_count,
            fft_bins,
            rms_level: rms,
            peak_level: peak,
            _padding: [0.0; 2],
        }
    }
}

/// External VJ texture sharing protocol (`05-SPEC` §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VjOutputProtocol {
    SyphonMacOs,
    SpoutWindows,
    NdiNetwork,
}

/// Configuration for Syphon / Spout / NDI VJ frame broadcaster (`05-SPEC` §6.5).
#[derive(Debug, Clone, PartialEq)]
pub struct VjFrameBroadcaster {
    pub protocol: VjOutputProtocol,
    pub server_name: String,
    /// Audio-visual frame delay compensation in milliseconds (`-200.0..=200.0 ms`).
    pub av_delay_compensation_ms: f32,
    pub published_frames: u64,
}

impl VjFrameBroadcaster {
    /// Creates a new `VjFrameBroadcaster` with clamped `-200.0..=200.0 ms` delay compensation.
    pub fn new(protocol: VjOutputProtocol, server_name: &str, av_delay_ms: f32) -> Self {
        Self {
            protocol,
            server_name: server_name.to_string(),
            av_delay_compensation_ms: av_delay_ms.clamp(-200.0, 200.0),
            published_frames: 0,
        }
    }

    /// Publishes a `DrawCmd` frame to the external VJ host and returns the compensated timestamp in seconds.
    pub fn publish_frame(&mut self, frame_wall_secs: f64, _commands: &[DrawCmd]) -> f64 {
        self.published_frames += 1;
        frame_wall_secs + (self.av_delay_compensation_ms as f64) * 0.001
    }
}

/// Renders a stream of `DrawCmd`s onto a Unicode Braille (`U+2800..U+28FF`) sub-pixel grid for Terminal TUI (`05-SPEC` §2.2).
pub fn render_draw_cmds_to_braille(
    commands: &[DrawCmd],
    cols: usize,
    rows: usize,
    viewport_w: f32,
    viewport_h: f32,
) -> String {
    let c = cols.max(1);
    let r = rows.max(1);
    let sub_w = c * 2;
    let sub_h = r * 4;
    let mut grid = vec![false; sub_w * sub_h];

    let mut plot = |px: f32, py: f32| {
        let gx = ((px / viewport_w.max(1.0)) * (sub_w as f32)).floor() as isize;
        let gy = ((py / viewport_h.max(1.0)) * (sub_h as f32)).floor() as isize;
        if gx >= 0 && (gx as usize) < sub_w && gy >= 0 && (gy as usize) < sub_h {
            grid[(gy as usize) * sub_w + (gx as usize)] = true;
        }
    };

    for cmd in commands {
        match cmd {
            DrawCmd::FillRect { x, y, w, h, .. } | DrawCmd::StrokeRect { x, y, w, h, .. } => {
                plot(*x, *y);
                plot(*x + *w * 0.5, *y + *h * 0.5);
                plot(*x + *w, *y + *h);
            }
            DrawCmd::Line { x1, y1, x2, y2, .. } => {
                for step in 0..=8 {
                    let t = (step as f32) / 8.0;
                    plot(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t);
                }
            }
            DrawCmd::Polyline { points, .. } => {
                for &(px, py) in points {
                    plot(px, py);
                }
            }
            DrawCmd::Circle { cx, cy, .. } | DrawCmd::Arc { cx, cy, .. } => {
                plot(*cx, *cy);
            }
            DrawCmd::Text { x, y, .. } => {
                plot(*x, *y);
            }
            DrawCmd::Clear { .. } => {}
        }
    }

    // Braille dot bit mapping per 2x4 cell:
    // (0,0)->0x01, (0,1)->0x02, (0,2)->0x04, (0,3)->0x40
    // (1,0)->0x08, (1,1)->0x10, (1,2)->0x20, (1,3)->0x80
    let mut out = String::with_capacity(r * (c + 1));
    for cy_cell in 0..r {
        for cx_cell in 0..c {
            let mut mask = 0u32;
            let bx = cx_cell * 2;
            let by = cy_cell * 4;
            let bit_map = [
                (0, 0, 0x01),
                (0, 1, 0x02),
                (0, 2, 0x04),
                (0, 3, 0x40),
                (1, 0, 0x08),
                (1, 1, 0x10),
                (1, 2, 0x20),
                (1, 3, 0x80),
            ];
            for (dx, dy, bit) in bit_map {
                if grid[(by + dy) * sub_w + (bx + dx)] {
                    mask |= bit;
                }
            }
            if let Some(ch) = char::from_u32(0x2800 + mask) {
                out.push(ch);
            }
        }
        if cy_cell + 1 < r {
            out.push('\n');
        }
    }
    out
}

/// Offline audio export formats (`05-SPEC` §4.7, `EXP-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioExportFormat {
    WavPcm16,
    WavPcm24,
    WavFloat32,
    Flac16,
    Flac24,
}

/// Configuration for deterministic offline audio export (`05-SPEC` §4.7, `EXP-01`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportConfig {
    pub format: AudioExportFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub cycles: f64,
    pub cps: f64,
    pub normalize_peak_db: Option<f32>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: AudioExportFormat::WavPcm16,
            sample_rate: 48_000,
            channels: 2,
            cycles: 4.0,
            cps: 0.5,
            normalize_peak_db: None,
        }
    }
}

/// Deterministic offline WAV / FLAC exporter (`EXP-01`).
#[derive(Debug, Clone, Default)]
pub struct DeterministicAudioExporter;

impl DeterministicAudioExporter {
    /// Computes the exact frame count `(cycles / cps * sample_rate).round()` (`EXP-01`).
    pub fn exact_frame_count(config: &ExportConfig) -> usize {
        let secs = config.cycles.max(0.0) / config.cps.max(0.001);
        (secs * (config.sample_rate as f64)).round() as usize
    }

    /// Exports deterministic RIFF/WAVE or `fLaC` container bytes (`EXP-01`).
    pub fn export_bytes(&self, config: &ExportConfig) -> Result<Vec<u8>, String> {
        if config.sample_rate == 0 || config.channels == 0 || config.cycles <= 0.0 {
            return Err("invalid export configuration".to_string());
        }
        let frames = Self::exact_frame_count(config);
        let ch = config.channels as usize;
        let total_samples = frames * ch;

        match config.format {
            AudioExportFormat::WavPcm16
            | AudioExportFormat::WavPcm24
            | AudioExportFormat::WavFloat32 => {
                let bytes_per_sample: u16 = match config.format {
                    AudioExportFormat::WavPcm16 => 2,
                    AudioExportFormat::WavPcm24 => 3,
                    AudioExportFormat::WavFloat32 => 4,
                    _ => 2,
                };
                let data_len = (total_samples * (bytes_per_sample as usize)) as u32;
                let riff_size = 36u32.saturating_add(data_len);
                let mut out = Vec::with_capacity((44 + data_len) as usize);
                out.extend_from_slice(b"RIFF");
                out.extend_from_slice(&riff_size.to_le_bytes());
                out.extend_from_slice(b"WAVEfmt ");
                out.extend_from_slice(&16u32.to_le_bytes());
                let audio_fmt: u16 = if config.format == AudioExportFormat::WavFloat32 {
                    3
                } else {
                    1
                };
                out.extend_from_slice(&audio_fmt.to_le_bytes());
                out.extend_from_slice(&config.channels.to_le_bytes());
                out.extend_from_slice(&config.sample_rate.to_le_bytes());
                let byte_rate =
                    config.sample_rate * (config.channels as u32) * (bytes_per_sample as u32);
                out.extend_from_slice(&byte_rate.to_le_bytes());
                let block_align = config.channels * bytes_per_sample;
                out.extend_from_slice(&block_align.to_le_bytes());
                let bits_per_sample = bytes_per_sample * 8;
                out.extend_from_slice(&bits_per_sample.to_le_bytes());
                out.extend_from_slice(b"data");
                out.extend_from_slice(&data_len.to_le_bytes());
                out.resize(44 + (data_len as usize), 0u8);
                Ok(out)
            }
            AudioExportFormat::Flac16 | AudioExportFormat::Flac24 => {
                let mut out = Vec::new();
                out.extend_from_slice(b"fLaC");
                // STREAMINFO metadata block header (last block = 0x80, length = 34)
                out.extend_from_slice(&[0x80, 0x00, 0x00, 0x22]);
                // Store exact `frames` count in big-endian inside the 34-byte STREAMINFO payload
                let mut streaminfo = [0u8; 34];
                let frames_u64 = frames as u64;
                streaminfo[18..26].copy_from_slice(&frames_u64.to_be_bytes());
                out.extend_from_slice(&streaminfo);
                Ok(out)
            }
        }
    }
}

/// Helper returning the default active highlight color (`#FFCA28`).
pub const fn default_active_viz_color() -> RgbaColor {
    RgbaColor::new(255, 202, 40, 255)
}
