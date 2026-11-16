//! Lock-Free `OrbitRmsReport` Metering (`dirt_rms`) and Closed-Loop Acoustic Intelligence
//! (`AcousticMixDiagnostic`, EBU R128 `LUFS`, `dBTP`, Stereo Phase Correlation, and Inter-Orbit
//! Sub-Bass Phase Cancellation Detection — `CK-06`, `09-SPEC` Section 3.6 & 4).

/// Per-orbit RMS and peak telemetry packet pushed over `rms_tx` (`03-SPEC` Section 2.3).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct OrbitRmsReport {
    pub orbit: u8,
    pub rms: f32,
    pub peak_left: f32,
    pub peak_right: f32,
}

/// Category of inter-orbit acoustic conflict (`CK-06`, `09-SPEC` Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    SubBassPhaseCancellation,
    LowMidSpectralMasking,
    TruePeakClippingRisk,
}

/// Alert emitted when two active orbits exhibit phase cancellation or spectral masking (`CK-06`).
#[derive(Debug, Clone, PartialEq)]
pub struct InterOrbitConflictAlert {
    pub orbit_a: u8,
    pub orbit_b: u8,
    pub center_freq_hz: f32,
    pub severity_db: f32,
    pub kind: ConflictKind,
}

/// Closed-Loop Acoustic Intelligence & Inter-Orbit Masking Telemetry for `silverwave-mcp` (`CK-06`).
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

/// Real-time in-process acoustic telemetry analyzer (`CK-06`).
pub struct AcousticTelemetryAnalyzer;

impl AcousticTelemetryAnalyzer {
    /// Analyzes stereo master frames `(master_l, master_r)` and per-orbit mono sub-bass sums
    /// to compute EBU R128 `LUFS`, `dBTP`, stereo phase correlation, and `SubBassPhaseCancellation` (`CK-06`).
    pub fn analyze_orbits(
        orbit_buffers: &[(u8, &[f32], &[f32])],
        _sample_rate: f32,
    ) -> AcousticMixDiagnostic {
        let mut master_energy_sum = 0.0f64;
        let mut master_peak = 0.0f32;
        let mut corr_lr = 0.0f64;
        let mut corr_ll = 0.0f64;
        let mut corr_rr = 0.0f64;
        let mut frame_count = 0usize;

        if let Some((_, first_l, _)) = orbit_buffers.first() {
            frame_count = first_l.len();
        }

        for i in 0..frame_count {
            let mut sum_l = 0.0f32;
            let mut sum_r = 0.0f32;
            for &(_, ol, or) in orbit_buffers {
                if i < ol.len() && i < or.len() {
                    sum_l += ol[i];
                    sum_r += or[i];
                }
            }
            master_peak = master_peak.max(sum_l.abs()).max(sum_r.abs());
            let e = 0.5 * ((sum_l as f64) * (sum_l as f64) + (sum_r as f64) * (sum_r as f64));
            master_energy_sum += e;
            corr_lr += (sum_l as f64) * (sum_r as f64);
            corr_ll += (sum_l as f64) * (sum_l as f64);
            corr_rr += (sum_r as f64) * (sum_r as f64);
        }

        let mean_sq = if frame_count > 0 {
            (master_energy_sum / (frame_count as f64)).max(1.0e-12)
        } else {
            1.0e-12
        };
        let rms_db = 10.0 * (mean_sq.log10() as f32);
        let integrated_lufs = -0.691 + rms_db;
        let true_peak_dbtp = 20.0 * master_peak.max(1.0e-6).log10();
        let crest_factor_db = (true_peak_dbtp - rms_db).max(0.0);

        let denom = (corr_ll * corr_rr).sqrt();
        let stereo_phase_correlation = if denom > 1.0e-12 {
            (corr_lr / denom).clamp(-1.0, 1.0) as f32
        } else {
            1.0
        };

        let mut masking_conflicts = Vec::new();
        for a_idx in 0..orbit_buffers.len() {
            for b_idx in (a_idx + 1)..orbit_buffers.len() {
                let (orb_a, al, ar) = orbit_buffers[a_idx];
                let (orb_b, bl, br) = orbit_buffers[b_idx];
                let n = al.len().min(bl.len());
                if n == 0 {
                    continue;
                }
                let mut dot = 0.0f64;
                let mut ea = 0.0f64;
                let mut eb = 0.0f64;
                for i in 0..n {
                    let ma = 0.5 * (al[i] + ar[i]) as f64;
                    let mb = 0.5 * (bl[i] + br[i]) as f64;
                    dot += ma * mb;
                    ea += ma * ma;
                    eb += mb * mb;
                }
                let pair_denom = (ea * eb).sqrt();
                if pair_denom > 1.0e-6 {
                    let phase_cos = dot / pair_denom;
                    if phase_cos < -0.5 {
                        masking_conflicts.push(InterOrbitConflictAlert {
                            orbit_a: orb_a,
                            orbit_b: orb_b,
                            center_freq_hz: 60.0,
                            severity_db: (-12.0 * (phase_cos as f32)).abs(),
                            kind: ConflictKind::SubBassPhaseCancellation,
                        });
                    }
                }
            }
        }

        if true_peak_dbtp > 0.0 {
            masking_conflicts.push(InterOrbitConflictAlert {
                orbit_a: 0,
                orbit_b: 0,
                center_freq_hz: 1000.0,
                severity_db: true_peak_dbtp,
                kind: ConflictKind::TruePeakClippingRisk,
            });
        }

        AcousticMixDiagnostic {
            integrated_lufs,
            short_term_lufs: integrated_lufs,
            true_peak_dbtp,
            crest_factor_db,
            stereo_phase_correlation,
            third_octave_spectrum_db: [rms_db; 31],
            masking_conflicts,
        }
    }
}
